//! The `sync` hook's hand-off: coalesce agent writes into ONE `POST /reindex` per repo per window.
//!
//! 🔴 **THE DEFECT THIS REPLACES (measured 2026-10-21).** `sync_hook` POSTed `/reindex` once per
//! `Write|Edit`, synchronously, inside the agent's turn. Five agents writing in one account produced
//! `429 too many concurrent requests for this account` and `error sending request`, because every
//! write took one of the account's in-flight slots and spent the customer's credits on a one-file
//! embed. The endpoint has always accepted a LIST of files; the hook just never sent more than one.
//!
//! ▶ **THE SHAPE.** A hook is a short-lived process, so an in-memory debounce cannot work — each
//! invocation would be the first. State lives in a small spool file per (repo, root) under
//! `~/.estelle/reindex-spool/`:
//!
//! 1. the hook filters the path LOCALLY ([`admit`]) — nothing outside the repo, in a directory the
//!    inventory skips, credential-shaped by name, of a non-indexable type, or gitignored ever
//!    reaches the network or the spool;
//! 2. it adds the relative path to the spool's pending set ([`enqueue`]) under a file lock;
//! 3. if no flusher is alive and the spool is not backing off, it reserves the flusher slot and the
//!    caller spawns ONE detached `estelle hook reindex-flush` — then the hook returns. The agent
//!    never waits on the network;
//! 4. the flusher ([`run_flusher`]) sleeps one [`DEBOUNCE_WINDOW`], drains everything that arrived,
//!    and sends it as one request. It repeats while writes keep arriving, at most
//!    [`MAX_FLUSH_ROUNDS`] rounds, and exits as soon as a round finds nothing pending.
//!
//! ⚠️ **STATED LIMITS.** The index trails a write by up to one window plus the request. Paths left
//! after [`MAX_RETRY_ATTEMPTS`] refusals stay SPOOLED, not dropped: they go out with the next
//! write's flush after the back-off expires, or via `estelle reindex` (which diffs HEAD), or the
//! GitHub push webhook where one is connected. Nothing re-sends them on a timer, on purpose — a
//! timer is exactly the hammering this module exists to stop. Coalescing is per machine: five
//! agents on one laptop share one flusher; five laptops are five flushers.
//!
//! 🔴 **AND PER (repo, root) WAS NOT ENOUGH (2026-10-29).** Two worktrees of one repository are two
//! spools and two flushers, and each posted every [`DEBOUNCE_WINDOW`]. N lane worktrees made N
//! `/reindex` writers against ONE repository's graph, and that load drove a production OOM. So
//! posts are now ALSO spaced per REPOSITORY, across every root on this machine:
//! `<spool dir>/repo-<hash>.last_post_ms` records the last post for the repo. A flusher must
//! [`reserve_repo_post`] under that marker's lock before it drains, and it waits out
//! [`MIN_REPO_POST_INTERVAL`] when another root posted more recently. Each worktree still sends
//! only its own paths. The cost is that the index can trail a write by up to one interval per
//! sibling worktree that is also writing.

use std::collections::BTreeSet;
use std::fs;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

/// How long the flusher collects writes before it sends them. One agent burst (a multi-file edit)
/// lands well inside it; the cost is that the index trails a write by this much.
pub(crate) const DEBOUNCE_WINDOW: Duration = Duration::from_secs(5);
/// Bound on one flusher's lifetime, in windows (5 min at 5 s). It exits EARLIER the first round
/// that finds nothing pending; this caps a session that never stops writing.
pub(crate) const MAX_FLUSH_ROUNDS: u32 = 60;
/// Consecutive retryable refusals (429/503/transport) before the flusher stops and leaves the
/// paths spooled. Each attempt may itself be retried by the client's own `Retry-After` handling
/// (`estelle_client::SHED_MAX_ATTEMPTS`), so the worst case is the product of the two.
pub(crate) const MAX_RETRY_ATTEMPTS: u32 = 3;
/// First back-off after a retryable refusal; doubles per consecutive refusal up to the cap.
pub(crate) const RETRY_BASE: Duration = Duration::from_secs(5);
pub(crate) const BACKOFF_CAP: Duration = Duration::from_secs(120);
/// Bound on the pending set. Past it a path is COUNTED, not stored, and the overflow is reported —
/// a capped spool means "cannot hold more", never "that's all there was".
pub(crate) const MAX_PENDING_PATHS: usize = 500;
/// Bound on one request's path list. The remainder stays pending for the next round.
pub(crate) const MAX_BATCH_PATHS: usize = 200;
/// The minimum spacing between two `/reindex` posts for ONE repository from this machine, whichever
/// worktree sends them. See the module note: per-root debouncing let N worktrees post N times per
/// window.
pub(crate) const MIN_REPO_POST_INTERVAL: Duration = Duration::from_secs(20);
/// Bound on how many times one round waits for the repo slot before it gives the round up. With
/// the interval above this is 10 minutes, enough for many sibling worktrees to take a turn each.
/// On exhaustion the paths stay pending and the next write spawns a successor.
pub(crate) const MAX_REPO_SLOT_WAITS: u32 = 30;
/// A reservation the spawned child has not yet claimed is honoured this long (process start).
const RESERVATION_GRACE_MS: u64 = 15_000;
/// Upper bound on a flusher's claim, whatever its PID says — guards against PID reuse. A single
/// round can legitimately spend 3 × 300 s client deadline + 2 × 30 s shed waits.
const FLUSHER_MAX_AGE_MS: u64 = 20 * 60 * 1000;
/// The spool lock is held for microseconds (no network under it). Bounded acquisition.
const LOCK_ATTEMPTS: u32 = 400;
const LOCK_RETRY: Duration = Duration::from_millis(5);
const LOCK_STALE: Duration = Duration::from_secs(10);

/// Name-shaped credential containers. Deliberately NOT `secret*`/`credential*` source files:
/// `secret_scanner.rs` is code, and its content is still screened by `is_secret_shaped` at flush.
const CREDENTIAL_NAMES: &[&str] = &[
    ".env",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "auth.json",
    "credentials.json",
    "estelle_accounts.json",
    "estelle_experience.json",
    "id_ecdsa",
    "id_ed25519",
    "id_rsa",
    "stress_accounts.json",
];
const CREDENTIAL_EXTENSIONS: &[&str] = &["key", "p12", "pem", "pfx"];

/// Why a path never reached the spool. Every variant is a LOCAL decision; none costs a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Skip {
    Empty,
    OutsideRepo,
    ScratchDirectory,
    CredentialFile,
    NotIndexable,
    GitIgnored,
}

/// Decide locally whether `path` may be reindexed, returning its repo-relative `/` key.
///
/// `skip_dirs` and `is_source` are the INGEST's own rules (`top_level::SKIP_DIRECTORIES`,
/// `top_level::is_source`), passed in rather than re-listed: skipping a directory the ingest indexes
/// would hide a newly-written file from the index, which is a false "unknown symbol" later.
pub(crate) fn admit(
    root: &Path,
    path: &str,
    skip_dirs: &[&str],
    is_source: fn(&Path) -> bool,
) -> Result<String, Skip> {
    if path.trim().is_empty() {
        return Err(Skip::Empty);
    }
    let relative = repo_relative(root, Path::new(path)).ok_or(Skip::OutsideRepo)?;
    assert!(
        !relative.is_absolute(),
        "repo_relative returned an absolute path"
    );
    if relative
        .components()
        .any(|part| skip_dirs.contains(&part.as_os_str().to_string_lossy().as_ref()))
    {
        return Err(Skip::ScratchDirectory);
    }
    if is_credential_file(&relative) {
        return Err(Skip::CredentialFile);
    }
    if !is_source(&relative) {
        return Err(Skip::NotIndexable);
    }
    let key = relative.to_string_lossy().replace('\\', "/");
    if is_git_ignored(root, &key) {
        return Err(Skip::GitIgnored);
    }
    Ok(key)
}

/// `path` relative to `root`, both canonicalised, or `None` when it is outside. A file that no longer
/// exists is resolved through its parent so a delete-then-write still maps to the same key.
fn repo_relative(root: &Path, path: &Path) -> Option<PathBuf> {
    let canonical_root = root.canonicalize().ok()?;
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical = full.canonicalize().ok().or_else(|| {
        let parent = full.parent()?.canonicalize().ok()?;
        Some(parent.join(full.file_name()?))
    })?;
    let relative = canonical.strip_prefix(&canonical_root).ok()?.to_path_buf();
    (!relative.as_os_str().is_empty()).then_some(relative)
}

fn is_credential_file(relative: &Path) -> bool {
    let Some(name) = relative.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if CREDENTIAL_NAMES.contains(&lower.as_str()) || lower.starts_with(".env.") {
        return true;
    }
    if lower.starts_with("id_rsa") || lower.starts_with("id_ed25519") {
        return true;
    }
    relative
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            CREDENTIAL_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// `git check-ignore` exits 0 for an ignored path, 1 for a not-ignored one, 128 on error. Only an
/// explicit 0 skips: a tree that is not a Git worktree is screened again by the flush's inventory.
fn is_git_ignored(root: &Path, key: &str) -> bool {
    ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-q", "--", key])
        .status()
        .is_ok_and(|status| status.code() == Some(0))
}

/// The on-disk spool for one (repo, root). Every field has ONE meaning:
/// `flusher_pid == 0 && flusher_since == 0` is "no flusher"; `flusher_pid == 0 && flusher_since > 0`
/// is "reserved, child not yet started".
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct SpoolState {
    pub repo: String,
    pub root: String,
    pub pending: BTreeSet<String>,
    /// Writes refused by [`MAX_PENDING_PATHS`] since the last successful flush.
    pub overflowed: u64,
    pub backoff_until_ms: u64,
    pub failures: u32,
    pub flusher_pid: u32,
    pub flusher_since_ms: u64,
    /// The last flush failure, surfaced to the agent ONCE by the next write, then cleared.
    pub last_error: Option<String>,
}

/// The spool file for `repo` at `root` inside `dir`. Keyed by both: two worktrees of one repo have
/// different contents and must not send each other's files.
pub(crate) fn spool_path_in(dir: &Path, repo: &str, root: &Path) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(repo.as_bytes());
    digest.update([0]);
    digest.update(root.to_string_lossy().as_bytes());
    let hex = format!("{:x}", digest.finalize());
    dir.join(format!("{}.json", &hex[..24]))
}

/// The per-REPOSITORY post marker beside the spools in `dir`. Keyed by the repo alone, so every
/// worktree of one repository shares it. The `repo-` prefix keeps it apart from spool files, which
/// hash (repo, root).
pub(crate) fn repo_marker_in(dir: &Path, repo: &str) -> PathBuf {
    let hex = format!("{:x}", Sha256::digest(repo.as_bytes()));
    dir.join(format!("repo-{}.last_post_ms", &hex[..24]))
}

fn read_last_post(marker: &Path) -> u64 {
    fs::read_to_string(marker)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn interval_ms() -> u64 {
    u64::try_from(MIN_REPO_POST_INTERVAL.as_millis()).unwrap_or(u64::MAX)
}

/// The marker for the repository this spool belongs to.
fn marker_for(spool: &Path) -> PathBuf {
    let dir = spool.parent().unwrap_or_else(|| Path::new("."));
    repo_marker_in(dir, &load(spool).repo)
}

/// Take the repository's post slot at `now`, or say how long until it is free. `Ok(0)` means this
/// flusher may post and the marker now reads `now`. Checked and written under ONE lock, so two
/// flushers waking together cannot both take it.
pub(crate) fn reserve_repo_post(marker: &Path, now: u64) -> Result<u64, String> {
    assert!(now != 0, "time 0 cannot be a post instant");
    let _lock = SpoolLock::acquire(marker)?;
    let next_free = read_last_post(marker).saturating_add(interval_ms());
    if now < next_free {
        return Ok(next_free - now);
    }
    let temporary = marker.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&temporary, now.to_string()).map_err(|error| error.to_string())?;
    fs::rename(&temporary, marker).map_err(|error| error.to_string())?;
    assert_eq!(
        read_last_post(marker),
        now,
        "the reservation must be readable back"
    );
    Ok(0)
}

/// `~/.estelle/reindex-spool/`.
pub(crate) fn default_spool_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("reindex-spool"))
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

/// An exclusive lock on one spool, released on drop. `create_new` is atomic on every platform we
/// ship; a lock older than [`LOCK_STALE`] belonged to a process that died holding it.
struct SpoolLock {
    path: PathBuf,
}

impl SpoolLock {
    fn acquire(spool: &Path) -> Result<Self, String> {
        let path = spool.with_extension("lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        for _ in 0..LOCK_ATTEMPTS {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&path)
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > LOCK_STALE);
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Err(format!(
            "the reindex spool stayed locked for {LOCK_ATTEMPTS} attempts"
        ))
    }
}

impl Drop for SpoolLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn load(spool: &Path) -> SpoolState {
    fs::read(spool)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Write-then-rename, so a reader never sees half a spool.
fn save(spool: &Path, state: &SpoolState) -> Result<(), String> {
    let body = serde_json::to_vec(state).map_err(|error| error.to_string())?;
    let temporary = spool.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&temporary, body).map_err(|error| error.to_string())?;
    fs::rename(&temporary, spool).map_err(|error| error.to_string())
}

/// Lock, load, mutate, save. The ONLY way state changes, so no path skips the lock.
fn with_state<T>(spool: &Path, edit: impl FnOnce(&mut SpoolState) -> T) -> Result<T, String> {
    let _lock = SpoolLock::acquire(spool)?;
    let mut state = load(spool);
    let result = edit(&mut state);
    save(spool, &state)?;
    Ok(result)
}

fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 performs only the existence/permission check and delivers nothing.
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    // 🔴 THIS USED TO BE `true` ON EVERY NON-UNIX HOST. A crashed flusher on Windows then held the
    // spool for the full FLUSHER_MAX_AGE_MS (20 minutes) with nothing draining it, because the
    // only liveness answer was "assume alive". OpenProcess + GetExitCodeProcess is the Windows
    // equivalent of `kill(pid, 0)`: a handle we may not query (ACCESS_DENIED) is a live process
    // owned by someone else, exactly as EPERM is above.
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::GetExitCodeProcess;
        use windows_sys::Win32::System::Threading::OpenProcess;
        use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

        // SAFETY: OpenProcess takes plain integers and returns a handle or 0; it reads no memory.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle == 0 {
            // SAFETY: reads this thread's last-error slot, set by the failed call just above.
            return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
        }
        let mut code: u32 = 0;
        // SAFETY: `handle` is a live process handle opened above; `code` outlives the call.
        let queried = unsafe { GetExitCodeProcess(handle, &mut code) };
        // SAFETY: `handle` was opened above and is closed exactly once, here.
        unsafe { CloseHandle(handle) };
        queried != 0 && code == STILL_ACTIVE as u32
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

/// Whether the recorded flusher is still to be trusted with the pending set.
fn flusher_alive(state: &SpoolState, now: u64, is_alive: fn(u32) -> bool) -> bool {
    if state.flusher_since_ms == 0 {
        return false;
    }
    let age = now.saturating_sub(state.flusher_since_ms);
    if state.flusher_pid == 0 {
        return age < RESERVATION_GRACE_MS;
    }
    age < FLUSHER_MAX_AGE_MS && is_alive(state.flusher_pid)
}

/// What a hook should do after recording a write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Enqueued {
    /// This invocation reserved the flusher slot and must spawn it (or call [`abandon`]).
    pub spawn: bool,
    pub pending: usize,
    /// A previous flush failure the agent has not yet been told about.
    pub report: Option<String>,
}

/// Record one admitted write. Never touches the network.
pub(crate) fn enqueue(
    spool: &Path,
    repo: &str,
    root: &Path,
    key: &str,
    now: u64,
) -> Result<Enqueued, String> {
    enqueue_with(spool, repo, root, key, now, pid_is_alive)
}

pub(crate) fn enqueue_with(
    spool: &Path,
    repo: &str,
    root: &Path,
    key: &str,
    now: u64,
    is_alive: fn(u32) -> bool,
) -> Result<Enqueued, String> {
    assert!(
        !key.is_empty(),
        "enqueue requires an admitted, non-empty key"
    );
    assert!(
        now != 0,
        "time 0 is the 'no flusher' sentinel and cannot be a write instant"
    );
    with_state(spool, |state| {
        state.repo = repo.to_string();
        state.root = root.to_string_lossy().into_owned();
        if state.pending.len() < MAX_PENDING_PATHS || state.pending.contains(key) {
            state.pending.insert(key.to_string());
        } else {
            state.overflowed += 1;
        }
        let report = state.last_error.take();
        let spawn = !flusher_alive(state, now, is_alive) && now >= state.backoff_until_ms;
        if spawn {
            state.flusher_pid = 0;
            state.flusher_since_ms = now;
        }
        assert!(
            state.pending.len() <= MAX_PENDING_PATHS,
            "pending set exceeded its bound"
        );
        Enqueued {
            spawn,
            pending: state.pending.len(),
            report,
        }
    })
}

/// Give the flusher slot back and record why, so the next write reports it and nothing retries
/// before [`BACKOFF_CAP`]. Used when the child cannot be spawned or cannot authenticate.
pub(crate) fn abandon(spool: &Path, error: &str, now: u64) -> Result<(), String> {
    with_state(spool, |state| {
        release(state);
        state.backoff_until_ms = now.saturating_add(backoff_ms(MAX_RETRY_ATTEMPTS, false));
        state.last_error = Some(format!(
            "{} change(s) are queued and were not sent: {error}",
            state.pending.len()
        ));
    })
}

/// How one batched request ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PostOutcome {
    Accepted,
    /// Every path was screened out locally at flush time (deleted-and-untracked, secret-shaped
    /// content): NO request was made.
    NothingToSend,
    /// 429 / 503 / transport: worth retrying after a back-off.
    Retry(String),
    /// Anything else (401, 402, 400): retrying cannot help; stop and report.
    Failed(String),
    /// The server answered and declined by name (`skipped_off_default`): this checkout is a lane, not
    /// the default branch, so the shared graph was NOT written. The batch is consumed (re-sending it
    /// cannot change the answer) and the local index is NOT stamped current.
    SkippedOffDefault,
}

/// The flusher's effects, injected so the loop is tested with a fake clock and a counting server.
#[allow(async_fn_in_trait)]
pub(crate) trait FlushPort {
    /// Send `paths` (repo-relative keys) as ONE request. `drained_at` is when they were taken off
    /// the spool — the freshness stamp must use it, not the completion time, or a write made
    /// during the request would read as indexed.
    async fn post(&mut self, paths: &[String], drained_at: u64) -> PostOutcome;
    async fn sleep(&mut self, duration: Duration);
    fn now(&self) -> u64;
}

/// What a flusher run did. Returned for tests and for the flusher's own exit line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FlushReport {
    pub requests: u32,
    pub paths_sent: usize,
    pub gave_up: bool,
}

/// Claim the spool for this process. `false` means another live flusher owns it — exit.
pub(crate) fn claim(spool: &Path, pid: u32, now: u64) -> Result<bool, String> {
    claim_with(spool, pid, now, pid_is_alive)
}

pub(crate) fn claim_with(
    spool: &Path,
    pid: u32,
    now: u64,
    is_alive: fn(u32) -> bool,
) -> Result<bool, String> {
    assert!(pid != 0, "a flusher must claim with a real pid");
    assert!(
        now != 0,
        "time 0 is the 'no flusher' sentinel and cannot be a claim instant"
    );
    with_state(spool, |state| {
        let other = state.flusher_pid != 0 && state.flusher_pid != pid;
        if other && flusher_alive(state, now, is_alive) {
            return false;
        }
        state.flusher_pid = pid;
        state.flusher_since_ms = now;
        true
    })
}

/// The flusher loop. Bounded by [`MAX_FLUSH_ROUNDS`]; exits early when a round drains nothing, or
/// after [`MAX_RETRY_ATTEMPTS`] consecutive retryable refusals, or on the first non-retryable one.
pub(crate) async fn run_flusher(spool: &Path, port: &mut impl FlushPort) -> FlushReport {
    let mut report = FlushReport::default();
    for round in 0..MAX_FLUSH_ROUNDS {
        let wait = next_wait(spool, port.now());
        port.sleep(wait).await;
        let last = round + 1 == MAX_FLUSH_ROUNDS;
        if !take_repo_slot(spool, port).await {
            // The paths stay pending; the slot is released so the next write spawns a successor.
            let _ = with_state(spool, release);
            report.gave_up = true;
            return report;
        }
        let drained_at = port.now();
        let Ok(batch) = drain(spool, drained_at, last) else {
            report.gave_up = true;
            return report;
        };
        if batch.is_empty() {
            return report;
        }
        let outcome = port.post(&batch, drained_at).await;
        if outcome != PostOutcome::NothingToSend {
            report.requests += 1;
            report.paths_sent += batch.len();
        }
        let stop = settle(spool, &batch, &outcome, port.now(), last).unwrap_or(true);
        if stop {
            report.gave_up = matches!(outcome, PostOutcome::Retry(_) | PostOutcome::Failed(_));
            return report;
        }
    }
    report
}

/// One window, or longer while a back-off is in force or another worktree of this repository
/// posted less than [`MIN_REPO_POST_INTERVAL`] ago.
fn next_wait(spool: &Path, now: u64) -> Duration {
    let state = load(spool);
    let backoff = state.backoff_until_ms.saturating_sub(now);
    // Spacing only matters when there is something to send: an empty round exits after one window.
    let spacing = if state.pending.is_empty() {
        0
    } else {
        read_last_post(&marker_for(spool))
            .saturating_add(interval_ms())
            .saturating_sub(now)
    };
    DEBOUNCE_WINDOW
        .max(Duration::from_millis(backoff))
        .max(Duration::from_millis(spacing))
}

/// Wait, bounded by [`MAX_REPO_SLOT_WAITS`], until this flusher holds the repository's post slot.
/// A round with nothing pending needs no slot: its drain finds nothing and the flusher exits.
/// `false` means the bound ran out (or the marker could not be locked) and nothing was posted.
async fn take_repo_slot(spool: &Path, port: &mut impl FlushPort) -> bool {
    let marker = marker_for(spool);
    for _ in 0..MAX_REPO_SLOT_WAITS {
        if load(spool).pending.is_empty() {
            return true;
        }
        match reserve_repo_post(&marker, port.now()) {
            Ok(0) => return true,
            Ok(wait) => port.sleep(Duration::from_millis(wait)).await,
            Err(_) => return false,
        }
    }
    false
}

/// Take up to [`MAX_BATCH_PATHS`] pending paths. An empty drain releases the flusher slot under the
/// SAME lock a hook checks it under, so a write can never land between "nothing left" and "exit"
/// without seeing that no flusher is alive. On the last round the slot is released before the
/// request, so a write during it spawns a successor instead of being stranded.
fn drain(spool: &Path, now: u64, last: bool) -> Result<Vec<String>, String> {
    with_state(spool, |state| {
        let batch: Vec<String> = state
            .pending
            .iter()
            .take(MAX_BATCH_PATHS)
            .cloned()
            .collect();
        for key in &batch {
            state.pending.remove(key);
        }
        if batch.is_empty() || last {
            release(state);
        } else {
            state.flusher_since_ms = now;
        }
        assert!(batch.len() <= MAX_BATCH_PATHS, "batch exceeded its bound");
        batch
    })
}

fn release(state: &mut SpoolState) {
    state.flusher_pid = 0;
    state.flusher_since_ms = 0;
}

/// Record a request's outcome. Returns whether the flusher must stop.
fn settle(
    spool: &Path,
    batch: &[String],
    outcome: &PostOutcome,
    now: u64,
    last: bool,
) -> Result<bool, String> {
    with_state(spool, |state| match outcome {
        PostOutcome::Accepted | PostOutcome::NothingToSend | PostOutcome::SkippedOffDefault => {
            state.failures = 0;
            state.overflowed = 0;
            state.backoff_until_ms = 0;
            last
        }
        PostOutcome::Retry(error) | PostOutcome::Failed(error) => {
            // Re-spool, bounded: the batch came out of the set, so it fits back unless new writes
            // filled it meanwhile — those count as overflow, never silently vanish.
            for key in batch {
                if state.pending.len() < MAX_PENDING_PATHS {
                    state.pending.insert(key.clone());
                } else {
                    state.overflowed += 1;
                }
            }
            let retryable = matches!(outcome, PostOutcome::Retry(_));
            state.failures = state.failures.saturating_add(1);
            state.backoff_until_ms = now.saturating_add(backoff_ms(state.failures, retryable));
            let stop = !retryable || state.failures >= MAX_RETRY_ATTEMPTS || last;
            if stop {
                release(state);
                state.last_error = Some(format!(
                    "{} change(s) are queued and will go out with the next write after the \
                     back-off, or with `estelle reindex`: {error}",
                    state.pending.len()
                ));
            }
            stop
        }
    })
}

/// Exponential from [`RETRY_BASE`], capped at [`BACKOFF_CAP`]; a non-retryable refusal waits the cap.
pub(crate) fn backoff_ms(failures: u32, retryable: bool) -> u64 {
    let cap = u64::try_from(BACKOFF_CAP.as_millis()).unwrap_or(u64::MAX);
    if !retryable {
        return cap;
    }
    let base = u64::try_from(RETRY_BASE.as_millis()).unwrap_or(u64::MAX);
    let shift = failures.saturating_sub(1).min(16);
    base.saturating_mul(1 << shift).min(cap)
}

#[cfg(test)]
#[path = "reindex_spool_tests.rs"]
mod tests;
