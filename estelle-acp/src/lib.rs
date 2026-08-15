//! ACP adapter for Estelle's server-owned agent.

#![deny(clippy::print_stderr, clippy::print_stdout)]

mod engine;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, Implementation,
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
    PromptResponse, ResourceLink, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent,
};
use agent_client_protocol::{Agent, Client as AcpClient, ConnectionTo, Dispatch, Stdio};
use estelle_client::{Client, DeepSearchResponse, Repo, RepoResolver, Source};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const ADAPTER_NAME: &str = "estelle";

/// One ACP session: its repo, and — on the local engine — the model chosen at session start.
#[derive(Clone)]
struct Session {
    repo: Repo,
    model: Option<engine::ModelSelection>,
}

#[derive(Clone)]
struct State {
    http: Client,
    engine: Arc<engine::Engine>,
    repo_override: Option<Repo>,
    sessions: Arc<Mutex<HashMap<SessionId, Session>>>,
    active: Arc<Mutex<HashMap<SessionId, CancellationToken>>>,
}

impl State {
    fn new(http: Client, engine: engine::Engine, repo_override: Option<Repo>) -> Self {
        Self {
            http,
            engine: Arc::new(engine),
            repo_override,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Serve Estelle as an ACP agent over stdin/stdout.
pub async fn run_stdio(
    http: Client,
    repo_override: Option<Repo>,
) -> Result<(), agent_client_protocol::Error> {
    // The engine decision is made once, up front: a loadable ChatGPT credential means the
    // user's own plan does the thinking; anything else keeps the server path unchanged.
    let engine =
        engine::Engine::resolve(engine::chatgpt_home(), engine::CHATGPT_BACKEND_BASE).await;
    let state = State::new(http, engine, repo_override);
    let new_session_state = state.clone();
    let prompt_state = state.clone();
    let cancel_state = state;

    Agent
        .builder()
        .name(ADAPTER_NAME)
        .on_receive_request(
            async move |request: InitializeRequest, responder, _connection| {
                responder.respond(initialize_response(request))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _connection| {
                if !request.mcp_servers.is_empty() || !request.additional_directories.is_empty() {
                    return responder.respond_with_error(agent_client_protocol::util::internal_error(
                        "Estelle ACP does not advertise client-provided MCP servers or additional directories",
                    ));
                }
                let Some(repo) = resolve_session_repo(&new_session_state.repo_override, request.cwd)
                else {
                    return responder.respond_with_error(agent_client_protocol::util::internal_error(
                        "the ACP working directory does not resolve to a repository",
                    ));
                };
                // The model slug is chosen at SESSION START: the backend's own /models list
                // with the user's credential, falling back to the bundled catalog.
                let model = match &*new_session_state.engine {
                    engine::Engine::Local(local) => Some(engine::select_model(local).await),
                    engine::Engine::Server => None,
                };
                let session_id = SessionId::new(Uuid::new_v4().to_string());
                lock_recover(&new_session_state.sessions)
                    .insert(session_id.clone(), Session { repo, model });
                responder.respond(NewSessionResponse::new(session_id))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, connection: ConnectionTo<AcpClient>| {
                let session_id = request.session_id.clone();
                let Some(session) = lock_recover(&prompt_state.sessions).get(&session_id).cloned()
                else {
                    return responder.respond_with_error(agent_client_protocol::util::internal_error(
                        "unknown Estelle ACP session",
                    ));
                };
                let question = match prompt_text(&request.prompt) {
                    Ok(question) => question,
                    Err(message) => {
                        return responder.respond_with_error(
                            agent_client_protocol::util::internal_error(message),
                        );
                    }
                };
                let Some(cancel) = begin_prompt(&prompt_state.active, &session_id) else {
                    return responder.respond_with_error(agent_client_protocol::util::internal_error(
                        "an Estelle ACP prompt is already active for this session",
                    ));
                };
                let request_cancel = responder.cancellation();
                let task_connection = connection.clone();
                let task_state = prompt_state.clone();
                connection.spawn(async move {
                    let mut send_error: Option<String> = None;
                    let notification_session = session_id.clone();
                    let outcome = {
                        let mut emit = |content: ContentBlock| {
                            if send_error.is_none()
                                && let Err(error) =
                                    task_connection.send_notification(SessionNotification::new(
                                        notification_session.clone(),
                                        SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                            content,
                                        )),
                                    ))
                            {
                                send_error = Some(error.to_string());
                            }
                        };
                        tokio::select! {
                            _ = request_cancel.cancelled() => {
                                cancel.cancel();
                                engine::PromptAnswer::Cancelled
                            }
                            outcome = engine::answer_prompt(
                                &task_state.engine,
                                &task_state.http,
                                &session.repo,
                                session.model.as_ref(),
                                &question,
                                &cancel,
                                &mut emit,
                            ) => outcome,
                        }
                    };
                    lock_recover(&task_state.active).remove(&session_id);
                    if let Some(error) = send_error {
                        return Err(agent_client_protocol::util::internal_error(error));
                    }

                    match outcome {
                        engine::PromptAnswer::Answered => {
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        engine::PromptAnswer::Cancelled => {
                            responder.respond(PromptResponse::new(StopReason::Cancelled))
                        }
                        engine::PromptAnswer::Failed(error) => responder.respond_with_error(
                            agent_client_protocol::util::internal_error(error),
                        ),
                    }
                })?;
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: CancelNotification, _connection| {
                if let Some(cancel) = lock_recover(&cancel_state.active)
                    .get(&notification.session_id)
                    .cloned()
                {
                    cancel.cancel();
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_dispatch(
            async move |message: Dispatch, connection: ConnectionTo<AcpClient>| {
                message.respond_with_error(
                    agent_client_protocol::util::internal_error(
                        "Estelle ACP received an unsupported method",
                    ),
                    connection,
                )
            },
            agent_client_protocol::on_receive_dispatch!(),
        )
        .connect_to(Stdio::new())
        .await
}

fn resolve_session_repo(
    override_repo: &Option<Repo>,
    cwd: impl Into<std::path::PathBuf>,
) -> Option<Repo> {
    RepoResolver::new(override_repo.clone(), cwd).resolve()
}

fn begin_prompt(
    active: &Mutex<HashMap<SessionId, CancellationToken>>,
    session_id: &SessionId,
) -> Option<CancellationToken> {
    let mut active = lock_recover(active);
    if active.contains_key(session_id) {
        return None;
    }
    let cancel = CancellationToken::new();
    active.insert(session_id.clone(), cancel.clone());
    Some(cancel)
}

fn initialize_response(request: InitializeRequest) -> InitializeResponse {
    InitializeResponse::new(request.protocol_version)
        .agent_capabilities(AgentCapabilities::new())
        .agent_info(Implementation::new(ADAPTER_NAME, env!("CARGO_PKG_VERSION")).title("Estelle"))
}

fn prompt_text(blocks: &[ContentBlock]) -> Result<String, &'static str> {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(text) => parts.push(text.text.clone()),
            ContentBlock::ResourceLink(link) => {
                parts.push(format!("Referenced resource: {} ({})", link.name, link.uri));
            }
            _ => return Err("Estelle ACP accepts only text and resource-link prompt blocks"),
        }
    }
    let joined = parts.join("\n");
    if joined.trim().is_empty() {
        Err("Estelle ACP received an empty prompt")
    } else {
        Ok(joined)
    }
}

fn answer_content(answer: &DeepSearchResponse) -> Vec<ContentBlock> {
    let text = answer
        .rendered_answer()
        .unwrap_or("Estelle returned no answer; no content was fabricated by the ACP adapter.");
    std::iter::once(ContentBlock::Text(TextContent::new(text)))
        .chain(answer.sources.iter().map(source_resource_link))
        .collect()
}

fn source_resource_link(source: &Source) -> ContentBlock {
    let encoded_path = source
        .file
        .split('/')
        .map(|segment| url::form_urlencoded::byte_serialize(segment.as_bytes()).collect::<String>())
        .collect::<Vec<_>>()
        .join("/");
    let mut uri = format!("estelle://repo/{encoded_path}");
    if let Some(line) = source.line {
        uri.push_str(&format!("?line={line}"));
    }
    let title = source.line.map_or_else(
        || source.file.clone(),
        |line| format!("{}:{line}", source.file),
    );
    ContentBlock::ResourceLink(
        ResourceLink::new(&source.file, uri)
            .title(title)
            .description("Grounding source returned by Estelle deep search"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::ProtocolVersion;
    use agent_client_protocol::schema::v1::ResourceLink;

    #[test]
    fn advertises_only_the_baseline_remote_agent_contract() {
        let response = initialize_response(InitializeRequest::new(ProtocolVersion::LATEST));

        assert!(!response.agent_capabilities.load_session);
        assert!(!response.agent_capabilities.prompt_capabilities.image);
        assert!(!response.agent_capabilities.prompt_capabilities.audio);
        assert!(
            !response
                .agent_capabilities
                .prompt_capabilities
                .embedded_context
        );
        assert!(!response.agent_capabilities.mcp_capabilities.http);
        assert!(!response.agent_capabilities.mcp_capabilities.sse);
        assert!(!response.agent_capabilities.mcp_capabilities.acp);
        assert!(response.auth_methods.is_empty());
    }

    #[test]
    fn converts_text_and_resource_links_without_reading_local_files() {
        let prompt = vec![
            ContentBlock::Text(TextContent::new("Where is this used?")),
            ContentBlock::ResourceLink(ResourceLink::new("charge.py", "file:///repo/charge.py")),
        ];

        assert_eq!(
            prompt_text(&prompt).unwrap(),
            "Where is this used?\nReferenced resource: charge.py (file:///repo/charge.py)"
        );
    }

    #[test]
    fn emits_deep_search_sources_as_acp_resource_links() {
        let answer: estelle_client::DeepSearchResponse =
            serde_json::from_value(serde_json::json!({
                "answer": "The retry is bounded.",
                "sources": [{"file": "api/charge.ts", "line": 52}]
            }))
            .expect("deep-search response");

        let blocks = answer_content(&answer);

        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[0],
            ContentBlock::Text(TextContent::new("The retry is bounded."))
        );
        let ContentBlock::ResourceLink(source) = &blocks[1] else {
            panic!("deep-search source was not emitted as an ACP resource link");
        };
        assert_eq!(source.name, "api/charge.ts");
        assert_eq!(source.title.as_deref(), Some("api/charge.ts:52"));
        assert_eq!(source.uri, "estelle://repo/api/charge.ts?line=52");
    }

    #[test]
    fn a_session_can_have_only_one_live_prompt_and_cancel_tracks_that_prompt() {
        let active = Mutex::new(HashMap::new());
        let session_id = SessionId::new("session-1".to_string());
        let first = begin_prompt(&active, &session_id).expect("first prompt starts");

        assert!(begin_prompt(&active, &session_id).is_none());
        lock_recover(&active)
            .get(&session_id)
            .expect("original prompt remains registered")
            .cancel();
        assert!(first.is_cancelled());
    }

    #[test]
    fn an_explicit_repo_overrides_even_an_invalid_client_cwd() {
        let expected = Repo::new("owner/repo").expect("repo");
        assert_eq!(
            resolve_session_repo(&Some(expected.clone()), "/definitely/not/a/repo"),
            Some(expected)
        );
    }
}
