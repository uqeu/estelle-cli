use super::*;
use pretty_assertions::assert_eq;

/// Live-SHAPED, obviously fake, and built at runtime so no credential-shaped literal sits in the
/// source for a scanner (ours or anyone's) to trip on.
fn fake_stripe() -> String {
    format!("sk_live_{}{}", "FAKEFAKE", "0123456789abcdef")
}

fn fake_github() -> String {
    format!("ghp_{}", "FakeToken0".repeat(4))
}

/// The CONTROL for every block below: the detector this module reuses must itself call the fake a
/// credential, or a "blocked" assertion would be measuring nothing.
#[test]
fn the_fakes_are_live_shaped_to_the_shared_detector() {
    assert_eq!(
        estelle_client::find_secret_shape(&fake_stripe()).map(|(kind, _)| kind),
        Some("a Stripe live key")
    );
    assert_eq!(
        estelle_client::find_secret_shape(&fake_github()).map(|(kind, _)| kind),
        Some("a GitHub token")
    );
}

/// The known false positives, each of which has blocked a real user or a real fixture before.
fn known_false_positives() -> Vec<String> {
    vec![
        // 0.4.2: a branch name read as an sk- key ("a·sk-flow-…").
        "git checkout fix/ask-flow-automation-title-kickoff".to_string(),
        "the task-runner-orchestration-pipeline-default is fine".to_string(),
        // 2026-08-10: AWS's published documentation example, allowlisted upstream.
        format!("aws_access_key_id = AKIA{}", "IOSFODNN7EXAMPLE"),
        // High-entropy, but not a credential.
        format!(
            "sha256 = \"{}\"",
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        ),
        "id = \"3fa85f64-5717-4562-b3fc-2c963f66afa6\"".to_string(),
        "DATABASE_URL=postgres://user:pass@localhost:5432/app".to_string(),
    ]
}

#[test]
fn known_false_positives_are_not_findings() {
    for text in known_false_positives() {
        let input = json!({ "content": text });
        assert_eq!(write_findings("a.txt", &input, None), Vec::new(), "{text}");
    }
}

#[test]
fn a_write_carrying_a_live_shaped_key_is_blocked_by_file_line_and_type_never_value() {
    let key = fake_stripe();
    let input = json!({ "content": format!("import os\n\nSTRIPE = \"{key}\"\n") });
    let findings = write_findings("billing.py", &input, None);
    let verdict = decide("write", &findings, None, None);
    let Verdict::Block(reason) = verdict else {
        panic!("expected a block, got {verdict:?}");
    };
    assert!(
        reason.contains("billing.py:3 (a Stripe live key)"),
        "{reason}"
    );
    assert!(
        !reason.contains(&key),
        "the refusal must never carry the value"
    );
    assert!(!reason.contains("FAKEFAKE"), "not even part of it");
    assert!(
        reason.contains(ALLOW_MARKER),
        "the refusal offers the bypass"
    );
}

#[test]
fn an_edit_is_placed_at_its_file_line_when_the_file_is_readable() {
    let file = "a\nb\nc\nOLD\n";
    let input =
        json!({ "old_string": "OLD", "new_string": format!("x\ntoken = \"{}\"", fake_github()) });
    let findings = write_findings("cfg.toml", &input, Some(file));
    assert_eq!(findings.len(), 1);
    assert_eq!((findings[0].line, findings[0].place.as_str()), (5, ""));
    // Unreadable file: still found, and the line says what it counts.
    let unplaced = write_findings("cfg.toml", &input, None);
    assert_eq!(unplaced[0].line, 2);
    assert_eq!(unplaced[0].place, "line of the edit's new text");
}

#[test]
fn an_allow_marker_with_a_reason_is_a_recorded_bypass_and_a_bare_one_is_not() {
    let dir = tempfile::tempdir().expect("tmp");
    let log = dir.path().join("push-protection.jsonl");
    let key = fake_stripe();
    let marked =
        json!({ "content": format!("K = \"{key}\"  # {ALLOW_MARKER} revoked test fixture") });
    let verdict = decide(
        "write",
        &write_findings("t.py", &marked, None),
        None,
        Some(&log),
    );
    assert!(matches!(verdict, Verdict::Bypassed(_)), "{verdict:?}");
    let recorded = std::fs::read_to_string(&log).expect("bypass recorded");
    assert!(recorded.contains("revoked test fixture"));
    assert!(recorded.contains("\"line\":1"));
    assert!(
        !recorded.contains(&key),
        "the audit log must never hold the value"
    );

    let bare = json!({ "content": format!("K = \"{key}\"  # {ALLOW_MARKER}") });
    let verdict = decide(
        "write",
        &write_findings("t.py", &bare, None),
        None,
        Some(&log),
    );
    assert!(
        matches!(verdict, Verdict::Block(_)),
        "a reasonless bypass must not count: {verdict:?}"
    );
}

#[test]
fn a_bypass_that_cannot_be_recorded_is_refused() {
    let marked =
        json!({ "content": format!("K = \"{}\" # {ALLOW_MARKER} fixture", fake_stripe()) });
    let verdict = decide("write", &write_findings("t.py", &marked, None), None, None);
    let Verdict::Block(reason) = verdict else {
        panic!("an unrecordable bypass must block, got {verdict:?}");
    };
    assert!(reason.contains("could not be recorded"), "{reason}");
}

#[test]
fn parse_push_reads_what_a_push_sends() {
    let words = |s: &str| shlex::split(s).expect("words");
    assert_eq!(parse_push(&words("git status")), None);
    assert_eq!(parse_push(&words("echo git push")), None);
    assert_eq!(
        parse_push(&words("git push")).expect("push").revs,
        vec!["HEAD"]
    );
    assert_eq!(
        parse_push(&words("git push -u origin feat/x"))
            .expect("push")
            .revs,
        vec!["feat/x"]
    );
    assert_eq!(
        parse_push(&words("git push origin +HEAD:refs/heads/main"))
            .expect("push")
            .revs,
        vec!["HEAD"]
    );
    assert_eq!(
        parse_push(&words("git push origin --delete old"))
            .expect("push")
            .revs,
        Vec::<String>::new()
    );
    assert_eq!(
        parse_push(&words("git push --all origin"))
            .expect("push")
            .revs,
        vec!["--branches"]
    );
    let with_dir = parse_push(&words("git -C sub/repo push origin main")).expect("push");
    assert_eq!(with_dir.dirs, vec!["sub/repo"]);
    let bypassed = parse_push(&words("ESTELLE_ALLOW_SECRET='revoked key' git push")).expect("push");
    assert_eq!(bypassed.bypass.as_deref(), Some("revoked key"));
}

#[test]
fn every_push_in_a_compound_command_is_found() {
    let pushes = parse_pushes("cargo test && git push origin a; echo 'x;y' | cat").expect("parsed");
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].revs, vec!["a"]);
    assert!(parse_pushes("git push 'unterminated").is_err());
}

#[test]
fn parse_patch_numbers_added_lines_by_their_file_line() {
    let patch = "estelle-commit 1234567890abcdef\n\ndiff --git a/x.py b/x.py\n--- a/x.py\n+++ b/x.py\n@@ -3,0 +4,2 @@\n+one\n+two\n@@ -10 +12 @@\n-old\n+new\ndiff --git a/gone b/gone\n--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";
    let lines = parse_patch(patch);
    let got: Vec<(String, usize, String)> = lines
        .into_iter()
        .map(|l| (l.path, l.line, l.text))
        .collect();
    assert_eq!(
        got,
        vec![
            ("x.py".to_string(), 4, "one".to_string()),
            ("x.py".to_string(), 5, "two".to_string()),
            ("x.py".to_string(), 12, "new".to_string()),
        ]
    );
}

// ─── the push door against a real git repository ─────────────────────────────────────────────

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

/// A clone with one pushed commit, so `--not --remotes` has something to subtract.
fn repo_with_remote() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("tmp");
    let remote = root.path().join("remote.git");
    let work = root.path().join("work");
    std::fs::create_dir_all(&work).expect("mkdir");
    run(
        root.path(),
        &["init", "--bare", "-q", remote.to_str().expect("utf8")],
    );
    run(&work, &["init", "-q"]);
    std::fs::write(work.join("README.md"), "hello\n").expect("write");
    run(&work, &["add", "README.md"]);
    run(&work, &["commit", "-q", "-m", "base"]);
    run(
        &work,
        &["remote", "add", "origin", remote.to_str().expect("utf8")],
    );
    run(&work, &["push", "-q", "origin", "HEAD:main"]);
    run(&work, &["fetch", "-q", "origin"]);
    (root, work)
}

fn commit(work: &Path, path: &str, body: &str, message: &str) {
    std::fs::write(work.join(path), body).expect("write");
    run(work, &["add", path]);
    run(work, &["commit", "-q", "-m", message]);
}

#[test]
fn a_push_whose_outgoing_commits_add_a_live_shaped_key_is_blocked() {
    let (_root, work) = repo_with_remote();
    commit(
        &work,
        "app.py",
        &format!("x = 1\nKEY = \"{}\"\n", fake_stripe()),
        "add key",
    );
    let verdict = check_push("git push origin main", &work, None);
    let Verdict::Block(reason) = verdict else {
        panic!("expected a block, got {verdict:?}");
    };
    assert!(
        reason.contains("app.py:2 (a Stripe live key, commit "),
        "{reason}"
    );
    assert!(!reason.contains("FAKEFAKE"), "{reason}");
}

/// The case a diff of endpoints misses: the key is added, then deleted. Both commits travel.
#[test]
fn a_key_added_then_deleted_before_the_push_is_still_blocked() {
    let (_root, work) = repo_with_remote();
    commit(
        &work,
        "app.py",
        &format!("KEY = \"{}\"\n", fake_github()),
        "add key",
    );
    commit(&work, "app.py", "KEY = os.environ['KEY']\n", "remove key");
    assert!(matches!(
        check_push("git push", &work, None),
        Verdict::Block(_)
    ));
}

#[test]
fn a_clean_push_and_an_already_pushed_key_are_clear() {
    let (_root, work) = repo_with_remote();
    commit(
        &work,
        "notes.md",
        &known_false_positives().join("\n"),
        "fp lines",
    );
    assert_eq!(
        check_push("git push origin main", &work, None),
        Verdict::Clear
    );
    // Once the commit is on the remote it is no longer outgoing.
    commit(
        &work,
        "k.py",
        &format!("K = \"{}\"\n", fake_stripe()),
        "key",
    );
    run(&work, &["push", "-q", "origin", "HEAD:main"]);
    run(&work, &["fetch", "-q", "origin"]);
    assert_eq!(check_push("git push", &work, None), Verdict::Clear);
}

#[test]
fn a_push_bypass_with_a_reason_is_recorded_and_allowed() {
    let (root, work) = repo_with_remote();
    let log = root.path().join("log.jsonl");
    commit(
        &work,
        "k.py",
        &format!("K = \"{}\"\n", fake_stripe()),
        "key",
    );
    let verdict = check_push(
        "ESTELLE_ALLOW_SECRET=\"revoked in the dashboard\" git push",
        &work,
        Some(&log),
    );
    assert!(matches!(verdict, Verdict::Bypassed(_)), "{verdict:?}");
    let recorded = std::fs::read_to_string(&log).expect("recorded");
    assert!(
        recorded.contains("revoked in the dashboard") && recorded.contains("\"surface\":\"push\"")
    );
    let empty = check_push("ESTELLE_ALLOW_SECRET= git push", &work, Some(&log));
    assert!(
        matches!(empty, Verdict::Block(ref r) if r.contains("no reason")),
        "{empty:?}"
    );
}

#[test]
fn an_unreadable_push_is_not_scanned_never_clear() {
    let (_root, work) = repo_with_remote();
    let verdict = check_push("git push origin no-such-branch", &work, None);
    assert!(matches!(verdict, Verdict::NotScanned(_)), "{verdict:?}");
    assert_eq!(check_push("git status", &work, None), Verdict::Clear);
}
