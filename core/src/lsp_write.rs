//! Local LSP write-through for structured file mutations.
//!
//! The server observes the pre-write document before the filesystem mutation, then receives the
//! matching watched-file and document lifecycle notifications after the committed delta. Slow
//! diagnostics leave the tool's inline budget and return through the session event channel.
//!
//! Design ported from oh-my-pi's `packages/coding-agent/src/lsp/writethrough.ts` at
//! `37eee71978951fccf66b21f7e3e2b74596ac9d74`, Copyright (c) 2025 Mario Zechner and
//! Copyright (c) 2025-2026 Can Bölük, under the MIT License. This Rust port preserves Estelle's
//! independently implemented committed-delta and local/remote-environment boundaries.

use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use codex_apply_patch::AppliedPatchDelta;
use codex_apply_patch::AppliedPatchFileChange;
use codex_apply_patch::ApplyPatchAction;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::WarningEvent;
use codex_utils_path_uri::PathUri;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::Command;
use tokio::sync::mpsc;

const INLINE_WAIT: Duration = Duration::from_millis(500);
const DEFERRED_WAIT: Duration = Duration::from_secs(25);
const INITIAL_DIAGNOSTIC_SETTLE: Duration = Duration::from_millis(100);
const MAX_FRAME_BYTES: usize = 16_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeKind {
    Created = 1,
    Changed = 2,
    Deleted = 3,
}

#[derive(Clone, Debug)]
struct WrittenDocument {
    path: PathBuf,
    uri: String,
    language_id: &'static str,
    old_content: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ServerSpec {
    command: &'static str,
    args: &'static [&'static str],
}

impl ServerSpec {
    fn for_path(path: &Path) -> Option<(Self, &'static str)> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Some((
                Self {
                    command: "rust-analyzer",
                    args: &[],
                },
                "rust",
            )),
            "ts" => Some((
                Self {
                    command: "typescript-language-server",
                    args: &["--stdio"],
                },
                "typescript",
            )),
            "tsx" => Some((
                Self {
                    command: "typescript-language-server",
                    args: &["--stdio"],
                },
                "typescriptreact",
            )),
            "js" | "mjs" | "cjs" => Some((
                Self {
                    command: "typescript-language-server",
                    args: &["--stdio"],
                },
                "javascript",
            )),
            "jsx" => Some((
                Self {
                    command: "typescript-language-server",
                    args: &["--stdio"],
                },
                "javascriptreact",
            )),
            "py" | "pyi" => Some((
                Self {
                    command: "pyright-langserver",
                    args: &["--stdio"],
                },
                "python",
            )),
            "go" => Some((
                Self {
                    command: "gopls",
                    args: &[],
                },
                "go",
            )),
            _ => None,
        }
    }
}

/// LSP clients prepared against the pre-write filesystem state.
pub(crate) struct LspWriteThrough {
    clients: Vec<LspClient>,
}

impl LspWriteThrough {
    /// Start only local servers. A remote `PathUri` is never translated into a host process path.
    pub(crate) async fn prepare(action: &ApplyPatchAction, is_local: bool) -> Self {
        if !is_local {
            return Self {
                clients: Vec::new(),
            };
        }
        let root = action.cwd.to_path_buf();
        let mut grouped: BTreeMap<ServerSpec, Vec<WrittenDocument>> = BTreeMap::new();
        for (path_uri, change) in action.changes() {
            let path = path_uri.to_path_buf();
            let final_path = match change {
                codex_apply_patch::ApplyPatchFileChange::Update {
                    move_path: Some(path),
                    ..
                } => path.to_path_buf(),
                _ => path.clone(),
            };
            let Some((spec, language_id)) = ServerSpec::for_path(&final_path) else {
                continue;
            };
            let old_content = tokio::fs::read_to_string(&path).await.ok();
            let uri = PathUri::from_host_native_path(&final_path)
                .map(|path| path.to_string())
                .unwrap_or_else(|_| path_uri.to_string());
            grouped.entry(spec).or_default().push(WrittenDocument {
                path: final_path,
                uri,
                language_id,
                old_content,
            });
        }

        let mut clients = Vec::new();
        for (spec, documents) in grouped {
            match LspClient::start(&root, spec, documents).await {
                Ok(client) => clients.push(client),
                Err(error) => tracing::debug!(%error, "LSP write-through unavailable"),
            }
        }
        Self { clients }
    }

    pub(crate) async fn committed(
        mut self,
        delta: &AppliedPatchDelta,
        session: Arc<Session>,
        turn: Arc<TurnContext>,
    ) -> Option<String> {
        if self.clients.is_empty() || delta.is_empty() {
            return None;
        }
        let changes = committed_changes(delta);
        let mut pending = Vec::new();
        for mut client in self.clients.drain(..) {
            if let Err(error) = client.notify_committed(&changes).await {
                tracing::debug!(%error, "LSP post-write notification failed");
                continue;
            }
            pending.push(client);
        }
        if pending.is_empty() {
            return None;
        }

        let inline = collect_diagnostics(&mut pending, INLINE_WAIT).await;
        if !pending.is_empty() {
            tokio::spawn(async move {
                let late = collect_diagnostics(&mut pending, DEFERRED_WAIT).await;
                if late.is_empty() {
                    return;
                }
                session
                    .send_event(
                        turn.as_ref(),
                        EventMsg::Warning(WarningEvent {
                            message: format!("Deferred LSP diagnostics:\n{}", late.join("\n")),
                        }),
                    )
                    .await;
            });
        }
        (!inline.is_empty()).then(|| format!("LSP diagnostics:\n{}", inline.join("\n")))
    }
}

fn committed_changes(delta: &AppliedPatchDelta) -> HashMap<PathBuf, (ChangeKind, Option<String>)> {
    let mut result = HashMap::new();
    for applied in delta.changes() {
        let source = applied.path.to_path_buf();
        match &applied.change {
            AppliedPatchFileChange::Add { content, .. } => {
                result.insert(source, (ChangeKind::Created, Some(content.clone())));
            }
            AppliedPatchFileChange::Delete { .. } => {
                result.insert(source, (ChangeKind::Deleted, None));
            }
            AppliedPatchFileChange::Update {
                move_path,
                new_content,
                ..
            } => {
                if let Some(destination) = move_path {
                    result.insert(source, (ChangeKind::Deleted, None));
                    result.insert(
                        destination.to_path_buf(),
                        (ChangeKind::Created, Some(new_content.clone())),
                    );
                } else {
                    result.insert(source, (ChangeKind::Changed, Some(new_content.clone())));
                }
            }
        }
    }
    result
}

async fn collect_diagnostics(clients: &mut Vec<LspClient>, budget: Duration) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut summaries = Vec::new();
    let mut still_pending = Vec::new();
    for mut client in clients.drain(..) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, client.next_fresh_diagnostics()).await {
            Ok(Ok(summary)) => {
                summaries.push(summary);
                client.shutdown().await;
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "LSP diagnostic read failed");
                client.shutdown().await;
            }
            Err(_) => still_pending.push(client),
        }
    }
    *clients = still_pending;
    summaries
}

struct LspClient {
    child: Child,
    input: ChildStdin,
    messages: mpsc::UnboundedReceiver<Value>,
    next_id: AtomicU64,
    documents: Vec<WrittenDocument>,
    expected_versions: HashMap<String, i64>,
}

impl LspClient {
    async fn start(
        root: &Path,
        spec: ServerSpec,
        documents: Vec<WrittenDocument>,
    ) -> io::Result<Self> {
        let executable = which::which(spec.command)
            .map_err(|error| io::Error::new(io::ErrorKind::NotFound, error))?;
        let mut command = Command::new(executable);
        command.args(spec.args).current_dir(root).kill_on_drop(true);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        scrub_environment(&mut command);
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("language server stdin unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("language server stdout unavailable"))?;
        let (tx, messages) = mpsc::unbounded_channel();
        tokio::spawn(read_messages(output, tx));
        let mut client = Self {
            child,
            input,
            messages,
            next_id: AtomicU64::new(1),
            documents,
            expected_versions: HashMap::new(),
        };
        let root_uri = PathUri::from_host_native_path(root)?.to_string();
        let id = client
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "capabilities": {
                        "workspace": { "didChangeWatchedFiles": { "dynamicRegistration": false } },
                        "textDocument": { "publishDiagnostics": { "versionSupport": true } }
                    },
                    "clientInfo": { "name": "estelle", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
            .await?;
        client.wait_for_response(id, Duration::from_secs(5)).await?;
        client.notify("initialized", json!({})).await?;
        for document in client.documents.clone() {
            if let Some(content) = &document.old_content {
                client
                    .notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": document.uri,
                                "languageId": document.language_id,
                                "version": 1,
                                "text": content,
                            }
                        }),
                    )
                    .await?;
            }
        }
        tokio::time::sleep(INITIAL_DIAGNOSTIC_SETTLE).await;
        client.drain_pre_write().await?;
        Ok(client)
    }

    async fn notify_committed(
        &mut self,
        changes: &HashMap<PathBuf, (ChangeKind, Option<String>)>,
    ) -> io::Result<()> {
        let mut watched = Vec::new();
        for document in &self.documents {
            let Some((kind, _)) = changes.get(&document.path) else {
                continue;
            };
            watched.push(json!({ "uri": document.uri, "type": *kind as u8 }));
        }
        if !watched.is_empty() {
            self.notify(
                "workspace/didChangeWatchedFiles",
                json!({ "changes": watched }),
            )
            .await?;
        }
        for document in self.documents.clone() {
            let Some((kind, content)) = changes.get(&document.path) else {
                continue;
            };
            match (kind, content) {
                (ChangeKind::Created, Some(content)) if document.old_content.is_none() => {
                    self.expected_versions.insert(document.uri.clone(), 1);
                    self.notify(
                        "textDocument/didOpen",
                        json!({
                            "textDocument": {
                                "uri": document.uri,
                                "languageId": document.language_id,
                                "version": 1,
                                "text": content,
                            }
                        }),
                    )
                    .await?;
                }
                (ChangeKind::Changed | ChangeKind::Created, Some(content)) => {
                    self.expected_versions.insert(document.uri.clone(), 2);
                    self.notify(
                        "textDocument/didChange",
                        json!({
                            "textDocument": { "uri": document.uri, "version": 2 },
                            "contentChanges": [{ "text": content }]
                        }),
                    )
                    .await?;
                }
                (ChangeKind::Deleted, _) => {
                    self.expected_versions.insert(document.uri.clone(), 1);
                    self.notify(
                        "textDocument/didClose",
                        json!({
                            "textDocument": { "uri": document.uri }
                        }),
                    )
                    .await?;
                }
                _ => {}
            }
            if *kind != ChangeKind::Deleted {
                self.notify(
                    "textDocument/didSave",
                    json!({
                        "textDocument": { "uri": document.uri },
                        "text": content,
                    }),
                )
                .await?;
            }
        }
        if self.expected_versions.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no prepared LSP document was present in the committed delta",
            ));
        }
        Ok(())
    }

    async fn next_fresh_diagnostics(&mut self) -> io::Result<String> {
        let mut pending = self
            .expected_versions
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut summaries = BTreeMap::new();
        while let Some(message) = self.messages.recv().await {
            if message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                let Some(params) = message.get("params") else {
                    continue;
                };
                let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                let Some(expected) = self.expected_versions.get(uri).copied() else {
                    continue;
                };
                if !pending.contains(uri) {
                    continue;
                }
                if let Some(version) = params.get("version").and_then(Value::as_i64)
                    && version < expected
                {
                    continue;
                }
                let diagnostics = params
                    .get("diagnostics")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let summary = if diagnostics.is_empty() {
                    format!("{}: no issues (fresh version {expected})", display_uri(uri))
                } else {
                    diagnostics
                        .iter()
                        .take(20)
                        .map(|diagnostic| {
                            let message = diagnostic
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("diagnostic");
                            let line = diagnostic
                                .pointer("/range/start/line")
                                .and_then(Value::as_u64)
                                .map(|line| line + 1)
                                .unwrap_or(0);
                            format!("{}:{line}: {message}", display_uri(uri))
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                summaries.insert(uri.to_string(), summary);
                pending.remove(uri);
                if pending.is_empty() {
                    return Ok(summaries.into_values().collect::<Vec<_>>().join("\n"));
                }
            }
            self.answer_server_request(&message).await?;
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "language server exited before fresh diagnostics",
        ))
    }

    async fn drain_pre_write(&mut self) -> io::Result<()> {
        while let Ok(message) = self.messages.try_recv() {
            self.answer_server_request(&message).await?;
        }
        Ok(())
    }

    async fn wait_for_response(&mut self, id: u64, budget: Duration) -> io::Result<()> {
        tokio::time::timeout(budget, async {
            while let Some(message) = self.messages.recv().await {
                if message.get("id").and_then(Value::as_u64) == Some(id)
                    && (message.get("result").is_some() || message.get("error").is_some())
                {
                    if let Some(error) = message.get("error") {
                        return Err(io::Error::other(format!(
                            "language server initialize failed: {error}"
                        )));
                    }
                    return Ok(());
                }
                self.answer_server_request(&message).await?;
            }
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "language server exited during initialize",
            ))
        })
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "language server initialize timed out",
            )
        })?
    }

    async fn answer_server_request(&mut self, message: &Value) -> io::Result<()> {
        let Some(id) = message.get("id") else {
            return Ok(());
        };
        if message.get("method").is_none() {
            return Ok(());
        }
        let result =
            if message.get("method").and_then(Value::as_str) == Some("workspace/configuration") {
                json!([])
            } else {
                Value::Null
            };
        self.write(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await
    }

    async fn request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        Ok(id)
    }

    async fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
    }

    async fn write(&mut self, message: Value) -> io::Result<()> {
        let body = serde_json::to_vec(&message).map_err(io::Error::other)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.input.write_all(header.as_bytes()).await?;
        self.input.write_all(&body).await?;
        self.input.flush().await
    }

    async fn shutdown(mut self) {
        if let Ok(id) = self.request("shutdown", Value::Null).await {
            let _ = self.wait_for_response(id, Duration::from_secs(1)).await;
        }
        let _ = self.notify("exit", Value::Null).await;
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

fn scrub_environment(command: &mut Command) {
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "TEMP",
        "TMP",
        "SHELL",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "RUSTC_WRAPPER",
        "NODE_PATH",
        "VIRTUAL_ENV",
    ];
    let values = ALLOWED
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (*name, value)))
        .collect::<Vec<_>>();
    command.env_clear();
    command.envs(values);
}

fn display_uri(uri: &str) -> &str {
    uri.strip_prefix("file://").unwrap_or(uri)
}

async fn read_messages(output: impl AsyncRead + Unpin, tx: mpsc::UnboundedSender<Value>) {
    let mut reader = BufReader::new(output);
    loop {
        match read_message(&mut reader).await {
            Ok(Some(message)) => {
                if tx.send(message).is_err() {
                    return;
                }
            }
            _ => return,
        }
    }
}

async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP frame missing Content-Length",
        )
    })?;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP frame exceeds 16MB bound",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn frame_reader_accepts_content_length_and_json() {
        let (mut writer, reader) = tokio::io::duplex(512);
        let body = br#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics"}"#;
        writer
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .await
            .unwrap();
        writer.write_all(body).await.unwrap();
        let message = read_message(&mut BufReader::new(reader))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(message["method"], "textDocument/publishDiagnostics");
    }

    #[tokio::test]
    async fn frame_reader_rejects_oversized_server_message() {
        let (mut writer, reader) = tokio::io::duplex(128);
        writer
            .write_all(b"Content-Length: 16000001\r\n\r\n")
            .await
            .unwrap();
        let error = read_message(&mut BufReader::new(reader)).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn server_selection_is_language_specific_and_closed_for_unknown_files() {
        assert_eq!(
            ServerSpec::for_path(Path::new("src/main.rs")).unwrap().1,
            "rust"
        );
        assert_eq!(
            ServerSpec::for_path(Path::new("src/main.tsx")).unwrap().1,
            "typescriptreact"
        );
        assert!(ServerSpec::for_path(Path::new("README.md")).is_none());
    }

    #[test]
    fn server_process_environment_does_not_inherit_credentials() {
        let mut command = Command::new("ignored");
        scrub_environment(&mut command);
        let debug = format!("{command:?}");
        assert!(!debug.contains("ESTELLE_RECEIPT_API_KEY"));
        assert!(!debug.contains("OPENAI_API_KEY"));
    }

    #[tokio::test]
    async fn remote_write_never_starts_a_local_language_server() {
        let directory = tempfile::tempdir().unwrap();
        let path = PathUri::from_host_native_path(directory.path().join("main.rs")).unwrap();
        let action = ApplyPatchAction::new_add_for_test(&path, "fn main() {}".to_string());
        let prepared = LspWriteThrough::prepare(&action, false).await;
        assert!(prepared.clients.is_empty());
    }

    const FAKE_SERVER: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/fake_lsp_server.py"
    );
    const FAKE_ARGS: &[&str] = &[FAKE_SERVER];

    async fn fake_client(file_name: &str) -> (tempfile::TempDir, LspClient, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(file_name);
        tokio::fs::write(&path, "fn before() {}\n").await.unwrap();
        let uri = PathUri::from_host_native_path(&path).unwrap().to_string();
        let client = LspClient::start(
            directory.path(),
            ServerSpec {
                command: "python3",
                args: FAKE_ARGS,
            },
            vec![WrittenDocument {
                path: path.clone(),
                uri,
                language_id: "rust",
                old_content: Some("fn before() {}\n".to_string()),
            }],
        )
        .await
        .unwrap();
        (directory, client, path)
    }

    #[tokio::test]
    async fn watched_save_path_rejects_stale_diagnostics_and_accepts_fresh_version() {
        let (_directory, mut client, path) = fake_client("fast.rs").await;
        let changes = HashMap::from([(
            path,
            (ChangeKind::Changed, Some("fn after() {}\n".to_string())),
        )]);
        client.notify_committed(&changes).await.unwrap();
        let summary = tokio::time::timeout(Duration::from_secs(2), client.next_fresh_diagnostics())
            .await
            .expect("fresh version 2 diagnostic never arrived")
            .unwrap();
        assert!(summary.contains(
            "fresh:workspace/didChangeWatchedFiles,textDocument/didChange,textDocument/didSave"
        ));
        assert!(!summary.contains("stale diagnostic must be ignored"));
        client.shutdown().await;
    }

    #[tokio::test]
    async fn slow_diagnostics_outlive_inline_budget_for_deferred_delivery() {
        let (_directory, mut client, path) = fake_client("slow.rs").await;
        let changes = HashMap::from([(
            path,
            (ChangeKind::Changed, Some("fn after() {}\n".to_string())),
        )]);
        client.notify_committed(&changes).await.unwrap();
        assert!(
            tokio::time::timeout(INLINE_WAIT, client.next_fresh_diagnostics())
                .await
                .is_err()
        );
        let late = tokio::time::timeout(DEFERRED_WAIT, client.next_fresh_diagnostics())
            .await
            .unwrap()
            .unwrap();
        assert!(late.contains("fresh:"));
        client.shutdown().await;
    }
}
