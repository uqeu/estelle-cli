#![allow(clippy::expect_used)]

use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn real_session_start_hook_rejects_malformed_stdin_with_an_attributable_exit() {
    let binary = codex_utils_cargo_bin::cargo_bin("estelle").expect("locate built Estelle binary");
    let root = tempfile::tempdir().expect("isolated hook cwd");
    let mut child = Command::new(binary)
        .args(["hook", "welcome", "--event", "SessionStart"])
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Estelle hook process");

    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(b"{not json")
        .expect("write malformed hook payload");
    let output = child.wait_with_output().expect("wait for Estelle hook");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.status.success(),
        "malformed hook input must not become a successful empty result: {combined}"
    );
    assert!(combined.contains("event=SessionStart"), "{combined}");
    assert!(combined.contains("mode=welcome"), "{combined}");
    assert!(combined.contains("branch=input-json"), "{combined}");
    assert!(
        combined.contains("needed=valid JSON hook payload on stdin"),
        "{combined}"
    );
}
