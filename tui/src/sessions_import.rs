//! `estelle sessions import` — put the sessions that ran BEFORE the stream hook on the session timeline.
//!
//! The live hook (`estelle hook stream`, 0.4.0) mirrors a session only from the moment it is installed, so
//! every session that ran before it is absent from `GET /session/timeline` — never deleted, never sent. This
//! command reads the transcripts the hosts already keep on this machine and sends them through the SAME
//! runner the hook uses ([`crate::hook_stream_run::run`]):
//!
//! - the same timeline id (`claude-code-<session>`, `codex-<session>`), the same event ids (entry uuid /
//!   rollout byte offset) and the entries' OWN timestamps, so a later live run of the same session is a
//!   no-op on the server, which dedupes by event id;
//! - the same mapping, so the same redaction ([`crate::hook_stream::redact`]) runs on every text field before
//!   anything leaves the machine — there is no second, weaker path;
//! - the same state files (`~/.estelle/stream/<session>.stream.json`), so an import is resumable, a second
//!   import sends nothing, and the live hook continues from where the import stopped.
//!
//! ⛔ NOTHING IS DELETED, LOCALLY OR REMOTELY. This module only reads transcripts and only appends events.
//!
//! ⚠️ LIMITS, STATED:
//! - It can only import what the host kept. Claude Code prunes old transcripts (`cleanupPeriodDays`); a
//!   session whose file is gone cannot be recovered from this machine.
//! - No `ended` lifecycle is invented for an old session: the lifecycle comes only from what the host
//!   recorded, so an imported session usually ends on its last turn's "waiting for your input".
//! - Subagents follow the hook's own rule: one untouched for more than a day is not mirrored. Codex
//!   subagent threads are skipped, as the live hook does not mirror them either.
//! - A 2025 Codex rollout records no per-line timestamp; its lines are skipped, never stamped with the clock.

use crate::hook_stream_run::{
    Dialect, SinkError, StreamInput, StreamSink, first_entry_time, last_entry_time, run,
    safe_session, streamed_offset,
};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use estelle_client::{Repo, RepoResolver};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Sessions imported (sent) per invocation; the rest wait for the next run of the same command.
pub(crate) const MAX_IMPORT_SESSIONS: usize = 1_000;
/// Events sent for one session per invocation (it may overshoot by one hook run's bound); the rest resume.
pub(crate) const MAX_IMPORT_EVENTS_PER_SESSION: usize = 25_000;
/// Hook runs per session per invocation (each bounded by the hook's own per-run limits).
pub(crate) const MAX_RUNS_PER_SESSION: usize = 1_000;
/// Transcript files listed across both hosts before the listing stops (and says so).
pub(crate) const MAX_TRANSCRIPTS: usize = 20_000;
/// Entries read per directory while listing.
const MAX_DIR_ENTRIES: usize = 5_000;
/// Sessions that failed in a row before the import stops instead of failing every remaining one.
const MAX_CONSECUTIVE_FAILURES: usize = 3;
/// Bytes read from a transcript's start to learn its working directory and (Codex) whether it is a subagent.
const HEAD_BYTES: u64 = 256_000;
/// Sessions named one per line in the summary; the rest are counted.
const MAX_LISTED: usize = 50;

/// Which host's transcripts to read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub(crate) enum Source {
    ClaudeCode,
    Codex,
    All,
}

impl Source {
    fn includes(self, dialect: Dialect) -> bool {
        matches!(
            (self, dialect),
            (Self::All, _) | (Self::ClaudeCode, Dialect::Claude) | (Self::Codex, Dialect::Codex)
        )
    }
}

/// Where each host keeps its transcripts.
pub(crate) struct Roots {
    /// `~/.claude/projects` — `<project>/<session>.jsonl`.
    pub(crate) claude_projects: PathBuf,
    /// `$CODEX_HOME/sessions` — `YYYY/MM/DD/rollout-<time>-<thread id>.jsonl`.
    pub(crate) codex_sessions: PathBuf,
}

/// One transcript that names a session.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) dialect: Dialect,
    pub(crate) session: String,
    pub(crate) transcript: PathBuf,
}

/// What listing found.
#[derive(Debug, Default)]
pub(crate) struct Discovery {
    pub(crate) candidates: Vec<Candidate>,
    /// `.jsonl` files whose name is not a session id the hook would accept.
    pub(crate) unnamed: usize,
    /// True when [`MAX_TRANSCRIPTS`] stopped the listing: "there may be more", never "that is all".
    pub(crate) truncated: bool,
}

/// The per-invocation bounds. Production uses [`Limits::DEFAULT`]; a test passes smaller ones to prove a
/// bound holds without writing a 25,000-event fixture.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) sessions: usize,
    pub(crate) events_per_session: usize,
}

impl Limits {
    pub(crate) const DEFAULT: Self = Self {
        sessions: MAX_IMPORT_SESSIONS,
        events_per_session: MAX_IMPORT_EVENTS_PER_SESSION,
    };
}

/// What one invocation asks for.
pub(crate) struct ImportInput<'a> {
    pub(crate) roots: &'a Roots,
    /// The hook's state directory ([`crate::hook_stream_run::state_dir_under`]).
    pub(crate) state_dir: &'a Path,
    pub(crate) source: Source,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) dry_run: bool,
    pub(crate) now: DateTime<Utc>,
    pub(crate) limits: Limits,
}

/// One session's result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// Every line was sent. `events` = events the server accepted (or, on a dry run, would be sent).
    Imported { events: usize },
    /// A bound stopped it; the next run of the same command continues from the saved offset.
    Partial { events: usize },
    /// The stream state already reaches the end of the transcript: nothing to send.
    AlreadyPresent,
    /// Not attempted, and why.
    Skipped(&'static str),
    /// The transcript was read to its end and none of it maps to a timeline event.
    Empty,
    /// The server refused or could not be reached; the offset stays where it was (nothing is lost).
    Failed(String),
}

/// One row of the summary.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) candidate: Candidate,
    pub(crate) first_at: Option<DateTime<Utc>>,
    pub(crate) outcome: Outcome,
}

/// Everything one invocation did.
#[derive(Debug, Default)]
pub(crate) struct ImportSummary {
    pub(crate) rows: Vec<Row>,
    pub(crate) unnamed: usize,
    pub(crate) listing_truncated: bool,
    /// Candidates never looked at because [`MAX_IMPORT_SESSIONS`] or the failure stop was reached.
    pub(crate) not_attempted: usize,
    pub(crate) stopped: Option<String>,
}

/// A sink that sends nothing and refuses nothing: a dry run maps every event through it.
pub(crate) struct NoSink;

impl StreamSink for NoSink {
    async fn post(&self, _session_id: &str, _events: &[Value]) -> Result<(), SinkError> {
        Ok(())
    }
}

/// `YYYY-MM-DD` (UTC midnight) or an RFC 3339 instant.
pub(crate) fn parse_since(text: &str) -> Result<DateTime<Utc>, String> {
    let text = text.trim();
    if let Ok(instant) = DateTime::parse_from_rfc3339(text) {
        return Ok(instant.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|midnight| midnight.and_utc())
        .ok_or_else(|| {
            format!("--since {text:?} is not a date (YYYY-MM-DD) or an RFC 3339 instant")
        })
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .take(MAX_DIR_ENTRIES)
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    out.sort();
    out
}

fn stem(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()?.strip_suffix(".jsonl")
}

/// The thread id at the end of `rollout-<time>-<thread id>` — the id the Codex hook payload carries.
fn codex_session(path: &Path) -> Option<String> {
    let name = stem(path)?.strip_prefix("rollout-")?;
    let tail = name.get(name.len().checked_sub(36)?..)?;
    let shaped = tail.len() == 36
        && tail.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    shaped.then(|| tail.to_string())
}

/// Every transcript under `roots` that `source` asks for, bounded by [`MAX_TRANSCRIPTS`].
pub(crate) fn discover(roots: &Roots, source: Source) -> Discovery {
    let mut found = Discovery::default();
    let push = |found: &mut Discovery, dialect, session: Option<String>, path: PathBuf| {
        if found.candidates.len() + found.unnamed >= MAX_TRANSCRIPTS {
            found.truncated = true;
            return;
        }
        match session.filter(|s| safe_session(s)) {
            Some(session) => found.candidates.push(Candidate {
                dialect,
                session,
                transcript: path,
            }),
            None => found.unnamed += 1,
        }
    };
    if source.includes(Dialect::Claude) {
        for project in entries(&roots.claude_projects)
            .into_iter()
            .filter(|p| p.is_dir())
        {
            for file in entries(&project).into_iter().filter(|p| p.is_file()) {
                if let Some(session) = stem(&file).map(str::to_string) {
                    push(&mut found, Dialect::Claude, Some(session), file);
                }
            }
        }
    }
    if source.includes(Dialect::Codex) {
        let days = entries(&roots.codex_sessions)
            .into_iter()
            .flat_map(|year| entries(&year))
            .flat_map(|month| entries(&month));
        for day in days.filter(|d| d.is_dir()) {
            for file in entries(&day).into_iter().filter(|p| stem(p).is_some()) {
                let named = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-"));
                if named {
                    push(&mut found, Dialect::Codex, codex_session(&file), file);
                }
            }
        }
    }
    assert!(
        found.candidates.len() + found.unnamed <= MAX_TRANSCRIPTS,
        "the listing is bounded"
    );
    found
}

/// The transcript's first [`HEAD_BYTES`], parsed line by line.
fn head_entries(path: &Path) -> Vec<Value> {
    let mut buf = Vec::new();
    let read =
        std::fs::File::open(path).and_then(|file| file.take(HEAD_BYTES).read_to_end(&mut buf));
    if read.is_err() {
        return Vec::new();
    }
    buf.split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .collect()
}

/// A Codex rollout written by a SUBAGENT thread: its `session_meta` names an agent path, or its source
/// is the `{subagent: …}` object rather than a host name like `vscode` or `exec`.
fn codex_subagent(head: &[Value]) -> bool {
    head.iter()
        .find(|e| e["type"] == "session_meta")
        .is_some_and(|meta| {
            let payload = &meta["payload"];
            !payload["agent_path"].is_null() || payload["source"].is_object()
        })
}

/// The working directory the session ran in, as the host recorded it.
fn recorded_cwd(head: &[Value], dialect: Dialect) -> Option<String> {
    head.iter().find_map(|entry| {
        let cwd = match dialect {
            Dialect::Claude => entry.get("cwd"),
            Dialect::Codex => (entry["type"] == "session_meta")
                .then(|| entry.pointer("/payload/cwd"))
                .flatten(),
        };
        cwd.and_then(Value::as_str)
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
    })
}

/// The repository the live hook would have named: resolved from the recorded working directory when that
/// directory still exists, else the wire's "unresolved" value — never a name guessed from a dead path.
fn repo_of(head: &[Value], dialect: Dialect) -> Repo {
    recorded_cwd(head, dialect)
        .and_then(|cwd| RepoResolver::new(None, PathBuf::from(cwd)).resolve())
        .unwrap_or_default()
}

/// What to do with one candidate before sending anything.
fn precheck(candidate: &Candidate, head: &[Value], input: &ImportInput<'_>) -> Option<Outcome> {
    if candidate.dialect == Dialect::Codex && codex_subagent(head) {
        return Some(Outcome::Skipped(
            "a Codex subagent thread (the live hook does not mirror these either)",
        ));
    }
    let Some(last) = last_entry_time(&candidate.transcript) else {
        return Some(Outcome::Skipped(
            "no timestamped entry to place it on a day",
        ));
    };
    if input.since.is_some_and(|since| last < since) {
        return Some(Outcome::Skipped("last activity is before --since"));
    }
    let len = std::fs::metadata(&candidate.transcript).map_or(0, |m| m.len());
    (streamed_offset(input.state_dir, &candidate.session) >= len).then_some(Outcome::AlreadyPresent)
}

/// A dry run works on a COPY of the session's state, so it maps exactly what a real import would send
/// and leaves the real offsets untouched.
fn dry_state(real: &Path, scratch: &Path, session: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(scratch)?;
    for suffix in ["stream.json", "offset", "subagents.json"] {
        let from = real.join(format!("{session}.{suffix}"));
        if from.is_file() {
            std::fs::copy(&from, scratch.join(format!("{session}.{suffix}")))?;
        }
    }
    Ok(())
}

/// Send one session through the hook's runner until its transcript is fully streamed or a bound stops it.
async fn import_one<S: StreamSink>(
    candidate: &Candidate,
    repo: &Repo,
    state_dir: &Path,
    input: &ImportInput<'_>,
    sink: &S,
) -> Outcome {
    let now = input.now;
    let len = std::fs::metadata(&candidate.transcript).map_or(0, |m| m.len());
    let stream = StreamInput {
        state_dir,
        transcript: &candidate.transcript,
        session: &candidate.session,
        // No host event: an import reports only what the transcript recorded, never a Stop or SessionEnd.
        hook_event: "",
        repo: repo.as_str(),
        now,
        dialect: candidate.dialect,
        git_root: None,
    };
    let mut events = 0usize;
    for _ in 0..MAX_RUNS_PER_SESSION {
        let before = streamed_offset(state_dir, &candidate.session);
        if before >= len {
            return if events == 0 {
                Outcome::Empty
            } else {
                Outcome::Imported { events }
            };
        }
        if events >= input.limits.events_per_session {
            return Outcome::Partial { events };
        }
        let report = run(&stream, sink).await;
        events += report.sent + report.subagent_events;
        if let Some(error) = report.error {
            return Outcome::Failed(error);
        }
        if streamed_offset(state_dir, &candidate.session) <= before {
            return Outcome::Failed("the stream offset did not advance".to_string());
        }
    }
    Outcome::Partial { events }
}

/// The import: list, check, send, summarise. `sink` is used only when `input.dry_run` is false.
pub(crate) async fn import<S: StreamSink>(input: &ImportInput<'_>, sink: &S) -> ImportSummary {
    let found = discover(input.roots, input.source);
    let mut summary = ImportSummary {
        unnamed: found.unnamed,
        listing_truncated: found.truncated,
        ..ImportSummary::default()
    };
    let mut candidates = found.candidates;
    // Newest first, so a bounded run lands the sessions a person is most likely to look for.
    candidates.sort_by_key(|c| {
        std::cmp::Reverse(
            std::fs::metadata(&c.transcript)
                .and_then(|m| m.modified())
                .ok(),
        )
    });
    let scratch = tempfile::tempdir().ok();
    let (mut sent, mut failures_in_row) = (0usize, 0usize);
    for (index, candidate) in candidates.iter().enumerate() {
        if sent >= input.limits.sessions || failures_in_row >= MAX_CONSECUTIVE_FAILURES {
            summary.not_attempted = candidates.len() - index;
            if failures_in_row >= MAX_CONSECUTIVE_FAILURES {
                summary.stopped = Some(format!(
                    "{MAX_CONSECUTIVE_FAILURES} sessions failed in a row"
                ));
            }
            break;
        }
        let head = head_entries(&candidate.transcript);
        let outcome = match precheck(candidate, &head, input) {
            Some(outcome) => outcome,
            None => {
                sent += 1;
                let repo = repo_of(&head, candidate.dialect);
                match (input.dry_run, scratch.as_ref()) {
                    (false, _) => import_one(candidate, &repo, input.state_dir, input, sink).await,
                    (true, Some(dir)) => {
                        match dry_state(input.state_dir, dir.path(), &candidate.session) {
                            Ok(()) => {
                                import_one(candidate, &repo, dir.path(), input, &NoSink).await
                            }
                            Err(error) => {
                                Outcome::Failed(format!("dry-run state copy failed: {error}"))
                            }
                        }
                    }
                    (true, None) => {
                        Outcome::Failed("no scratch directory for a dry run".to_string())
                    }
                }
            }
        };
        failures_in_row = if matches!(outcome, Outcome::Failed(_)) {
            failures_in_row + 1
        } else {
            0
        };
        summary.rows.push(Row {
            candidate: candidate.clone(),
            first_at: first_entry_time(&candidate.transcript),
            outcome,
        });
    }
    assert!(
        summary.rows.len() + summary.not_attempted == candidates.len(),
        "every listed session is either reported or counted as not attempted"
    );
    summary
}

fn count(summary: &ImportSummary, wanted: impl Fn(&Outcome) -> bool) -> usize {
    summary.rows.iter().filter(|r| wanted(&r.outcome)).count()
}

fn row_line(row: &Row) -> String {
    let day = row.first_at.map_or_else(
        || "undated".to_string(),
        |t| t.to_rfc3339_opts(SecondsFormat::Secs, true),
    );
    let what = match &row.outcome {
        Outcome::Imported { events } => format!("{events} events"),
        Outcome::Partial { events } => format!("{events} events, PARTIAL — run again to continue"),
        Outcome::Failed(error) => format!("FAILED: {error}"),
        _ => String::new(),
    };
    format!(
        "  {}-{}  {day}  {what}",
        row.candidate.dialect.harness(),
        row.candidate.session
    )
}

/// The plain summary the command prints.
pub(crate) fn summary_lines(summary: &ImportSummary, dry_run: bool) -> Vec<String> {
    let verb = if dry_run { "Would import" } else { "Imported" };
    let sent = |o: &Outcome| matches!(o, Outcome::Imported { .. } | Outcome::Partial { .. });
    let events: usize = summary
        .rows
        .iter()
        .map(|r| match r.outcome {
            Outcome::Imported { events } | Outcome::Partial { events } => events,
            _ => 0,
        })
        .sum();
    let mut lines = vec![format!(
        "{verb} {} sessions ({events} events).{}",
        count(summary, sent),
        if dry_run { " Nothing was sent." } else { "" }
    )];
    let listed: Vec<&Row> = summary
        .rows
        .iter()
        .filter(|r| sent(&r.outcome) || matches!(r.outcome, Outcome::Failed(_)))
        .collect();
    lines.extend(listed.iter().take(MAX_LISTED).map(|r| row_line(r)));
    if listed.len() > MAX_LISTED {
        lines.push(format!("  … and {} more", listed.len() - MAX_LISTED));
    }
    lines.push(format!(
        "{} already present on your timeline (nothing re-sent).",
        count(summary, |o| *o == Outcome::AlreadyPresent)
    ));
    let mut reasons: Vec<(&str, usize)> = Vec::new();
    for row in &summary.rows {
        let reason = match &row.outcome {
            Outcome::Skipped(reason) => *reason,
            Outcome::Empty => "nothing in the transcript maps to a timeline event",
            _ => continue,
        };
        match reasons.iter_mut().find(|(r, _)| *r == reason) {
            Some((_, n)) => *n += 1,
            None => reasons.push((reason, 1)),
        }
    }
    if summary.unnamed > 0 {
        reasons.push(("the file name is not a session id", summary.unnamed));
    }
    let skipped: usize = reasons.iter().map(|(_, n)| n).sum();
    lines.push(format!("{skipped} skipped."));
    lines.extend(reasons.iter().map(|(reason, n)| format!("  {n}: {reason}")));
    let failed = count(summary, |o| matches!(o, Outcome::Failed(_)));
    if failed > 0 {
        lines.push(format!(
            "{failed} failed; their offsets did not move, so running the same command again retries them."
        ));
    }
    if let Some(stopped) = &summary.stopped {
        lines.push(format!("Stopped early: {stopped}."));
    }
    if summary.not_attempted > 0 {
        lines.push(format!(
            "{} sessions not attempted this run (bound reached); run the same command again.",
            summary.not_attempted
        ));
    }
    if summary.listing_truncated {
        lines.push(format!(
            "The listing stopped at {MAX_TRANSCRIPTS} transcripts; there may be more on this machine."
        ));
    }
    lines.push(
        "Only transcripts still on this machine can be imported; nothing was deleted, here or on the server."
            .to_string(),
    );
    lines
}

/// The one-line offer `estelle setup` prints when this machine holds sessions the timeline does not have.
/// Cheap by construction: it lists files and compares sizes with saved offsets, reading no transcript.
pub(crate) fn setup_offer(roots: &Roots, state_dir: &Path) -> Option<String> {
    let found = discover(roots, Source::All);
    let unsent = found
        .candidates
        .iter()
        .filter(|c| {
            let len = std::fs::metadata(&c.transcript).map_or(0, |m| m.len());
            len > 0 && streamed_offset(state_dir, &c.session) < len
        })
        .count();
    (unsent > 0).then(|| {
        format!(
            "{unsent} session transcripts on this machine are not fully on your timeline. To add them \
             (opt-in, nothing is deleted): `estelle sessions import --dry-run`, then `estelle sessions import`."
        )
    })
}

#[cfg(test)]
#[path = "sessions_import_tests.rs"]
mod tests;
