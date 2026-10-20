#![allow(clippy::expect_used)]

use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn real_session_start_hook_rejects_malformed_stdin_with_an_attributable_exit() {
    let binary = codex_utils_cargo_bin::cargo_bin("estelle").expect("locate built Estelle binary");
    let root = tempfile::tempdir().expect("isolated hook cwd");
    let mut child = Command::new(binary)
        .env_clear()
        .env("HOME", root.path())
        .env("PATH", "/usr/bin:/bin")
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
    assert!(
        output.stdout.is_empty(),
        "failed hook wrote control stdout: {combined}"
    );
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        !error.trim().is_empty(),
        "the host needs a nonempty stderr diagnostic"
    );
    assert!(error.contains("event=SessionStart"), "{error}");
    assert!(combined.contains("mode=welcome"), "{combined}");
    assert!(combined.contains("branch=input-json"), "{combined}");
    assert!(
        combined.contains("needed=valid JSON hook payload on stdin"),
        "{combined}"
    );
}

fn isolated_command(root: &std::path::Path) -> Command {
    let binary = codex_utils_cargo_bin::cargo_bin("estelle").expect("locate built Estelle binary");
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("HOME", root)
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(root);
    command
}
fn hook(
    root: &std::path::Path,
    mode: &str,
    policy: &str,
    shell: Option<&str>,
    payload: serde_json::Value,
) -> serde_json::Value {
    let mut command = isolated_command(root);
    command
        .args(["--repo", "acme/fixture", "hook", mode])
        .env("ESTELLE_HOOK_POLICY", policy);
    if let Some(value) = shell {
        command.env("ESTELLE_HOOK_SHELL_POLICY", value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated hook process");
    child
        .stdin
        .take()
        .expect("hook stdin is piped")
        .write_all(payload.to_string().as_bytes())
        .expect("write hook payload");
    let output = child.wait_with_output().expect("collect hook output");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("exactly one JSON control envelope")
}
#[test]
fn actual_required_process_denies_unpreviewable_edit_but_disabled_discloses_opt_out() {
    let root = tempfile::tempdir().unwrap();
    let payload = serde_json::json!({"hook_event_name":"PreToolUse","tool_name":"apply_patch","cwd":root.path(),"tool_input":{"patch":"wrong field, not command"}});
    let denied = hook(root.path(), "ground", "required", None, payload.clone());
    assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        !denied["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert!(denied.get("continue").is_none());
    let disabled = hook(root.path(), "ground", "disabled", None, payload.clone());
    assert!(
        disabled["systemMessage"]
            .as_str()
            .unwrap()
            .contains("not checked or certified")
    );
    assert!(
        disabled
            .pointer("/hookSpecificOutput/permissionDecision")
            .is_none()
    );
    let advisory = hook(root.path(), "ground", "advisory", None, payload);
    assert!(
        advisory
            .pointer("/hookSpecificOutput/permissionDecision")
            .is_none()
    );
    assert!(advisory.to_string().contains("not blocked"));
}
#[test]
fn actual_process_shell_requires_separate_choice_and_observe_never_claims_a_pass() {
    let root = tempfile::tempdir().unwrap();
    let payload = serde_json::json!({"hook_event_name":"PreToolUse","tool_name":"exec_command","cwd":root.path(),"tool_input":{"cmd":"python -c 'open(\"a.py\",\"w\").write(\"invented()\")'"}});
    for shell in [None, Some("pause")] {
        let denied = hook(root.path(), "guard", "required", shell, payload.clone());
        assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
    }
    let observed = hook(root.path(), "guard", "required", Some("observe"), payload);
    assert!(
        observed["systemMessage"]
            .as_str()
            .unwrap()
            .contains("UNVERIFIED")
    );
    assert!(
        observed
            .pointer("/hookSpecificOutput/permissionDecision")
            .is_none()
    );
    assert!(!root.path().join("a.py").exists());
}
#[test]
fn actual_process_whole_file_deletion_has_scoped_no_obligation_not_fake_approval() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("gone.py"), "def gone():\n    pass\n").unwrap();
    let output = hook(
        root.path(),
        "ground",
        "required",
        None,
        serde_json::json!({"hook_event_name":"PreToolUse","tool_name":"apply_patch","cwd":root.path(),"tool_input":{"command":"*** Begin Patch\n*** Delete File: gone.py\n*** End Patch"}}),
    );
    assert!(
        output["systemMessage"]
            .as_str()
            .unwrap()
            .contains("not applicable")
    );
    assert!(
        output
            .pointer("/hookSpecificOutput/permissionDecision")
            .is_none()
    );
    assert!(root.path().join("gone.py").exists());
}
#[test]
fn required_install_without_shell_choice_is_refused_before_settings_mutation() {
    let root = tempfile::tempdir().unwrap();
    let output = isolated_command(root.path())
        .args(["install-hooks", "--policy", "required"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("explicit shell-policy choice"));
    for file in [
        ".claude/settings.json",
        ".codex/hooks.json",
        ".estelle/hook-policy.json",
    ] {
        assert!(!root.path().join(file).exists());
    }
}
#[test]
fn explicit_install_preserves_customer_settings_and_reports_native_trust_uncertainty() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".claude")).unwrap();
    let existing = serde_json::json!({"customerSetting":"keep","hooks":{"Stop":[{"hooks":[{"type":"command","command":"customer-check"}]}]}});
    std::fs::write(
        root.path().join(".claude/settings.json"),
        existing.to_string(),
    )
    .unwrap();
    let output = isolated_command(root.path())
        .args([
            "install-hooks",
            "--policy",
            "required",
            "--shell-policy",
            "observe",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("unverified, not enforced"));
    assert!(text.contains("has not verified the host's active runner or trust state"));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(saved["customerSetting"], "keep");
    assert_eq!(saved["hooks"]["Stop"][0], existing["hooks"]["Stop"][0]);
    let policy: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join(".estelle/hook-policy.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        policy,
        serde_json::json!({"grounding":"required","shell":"observe"})
    );
    for file in [".claude/settings.json", ".codex/hooks.json"] {
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.path().join(file)).unwrap()).unwrap();
        let rows = saved["hooks"]["Stop"].as_array().unwrap();
        let reviews: Vec<_> = rows
            .iter()
            .flat_map(|row| row["hooks"].as_array().unwrap())
            .filter(|row| row["command"].as_str().unwrap().contains(" hook review"))
            .collect();
        assert_eq!(reviews.len(), 1);
        assert_ne!(reviews[0].get("async"), Some(&serde_json::json!(true)));
    }
    let again = isolated_command(root.path())
        .arg("install-hooks")
        .output()
        .unwrap();
    assert!(again.status.success());
    let preserved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join(".estelle/hook-policy.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(preserved, policy);
}
