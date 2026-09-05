//! The SessionEnd handoff: a bounded, local, durable note that a checkpoint is OWED.
//!
//! # Why this module exists
//!
//! Codex clamps every `SessionEnd` hook to **three seconds** and no plugin can ask for more.
//! The clamp is deliberate and it is upstream's, not fork drift — `SESSION_END_MAX_TIMEOUT_SEC`
//! is `3` in `hooks/src/events/session_end.rs`, and `normalize_command_hook`
//! (`hooks/src/engine/discovery.rs:605`) applies it to `SessionEnd` **and to no other event**;
//! every other event gets `timeout_sec.unwrap_or(600)`. A hook that declares more is rewritten
//! with the warning the founder saw:
//!
//! ```text
//! clamping SessionEnd hook timeout to 3s in ~/.codex/plugins/cache/fatelabs/estelle/…/hooks.json
//! ```
//!
//! The full checkpoint is a NETWORK POST of the conversation and its measured round trip is
//! 4.5–15s, so on `SessionEnd` it was being killed mid-flight — every time, silently.
//!
//! # What this module does instead
//!
//! `SessionEnd` stops doing the network write and does a **local** one: the session id, the
//! transcript path the host already handed us, the cwd and the timestamp. That is a few hundred
//! bytes and one `rename(2)`. The NEXT session's `SessionStart` (`welcome`) — which gets the
//! full budget — claims the note and performs the upload that `SessionEnd` could not.
//!
//! ⚠️ **`Stop` and `PreCompact` are untouched.** They get the full budget and keep doing the
//! full durable write. Only the event that cannot afford the network changed.
//!
//! # At-most-once, on purpose
//!
//! [`claim`] **removes the note before the upload is attempted**, so a note is uploaded at most
//! once and never twice. That is the deliberate direction to be wrong in: this repo has a
//! measured defect in which a double run bills the customer twice with no per-execution
//! idempotency anywhere to stop it. Losing one deferred checkpoint costs a session summary the
//! server can rebuild from the next `Stop`; double-writing one costs money and corrupts a count.
//! The trade is stated here rather than left for a reader to infer.
//!
//! # Bounded, on purpose
//!
//! A note nobody ever claims must not accumulate forever, so the store is bounded three ways —
//! [`MAX_PENDING`] entries, [`MAX_AGE_SECONDS`] of age, and [`MAX_STATE_BYTES`] on disk — and
//! every one of those is a named constant rather than a literal in a body.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

/// The event whose three-second ceiling this whole module exists to fit inside.
pub const DEFERRED_EVENT: &str = "SessionEnd";

/// How many deferred checkpoints the store may hold. Reached only by a customer who ends many
/// sessions without ever starting another one in a repo Estelle is installed in.
pub const MAX_PENDING: usize = 16;

/// How long an unclaimed note stays useful. Past this it is dropped on the next touch of the
/// store, so a note nobody ever consumes cannot accumulate forever.
pub const MAX_AGE_SECONDS: i64 = 14 * 24 * 60 * 60;

/// How many notes one `SessionStart` may drain. Bounds the work a returning session inherits:
/// without it, sixteen stale notes would each cost a 4.5–15s upload before the first prompt.
pub const MAX_CLAIM_PER_START: usize = 4;

/// Refuse to parse a store larger than this rather than read an unbounded file into memory.
const MAX_STATE_BYTES: u64 = 256 * 1024;

/// The longest any single recorded string may be. A path longer than this is not a path.
const MAX_FIELD_CHARS: usize = 4_096;

/// Guards the read-modify-write of the store against two tasks in ONE process. Cross-process
/// exclusion for the consuming side is the `rename(2)` in [`claim`], not this.
static STORE_LOCK: Mutex<()> = Mutex::new(());

/// One checkpoint that was owed at `SessionEnd` and could not be paid inside three seconds.
///
/// NOTE WHAT IS ABSENT: the conversation itself. This is a POINTER to the transcript the host
/// already wrote, never a copy of it — copying it is the expensive thing we are avoiding, and a
/// second copy of a transcript on disk is a second thing to leak.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingCheckpoint {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    /// The event that deferred this note. Recorded rather than assumed so the upload can say
    /// WHY it fired; a resume that cannot tell a `SessionEnd` from a `Stop` cannot rank them.
    pub event: String,
    /// RFC3339, the moment the session ended — NOT the moment the upload happens. The server
    /// is told when the session stopped, which is the fact it needs.
    pub at: String,
}

/// Keyed by session id: one session owes at most one deferred checkpoint, so a `SessionEnd`
/// that somehow fires twice overwrites rather than accumulates.
type HandoffStore = BTreeMap<String, PendingCheckpoint>;

/// Why a handoff could not be written. Every variant names the missing thing, because this is
/// the verb whose silence would cost a customer their session memory without telling them.
#[derive(Debug, Eq, PartialEq)]
pub enum HandoffError {
    NoHome,
    MissingField(&'static str),
    FieldTooLong(&'static str),
    Unwritable(String),
}

impl std::fmt::Display for HandoffError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHome => write!(
                formatter,
                "this machine has no home directory, so there is nowhere to write the handoff"
            ),
            Self::MissingField(field) => write!(
                formatter,
                "the host's SessionEnd payload carried no {field}, so there is nothing to resume from"
            ),
            Self::FieldTooLong(field) => write!(
                formatter,
                "the host's SessionEnd payload carried a {field} longer than {MAX_FIELD_CHARS} characters"
            ),
            Self::Unwritable(detail) => write!(
                formatter,
                "the handoff file could not be written: {detail}"
            ),
        }
    }
}

/// The store's location. Beside `last-session.json`, which is the same kind of fact.
pub fn state_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("pending-checkpoints.json"))
}

/// Validate the four fields a deferred checkpoint needs, or name the one that is missing.
///
/// Separated from the write so the refusal is decided WITHOUT touching the disk — the caller
/// gets the same named reason on a machine with no home directory as on a read-only one.
fn validated(pending: PendingCheckpoint) -> Result<PendingCheckpoint, HandoffError> {
    let fields = [
        ("session_id", &pending.session_id),
        ("transcript_path", &pending.transcript_path),
        ("cwd", &pending.cwd),
        ("hook_event_name", &pending.event),
    ];
    for (name, value) in fields {
        if value.trim().is_empty() {
            return Err(HandoffError::MissingField(name));
        }
        if value.chars().count() > MAX_FIELD_CHARS {
            return Err(HandoffError::FieldTooLong(name));
        }
    }
    Ok(pending)
}

/// Record that a checkpoint is owed. **Local, bounded, and no network** — this is the whole
/// point: a `rename(2)` fits in three seconds and a POST does not.
pub async fn record(
    pending: PendingCheckpoint,
    state_path: PathBuf,
    now: DateTime<Utc>,
) -> Result<(), HandoffError> {
    let mut pending = validated(pending)?;
    pending.at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let key = pending.session_id.trim().to_string();
    tokio::task::spawn_blocking(move || {
        let _guard = STORE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut store = read_store(&state_path).unwrap_or_default();
        store.insert(key.clone(), pending);
        prune(&mut store, now, &key);
        write_store(&state_path, &store)
            .map_err(|error| HandoffError::Unwritable(error.to_string()))
    })
    .await
    .unwrap_or_else(|error| Err(HandoffError::Unwritable(error.to_string())))
}

/// Take up to [`MAX_CLAIM_PER_START`] owed checkpoints, **removing them before the caller
/// uploads anything**.
///
/// The claim is a `rename(2)` of the whole store, which is atomic: a second `welcome` racing
/// this one gets `NotFound` and claims nothing, so two returning sessions cannot upload the
/// same note. Entries beyond the cap, and entries this call does not take, are written back.
///
/// Returns oldest-first: the checkpoint that has been owed longest is paid first.
pub async fn claim(state_path: PathBuf, now: DateTime<Utc>) -> Vec<PendingCheckpoint> {
    tokio::task::spawn_blocking(move || {
        let _guard = STORE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        claim_blocking(&state_path, now)
    })
    .await
    .unwrap_or_default()
}

fn claim_blocking(state_path: &Path, now: DateTime<Utc>) -> Vec<PendingCheckpoint> {
    let claim_path = state_path.with_extension(format!("claim-{}", std::process::id()));
    if fs::rename(state_path, &claim_path).is_err() {
        return Vec::new(); // nothing owed, or another process claimed it first
    }
    let store = read_store(&claim_path).unwrap_or_default();
    let _ = fs::remove_file(&claim_path);

    let mut fresh = store
        .into_values()
        .filter(|entry| !expired(entry, now))
        .collect::<Vec<_>>();
    fresh.sort_by(|left, right| left.at.cmp(&right.at));
    let leftovers = fresh.split_off(fresh.len().min(MAX_CLAIM_PER_START));
    if !leftovers.is_empty() {
        // Put back what this session will not drain, merged with anything a concurrent
        // SessionEnd recorded while we held the claim. Failure here loses the leftovers and
        // nothing else — the claimed entries are already in hand.
        let mut store = read_store(state_path).unwrap_or_default();
        for entry in leftovers {
            store.insert(entry.session_id.clone(), entry);
        }
        let newest = store
            .keys()
            .next_back()
            .cloned()
            .unwrap_or_default();
        prune(&mut store, now, &newest);
        let _ = write_store(state_path, &store);
    }
    fresh
}

fn expired(entry: &PendingCheckpoint, now: DateTime<Utc>) -> bool {
    let Ok(at) = DateTime::parse_from_rfc3339(&entry.at) else {
        return true; // an unparseable timestamp is not a note we can reason about
    };
    now.signed_duration_since(at.with_timezone(&Utc))
        .num_seconds()
        > MAX_AGE_SECONDS
}

/// Drop expired notes, then the oldest, until the store is inside [`MAX_PENDING`]. `keep` is
/// never dropped: the entry this write exists to add outranks anything already there.
fn prune(store: &mut HandoffStore, now: DateTime<Utc>, keep: &str) {
    store.retain(|key, entry| key == keep || !expired(entry, now));
    if store.len() <= MAX_PENDING {
        return;
    }
    let mut oldest = store
        .iter()
        .filter(|(key, _)| key.as_str() != keep)
        .map(|(key, entry)| (entry.at.clone(), key.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, key) in oldest
        .into_iter()
        .take(store.len().saturating_sub(MAX_PENDING))
    {
        store.remove(&key);
    }
}

fn read_store(path: &Path) -> io::Result<HandoffStore> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_STATE_BYTES => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the pending-checkpoint store exceeds its local size bound",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(HandoffStore::new()),
        Err(error) => return Err(error),
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Write the store atomically and privately: a half-written handoff is a handoff that reads as
/// corrupt, and a world-readable one is a list of the customer's repositories.
fn write_store(path: &Path, store: &HandoffStore) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the pending-checkpoint store has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".pending-checkpoints-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut temporary, store).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "session_handoff_tests.rs"]
mod tests;
