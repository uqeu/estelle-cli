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
//! point already sent (a replay), never past unsent lines.
//!
//! Two dialects share this runner ([`Dialect`]): Claude Code's JSONL transcript ([`crate::hook_stream`])
//! and Codex's rollout ([`crate::hook_stream_codex`]). After the transcript, the run sends the session's
//! git-derived events ([`crate::hook_stream_git`]) when given a repository root.

use crate::hook_stream::{Notes, batches, entry_time, events_of, lifecycle, scrub, stamp, started};
use crate::hook_stream_codex::{codex_events_of, codex_mentions};
use crate::hook_stream_git::{GitInput, claude_mentions};
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
/// Read windows per run on a Codex rollout. Codex streams only from its Stop hook (one run per turn), so a
/// turn that wrote more than one window must still be caught up in that run — up to this many.
const CODEX_WINDOWS_PER_RUN: usize = 4;
/// Events per run on a Codex rollout, for the same reason: one run per turn, not one per tool call.
pub(crate) const MAX_CODEX_EVENTS_PER_RUN: usize = 1_000;
/// Bytes read from the transcript's START to learn the session's first entry time (git's floor).
const HEAD_SEED_BYTES: u64 = 256_000;

/// Which host wrote the transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dialect {
    Claude,
    Codex,
}

impl Dialect {
    /// The harness name, which is also the timeline id prefix (`claude-code-<session>`, `codex-<session>`).
    pub(crate) fn harness(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            Self::Codex => "codex",
        }
    }
    fn untitled(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code session",
            Self::Codex => "Codex session",
        }
    }
    fn window_budget(self) -> (usize, usize) {
        match self {
            Self::Claude => (1, MAX_EVENTS_PER_RUN),
            Self::Codex => (CODEX_WINDOWS_PER_RUN, MAX_CODEX_EVENTS_PER_RUN),
        }
    }
}

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
    pub(crate) dialect: Dialect,
    /// The repository root whose working tree, log and file list are mirrored; `None` = no git events.
    pub(crate) git_root: Option<&'a Path>,
}

/// What a run did — returned for tests and kept (as `last_error`) in the state file for diagnosis.
#[derive(Debug, Default)]
pub(crate) struct StreamReport {
    pub(crate) sent: usize,
    pub(crate) dropped: usize,
    pub(crate) subagent_events: usize,
    pub(crate) git_events: usize,
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
    /// The `state/reason` of the last lifecycle event this session LANDED. One owner for "did the
    /// timeline already say this": see [`collapse_lifecycle`].
    #[serde(default)]
    last_lifecycle: Option<String>,
}

/// Drop a `waiting_for_review/input_required` that repeats the lifecycle before it, counting the last one
/// already landed in an earlier run. ⛔ WHY: one turn end arrives twice — the Stop hook reports it at
/// once, and Claude Code's `system/turn_duration` entry, written AFTER the Stop hook, reports it again on a
/// later run under a different id, so the server (idempotent per event id) keeps both and the dashboard
/// showed "Waiting for your input" twice per turn. The ids cannot be shared: the entry's uuid does not
/// exist when Stop fires. So the dedupe is on what the event SAYS, carried across runs in the state file.
/// Every other lifecycle passes untouched.
/// The one repeat that was MEASURED (founder's timeline, 2026-10-23: a pair per turn from seq 525 on). Other
/// consecutive lifecycles — two prompts in a row, a codex rollout's own transitions — are distinct rows and stay.
const WAITING_KEY: &str = "waiting_for_review/input_required";

pub(crate) fn collapse_lifecycle(
    events: Vec<Value>,
    last: Option<String>,
) -> (Vec<Value>, Option<String>) {
    let mut last = last;
    let mut kept = Vec::with_capacity(events.len());
    for event in events {
        if event["kind"] != "lifecycle" {
            kept.push(event);
            continue;
        }
        let key = format!(
            "{}/{}",
            event["payload"]["state"].as_str().unwrap_or(""),
            event["payload"]["reason"].as_str().unwrap_or("")
        );
        if key == WAITING_KEY && last.as_deref() == Some(WAITING_KEY) {
            continue;
        }
        last = Some(key);
        kept.push(event);
    }
    (kept, last)
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
        last_lifecycle: None,
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

/// The OLDEST entry time in the transcript's first [`HEAD_SEED_BYTES`] — when the session began.
pub(crate) fn first_entry_time(path: &Path) -> Option<DateTime<Utc>> {
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(HEAD_SEED_BYTES)
        .read_to_end(&mut buf)
        .ok()?;
    buf.split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter_map(|entry| entry_time(&entry))
        .min()
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
    let timeline = format!("{}-{}", input.dialect.harness(), input.session);
    let mut state = load_state(input);
    let mut notes = Notes::default();
    let mut mentions = Vec::new();
    match parent_events(input, &timeline, &state, &mut notes, &mut mentions) {
        Err(error) => report.error = Some(error),
        Ok((events, offset, started_now)) => {
            let (events, last_lifecycle) = collapse_lifecycle(events, state.last_lifecycle.clone());
            match (events.len(), send(sink, &timeline, events).await) {
                (total, Ok(dropped)) => {
                    (report.sent, report.dropped) = (total - dropped, dropped);
                    state.offset = offset;
                    state.started |= started_now;
                    state.last_error = None;
                    state.last_lifecycle = last_lifecycle;
                }
                (_, Err(error)) => report.error = Some(error),
            }
        }
    }
    if report.error.is_some() {
        state.last_error = report.error.clone();
    }
    save_json(&state_path(input), &state);
    if report.error.is_none() {
        let (sent, error) = git_pass(input, &timeline, mentions, sink).await;
        report.git_events = sent;
        report.error = error;
    }
    if input.dialect == Dialect::Claude {
        let sub = crate::hook_stream_subagents::sync(input, &timeline, &notes.notified, sink).await;
        report.subagent_events = sub.events;
        report.error = report.error.or(sub.error);
    }
    report
}

/// The git-derived events ([`crate::hook_stream_git`]), sent; their state is saved only once they landed.
async fn git_pass<S: StreamSink>(
    input: &StreamInput<'_>,
    timeline: &str,
    mentions: Vec<String>,
    sink: &S,
) -> (usize, Option<String>) {
    let Some(root) = input.git_root else {
        return (0, None);
    };
    let state_file = input.state_dir.join(format!("{}.git.json", input.session));
    let git = GitInput {
        root,
        state_file: &state_file,
        first_at: first_entry_time(input.transcript),
        last: last_entry_time(input.transcript),
        now: input.now,
        force: matches!(input.hook_event, "Stop" | "SessionEnd"),
        mentions,
    };
    let (events, state) = crate::hook_stream_git::collect(&git).await;
    let total = events.len();
    match send(sink, timeline, events).await {
        Ok(dropped) => {
            save_json(&state_file, &state);
            (total - dropped, None)
        }
        Err(error) => {
            save_json(
                &state_file,
                &crate::hook_stream_git::unsent(&state_file, state),
            );
            (0, Some(error))
        }
    }
}

/// The parent's new events, the offset they reach, and whether they include the session's `started`.
fn parent_events(
    input: &StreamInput<'_>,
    timeline: &str,
    state: &ParentState,
    notes: &mut Notes,
    mentions: &mut Vec<String>,
) -> Result<(Vec<Value>, u64, bool), String> {
    let unreadable = |_| "the transcript is unreadable".to_string();
    let len = std::fs::metadata(input.transcript)
        .map_err(unreadable)?
        .len();
    // A transcript shorter than the saved offset was rewritten: start over (the server dedupes the replay).
    let from = if state.offset > len { 0 } else { state.offset };
    let (windows, max_events) = input.dialect.window_budget();
    let mut events = Vec::new();
    let mut offset = from;
    'windows: for _ in 0..windows {
        let lines = read_lines(input.transcript, offset, usize::MAX).map_err(unreadable)?;
        if lines.consumed == 0 {
            break;
        }
        let start = offset;
        offset += lines.consumed;
        let mut used = 0u64;
        for (entry, bytes) in &lines.entries {
            if events.len() >= max_events {
                offset = start + used; // the rest of this read goes on the next run
                break 'windows;
            }
            if let Some(entry) = entry {
                events.extend(match input.dialect {
                    Dialect::Claude => events_of(entry, notes, false),
                    Dialect::Codex => codex_events_of(entry, start + used, notes),
                });
                mentions.extend(match input.dialect {
                    Dialect::Claude => claude_mentions(entry),
                    Dialect::Codex => codex_mentions(entry),
                });
            }
            used += bytes;
        }
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
            .unwrap_or_else(|| input.dialect.untitled().to_string());
        events.insert(
            0,
            started(
                timeline,
                &first_at,
                &title,
                input.repo,
                input.dialect.harness(),
            ),
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
