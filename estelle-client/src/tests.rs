use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

use super::*;

fn test_key() -> ApiKey {
    ApiKey::new("estelle_live_test-only").expect("test key")
}

#[test]
fn endpoint_inventory_is_unique_and_matches_the_server_audit() {
    assert_eq!(API_ENDPOINTS.len(), 82);
    let unique = API_ENDPOINTS
        .iter()
        .map(|spec| spec.path)
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), API_ENDPOINTS.len());
    assert!(!unique.contains("help"));
    assert!(!unique.contains("c"));
    // Removed 2026-08-07: "github/app/callback" (the browser redirect target — correctly never a
    // client call). "checkpoint" was removed the same day as declared-but-never-called; it is
    // reinstated 2026-08-13 WITH its surface — the `checkpoint` hook mode posts the host's own
    // transcript to it on Stop / PreCompact / SessionEnd.
    assert!(!unique.contains("github/app/callback"));
    assert!(!unique.contains("github/callback"));
    let checkpoint = API_ENDPOINTS
        .iter()
        .find(|spec| spec.path == "checkpoint")
        .expect("the checkpoint hook mode posts here");
    assert_eq!(checkpoint.methods, &[HttpMethod::Post]);
    assert!(!checkpoint.requires_repo);
}

#[test]
fn account_github_contract_preserves_unknown_connection_and_absent_gate() {
    assert_eq!(Endpoint::GithubStatus.path(), "github/status");
    assert_eq!(Endpoint::GithubStatus.methods(), &[HttpMethod::Get]);
    assert!(!Endpoint::GithubStatus.requires_repo());
    assert_eq!(Endpoint::ProposedPrs.path(), "prs");
    assert_eq!(Endpoint::ProposedPrs.methods(), &[HttpMethod::Get]);
    assert!(!Endpoint::ProposedPrs.requires_repo());

    let status: GithubStatusResponse = serde_json::from_value(serde_json::json!({
        "connected": null,
        "provider": "github",
        "login": "acme-owner",
        "observed_at": 1785203400.0,
        "absent_reason": "installation store unavailable: RuntimeError"
    }))
    .expect("GitHub status contract");
    assert_eq!(status.connected, None);
    assert_eq!(status.login.as_deref(), Some("acme-owner"));
    assert_eq!(
        status.absent_reason.as_deref(),
        Some("installation store unavailable: RuntimeError")
    );

    let proposed: ProposedPrsResponse = serde_json::from_value(serde_json::json!({
        "prs": [{
            "number": 17,
            "title": "Repair checkout",
            "url": "https://github.com/acme/shop/pull/17",
            "repo": "acme/shop",
            "issue_key": "shop-17",
            "repair_status": "pr",
            "gate": null,
            "gate_absent_reason": "no gate verdict has been recorded for this issue",
            "created_at": "2026-08-17T01:02:03Z",
            "updated_at": "2026-08-17T02:03:04Z"
        }],
        "next_cursor": "opaque-next",
        "has_more": true
    }))
    .expect("proposed PR contract");
    assert_eq!(proposed.prs[0].number, 17);
    assert!(proposed.prs[0].gate.is_none());
    assert_eq!(
        proposed.prs[0].gate_absent_reason.as_deref(),
        Some("no gate verdict has been recorded for this issue")
    );
    assert_eq!(proposed.next_cursor.as_deref(), Some("opaque-next"));
    assert!(proposed.has_more);
}

#[tokio::test]
async fn account_github_client_calls_both_server_owned_read_surfaces() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/github/status"))
        .and(header("authorization", "Bearer estelle_live_test-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "connected": true,
            "provider": "github",
            "login": "acme-owner",
            "observed_at": 1785203400.0,
            "absent_reason": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/prs"))
        .and(query_param("repo", "acme/shop"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "prs": [], "next_cursor": null, "has_more": false
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let repo = Repo::new("acme/shop").expect("repo");
    let cancel = CancellationToken::new();

    let status = client.github_status(&cancel).await.expect("status");
    let proposed = client
        .proposed_prs(&ProposedPrsQuery::first(&repo), &cancel)
        .await
        .expect("proposed PRs");

    assert_eq!(status.connected, Some(true));
    assert!(proposed.prs.is_empty());
}

#[tokio::test]
async fn durable_job_read_preserves_revisioned_work_progress() {
    let server = MockServer::start().await;
    let job_id = "job_0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path(format!("/jobs/{job_id}")))
        .and(header("authorization", "Bearer estelle_live_test-only"))
        .and(header("x-estelle-client-protocol", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "job_id": job_id,
            "state": "running",
            "terminal": false,
            "progress": {
                "revision": 2,
                "work": {
                    "phase": "recall",
                    "phases": {"scope": 0.4, "recall": 1.2},
                    "elapsed_s": 1.6
                }
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let snapshot = client
        .job(job_id, &CancellationToken::new())
        .await
        .expect("caller-bound job snapshot");

    let progress = snapshot.progress.expect("work progress");
    assert_eq!(progress.revision, 2);
    assert_eq!(progress.work.phase, "recall");
    assert_eq!(progress.work.phases.len(), 2);
    assert!(!snapshot.terminal);
}

#[tokio::test]
async fn durable_job_read_rejects_a_path_shaped_locator_before_transport() {
    let server = MockServer::start().await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let error = client
        .job(
            "job_0123456789abcdef01234567/../../account",
            &CancellationToken::new(),
        )
        .await
        .expect_err("path-shaped job id must be refused");

    assert!(matches!(error, Error::InvalidJobId));
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}

#[test]
fn orchestra_live_endpoints_match_the_server_contract() {
    assert_eq!(Endpoint::OrchestraRun.path(), "orchestra/run");
    assert_eq!(Endpoint::OrchestraRun.methods(), &[HttpMethod::Post]);
    assert!(Endpoint::OrchestraRun.requires_repo());
    assert_eq!(Endpoint::OrchestraStatus.path(), "orchestra/status");
    assert_eq!(Endpoint::OrchestraStatus.methods(), &[HttpMethod::Get]);
    assert!(Endpoint::OrchestraStatus.requires_repo());
}

#[test]
fn agent_health_contract_preserves_unknown_counts_and_server_reported_states() {
    assert_eq!(Endpoint::AgentHealth.path(), "agent/health");
    assert_eq!(Endpoint::AgentHealth.methods(), &[HttpMethod::Get]);
    assert!(!Endpoint::AgentHealth.requires_repo());

    let response: AgentHealthResponse = serde_json::from_value(serde_json::json!({
        "enabled": true,
        "enabled_absent_reason": null,
        "observed_at": 1785203400.0,
        "stale_after_s": 120,
        "counts": {"reporting": 7, "degraded": 1, "silent": null},
        "agents": [{
            "id": "checkout-agent",
            "state": "degraded",
            "state_absent_reason": null,
            "events": 19,
            "last_seen": 1785203370.0,
            "current_signal": "tool timeout"
        }]
    }))
    .expect("agent health contract");

    assert_eq!(response.enabled, Some(true));
    let counts = response.counts.expect("measured counts");
    assert_eq!(counts.reporting, Some(7));
    assert_eq!(counts.degraded, Some(1));
    assert_eq!(counts.silent, None);
    assert_eq!(response.agents[0].state, AgentHealthState::Degraded);
    assert_eq!(response.agents[0].events, Some(19));

    let unknown: AgentHealthResponse = serde_json::from_value(serde_json::json!({
        "enabled": null,
        "enabled_absent_reason": "event store unavailable",
        "counts": null,
        "agents": []
    }))
    .expect("unknown health contract");
    assert_eq!(unknown.enabled, None);
    assert!(unknown.counts.is_none());
    assert_eq!(
        unknown.enabled_absent_reason.as_deref(),
        Some("event store unavailable")
    );
}

#[test]
fn orchestra_snapshot_keeps_the_server_owned_plan_floor_classification() {
    let reply: OrchestraRunResponse = serde_json::from_value(serde_json::json!({
        "accepted": true,
        "job_id": "job_123",
        "fleet": {
            "id": "job_123",
            "batch": "one admitted assignment",
            "state": "created",
            "observed_at": 1.0,
            "plan_floor_usd": 0.00447,
            "plan_floor_basis": "initial worker prompt before grounded context or retries"
        }
    }))
    .expect("typed Orchestra receipt");

    assert_eq!(reply.fleet.plan_floor_usd, Some(0.00447));
    assert_eq!(
        reply.fleet.plan_floor_basis,
        "initial worker prompt before grounded context or retries"
    );
    assert_eq!(
        reply.fleet.plan_floor_line().as_deref(),
        Some("Plan floor · $0.004470 · not expected or final spend")
    );
}

#[tokio::test]
async fn orchestra_live_client_starts_then_reads_a_whole_newer_snapshot() {
    let server = MockServer::start().await;
    let fleet = serde_json::json!({
        "id": "job_123", "batch": "1 admitted assignment", "models": ["provider/model-a"],
        "state": "created", "attempt": "first", "revision": 1, "observed_at": 1000.0,
        "stale_after_s": 60, "completed": 0, "total": 1, "agents": [{
            "index": 1, "status": "queued", "attempt": "first", "state_observed_at": 1000.0,
            "current_action": null, "progress": null,
            "assignments": {"attempted": null, "completed": null, "lost": null}
        }]
    });
    Mock::given(method("POST"))
        .and(path("/orchestra/run"))
        .and(body_json(
            serde_json::json!({"repo": "acme/api", "task": "inspect auth"}),
        ))
        .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "accepted": true, "job_id": "job_123", "fleet": fleet
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut running_fleet = fleet.clone();
    running_fleet["revision"] = serde_json::json!(2);
    running_fleet["state"] = serde_json::json!("running");
    Mock::given(method("GET"))
        .and(path("/orchestra/status"))
        .and(query_param("fleet_id", "job_123"))
        .and(query_param("after_revision", "1"))
        .and(query_param("wait_s", "20"))
        .and(query_param("repo", "acme/api"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "fleet": running_fleet
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let repo = Repo::new("acme/api").expect("repo");
    let cancel = CancellationToken::new();

    let started = client
        .orchestra_run(&repo, &OrchestraRunRequest::one("inspect auth"), &cancel)
        .await
        .expect("accepted fleet");
    let current = client
        .orchestra_status(
            &repo,
            &OrchestraStatusQuery::new(&started.job_id, 1),
            &cancel,
        )
        .await
        .expect("newer fleet");

    assert!(started.accepted && started.fleet.revision == 1);
    assert_eq!(current.fleet.revision, 2);
    assert_eq!(current.fleet.agents[0].status, FleetAgentStatus::Queued);
}

#[test]
fn orchestra_command_reply_round_trips_through_the_session_protocol() {
    let reply: CommandReply = serde_json::from_value(serde_json::json!({
        "accepted": true, "job_id": "job_123", "fleet": {
            "id": "job_123", "batch": "one", "state": "created", "revision": 1,
            "observed_at": 1000.0, "stale_after_s": 60, "completed": 0, "total": 1,
            "agents": [{"index": 1, "status": "queued", "state_observed_at": 1000.0}]
        }
    }))
    .expect("wire reply");

    let encoded = serde_json::to_value(&reply).expect("session encode");
    let decoded: CommandReply = serde_json::from_value(encoded).expect("session decode");

    assert_eq!(decoded.orchestra_accepted(), Some(true));
    assert_eq!(decoded.orchestra_job_id(), Some("job_123"));
    assert_eq!(decoded.fleet.expect("fleet").revision, 1);
}

#[test]
fn provider_key_write_uses_the_authenticated_server_contract() {
    assert_eq!(Endpoint::ProviderKey.path(), "key");
    assert_eq!(Endpoint::ProviderKey.methods(), &[HttpMethod::Post]);
    assert!(!Endpoint::ProviderKey.requires_repo());
}

#[test]
fn settings_and_global_autonomy_are_registered_with_their_real_methods() {
    assert_eq!(Endpoint::Autonomy.path(), "autonomy");
    assert_eq!(Endpoint::Autonomy.methods(), &[HttpMethod::Post]);
    assert!(!Endpoint::Autonomy.requires_repo());

    assert_eq!(Endpoint::SettingsSuite.path(), "settings/suite");
    assert_eq!(
        Endpoint::SettingsSuite.methods(),
        &[HttpMethod::Get, HttpMethod::Post]
    );
    assert!(!Endpoint::SettingsSuite.requires_repo());

    assert_eq!(Endpoint::AgentPresets.path(), "agent-presets");
    assert_eq!(
        Endpoint::AgentPresets.methods(),
        &[HttpMethod::Get, HttpMethod::Put]
    );
    assert!(!Endpoint::AgentPresets.requires_repo());
}

#[tokio::test]
async fn plain_english_dispatch_posts_the_untouched_prompt_to_the_server_owned_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/turn/route"))
        .and(body_json(
            serde_json::json!({"prompt": "Is production up right now"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "dispatch": {
                "suite": "monitor",
                "action": "monitor.uptime",
                "confidence": 1.0,
                "reason": "matched production-up"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let response = client
        .suite_dispatch(
            &SuiteDispatchRequest::new("Is production up right now"),
            &CancellationToken::new(),
        )
        .await
        .expect("dispatch");

    assert_eq!(response.dispatch.suite, "monitor");
    assert_eq!(response.dispatch.action, "monitor.uptime");
    assert_eq!(Endpoint::TurnRoute.path(), "turn/route");
    assert!(!Endpoint::TurnRoute.requires_repo());
}

#[tokio::test]
async fn account_provider_selection_posts_the_exact_provider_and_model() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/provider/select"))
        .and(body_json(serde_json::json!({
            "provider": "anthropic",
            "model": "claude-opus"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "provider": "anthropic",
            "provider_model": "claude-opus"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let reply: CommandReply = client
        .post(
            Endpoint::ProviderSelect,
            &serde_json::json!({"provider": "anthropic", "model": "claude-opus"}),
            &CancellationToken::new(),
        )
        .await
        .expect("provider selection");

    assert_eq!(reply.provider.as_deref(), Some("anthropic"));
    assert_eq!(
        reply.extra.get("provider_model").and_then(Value::as_str),
        Some("claude-opus")
    );
}

#[tokio::test]
async fn explicit_receipt_records_the_http_contract_without_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gate"))
        .and(body_json(serde_json::json!({
            "repo": "fatelabs/estelle",
            "diff": "diff body",
            "deep": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "verdict": "merge",
            "deep": {"changed_outcome": false},
            "token": "response-token-sentinel"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let output = tempfile::tempdir().expect("receipt directory");
    let receipt_path = output.path().join("http.jsonl");
    let client = Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT)
        .expect("client")
        .with_receipt_path(receipt_path.clone());

    let _: CommandReply = client
        .post_scoped(
            Endpoint::Gate,
            &Repo::new("fatelabs/estelle").expect("repo"),
            &serde_json::json!({"diff": "diff body", "deep": true}),
            &CancellationToken::new(),
        )
        .await
        .expect("gate response");

    let raw_receipt = std::fs::read_to_string(receipt_path).expect("receipt");
    let receipt: Value = serde_json::from_str(raw_receipt.trim()).expect("receipt JSON");
    assert_eq!(receipt["request"]["method"], "POST");
    assert_eq!(receipt["request"]["path"], "/gate");
    assert_eq!(receipt["request"]["body"]["deep"], true);
    assert_eq!(receipt["response"]["status"], 200);
    assert_eq!(receipt["response"]["body"]["verdict"], "merge");
    assert_eq!(receipt["response"]["body"]["token"], "[credential hidden]");
    assert!(receipt.get("headers").is_none());
    assert!(receipt["request"].get("headers").is_none());
    assert!(!raw_receipt.contains("estelle_live_test-only"));
}

#[tokio::test]
async fn explicit_receipt_fails_closed_when_it_cannot_be_written() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "plan": "pro"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let output = tempfile::tempdir().expect("receipt directory");
    let client = Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT)
        .expect("client")
        .with_receipt_path(output.path().to_path_buf());

    let result = client.account(&CancellationToken::new()).await;

    assert!(matches!(result, Err(Error::ReceiptIo(_))));
}

#[test]
fn timeout_below_two_minutes_is_refused() {
    let result = Client::new(
        "https://api.fatelabs.ca/",
        test_key(),
        Duration::from_secs(119),
    );
    assert!(matches!(result, Err(Error::TimeoutTooShort)));
}

#[test]
fn api_key_debug_never_contains_the_secret() {
    let key = test_key();
    let rendered = format!("{key:?}");
    assert_eq!(rendered, "ApiKey([REDACTED])");
    assert!(!rendered.contains("estelle_live"));
}

#[test]
fn secret_shapes_drive_input_rejection_while_prefixes_are_always_masked_for_display() {
    let secrets = [
        format!("estelle_live_{}", "a".repeat(12)),
        format!("sk-{}", "b".repeat(16)),
        format!("ghp_{}", "c".repeat(20)),
        format!("github_pat_{}", "d".repeat(20)),
        format!("sk_live_{}", "e".repeat(10)),
        format!("AKIA{}", "F".repeat(16)),
        "-----BEGIN RSA PRIVATE KEY-----".to_string(),
    ];
    for secret in secrets {
        let sentence = format!("my key is {secret} please use it");
        assert!(is_secret_shaped(&sentence), "missed {secret}");
        assert_eq!(mask_secret(&sentence), "[credential hidden]");
        assert!(!mask_secret(&sentence).contains(&secret));
    }

    for ordinary in [
        "estelle_live_short",
        "sk-short",
        "ghp_short",
        "github_pat_short",
        "explain the sk- prefix",
    ] {
        assert!(!is_secret_shaped(ordinary), "false positive: {ordinary}");
        assert_eq!(mask_secret(ordinary), "[credential hidden]");
    }
}

#[test]
fn redact_secrets_replaces_the_value_with_a_named_marker_and_leaves_prose_alone() {
    // F-2: the checkpoint wire's rule. The value never survives; the shape is named; the sentence lives.
    let token = format!("ghp_{}", "A".repeat(36));
    let redacted =
        crate::redact_secrets(&format!("here is my token {token} — why is auth failing?"));
    assert!(!redacted.contains(&token));
    assert!(redacted.contains("[redacted: a GitHub token]"));
    assert!(redacted.contains("why is auth failing?"));
    // clean prose is byte-identical, and the marker is idempotent (already-redacted text stays put)
    let plain = "an ordinary sentence about auth";
    assert_eq!(crate::redact_secrets(plain), plain);
    assert_eq!(crate::redact_secrets(&redacted), redacted);
}

#[test]
fn repo_resolver_prefers_override_and_parses_remote_shapes() {
    let override_repo = Repo::new("chosen/repo").expect("repo");
    let resolver = RepoResolver::new(Some(override_repo.clone()), "/definitely/not/a/repo");
    assert_eq!(resolver.resolve(), Some(override_repo));
    assert_eq!(
        repo_from_remote_url("git@github.com:fatelabs/estelle.git"),
        Repo::new("fatelabs/estelle")
    );
    assert_eq!(
        repo_from_remote_url("https://github.com/fatelabs/estelle.git"),
        Repo::new("fatelabs/estelle")
    );
    assert_eq!(
        repo_from_remote_url("ssh://git@github.com/fatelabs/estelle"),
        Repo::new("fatelabs/estelle")
    );
}

#[test]
fn repo_resolver_refuses_a_nonexistent_path_instead_of_inventing_its_basename() {
    assert_eq!(
        RepoResolver::new(None, "/definitely/not/a/repo").resolve(),
        None
    );
}

#[cfg(unix)]
#[test]
fn credential_file_is_created_with_mode_0600_and_secret_is_masked() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    let key = test_key();
    store.write(&key).expect("write credential");
    let mode = std::fs::metadata(store.path())
        .expect("credential metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let resolved = store.resolve().expect("read credential");
    assert_eq!(resolved.source, CredentialSource::Stored);
    let masked = mask_secret("estelle_live_never-render-this");
    assert_eq!(masked, "[credential hidden]");
    assert!(!masked.contains("never-render-this"));
}

#[test]
fn two_independent_default_stores_share_one_file_without_a_build_identity() {
    let home = tempfile::tempdir().expect("temp home");
    let estelle_home = home.path().join(".estelle");
    let first_binary = CredentialStore::from_estelle_home(&estelle_home);
    let second_binary = CredentialStore::from_estelle_home(&estelle_home);

    first_binary
        .write(&test_key())
        .expect("first binary writes");

    assert_eq!(first_binary.path(), second_binary.path());
    assert_eq!(
        second_binary.resolve().expect("second binary reads").source,
        CredentialSource::Stored
    );
    assert!(second_binary.resolve().is_ok());
}

#[cfg(unix)]
#[test]
fn world_readable_credential_file_is_refused_with_the_required_mode() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    store.write(&test_key()).expect("write credential");
    std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o644))
        .expect("make fixture unsafe");

    assert!(matches!(
        store.resolve(),
        Err(Error::InsecureCredentialPermissions { mode: 0o644 })
    ));
    assert!(
        store
            .resolve()
            .expect_err("world-readable credential must fail closed")
            .to_string()
            .contains("0600")
    );
}

#[cfg(unix)]
#[test]
fn environment_credential_bypasses_persistent_storage_without_reading_it() {
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    store.write(&test_key()).expect("write credential");
    std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o644))
        .expect("make stored fixture unreadable by policy");

    let resolved = store
        .resolve_with_environment(Some(OsString::from("estelle_live_environment-test")))
        .expect("environment must bypass the persistent backend");

    assert_eq!(resolved.source, CredentialSource::Environment);
    assert!(matches!(
        store.resolve_with_environment(None),
        Err(Error::InsecureCredentialPermissions { mode: 0o644 })
    ));
}

#[test]
fn delete_stored_only_deletes_a_stored_credential_and_needs_no_error() {
    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    store.write(&test_key()).expect("write credential");
    // An environment credential has no stored file to delete; the call is a no-op and the
    // stored file stays.
    assert!(
        !store
            .delete_stored(CredentialSource::Environment)
            .expect("environment is not stored")
    );
    assert!(store.path().exists());
    assert!(
        store
            .delete_stored(CredentialSource::Stored)
            .expect("delete stored credential")
    );
    assert!(!store.path().exists());
}

#[test]
fn an_explicit_auth_rejection_is_a_recording_signal_never_a_delete_trigger() {
    // The store exposes no path from an error value to deletion: a rejection is recorded by the
    // CALLER, and deletion (delete_stored) takes no error argument at all. This test pins the
    // predicate the caller records on — 401/403/404 are explicit rejections, an outage is not.
    for status in [
        reqwest::StatusCode::UNAUTHORIZED,
        reqwest::StatusCode::FORBIDDEN,
        reqwest::StatusCode::NOT_FOUND,
    ] {
        assert!(
            Error::Http {
                status,
                message: "explicit rejection".to_string(),
            }
            .is_explicit_auth_rejection(),
            "HTTP {status} must read as an explicit rejection"
        );
    }
    assert!(
        !Error::Http {
            status: reqwest::StatusCode::BAD_GATEWAY,
            message: "outage".to_string(),
        }
        .is_explicit_auth_rejection(),
        "an outage is not an auth rejection"
    );
}

#[tokio::test]
async fn chat_is_openai_shaped_and_repo_is_only_in_the_header() {
    let server = MockServer::start().await;
    let request = ChatCompletionRequest::question("where is auth?");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer estelle_live_test-only"))
        .and(header("x-estelle-client-protocol", "1"))
        .and(header("x-estelle-hook-contract", "1"))
        .and(header(
            "x-estelle-client-version",
            env!("CARGO_PKG_VERSION"),
        ))
        .and(header("x-estelle-repo", "fatelabs/estelle"))
        .and(body_json(serde_json::json!({
            "model": "estelle",
            "messages": [{"role": "user", "content": "where is auth?"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "estelle-cmpl",
            "object": "chat.completion",
            "model": "estelle",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "in auth.rs"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
        })))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let response = client
        .chat_completion(
            &Repo::new("fatelabs/estelle").expect("repo"),
            &request,
            &CancellationToken::new(),
        )
        .await
        .expect("chat response");
    assert_eq!(response.answer(), Some("in auth.rs"));
}

#[test]
fn compatibility_versions_are_positive_and_independent_of_the_release_semver() {
    const {
        assert!(CLIENT_PROTOCOL_VERSION > 0);
        assert!(HOOK_CONTRACT_VERSION > 0);
    }
    assert_ne!(
        CLIENT_PROTOCOL_VERSION.to_string(),
        env!("CARGO_PKG_VERSION")
    );
}

#[tokio::test]
async fn scoped_post_inserts_repo_and_unscoped_call_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/verify"))
        .and(body_json(serde_json::json!({
            "answer": "fn main() {}",
            "repo": "fatelabs/estelle"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let cancel = CancellationToken::new();
    let omitted = client
        .post::<_, Value>(
            Endpoint::Verify,
            &serde_json::json!({"answer": "fn main() {}"}),
            &cancel,
        )
        .await;
    assert!(matches!(
        omitted,
        Err(Error::RepoRequired(Endpoint::Verify))
    ));
    let response: Value = client
        .post_scoped(
            Endpoint::Verify,
            &Repo::new("fatelabs/estelle").expect("repo"),
            &serde_json::json!({"answer": "fn main() {}"}),
            &cancel,
        )
        .await
        .expect("scoped response");
    assert_eq!(response["ok"], true);
}

#[tokio::test]
async fn scoped_get_inserts_repo_on_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wiki"))
        .and(wiremock::matchers::query_param("repo", "fatelabs/estelle"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "wiki": "grounded brief",
            "repo": "fatelabs/estelle"
        })))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let response: CommandReply = client
        .get_scoped(
            Endpoint::Wiki,
            &Repo::new("fatelabs/estelle").expect("repo"),
            &NoQuery,
            &CancellationToken::new(),
        )
        .await
        .expect("wiki response");
    assert_eq!(response.wiki.as_deref(), Some("grounded brief"));
}

#[tokio::test]
async fn json_get_query_values_are_encoded_on_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/deletion-receipts"))
        .and(wiremock::matchers::query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "receipts": []
        })))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let response: Value = client
        .get(
            Endpoint::DeletionReceipts,
            &serde_json::json!({"limit": 2}),
            &CancellationToken::new(),
        )
        .await
        .expect("query response");

    assert_eq!(response["receipts"], serde_json::json!([]));
}

#[test]
fn command_reply_deserializes_every_p3_renderer_shape() {
    let reply: CommandReply = serde_json::from_value(serde_json::json!({
        "wiki": "architecture",
        "sessions": [{"id": "s-1", "title": "Auth work", "run_count": 2}],
        "findings": [{"path": "auth.rs", "line": 52, "severity": "high", "body": "key leak"}],
        "proposals": [{"title": "Centralize auth", "file": "auth.rs", "line": 52}],
        "runs": [{"task": "trace auth", "model": "strong", "grounded": true}],
        "grounded": false,
        "scope_ask": true,
        "candidates": ["fatelabs/estelle"],
        "provider": "anthropic",
        "routed": "strong",
        "diff": "diff --git a/a b/a",
        "count": 1
    }))
    .expect("typed command reply");

    assert_eq!(reply.session_summaries().len(), 1);
    assert_eq!(reply.findings[0].line, Some(52));
    assert_eq!(reply.proposals.len(), 1);
    assert_eq!(reply.agent_runs().len(), 1);
    assert_eq!(reply.candidates, ["fatelabs/estelle"]);
}

#[test]
fn multi_shape_envelope_keys_fail_loudly_on_the_wrong_shape() {
    // The envelope-collision class: one JSON key, two row shapes. A wrong-arm payload must
    // produce NOTHING RENDERABLE — never a vec of all-None rows, which looks like data, passes
    // every check, and means nothing.
    use serde_json::json;

    // leaderboard: skill rows vs member rows.
    let skill_board: CommandReply = serde_json::from_value(json!({
        "leaderboard": [{"skill": "review", "uses": 9, "successes": 8, "success_rate": 0.889}]
    }))
    .expect("skill board");
    let rows = skill_board.skill_leaderboard_rows();
    assert_eq!(rows.len(), 1, "right shape must parse");
    assert_eq!(
        rows[0].skill, "review",
        "shape assertion, not a vacuity guard"
    );
    let member_board: CommandReply = serde_json::from_value(json!({
        "leaderboard": [{"email": "dana@example.com", "metric_key": "runs", "value": 12, "rank": 1}]
    }))
    .expect("member board");
    assert!(
        member_board.skill_leaderboard_rows().is_empty(),
        "member rows were silently absorbed as all-None skill rows"
    );

    // runs/sessions: the /analytics counts must not become run/session rows.
    let analytics: CommandReply =
        serde_json::from_value(json!({"runs": 12, "sessions": 5})).expect("analytics");
    assert!(
        analytics.agent_runs().is_empty(),
        "a runs COUNT absorbed as run rows"
    );
    assert!(
        analytics.session_summaries().is_empty(),
        "a sessions COUNT absorbed as session rows"
    );
    let real_runs: CommandReply =
        serde_json::from_value(json!({"runs": [{"task": "trace auth"}]})).expect("runs list");
    assert_eq!(real_runs.agent_runs().len(), 1);
    let real_sessions: CommandReply =
        serde_json::from_value(json!({"sessions": [{"id": "s-1", "title": "t"}]}))
            .expect("sessions list");
    assert_eq!(real_sessions.session_summaries().len(), 1);

    // entities: count vs rows — the arms read the raw Value, each claiming its own shape.
    let graph: CommandReply = serde_json::from_value(json!({"entities": 42})).expect("graph count");
    assert_eq!(
        graph.graph_entities.as_ref().and_then(Value::as_u64),
        Some(42)
    );
    assert!(
        graph
            .graph_entities
            .as_ref()
            .and_then(Value::as_array)
            .is_none()
    );
    let entity_rows: CommandReply =
        serde_json::from_value(json!({"entities": [{"symbol": "s", "files": []}]}))
            .expect("entity rows");
    assert!(
        entity_rows
            .graph_entities
            .as_ref()
            .and_then(Value::as_u64)
            .is_none()
    );
    assert_eq!(
        entity_rows
            .graph_entities
            .as_ref()
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn fleet_snapshot_preserves_the_server_reported_model_roster() {
    let reply: CommandReply = serde_json::from_value(serde_json::json!({
        "fleet": {
            "id": "fleet-model-roster",
            "batch": "Grounding review",
            "models": ["K3", "gpt-5.5", "K3"],
            "state": "running",
            "observed_at": 4102444800.0,
            "agents": []
        }
    }))
    .expect("fleet model roster contract");

    let fleet = reply.fleet.expect("typed fleet");
    assert_eq!(fleet.models, ["K3", "gpt-5.5", "K3"]);
    assert!(fleet.model.is_empty());
}

#[test]
fn monitor_contract_preserves_absent_ranges_gates_and_denominators() {
    let issues: MonitorIssuesResponse = serde_json::from_value(serde_json::json!({
        "issues": [{
            "key": "iss-1",
            "symbol": "charge_card",
            "symbol_range": null,
            "title": "TimeoutError in charge_card",
            "events_in_window": 12,
            "status": "unresolved",
            "bind_status": "symbol-not-in-graph",
            "bind_detail": "symbol was not present in the swept graph",
            "repair_status": "proposed",
            "repair_gate_state": null,
            "repair_gate_verdict": null,
            "gate_absent_reason": "repair has not reached the gate"
        }],
        "counts": {"unresolved": 1},
        "window_s": 3600
    }))
    .expect("issues contract");
    assert!(issues.issues[0].symbol_range.is_none());
    assert_eq!(
        issues.issues[0].gate_absent_reason.as_deref(),
        Some("repair has not reached the gate")
    );

    let overview: MonitorOverviewResponse = serde_json::from_value(serde_json::json!({
        "series": {
            "window_s": 3600,
            "bucket_s": 300,
            "requests_source": "unavailable",
            "buckets": [{"t": 1, "errors": 4, "requests": null, "p99_ms": null}]
        }
    }))
    .expect("overview contract");
    assert_eq!(overview.error_buckets()[0].errors, 4);
    assert_eq!(overview.error_buckets()[0].requests, None);
    assert_eq!(overview.requests_source(), Some("unavailable"));
}

#[test]
fn production_issues_feed_preserves_nested_signal_binding_repair_and_absent_gate() {
    assert_eq!(Endpoint::Issues.path(), "issues");
    let issues: MonitorIssuesResponse = serde_json::from_value(serde_json::json!({
        "issues": [{
            "key": "iss-live",
            "cursor": 42.5,
            "signal": {
                "title": "TimeoutError in charge_card",
                "error_type": "TimeoutError",
                "count": 12
            },
            "bound": {
                "symbol": "charge_card",
                "status": "bound",
                "detail": "resolved from code graph",
                "file": "billing.py",
                "line": 88
            },
            "repair": {"status": "proposed", "detail": "draft ready", "pr": null},
            "gate": null,
            "gate_absent_reason": "the repair is queued"
        }],
        "next_since": 42.5,
        "has_more": false,
        "repo": "uqeu/estelle"
    }))
    .expect("issues feed contract");

    let issue = &issues.issues[0];
    assert_eq!(issue.display_title(), "TimeoutError in charge_card");
    assert_eq!(issue.event_count(), 12);
    assert_eq!(issue.bound_location(), Some(("billing.py", 88)));
    assert_eq!(issue.effective_repair_status(), "proposed");
    assert_eq!(issues.next_since, Some(42.5));
}

#[test]
fn production_issues_feed_preserves_the_exact_patch_receipt_or_its_absence_reason() {
    let exact = "--- a/billing.py\n+++ b/billing.py\n@@ -1 +1 @@\n-old\n+new\n";
    let issues: MonitorIssuesResponse = serde_json::from_value(serde_json::json!({
        "issues": [{
            "key": "with-patch",
            "repair": {
                "status": "proposed",
                "patch": {"format": "unified_diff", "base_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                          "text": exact, "observed_at": 42.5},
                "patch_absent_reason": null
            }
        }, {
            "key": "without-patch",
            "repair": {"status": "proposed", "patch": null, "patch_absent_reason": "not_persisted"}
        }]
    }))
    .expect("patch receipt contract");

    let patch = issues.issues[0]
        .effective_repair_patch()
        .expect("exact patch");
    assert_eq!(patch.format, "unified_diff");
    assert_eq!(patch.base_sha, "a".repeat(40));
    assert_eq!(patch.text, exact);
    assert_eq!(patch.observed_at, 42.5);
    assert_eq!(
        issues.issues[1].effective_patch_absent_reason(),
        Some("not_persisted")
    );
}

#[tokio::test]
async fn cancellation_wins_without_waiting_for_the_server() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(10))
                .set_body_json(serde_json::json!({"plan": "pro"})),
        )
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = client.account(&cancel).await;
    assert!(matches!(result, Err(Error::Cancelled)));
}

#[tokio::test]
async fn foreign_error_body_is_not_leaked() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(502).set_body_string("Application failed to respond"))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let result = client.account(&CancellationToken::new()).await;
    let Err(Error::Http { message, .. }) = result else {
        panic!("expected HTTP error")
    };
    assert_eq!(message, "the server returned a non-Estelle error body");
}

#[tokio::test]
#[ignore = "requires a live Estelle credential and production network access"]
async fn production_deep_search_contract() {
    let store = CredentialStore::default_location().expect("credential location");
    let credential = store.resolve().expect("configured credential");
    let client = Client::production(credential.api_key).expect("production client");
    let cancel = CancellationToken::new();
    let account = client.account(&cancel).await;
    if let Err(error) = &account {
        assert!(
            !error.is_explicit_auth_rejection(),
            "the configured credential was rejected on /account: {error} — the credential is NOT deleted; fix or re-login manually"
        );
    }
    account.expect("production account contract");

    let root = std::env::current_dir().expect("working directory");
    let repo = RepoResolver::new(None, root)
        .resolve()
        .expect("repository scope");
    let response = client
        .deep_search(
            &repo,
            &DeepSearchRequest::new(
                "Which file defines the Rust CLI port specification? Answer with the path only.",
            ),
            &cancel,
        )
        .await
        .expect("production deep-search contract");
    assert!(response.rendered_answer().is_some());
}
