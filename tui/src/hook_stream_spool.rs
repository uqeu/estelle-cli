//! The stream hook's on-disk spool: events the server could not take are KEPT, and replayed later.
//!
//! WHY THIS EXISTS (measured 2026-10-26 on 0.4.2, three probe sessions, API pointed at a closed port):
//! the runner's rule — *the offset moves only after the server took the events* — makes the TRANSCRIPT the
//! spool. That recovers a session only if the same session fires another hook while its transcript still
//! exists. Two ordinary cases broke it, and both landed ZERO rows on production:
//!
//! - the outage covered a session's LAST hook (its final Stop / SessionEnd): nothing ever runs that
//!   session again, so its unsent tail waits forever (three real Codex sessions on the founder's machine
//!   had sat stuck since 2026-10-23, one with 264 MB behind its offset);
//! - the transcript was deleted before the next hook: the only copy of the unsent events was gone.
//!
//! [`SpoolingSink`] wraps the live sink. A batch refused for a reason that is about the SERVER, not the
//! batch — no answer, a 5xx, 401/403 (a key being fixed), 408, 429 — is written here and reported as
//! landed, so the offset moves on. [`drain`] runs at the start of EVERY stream hook, for any session, and
//! replays the oldest files first through the same [`crate::hook_stream_run::send`] the live path uses.
//!
//! ⛔ NOTHING IS SILENTLY DROPPED. A spool that is full returns the ORIGINAL error, which leaves the offset
//! where it was — exactly the pre-spool behaviour, never worse. A file the server keeps REFUSING (it
//! answered, and said no) [`MAX_REFUSALS`] times is moved to `dead/`, kept on disk and still counted
//! against the byte bound, so one poison batch cannot block every later one.
//!
//! ⚠️ LIMITS, STATED:
//! - A drain needs a hook to fire. Events spooled by a session's last hook replay on the next hook of ANY
//!   session on this machine — not before.
//! - Two concurrent hooks may replay the same file; the server dedupes by event id, so the cost is a
//!   duplicate request, never a duplicate row.
//! - Order is FIFO by file across sessions; a refused file is skipped (not retried in the same drain), so a
//!   later batch of the same session can land before it. Every event carries its own `at`.
//! - The spool holds what the transcript held, redacted the same way ([`crate::hook_stream::redact`] runs
//!   before an event exists). Files are written `0600` on unix.

use crate::hook_stream_run::{SinkError, StreamSink, send};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Bytes the spool (live + dead files) may hold. Past it, a failed batch is NOT spooled: the offset stays.
pub(crate) const MAX_SPOOL_BYTES: u64 = 64 * 1024 * 1024;
/// Files the spool may hold, for the same reason; also the bound on one directory listing.
pub(crate) const MAX_SPOOL_FILES: usize = 4_096;
/// Spooled files one drain replays; the rest wait for the next hook.
pub(crate) const MAX_DRAIN_FILES: usize = 64;
/// Refusals (the server ANSWERED no) before a file is moved to `dead/` instead of retried.
pub(crate) const MAX_REFUSALS: u32 = 24;
const DEAD_DIR: &str = "dead";

/// One spooled batch, as written to disk.
#[derive(Debug, Serialize, Deserialize)]
struct Spooled {
    session_id: String,
    events: Vec<Value>,
    #[serde(default)]
    refusals: u32,
}

/// What one drain did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DrainReport {
    pub(crate) replayed: usize,
    pub(crate) still_owed: usize,
    pub(crate) dead: usize,
}

/// Whether a failed POST is about the server (spool it) rather than about the batch (let the runner see it).
/// A 400 must pass through: the runner drops exactly the event a `400 event N:` names.
pub(crate) fn retryable(error: &SinkError) -> bool {
    match error {
        SinkError::Unavailable(_) => true,
        SinkError::Refused { status, .. } => {
            *status >= 500 || matches!(*status, 401 | 403 | 408 | 429)
        }
    }
}

/// Whether a failure says the server (or the credential) is unavailable right now, rather than that it
/// looked at this batch and refused it. A drain STOPS on this and charges no file for it; any other refusal
/// (a 500, a 4xx) is charged to the file that drew it, toward [`MAX_REFUSALS`].
pub(crate) fn server_away(error: &SinkError) -> bool {
    match error {
        SinkError::Unavailable(_) => true,
        SinkError::Refused { status, .. } => {
            matches!(*status, 401 | 403 | 408 | 429 | 502 | 503 | 504)
        }
    }
}

/// The live sink, with a spool behind it.
pub(crate) struct SpoolingSink<'a, S> {
    inner: &'a S,
    dir: PathBuf,
    spooled: AtomicUsize,
    seq: AtomicU64,
}

impl<'a, S: StreamSink> SpoolingSink<'a, S> {
    pub(crate) fn new(inner: &'a S, dir: &Path) -> Self {
        Self {
            inner,
            dir: dir.to_path_buf(),
            spooled: AtomicUsize::new(0),
            seq: AtomicU64::new(0),
        }
    }
    /// Batches this sink wrote to disk instead of the server. Test-only: the hook is silent by design and
    /// the spool directory itself is the record of what is owed.
    #[cfg(test)]
    pub(crate) fn spooled(&self) -> usize {
        self.spooled.load(Ordering::Relaxed)
    }
}

impl<S: StreamSink> StreamSink for SpoolingSink<'_, S> {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        let error = match self.inner.post(session_id, events).await {
            Ok(()) => return Ok(()),
            Err(error) if !retryable(&error) => return Err(error),
            Err(error) => error,
        };
        let record = Spooled {
            session_id: session_id.to_string(),
            events: events.to_vec(),
            refusals: 0,
        };
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        if write_new(&self.dir, &record, seq) {
            self.spooled.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        Err(error)
    }
}

/// Spool files in `dir` (oldest first by name), at most [`MAX_SPOOL_FILES`] + 1 listed.
fn listing(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .take(MAX_SPOOL_FILES + 1)
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

/// Bytes and files held, live and dead together — the bound covers both.
fn usage(dir: &Path) -> (u64, usize) {
    let mut files = listing(dir);
    files.extend(listing(&dir.join(DEAD_DIR)));
    let bytes = files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    (bytes, files.len())
}

/// Write `record` as a new spool file if the bounds allow it. `false` = not spooled (full, or disk error).
fn write_new(dir: &Path, record: &Spooled, seq: u64) -> bool {
    let Ok(bytes) = serde_json::to_vec(record) else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let (held, count) = usage(dir);
    if count >= MAX_SPOOL_FILES || held + bytes.len() as u64 > MAX_SPOOL_BYTES {
        return false;
    }
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).max(0);
    let name = format!("{nanos:020}-{}-{seq:06}.json", std::process::id());
    write_private(&dir.join(name), &bytes)
}

/// Write-then-rename, `0600` on unix, so a concurrent drain never reads a torn file.
fn write_private(path: &Path, bytes: &[u8]) -> bool {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

/// Replay up to [`MAX_DRAIN_FILES`] spooled batches, oldest first, through `sink` (the UNWRAPPED live sink).
/// Stops at the first failure that says the server is still away; a refusal counts against that file only.
pub(crate) async fn drain<S: StreamSink>(dir: &Path, sink: &S) -> DrainReport {
    let mut report = DrainReport::default();
    let files = listing(dir);
    let total = files.len();
    for path in files.into_iter().take(MAX_DRAIN_FILES) {
        let Some(mut record) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Spooled>(&b).ok())
        else {
            report.dead += usize::from(bury(dir, &path));
            continue;
        };
        match send_typed(sink, &record).await {
            Ok(()) => {
                let _ = std::fs::remove_file(&path);
                report.replayed += 1;
            }
            Err(away) if away => break,
            Err(_) => {
                record.refusals += 1;
                if record.refusals >= MAX_REFUSALS {
                    report.dead += usize::from(bury(dir, &path));
                } else if let Ok(bytes) = serde_json::to_vec(&record) {
                    write_private(&path, &bytes);
                }
            }
        }
    }
    report.still_owed = listing(dir).len();
    assert!(
        report.replayed + report.dead <= total,
        "a drain cannot account for more files than it listed"
    );
    report
}

/// Send one spooled record. `Err(true)` = the server is still away (stop draining); `Err(false)` = it
/// answered and refused this record.
async fn send_typed<S: StreamSink>(sink: &S, record: &Spooled) -> Result<(), bool> {
    assert!(
        !record.session_id.is_empty(),
        "a spooled batch always names its timeline"
    );
    let probe = Probe {
        sink,
        away: AtomicUsize::new(0),
    };
    match send(&probe, &record.session_id, record.events.clone()).await {
        Ok(_) => Ok(()),
        Err(_) => Err(probe.away.load(Ordering::Relaxed) > 0),
    }
}

/// Passes through to `sink`, remembering whether a failure was the server being away.
struct Probe<'a, S> {
    sink: &'a S,
    away: AtomicUsize,
}

impl<S: StreamSink> StreamSink for Probe<'_, S> {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        let result = self.sink.post(session_id, events).await;
        if let Err(error) = &result
            && server_away(error)
        {
            self.away.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}

/// Move a file to `dead/` (kept, never retried). `true` when it moved.
fn bury(dir: &Path, path: &Path) -> bool {
    let dead = dir.join(DEAD_DIR);
    let Some(name) = path.file_name() else {
        return false;
    };
    std::fs::create_dir_all(&dead).is_ok() && std::fs::rename(path, dead.join(name)).is_ok()
}

#[cfg(test)]
#[path = "hook_stream_spool_tests.rs"]
mod tests;
