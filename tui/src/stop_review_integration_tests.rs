//! Loopback-only transport fixtures. No ambient credential or provider endpoint is consulted.
use super::*;
use crate::hook_policy::Policy;
use sha2::Digest;
use sha2::Sha256;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::{body_partial_json, method, path};

fn api(server: &MockServer) -> Api {
    let key = estelle_client::ApiKey::new("test-key").expect("fixture key");
    Api {
        client: Client::new(
            &format!("{}/", server.uri()),
            key.clone(),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("loopback client"),
        api_key: key,
        cancel: CancellationToken::new(),
    }
}

fn payload(root: &Path, answer: &str, active: bool) -> HookPayload {
    serde_json::from_value(
        json!({"session_id":"fixture-session", "turn_id":"fixture-turn", "cwd":root,
        "hook_event_name":"Stop", "last_assistant_message":answer, "stop_hook_active":active}),
    )
    .expect("payload")
}

fn response(answer: &str, disposition: &str) -> Value {
    let contradicted = disposition == "block";
    json!({"answer_review":{
        "schema_version":1, "disposition":disposition, "answer_sha256":format!("{:x}", Sha256::digest(answer.as_bytes())),
        "repo":"acme/repo", "coverage_complete":true, "material_claims":1, "checked_claims":1,
        "evidence_revision":"fixture-snapshot", "indexed_head":"", "currency_basis":"checkout_currency_not_established",
        "checked_scope":"literal_active_decisions_and_simple_indexed_repository_statements", "reason":"Fixture review.",
        "findings":[{"claim":answer, "verdict":if contradicted {"contradicted"} else {"supported"},
            "reason":"The active decision is explicit.", "evidence":["decision:fixture"], "repair":"Use the active decision exactly.", "checked_by":"literal_decision"}]
    }})
}

fn output(lines: &[String]) -> Value {
    assert_eq!(lines.len(), 1, "one JSON envelope per Stop");
    serde_json::from_str(&lines[0]).expect("JSON")
}

#[tokio::test]
async fn request_uses_existing_scoped_verify_and_exact_answer_bytes() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    let answer = "  Exact café answer.\n";
    Mock::given(method("POST")).and(path("/verify"))
        .and(body_partial_json(json!({"verify_mode":"answer","answer":answer,"repo":"acme/repo","working_tree_clean":false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response(answer,"pass"))).expect(1).mount(&server).await;
    let credential = api(&server);
    let lines = stop_review_hook_with(
        &payload(root.path(), answer, false),
        &Repo::new("acme/repo").expect("repo"),
        root.path(),
        Some(state.path()),
        Duration::from_secs(1),
        Policy::Required,
        || Ok(credential),
    )
    .await;
    let output = output(&lines);
    assert!(output.get("decision").is_none());
    assert!(
        output["systemMessage"]
            .as_str()
            .expect("message")
            .contains("checked 1")
    );
}

#[tokio::test]
async fn every_changed_repair_is_sent_and_third_failure_stops_unresolved() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verify"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("request");
            ResponseTemplate::new(200)
                .set_body_json(response(body["answer"].as_str().expect("answer"), "block"))
        })
        .expect(3)
        .mount(&server)
        .await;
    for (index, answer) in ["original", "repair one", "repair two"].iter().enumerate() {
        let credential = api(&server);
        let lines = stop_review_hook_with(
            &payload(root.path(), answer, index > 0),
            &Repo::new("acme/repo").expect("repo"),
            root.path(),
            Some(state.path()),
            Duration::from_secs(1),
            Policy::Required,
            || Ok(credential),
        )
        .await;
        let output = output(&lines);
        if index < 2 {
            assert_eq!(output["decision"], "block");
            assert!(
                output["reason"]
                    .as_str()
                    .expect("reason")
                    .contains(&format!("Repair attempt {}/2", index + 1))
            );
        } else {
            assert_eq!(output["continue"], false);
            assert!(
                output["stopReason"]
                    .as_str()
                    .expect("reason")
                    .contains("UNRESOLVED")
            );
            assert!(output.get("decision").is_none());
        }
    }
}

#[tokio::test]
async fn same_artifact_active_retry_still_makes_a_second_request() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response("same", "block")))
        .expect(2)
        .mount(&server)
        .await;
    for active in [false, true] {
        let credential = api(&server);
        let lines = stop_review_hook_with(
            &payload(root.path(), "same", active),
            &Repo::new("acme/repo").expect("repo"),
            root.path(),
            Some(state.path()),
            Duration::from_secs(1),
            Policy::Required,
            || Ok(credential),
        )
        .await;
        if active {
            assert_eq!(output(&lines)["continue"], false);
        } else {
            assert_eq!(output(&lines)["decision"], "block");
        }
    }
}

#[tokio::test]
async fn real_transport_timeout_is_disclosed_not_pass_or_repair() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verify"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(response("answer", "pass"))
                .set_delay(Duration::from_secs(2)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let credential = api(&server);
    let started = Instant::now();
    let lines = stop_review_hook_with(
        &payload(root.path(), "answer", false),
        &Repo::new("acme/repo").expect("repo"),
        root.path(),
        Some(state.path()),
        Duration::from_millis(30),
        Policy::Required,
        || Ok(credential),
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(1));
    let output = output(&lines);
    assert!(output.get("decision").is_none());
    assert!(
        output["systemMessage"]
            .as_str()
            .expect("message")
            .contains("deadline expired")
    );
    assert!(
        output["systemMessage"]
            .as_str()
            .expect("message")
            .contains("UNVERIFIED")
    );
}

#[tokio::test]
async fn legacy_or_malformed_server_response_never_certifies() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"grounded":true})))
        .expect(1)
        .mount(&server)
        .await;
    let credential = api(&server);
    let lines = stop_review_hook_with(
        &payload(root.path(), "answer", false),
        &Repo::new("acme/repo").expect("repo"),
        root.path(),
        Some(state.path()),
        Duration::from_secs(1),
        Policy::Required,
        || Ok(credential),
    )
    .await;
    assert!(
        output(&lines)["systemMessage"]
            .as_str()
            .expect("message")
            .contains("UNVERIFIED")
    );
    assert!(output(&lines).get("decision").is_none());
}

#[tokio::test]
async fn advisory_and_legacy_contradictions_are_visible_but_never_grant_repairs() {
    for policy in [Policy::Advisory, Policy::LegacyFlaggedOnly] {
        let root = tempfile::tempdir().expect("root");
        let state = tempfile::tempdir().expect("state");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response("answer", "block")))
            .expect(1)
            .mount(&server)
            .await;
        let credential = api(&server);
        let lines = stop_review_hook_with(
            &payload(root.path(), "answer", false),
            &Repo::new("acme/repo").expect("repo"),
            root.path(),
            Some(state.path()),
            Duration::from_secs(1),
            policy,
            || Ok(credential),
        )
        .await;
        let output = output(&lines);
        assert!(output.get("decision").is_none());
        assert!(
            output["systemMessage"]
                .as_str()
                .expect("message")
                .contains("REPAIR FINDINGS")
        );
        for entry in fs::read_dir(state.path()).expect("state files") {
            let state: Value =
                serde_json::from_slice(&fs::read(entry.expect("entry").path()).expect("state"))
                    .expect("JSON");
            assert_eq!(state["repair_grants"], 0);
        }
    }
}

#[tokio::test]
async fn disabled_and_missing_input_do_not_resolve_a_credential() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let repo = Repo::new("acme/repo").expect("repo");
    let lines = stop_review_hook_with(
        &payload(root.path(), "answer", false),
        &repo,
        root.path(),
        Some(state.path()),
        Duration::from_secs(1),
        Policy::Disabled,
        || panic!("disabled review cannot read credentials"),
    )
    .await;
    assert!(
        output(&lines)["systemMessage"]
            .as_str()
            .expect("message")
            .contains("disabled")
    );
    let lines = stop_review_hook_with(
        &payload(root.path(), "", false),
        &repo,
        root.path(),
        Some(state.path()),
        Duration::from_secs(1),
        Policy::Required,
        || panic!("empty extraction cannot reach credentials"),
    )
    .await;
    assert!(
        output(&lines)["systemMessage"]
            .as_str()
            .expect("message")
            .contains("UNVERIFIED")
    );
    assert_eq!(fs::read_dir(state.path()).expect("state files").count(), 0);
}

#[tokio::test]
async fn required_and_advisory_credential_failures_disclose_without_leaking_or_granting_repairs() {
    for policy in [Policy::Required, Policy::Advisory] {
        let root = tempfile::tempdir().expect("root");
        let state = tempfile::tempdir().expect("state");
        let lines = stop_review_hook_with(
            &payload(root.path(), "answer", false),
            &Repo::new("acme/repo").expect("repo"),
            root.path(),
            Some(state.path()),
            Duration::from_secs(1),
            policy,
            || Err("SENSITIVE_FIXTURE credential path and token".to_string()),
        )
        .await;
        let output = output(&lines);
        let message = output["systemMessage"].as_str().expect("message");
        assert!(message.contains("UNVERIFIED"));
        assert!(message.contains("no usable Estelle credential"));
        assert!(!message.contains("SENSITIVE_FIXTURE"));
        assert!(output.get("decision").is_none());
        assert!(output.get("continue").is_none());
        assert_eq!(fs::read_dir(state.path()).expect("state files").count(), 0);
    }
}

#[tokio::test]
async fn required_and_advisory_http_auth_and_bad_response_failures_are_not_pass_or_repairs() {
    for policy in [Policy::Required, Policy::Advisory] {
        for status in [401, 403, 429, 500, 503, 200] {
            let root = tempfile::tempdir().expect("root");
            let state = tempfile::tempdir().expect("state");
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/verify"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_string("SENSITIVE_FIXTURE invalid JSON or raw server error"),
                )
                .expect(1)
                .mount(&server)
                .await;
            let credential = api(&server);
            let lines = stop_review_hook_with(
                &payload(root.path(), "answer", false),
                &Repo::new("acme/repo").expect("repo"),
                root.path(),
                Some(state.path()),
                Duration::from_secs(1),
                policy,
                || Ok(credential),
            )
            .await;
            let output = output(&lines);
            let message = output["systemMessage"].as_str().expect("message");
            assert!(message.contains("UNVERIFIED"), "{policy:?} {status}");
            assert!(!message.contains("SENSITIVE_FIXTURE"));
            assert!(output.get("decision").is_none());
            assert!(output.get("continue").is_none());
            assert!(!message.contains("checked 1"));
            for entry in fs::read_dir(state.path()).expect("state files") {
                let state: Value =
                    serde_json::from_slice(&fs::read(entry.expect("entry").path()).expect("state"))
                        .expect("JSON");
                assert_eq!(state["repair_grants"], 0);
            }
            let requests = server.received_requests().await.expect("requests");
            let body: Value = serde_json::from_slice(&requests[0].body).expect("body");
            assert!(body.get("semantic_review").is_none());
            assert!(body.get("semantic_funding").is_none());
        }
    }
}

#[test]
fn snapshot_requires_valid_head_and_no_status_entries() {
    let head = "a".repeat(40);
    let clean = format!("# branch.oid {head}\0# branch.head main\0");
    assert_eq!(
        parse_stop_checkout_status(clean.as_bytes()),
        (Some(head.clone()), true)
    );
    assert_eq!(
        parse_stop_checkout_status(format!("{clean}? new.py\0").as_bytes()),
        (Some(head), false)
    );
    for invalid in [
        "",
        "# branch.oid (initial)\0",
        "# branch.oid fake\0",
        "unknown",
    ] {
        assert!(!parse_stop_checkout_status(invalid.as_bytes()).1);
    }
}

#[test]
fn review_is_synchronous_and_precedes_checkpoint_on_all_install_doors() {
    let review = HOOK_TABLE
        .iter()
        .position(|row| row.event == "Stop" && row.mode == "review")
        .expect("review row");
    let checkpoint = HOOK_TABLE
        .iter()
        .position(|row| row.event == "Stop" && row.mode == "checkpoint")
        .expect("checkpoint row");
    assert!(review < checkpoint);
    let row = &HOOK_TABLE[review];
    assert!(!row.claude_async && !row.plugin_async);
    assert!(
        row.timeout > STOP_REVIEW_REQUEST_BUDGET.as_secs() + 2 * STOP_REVIEW_GIT_BUDGET.as_secs()
    );
    for host in [HookHost::Claude, HookHost::Codex] {
        let handlers = estelle_hook_groups(host, "estelle");
        let review = handlers
            .iter()
            .find(|(event, group)| {
                event == "Stop"
                    && group["hooks"][0]["command"]
                        .as_str()
                        .is_some_and(|cmd| cmd.contains("hook review"))
            })
            .expect("installed review");
        assert!(review.1["hooks"][0].get("async").is_none());
    }
}

fn clean_fixture_repo(root: &Path) {
    assert!(
        scoped_git(root)
            .args(["init", "--quiet"])
            .output()
            .expect("git init")
            .status
            .success()
    );
    fs::write(root.join("fixture.txt"), "fixture\n").expect("fixture file");
    assert!(
        scoped_git(root)
            .args(["add", "fixture.txt"])
            .output()
            .expect("git add")
            .status
            .success()
    );
    assert!(
        scoped_git(root)
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture"
            ])
            .output()
            .expect("fixture commit")
            .status
            .success()
    );
}

#[tokio::test]
async fn actual_sanitized_status_distinguishes_clean_dirty_and_unknown_checkouts() {
    let root = tempfile::tempdir().expect("root");
    assert_eq!(stop_checkout_snapshot(root.path()).await, (None, false));
    clean_fixture_repo(root.path());
    let clean = stop_checkout_snapshot(root.path()).await;
    assert!(clean.0.is_some() && clean.1);
    fs::write(root.path().join("new.py"), "new_symbol = 1\n").expect("dirty source");
    let dirty = stop_checkout_snapshot(root.path()).await;
    assert_eq!(dirty.0, clean.0);
    assert!(!dirty.1);
}

#[tokio::test]
async fn checkout_mutation_during_request_invalidates_a_positive_result() {
    let root = tempfile::tempdir().expect("root");
    clean_fixture_repo(root.path());
    let state = tempfile::tempdir().expect("state");
    let server = MockServer::start().await;
    let changed_file = root.path().join("changed-during-review.py");
    Mock::given(method("POST"))
        .and(path("/verify"))
        .and(body_partial_json(json!({"working_tree_clean":true})))
        .respond_with(move |_: &wiremock::Request| {
            fs::write(&changed_file, "changed = True\n").expect("concurrent edit fixture");
            ResponseTemplate::new(200).set_body_json(response("answer", "pass"))
        })
        .expect(1)
        .mount(&server)
        .await;
    let credential = api(&server);
    let lines = stop_review_hook_with(
        &payload(root.path(), "answer", false),
        &Repo::new("acme/repo").expect("repo"),
        root.path(),
        Some(state.path()),
        Duration::from_secs(1),
        Policy::Required,
        || Ok(credential),
    )
    .await;
    let output = output(&lines);
    assert!(output.get("decision").is_none());
    assert!(
        output["systemMessage"]
            .as_str()
            .expect("message")
            .contains("checkout changed")
    );
    assert!(
        output["systemMessage"]
            .as_str()
            .expect("message")
            .contains("UNVERIFIED")
    );
}
