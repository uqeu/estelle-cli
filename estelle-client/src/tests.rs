use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use codex_keyring_store::tests::MockKeyringStore;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::*;

fn test_key() -> ApiKey {
    ApiKey::new("estelle_live_test-only").expect("test key")
}

#[test]
fn endpoint_inventory_is_unique_and_matches_the_server_audit() {
    assert_eq!(API_ENDPOINTS.len(), 63);
    let unique = API_ENDPOINTS
        .iter()
        .map(|spec| spec.path)
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), API_ENDPOINTS.len());
    assert!(!unique.contains("help"));
    assert!(!unique.contains("c"));
    assert!(unique.contains("github/app/callback"));
    assert!(!unique.contains("github/callback"));
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
fn secure_store_round_trips_and_discards_the_rejected_source() {
    let home = tempfile::tempdir().expect("temp home");
    let estelle_home = home.path().join(".estelle");
    let keyring = Arc::new(MockKeyringStore::default());
    let store = CredentialStore::new_secure(&estelle_home, keyring);

    store.write(&test_key()).expect("write secure credential");

    assert!(!estelle_home.join("auth.json").exists());
    assert!(
        estelle_home
            .join("secrets")
            .join("estelle_auth.age")
            .exists()
    );
    assert_eq!(
        store.resolve().expect("resolve secure credential").source,
        CredentialSource::SecureStore
    );

    let rejected = Error::Http {
        status: reqwest::StatusCode::UNAUTHORIZED,
        message: "rejected".to_string(),
    };
    assert!(
        store
            .clear_if_rejected(CredentialSource::SecureStore, &rejected)
            .expect("clear secure rejection")
    );
    assert!(matches!(store.resolve(), Err(Error::NoCredential)));
}

#[test]
fn only_a_stored_key_rejected_with_401_is_deleted() {
    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    store.write(&test_key()).expect("write credential");
    let outage = Error::Http {
        status: reqwest::StatusCode::BAD_GATEWAY,
        message: "outage".to_string(),
    };
    assert!(
        !store
            .clear_if_rejected(CredentialSource::Stored, &outage)
            .expect("retain on outage")
    );
    assert!(store.path().exists());
    let rejected = Error::Http {
        status: reqwest::StatusCode::UNAUTHORIZED,
        message: "rejected".to_string(),
    };
    assert!(
        !store
            .clear_if_rejected(CredentialSource::Environment, &rejected)
            .expect("environment is not stored")
    );
    assert!(store.path().exists());
    assert!(
        store
            .clear_if_rejected(CredentialSource::Stored, &rejected)
            .expect("clear stored rejection")
    );
    assert!(!store.path().exists());
}

#[test]
fn every_explicit_auth_rejection_discards_a_stored_key_but_an_outage_does_not() {
    for status in [
        reqwest::StatusCode::UNAUTHORIZED,
        reqwest::StatusCode::FORBIDDEN,
        reqwest::StatusCode::NOT_FOUND,
    ] {
        let home = tempfile::tempdir().expect("temp home");
        let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
        store.write(&test_key()).expect("write credential");
        assert!(
            store
                .clear_if_rejected(
                    CredentialSource::Stored,
                    &Error::Http {
                        status,
                        message: "explicit rejection".to_string(),
                    },
                )
                .expect("clear explicit rejection"),
            "HTTP {status} must discard the rejected key"
        );
        assert!(!store.path().exists());
    }

    let home = tempfile::tempdir().expect("temp home");
    let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
    store.write(&test_key()).expect("write credential");
    assert!(
        !store
            .clear_if_rejected(
                CredentialSource::Stored,
                &Error::Http {
                    status: reqwest::StatusCode::BAD_GATEWAY,
                    message: "outage".to_string(),
                },
            )
            .expect("retain on outage")
    );
    assert!(store.path().exists());
}

#[tokio::test]
async fn chat_is_openai_shaped_and_repo_is_only_in_the_header() {
    let server = MockServer::start().await;
    let request = ChatCompletionRequest::question("where is auth?");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer estelle_live_test-only"))
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

    assert_eq!(reply.sessions.len(), 1);
    assert_eq!(reply.findings[0].line, Some(52));
    assert_eq!(reply.proposals.len(), 1);
    assert_eq!(reply.runs.len(), 1);
    assert_eq!(reply.candidates, ["fatelabs/estelle"]);
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
        store
            .clear_if_rejected(credential.source, error)
            .expect("rejected-key handling");
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
