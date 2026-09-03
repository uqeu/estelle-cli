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
    assert_eq!(API_ENDPOINTS.len(), 85);
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

#[test]
fn govern_is_the_unscoped_session_compaction_owner() {
    assert_eq!(Endpoint::Govern.path(), "govern");
    assert_eq!(Endpoint::Govern.methods(), &[HttpMethod::Post]);
    assert!(!Endpoint::Govern.requires_repo());
}

#[test]
fn hardware_advice_is_an_unscoped_post_endpoint() {
    assert_eq!(Endpoint::HardwareAdvice.path(), "hardware/advice");
    assert_eq!(Endpoint::HardwareAdvice.methods(), &[HttpMethod::Post]);
    assert!(!Endpoint::HardwareAdvice.requires_repo());
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
                    "label": "Recalling your codebase",
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
    assert_eq!(
        progress.work.label.as_deref(),
        Some("Recalling your codebase")
    );
    let work_json = serde_json::to_value(&progress.work).expect("serializable work progress");
    assert_eq!(work_json["label"], "Recalling your codebase");
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

#[tokio::test]
async fn durable_job_stream_delivers_phase_revisions_before_the_terminal_receipt() {
    let server = MockServer::start().await;
    let job_id = "job_0123456789abcdef01234567";
    let body = [
        serde_json::json!({"event": "progress", "snapshot": {
            "job_id": job_id, "state": "running", "terminal": false,
            "progress": {"revision": 1, "work": {"phase": "scope"}}
        }}),
        serde_json::json!({"event": "heartbeat", "revision": 1, "state": "running"}),
        serde_json::json!({"event": "progress", "snapshot": {
            "job_id": job_id, "state": "running", "terminal": false,
            "progress": {"revision": 2, "work": {"phase": "recall"}}
        }}),
        serde_json::json!({"event": "complete", "snapshot": {
            "job_id": job_id, "state": "done", "terminal": true,
            "progress": {"revision": 3, "work": {"phase": "gate"}},
            "result": {"answer": "bounded"}
        }}),
    ]
    .into_iter()
    .map(|event| serde_json::to_string(&event).expect("event"))
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";
    Mock::given(method("GET"))
        .and(path(format!("/jobs/{job_id}/events")))
        .and(header("authorization", "Bearer estelle_live_test-only"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/x-ndjson")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");
    let mut phases = Vec::new();

    let terminal = client
        .stream_job(job_id, &CancellationToken::new(), |snapshot| {
            if let Some(progress) = &snapshot.progress {
                phases.push(progress.work.phase.clone());
            }
        })
        .await
        .expect("terminal stream receipt");

    assert_eq!(phases, ["scope", "recall", "gate"]);
    assert!(terminal.terminal);
    assert_eq!(
        terminal.result,
        Some(serde_json::json!({"answer": "bounded"}))
    );
}

#[tokio::test]
async fn durable_job_stream_refuses_a_complete_event_without_a_typed_snapshot() {
    let server = MockServer::start().await;
    let job_id = "job_0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path(format!("/jobs/{job_id}/events")))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"event\":\"complete\"}\n"))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let error = client
        .stream_job(job_id, &CancellationToken::new(), |_| {})
        .await
        .expect_err("a completion without its durable receipt must go red");

    assert!(matches!(error, Error::InvalidProgressStream));
}

#[tokio::test]
async fn real_http_stream_surfaces_progress_before_the_server_releases_completion() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc, oneshot};

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let job_id = "job_0123456789abcdef01234567";
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut part = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut part).await.expect("request read");
            assert_ne!(read, 0, "request ended before headers");
            request.extend_from_slice(&part[..read]);
        }
        assert!(
            String::from_utf8_lossy(&request)
                .starts_with(&format!("GET /jobs/{job_id}/events HTTP/1.1"))
        );
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n")
            .await
            .expect("headers");
        socket
            .write_all(
                format!(
                    "{}\n",
                    serde_json::json!({"event": "progress", "snapshot": {
                        "job_id": job_id, "state": "running", "terminal": false,
                        "progress": {"revision": 1, "work": {"phase": "scope"}}
                    }})
                )
                .as_bytes(),
            )
            .await
            .expect("progress");
        socket.flush().await.expect("progress flush");
        release_rx.await.expect("terminal release");
        socket
            .write_all(
                format!(
                    "{}\n",
                    serde_json::json!({"event": "complete", "snapshot": {
                        "job_id": job_id, "state": "done", "terminal": true,
                        "progress": {"revision": 2, "work": {"phase": "gate"}},
                        "result": {"answer": "bounded"}
                    }})
                )
                .as_bytes(),
            )
            .await
            .expect("complete");
    });
    let client =
        Client::new(&format!("http://{address}/"), test_key(), MINIMUM_TIMEOUT).expect("client");
    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
    let watcher = tokio::spawn(async move {
        client
            .stream_job(job_id, &CancellationToken::new(), move |snapshot| {
                if let Some(progress) = &snapshot.progress {
                    let _ = progress_tx.send(progress.work.phase.clone());
                }
            })
            .await
    });

    let first = tokio::time::timeout(Duration::from_secs(2), progress_rx.recv())
        .await
        .expect("progress arrived before completion was released")
        .expect("progress channel");
    assert_eq!(first, "scope");
    release_tx.send(()).expect("release completion");
    let terminal = watcher
        .await
        .expect("watch task")
        .expect("terminal receipt");
    server.await.expect("server task");
    assert_eq!(terminal.progress.expect("progress").work.phase, "gate");
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
fn the_wire_now_covers_the_full_catalogue_with_entropy_gates_and_allowlists_on() {
    // Beyond the legacy seven: an invented slack token in prose is flagged by the engine and
    // redacted with the engine's fingerprinted marker. The fixture is assembled, never a
    // literal — a verbatim scanner-shaped token in source trips GitHub push protection.
    let slack = format!(
        "xoxb-{}-{}-{}",
        "123456789012", "123456789012", "AbCdEfGhIjKlMnOpQrStUvWx"
    );
    let sentence = format!("here is the bot token {slack} please rotate it");
    let (shape, line) = crate::find_secret_shape(&sentence).expect("engine coverage");
    assert_eq!(shape, "slack-bot-token");
    assert_eq!(line, 1);
    let redacted = crate::redact_secrets(&sentence);
    assert!(!redacted.contains(&slack));
    assert!(redacted.contains("[REDACTED:slack-bot-token:"));
    assert!(redacted.contains("please rotate it"));

    // Negative controls on the WIRE path — allowlists and entropy gates are ON: AWS's own
    // published example key and a git SHA-256 checksum survive untouched. Paired positive
    // controls: a real-shaped AWS key and a real-shaped slack token DO fire.
    let docs = "the AWS docs use AKIAIOSFODNN7EXAMPLE as their example";
    assert_eq!(crate::redact_secrets(docs), docs);
    assert_eq!(crate::find_secret_shape(docs), None);
    let checksum = "checksum e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 ok";
    assert_eq!(crate::redact_secrets(checksum), checksum);
    assert_eq!(crate::find_secret_shape(checksum), None);
    // The positive control is assembled like every other credential-shaped fixture — no
    // scanner-shaped literal sits in source (GitHub push protection).
    let live_aws = format!("key AKIA{} here", "QF7DMC5BAZ2W7XKP");
    assert!(crate::find_secret_shape(&live_aws).is_some());
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
    // `resolve()` reads ESTELLE_API_KEY / ESTELLE_KEY from the AMBIENT process environment
    // (auth.rs:154). On any machine where a developer has exported one — which is the normal
    // state for anyone actually using the product — that short-circuits at auth.rs:167 and
    // this test never reaches the stored file it exists to check. Drive the hermetic seam.
    let resolved = store
        .resolve_with_environment(None)
        .expect("read credential");
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
    // `resolve()` reads ESTELLE_API_KEY / ESTELLE_KEY from the AMBIENT process environment
    // (auth.rs:154). On any machine where a developer has exported one — which is the normal
    // state for anyone actually using the product — that short-circuits at auth.rs:167 and
    // this test never reaches the stored file it exists to check. Drive the hermetic seam.
    assert_eq!(
        second_binary
            .resolve_with_environment(None)
            .expect("second binary reads")
            .source,
        CredentialSource::Stored
    );
    assert!(second_binary.resolve_with_environment(None).is_ok());
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

    // `resolve()` reads ESTELLE_API_KEY / ESTELLE_KEY from the AMBIENT process environment
    // (auth.rs:154). On any machine where a developer has exported one — which is the normal
    // state for anyone actually using the product — that short-circuits at auth.rs:167 and
    // this test never reaches the stored file it exists to check. Drive the hermetic seam.
    assert!(matches!(
        store.resolve_with_environment(None),
        Err(Error::InsecureCredentialPermissions { mode: 0o644 })
    ));
    assert!(
        store
            .resolve_with_environment(None)
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

// ── Server-advertised load shedding ────────────────────────────────────────────────────────────
//
// 🔴 WHY THESE EXIST. The public-binary receipt for v0.2.28 failed nine contracts, and eight of them
// were a cascade off ONE: `estelle sweep` met `503 dependency slow-path cooldown; retry after the
// advertised interval` and exited 1. The server was behaving correctly — `api.py:_guard` sheds a
// dependency-bound route with a bounded `Retry-After` BEFORE the handler dispatches, so the request
// had no effect and coming back later is exactly right. The Python client honours it
// (`serve/backend.py:585`); this client did not, so a routine cooldown read to a user as a hard
// failure and every downstream contract went unobserved.
//
// ⚠️ The negative control is the load-bearing test. Retrying on a 503 ALONE would make the client
// hammer a server that never asked it to — so `shed_without_retry_after_is_not_retried` pins that
// the HEADER, not the status, is what authorises a second attempt.

#[tokio::test]
async fn advertised_cooldown_is_honoured_and_the_call_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("Retry-After", "1")
                .set_body_json(serde_json::json!({
                    "error": {"message": "dependency slow-path cooldown; retry after the advertised interval"}
                })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "plan": "ultra", "plan_active": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let account = client.account(&CancellationToken::new()).await.expect(
        "a shed request must be retried after the advertised interval, not surfaced as a failure",
    );

    assert_eq!(account.plan.as_deref(), Some("ultra"));
}

#[tokio::test]
async fn a_persistent_cooldown_fails_with_the_servers_own_503_after_a_bounded_number_of_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("Retry-After", "1")
                .set_body_json(serde_json::json!({
                    "error": {"message": "dependency slow-path cooldown; retry after the advertised interval"}
                })),
        )
        // 🔑 THE BOUND IS THE ASSERTION. A retry loop with no stated ceiling is the defect this fix
        // would otherwise introduce, so pin the exact attempt count the constant promises.
        .expect(u64::from(SHED_MAX_ATTEMPTS))
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let error = client.account(&CancellationToken::new()).await.expect_err(
        "an unrelenting cooldown must still surface, never be swallowed into a fake success",
    );

    match error {
        Error::Http { status, .. } => assert_eq!(status.as_u16(), 503),
        other => panic!("expected the server's own 503 to survive the retries, got {other:?}"),
    }
}

#[tokio::test]
async fn shed_without_retry_after_is_not_retried() {
    // NEGATIVE CONTROL: status alone must never authorise a second attempt. Without this, the fix
    // would turn every unrelated 503 into three requests against an already-struggling server.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "error": {"message": "service unavailable"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        Client::new(&format!("{}/", server.uri()), test_key(), MINIMUM_TIMEOUT).expect("client");

    let error = client
        .account(&CancellationToken::new())
        .await
        .expect_err("a 503 with no advertised interval is a plain failure");

    assert!(matches!(error, Error::Http { status, .. } if status.as_u16() == 503));
}

#[test]
fn an_interval_we_cannot_afford_to_wait_is_declined_rather_than_silently_obeyed() {
    let ok = shed_delay_for(
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        Some(SHED_MAX_WAIT.as_secs().to_string().as_str()),
    );
    assert_eq!(ok, Some(SHED_MAX_WAIT));

    // Past the ceiling we do NOT sleep and we do NOT pretend to succeed — the 503 goes to the caller.
    let too_long = shed_delay_for(
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        Some((SHED_MAX_WAIT.as_secs() + 1).to_string().as_str()),
    );
    assert_eq!(too_long, None);

    assert_eq!(
        shed_delay_for(reqwest::StatusCode::SERVICE_UNAVAILABLE, Some("0")),
        None
    );
    assert_eq!(
        shed_delay_for(reqwest::StatusCode::SERVICE_UNAVAILABLE, Some("soon")),
        None
    );
    assert_eq!(shed_delay_for(reqwest::StatusCode::OK, Some("1")), None);
    assert_eq!(
        shed_delay_for(reqwest::StatusCode::TOO_MANY_REQUESTS, Some("2")),
        Some(Duration::from_secs(2))
    );
}

// ── The HTTP receipt must be wired at the SHARED constructor ───────────────────────────────────
//
// 🔴 THE DEFECT THIS GUARDS. `ESTELLE_RECEIPT_PATH` used to be read inside `Client::production()`
// alone. The TUI calls `production()` twice and `new()` thirteen times — `session_server.rs` builds a
// client six times by itself — so nearly every request the app made recorded nothing. The
// public-binary probe then reported **"not observed" for all 26 routes it asserts, 29 contracts**,
// during a session that was visibly pulling a 275-file repo graph. The calls happened; the observer
// was wired to a path the app mostly does not take.
//
// ⚠️ This is a STATIC wiring proof on purpose. A behavioural test would have to mutate a
// process-global env var, which is `unsafe` in edition 2024 and races every other test in this
// binary. What actually regresses here is not the reading of the variable — it is WHERE the reading
// lives, and that is a fact about the source, so the source is what this asserts.

#[test]
fn the_receipt_path_is_read_at_the_one_constructor_every_caller_reaches() {
    const SOURCE: &str = include_str!("lib.rs");
    const VAR: &str = "ESTELLE_RECEIPT_PATH";

    let new_at = SOURCE.find("pub fn new(").expect("Client::new must exist");
    let after_new = &SOURCE[new_at..];
    let new_body_end = after_new
        .find("pub fn with_receipt_path")
        .expect("with_receipt_path follows new()");
    let new_body = &after_new[..new_body_end];

    assert!(
        new_body.contains(VAR),
        "{VAR} is not read in Client::new. Every constructor funnels through new(), so a client \
         built any other way records nothing and every route reads 'not observed'."
    );

    // And it must not ALSO be read in production(), or there are two owners of one decision again.
    let prod_at = SOURCE
        .find("pub fn production(")
        .expect("Client::production must exist");
    let prod_body = &SOURCE[prod_at..prod_at + new_at.saturating_sub(prod_at).max(1)];
    assert!(
        !prod_body.contains(VAR),
        "{VAR} is read in production() as well as new(). One owner per derived fact: production() \
         delegates to new(), so the read belongs in exactly one of them."
    );
}

/// 🔴 THE BLOCK IS THE SERVER'S SHAPE, PARSED, NOT GUESSED AT.
///
/// Fields and vocabulary come from `serve/answer_currency.py::_fields` and
/// `serve/graph_currency.py`. Parsing a payload written in that shape is the only thing that can
/// tell a correct type from a plausible one — and a plausible one is what a client invents when it
/// duck-types a contract that is written down elsewhere.
#[test]
fn a_decertified_answer_parses_its_currency_block() {
    let payload = serde_json::json!({
        "answer": "NOT CERTIFIED - this answer quotes indexed code that may no longer be current.\n\ncharge_card lives in billing/charge.rs.",
        "sources": [{"file": "billing/charge.rs", "line": 82}],
        "grounded": false,
        "code_currency": {
            "status": "stale",
            "indexed_head": "6ff03b1857ab4c0d9e21",
            "current_head": "75557c7f11ab2e0044aa",
            "depends_on_code": "certified_code_claim",
            "cited_paths": 1,
            "detail": "uqeu/estelle: STALE — indexed at 6ff03b1857ab, repo is now 75557c7f11ab. \
                       Real code added since then reads as invented. Re-sweep/reindex this repo to \
                       advance the marker, then retry."
        }
    });

    let response: DeepSearchResponse = serde_json::from_value(payload).expect("decertified answer");
    let currency = response.code_currency.expect("the block is present");

    assert!(currency.is_stale());
    assert_eq!(currency.depends_on_code, "certified_code_claim");
    assert_eq!(currency.cited_paths, 1);
    assert_eq!(CodeCurrency::short(&currency.indexed_head), "6ff03b1857ab");
    assert_eq!(CodeCurrency::short(&currency.current_head), "75557c7f11ab");
    assert!(currency.detail.contains("Re-sweep/reindex this repo"));
}

/// The negative control, and it is the whole point of the field being an `Option`.
///
/// `serve/answer_currency.py` returns the block ONLY when the index is behind AND the answer leans
/// on the code; the healthy payload is byte-identical to one from a build that never had the
/// field. So a healthy answer must parse to `None` — not to a default-filled block that a renderer
/// would then have to guess was meaningless.
#[test]
fn a_current_index_produces_no_currency_block_at_all() {
    let payload = serde_json::json!({
        "answer": "charge_card lives in billing/charge.rs.",
        "sources": [{"file": "billing/charge.rs", "line": 82}],
        "grounded": true
    });

    let response: DeepSearchResponse = serde_json::from_value(payload).expect("healthy answer");
    assert!(response.code_currency.is_none());
}

/// A head shorter than the cut is returned WHOLE. Shortening it to a stub would produce a SHA that
/// looks like a SHA and identifies nothing, which is the failure mode this repo has paid for twice.
#[test]
fn a_short_head_is_not_padded_or_clipped_into_a_plausible_one() {
    assert_eq!(CodeCurrency::short("abc"), "abc");
    assert_eq!(CodeCurrency::short(""), "");
    assert_eq!(CodeCurrency::short("6ff03b1857ab4c0d"), "6ff03b1857ab");
}

/// 🔴 **A TEST THAT READS THE AMBIENT ENVIRONMENT IS A TEST THAT RUNS DIFFERENTLY ON YOUR MACHINE.**
///
/// [`CredentialStore::resolve`] reads `ESTELLE_API_KEY` / `ESTELLE_KEY` off the real process
/// environment (`auth.rs:154`) and short-circuits before ever touching the stored file
/// (`auth.rs:167`). **Four** tests across **two** crates called it while asserting things about the
/// STORED backend:
///
/// * `credential_file_is_created_with_mode_0600_and_secret_is_masked` (this file)
/// * `two_independent_default_stores_share_one_file_without_a_build_identity` (this file)
/// * `world_readable_credential_file_is_refused_with_the_required_mode` (this file)
/// * `login::tests::login_stores_success_refuses_rejection_and_keeps_failure_to_ask` (`codex-tui`)
///
/// On a developer machine with a key exported — the normal state for anyone using the product —
/// all four took the environment branch. Three failed loudly; one asserted only `is_ok()`, which
/// the environment branch also satisfies, so it **passed for the wrong reason**.
///
/// ⚠️ **The first version of this guard scanned only this file with `include_str!`, and the fourth
/// instance was in another crate — so it was a guard on one path, which is the exact defect it was
/// written to catch.** It walks the workspace now.
///
/// ⚠️ **Limits, stated:** this is a STRUCTURAL check over source text. It proves which function a
/// call site NAMES, never which function runs; it cannot see an ambient read reached through a
/// helper, and a call spelled differently is invisible to it. It is a ratchet on the shape we have
/// paid for four times, not a proof of isolation.
#[test]
fn no_test_in_the_workspace_resolves_through_the_ambient_environment() {
    use std::path::Path;
    use std::path::PathBuf;

    // Split so the needle does not appear literally in this file — a guard that matches its own
    // source reports itself as the offender, which is how the first run of this test failed.
    let ambient = concat!(".re", "solve()");
    let hermetic = concat!(".resolve_with_env", "ironment(");

    // The one sanctioned ambient reader: an #[ignore]d test that needs a live credential.
    const SANCTIONED: &str = "production_deep_search_contract";

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("estelle-client has a parent directory")
        .to_path_buf();

    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                // Build output and vendored trees are not ours to police, and walking them is slow.
                if name == "target" || name == "vendor" || name.starts_with('.') {
                    continue;
                }
                collect(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(&workspace, &mut files);

    // Only TEST functions are in scope. Production code is SUPPOSED to read the ambient
    // environment — `resolve_credential` (`tui/src/main.rs:6322`), `provider_keys.rs:90` and three
    // others are the real credential path, and an earlier version of this guard flagged all five.
    // The enclosing function is found by scanning back to its `fn` line and checking the few lines
    // above it for a test attribute.
    ///
    /// Returns the enclosing TEST function's declaration line, or `None` when the enclosing
    /// function is not a test. Returning the declaration rather than a bool is what lets the
    /// sanctioned reader be exempted by FUNCTION instead of by file — an earlier version exempted
    /// any file mentioning the sanctioned name, which made this guard **inert across the very file
    /// that held three of the four defects**. Proven by mutation: it passed on a reverted call
    /// site until this changed.
    fn enclosing_test_fn<'a>(lines: &[&'a str], index: usize) -> Option<&'a str> {
        let fn_line = (0..=index).rev().find(|&n| {
            lines[n].trim_start().starts_with("fn ") || lines[n].contains("async fn ")
        })?;
        let is_test = lines[fn_line.saturating_sub(6)..=fn_line]
            .iter()
            .any(|line| {
                let line = line.trim_start();
                line.starts_with("#[test]") || line.starts_with("#[tokio::test]")
            });
        is_test.then_some(lines[fn_line])
    }

    let mut offenders = Vec::new();
    let mut hermetic_sites = 0usize;
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        hermetic_sites += text.matches(hermetic).count();
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            // Only credential stores are in scope; `resolve()` is a common method name.
            let is_credential_store = line.contains("store.re") || line.contains("binary.re");
            // Exempt the ONE sanctioned ambient reader by the name of the function it lives in.
            let enclosing = enclosing_test_fn(&lines, number);
            let is_sanctioned = enclosing.is_some_and(|decl| decl.contains(SANCTIONED));
            if is_credential_store
                && line.contains(ambient)
                && enclosing.is_some()
                && !is_sanctioned
            {
                offenders.push(format!(
                    "{}:{}",
                    file.strip_prefix(&workspace).unwrap_or(file).display(),
                    number + 1
                ));
            }
        }
    }

    // Two vacuity guards. The walk finding no files, or the hermetic seam having been renamed,
    // would each make `offenders` trivially empty while proving nothing.
    assert!(
        files.len() > 100,
        "the workspace walk found only {} .rs files — it is pointed at the wrong root, so the \
         emptiness below is a claim about the walk, not about the tests",
        files.len()
    );
    assert!(
        hermetic_sites >= 5,
        "found only {hermetic_sites} uses of the hermetic seam across the workspace — it has \
         probably been renamed, and this guard is measuring nothing"
    );
    assert!(
        offenders.is_empty(),
        "these call sites resolve a credential through the AMBIENT process environment, so they \
         assert different things depending on whether ESTELLE_API_KEY is exported: {offenders:#?}. \
         Use `resolve_with_environment(None)` for the stored backend, or \
         `resolve_with_environment(Some(..))` to test the environment branch on purpose."
    );
}
