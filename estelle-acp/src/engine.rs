//! The engine decision for ACP prompts: WHOSE credential does the thinking.
//!
//! When the user has signed in with `estelle login --chatgpt`, the ChatGPT plan serves the
//! model call directly (the local engine path) and Estelle's /search rides in as a labelled
//! context block — their plan does the thinking, Estelle does the grounding, nobody pays
//! twice. Without that credential the server path (/deep-search, Estelle key) is unchanged.
//! Every prompt ends with THE RECEIPT: one honest line naming whose credential served.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::ContentBlock;
use agent_client_protocol::schema::v1::TextContent;
use codex_api::ApiError;
use codex_api::AuthProvider;
use codex_api::ModelsClient;
use codex_api::Provider;
use codex_api::ReqwestTransport;
use codex_api::ResponseEvent;
use codex_api::ResponsesApiRequest;
use codex_api::ResponsesClient;
use codex_api::ResponsesOptions;
use codex_api::RetryConfig;
use codex_api::TransportError;
use codex_login::AuthManager;
use codex_models_manager::bundled_models_response;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ReasoningEffort;
use estelle_client::Client;
use estelle_client::DeepSearchRequest;
use estelle_client::Endpoint;
use estelle_client::Repo;
use http::HeaderMap;
use http::HeaderValue;
use http::StatusCode;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// The ChatGPT backend the inherited codex-app client family calls directly.
pub(crate) const CHATGPT_BACKEND_BASE: &str = "https://chatgpt.com/backend-api/codex";

/// THE RECEIPT. One honest line per prompt naming whose credential served — the founder's
/// evidence artifact. Always present, never decorated into looking like more than it is.
pub(crate) const LOCAL_RECEIPT: &str =
    "— engine: your ChatGPT plan (device-code login) · grounding: estelle /search";
pub(crate) const SERVER_RECEIPT: &str = "— engine: estelle server (your API key)";
/// A rejected plan credential is SAID, once, never silently fallen back from — a silent
/// fallback would hide a dead plan.
pub(crate) const FALLBACK_NOTICE: &str =
    "your ChatGPT plan credential was rejected — answering via the Estelle server instead";
/// The label the Estelle recall rides in under, above the user's prompt.
pub(crate) const MEMORY_LABEL: &str = "Estelle memory, authoritative for this repo:";

/// Where `estelle login --chatgpt` stores the credential (see tui/src/login.rs).
pub(crate) fn chatgpt_home() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("chatgpt"))
}

/// The model a local-engine session runs on: slug, catalog instructions (the backend
/// historically validates codex instructions), and the catalog's default reasoning effort.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelSelection {
    slug: String,
    instructions: String,
    effort: Option<ReasoningEffort>,
}

pub(crate) enum Engine {
    /// /deep-search on the Estelle server, paid by the Estelle API key.
    Server,
    /// The user's own ChatGPT plan serves the model call.
    Local(ChatGptEngine),
}

pub(crate) struct ChatGptEngine {
    auth_manager: Arc<AuthManager>,
    base_url: String,
}

impl Engine {
    /// Local engine when the ChatGPT credential EXISTS AND LOADS; the server path otherwise.
    /// A credential that cannot be loaded must neither hijack nor break the server path.
    pub(crate) async fn resolve(chatgpt_home: Option<PathBuf>, base_url: &str) -> Engine {
        let Some(home) = chatgpt_home else {
            return Engine::Server;
        };
        if !home.join("auth.json").exists() {
            return Engine::Server;
        }
        let manager = Arc::new(
            AuthManager::new(
                home,
                /*enable_codex_api_key_env*/ false,
                codex_login::AuthCredentialsStoreMode::File,
                /*forced_chatgpt_workspace_id*/ None,
                /*chatgpt_base_url*/ None,
                codex_login::AuthKeyringBackendKind::default(),
                chatgpt_auth_route_config(),
            )
            .await,
        );
        // auth() is the manager's reusable valid-token path: it refreshes proactively inside
        // the 5-minute expiry window and persists the rotated refresh token itself.
        match manager.auth().await {
            Some(_) => Engine::Local(ChatGptEngine {
                auth_manager: manager,
                base_url: base_url.to_string(),
            }),
            None => Engine::Server,
        }
    }
}

fn chatgpt_auth_route_config() -> codex_login::AuthRouteConfig {
    codex_login::AuthRouteConfig::from_http_client_factory(
        codex_http_client::HttpClientFactory::new(
            codex_http_client::OutboundProxyPolicy::ReqwestDefault,
        ),
    )
}

/// The headers the backend expects from the plan credential: Bearer + ChatGPT-Account-ID.
struct PlanAuth {
    access_token: String,
    account_id: Option<String>,
}

impl AuthProvider for PlanAuth {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.access_token)) {
            headers.insert(http::header::AUTHORIZATION, value);
        }
        if let Some(account_id) = &self.account_id
            && let Ok(value) = HeaderValue::from_str(account_id)
        {
            headers.insert("ChatGPT-Account-ID", value);
        }
    }
}

impl ChatGptEngine {
    /// The currently-valid plan credential, refreshed by the manager when due.
    async fn plan_auth(&self) -> Option<PlanAuth> {
        let auth = self.auth_manager.auth().await?;
        Some(PlanAuth {
            access_token: auth.get_token().ok()?,
            account_id: auth.get_account_id(),
        })
    }

    /// GET {base}/models?client_version=… with the user's credential.
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>, ApiError> {
        let auth = self.plan_auth().await.ok_or_else(|| {
            ApiError::Stream("the ChatGPT credential is not loadable".to_string())
        })?;
        let client = ModelsClient::new(
            ReqwestTransport::new(reqwest::Client::new()),
            backend_provider(&self.base_url),
            Arc::new(auth),
        );
        let url = ModelsClient::<ReqwestTransport>::request_url(
            &backend_provider(&self.base_url),
            env!("CARGO_PKG_VERSION"),
        );
        let (models, _etag) = client.list_models(url, HeaderMap::new()).await?;
        Ok(models)
    }
}

fn backend_provider(base_url: &str) -> Provider {
    Provider {
        name: "chatgpt-backend".to_string(),
        base_url: base_url.to_string(),
        query_params: None,
        headers: HeaderMap::new(),
        retry: RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(200),
            retry_429: false,
            retry_5xx: false,
            retry_transport: true,
        },
        stream_idle_timeout: Duration::from_secs(300),
    }
}

/// The top-priority VISIBLE slug — a hidden model never wins, whatever its priority number.
pub(crate) fn pick_model(models: &[ModelInfo]) -> Option<ModelSelection> {
    models
        .iter()
        .filter(|model| model.visibility == ModelVisibility::List)
        .min_by_key(|model| model.priority)
        .map(|model| ModelSelection {
            slug: model.slug.clone(),
            instructions: model.base_instructions.clone(),
            effort: model.default_reasoning_level.clone(),
        })
}

/// The bundled catalog (models-manager/models.json) — the fallback when GET /models fails.
pub(crate) fn bundled_model() -> ModelSelection {
    let models = bundled_models_response()
        .map(|response| response.models)
        .unwrap_or_default();
    if let Some(selection) = pick_model(&models) {
        return selection;
    }
    // Unreachable while the shipped catalog parses and lists a visible model; a corrupt
    // embedded catalog must degrade, never panic a customer session.
    ModelSelection {
        slug: "gpt-5.1-codex".to_string(),
        instructions: String::new(),
        effort: None,
    }
}

/// The session-start model choice: the backend's own list when it answers, bundled otherwise.
pub(crate) async fn select_model(engine: &ChatGptEngine) -> ModelSelection {
    match engine.fetch_models().await {
        Ok(models) => pick_model(&models).unwrap_or_else(bundled_model),
        Err(_) => bundled_model(),
    }
}

/// The one user message: the Estelle recall as a labelled context block ABOVE the prompt.
fn labelled_input(question: &str, recall: Option<&str>) -> String {
    match recall.map(str::trim).filter(|text| !text.is_empty()) {
        Some(recall) => format!("{MEMORY_LABEL}\n{recall}\n\n{question}"),
        None => question.to_string(),
    }
}

/// The Responses body the backend expects — one message, no stored state, streaming.
pub(crate) fn local_request(
    model: &ModelSelection,
    question: &str,
    recall: Option<&str>,
) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: model.slug.clone(),
        instructions: model.instructions.clone(),
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: labelled_input(question, recall),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }],
        tools: None,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: false,
        reasoning: model.effort.clone().map(|effort| codex_api::Reasoning {
            effort: Some(effort),
            summary: None,
            context: None,
        }),
        store: false,
        stream: true,
        stream_options: None,
        include: vec!["reasoning.encrypted_content".to_string()],
        service_tier: None,
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
    }
}

/// The fuel: Estelle memory for this repo. Silent on ANY failure — no memory is not an error.
async fn gather_recall(
    http: &Client,
    repo: &Repo,
    question: &str,
    cancel: &CancellationToken,
) -> Option<String> {
    let result: Result<Value, estelle_client::Error> = http
        .post_scoped(
            Endpoint::Search,
            repo,
            &serde_json::json!({"query": question}),
            cancel,
        )
        .await;
    result.ok().and_then(|value| {
        value
            .get("recall")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(value))
}

/// 401/403 — the backend looked at the credential and refused it. Anything else is a ordinary
/// failure, not a rejection.
fn is_credential_rejection(error: &ApiError) -> bool {
    let status = match error {
        ApiError::Transport(TransportError::Http { status, .. }) => *status,
        ApiError::Api { status, .. } => *status,
        _ => return false,
    };
    status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
}

pub(crate) enum PromptAnswer {
    Answered,
    Cancelled,
    Failed(String),
}

enum PlanOutcome {
    Answered,
    Rejected,
    Cancelled,
    Failed(String),
}

/// The local engine path: gather the fuel, call the ChatGPT backend directly, stream the
/// deltas back. A credential rejection is the ONLY fallback to the server path.
async fn answer_via_plan(
    engine: &ChatGptEngine,
    http: &Client,
    repo: &Repo,
    model: Option<&ModelSelection>,
    question: &str,
    cancel: &CancellationToken,
    emit: &mut (dyn FnMut(ContentBlock) + Send),
) -> PlanOutcome {
    let recall = gather_recall(http, repo, question, cancel).await;
    if cancel.is_cancelled() {
        return PlanOutcome::Cancelled;
    }
    let Some(auth) = engine.plan_auth().await else {
        return PlanOutcome::Rejected;
    };
    let client = ResponsesClient::new(
        ReqwestTransport::new(reqwest::Client::new()),
        backend_provider(&engine.base_url),
        Arc::new(auth),
    );
    let bundled;
    let model = match model {
        Some(model) => model,
        None => {
            bundled = bundled_model();
            &bundled
        }
    };
    let mut extra_headers = HeaderMap::new();
    extra_headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
    let options = ResponsesOptions {
        session_id: Some(Uuid::new_v4().to_string()),
        extra_headers,
        ..Default::default()
    };
    let mut stream = match client
        .stream_request(local_request(model, question, recall.as_deref()), options)
        .await
    {
        Ok(stream) => stream,
        Err(error) if is_credential_rejection(&error) => return PlanOutcome::Rejected,
        Err(error) => return PlanOutcome::Failed(error.to_string()),
    };
    loop {
        let event = tokio::select! {
            () = cancel.cancelled() => return PlanOutcome::Cancelled,
            event = stream.rx_event.recv() => event,
        };
        match event {
            Some(Ok(ResponseEvent::OutputTextDelta(delta))) => emit(text(&delta)),
            Some(Ok(ResponseEvent::Completed { .. })) | None => return PlanOutcome::Answered,
            Some(Ok(_)) => {}
            Some(Err(error)) if is_credential_rejection(&error) => return PlanOutcome::Rejected,
            Some(Err(error)) => return PlanOutcome::Failed(error.to_string()),
        }
    }
}

/// The server path, unchanged in substance from the pre-B2 adapter — plus the receipt.
async fn answer_via_server(
    http: &Client,
    repo: &Repo,
    question: &str,
    cancel: &CancellationToken,
    emit: &mut (dyn FnMut(ContentBlock) + Send),
) -> PromptAnswer {
    match http
        .deep_search(repo, &DeepSearchRequest::new(question.to_string()), cancel)
        .await
    {
        Ok(answer) => {
            for content in crate::answer_content(&answer) {
                emit(content);
            }
            emit(text(SERVER_RECEIPT));
            PromptAnswer::Answered
        }
        Err(estelle_client::Error::Cancelled) => PromptAnswer::Cancelled,
        Err(error) => PromptAnswer::Failed(error.to_string()),
    }
}

/// One prompt, one engine, one receipt.
pub(crate) async fn answer_prompt(
    engine: &Engine,
    http: &Client,
    repo: &Repo,
    model: Option<&ModelSelection>,
    question: &str,
    cancel: &CancellationToken,
    emit: &mut (dyn FnMut(ContentBlock) + Send),
) -> PromptAnswer {
    match engine {
        Engine::Server => answer_via_server(http, repo, question, cancel, emit).await,
        Engine::Local(local) => {
            match answer_via_plan(local, http, repo, model, question, cancel, emit).await {
                PlanOutcome::Answered => {
                    emit(text(LOCAL_RECEIPT));
                    PromptAnswer::Answered
                }
                PlanOutcome::Cancelled => PromptAnswer::Cancelled,
                PlanOutcome::Rejected => {
                    emit(text(FALLBACK_NOTICE));
                    answer_via_server(http, repo, question, cancel, emit).await
                }
                PlanOutcome::Failed(message) => PromptAnswer::Failed(message),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use agent_client_protocol::schema::v1::ContentBlock;
    use base64::Engine as Base64Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use codex_models_manager::bundled_models_response;
    use codex_protocol::openai_models::ModelInfo;
    use codex_protocol::openai_models::ModelVisibility;
    use codex_protocol::openai_models::ReasoningEffort;
    use estelle_client::ApiKey;
    use estelle_client::Client;
    use estelle_client::Repo;
    use serde_json::json;
    use serial_test::serial;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;

    fn jwt(payload: serde_json::Value) -> String {
        let encode = |value: &serde_json::Value| {
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).expect("jwt part"))
        };
        format!(
            "{}.{}.{}",
            encode(&json!({"alg": "none", "typ": "JWT"})),
            encode(&payload),
            URL_SAFE_NO_PAD.encode(b"sig")
        )
    }

    fn id_token(account: &str) -> String {
        jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": account}}))
    }

    fn access_token_expiring_at(exp: i64) -> String {
        jwt(json!({"exp": exp}))
    }

    fn write_chatgpt_auth(home: &Path, access_token: &str, refresh_token: &str) {
        // Mirrors persist_tokens_async in codex-login: account_id comes from the id_token's
        // chatgpt_account_id claim, and last_refresh is set (get_token_data refuses without it).
        let auth = codex_login::AuthDotJson {
            auth_mode: Some(codex_protocol::auth::AuthMode::Chatgpt),
            openai_api_key: None,
            tokens: Some(codex_login::token_data::TokenData {
                id_token: codex_login::token_data::parse_chatgpt_jwt_claims(&id_token("acct-1"))
                    .expect("id token claims"),
                access_token: access_token.to_string(),
                refresh_token: refresh_token.to_string(),
                account_id: Some("acct-1".to_string()),
            }),
            last_refresh: Some(chrono::Utc::now()),
            agent_identity: None,
            personal_access_token: None,
            bedrock_api_key: None,
        };
        codex_login::save_auth(
            home,
            &auth,
            codex_login::AuthCredentialsStoreMode::File,
            codex_login::AuthKeyringBackendKind::default(),
        )
        .expect("write auth.json");
    }

    fn valid_access_token() -> String {
        access_token_expiring_at(
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs()
                + 3_600) as i64,
        )
    }

    fn model_fixture(slug: &str, priority: i32, visibility: ModelVisibility) -> ModelInfo {
        let mut model = bundled_models_response()
            .expect("bundled catalog")
            .models
            .into_iter()
            .find(|model| model.visibility == ModelVisibility::List)
            .expect("a visible bundled model");
        model.slug = slug.to_string();
        model.priority = priority;
        model.visibility = visibility;
        model.default_reasoning_level = Some(ReasoningEffort::Low);
        model
    }

    fn sse_body(events: &[serde_json::Value]) -> String {
        events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {event}\n\n",
                    event["type"].as_str().expect("event type")
                )
            })
            .collect()
    }

    fn text_events() -> Vec<serde_json::Value> {
        vec![
            json!({"type": "response.created", "response": {"id": "resp-1"}}),
            json!({"type": "response.output_text.delta", "delta": "Hello"}),
            json!({"type": "response.output_text.delta", "delta": " back"}),
            json!({"type": "response.completed", "response": {"id": "resp-1"}}),
        ]
    }

    async fn mock_backend_models(server: &MockServer, models: Vec<ModelInfo>) {
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": models})))
            .mount(server)
            .await;
    }

    async fn mock_backend_responses(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body(&text_events())),
            )
            .mount(server)
            .await;
    }

    async fn mock_estelle(server: &MockServer, search_status: u16) {
        let search = ResponseTemplate::new(search_status);
        let search = if search_status == 200 {
            search.set_body_json(json!({"recall": "the retry policy lives in retry.rs"}))
        } else {
            search
        };
        Mock::given(method("POST"))
            .and(path("/search"))
            .respond_with(search)
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"answer": "server answer", "sources": []})),
            )
            .mount(server)
            .await;
    }

    fn estelle_client(server: &MockServer) -> Client {
        Client::new(
            &format!("{}/", server.uri()),
            ApiKey::new(format!("estelle_live_{}", "b".repeat(24))).expect("key"),
            Duration::from_secs(120),
        )
        .expect("estelle client")
    }

    fn emitted_texts(blocks: &[ContentBlock]) -> Vec<String> {
        blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect()
    }

    async fn answer(
        engine: &Engine,
        estelle: &MockServer,
        question: &str,
    ) -> (PromptAnswer, Vec<ContentBlock>) {
        let client = estelle_client(estelle);
        let model = match engine {
            Engine::Local(local) => Some(select_model(local).await),
            Engine::Server => None,
        };
        let mut blocks = Vec::new();
        let outcome = answer_prompt(
            engine,
            &client,
            &Repo::new("owner/repo").expect("repo"),
            model.as_ref(),
            question,
            &CancellationToken::new(),
            &mut |block| blocks.push(block),
        )
        .await;
        (outcome, blocks)
    }

    #[tokio::test]
    async fn server_path_when_no_credential_emits_the_server_receipt() {
        let home = tempdir().expect("home");
        let backend = MockServer::start().await;
        let estelle = MockServer::start().await;
        mock_estelle(&estelle, 200).await;

        let engine = Engine::resolve(Some(home.path().to_path_buf()), &backend.uri()).await;
        assert!(matches!(engine, Engine::Server));

        let (outcome, blocks) = answer(&engine, &estelle, "what retries?").await;

        assert!(matches!(outcome, PromptAnswer::Answered));
        let texts = emitted_texts(&blocks);
        assert!(texts.contains(&"server answer".to_string()), "{texts:?}");
        assert_eq!(texts.last(), Some(&SERVER_RECEIPT.to_string()));
        assert!(
            backend
                .received_requests()
                .await
                .expect("requests")
                .is_empty(),
            "the server path must not touch the ChatGPT backend"
        );
    }

    #[tokio::test]
    async fn local_path_calls_the_backend_with_the_plan_credential_and_catalog_body() {
        let home = tempdir().expect("home");
        write_chatgpt_auth(home.path(), &valid_access_token(), "refresh-1");
        let backend = MockServer::start().await;
        // A hidden model with the BEST priority number proves visibility wins over priority.
        mock_backend_models(
            &backend,
            vec![
                model_fixture("mock-hidden", 1, ModelVisibility::Hide),
                model_fixture("mock-best", 2, ModelVisibility::List),
                model_fixture("mock-worse", 5, ModelVisibility::List),
            ],
        )
        .await;
        mock_backend_responses(&backend).await;
        let estelle = MockServer::start().await;
        mock_estelle(&estelle, 200).await;

        let engine = Engine::resolve(Some(home.path().to_path_buf()), &backend.uri()).await;
        assert!(matches!(engine, Engine::Local(_)));
        let (outcome, blocks) = answer(&engine, &estelle, "what retries?").await;

        assert!(matches!(outcome, PromptAnswer::Answered));
        assert_eq!(
            emitted_texts(&blocks),
            vec![
                "Hello".to_string(),
                " back".to_string(),
                LOCAL_RECEIPT.to_string()
            ]
        );

        let requests = backend.received_requests().await.expect("requests");
        let models_request = requests
            .iter()
            .find(|request| request.url.path() == "/models")
            .expect("the model catalog was fetched");
        assert!(
            models_request
                .url
                .query()
                .unwrap_or_default()
                .contains("client_version="),
            "client_version query: {}",
            models_request.url
        );
        assert_eq!(
            models_request
                .headers
                .get("authorization")
                .expect("auth header"),
            &format!("Bearer {}", write_chatgpt_auth_token(home.path()))
        );
        let request = requests
            .iter()
            .find(|request| request.url.path() == "/responses")
            .expect("the model was called");
        assert_eq!(
            request.headers.get("authorization").expect("auth header"),
            &format!("Bearer {}", write_chatgpt_auth_token(home.path()))
        );
        assert_eq!(
            request
                .headers
                .get("chatgpt-account-id")
                .expect("account header"),
            "acct-1"
        );
        assert_eq!(
            request
                .headers
                .get("originator")
                .expect("originator header"),
            "codex_cli_rs"
        );
        assert!(
            request
                .headers
                .get("session-id")
                .is_some_and(|value| !value.is_empty()),
            "session-id header"
        );
        assert_eq!(
            request.headers.get("accept").expect("accept header"),
            "text/event-stream"
        );

        let body: serde_json::Value = request.body_json().expect("request body");
        assert_eq!(body["model"], json!("mock-best"));
        let expected = model_fixture("mock-best", 2, ModelVisibility::List);
        assert_eq!(
            body["instructions"],
            json!(expected.base_instructions),
            "instructions come from the model catalog"
        );
        assert_eq!(body["tool_choice"], json!("auto"));
        assert_eq!(body["parallel_tool_calls"], json!(false));
        assert_eq!(body["store"], json!(false));
        assert_eq!(body["stream"], json!(true));
        assert!(
            body["include"]
                .as_array()
                .expect("include")
                .contains(&json!("reasoning.encrypted_content"))
        );
        assert_eq!(
            body["reasoning"]["effort"],
            serde_json::to_value(ReasoningEffort::Low).expect("effort json")
        );
        let input = &body["input"];
        assert_eq!(
            input.as_array().expect("input").len(),
            1,
            "ONE user message"
        );
        assert_eq!(input[0]["role"], json!("user"));
        let text = input[0]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains(MEMORY_LABEL), "{text}");
        assert!(
            text.contains("the retry policy lives in retry.rs"),
            "{text}"
        );
        assert!(text.contains("what retries?"), "{text}");
        assert!(
            text.find(MEMORY_LABEL) < text.find("what retries?"),
            "the context block rides above the prompt: {text}"
        );
    }

    fn write_chatgpt_auth_token(home: &Path) -> String {
        codex_login::load_auth_dot_json(
            home,
            codex_login::AuthCredentialsStoreMode::File,
            codex_login::AuthKeyringBackendKind::default(),
        )
        .expect("auth loads")
        .and_then(|auth| auth.tokens)
        .expect("tokens")
        .access_token
    }

    /// Use sparingly (mirrors codex-login's EnvVarGuard).
    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.original {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    #[tokio::test]
    #[serial(chatgpt_refresh)]
    async fn expired_token_is_refreshed_and_the_rotated_refresh_token_is_persisted() {
        let authority = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id_token": id_token("acct-1"),
                "access_token": valid_access_token(),
                "refresh_token": "refresh-NEW"
            })))
            .mount(&authority)
            .await;
        let _guard = EnvVarGuard::set(
            codex_login::REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR,
            &format!("{}/oauth/token", authority.uri()),
        );
        let home = tempdir().expect("home");
        write_chatgpt_auth(
            home.path(),
            &access_token_expiring_at(1_000_000),
            "refresh-OLD",
        );

        let engine = Engine::resolve(Some(home.path().to_path_buf()), "http://127.0.0.1:1").await;

        assert!(
            matches!(engine, Engine::Local(_)),
            "the refreshed credential loads"
        );
        let stored = write_chatgpt_auth_token(home.path());
        assert_ne!(stored, access_token_expiring_at(1_000_000));
        let refresh = codex_login::load_auth_dot_json(
            home.path(),
            codex_login::AuthCredentialsStoreMode::File,
            codex_login::AuthKeyringBackendKind::default(),
        )
        .expect("auth loads")
        .and_then(|auth| auth.tokens)
        .expect("tokens")
        .refresh_token;
        assert_eq!(
            refresh, "refresh-NEW",
            "the rotated refresh token is persisted"
        );
        let refresh_calls = authority
            .received_requests()
            .await
            .expect("requests")
            .into_iter()
            .filter(|request| request.url.path() == "/oauth/token")
            .count();
        assert_eq!(refresh_calls, 1);
    }

    #[tokio::test]
    async fn backend_rejection_falls_back_to_the_server_with_a_named_notice() {
        let home = tempdir().expect("home");
        write_chatgpt_auth(home.path(), &valid_access_token(), "refresh-1");
        let backend = MockServer::start().await;
        mock_backend_models(
            &backend,
            vec![model_fixture("mock-best", 1, ModelVisibility::List)],
        )
        .await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "bad token"})))
            .mount(&backend)
            .await;
        let estelle = MockServer::start().await;
        mock_estelle(&estelle, 200).await;

        let engine = Engine::resolve(Some(home.path().to_path_buf()), &backend.uri()).await;
        let (outcome, blocks) = answer(&engine, &estelle, "what retries?").await;

        assert!(matches!(outcome, PromptAnswer::Answered));
        let texts = emitted_texts(&blocks);
        assert!(texts.contains(&FALLBACK_NOTICE.to_string()), "{texts:?}");
        assert!(texts.contains(&"server answer".to_string()), "{texts:?}");
        assert_eq!(texts.last(), Some(&SERVER_RECEIPT.to_string()));
    }

    #[tokio::test]
    async fn search_failure_still_calls_the_model_without_the_context_block() {
        let home = tempdir().expect("home");
        write_chatgpt_auth(home.path(), &valid_access_token(), "refresh-1");
        let backend = MockServer::start().await;
        mock_backend_models(
            &backend,
            vec![model_fixture("mock-best", 1, ModelVisibility::List)],
        )
        .await;
        mock_backend_responses(&backend).await;
        let estelle = MockServer::start().await;
        mock_estelle(&estelle, 500).await;

        let engine = Engine::resolve(Some(home.path().to_path_buf()), &backend.uri()).await;
        let (outcome, blocks) = answer(&engine, &estelle, "what retries?").await;

        assert!(matches!(outcome, PromptAnswer::Answered));
        assert_eq!(
            emitted_texts(&blocks).last(),
            Some(&LOCAL_RECEIPT.to_string())
        );
        let requests = backend.received_requests().await.expect("requests");
        let request = requests
            .iter()
            .find(|request| request.url.path() == "/responses")
            .expect("the model was called even with no memory");
        let body: serde_json::Value = request.body_json().expect("request body");
        let text = body["input"][0]["content"][0]["text"]
            .as_str()
            .expect("text");
        assert!(!text.contains(MEMORY_LABEL), "{text}");
        assert!(text.contains("what retries?"), "{text}");
    }

    #[tokio::test]
    async fn models_fetch_failure_falls_back_to_the_bundled_catalog() {
        let home = tempdir().expect("home");
        write_chatgpt_auth(home.path(), &valid_access_token(), "refresh-1");
        let backend = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&backend)
            .await;
        mock_backend_responses(&backend).await;
        let estelle = MockServer::start().await;
        mock_estelle(&estelle, 200).await;

        let engine = Engine::resolve(Some(home.path().to_path_buf()), &backend.uri()).await;
        let (outcome, _blocks) = answer(&engine, &estelle, "what retries?").await;

        assert!(matches!(outcome, PromptAnswer::Answered));
        let bundled = pick_model(&bundled_models_response().expect("catalog").models)
            .expect("a visible bundled model");
        let requests = backend.received_requests().await.expect("requests");
        let request = requests
            .iter()
            .find(|request| request.url.path() == "/responses")
            .expect("the model was called");
        let body: serde_json::Value = request.body_json().expect("request body");
        assert_eq!(body["model"], json!(bundled.slug));
        assert_eq!(body["instructions"], json!(bundled.instructions));
    }
}
