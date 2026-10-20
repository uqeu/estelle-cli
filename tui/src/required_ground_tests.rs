//! Synthetic, loopback-only tests of the actual required hook call path.
use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let result = scoped_git(dir.path()).args(args).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    run(&["init", "-q"]);
    fs::write(dir.path().join("a.py"), "def real():\n    return 1\n").unwrap();
    fs::write(dir.path().join("gone.py"), "def removed():\n    return 1\n").unwrap();
    run(&["add", "."]);
    run(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    dir
}
fn api(server: &MockServer) -> Api {
    let key = estelle_client::ApiKey::new("test-key").unwrap();
    Api {
        client: Client::new(
            &format!("{}/", server.uri()),
            key.clone(),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .unwrap(),
        api_key: key,
        cancel: CancellationToken::new(),
    }
}
fn payload(root: &Path, patch: &str) -> HookPayload {
    serde_json::from_value(json!({"hook_event_name":"PreToolUse","tool_name":"apply_patch","cwd":root,"tool_input":{"command":patch}})).unwrap()
}
fn clean(request: &wiremock::Request) -> Value {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    let diff = body["diff"].as_str().unwrap();
    let name = diff
        .lines()
        .find_map(|line| line.strip_prefix("+++ b/"))
        .unwrap();
    let count = body["sources"].as_object().unwrap().len();
    let deleted = body["working_tree"]
        .get("deleted")
        .cloned()
        .unwrap_or(json!([]));
    let deleted_count = deleted.as_array().unwrap().len();
    json!({"repo":body["repo"],"scope":format!("repo:{}",body["repo"].as_str().unwrap()),"merge":true,"verified":true,"state":"verified-clean","outcome":"verified","verdict":"clean","blockers":[],
        "currency":{"agrees":true,"certifiable":true,"indexed_head":body["head"],"asked_head":body["head"]},
        "coverage":{"paths":[name],"inert":[],"out_of_domain":[],"needs_verdict":[name]},"counts":{"evidence":1,"api_checks":1,"api_verdicts":1,"symbols_seen":1},
        "working_tree":{"schema_version":2,"deleted":deleted,"complete":true,"files":count,"declared":count+deleted_count,"dropped":[],"undelivered":[]}})
}
const ADD: &str =
    "*** Begin Patch\n*** Add File: b.py\n+def added():\n+    return real()\n*** End Patch";
#[tokio::test]
async fn canonical_patch_calls_gate_not_verify_and_preserves_candidate_bytes() {
    let root = checkout();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gate"))
        .respond_with(|r: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body["sources"]["b.py"], "def added():\n    return real()\n");
            assert_eq!(body["working_tree"]["sources"], body["sources"]);
            assert_eq!(body["require_verified"], true);
            ResponseTemplate::new(200).set_body_json(clean(r))
        })
        .expect(1)
        .mount(&server)
        .await;
    let result = required_ground_with(
        &payload(root.path(), ADD),
        &Repo::new("acme/repo").unwrap(),
        root.path(),
        Duration::from_secs(1),
        || Ok(api(&server)),
    )
    .await
    .unwrap();
    assert!(result.contains("admitted 1"));
    assert!(!root.path().join("b.py").exists());
}
#[tokio::test]
async fn mixed_rename_delete_and_introductions_require_acknowledged_tombstones() {
    let root = checkout();
    let server = MockServer::start().await;
    let patch = "*** Begin Patch\n*** Update File: a.py\n*** Move to: renamed.py\n@@\n-    return 1\n+    return 2\n*** Delete File: gone.py\n*** Add File: b.py\n+def b():\n+    return 2\n*** End Patch";
    Mock::given(method("POST"))
        .and(path("/gate"))
        .respond_with(|r: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body["working_tree"]["deleted"], json!(["a.py", "gone.py"]));
            assert_eq!(
                body["working_tree"]["changed"],
                json!(["a.py", "b.py", "gone.py", "renamed.py"])
            );
            assert!(body["sources"].get("a.py").is_none());
            assert!(body["sources"].get("gone.py").is_none());
            assert_eq!(body["sources"]["renamed.py"], "def real():\n    return 2\n");
            ResponseTemplate::new(200).set_body_json(clean(r))
        })
        .expect(2)
        .mount(&server)
        .await;
    let result = required_ground_with(
        &payload(root.path(), patch),
        &Repo::new("acme/repo").unwrap(),
        root.path(),
        Duration::from_secs(1),
        || Ok(api(&server)),
    )
    .await
    .unwrap();
    assert!(result.contains("admitted 2"));
    assert!(root.path().join("a.py").exists());
    assert!(root.path().join("gone.py").exists());
    assert!(!root.path().join("renamed.py").exists());
}
#[tokio::test]
async fn multifile_new_definitions_keep_the_whole_candidate_overlay() {
    let root = checkout();
    let server = MockServer::start().await;
    let patch = "*** Begin Patch\n*** Add File: b.py\n+def b():\n+    return 2\n*** Add File: c.py\n+from b import b\n+result = b()\n*** End Patch";
    Mock::given(method("POST"))
        .and(path("/gate"))
        .respond_with(|r: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body["sources"]["b.py"], "def b():\n    return 2\n");
            assert_eq!(body["sources"]["c.py"], "from b import b\nresult = b()\n");
            ResponseTemplate::new(200).set_body_json(clean(r))
        })
        .expect(2)
        .mount(&server)
        .await;
    assert!(
        required_ground_with(
            &payload(root.path(), patch),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || Ok(api(&server))
        )
        .await
        .unwrap()
        .contains("admitted 2")
    );
}
#[tokio::test]
async fn pure_deletion_is_explicit_not_applicable_not_fake_checked() {
    let root = checkout();
    let patch = "*** Begin Patch\n*** Delete File: gone.py\n*** End Patch";
    let result = required_ground_with(
        &payload(root.path(), patch),
        &Repo::new("acme/repo").unwrap(),
        root.path(),
        Duration::from_secs(1),
        || panic!("no API request is owed"),
    )
    .await
    .unwrap();
    assert!(result.contains("not applicable"));
    assert!(result.contains("impact and overall correctness were not checked"));
}
#[tokio::test]
async fn timeout_legacy_and_wrong_scope_results_are_valid_host_denials() {
    for case in ["timeout", "legacy", "scope", "blocked"] {
        let root = checkout();
        let server = MockServer::start().await;
        let chosen = case.to_string();
        Mock::given(method("POST"))
            .and(path("/gate"))
            .respond_with(move |r: &wiremock::Request| {
                let mut reply = clean(r);
                if chosen == "legacy" {
                    reply = json!({"merge":true});
                }
                if chosen == "scope" {
                    reply["repo"] = json!("other/repo");
                }
                if chosen == "blocked" {
                    reply["merge"] = json!(false);
                    reply["blockers"] = json!([{"body":"invented API; replace it with real()"}]);
                }
                let response = ResponseTemplate::new(200).set_body_json(reply);
                if chosen == "timeout" {
                    response.set_delay(Duration::from_millis(200))
                } else {
                    response
                }
            })
            .expect(1)
            .mount(&server)
            .await;
        let error = required_ground_with(
            &payload(root.path(), ADD),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_millis(40),
            || Ok(api(&server)),
        )
        .await
        .unwrap_err();
        let envelope: Value = serde_json::from_str(&required_ground_denial(&error)).unwrap();
        assert_eq!(envelope["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(
            !envelope["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert!(envelope.get("continue").is_none());
    }
}
#[tokio::test]
async fn source_changed_during_request_never_uses_old_approval() {
    let root = checkout();
    let server = MockServer::start().await;
    let target = root.path().join("a.py");
    Mock::given(method("POST"))
        .and(path("/gate"))
        .respond_with(move |r: &wiremock::Request| {
            fs::write(&target, "changed while checking\n").unwrap();
            ResponseTemplate::new(200).set_body_json(clean(r))
        })
        .expect(1)
        .mount(&server)
        .await;
    let error = required_ground_with(
        &payload(root.path(), ADD),
        &Repo::new("acme/repo").unwrap(),
        root.path(),
        Duration::from_secs(1),
        || Ok(api(&server)),
    )
    .await
    .unwrap_err();
    assert!(error.contains("checkout changed"));
}
#[tokio::test]
async fn wrong_cwd_and_secret_candidates_pause_before_any_request() {
    let root = checkout();
    let other = tempfile::tempdir().unwrap();
    let mut p = payload(root.path(), ADD);
    p.cwd = other.path().to_str().unwrap().into();
    assert!(
        required_ground_with(
            &p,
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || panic!("scope failure must precede network")
        )
        .await
        .is_err()
    );
    let p = payload(
        root.path(),
        "*** Begin Patch\n*** Add File: .env\n+KEY=private\n*** End Patch",
    );
    assert!(
        required_ground_with(
            &p,
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || panic!("credential paths are never sent")
        )
        .await
        .is_err()
    );
}

#[test]
fn same_bad_edit_through_every_shell_host_is_denied_and_pwd_is_not_certified() {
    for (tool, field) in [
        ("Bash", "command"),
        ("shell_command", "command"),
        ("exec_command", "cmd"),
    ] {
        for (command, denied) in [
            (
                "python -c 'open(\"b.py\",\"w\").write(\"invented()\")'",
                true,
            ),
            ("pwd", false),
        ] {
            let mut input = json!({});
            input[field] = json!(command);
            let p: HookPayload = serde_json::from_value(
                json!({"hook_event_name":"PreToolUse","tool_name":tool,"tool_input":input}),
            )
            .unwrap();
            let out: Value = serde_json::from_str(&required_shell_output(&p)[0]).unwrap();
            assert_eq!(
                out.pointer("/hookSpecificOutput/permissionDecision")
                    .and_then(Value::as_str)
                    == Some("deny"),
                denied,
                "{tool}"
            );
            if !denied {
                assert!(
                    out["systemMessage"]
                        .as_str()
                        .unwrap()
                        .contains("No code was verified")
                );
            }
        }
    }
}

#[tokio::test]
async fn activated_quoted_code_with_or_without_harmless_addition_needs_candidate_review() {
    for extra in ["", "+    pass\n"] {
        let root = checkout();
        fs::write(
            root.path().join("a.py"),
            "def task():\n    \"\"\"\n    ghost_api()\n    \"\"\"\n",
        )
        .unwrap();
        let patch = format!(
            "*** Begin Patch\n*** Update File: a.py\n@@\n def task():\n-    \"\"\"\n     ghost_api()\n-    \"\"\"\n{extra}*** End Patch"
        );
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gate"))
            .respond_with(|r: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&r.body).unwrap();
                let mut report = clean(r);
                if body["diff"].as_str().unwrap().contains("+    ghost_api()") {
                    report["merge"] = json!(false);
                    report["blockers"] = json!([{"body":"ghost_api is not defined"}]);
                }
                ResponseTemplate::new(200).set_body_json(report)
            })
            .expect(if extra.is_empty() { 1 } else { 2 })
            .mount(&server)
            .await;
        let error = required_ground_with(
            &payload(root.path(), &patch),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || Ok(api(&server)),
        )
        .await
        .unwrap_err();
        assert!(error.contains("complete candidate artifact"));
        assert!(error.contains("ghost_api"));
    }
}

#[tokio::test]
async fn deletion_tombstones_cannot_be_silently_ignored_by_legacy_server() {
    for dirty in [false, true] {
        let root = checkout();
        let server = MockServer::start().await;
        if dirty {
            fs::remove_file(root.path().join("gone.py")).unwrap();
        }
        let patch = if dirty {
            "*** Begin Patch\n*** Add File: b.py\n+import gone\n*** End Patch"
        } else {
            "*** Begin Patch\n*** Delete File: gone.py\n*** Add File: b.py\n+import gone\n*** End Patch"
        };
        Mock::given(method("POST"))
            .and(path("/gate"))
            .respond_with(|r: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&r.body).unwrap();
                assert_eq!(body["working_tree"]["deleted"], json!(["gone.py"]));
                assert!(body["sources"].get("gone.py").is_none());
                let mut report = clean(r);
                report["working_tree"]
                    .as_object_mut()
                    .unwrap()
                    .remove("schema_version");
                ResponseTemplate::new(200).set_body_json(report)
            })
            .expect(1)
            .mount(&server)
            .await;
        let error = required_ground_with(
            &payload(root.path(), patch),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || Ok(api(&server)),
        )
        .await
        .unwrap_err();
        assert!(error.contains("tombstones"));
        assert_eq!(root.path().join("gone.py").exists(), !dirty);
        assert!(!root.path().join("b.py").exists());
    }
}
#[tokio::test]
async fn deleted_module_import_findings_deny_without_mutating_checkout() {
    for dirty in [false, true] {
        let root = checkout();
        let server = MockServer::start().await;
        if dirty {
            fs::remove_file(root.path().join("gone.py")).unwrap();
        }
        let patch = if dirty {
            "*** Begin Patch\n*** Add File: b.py\n+import gone\n*** End Patch"
        } else {
            "*** Begin Patch\n*** Delete File: gone.py\n*** Add File: b.py\n+import gone\n*** End Patch"
        };
        Mock::given(method("POST"))
            .and(path("/gate"))
            .respond_with(|r: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&r.body).unwrap();
                assert_eq!(body["working_tree"]["deleted"], json!(["gone.py"]));
                let mut report = clean(r);
                report["merge"] = json!(false);
                report["blockers"] =
                    json!([{"body":"gone module was deleted; remove or correct the import"}]);
                ResponseTemplate::new(200).set_body_json(report)
            })
            .expect(1)
            .mount(&server)
            .await;
        let error = required_ground_with(
            &payload(root.path(), patch),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_secs(1),
            || Ok(api(&server)),
        )
        .await
        .unwrap_err();
        assert!(error.contains("gone module was deleted"));
        assert_eq!(root.path().join("gone.py").exists(), !dirty);
        assert!(!root.path().join("b.py").exists());
    }
}

#[tokio::test]
async fn overload_and_auth_failures_are_distinct_sanitized_denials_without_retry_sleep() {
    for (status, wanted) in [
        (401, "credential"),
        (403, "credential"),
        (429, "capacity"),
        (503, "capacity"),
    ] {
        let root = checkout();
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/gate")).respond_with(
            ResponseTemplate::new(status).insert_header("Retry-After","30").set_body_json(json!({
                "error":{"code":"pool_exhausted","message":"private-canary-token-do-not-reflect"},"wait_s":30
            }))
        ).expect(1).mount(&server).await;
        let started = std::time::Instant::now();
        let error = required_ground_with(
            &payload(root.path(), ADD),
            &Repo::new("acme/repo").unwrap(),
            root.path(),
            Duration::from_millis(250),
            || Ok(api(&server)),
        )
        .await
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(error.contains(wanted), "{status}: {error}");
        assert!(error.contains(&status.to_string()), "{error}");
        assert!(!error.contains("private-canary"));
        assert!(!error.contains("timed out"));
        let out: Value = serde_json::from_str(&required_ground_denial(&error)).unwrap();
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
        if status == 503 {
            assert!(error.contains("not a CLI login failure"));
            assert!(!error.contains("estelle login"));
        }
        assert!(!root.path().join("b.py").exists());
    }
}
#[tokio::test]
async fn missing_cli_auth_is_not_misreported_as_server_overload() {
    let root = checkout();
    let error = required_ground_with(
        &payload(root.path(), ADD),
        &Repo::new("acme/repo").unwrap(),
        root.path(),
        Duration::from_millis(250),
        || Err("private-credential-path".into()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("no usable credential"));
    assert!(error.contains("estelle doctor"));
    assert!(!error.contains("503"));
    assert!(!error.contains("private-credential-path"));
}
