//! Maintained MCP transports around Estelle's server-owned tool catalog.

#![deny(clippy::print_stderr, clippy::print_stdout)]

use std::ffi::OsString;
use std::process::Stdio;

use estelle_client::{Client, Endpoint, Repo};
use rmcp::ErrorData as McpError;
use rmcp::ServiceExt;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::transport::child_process::TokioChildProcess;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct RemoteServer {
    client: Client,
    repo: Repo,
    cancel: CancellationToken,
}

impl RemoteServer {
    fn new(client: Client, repo: Repo) -> Self {
        Self {
            client,
            repo,
            cancel: CancellationToken::new(),
        }
    }

    async fn rpc<R>(&self, method: &str, params: Value) -> Result<R, McpError>
    where
        R: serde::de::DeserializeOwned,
    {
        let response: Value = self
            .client
            .post(
                Endpoint::Mcp,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": method,
                    "params": params,
                }),
                &self.cancel,
            )
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        decode_rpc_result(response)
    }
}

impl ServerHandler for RemoteServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("estelle", env!("CARGO_PKG_VERSION")).with_title("Estelle"),
            )
            .with_instructions(
                "Estelle tools execute on api.fatelabs.ca; this process is a transport adapter.",
            )
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.rpc(
            "tools/list",
            serde_json::to_value(request).unwrap_or(Value::Null),
        )
        .await
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        inject_repo(&mut request, &self.repo);
        self.rpc::<CallToolResult>(
            "tools/call",
            serde_json::to_value(request)
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
        )
        .await
        .map(Into::into)
    }
}

/// Serve Estelle's remote MCP catalog to an external harness over stdio.
pub async fn serve_stdio(
    client: Client,
    repo: Repo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let running = RemoteServer::new(client, repo)
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    running.waiting().await?;
    Ok(())
}

/// Connect to an external stdio MCP server and return its advertised tool names.
pub async fn inspect_stdio(
    command: Vec<OsString>,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let (program, arguments) = command
        .split_first()
        .ok_or("an MCP server command is required after --")?;
    let mut child = Command::new(program);
    child
        .args(arguments)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let transport = TokioChildProcess::new(child)?;
    let service = ().serve(transport).await?;
    let result = service.list_tools(None).await?;
    let names = result
        .tools
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    service.cancel().await?;
    Ok(names)
}

/// Connect to an external stdio MCP server and invoke one advertised tool.
pub async fn call_stdio(
    command: Vec<OsString>,
    tool: String,
    arguments: serde_json::Map<String, Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let (program, child_arguments) = command
        .split_first()
        .ok_or("an MCP server command is required after --")?;
    let mut child = Command::new(program);
    child
        .args(child_arguments)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let transport = TokioChildProcess::new(child)?;
    let service = ().serve(transport).await?;
    let result = service
        .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
        .await?;
    let value = serde_json::to_value(result)?;
    service.cancel().await?;
    Ok(value)
}

fn inject_repo(request: &mut CallToolRequestParams, repo: &Repo) {
    let tool = request.name.as_ref();
    let arguments = request.arguments.get_or_insert_with(Default::default);
    let Some(Value::String(raw)) = arguments.get("args").cloned() else {
        arguments.insert("repo".to_string(), Value::String(repo.as_str().to_string()));
        return;
    };

    let mut payload = serde_json::from_str::<serde_json::Map<String, Value>>(&raw).ok();
    if payload.is_none() {
        payload = launch_scoped_plain_args(tool, raw);
    }
    let Some(mut payload) = payload else {
        // The remote contract accepts structured arguments without the advertised `args` wrapper. Keep
        // the legacy sibling for tools whose free-text shape has no lossless structured equivalent.
        arguments.insert("repo".to_string(), Value::String(repo.as_str().to_string()));
        return;
    };
    payload.insert("repo".to_string(), Value::String(repo.as_str().to_string()));
    arguments.insert(
        "args".to_string(),
        Value::String(Value::Object(payload).to_string()),
    );
    arguments.remove("repo");
}

fn launch_scoped_plain_args(tool: &str, raw: String) -> Option<serde_json::Map<String, Value>> {
    let payload_key = match tool {
        "find_definition" | "find_usages" => Some("symbol"),
        "find_references" | "blast_radius" => Some("file"),
        "locate" => Some("query"),
        "dependency_path" => Some("q"),
        "verify" => Some("code"),
        "estelle_resume" => Some("session_id"),
        "research_ask" => Some("question"),
        "import_cycles" | "core_files" | "chokepoints" | "subsystems" | "refactor_order" => None,
        _ => return None,
    };
    let mut payload = serde_json::Map::new();
    if let Some(key) = payload_key {
        payload.insert(key.to_string(), Value::String(raw));
    }
    Some(payload)
}

fn decode_rpc_result<R>(response: Value) -> Result<R, McpError>
where
    R: serde::de::DeserializeOwned,
{
    if let Some(error) = response.get("error") {
        return Err(McpError::internal_error(error.to_string(), None));
    }
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| McpError::internal_error("Estelle MCP response omitted result", None))?;
    serde_json::from_value(result)
        .map_err(|error| McpError::internal_error(error.to_string(), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_calls_always_carry_the_launch_repo() {
        let repo = Repo::new("uqeu/estelle").unwrap();
        let mut missing = CallToolRequestParams::new("find_definition")
            .with_arguments(serde_json::from_value(json!({"args": "handle_mcp"})).unwrap());
        inject_repo(&mut missing, &repo);
        let missing_args = missing.arguments.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(missing_args["args"].as_str().unwrap()).unwrap(),
            json!({"symbol": "handle_mcp", "repo": "uqeu/estelle"})
        );

        let mut explicit = CallToolRequestParams::new("gate").with_arguments(
            serde_json::from_value(json!({
                "args": "{\"diff\":\"--- a/x\\n+++ b/x\",\"repo\":\"owner/other\"}"
            }))
            .unwrap(),
        );
        inject_repo(&mut explicit, &repo);
        let explicit_args = explicit.arguments.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(explicit_args["args"].as_str().unwrap()).unwrap(),
            json!({"diff": "--- a/x\n+++ b/x", "repo": "uqeu/estelle"})
        );

        let mut repo_only = CallToolRequestParams::new("import_cycles")
            .with_arguments(serde_json::from_value(json!({"args": ""})).unwrap());
        inject_repo(&mut repo_only, &repo);
        let repo_only_args = repo_only.arguments.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(repo_only_args["args"].as_str().unwrap()).unwrap(),
            json!({"repo": "uqeu/estelle"})
        );
    }

    #[test]
    fn rejects_non_result_mcp_envelopes() {
        let error =
            decode_rpc_result::<ListToolsResult>(json!({"jsonrpc": "2.0", "id": 1})).unwrap_err();
        assert!(error.message.contains("omitted result"));
    }
}
