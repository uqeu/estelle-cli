//! The sync hook and its flusher against a REAL HTTP server (wiremock), counting the requests that
//! actually arrive. The spool tests prove the coalescing logic with a scripted server; these prove
//! the bytes on the wire: one `POST /reindex` whose `files` carries every path.

use std::path::Path;
use std::process::Command as ProcessCommand;
use std::time::Duration;

use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::reindex_spool::FlushPort;
use crate::reindex_spool::PostOutcome;

const SESSIONS: u64 = 5;
const WRITES: u64 = 50;
const WRITE_SPACING_MS: u64 = 200;
/// Simulated time starts here: 0 is the spool's "no flusher" sentinel and a real clock never reads it.
const START: u64 = 1_000_000;

fn git_repo_with(files: &[String]) -> tempfile::TempDir {
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
    for (index, file) in files.iter().enumerate() {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, format!("def f{index}():\n    return {index}\n")).expect("write");
    }
    root
}

async fn server_answering(status: u16) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/reindex"))
        .respond_with(
            wiremock::ResponseTemplate::new(status)
                .set_body_json(json!({"indexed": 1, "error": {"message": "x"}})),
        )
        .mount(&server)
        .await;
    server
}

fn api_for(server: &wiremock::MockServer) -> Api {
    let key = estelle_client::ApiKey::new("test-key").expect("key");
    Api {
        client: Client::new(
            &format!("{}/", server.uri()),
            key.clone(),
            Duration::from_secs(120),
        )
        .expect("client"),
        api_key: key,
        cancel: CancellationToken::new(),
    }
}

async fn reindex_bodies(server: &wiremock::MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .expect("recording")
        .iter()
        .filter(|request| request.url.path() == "/reindex")
        .map(|request| serde_json::from_slice(&request.body).expect("json body"))
        .collect()
}

fn burst_files() -> Vec<String> {
    (0..WRITES)
        .map(|i| format!("session{}/file{}.py", i % SESSIONS, i / SESSIONS))
        .collect()
}

/// The real HTTP path with a fake clock: the flusher's sleeps advance time and deliver the writes
/// that fall inside them through the REAL `sync_hook_with`.
struct WiredPort<'a> {
    api: &'a Api,
    repo: &'a Repo,
    root: &'a Path,
    dir: &'a Path,
    now: u64,
    writes: Vec<(u64, String)>,
    delivered: usize,
}

impl WiredPort<'_> {
    fn deliver_until(&mut self, until: u64) {
        while self.delivered < self.writes.len() && self.writes[self.delivered].0 <= until {
            let (at, file) = self.writes[self.delivered].clone();
            let path = self.root.join(&file).to_string_lossy().into_owned();
            let lines = sync_hook_with(&path, self.repo, self.root, self.dir, at, |_| Ok(()))
                .expect("hook");
            assert!(lines.is_empty(), "a healthy write says nothing: {lines:?}");
            self.delivered += 1;
        }
    }
}

impl FlushPort for WiredPort<'_> {
    async fn post(&mut self, paths: &[String], drained_at: u64) -> PostOutcome {
        post_reindex_batch(self.api, self.repo, self.root, paths, drained_at).await
    }

    async fn sleep(&mut self, duration: Duration) {
        self.now += u64::try_from(duration.as_millis()).expect("ms");
        self.deliver_until(self.now);
    }

    fn now(&self) -> u64 {
        self.now
    }
}

/// 🔴 THE MEASUREMENT. Same 50 writes, same server, same POST function.
///
/// BEFORE is the origin/main hook's shape — `collect_files([path])` then ONE `post_scoped` per
/// write (`sync_hook`, top_level.rs on origin/main) — reproduced by calling the batch poster with a
/// one-path batch per write. ⚠️ It does not execute origin/main's binary; it counts what that call
/// shape puts on the wire. AFTER is the shipped path end to end: hook → spool → flusher → HTTP.
// Prints the measurement the PR quotes, so `--nocapture` shows the number, not just a pass.
#[allow(clippy::print_stderr)]
#[tokio::test]
async fn fifty_writes_across_five_sessions_on_the_wire() {
    let files = burst_files();
    let root = git_repo_with(&files);
    let repo = Repo::default();

    let before_server = server_answering(200).await;
    let before_api = api_for(&before_server);
    for file in &files {
        let outcome = post_reindex_batch(
            &before_api,
            &repo,
            root.path(),
            std::slice::from_ref(file),
            0,
        )
        .await;
        assert_eq!(outcome, PostOutcome::Accepted);
    }
    let before = reindex_bodies(&before_server).await;

    let after_server = server_answering(200).await;
    let after_api = api_for(&after_server);
    let spool_dir = tempfile::tempdir().expect("spool");
    let writes = files
        .iter()
        .enumerate()
        .map(|(i, file)| (START + i as u64 * WRITE_SPACING_MS, file.clone()))
        .collect();
    let mut port = WiredPort {
        api: &after_api,
        repo: &repo,
        root: root.path(),
        dir: spool_dir.path(),
        now: START,
        writes,
        delivered: 0,
    };
    port.deliver_until(START);
    let spool = reindex_spool::spool_path_in(spool_dir.path(), repo.as_str(), root.path());
    assert!(reindex_spool::claim(&spool, std::process::id(), START).expect("claim"));
    let report = reindex_spool::run_flusher(&spool, &mut port).await;
    let after = reindex_bodies(&after_server).await;

    let sent: usize = after
        .iter()
        .map(|body| body["files"].as_array().map_or(0, Vec::len))
        .sum();
    eprintln!(
        "BURST ON THE WIRE: {WRITES} writes / {SESSIONS} sessions / {} ms -> before {} POST /reindex, after {} POST /reindex carrying {sent} files",
        WRITES * WRITE_SPACING_MS,
        before.len(),
        after.len(),
    );
    assert_eq!(
        before.len(),
        WRITES as usize,
        "the old shape is one request per write"
    );
    assert_eq!(after.len() as u32, report.requests);
    assert_eq!(sent, WRITES as usize, "no write was lost in the coalescing");
    assert!(after.len() <= 3, "coalesced into {} requests", after.len());
    for body in &after {
        assert!(body.get("removed").is_none(), "nothing was deleted: {body}");
    }
}

// Prints the measurement the PR quotes, so `--nocapture` shows the number, not just a pass.
#[allow(clippy::print_stderr)]
#[tokio::test]
async fn a_persistent_429_is_three_requests_not_one_per_write() {
    let files = burst_files();
    let root = git_repo_with(&files);
    let repo = Repo::default();
    let server = server_answering(429).await;
    let api = api_for(&server);
    let spool_dir = tempfile::tempdir().expect("spool");
    let writes = files
        .iter()
        .enumerate()
        .map(|(i, file)| (START + i as u64 * WRITE_SPACING_MS, file.clone()))
        .collect();
    let mut port = WiredPort {
        api: &api,
        repo: &repo,
        root: root.path(),
        dir: spool_dir.path(),
        now: START,
        writes,
        delivered: 0,
    };
    port.deliver_until(START);
    let spool = reindex_spool::spool_path_in(spool_dir.path(), repo.as_str(), root.path());
    assert!(reindex_spool::claim(&spool, std::process::id(), START).expect("claim"));
    let report = reindex_spool::run_flusher(&spool, &mut port).await;
    let arrived = reindex_bodies(&server).await.len();
    eprintln!("PERSISTENT 429: {WRITES} writes -> {arrived} requests, then stopped");
    assert!(report.gave_up);
    assert_eq!(arrived as u32, reindex_spool::MAX_RETRY_ATTEMPTS);
    // The refusal reaches the agent on its next write, once.
    let next = root.path().join(&files[0]).to_string_lossy().into_owned();
    let lines = sync_hook_with(
        &next,
        &repo,
        root.path(),
        spool_dir.path(),
        port.now,
        |_| panic!("no spawn during the back-off"),
    )
    .expect("hook");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("http 429"), "{}", lines[0]);
}

#[tokio::test]
async fn a_deleted_file_goes_out_as_removed_and_a_filtered_one_never_leaves() {
    let files = vec!["src/a.py".to_string(), "src/gone.py".to_string()];
    let root = git_repo_with(&files);
    let repo = Repo::default();
    let spool_dir = tempfile::tempdir().expect("spool");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("x.py"), "x = 1\n").expect("outside");
    for path in [
        outside.path().join("x.py"),
        root.path().join("src/a.py"),
        root.path().join("src/gone.py"),
        root.path().join(".env"),
    ] {
        sync_hook_with(
            &path.to_string_lossy(),
            &repo,
            root.path(),
            spool_dir.path(),
            START,
            |_| Ok(()),
        )
        .expect("hook");
    }
    std::fs::remove_file(root.path().join("src/gone.py")).expect("rm");
    let server = server_answering(200).await;
    let api = api_for(&server);
    let paths = vec!["src/a.py".to_string(), "src/gone.py".to_string()];
    let spool = reindex_spool::spool_path_in(spool_dir.path(), repo.as_str(), root.path());
    let pending: Vec<String> = serde_json::from_slice::<Value>(
        &std::fs::read(&spool).expect("spool"),
    )
    .expect("json")["pending"]
        .as_array()
        .expect("pending")
        .iter()
        .map(|value| value.as_str().expect("str").to_string())
        .collect();
    assert_eq!(
        pending, paths,
        "outside-repo and credential files never reached the spool"
    );
    assert_eq!(
        post_reindex_batch(&api, &repo, root.path(), &paths, 0).await,
        PostOutcome::Accepted
    );
    let bodies = reindex_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["files"][0]["path"], "src/a.py");
    assert_eq!(bodies[0]["removed"], json!(["src/gone.py"]));
}

/// 🔴 A LANE'S EDIT IS DECLARED, AND A DECLINE IS NEVER READ AS "INDEXED". The body carries the
/// checkout identity the server rules on, and a `skipped_off_default` answer is its own outcome —
/// before this, any 2xx was `Accepted` and stamped the local index as current.
#[tokio::test]
async fn a_declined_lane_edit_is_not_accepted_and_the_body_names_its_checkout() {
    let files = vec!["src/a.py".to_string()];
    let root = git_repo_with(&files);
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/reindex"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(
                json!({"indexed": 0, "skipped_off_default": true, "reason": "lane"}),
            ),
        )
        .mount(&server)
        .await;
    let api = api_for(&server);
    let outcome = post_reindex_batch(&api, &Repo::default(), root.path(), &files, 0).await;
    assert_eq!(outcome, PostOutcome::SkippedOffDefault);
    let bodies = reindex_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["checkout"]["linked_worktree"], json!(false));
}
