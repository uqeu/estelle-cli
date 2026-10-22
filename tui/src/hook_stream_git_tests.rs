use super::*;
use chrono::TimeZone;
use std::process::Command;

fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn hunks_only_drops_the_file_headers() {
    let diff = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new";
    assert_eq!(hunks_only(diff), "@@ -1 +1 @@\n-old\n+new");
}

#[test]
fn status_resolves_renames_and_skips_build_output() {
    let status = " M src/a.rs\nR  old.rs -> new.rs\n?? node_modules/x.js\n?? \"with space.txt\"\n";
    assert_eq!(
        parse_status(status),
        vec![
            (" M".to_string(), "src/a.rs".to_string()),
            ("R ".to_string(), "new.rs".to_string()),
            ("??".to_string(), "with space.txt".to_string()),
        ]
    );
}

#[test]
fn a_file_counts_only_when_the_session_named_it() {
    let mut state = GitState::default();
    remember(
        &mut state,
        &["sed -i s/a/b/ tui/src/hook_stream.rs && cat README.md".to_string()],
    );
    assert!(mentioned("tui/src/hook_stream.rs", &state.mentions));
    assert!(mentioned("README.md", &state.mentions));
    // `parent/name` tail, the bridge's second rule.
    assert!(mentioned("cli-rs/tui/src/hook_stream.rs", &state.mentions));
    assert!(
        !mentioned("tui/src/other.rs", &state.mentions),
        "a file the session never named is not its change"
    );
}

#[test]
fn the_mention_set_is_bounded_and_keeps_the_newest() {
    let mut state = GitState::default();
    let texts: Vec<String> = (0..MAX_MENTIONS + 10)
        .map(|i| format!("src/f{i}.rs"))
        .collect();
    remember(&mut state, &texts);
    assert_eq!(state.mentions.len(), MAX_MENTIONS);
    assert!(mentioned(
        &format!("src/f{}.rs", MAX_MENTIONS + 9),
        &state.mentions
    ));
    assert!(!mentioned("src/f0.rs", &state.mentions));
}

#[test]
fn a_diff_is_sent_once_per_digest_capped_and_redacted() {
    let mut state = GitState::default();
    let diff = format!(
        "@@ -0,0 +1 @@\n+KEY=ghp_abcdefghijklmnopqrstuvwxyz0123\n+{}",
        "x".repeat(MAX_DIFF_BYTES)
    );
    let at = "2026-10-21T10:00:00.000Z";
    let first = diff_event(
        &mut state,
        "src/a.rs",
        false,
        (diff.clone(), diff.clone()),
        at,
    )
    .expect("sent");
    assert!(
        first["event_id"]
            .as_str()
            .unwrap()
            .starts_with("git:src/a.rs:")
    );
    assert_eq!(first["payload"]["truncated"], true);
    assert!(
        first["payload"]["full"].is_null(),
        "a full diff over the cap is not sent"
    );
    let shown = first["payload"]["diff"].as_str().unwrap();
    assert!(
        !shown.contains("ghp_"),
        "diffs are redacted like every other field"
    );
    assert!(
        diff_event(&mut state, "src/a.rs", false, (diff.clone(), diff), at).is_none(),
        "same digest: not resent"
    );
}

#[test]
fn an_id_the_server_would_refuse_falls_back_to_a_hashed_path() {
    let mut state = GitState::default();
    let out = diff_event(
        &mut state,
        "docs/a file.md",
        true,
        ("@@ x".into(), "@@ x".into()),
        "2026-10-21T10:00:00Z",
    )
    .unwrap();
    let id = out["event_id"].as_str().unwrap();
    assert!(id.starts_with("git:") && !id.contains(' '), "{id}");
    assert_eq!(out["payload"]["path"], "docs/a file.md");
    assert_eq!(out["payload"]["status"], "added");
}

#[test]
fn commits_after_the_grace_are_skipped_and_times_are_utc_and_never_after_the_last_entry() {
    let last = Some(utc("2026-10-21T14:00:00Z"));
    let now = utc("2026-10-22T00:00:00Z");
    let log = "aaa\x1fKhai\x1f2026-10-21T09:30:00-04:00\x1ffix: one\n\nM\tsrc/a.rs\nA\tsrc/b.rs\n\
               bbb\x1fKhai\x1f2026-10-21T10:03:00-04:00\x1ffeat: within grace\n\nM\tsrc/c.rs\n\
               ccc\x1fOther\x1f2026-10-21T10:06:00-04:00\x1fanother session\n\nM\tsrc/d.rs\n";
    let out = commit_events(log, "estelle", last, now, &[]);
    let shas: Vec<&str> = out
        .iter()
        .map(|e| e["payload"]["sha"].as_str().unwrap())
        .collect();
    assert_eq!(
        shas,
        ["aaa", "bbb"],
        "a commit 6 minutes after the last entry is another session's"
    );
    assert_eq!(out[0]["event_id"], "commit:aaa");
    assert_eq!(
        out[0]["at"], "2026-10-21T13:30:00.000Z",
        "UTC, never the -04:00 offset"
    );
    assert_eq!(out[0]["payload"]["files"], json!(["src/a.rs", "src/b.rs"]));
    assert_eq!(
        out[1]["at"], "2026-10-21T14:00:00.000Z",
        "clamped to the transcript's last entry"
    );
    assert_eq!(
        out[1]["payload"]["files"],
        json!(["src/c.rs"]),
        "the skipped commit's files are not attached"
    );
    assert!(commit_events(log, "estelle", last, now, &["aaa".into(), "bbb".into()]).is_empty());
}

#[test]
fn the_tree_is_split_into_parts_and_resent_only_when_it_changes() {
    let mut state = GitState::default();
    let listing: String = (0..TREE_PART + 1).map(|i| format!("f{i}\n")).collect();
    let at = "2026-10-21T10:00:00Z";
    let out = tree_events(&listing, "estelle", &mut state, at);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["payload"]["parts"], 2);
    assert_eq!(out[1]["payload"]["paths"].as_array().unwrap().len(), 1);
    assert!(
        out[0]["event_id"]
            .as_str()
            .unwrap()
            .starts_with("tree:estelle:")
    );
    assert!(tree_events(&listing, "estelle", &mut state, at).is_empty());
    assert_eq!(tree_events("other\n", "estelle", &mut state, at).len(), 1);
}

#[test]
fn claude_mentions_read_the_tool_inputs_only() {
    let entry = json!({"message": {"content": [
        {"type": "text", "text": "src/said.rs"},
        {"type": "tool_use", "name": "Bash", "input": {"command": "cargo fmt", "description": "src/no.rs"}},
        {"type": "tool_use", "name": "Edit", "input": {"file_path": "/r/src/a.rs"}},
    ]}});
    assert_eq!(
        claude_mentions(&entry),
        vec!["cargo fmt".to_string(), "/r/src/a.rs".to_string()]
    );
}

fn sh(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_DATE", "2026-10-21T10:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-10-21T10:00:00Z")
        .output()
        .expect("git runs")
        .status
        .success();
    assert!(ok, "git {args:?}");
}

/// The real thing, end to end, against a temporary repository: a named tracked edit, a named new file, an
/// UNNAMED edit (must not appear), one commit, and the tree. Then a second run sends nothing new.
#[tokio::test]
async fn collect_reads_a_real_repository_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    sh(&repo, &["init", "-q"]);
    sh(&repo, &["config", "user.email", "t@example.com"]);
    sh(&repo, &["config", "user.name", "Tester"]);
    std::fs::write(repo.join("src/a.rs"), "one\n").unwrap();
    std::fs::write(repo.join("src/quiet.rs"), "one\n").unwrap();
    sh(&repo, &["add", "."]);
    sh(&repo, &["commit", "-q", "-m", "base"]);
    std::fs::write(repo.join("src/a.rs"), "two\n").unwrap();
    std::fs::write(repo.join("src/quiet.rs"), "two\n").unwrap();
    std::fs::write(repo.join("src/new.rs"), "fresh\n").unwrap();
    let state_file = dir.path().join("s.git.json");
    let input = GitInput {
        root: &repo.join("src"), // from a SUBDIRECTORY: the top level is found by git
        state_file: &state_file,
        first_at: Some(Utc.with_ymd_and_hms(2026, 10, 21, 9, 0, 0).unwrap()),
        last: Some(Utc.with_ymd_and_hms(2026, 10, 21, 10, 1, 0).unwrap()),
        now: Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0).unwrap(),
        force: true,
        mentions: vec![
            "sed -i s/one/two/ src/a.rs".into(),
            "cat > src/new.rs".into(),
        ],
    };
    let (events, state) = collect(&input).await;
    let kinds: Vec<(&str, &str)> = events
        .iter()
        .map(|e| (e["kind"].as_str().unwrap(), e["event_id"].as_str().unwrap()))
        .collect();
    let diffs: Vec<&str> = events
        .iter()
        .filter(|e| e["kind"] == "workspace_diff")
        .map(|e| e["payload"]["path"].as_str().unwrap())
        .collect();
    assert_eq!(diffs.len(), 2, "{kinds:?}");
    assert!(diffs.contains(&"src/a.rs") && diffs.contains(&"src/new.rs"));
    assert!(
        !diffs.contains(&"src/quiet.rs"),
        "a file the session never named is not its change"
    );
    let a = events
        .iter()
        .find(|e| e["payload"]["path"] == "src/a.rs")
        .unwrap();
    assert_eq!(a["payload"]["diff"], "@@ -1 +1 @@\n-one\n+two");
    assert_eq!(
        a["at"], "2026-10-21T10:01:00.000Z",
        "a file changed now is stamped at the last entry"
    );
    assert_eq!(events.iter().filter(|e| e["kind"] == "commit").count(), 1);
    let tree = events
        .iter()
        .find(|e| e["kind"] == "workspace_tree")
        .expect("tree");
    assert_eq!(
        tree["payload"]["paths"],
        json!(["src/a.rs", "src/quiet.rs"])
    );
    assert_eq!(tree["payload"]["root"], "repo");
    for e in &events {
        assert!(
            e["at"].as_str().unwrap() <= "2026-10-21T10:01:00.000Z",
            "{e}"
        );
    }
    crate::hook_stream_run::save_json(&state_file, &state);
    let (again, _) = collect(&input).await;
    assert!(again.is_empty(), "the second run resent {again:?}");
}

#[tokio::test]
async fn a_run_inside_the_snapshot_interval_reads_no_git_unless_forced() {
    let dir = tempfile::tempdir().unwrap();
    let state_file = dir.path().join("s.git.json");
    let now = Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0).unwrap();
    let state = GitState {
        last_snapshot: Some(now),
        first_at: Some(now),
        ..GitState::default()
    };
    crate::hook_stream_run::save_json(&state_file, &state);
    let input = GitInput {
        root: Path::new("/definitely/not/a/repo"),
        state_file: &state_file,
        first_at: None,
        last: None,
        now: now + chrono::Duration::seconds(SNAPSHOT_EVERY_S - 1),
        force: false,
        mentions: vec!["src/x.rs".into()],
    };
    let (events, saved) = collect(&input).await;
    assert!(events.is_empty());
    assert_eq!(
        saved.last_snapshot,
        Some(now),
        "not due: the clock did not move"
    );
    assert_eq!(
        saved.mentions,
        vec!["src/x.rs".to_string()],
        "mentions are still remembered"
    );
}

#[tokio::test]
async fn git_returns_none_when_the_command_fails() {
    let dir = tempfile::tempdir().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(git(dir.path(), &["log"], deadline).await.is_none());
    assert!(git(dir.path(), &["--version"], deadline).await.is_some());
}
