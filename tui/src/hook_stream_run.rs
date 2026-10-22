//! `estelle hook stream` — the I/O half: read what the transcript gained, send it, then move the offset.
//!
//! One run = one hook invocation. It reads the parent transcript from the saved byte offset (whole lines
//! only, bounded), maps each entry through [`crate::hook_stream`], adds the lifecycle the HOST just reported
//! (Stop, SessionEnd, a prompt), sends everything to `POST /session/events`, and only then saves the new
//! offset. It then does the same for every subagent transcript beside the session
//! ([`crate::hook_stream_subagents`]).
//!
//! 🔴 THE OFFSET MOVES ONLY AFTER THE SERVER TOOK THE EVENTS. A stop mid-run resends the same lines next
//! time, which the server turns into duplicate receipts — never a gap. The cost of that rule is that ONE
//! event the server can never accept used to block the stream forever; two things now prevent it:
//! [`crate::hook_stream::scrub`] removes the NUL that Postgres refuses, and a `400 event N: …` refusal
//! drops exactly event N and resends the rest ([`send`]), the same isolation the stand-in bridge does.
//!
//! ⚠️ LIMITS, STATED: concurrent runs (parallel PostToolUse hooks) may read the same offset and both send —
//! harmless, the server dedupes — and the later writer of the offset wins, which can only move it BACK to a
//! point already sent (a replay), never past unsent lines. Git-derived events (`workspace_diff`, `commit`,
//! `workspace_tree`) are NOT produced by the hook; the bridge reads them from the working tree and this
//! port does not, so a hook-only install shows the transcript's own edits and no git view.

use crate::hook_stream::{Notes, batches, entry_time, events_of, lifecycle, scrub, stamp, started};
use chrono::{DateTime, Utc};
use estelle_client::{Client, Endpoint};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// Parent-transcript events one run sends; the rest goes on the next run.
pub(crate) const MAX_EVENTS_PER_RUN: usize = 200;
/// Bytes read from the saved offset per run. A line longer than this is skipped, stated, never retried forever.
pub(crate) const MAX_READ_BYTES: u64 = 8_000_000;
/// Bytes read from the transcript's END to learn its last entry time (the clamp for derived events).
const TAIL_SEED_BYTES: u64 = 2_000_000;
/// How far past the read window the reader looks for the end of an over-long line before giving up.
const MAX_SKIP_BYTES: u64 = 256_000_000;
/// Longest host session id accepted (the server's own bound on a session id is 256).
const MAX_SESSION_CHARS: usize = 200;

/// Why a POST did not land.
#[derive(Debug)]
pub(crate) enum SinkError {
    /// The server answered and refused (`status`, its `error.message`).
    Refused { status: u16, message: String },
    /// No answer: transport failure, timeout, cancelled.
    Unavailable(String),
}

/// Where the events go. A trait so the offset rules are tested against a scripted server.
#[allow(async_fn_in_trait)]
pub(crate) trait StreamSink {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError>;
}

/// The shipped sink: `POST /session/events` through the authenticated client.
pub(crate) struct ClientSink {
    client: Client,
    cancel: CancellationToken,
}

impl ClientSink {
    pub(crate) fn new(client: Client, cancel: CancellationToken) -> Self {
        Self { client, cancel }
    }
}

impl StreamSink for ClientSink {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        let body = json!({"session_id": session_id, "events": events});
        match self
            .client
            .post::<Value, Value>(Endpoint::SessionEvents, &body, &self.cancel)
            .await
        {
            Ok(_) => Ok(()),
            Err(estelle_client::Error::Http { status, message }) => Err(SinkError::Refused {
                status: status.as_u16(),
                message,
            }),
            Err(other) => Err(SinkError::Unavailable(other.to_string())),
        }
    }
}

/// Everything one run needs, resolved by the caller.
pub(crate) struct StreamInput<'a> {
    pub(crate) state_dir: &'a Path,
    pub(crate) transcript: &'a Path,
    /// The HOST's session id (validated by [`safe_session`]); the timeline id is `claude-code-<session>`.
    pub(crate) session: &'a str,
    pub(crate) hook_event: &'a str,
    pub(crate) repo: &'a str,
    pub(crate) now: DateTime<Utc>,
}

/// What a run did — returned for tests and kept (as `last_error`) in the state file for diagnosis.
#[derive(Debug, Default)]
pub(crate) struct StreamReport {
    pub(crate) sent: usize,
    pub(crate) dropped: usize,
    pub(crate) subagent_events: usize,
    pub(crate) error: Option<String>,
}

/// A host session id safe to put in a file name and an event id.
pub(crate) fn safe_session(session: &str) -> bool {
    !session.is_empty()
        && session.len() <= MAX_SESSION_CHARS
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ParentState {
    offset: u64,
    started: bool,
    #[serde(default)]
    last_error: Option<String>,
}

fn state_path(input: &StreamInput<'_>) -> PathBuf {
    input
        .state_dir
        .join(format!("{}.stream.json", input.session))
}

/// The saved state; a pre-0.4.0 `<session>.offset` (a bare number) is read as a session already started.
fn load_state(input: &StreamInput<'_>) -> ParentState {
    if let Some(state) = std::fs::read(state_path(input))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        return state;
    }
    let legacy = input.state_dir.join(format!("{}.offset", input.session));
    let offset = std::fs::read_to_string(legacy)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    ParentState {
        offset,
        started: offset > 0,
        last_error: None,
    }
}

/// Write-then-rename, so a concurrent reader never sees a torn file.
pub(crate) fn save_json<T: Serialize>(path: &Path, value: &T) {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Whole lines read from `offset`: each parsed entry (`None` for a line that is not JSON) with its byte length.
pub(crate) struct Lines {
    pub(crate) entries: Vec<(Option<Value>, u64)>,
    /// Bytes past `offset` the caller may move to when it has used every entry (includes a skipped line).
    pub(crate) consumed: u64,
    /// Lines skipped because they were longer than the whole read window.
    pub(crate) skipped: usize,
}

/// Read up to `max_lines` complete lines starting at `offset`, never more than [`MAX_READ_BYTES`].
///
/// A half-written last line is left for the next run. A single line longer than the whole window can never
/// complete inside it, so it is skipped (counted in `skipped`) rather than re-read on every run forever.
pub(crate) fn read_lines(path: &Path, offset: u64, max_lines: usize) -> std::io::Result<Lines> {
    assert!(
        max_lines > 0,
        "a read that may take no line cannot make progress"
    );
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut lines = Lines {
        entries: Vec::new(),
        consumed: 0,
        skipped: 0,
    };
    if offset >= len {
        return Ok(lines);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    file.by_ref().take(MAX_READ_BYTES).read_to_end(&mut buf)?;
    for line in buf.split_inclusive(|b| *b == b'\n') {
        if line.last() != Some(&b'\n') || lines.entries.len() >= max_lines {
            break;
        }
        lines.entries.push((
            serde_json::from_slice(&line[..line.len() - 1]).ok(),
            line.len() as u64,
        ));
        lines.consumed += line.len() as u64;
    }
    if lines.entries.is_empty()
        && buf.len() as u64 == MAX_READ_BYTES
        && let Some(end) = end_of_line(&mut file, offset + MAX_READ_BYTES)?
    {
        (lines.consumed, lines.skipped) = (end - offset, 1);
    }
    assert!(
        lines.consumed <= len - offset,
        "consumed bytes past the end of the file"
    );
    Ok(lines)
}

/// The offset just past the next newline at or after `from`, looking at most [`MAX_SKIP_BYTES`] ahead (then
/// that bound itself). `None` when the file ends first: an unterminated line is still being written.
fn end_of_line(file: &mut std::fs::File, from: u64) -> std::io::Result<Option<u64>> {
    file.seek(SeekFrom::Start(from))?;
    let mut chunk = vec![0u8; 1 << 20];
    let mut at = from;
    while at - from < MAX_SKIP_BYTES {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        if let Some(i) = chunk[..n].iter().position(|b| *b == b'\n') {
            return Ok(Some(at + i as u64 + 1));
        }
        at += n as u64;
    }
    Ok(Some(at))
}

/// The newest entry time in the transcript's last [`TAIL_SEED_BYTES`] — the clamp for every derived event.
pub(crate) fn last_entry_time(path: &Path) -> Option<DateTime<Utc>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL_SEED_BYTES)))
        .ok()?;
    let mut buf = Vec::new();
    file.take(TAIL_SEED_BYTES).read_to_end(&mut buf).ok()?;
    buf.split(|b| *b == b'\n')
        .skip(usize::from(len > TAIL_SEED_BYTES)) // the first piece may be a partial line
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter_map(|entry| entry_time(&entry))
        .max()
}

/// `"event N: …"` — the server's refusal naming the one malformed event of a batch.
fn poisoned_index(message: &str) -> Option<usize> {
    message
        .strip_prefix("event ")?
        .split(':')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Send `events` to `session_id` in bounded batches, scrubbed. `Ok(dropped)` when every batch landed
/// (`dropped` = events the server refused individually and that were therefore left out); `Err` otherwise.
pub(crate) async fn send<S: StreamSink>(
    sink: &S,
    session_id: &str,
    events: Vec<Value>,
) -> Result<usize, String> {
    let mut dropped = 0;
    for batch in batches(events.into_iter().map(scrub).collect()) {
        dropped += send_batch(sink, session_id, batch).await?;
    }
    Ok(dropped)
}

async fn send_batch<S: StreamSink>(
    sink: &S,
    session_id: &str,
    mut pending: Vec<Value>,
) -> Result<usize, String> {
    let bound = pending.len() + 1; // each pass lands the batch, removes one event, or returns
    let mut dropped = 0;
    for _ in 0..bound {
        if pending.is_empty() {
            return Ok(dropped);
        }
        match sink.post(session_id, &pending).await {
            Ok(()) => return Ok(dropped),
            Err(SinkError::Refused {
                status: 400,
                message,
            }) => match poisoned_index(&message) {
                Some(index) if index < pending.len() => {
                    pending.remove(index);
                    dropped += 1;
                }
                _ => return Err(format!("400: {message}")),
            },
            Err(SinkError::Refused { status, message }) => {
                return Err(format!("{status}: {message}"));
            }
            Err(SinkError::Unavailable(message)) => return Err(message),
        }
    }
    unreachable!("the loop bound is one more than the batch length")
}

/// The lifecycle the HOST just reported by firing this hook, stamped at the transcript's last entry.
fn host_lifecycle(
    input: &StreamInput<'_>,
    timeline: &str,
    mark: u64,
    last: Option<DateTime<Utc>>,
    started: bool,
) -> Option<Value> {
    let at = stamp(None, last, input.now);
    let (tag, state, reason) = match input.hook_event {
        // `system/turn_duration` is written AFTER the Stop hooks run, so this run cannot see it yet; the
        // Stop itself is the host saying the turn ended.
        "Stop" => ("stop", "waiting_for_review", "input_required"),
        "SessionEnd" => ("end", "ended", "completed"),
        "UserPromptSubmit" if started => ("prompt", "running", "input_received"),
        _ => return None,
    };
    Some(lifecycle(
        format!("{timeline}:{tag}:{mark}"),
        &at,
        state,
        reason,
    ))
}

/// One hook run: the parent transcript, then its subagents.
pub(crate) async fn run<S: StreamSink>(input: &StreamInput<'_>, sink: &S) -> StreamReport {
    let mut report = StreamReport::default();
    if std::fs::create_dir_all(input.state_dir).is_err() {
        report.error = Some("the stream state directory could not be created".to_string());
        return report;
    }
    let timeline = format!("claude-code-{}", input.session);
    let mut state = load_state(input);
    let mut notes = Notes::default();
    match parent_events(input, &timeline, &state, &mut notes) {
        Err(error) => report.error = Some(error),
        Ok((events, offset, started_now)) => {
            match (events.len(), send(sink, &timeline, events).await) {
                (total, Ok(dropped)) => {
                    (report.sent, report.dropped) = (total - dropped, dropped);
                    state.offset = offset;
                    state.started |= started_now;
                    state.last_error = None;
                }
                (_, Err(error)) => report.error = Some(error),
            }
        }
    }
    if report.error.is_some() {
        state.last_error = report.error.clone();
    }
    save_json(&state_path(input), &state);
    let sub = crate::hook_stream_subagents::sync(input, &timeline, &notes.notified, sink).await;
    report.subagent_events = sub.events;
    report.error = report.error.or(sub.error);
    report
}

/// The parent's new events, the offset they reach, and whether they include the session's `started`.
fn parent_events(
    input: &StreamInput<'_>,
    timeline: &str,
    state: &ParentState,
    notes: &mut Notes,
) -> Result<(Vec<Value>, u64, bool), String> {
    let unreadable = |_| "the transcript is unreadable".to_string();
    let len = std::fs::metadata(input.transcript)
        .map_err(unreadable)?
        .len();
    // A transcript shorter than the saved offset was rewritten: start over (the server dedupes the replay).
    let from = if state.offset > len { 0 } else { state.offset };
    let lines = read_lines(input.transcript, from, usize::MAX).map_err(unreadable)?;
    let mut events = Vec::new();
    let mut offset = from + lines.consumed;
    let mut used = 0u64;
    for (entry, bytes) in &lines.entries {
        if events.len() >= MAX_EVENTS_PER_RUN {
            offset = from + used; // the rest of this read goes on the next run
            break;
        }
        if let Some(entry) = entry {
            events.extend(events_of(entry, notes, false));
        }
        used += bytes;
    }
    let mut started_now = false;
    if !state.started
        && let Some(first_at) = events
            .first()
            .and_then(|e| e["at"].as_str())
            .map(str::to_string)
    {
        let title = notes
            .title
            .clone()
            .unwrap_or_else(|| "Claude Code session".to_string());
        events.insert(
            0,
            started(timeline, &first_at, &title, input.repo, "claude-code"),
        );
        started_now = true;
    }
    let last = last_entry_time(input.transcript);
    if let Some(event) = host_lifecycle(input, timeline, offset, last, state.started || started_now)
    {
        events.push(event);
    }
    assert!(
        offset >= from && offset <= len,
        "the offset moves forward, and never past the file"
    );
    Ok((events, offset, started_now))
}

#[cfg(test)]
#[path = "hook_stream_run_tests.rs"]
mod tests;
