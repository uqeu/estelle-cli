use std::path::Path;
use std::process::Command as ProcessCommand;
use std::time::Duration;

use pretty_assertions::assert_eq;

use super::*;

const SKIP: &[&str] = &[".git", "node_modules", "target"];
const START: u64 = 1_000_000;

fn rust_or_python(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "rs" | "py"))
}

fn alive(_: u32) -> bool {
    true
}

fn dead(_: u32) -> bool {
    false
}

// The REAL liveness probe, not the `alive`/`dead` doubles every other test injects. Its two answers
// must DIFFER on two processes we control, or the test cannot see the probe at all: before this
// existed, the non-unix arm returned `true` for every PID and no test could tell.
#[test]
fn the_real_liveness_probe_tells_a_running_process_from_an_exited_one() {
    assert!(
        pid_is_alive(std::process::id()),
        "this test process is running"
    );
    let mut child = ProcessCommand::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn git");
    let pid = child.id();
    assert!(child.wait().expect("wait for git").success());
    // Reaped (unix) / exited with a code (windows): either way, no longer a live process. PID reuse
    // inside this window is possible in principle and would make this flaky, never falsely green.
    assert!(
        !pid_is_alive(pid),
        "an exited, reaped child must read as dead"
    );
}

fn git_repo() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("root");
    let status = ProcessCommand::new("git")
        .arg("-C")
        .arg(root.path())
        .args(["init", "-q"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .expect("git init");
    assert!(status.success());
    std::fs::write(root.path().join(".gitignore"), "ignored/\n").expect("gitignore");
    for dir in ["src", "ignored", "node_modules/pkg"] {
        std::fs::create_dir_all(root.path().join(dir)).expect("dir");
    }
    root
}

#[test]
fn the_filter_decides_locally_and_names_why() {
    let root = git_repo();
    let outside = tempfile::tempdir().expect("outside");
    let write = |relative: &str| {
        let path = root.path().join(relative);
        std::fs::write(&path, "x = 1\n").expect("write");
        path.to_string_lossy().into_owned()
    };
    let outside_file = outside.path().join("scratch.py");
    std::fs::write(&outside_file, "x = 1\n").expect("outside write");

    let cases = [
        (write("src/a.py"), Ok("src/a.py".to_string())),
        ("src/b.rs".to_string(), Ok("src/b.rs".to_string())),
        (
            outside_file.to_string_lossy().into_owned(),
            Err(Skip::OutsideRepo),
        ),
        (write("node_modules/pkg/i.py"), Err(Skip::ScratchDirectory)),
        (write(".env"), Err(Skip::CredentialFile)),
        (write("src/deploy.pem"), Err(Skip::CredentialFile)),
        (write("src/notes.txt"), Err(Skip::NotIndexable)),
        (write("ignored/gen.py"), Err(Skip::GitIgnored)),
        ("  ".to_string(), Err(Skip::Empty)),
    ];
    for (path, expected) in cases {
        assert_eq!(
            admit(root.path(), &path, SKIP, rust_or_python),
            expected,
            "{path}"
        );
    }
    // A code file that merely has "secret" in its NAME is code, not a credential container.
    std::fs::write(root.path().join("src/secret_scanner.py"), "x = 1\n").expect("write");
    assert_eq!(
        admit(root.path(), "src/secret_scanner.py", SKIP, rust_or_python),
        Ok("src/secret_scanner.py".to_string())
    );
}

// Unix-only: creating a symlink on Windows needs Developer Mode or an elevated token, so the case
// cannot be built portably. `admit`'s canonicalising containment check is the same code on both.
#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_repo_is_outside_it() {
    let root = git_repo();
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("x.py"), "x = 1\n").expect("write");
    std::os::unix::fs::symlink(outside.path().join("x.py"), root.path().join("src/l.py"))
        .expect("symlink");
    assert_eq!(
        admit(root.path(), "src/l.py", SKIP, rust_or_python),
        Err(Skip::OutsideRepo)
    );
}

fn spool_in(dir: &tempfile::TempDir) -> PathBuf {
    spool_path_in(dir.path(), "owner/name", Path::new("/repo"))
}

#[test]
fn only_the_first_write_reserves_the_flusher() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let root = Path::new("/repo");
    let first = enqueue_with(&spool, "owner/name", root, "a.py", 1_000, alive).expect("first");
    assert!(first.spawn);
    for (index, key) in ["b.py", "c.py", "a.py"].iter().enumerate() {
        let next =
            enqueue_with(&spool, "owner/name", root, key, 1_100 + index as u64, alive).expect("n");
        assert!(
            !next.spawn,
            "{key}: a reserved flusher must not be spawned twice"
        );
    }
    assert_eq!(
        load(&spool).pending.len(),
        3,
        "a repeated path is one entry"
    );
    // A reservation the child never claimed expires, and the next write may try again.
    let later = 1_000 + RESERVATION_GRACE_MS;
    assert!(
        enqueue_with(&spool, "owner/name", root, "d.py", later, alive)
            .expect("d")
            .spawn
    );
}

#[test]
fn a_dead_flusher_is_replaced_and_a_live_one_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    assert!(claim_with(&spool, 4242, 1_000, alive).expect("claim"));
    assert!(
        !claim_with(&spool, 5151, 1_001, alive).expect("second"),
        "live owner wins"
    );
    let root = Path::new("/repo");
    assert!(
        !enqueue_with(&spool, "owner/name", root, "a.py", 1_002, alive)
            .expect("a")
            .spawn
    );
    assert!(
        enqueue_with(&spool, "owner/name", root, "b.py", 1_003, dead)
            .expect("b")
            .spawn
    );
}

#[test]
fn the_pending_set_is_bounded_and_overflow_is_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let root = Path::new("/repo");
    for index in 0..MAX_PENDING_PATHS + 7 {
        enqueue_with(
            &spool,
            "owner/name",
            root,
            &format!("f{index}.py"),
            1,
            alive,
        )
        .expect("q");
    }
    let state = load(&spool);
    assert_eq!(state.pending.len(), MAX_PENDING_PATHS);
    assert_eq!(state.overflowed, 7);
}

#[test]
fn a_back_off_suppresses_the_spawn_until_it_expires() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    abandon(&spool, "no credential", 1_000).expect("abandon");
    let root = Path::new("/repo");
    let during = enqueue_with(&spool, "owner/name", root, "a.py", 2_000, dead).expect("during");
    assert!(!during.spawn);
    assert!(
        during
            .report
            .expect("reported once")
            .contains("no credential")
    );
    let again = enqueue_with(&spool, "owner/name", root, "a.py", 2_001, dead).expect("again");
    assert_eq!(
        again.report, None,
        "a failure is reported once, not on every write"
    );
    let after = 1_000 + backoff_ms(MAX_RETRY_ATTEMPTS, false);
    assert!(
        enqueue_with(&spool, "owner/name", root, "a.py", after, dead)
            .expect("after")
            .spawn
    );
}

#[test]
fn backoff_is_exponential_and_capped() {
    assert_eq!(backoff_ms(1, true), 5_000);
    assert_eq!(backoff_ms(2, true), 10_000);
    assert_eq!(backoff_ms(3, true), 20_000);
    assert_eq!(backoff_ms(40, true), 120_000);
    assert_eq!(backoff_ms(1, false), 120_000);
}

/// A fake clock that delivers scheduled writes while the flusher sleeps, and a server that answers
/// from a script. Every write goes through the REAL `enqueue_with`.
struct Simulated<'a> {
    spool: &'a Path,
    root: &'a Path,
    now: u64,
    writes: Vec<(u64, String)>,
    delivered: usize,
    answers: Vec<PostOutcome>,
    requests: Vec<Vec<String>>,
    sleeps: Vec<Duration>,
    spawns: u32,
}

impl<'a> Simulated<'a> {
    /// Real clocks never read 0, and 0 is the spool's "no flusher" sentinel, so simulated time
    /// starts later — a simulation at t=0 measured a sentinel collision, not the design.
    fn new(spool: &'a Path, writes: Vec<(u64, String)>, answers: Vec<PostOutcome>) -> Self {
        Self {
            spool,
            root: Path::new("/repo"),
            now: START,
            writes: writes
                .into_iter()
                .map(|(at, key)| (at + START, key))
                .collect(),
            delivered: 0,
            answers,
            requests: Vec::new(),
            sleeps: Vec::new(),
            spawns: 0,
        }
    }

    fn deliver_until(&mut self, until: u64) {
        while self.delivered < self.writes.len() && self.writes[self.delivered].0 <= until {
            let (at, key) = self.writes[self.delivered].clone();
            let queued = enqueue_with(self.spool, "owner/name", self.root, &key, at, alive)
                .expect("enqueue");
            self.spawns += u32::from(queued.spawn);
            self.delivered += 1;
        }
    }
}

impl FlushPort for Simulated<'_> {
    async fn post(&mut self, paths: &[String], _drained_at: u64) -> PostOutcome {
        self.requests.push(paths.to_vec());
        if self.answers.len() > 1 {
            self.answers.remove(0)
        } else {
            self.answers[0].clone()
        }
    }

    async fn sleep(&mut self, duration: Duration) {
        self.sleeps.push(duration);
        self.now += u64::try_from(duration.as_millis()).expect("ms");
        self.deliver_until(self.now);
    }

    fn now(&self) -> u64 {
        self.now
    }
}

/// Drive every flusher the writes cause, the way the hooks would spawn them, bounded by the number
/// of writes (each flusher needs at least one write to exist).
async fn simulate(sim: &mut Simulated<'_>) -> Vec<FlushReport> {
    sim.deliver_until(sim.now);
    let mut reports = Vec::new();
    let mut started = 0;
    while started < sim.spawns && started as usize <= sim.writes.len() {
        started += 1;
        assert!(claim_with(sim.spool, 4242, sim.now, alive).expect("claim"));
        let spool = sim.spool;
        reports.push(run_flusher(spool, sim).await);
        if sim.spawns == started && sim.delivered < sim.writes.len() {
            let next = sim.writes[sim.delivered].0;
            sim.now = next;
            sim.deliver_until(next);
        }
    }
    reports
}

#[tokio::test]
async fn n_writes_inside_one_window_become_one_request_with_n_paths() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let writes = (0..12).map(|i| (i * 300, format!("src/f{i}.py"))).collect();
    let mut sim = Simulated::new(&spool, writes, vec![PostOutcome::Accepted]);
    let reports = simulate(&mut sim).await;
    assert_eq!(sim.requests.len(), 1, "{:?}", sim.requests);
    assert_eq!(sim.requests[0].len(), 12);
    assert_eq!(reports.iter().map(|r| r.paths_sent).sum::<usize>(), 12);
    assert!(load(&spool).pending.is_empty());
    assert_eq!(
        load(&spool).flusher_since_ms,
        0,
        "the flusher released its slot"
    );
}

/// THE MEASUREMENT the PR body quotes: 50 writes from 5 sessions over 10 s. The old hook sent one
/// request per admitted write, so its count is the number of writes; this counts what is sent now.
// Prints the measurement the PR quotes, so `--nocapture` shows the number, not just a pass.
#[allow(clippy::print_stderr)]
#[tokio::test]
async fn a_burst_of_fifty_writes_across_five_sessions() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let mut writes: Vec<(u64, String)> = (0..50u64)
        .map(|i| (i * 200, format!("session{}/file{}.py", i % 5, i / 5)))
        .collect();
    writes.sort();
    let mut sim = Simulated::new(&spool, writes, vec![PostOutcome::Accepted]);
    simulate(&mut sim).await;
    let sent: usize = sim.requests.iter().map(Vec::len).sum();
    eprintln!(
        "BURST: 50 writes / 5 sessions / 10 s -> before 50 requests, after {} requests carrying {sent} paths",
        sim.requests.len()
    );
    assert_eq!(sent, 50, "every write reached a request");
    assert!(sim.requests.len() <= 3, "{} requests", sim.requests.len());
}

#[tokio::test]
async fn a_429_backs_off_a_bounded_number_of_times_and_keeps_the_paths() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let writes = (0..5).map(|i| (i * 100, format!("f{i}.py"))).collect();
    let refusal = PostOutcome::Retry("Estelle answered and declined (http 429)".to_string());
    let mut sim = Simulated::new(&spool, writes, vec![refusal]);
    let reports = simulate(&mut sim).await;
    assert_eq!(
        sim.requests.len() as u32,
        MAX_RETRY_ATTEMPTS,
        "bounded, not hammering"
    );
    assert!(reports.last().expect("report").gave_up);
    let state = load(&spool);
    assert_eq!(
        state.pending.len(),
        5,
        "refused paths stay spooled for the catch-up"
    );
    assert!(state.backoff_until_ms > sim.now);
    assert!(state.last_error.expect("reported").contains("http 429"));
    // First a window; then each retry waits the LONGER of its back-off (5 s, then 10 s) and the
    // per-repo spacing since the previous post. The post happens at the instant the window ends, so
    // the spacing is the whole 20 s and wins both times.
    assert_eq!(
        sim.sleeps,
        vec![
            DEBOUNCE_WINDOW,
            MIN_REPO_POST_INTERVAL,
            MIN_REPO_POST_INTERVAL
        ]
    );
}

#[tokio::test]
async fn a_non_retryable_refusal_stops_after_one_request() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let writes = vec![(0, "a.py".to_string())];
    let refusal = PostOutcome::Failed("Estelle answered and declined (http 402)".to_string());
    let mut sim = Simulated::new(&spool, writes, vec![refusal]);
    simulate(&mut sim).await;
    assert_eq!(sim.requests.len(), 1);
    assert_eq!(
        load(&spool).backoff_until_ms,
        sim.now + backoff_ms(1, false)
    );
}

#[tokio::test]
async fn a_retry_then_success_sends_the_same_paths_again_once() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let writes = vec![(0, "a.py".to_string()), (100, "b.py".to_string())];
    let answers = vec![
        PostOutcome::Retry("http 503".to_string()),
        PostOutcome::Accepted,
    ];
    let mut sim = Simulated::new(&spool, writes, answers);
    simulate(&mut sim).await;
    assert_eq!(sim.requests.len(), 2);
    assert_eq!(sim.requests[0], sim.requests[1]);
    let state = load(&spool);
    assert!(state.pending.is_empty());
    assert_eq!((state.failures, state.backoff_until_ms), (0, 0));
}

#[tokio::test]
async fn a_batch_never_exceeds_its_bound() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_in(&dir);
    let writes = (0..(MAX_BATCH_PATHS as u64 + 50))
        .map(|i| (0, format!("f{i:04}.py")))
        .collect();
    let mut sim = Simulated::new(&spool, writes, vec![PostOutcome::Accepted]);
    simulate(&mut sim).await;
    assert_eq!(sim.requests.len(), 2);
    assert_eq!(sim.requests[0].len(), MAX_BATCH_PATHS);
    assert_eq!(sim.requests[1].len(), 50);
}

// ---- per-REPOSITORY spacing across worktrees (2026-10-29) ------------------------------------------

#[test]
fn the_repo_slot_is_shared_by_every_root_of_one_repo_and_only_that_repo() {
    let dir = tempfile::tempdir().expect("dir");
    let repo = repo_marker_in(dir.path(), "owner/name");
    let other = repo_marker_in(dir.path(), "owner/other");
    let interval = u64::try_from(MIN_REPO_POST_INTERVAL.as_millis()).expect("ms");
    assert_eq!(
        reserve_repo_post(&repo, START).expect("reserve"),
        0,
        "a fresh repo posts at once"
    );
    assert_eq!(
        reserve_repo_post(&repo, START + 5_000).expect("reserve"),
        interval - 5_000,
        "a second worktree of the SAME repo waits out the interval"
    );
    // Control: the spacing is per repository, not a global throttle.
    assert_eq!(
        reserve_repo_post(&other, START + 5_000).expect("reserve"),
        0
    );
    assert_eq!(
        reserve_repo_post(&repo, START + interval).expect("reserve"),
        0
    );
    assert_eq!(
        std::fs::read_to_string(&repo).expect("marker").trim(),
        (START + interval).to_string()
    );
}

/// THE CASE THAT DROVE THE OOM: two worktrees of ONE repository, both writing. Before this, each
/// root's flusher posted one window after its first write, so both posted 5 s in. Now the second
/// root's post lands a full interval after the first, and each request carries only its own root's
/// paths. Discriminating: with the marker ignored, `gap` is 0.
#[tokio::test]
async fn two_worktrees_of_one_repo_are_spaced_and_send_only_their_own_paths() {
    let dir = tempfile::tempdir().expect("dir");
    let root_a = Path::new("/lanes/a");
    let root_b = Path::new("/lanes/b");
    let spool_a = spool_path_in(dir.path(), "owner/name", root_a);
    let spool_b = spool_path_in(dir.path(), "owner/name", root_b);
    assert_ne!(spool_a, spool_b, "two roots are two spools");

    let mut a = Simulated::new(
        &spool_a,
        vec![(0, "src/a.py".to_string())],
        vec![PostOutcome::Accepted],
    );
    a.root = root_a;
    simulate(&mut a).await;
    let marker = repo_marker_in(dir.path(), "owner/name");
    let posted_a = read_last_post(&marker);
    assert!(posted_a > START, "A reserved the repo slot when it posted");

    let mut b = Simulated::new(
        &spool_b,
        vec![(0, "src/b.py".to_string())],
        vec![PostOutcome::Accepted],
    );
    b.root = root_b;
    simulate(&mut b).await;

    assert_eq!(a.requests, vec![vec!["src/a.py".to_string()]]);
    assert_eq!(
        b.requests,
        vec![vec!["src/b.py".to_string()]],
        "B never sends A's paths"
    );
    let interval = u64::try_from(MIN_REPO_POST_INTERVAL.as_millis()).expect("ms");
    let gap = read_last_post(&marker) - posted_a;
    assert!(
        gap >= interval,
        "B posted {gap} ms after A; the floor is {interval} ms"
    );
    assert!(
        load(&spool_b).pending.is_empty(),
        "B still delivered, just later"
    );
}

#[test]
fn next_wait_reads_the_repo_marker_and_ignores_another_repos() {
    let dir = tempfile::tempdir().expect("dir");
    let spool = spool_path_in(dir.path(), "owner/name", Path::new("/lanes/b"));
    enqueue_with(
        &spool,
        "owner/name",
        Path::new("/lanes/b"),
        "x.py",
        START,
        alive,
    )
    .expect("enqueue");
    assert_eq!(
        next_wait(&spool, START),
        DEBOUNCE_WINDOW,
        "no marker: one window"
    );
    reserve_repo_post(&repo_marker_in(dir.path(), "owner/other"), START).expect("other repo");
    assert_eq!(
        next_wait(&spool, START),
        DEBOUNCE_WINDOW,
        "another repo's post does not slow this one"
    );
    reserve_repo_post(&repo_marker_in(dir.path(), "owner/name"), START).expect("sibling root");
    assert_eq!(
        next_wait(&spool, START + 1_000),
        MIN_REPO_POST_INTERVAL - Duration::from_millis(1_000)
    );
}
