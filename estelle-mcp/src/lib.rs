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
    request
        .arguments
        .get_or_insert_with(Default::default)
        .insert("repo".to_string(), Value::String(repo.as_str().to_string()));
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
        let mut missing = CallToolRequestParams::new("search");
        inject_repo(&mut missing, &repo);
        assert_eq!(
            missing.arguments.unwrap().get("repo"),
            Some(&json!("uqeu/estelle"))
        );

        let mut explicit = CallToolRequestParams::new("search")
            .with_arguments(serde_json::from_value(json!({"repo": "owner/other"})).unwrap());
        inject_repo(&mut explicit, &repo);
        assert_eq!(
            explicit.arguments.unwrap().get("repo"),
            Some(&json!("uqeu/estelle"))
        );
    }

    #[test]
    fn rejects_non_result_mcp_envelopes() {
        let error =
            decode_rpc_result::<ListToolsResult>(json!({"jsonrpc": "2.0", "id": 1})).unwrap_err();
        assert!(error.message.contains("omitted result"));
    }
}
