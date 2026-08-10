use std::collections::BTreeSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Duration;
use std::time::Instant;

use estelle_client::ChatCompletionRequest;
use estelle_client::ChatCompletionResponse;
use estelle_client::Client;
use estelle_client::CommandReply;
use estelle_client::CredentialStore;
use estelle_client::Endpoint;
use estelle_client::Error;
use estelle_client::Repo;
use estelle_client::is_secret_shaped;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::Command;
use crate::commands;

const SOURCE_EXTENSIONS: &[&str] = &[
    "c", "cpp", "cs", "go", "h", "hpp", "java", "js", "jsx", "kt", "md", "php", "py", "rb", "rs",
    "scala", "swift", "ts", "tsx",
];
const GITHUB_LOOPBACK_PORT: u16 = 8788;
const GITHUB_CALLBACK_PATH: &str = "/github/callback";
const GITHUB_CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
const SYNC_MAX_FILES: usize = 200;
const INGEST_POLL_INTERVAL: Duration = Duration::from_secs(2);
const INGEST_MAX_POLLS: usize = 7_200;
const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Contract {
    Local,
    Remote,
    Compound,
}

#[cfg(test)]
fn contract(command: &Command) -> Contract {
    match command {
        Command::Login
        | Command::Connect { .. }
        | Command::Remove
        | Command::Hook { .. }
        | Command::InstallHooks
        | Command::UninstallHooks
        | Command::Acp
        | Command::Mcp { .. }
        | Command::McpServer => Contract::Local,
        Command::Init { .. }
        | Command::Sweep { .. }
        | Command::Reindex { .. }
        | Command::Github { .. }
        | Command::Research { .. } => Contract::Compound,
        Command::Monitor { .. }
        | Command::Memory { .. }
        | Command::Ask { .. }
        | Command::Recall { .. }
        | Command::Verify { .. }
        | Command::Gate { .. } => Contract::Remote,
    }
}

pub(crate) async fn run(command: Command, repo: Repo, root: &Path) -> Result<Vec<String>, String> {
    match command {
        Command::Login => Err("login is handled by the credential reader".to_string()),
        Command::Connect { client } => Ok(connect_lines(client.as_deref().unwrap_or("cursor"))),
        Command::Remove => remove_editor_configs(root),
        Command::Hook { mode } => run_hook(mode.as_deref().unwrap_or("ground"), &repo, root).await,
        Command::InstallHooks => install_hooks(),
        Command::UninstallHooks => uninstall_hooks(),
        Command::Acp => Err("ACP is handled by the protocol runtime".to_string()),
        Command::Mcp { .. } | Command::McpServer => {
            Err("MCP is handled by the protocol runtime".to_string())
        }
        command => {
            let api = Api::resolve()?;
            run_authenticated(command, repo, root, &api).await
        }
    }
}

#[derive(Debug, Deserialize)]
struct HookPayload {
    #[serde(default)]
    tool_input: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GroundKind {
    Unreachable,
    Unverified,
    Flagged,
    Clean,
}

#[derive(Debug, Eq, PartialEq)]
struct GroundVerdict {
    kind: GroundKind,
    detail: String,
}

async fn run_hook(mode: &str, repo: &Repo, root: &Path) -> Result<Vec<String>, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("could not read hook input: {error}"))?;
    let Ok(payload) = serde_json::from_str::<HookPayload>(&input) else {
        return Ok(Vec::new());
    };
    match mode {
        "ground" => ground_hook(&payload, repo).await,
        "sync" => sync_hook(&payload, repo, root).await,
        _ => Err(format!(
            "unknown hook mode {mode:?}; expected ground or sync"
        )),
    }
}

async fn ground_hook(payload: &HookPayload, repo: &Repo) -> Result<Vec<String>, String> {
    let (path, code) = edited_file(payload);
    if !path.ends_with(".py") || code.trim().is_empty() {
        return Ok(Vec::new());
    }
    let name = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path.as_str());
    let api = match Api::resolve() {
        Ok(api) => api,
        Err(error) => {
            return Ok(vec![hook_message(
                format!("Estelle UNREACHABLE - {name} was NOT grounded: {error}"),
                None,
            )]);
        }
    };
    let report = match api
        .post_scoped(Endpoint::Verify, repo, &json!({"answer": code}))
        .await
    {
        Ok(report) => report,
        Err(error) => {
            return Ok(vec![hook_message(
                format!("Estelle UNREACHABLE - {name} was NOT grounded: {error}"),
                None,
            )]);
        }
    };
    let verdict = ground_verdict(Some(&report));
    let (message, context) = match verdict.kind {
        GroundKind::Unreachable => (
            format!(
                "Estelle UNREACHABLE - {name} was NOT grounded: {}",
                verdict.detail
            ),
            None,
        ),
        GroundKind::Unverified => (
            format!("Estelle ABSTAINED on {name}: {}", verdict.detail),
            Some(format!(
                "Estelle's grounding gate ABSTAINED on this edit to {path}: {}. This is not a pass; no symbol in this edit was certified.",
                verdict.detail
            )),
        ),
        GroundKind::Flagged => (
            format!("Estelle FLAGGED {name}: {}", verdict.detail),
            Some(format!(
                "Estelle's grounding gate FLAGGED this edit to {path}: {}. The finding is advisory because the server does not yet attest index freshness.",
                verdict.detail
            )),
        ),
        GroundKind::Clean => (
            format!("Estelle PASSED {name}: grounded against {repo}."),
            None,
        ),
    };
    Ok(vec![hook_message(message, context)])
}

async fn sync_hook(payload: &HookPayload, repo: &Repo, root: &Path) -> Result<Vec<String>, String> {
    let (path, _) = edited_file(payload);
    if path.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(reason) = hook_sync_refusal(&path, "")
        && reason == "not an indexable file type"
    {
        return Ok(Vec::new());
    }
    let (files, skipped) = collect_files(root, &[PathBuf::from(&path)])?;
    if files.is_empty() {
        let reason = skipped
            .first()
            .map(String::as_str)
            .unwrap_or("the file was not readable");
        return Ok(vec![hook_message(
            format!("Estelle did not reindex {path}: {reason}."),
            None,
        )]);
    }
    let api = match Api::resolve() {
        Ok(api) => api,
        Err(error) => {
            return Ok(vec![hook_message(
                format!("Estelle did not reindex {path}: {error}."),
                None,
            )]);
        }
    };
    match api
        .post_scoped(Endpoint::Reindex, repo, &json!({"files": files}))
        .await
    {
        Ok(_) => Ok(Vec::new()),
        Err(error) => Ok(vec![hook_message(
            format!("Estelle did not reindex {path}: {error}."),
            None,
        )]),
    }
}

fn edited_file(payload: &HookPayload) -> (String, String) {
    let path = payload
        .tool_input
        .get("file_path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let code = payload
        .tool_input
        .get("content")
        .or_else(|| payload.tool_input.get("new_string"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    (path, code)
}

fn hook_message(message: String, context: Option<String>) -> String {
    let mut output = json!({"systemMessage": message});
    if let Some(context) = context {
        output["hookSpecificOutput"] = json!({
            "hookEventName": "PreToolUse",
            "additionalContext": context,
        });
    }
    serde_json::to_string(&output).unwrap_or_else(|_| {
        "{\"systemMessage\":\"Estelle hook output could not be encoded\"}".to_string()
    })
}

fn ground_verdict(report: Option<&Value>) -> GroundVerdict {
    let Some(report) = report else {
        return GroundVerdict {
            kind: GroundKind::Unreachable,
            detail: "unreachable".to_string(),
        };
    };
    if let Some(error) = report.get("error").filter(|value| json_truthy(value)) {
        let why = match error {
            Value::Object(fields) => fields
                .get("message")
                .or_else(|| fields.get("detail"))
                .map(finding_text)
                .unwrap_or_else(|| "refused".to_string()),
            Value::Array(_) => "refused".to_string(),
            value => finding_text(value),
        };
        return GroundVerdict {
            kind: GroundKind::Unreachable,
            detail: format!("could not verify ({why})"),
        };
    }
    if let Some(reason) = report
        .get("unverified_reason")
        .filter(|value| json_truthy(value))
    {
        return GroundVerdict {
            kind: GroundKind::Unverified,
            detail: finding_text(reason),
        };
    }
    let labels = [
        ("ungrounded", "not defined in this repo"),
        ("arity_errors", "signature mismatch"),
        ("type_errors", "type error"),
        ("third_party", "invented library API"),
    ];
    let problems = labels
        .into_iter()
        .filter_map(|(field, label)| {
            let values = finding_values(report.get(field)?);
            (!values.is_empty()).then(|| {
                format!(
                    "{label}: {}",
                    values.into_iter().take(5).collect::<Vec<_>>().join(", ")
                )
            })
        })
        .collect::<Vec<_>>();
    if !problems.is_empty() {
        return GroundVerdict {
            kind: GroundKind::Flagged,
            detail: problems.join(" · "),
        };
    }
    if report.get("grounded") == Some(&Value::Bool(true)) {
        return GroundVerdict {
            kind: GroundKind::Clean,
            detail: String::new(),
        };
    }
    let detail = report
        .get("reason")
        .filter(|value| json_truthy(value))
        .map(|reason| format!("the gate did not certify — {}", finding_text(reason)))
        .unwrap_or_else(|| "the gate did not certify and gave no reason".to_string());
    GroundVerdict {
        kind: GroundKind::Unverified,
        detail,
    }
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_none_or(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn finding_values(value: &Value) -> Vec<String> {
    if !json_truthy(value) {
        return Vec::new();
    }
    match value {
        Value::Array(values) => values.iter().map(finding_text).collect(),
        value => vec![finding_text(value)],
    }
}

fn finding_text(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| serde_json::to_string(value).unwrap_or_else(|_| value.to_string()))
}

fn hook_sync_refusal(path: &str, content: &str) -> Option<&'static str> {
    const EXTENSIONS: &[&str] = &["py", "md", "ts", "js", "tsx", "jsx", "go", "rs"];
    let indexable = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension));
    if !indexable {
        return Some("not an indexable file type");
    }
    is_secret_shaped(content).then_some("contains something shaped like a live credential")
}

fn install_hooks() -> Result<Vec<String>, String> {
    let claude_path = claude_settings_path()?;
    let codex_path = codex_hooks_path()?;
    let runner = std::env::current_exe().map_err(|error| error.to_string())?;
    let runner = shell_command_path(&runner);
    install_hooks_at(&claude_path, &runner)?;
    install_hooks_at(&codex_path, &runner)?;
    Ok(vec![
        "Estelle PreToolUse and PostToolUse hooks installed.".to_string(),
        format!("Claude Code settings: {}", claude_path.display()),
        format!("Codex hooks: {}", codex_path.display()),
        "Existing settings and non-Estelle hooks were preserved.".to_string(),
    ])
}

fn uninstall_hooks() -> Result<Vec<String>, String> {
    let paths = [claude_settings_path()?, codex_hooks_path()?];
    if paths.iter().all(|path| !path.exists()) {
        return Ok(vec![
            "No Claude Code or Codex hook settings exist; nothing changed.".to_string(),
        ]);
    }
    let mut removed = false;
    for path in paths.iter().filter(|path| path.exists()) {
        removed |= uninstall_hooks_at(path)?;
    }
    Ok(vec![if removed {
        "Estelle hooks removed. Existing settings and non-Estelle hooks were preserved.".to_string()
    } else {
        "No Estelle hooks were installed; nothing changed.".to_string()
    }])
}

fn claude_settings_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|home| home.join(".claude/settings.json"))
        .ok_or_else(|| "could not locate the home directory for Claude Code settings".to_string())
}

fn codex_hooks_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path).join("hooks.json"));
    }
    dirs::home_dir()
        .map(|home| home.join(".codex/hooks.json"))
        .ok_or_else(|| "could not locate CODEX_HOME for Codex hooks".to_string())
}

fn shell_command_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "/._-".contains(character))
    {
        value.into_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn install_hooks_at(path: &Path, runner: &str) -> Result<(), String> {
    let existed = path.exists();
    let mut settings = read_json_object_or_empty(path)?;
    merge_estelle_hooks(&mut settings, runner)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if existed {
        fs::copy(path, backup_path(path)).map_err(|error| error.to_string())?;
    }
    write_json_0600(path, &settings)
}

fn uninstall_hooks_at(path: &Path) -> Result<bool, String> {
    let mut settings = read_json_object_or_empty(path)?;
    let removed = remove_estelle_hooks(&mut settings)?;
    if removed {
        fs::copy(path, backup_path(path)).map_err(|error| error.to_string())?;
        write_json_0600(path, &settings)?;
    }
    Ok(removed)
}

fn read_json_object_or_empty(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "refusing to overwrite unreadable {}: {error}",
            path.display()
        )
    })?;
    if !value.is_object() {
        return Err(format!(
            "refusing to overwrite {}: root is not an object",
            path.display()
        ));
    }
    Ok(value)
}

fn estelle_hook_groups(runner: &str) -> [(String, Value); 2] {
    [
        (
            "PreToolUse".to_string(),
            json!({
                "matcher": "Write|Edit",
                "hooks": [{
                    "type": "command",
                    "command": format!("{runner} hook ground"),
                    "timeout": 180,
                    "statusMessage": "Estelle grounding",
                }],
            }),
        ),
        (
            "PostToolUse".to_string(),
            json!({
                "matcher": "Write|Edit",
                "hooks": [{
                    "type": "command",
                    "command": format!("{runner} hook sync"),
                    "timeout": 180,
                    "async": true,
                    "statusMessage": "Estelle reindexing",
                }],
            }),
        ),
    ]
}

fn merge_estelle_hooks(settings: &mut Value, runner: &str) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings root is not an object".to_string())?;
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            "refusing to replace settings.hooks because it is not an object".to_string()
        })?;
    for (event, ours) in estelle_hook_groups(runner) {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                "refusing to replace a hook event because it is not an array".to_string()
            })?;
        groups.retain(|group| !is_estelle_hook(group));
        groups.push(ours);
    }
    Ok(())
}

fn remove_estelle_hooks(settings: &mut Value) -> Result<bool, String> {
    let Some(root) = settings.as_object_mut() else {
        return Err("settings root is not an object".to_string());
    };
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks.as_object_mut().ok_or_else(|| {
        "refusing to replace settings.hooks because it is not an object".to_string()
    })?;
    let mut removed = false;
    hooks.retain(|_, groups| {
        let Some(groups) = groups.as_array_mut() else {
            return true;
        };
        let before = groups.len();
        groups.retain(|group| !is_estelle_hook(group));
        removed |= groups.len() != before;
        !groups.is_empty()
    });
    if hooks.is_empty() {
        root.remove("hooks");
    }
    Ok(removed)
}

fn is_estelle_hook(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hook| hook.get("command").and_then(Value::as_str))
        .any(|command| {
            ["ground", "sync", "guard", "distil", "checkpoint", "welcome"]
                .iter()
                .any(|mode| command.contains(&format!(" hook {mode}")))
        })
}

struct Api {
    client: Client,
    api_key: estelle_client::ApiKey,
    cancel: CancellationToken,
}

impl Api {
    fn resolve() -> Result<Self, String> {
        let store = CredentialStore::default_location().map_err(|error| error.to_string())?;
        let credential = store.resolve().map_err(|error| error.to_string())?;
        let api_key = credential.api_key;
        let client = Client::production(api_key.clone()).map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            api_key,
            cancel: CancellationToken::new(),
        })
    }

    /// A headless command is one-shot — no cross-route evidence can accumulate, so a rejection
    /// NEVER deletes the credential here. The route is named so the user can tell route scope
    /// from a bad key.
    fn finish<T>(&self, result: Result<T, Error>, route: &str) -> Result<T, String> {
        result.map_err(|error| {
            if error.is_explicit_auth_rejection() {
                format!(
                    "{error} — the stored credential was rejected on {route} and was NOT removed; a single rejection can be route scope, not a bad key. Run estelle login only if you revoked it."
                )
            } else {
                error.to_string()
            }
        })
    }

    async fn get(&self, endpoint: Endpoint, query: &Value) -> Result<Value, String> {
        let result = self.client.get(endpoint, query, &self.cancel).await;
        self.finish(result, endpoint.path())
    }

    async fn get_scoped(
        &self,
        endpoint: Endpoint,
        repo: &Repo,
        query: &Value,
    ) -> Result<Value, String> {
        let result = self
            .client
            .get_scoped(endpoint, repo, query, &self.cancel)
            .await;
        self.finish(result, endpoint.path())
    }

    async fn post(&self, endpoint: Endpoint, body: &Value) -> Result<Value, String> {
        let result = self.client.post(endpoint, body, &self.cancel).await;
        self.finish(result, endpoint.path())
    }

    async fn post_scoped(
        &self,
        endpoint: Endpoint,
        repo: &Repo,
        body: &Value,
    ) -> Result<Value, String> {
        let result = self
            .client
            .post_scoped(endpoint, repo, body, &self.cancel)
            .await;
        self.finish(result, endpoint.path())
    }

    async fn put(&self, endpoint: Endpoint, body: &Value) -> Result<Value, String> {
        let result = self.client.put(endpoint, body, &self.cancel).await;
        self.finish(result, endpoint.path())
    }
}

async fn run_authenticated(
    command: Command,
    repo: Repo,
    root: &Path,
    api: &Api,
) -> Result<Vec<String>, String> {
    match command {
        Command::Init { client, dry_run } => init(api, root, client.as_deref(), dry_run).await,
        Command::Sweep { path, dry_run } => {
            sweep(api, &repo, path.as_deref().unwrap_or(root), dry_run).await
        }
        Command::Reindex {
            path,
            dry_run,
            paths,
        } => reindex(api, &repo, path.as_deref().unwrap_or(root), &paths, dry_run).await,
        Command::Github { action, values } => github(api, action.as_deref(), &values).await,
        Command::Monitor { action, values } => monitor(api, action.as_deref(), &values).await,
        Command::Research { action, values } => {
            research(api, &repo, root, action.as_deref(), &values).await
        }
        Command::Memory { action, values } => memory(api, &repo, action.as_deref(), &values).await,
        Command::Ask { question } => ask(api, &repo, &question).await,
        Command::Recall { query } => recall(api, &repo, &query).await,
        Command::Verify { file } => verify(api, &repo, file.as_deref()).await,
        Command::Gate { base } => gate(api, &repo, root, base.as_deref()).await,
        Command::Login
        | Command::Connect { .. }
        | Command::Remove
        | Command::Hook { .. }
        | Command::InstallHooks
        | Command::UninstallHooks
        | Command::Acp
        | Command::Mcp { .. }
        | Command::McpServer => Err("local command reached the remote dispatcher".to_string()),
    }
}

#[derive(Serialize)]
struct FilePayload {
    path: String,
    content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkingMemoryFile {
    pub(crate) path: String,
    pub(crate) content: String,
}

pub(crate) fn working_memory_files(root: &Path) -> Result<Vec<WorkingMemoryFile>, String> {
    if !is_git_worktree(root)? {
        return Ok(Vec::new());
    }
    let mut paths = BTreeSet::new();
    for arguments in [
        &["diff", "--name-only", "-z"][..],
        &["diff", "--cached", "--name-only", "-z"][..],
        &["ls-files", "--others", "--exclude-standard", "-z"][..],
    ] {
        paths.extend(git_paths(root, arguments)?);
    }
    let named = paths.into_iter().collect::<Vec<_>>();
    let (files, _skipped) = collect_files(root, &named)?;
    Ok(files
        .into_iter()
        .map(|file| WorkingMemoryFile {
            path: file.path,
            content: file.content,
        })
        .collect())
}

fn collect_files(
    root: &Path,
    named: &[PathBuf],
) -> Result<(Vec<FilePayload>, Vec<String>), String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let named_git_inventory = if !named.is_empty() && is_git_worktree(root)? {
        Some(
            git_paths(
                root,
                &[
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ],
            )?
            .into_iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<BTreeSet<_>>(),
        )
    } else {
        None
    };
    let paths = if named.is_empty() {
        inventory_paths(root)?
    } else {
        named.iter().map(PathBuf::from).collect()
    };
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for path in paths.into_iter().take(4000) {
        let full = if path.is_absolute() {
            path.clone()
        } else {
            root.join(&path)
        };
        let canonical = match full.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                skipped.push(format!("{} (not readable)", path.display()));
                continue;
            }
        };
        let relative = match canonical.strip_prefix(&canonical_root) {
            Ok(relative) => relative,
            Err(_) => {
                skipped.push(format!("{} (outside repo)", path.display()));
                continue;
            }
        };
        let relative_key = relative.to_string_lossy().replace('\\', "/");
        if named_git_inventory
            .as_ref()
            .is_some_and(|inventory| !inventory.contains(&relative_key))
        {
            skipped.push(format!("{relative_key} (outside Git inventory)"));
            continue;
        }
        if !is_source(relative) {
            skipped.push(relative.display().to_string());
            continue;
        }
        let metadata = match fs::symlink_metadata(&full) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= 400_000 => metadata,
            _ => {
                skipped.push(relative.display().to_string());
                continue;
            }
        };
        let _ = metadata;
        let content =
            fs::read_to_string(&full).map_err(|error| format!("{}: {error}", full.display()))?;
        if is_secret_shaped(&content) {
            skipped.push(format!(
                "{} (credential-shaped content)",
                relative.display()
            ));
            continue;
        }
        files.push(FilePayload {
            path: relative_key,
            content,
        });
    }
    Ok((files, skipped))
}

fn inventory_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if is_git_worktree(root)? {
        git_paths(
            root,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        )
        .map_err(|error| {
            format!("git inventory failed; refusing a directory-walk fallback: {error}")
        })
    } else {
        walk_paths(root)
    }
}

fn is_git_worktree(root: &Path) -> Result<bool, String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|error| format!("could not determine whether this is a Git worktree: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim() == "true");
    }
    if root
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
    {
        return Err(format!(
            "Git metadata is present but worktree detection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(false)
}

fn walk_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
        if paths.len() >= 4_000 {
            return Ok(());
        }
        let mut entries = fs::read_dir(directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if paths.len() >= 4_000 {
                break;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_dir() {
                if !name.starts_with('.') && !SKIP_DIRECTORIES.contains(&name.as_ref()) {
                    visit(root, &entry.path(), paths)?;
                }
            } else if file_type.is_file() {
                paths.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|error| error.to_string())?
                        .to_path_buf(),
                );
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    visit(root, root, &mut paths)?;
    Ok(paths)
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SOURCE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn git_paths(root: &Path, args: &[&str]) -> Result<Vec<PathBuf>, String> {
    if !args.contains(&"-z") {
        return Err("internal Git inventory omitted required NUL delimiters".to_string());
    }
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8_lossy(path).into_owned()))
        .collect())
}

async fn sweep(api: &Api, repo: &Repo, root: &Path, dry_run: bool) -> Result<Vec<String>, String> {
    match sweep_with_progress(&api.client, repo, root, dry_run, &api.cancel, |progress| {
        emit_lines(&[progress.line()])
    })
    .await
    {
        Ok(lines) => Ok(lines),
        Err(SweepFailure::Client(error)) => {
            let message = if error.is_explicit_auth_rejection() {
                format!(
                    "{error} — the stored credential was rejected during the sweep and was NOT removed; a single rejection can be route scope, not a bad key. Run estelle login only if you revoked it."
                )
            } else {
                error.to_string()
            };
            Err(message)
        }
        Err(SweepFailure::Local(error)) => Err(error),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SweepProgress {
    pub(crate) state: String,
    pub(crate) percent: f64,
    pub(crate) files: usize,
    pub(crate) bytes: usize,
}

impl SweepProgress {
    pub(crate) fn line(&self) -> String {
        format!(
            "{} · {:.0}% · {} files · {} KB",
            self.state,
            self.percent,
            self.files,
            self.bytes / 1024
        )
    }
}

#[derive(Debug)]
pub(crate) enum SweepFailure {
    Client(Error),
    Local(String),
}

pub(crate) async fn sweep_with_progress<F>(
    client: &Client,
    repo: &Repo,
    root: &Path,
    dry_run: bool,
    cancel: &CancellationToken,
    mut report: F,
) -> Result<Vec<String>, SweepFailure>
where
    F: FnMut(SweepProgress) -> Result<(), String>,
{
    let (files, skipped) = collect_files(root, &[]).map_err(SweepFailure::Local)?;
    if files.is_empty() {
        return Err(SweepFailure::Local(
            "no ingestable source files were found".to_string(),
        ));
    }
    let bytes: usize = files.iter().map(|file| file.content.len()).sum();
    let file_count = files.len();
    let mut lines = vec![format!(
        "Found {} files ({} KB) for {repo}; {} skipped.",
        file_count,
        bytes / 1024,
        skipped.len()
    )];
    report(SweepProgress {
        state: "files collected safely".to_string(),
        percent: 10.0,
        files: file_count,
        bytes,
    })
    .map_err(SweepFailure::Local)?;
    if dry_run {
        lines.push("--dry-run: nothing was sent.".to_string());
        report(SweepProgress {
            state: "dry run complete".to_string(),
            percent: 100.0,
            files: file_count,
            bytes,
        })
        .map_err(SweepFailure::Local)?;
        return Ok(lines);
    }
    let estimate_files = files
        .iter()
        .map(|file| json!({"path": file.path, "bytes": file.content.len()}))
        .collect::<Vec<_>>();
    report(SweepProgress {
        state: "checking account capacity".to_string(),
        percent: 20.0,
        files: file_count,
        bytes,
    })
    .map_err(SweepFailure::Local)?;
    let estimate: Value = client
        .post_scoped(
            Endpoint::SweepEstimate,
            repo,
            &json!({"files": estimate_files}),
            cancel,
        )
        .await
        .map_err(SweepFailure::Client)?;
    if estimate.get("fits") == Some(&Value::Bool(false)) {
        return Err(SweepFailure::Local(format!(
            "this sweep does not fit the account capacity: {}",
            concise_value(&estimate)
        )));
    }
    match sweep_transport(file_count) {
        SweepTransport::Sync => {
            report(SweepProgress {
                state: "sending source set".to_string(),
                percent: 35.0,
                files: file_count,
                bytes,
            })
            .map_err(SweepFailure::Local)?;
            let response: Value = client
                .post_scoped(Endpoint::Sync, repo, &json!({"files": files}), cancel)
                .await
                .map_err(SweepFailure::Client)?;
            lines.push("Repo swept. The server accepted the complete source set.".to_string());
            lines.extend(dropped_lines(&response));
            report(SweepProgress {
                state: "repo swept".to_string(),
                percent: 100.0,
                files: file_count,
                bytes,
            })
            .map_err(SweepFailure::Local)?;
        }
        SweepTransport::Background => {
            lines.extend(
                ingest_with_progress(client, repo, files, file_count, bytes, cancel, &mut report)
                    .await?,
            );
        }
    }
    Ok(lines)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SweepTransport {
    Sync,
    Background,
}

fn sweep_transport(file_count: usize) -> SweepTransport {
    if file_count < SYNC_MAX_FILES {
        SweepTransport::Sync
    } else {
        SweepTransport::Background
    }
}

async fn ingest_with_progress<F>(
    client: &Client,
    repo: &Repo,
    files: Vec<FilePayload>,
    file_count: usize,
    bytes: usize,
    cancel: &CancellationToken,
    report: &mut F,
) -> Result<Vec<String>, SweepFailure>
where
    F: FnMut(SweepProgress) -> Result<(), String>,
{
    report(SweepProgress {
        state: "starting background ingest".to_string(),
        percent: 30.0,
        files: file_count,
        bytes,
    })
    .map_err(SweepFailure::Local)?;
    let started: Value = client
        .post_scoped(
            Endpoint::IngestStart,
            repo,
            &json!({"files": files}),
            cancel,
        )
        .await
        .map_err(SweepFailure::Client)?;
    let mut lines = dropped_lines(&started);
    lines.push("Ingestion started. It continues server-side if this terminal closes.".to_string());
    let began = Instant::now();
    let mut last_status = String::new();
    let mut stale = 0_usize;
    for _ in 0..INGEST_MAX_POLLS {
        tokio::select! {
            () = cancel.cancelled() => return Err(SweepFailure::Client(Error::Cancelled)),
            () = tokio::time::sleep(INGEST_POLL_INTERVAL) => {}
        }
        let progress: Value = match client
            .get_scoped(Endpoint::IngestProgress, repo, &json!({}), cancel)
            .await
        {
            Ok(progress) => {
                stale = 0;
                progress
            }
            Err(error) => {
                stale += 1;
                if stale > 5 {
                    return Err(SweepFailure::Local(format!(
                        "could not read ingest progress after five retries: {error}"
                    )));
                }
                continue;
            }
        };
        let state = progress
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("ingesting");
        let percent = progress
            .get("percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let status = format!(
            "{state} {:.0}% (elapsed {})",
            percent,
            format_elapsed(began.elapsed())
        );
        if status != last_status {
            report(SweepProgress {
                state: status.clone(),
                percent,
                files: file_count,
                bytes,
            })
            .map_err(SweepFailure::Local)?;
            last_status = status;
        }
        match state {
            "done" => {
                lines.push(
                    progress
                        .get("message")
                        .and_then(Value::as_str)
                        .filter(|message| !message.trim().is_empty())
                        .unwrap_or("Repo swept. Your agent can now recall and verify against it.")
                        .to_string(),
                );
                report(SweepProgress {
                    state: "repo swept".to_string(),
                    percent: 100.0,
                    files: file_count,
                    bytes,
                })
                .map_err(SweepFailure::Local)?;
                return Ok(lines);
            }
            "error" => {
                return Err(SweepFailure::Local(format!(
                    "ingest failed at {:.0}%: {}",
                    percent,
                    progress_message(&progress)
                )));
            }
            "stalled" => {
                return Err(SweepFailure::Local(format!(
                    "ingest stopped at {:.0}%: {}",
                    percent,
                    progress_message(&progress)
                )));
            }
            _ => {}
        }
    }
    Err(SweepFailure::Local(
        "stopped polling; the ingest may still be running server-side, so check the dashboard before retrying".to_string(),
    ))
}

fn progress_message(progress: &Value) -> String {
    progress
        .get("message")
        .or_else(|| progress.get("error"))
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or("the server reported no reason")
        .to_string()
}

fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

async fn reindex(
    api: &Api,
    repo: &Repo,
    root: &Path,
    named: &[PathBuf],
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let changed = if named.is_empty() {
        let mut paths = git_paths(root, &["diff", "--name-only", "-z", "HEAD"])?;
        paths.extend(git_paths(
            root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "--full-name",
                "-z",
            ],
        )?);
        paths
    } else {
        named.to_vec()
    };
    let deleted = if named.is_empty() {
        git_paths(
            root,
            &["diff", "--name-only", "--diff-filter=D", "-z", "HEAD"],
        )?
    } else {
        Vec::new()
    };
    let deleted_set = deleted
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let present = changed
        .into_iter()
        .filter(|path| !deleted_set.contains(&path.to_string_lossy().replace('\\', "/")))
        .collect::<Vec<_>>();
    let (files, skipped) = collect_files(root, &present)?;
    if files.is_empty() && deleted_set.is_empty() {
        return Ok(vec![
            "Nothing changed. Estelle memory is already current.".to_string(),
        ]);
    }
    let mut lines = vec![format!(
        "Reindexing {} changed and {} removed files; {} skipped.",
        files.len(),
        deleted_set.len(),
        skipped.len()
    )];
    if dry_run {
        lines.push("--dry-run: nothing was sent.".to_string());
        return Ok(lines);
    }
    let response = api
        .post_scoped(
            Endpoint::Reindex,
            repo,
            &json!({"files": files, "removed": deleted_set}),
        )
        .await?;
    lines.push("Memory current. Untouched files kept their symbols.".to_string());
    lines.extend(dropped_lines(&response));
    Ok(lines)
}

fn dropped_lines(response: &Value) -> Vec<String> {
    let Some(dropped) = response.get("dropped").and_then(Value::as_object) else {
        return Vec::new();
    };
    let count: u64 = dropped.values().filter_map(Value::as_u64).sum();
    (count > 0)
        .then(|| {
            format!(
                "Warning: {count} files were not indexed: {}",
                concise_value(response)
            )
        })
        .into_iter()
        .collect()
}

async fn ask(api: &Api, repo: &Repo, words: &[String]) -> Result<Vec<String>, String> {
    let question = require_words(words, "Ask what?")?;
    let result = api
        .client
        .chat_completion(
            repo,
            &ChatCompletionRequest::question(question),
            &api.cancel,
        )
        .await;
    let response: ChatCompletionResponse = api.finish(result, "v1/chat/completions")?;
    Ok(vec![
        response
            .answer()
            .filter(|answer| !answer.trim().is_empty())
            .unwrap_or("Estelle returned no answer.")
            .to_string(),
    ])
}

async fn recall(api: &Api, repo: &Repo, words: &[String]) -> Result<Vec<String>, String> {
    let query = require_words(words, "Recall what?")?;
    let reply = api
        .post_scoped(Endpoint::Search, repo, &json!({"query": query}))
        .await?;
    let mut lines = value_text(&reply, &["recall", "answer", "question"]);
    append_citations(&mut lines, reply.get("code"));
    nonblank(lines, &reply)
}

async fn verify(api: &Api, repo: &Repo, file: Option<&Path>) -> Result<Vec<String>, String> {
    let file = file.ok_or_else(|| "verify needs a readable file path".to_string())?;
    let answer =
        fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let reply = api
        .post_scoped(Endpoint::Verify, repo, &json!({"answer": answer}))
        .await?;
    let typed: CommandReply = serde_json::from_value(reply).map_err(|error| error.to_string())?;
    Ok(commands::render_remote_reply("verify", &typed))
}

async fn gate(
    api: &Api,
    repo: &Repo,
    root: &Path,
    base: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut command = ProcessCommand::new("git");
    command.arg("-C").arg(root).args(["diff", "--no-color"]);
    if let Some(base) = base {
        command.arg(format!("{base}...HEAD"));
    } else {
        command.arg("--cached");
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "could not compute the diff: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let diff = String::from_utf8_lossy(&output.stdout);
    if diff.trim().is_empty() {
        return Ok(vec![
            "No diff to gate. Stage changes or pass --base <ref>.".to_string(),
        ]);
    }
    let reply = api
        .post_scoped(Endpoint::Gate, repo, &json!({"diff": diff}))
        .await?;
    let typed: CommandReply = serde_json::from_value(reply).map_err(|error| error.to_string())?;
    Ok(commands::render_remote_reply("gate", &typed))
}

async fn github(api: &Api, action: Option<&str>, values: &[String]) -> Result<Vec<String>, String> {
    match action.unwrap_or("status") {
        "status" => {
            let identity = api.get(Endpoint::GithubIdentity, &json!({})).await?;
            if identity.get("linked") != Some(&Value::Bool(true)) {
                return Ok(github_status_lines(&identity, &json!({}), &json!({})));
            }
            let installations = api
                .get(Endpoint::GithubIdentityInstallations, &json!({}))
                .await?;
            let repos = api.get(Endpoint::GithubRepos, &json!({})).await?;
            Ok(github_status_lines(&identity, &installations, &repos))
        }
        "repos" => {
            let reply = api.get(Endpoint::GithubRepos, &json!({})).await?;
            nonblank(value_rows(&reply, "repos"), &reply)
        }
        "connect" => {
            let listed = api
                .get(Endpoint::GithubIdentityInstallations, &json!({}))
                .await?;
            let rows = listed
                .get("installations")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let installation = match pick_github_installation(
                rows,
                values.first().map(String::as_str),
            ) {
                GithubInstallationChoice::Chosen(row) => row,
                GithubInstallationChoice::None => {
                    return Err(
                        "that identity cannot see an Estelle GitHub App installation; install the App on the org or user that owns the repos, then retry"
                            .to_string(),
                    );
                }
                GithubInstallationChoice::Ambiguous(rows) => {
                    return Err(format!(
                        "more than one GitHub installation is visible; choose an id or owner: {}",
                        github_installation_options(&rows)
                    ));
                }
                GithubInstallationChoice::Unknown(rows) => {
                    return Err(format!(
                        "no visible GitHub installation matches {}; choose one of: {}",
                        values
                            .first()
                            .map(String::as_str)
                            .unwrap_or("the requested value"),
                        github_installation_options(&rows)
                    ));
                }
            };
            let installation_id = installation
                .get("id")
                .cloned()
                .ok_or_else(|| "GitHub returned an installation without an id".to_string())?;
            let reply = api
                .post(
                    Endpoint::GithubAppSetup,
                    &json!({"installation_id": installation_id, "sweep": false}),
                )
                .await?;
            Ok(vec![
                format!(
                    "Connected GitHub installation {}.",
                    concise_value(&installation_id)
                ),
                "Nothing was ingested; run estelle github repos, then choose a sweep.".to_string(),
                concise_value(&reply),
            ])
        }
        "sweep" => {
            if values.is_empty() {
                return Err("github sweep needs one or more owner/repo names".to_string());
            }
            let reply = api
                .post(Endpoint::GithubSweep, &json!({"repos": values}))
                .await?;
            nonblank(value_rows(&reply, "queued"), &reply)
        }
        "link" => github_link(api).await,
        action => Err(format!("unknown github action {action}")),
    }
}

#[derive(Debug, PartialEq)]
enum GithubInstallationChoice {
    None,
    Chosen(Value),
    Ambiguous(Vec<Value>),
    Unknown(Vec<Value>),
}

fn pick_github_installation(rows: &[Value], requested: Option<&str>) -> GithubInstallationChoice {
    if rows.is_empty() {
        return GithubInstallationChoice::None;
    }
    if let Some(requested) = requested {
        let requested = requested.trim().to_ascii_lowercase();
        if let Some(row) = rows.iter().find(|row| {
            row.get("id")
                .map(concise_value)
                .is_some_and(|id| id == requested)
                || row
                    .get("account")
                    .and_then(Value::as_str)
                    .is_some_and(|account| account.to_ascii_lowercase() == requested)
        }) {
            return GithubInstallationChoice::Chosen(row.clone());
        }
        return GithubInstallationChoice::Unknown(rows.to_vec());
    }
    if rows.len() == 1 {
        GithubInstallationChoice::Chosen(rows[0].clone())
    } else {
        GithubInstallationChoice::Ambiguous(rows.to_vec())
    }
}

fn github_installation_options(rows: &[Value]) -> String {
    rows.iter()
        .map(|row| {
            let id = row
                .get("id")
                .map(concise_value)
                .unwrap_or_else(|| "?".to_string());
            let account = row.get("account").and_then(Value::as_str).unwrap_or("?");
            format!("{id} {account}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn github_status_lines(identity: &Value, installations: &Value, repos: &Value) -> Vec<String> {
    let linked = identity.get("linked") == Some(&Value::Bool(true));
    if !linked {
        return vec![
            "GitHub identity: not linked".to_string(),
            "Run: estelle github link".to_string(),
        ];
    }
    let login = identity
        .get("login")
        .and_then(Value::as_str)
        .map(|login| format!(" as {login}"))
        .unwrap_or_default();
    let mut lines = vec![format!("GitHub identity: linked{login}")];
    let bound = github_bound_installations(repos);
    let rows = installations
        .get("installations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for row in rows {
        let id = row
            .get("id")
            .map(concise_value)
            .unwrap_or_else(|| "?".to_string());
        let account = row.get("account").and_then(Value::as_str).unwrap_or("");
        let kind = row
            .get("type")
            .and_then(Value::as_str)
            .map(|kind| format!(" ({kind})"))
            .unwrap_or_default();
        let mark = if bound.contains(&id) {
            "  connected"
        } else {
            ""
        };
        lines.push(format!("  {id}  {account}{kind}{mark}"));
    }
    if !bound.is_empty() {
        lines.push(format!("Connected: installation {}", bound.join(", ")));
        lines.push("Run: estelle github repos".to_string());
    } else if rows.is_empty() {
        lines.push(
            "No App installations are visible to that identity; install the Estelle GitHub App, then run: estelle github connect"
                .to_string(),
        );
    } else {
        lines.push("Run: estelle github connect [id|owner]".to_string());
    }
    lines
}

fn github_bound_installations(repos: &Value) -> Vec<String> {
    if repos.get("connected") != Some(&Value::Bool(true)) {
        return Vec::new();
    }
    let mut ids = repos
        .get("installations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(concise_value)
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    if ids.is_empty()
        && let Some(id) = repos.get("installation_id").filter(|id| !id.is_null())
    {
        ids.push(concise_value(id));
    }
    ids
}

async fn github_link(api: &Api) -> Result<Vec<String>, String> {
    let listener = bind_github_listener(GITHUB_LOOPBACK_PORT)?;
    let redirect_uri = github_redirect_uri(GITHUB_LOOPBACK_PORT);
    let start = api
        .get(
            Endpoint::GithubIdentityAuthorizeUrl,
            &json!({"redirect_uri": redirect_uri}),
        )
        .await?;
    let authorize_url = start
        .get("authorize_url")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| "the server returned no GitHub authorization URL".to_string())?
        .to_string();

    emit_lines(&[
        "Authorize Estelle on GitHub (opening your browser).".to_string(),
        authorize_url.clone(),
        format!("Waiting for the redirect on {redirect_uri} ..."),
    ])?;
    webbrowser::open(&authorize_url).map_err(|error| {
        format!("could not open the browser: {error}; open the URL above manually")
    })?;

    let (code, state) = tokio::task::spawn_blocking(move || {
        await_github_callback(listener, GITHUB_CALLBACK_TIMEOUT)
    })
    .await
    .map_err(|error| format!("GitHub callback listener failed: {error}"))??;
    let linked = api
        .post(
            Endpoint::GithubIdentityLink,
            &json!({"code": code, "state": state}),
        )
        .await?;
    let login = linked.get("login").and_then(Value::as_str);
    Ok(vec![
        login.map_or_else(
            || "GitHub identity linked.".to_string(),
            |login| format!("GitHub identity linked as {login}."),
        ),
        "Next: estelle github connect".to_string(),
    ])
}

fn github_redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{GITHUB_CALLBACK_PATH}")
}

fn bind_github_listener(port: u16) -> Result<TcpListener, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            format!("port {port} is already in use; close whatever is on it and retry")
        } else {
            format!("could not listen for the GitHub callback: {error}")
        }
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure the GitHub callback listener: {error}"))?;
    Ok(listener)
}

fn parse_github_callback(raw_url: &str) -> Option<Result<(String, String), String>> {
    let base = url::Url::parse(&github_redirect_uri(GITHUB_LOOPBACK_PORT)).ok()?;
    let parsed = base.join(raw_url).ok()?;
    if parsed.origin() != base.origin() || parsed.path() != GITHUB_CALLBACK_PATH {
        return None;
    }
    let values = parsed
        .query_pairs()
        .collect::<std::collections::BTreeMap<_, _>>();
    if let Some(error) = values.get("error") {
        let detail = values.get("error_description").unwrap_or(error).to_string();
        return Some(Err(detail));
    }
    let Some(code) = values.get("code").filter(|value| !value.is_empty()) else {
        return Some(Err("GitHub redirected without a code".to_string()));
    };
    let Some(state) = values.get("state").filter(|value| !value.is_empty()) else {
        return Some(Err("GitHub redirected without a state".to_string()));
    };
    Some(Ok((code.to_string(), state.to_string())))
}

fn await_github_callback(
    listener: TcpListener,
    timeout: Duration,
) -> Result<(String, String), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            return Err("timed out waiting for GitHub".to_string());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .map_err(|error| error.to_string())?;
                let mut request = [0_u8; 8192];
                let read = stream
                    .read(&mut request)
                    .map_err(|error| error.to_string())?;
                let raw = String::from_utf8_lossy(&request[..read]);
                let target = raw
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("");
                match parse_github_callback(target) {
                    None => write_github_callback_response(&mut stream, 404, "not found\n")?,
                    Some(Ok(pair)) => {
                        write_github_callback_response(
                            &mut stream,
                            200,
                            "Estelle: GitHub authorized. You can close this tab and return to your terminal.\n",
                        )?;
                        return Ok(pair);
                    }
                    Some(Err(error)) => {
                        write_github_callback_response(
                            &mut stream,
                            400,
                            "Estelle: GitHub authorization failed. Close this tab and try again in your terminal.\n",
                        )?;
                        return Err(error);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("GitHub callback listener failed: {error}")),
        }
    }
}

fn write_github_callback_response(
    stream: &mut std::net::TcpStream,
    status: u16,
    body: &str,
) -> Result<(), String> {
    let reason = if status == 200 {
        "OK"
    } else if status == 404 {
        "Not Found"
    } else {
        "Bad Request"
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| error.to_string())
}

fn emit_lines(lines: &[String]) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    for line in lines {
        writeln!(writer, "{}", estelle_client::mask_secret(line))
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

async fn monitor(
    api: &Api,
    action: Option<&str>,
    values: &[String],
) -> Result<Vec<String>, String> {
    let (endpoint, method, body) = match action.unwrap_or("status") {
        "status" | "overview" => (Endpoint::MonitorOverview, "get", json!({})),
        "issues" => (Endpoint::MonitorIssues, "get", json!({})),
        "issue" => (
            Endpoint::MonitorIssue,
            "post",
            json!({"key": require_words(values, "monitor issue needs a key")?}),
        ),
        "alerts" => (Endpoint::MonitorAlerts, "get", json!({})),
        "uptime" => (Endpoint::MonitorUptime, "get", json!({})),
        "logs" => (
            Endpoint::MonitorLogs,
            "get",
            json!({"query": values.join(" ")}),
        ),
        action => return Err(format!("unknown monitor action {action}")),
    };
    let reply = if method == "get" {
        api.get(endpoint, &body).await?
    } else {
        api.post(endpoint, &body).await?
    };
    render_suite_reply("monitor", action.unwrap_or("status"), &reply)
}

async fn research(
    api: &Api,
    repo: &Repo,
    root: &Path,
    action: Option<&str>,
    values: &[String],
) -> Result<Vec<String>, String> {
    match action.unwrap_or("status") {
        "status" => {
            let reply = api.get(Endpoint::VendorDriftWatchlist, &json!({})).await?;
            render_suite_reply("research", "status", &reply)
        }
        "watch" => {
            let body = research_watch_body(values)?;
            let reply = api.put(Endpoint::VendorDriftWatchlist, &body).await?;
            render_suite_reply("research", "watch", &reply)
        }
        "off" => {
            let reply = api
                .put(Endpoint::VendorDriftWatchlist, &json!({"cadence": "off"}))
                .await?;
            let mut lines = vec!["Vendor watch is off; the vendor list was retained.".to_string()];
            lines.extend(summary_lines(&reply));
            Ok(lines)
        }
        "drift" => {
            let reply = api.post(Endpoint::VendorDrift, &json!({})).await?;
            render_suite_reply("research", "drift", &reply)
        }
        "repair" => {
            let scan = api.post(Endpoint::VendorDrift, &json!({})).await?;
            let findings = scan.get("findings").cloned().unwrap_or_else(|| json!([]));
            let mut sources = serde_json::Map::new();
            if let Some(rows) = findings.as_array() {
                for path in rows
                    .iter()
                    .flat_map(|finding| {
                        finding
                            .get("usage_sites")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                    })
                    .filter_map(Value::as_str)
                    .take(6)
                {
                    if let Ok(text) = fs::read_to_string(root.join(path)) {
                        sources.insert(path.to_string(), Value::String(text));
                    }
                }
            }
            let reply = api
                .post(
                    Endpoint::VendorDriftRepair,
                    &json!({"findings": findings, "sources": sources}),
                )
                .await?;
            render_suite_reply("research", "repair", &reply)
        }
        "ask" => {
            let question = require_words(values, "research ask needs a question")?;
            let reply = api
                .post_scoped(Endpoint::DeepSearch, repo, &json!({"question": question}))
                .await?;
            let mut lines = value_text(&reply, &["answer", "question"]);
            append_citations(&mut lines, reply.get("sources"));
            nonblank(lines, &reply)
        }
        action => Err(format!("unknown research action {action}")),
    }
}

fn research_watch_body(values: &[String]) -> Result<Value, String> {
    let (without_api, apis) = split_flag(values, "--api");
    let (vendors, cadence) = split_flag(&without_api, "--cadence");
    let cadence = cadence.or_else(|| (!vendors.is_empty()).then(|| "daily".to_string()));
    if cadence
        .as_deref()
        .is_some_and(|value| !["off", "hourly", "daily", "weekly"].contains(&value))
    {
        return Err("cadence must be off, hourly, daily, or weekly".to_string());
    }
    if apis.is_some() && vendors.len() != 1 {
        return Err("--api requires exactly one vendor".to_string());
    }
    let mut body = serde_json::Map::new();
    if let Some(cadence) = cadence {
        body.insert("cadence".to_string(), Value::String(cadence));
    }
    if !vendors.is_empty() {
        let apis = apis.map(|value| {
            value
                .split([',', ' '])
                .filter(|api| !api.trim().is_empty())
                .map(|api| Value::String(api.trim().to_string()))
                .collect::<Vec<_>>()
        });
        let rows = vendors
            .into_iter()
            .map(|name| {
                let mut vendor = serde_json::Map::from_iter([(
                    "name".to_string(),
                    Value::String(name.trim().to_ascii_lowercase()),
                )]);
                if let Some(apis) = &apis {
                    vendor.insert("apis".to_string(), Value::Array(apis.clone()));
                }
                Value::Object(vendor)
            })
            .collect();
        body.insert("vendors".to_string(), Value::Array(rows));
    }
    Ok(Value::Object(body))
}

async fn memory(
    api: &Api,
    repo: &Repo,
    action: Option<&str>,
    values: &[String],
) -> Result<Vec<String>, String> {
    let action = action.unwrap_or("receipts");
    let confirmed = values.iter().any(|value| value == "--yes");
    let values = values
        .iter()
        .filter(|value| *value != "--yes")
        .cloned()
        .collect::<Vec<_>>();
    if let Some(lines) = erasure_gate(action, &values, confirmed) {
        return Ok(lines);
    }
    let (endpoint, method, payload) = memory_request(action, &values)?;
    let reply = match (method, memory_scope(endpoint)) {
        (MemoryMethod::Get, MemoryScope::Account) => api.get(endpoint, &payload).await?,
        (MemoryMethod::Get, MemoryScope::Repo) => api.get_scoped(endpoint, repo, &payload).await?,
        (MemoryMethod::Post, MemoryScope::Account) => api.post(endpoint, &payload).await?,
        (MemoryMethod::Post, MemoryScope::Repo) => {
            api.post_scoped(endpoint, repo, &payload).await?
        }
    };
    render_suite_reply("memory", action, &reply)
}

/// The S2 gate: `memory forget`/`retract` erase across EVERY namespace the account owns — the
/// server has no repo-scoped erasure, and the class sweep caught the client implying one by
/// demanding and injecting a repo the server never reads. The confirmation names the true
/// radius BEFORE anything is sent; without `--yes` nothing leaves the machine.
fn erasure_gate(action: &str, values: &[String], confirmed: bool) -> Option<Vec<String>> {
    if !matches!(action, "forget" | "retract") || confirmed {
        return None;
    }
    let target = values
        .iter()
        .find(|value| !value.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("");
    Some(vec![
        format!(
            "memory {action} {target} erases across ALL namespaces this account owns — not just this repo."
        ),
        "The server has no repo-scoped erasure today, so the CLI does not imply one.".to_string(),
        "Re-run with --yes to confirm. Nothing was sent.".to_string(),
    ])
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemoryMethod {
    Get,
    Post,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemoryScope {
    Account,
    Repo,
}

fn memory_scope(endpoint: Endpoint) -> MemoryScope {
    if endpoint.requires_repo() {
        MemoryScope::Repo
    } else {
        MemoryScope::Account
    }
}

fn memory_request(
    action: &str,
    values: &[String],
) -> Result<(Endpoint, MemoryMethod, Value), String> {
    match action {
        "receipts" | "proof" => {
            let (_, limit) = split_flag(values, "--limit");
            let query = match limit {
                Some(limit) => {
                    let limit = limit
                        .parse::<u64>()
                        .map_err(|_| "--limit must be a positive integer".to_string())?;
                    if limit == 0 {
                        return Err("--limit must be a positive integer".to_string());
                    }
                    json!({"limit": limit})
                }
                None => json!({}),
            };
            Ok((Endpoint::DeletionReceipts, MemoryMethod::Get, query))
        }
        "retract" => {
            let (subjects, reason) = split_flag(values, "--reason");
            let subject = subjects
                .first()
                .filter(|subject| !subject.trim().is_empty())
                .ok_or_else(|| "memory retract needs a subject".to_string())?;
            let mut body = json!({"subject": subject});
            if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
                body["reason"] = Value::String(reason);
            }
            Ok((Endpoint::Retract, MemoryMethod::Post, body))
        }
        "forget" => {
            let source = values
                .first()
                .filter(|source| !source.starts_with('-') && !source.trim().is_empty())
                .ok_or_else(|| "memory forget needs a source".to_string())?;
            Ok((
                Endpoint::Forget,
                MemoryMethod::Post,
                json!({"source": source}),
            ))
        }
        "learned" => Ok((Endpoint::Instincts, MemoryMethod::Get, json!({}))),
        "unlearn" => {
            let (instinct, skill) = split_flag(values, "--skill");
            let body = if let Some(skill) = skill.filter(|skill| !skill.trim().is_empty()) {
                json!({"skill": skill})
            } else if instinct.len() >= 2
                && !instinct[0].starts_with('-')
                && !instinct[1].starts_with('-')
            {
                json!({"instinct": {"trigger": instinct[0], "response": instinct[1]}})
            } else {
                return Err(
                    "memory unlearn needs --skill <name> or a quoted trigger and response"
                        .to_string(),
                );
            };
            Ok((Endpoint::Unlearn, MemoryMethod::Post, body))
        }
        action => Err(format!("unknown memory action {action}")),
    }
}

fn connect_lines(client: &str) -> Vec<String> {
    vec![
        format!("Connect {client} to Estelle without printing a stored credential."),
        "Use the client's HTTP MCP configuration with https://api.fatelabs.ca/mcp.".to_string(),
        "Set Authorization to Bearer <YOUR_ESTELLE_KEY> in that client's secure configuration."
            .to_string(),
    ]
}

#[derive(Clone)]
struct EditorConfig {
    name: &'static str,
    path: PathBuf,
    top_key: &'static str,
}

fn editor_configs(root: &Path) -> Vec<EditorConfig> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        EditorConfig {
            name: "cursor",
            path: home.join(".cursor/mcp.json"),
            top_key: "mcpServers",
        },
        EditorConfig {
            name: "cline",
            path: home.join(
                "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
            ),
            top_key: "mcpServers",
        },
        EditorConfig {
            name: "windsurf",
            path: home.join(".codeium/windsurf/mcp_config.json"),
            top_key: "mcpServers",
        },
        EditorConfig {
            name: "jetbrains",
            path: home.join(".junie/mcp/mcp.json"),
            top_key: "mcpServers",
        },
        EditorConfig {
            name: "vscode",
            path: root.join(".vscode/mcp.json"),
            top_key: "servers",
        },
    ]
}

async fn init(
    api: &Api,
    root: &Path,
    only: Option<&str>,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    if let Some(client @ ("claude-desktop" | "continue" | "claude-code")) = only {
        return Ok(connect_lines(client));
    }
    let configs = editor_configs(root);
    let selected = if let Some(only) = only {
        let config = configs
            .into_iter()
            .find(|config| config.name == only)
            .ok_or_else(|| format!("unknown client {only}"))?;
        vec![config]
    } else {
        configs
            .into_iter()
            .filter(|config| config.path.exists() || config.path.parent().is_some_and(Path::exists))
            .collect()
    };
    if selected.is_empty() {
        return Ok(vec![
            "No supported editor was detected; nothing was written.".to_string(),
            "Run estelle init --client cursor|cline|windsurf|jetbrains|vscode.".to_string(),
        ]);
    }
    let bearer = api.api_key.bearer_header_value();
    let key = bearer
        .strip_prefix("Bearer ")
        .ok_or_else(|| "credential header could not be formed".to_string())?;
    let mut lines = Vec::new();
    for config in selected {
        write_editor_config(&config.path, config.top_key, key, dry_run)?;
        lines.push(if dry_run {
            format!(
                "{}: would write {}; nothing changed",
                config.name,
                config.path.display()
            )
        } else {
            format!(
                "{}: wrote {} (existing bytes backed up)",
                config.name,
                config.path.display()
            )
        });
    }
    if dry_run {
        return Ok(lines);
    }
    let initialized = api
        .post(
            Endpoint::Mcp,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "estelle-cli", "version": env!("CARGO_PKG_VERSION")}
                }
            }),
        )
        .await?;
    if initialized.get("result").is_none() {
        return Err("configs were written, but the MCP initialize reply had no result".to_string());
    }
    lines.push(
        "Estelle answered an MCP initialize request; the connection is verified.".to_string(),
    );
    Ok(lines)
}

fn write_editor_config(path: &Path, top_key: &str, key: &str, dry_run: bool) -> Result<(), String> {
    let existed = path.exists();
    let mut root = if existed {
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "refusing to overwrite unreadable {}: {error}",
                path.display()
            )
        })?;
        value.as_object().cloned().ok_or_else(|| {
            format!(
                "refusing to overwrite {}: root is not an object",
                path.display()
            )
        })?
    } else {
        serde_json::Map::new()
    };
    let servers = root
        .entry(top_key.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            format!(
                "refusing to overwrite {}: {top_key} is not an object",
                path.display()
            )
        })?;
    servers.insert(
        "estelle".to_string(),
        json!({
            "type": "http",
            "url": "https://api.fatelabs.ca/mcp",
            "headers": {"Authorization": format!("Bearer {key}")}
        }),
    );
    if dry_run {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if existed {
        fs::copy(path, backup_path(path)).map_err(|error| error.to_string())?;
    }
    write_json_0600(path, &Value::Object(root))
}

fn remove_editor_configs(root: &Path) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for config in editor_configs(root) {
        if !config.path.exists() {
            continue;
        }
        let bytes = fs::read(&config.path).map_err(|error| error.to_string())?;
        let mut value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "refusing to overwrite unreadable {}: {error}",
                config.path.display()
            )
        })?;
        let Some(servers) = value.get_mut(config.top_key).and_then(Value::as_object_mut) else {
            continue;
        };
        if servers.remove("estelle").is_none() {
            continue;
        }
        fs::copy(&config.path, backup_path(&config.path)).map_err(|error| error.to_string())?;
        write_json_0600(&config.path, &value)?;
        lines.push(format!(
            "{}: removed Estelle from {}",
            config.name,
            config.path.display()
        ));
    }
    if lines.is_empty() {
        lines.push("No Estelle MCP entry was found; nothing changed.".to_string());
    }
    lines.push("Stored authentication is separate and was not removed.".to_string());
    Ok(lines)
}

fn backup_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    path.with_extension(format!("{extension}.bak"))
}

fn write_json_0600(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "config path has no parent".to_string())?;
    let temporary = parent.join(format!(".estelle-config-{}.tmp", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        set_private_create_mode(&mut options);
        let mut file = options
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer_pretty(&mut file, value).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        set_private_permissions(&temporary)?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        set_private_permissions(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn set_private_create_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_create_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn render_suite_reply(suite: &str, action: &str, reply: &Value) -> Result<Vec<String>, String> {
    if let Some(message) = suite_empty_message(suite, action, reply) {
        return Ok(vec![format!("{suite} {action}"), message]);
    }
    let mut lines = vec![format!("{suite} {action}")];
    const SCALARS: &[&str] = &[
        "note",
        "message",
        "answer",
        "warning",
        "cadence",
        "enrolled",
        "connected",
        "count",
        "purged",
        "forgotten",
        "retracted",
        "removed",
        "proceed",
        "planned",
    ];
    const ROWS: &[&str] = &[
        "issues",
        "alerts",
        "checks",
        "logs",
        "findings",
        "plans",
        "vendors",
        "receipts",
        "instincts",
        "queued",
        "repos",
    ];
    for key in SCALARS {
        if let Some(value) = reply.get(key) {
            lines.push(format!("{key}: {}", concise_value(value)));
        }
    }
    for key in ROWS {
        lines.extend(value_rows(reply, key));
    }
    if let Some(object) = reply.as_object() {
        lines.extend(
            object
                .iter()
                .filter(|(key, _)| {
                    !SCALARS.contains(&key.as_str())
                        && !ROWS.contains(&key.as_str())
                        && !sensitive_key(key)
                })
                .take(12)
                .map(|(key, value)| format!("{key}: {}", concise_value(value))),
        );
    }
    nonblank(lines, reply)
}

fn suite_empty_message(suite: &str, action: &str, reply: &Value) -> Option<String> {
    let empty = |key: &str| {
        reply
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    };
    match (suite, action) {
        ("monitor", "issues") if empty("issues") => {
            let events = reply
                .pointer("/counts/events")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            Some(if events > 0 {
                "No issues in the current retention window; earlier ones aged out.".to_string()
            } else {
                "No errors have reached Estelle yet. Point OTLP or Sentry at api.fatelabs.ca/monitor/ingest."
                    .to_string()
            })
        }
        ("monitor", "alerts") if empty("rules") => {
            Some("No alert rules exist. Nothing will page you when production breaks.".to_string())
        }
        ("monitor", "uptime") if empty("status") || empty("checks") => {
            Some("No uptime checks are registered.".to_string())
        }
        ("monitor", "logs") if empty("logs") => Some("No log lines matched.".to_string()),
        ("research", "status") if reply.get("enrolled") != Some(&Value::Bool(true)) => Some(
            "NOT enrolled; nothing is being watched on a schedule. Run: estelle research watch stripe openai --cadence daily"
                .to_string(),
        ),
        ("memory", "learned") if empty("instincts") => Some(
            "Estelle has not graduated any reflexes for this account; nothing is being applied on its own."
                .to_string(),
        ),
        ("memory", "receipts" | "proof") if empty("receipts") => Some(
            "No erasures are on record for this account; nothing has been purged, forgotten, or retracted."
                .to_string(),
        ),
        _ => None,
    }
}

fn value_rows(reply: &Value, key: &str) -> Vec<String> {
    let Some(rows) = reply.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    if rows.is_empty() {
        return vec![format!("{key}: none")];
    }
    let mut lines = vec![format!("{key}: {}", rows.len())];
    lines.extend(
        rows.iter()
            .take(20)
            .map(|row| format!("- {}", concise_value(row))),
    );
    lines
}

fn summary_lines(reply: &Value) -> Vec<String> {
    reply
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(key, _)| !sensitive_key(key))
        .take(12)
        .map(|(key, value)| format!("{key}: {}", concise_value(value)))
        .collect()
}

fn concise_value(value: &Value) -> String {
    match value {
        Value::Null => "none".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => {
            let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
            estelle_client::mask_secret(&compact)
        }
        Value::Array(values) => format!("{} items", values.len()),
        Value::Object(object) => object
            .iter()
            .filter(|(key, _)| !sensitive_key(key))
            .take(5)
            .map(|(key, value)| format!("{key}={}", concise_value(value)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("key")
        || key.contains("secret")
        || key.contains("credential")
        || key.contains("token")
        || key.contains("namespace")
}

fn value_text(reply: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter_map(|key| reply.get(key).and_then(Value::as_str))
        .find(|value| !value.trim().is_empty())
        .map(|value| value.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn append_citations(lines: &mut Vec<String>, value: Option<&Value>) {
    let Some(rows) = value.and_then(Value::as_array) else {
        return;
    };
    for row in rows.iter().take(8) {
        if let Some(source) = row.as_str() {
            lines.push(format!("cited: {source}"));
            continue;
        }
        let file = row
            .get("file")
            .or_else(|| row.get("source_file"))
            .or_else(|| row.get("path"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        let line = row.get("line").and_then(Value::as_u64);
        lines.push(line.map_or_else(
            || format!("cited: {file}"),
            |line| format!("cited: {file}:{line}"),
        ));
    }
}

fn nonblank(mut lines: Vec<String>, reply: &Value) -> Result<Vec<String>, String> {
    lines.retain(|line| !line.trim().is_empty());
    if lines.is_empty() {
        lines = summary_lines(reply);
    }
    if lines.is_empty() {
        return Err(
            "the server returned an empty reply; this build has nothing truthful to render"
                .to_string(),
        );
    }
    Ok(lines)
}

fn require_words(words: &[String], message: &str) -> Result<String, String> {
    let joined = words.join(" ");
    (!joined.trim().is_empty())
        .then_some(joined)
        .ok_or_else(|| message.to_string())
}

fn split_flag(values: &[String], flag: &str) -> (Vec<String>, Option<String>) {
    let mut positional = Vec::new();
    let mut found = None;
    let mut index = 0;
    while index < values.len() {
        if values[index] == flag {
            found = values.get(index + 1).cloned();
            index += 2;
        } else {
            positional.push(values[index].clone());
            index += 1;
        }
    }
    (positional, found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn python_hook(function: &str, payload: &Value) -> Value {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/hooks/estelle_hook.py")
            .canonicalize()
            .expect("Python hook source");
        let script = format!(
            "import importlib.util,json,sys\np={hook:?}\ns=importlib.util.spec_from_file_location('estelle_hook_contract',p)\nm=importlib.util.module_from_spec(s)\ns.loader.exec_module(m)\nv=json.load(sys.stdin)\nprint(json.dumps(m.{function}(*v) if isinstance(v,list) else m.{function}(v),separators=(',',':')))"
        );
        let mut child = ProcessCommand::new("python3")
            .args(["-c", &script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("python3 is required for the hook parity contract");
        child
            .stdin
            .as_mut()
            .expect("Python stdin")
            .write_all(
                serde_json::to_string(payload)
                    .expect("fixture JSON")
                    .as_bytes(),
            )
            .expect("write Python fixture");
        let output = child.wait_with_output().expect("Python hook result");
        assert!(
            output.status.success(),
            "Python hook failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("Python hook JSON")
    }

    #[test]
    fn rust_ground_verdict_matches_the_python_hook_contract() {
        let reports = [
            Value::Null,
            json!({"error": {"message": "no provider key"}}),
            json!({"error": {}}),
            json!({"error": []}),
            json!({"error": ""}),
            json!({"unverified_reason": "grounding surface too thin"}),
            json!({"grounded": true, "unverified_reason": "", "ungrounded": []}),
            json!({"grounded": false, "reason": "this repo has not been swept", "ungrounded": []}),
            json!({"grounded": false, "ungrounded": []}),
            json!({"ungrounded": ["frobnicate", "widgetise"]}),
            json!({"ungrounded": "frobnicate"}),
            json!({"ungrounded": 5}),
            json!({"ungrounded": [null, "Foo"]}),
            json!({"unverified_reason": "surface too thin", "ungrounded": ["x"]}),
        ];
        for report in reports {
            let actual = ground_verdict((report != Value::Null).then_some(&report));
            let expected = python_hook("ground_verdict", &report);
            let expected = expected.as_array().expect("Python verdict tuple");
            let kind = match actual.kind {
                GroundKind::Unreachable => "unreachable",
                GroundKind::Unverified => "unverified",
                GroundKind::Flagged => "flagged",
                GroundKind::Clean => "clean",
            };
            assert_eq!(Value::String(kind.to_string()), expected[0], "{report}");
            assert_eq!(Value::String(actual.detail), expected[1], "{report}");
        }
    }

    #[test]
    fn rust_sync_refusal_matches_the_python_hook_contract() {
        let live_key = format!("sk-{}", "a".repeat(32));
        let fixtures = [
            ("serve/api.py", "def handler():\n    return 1\n".to_string()),
            ("README.md", "# hello\n".to_string()),
            ("logo.png", "PNG".to_string()),
            (".env", "ESTELLE_KEY=x".to_string()),
            ("config.py", format!("OPENAI_KEY = \"{live_key}\"")),
            (
                "stripe.ts",
                "const k = \"sk_live_abcdefghij1234\"".to_string(),
            ),
            ("aws.py", format!("AWS = \"AKIA{}\"", "C".repeat(16))),
            (
                "key.md",
                "-----BEGIN RSA PRIVATE KEY-----\nMIIE".to_string(),
            ),
            ("settings.py", "PASSWORD = \"hunter2\"".to_string()),
            (".py", "print(1)".to_string()),
            ("dir/a.py", "print(1)".to_string()),
        ];
        for (path, content) in fixtures {
            let actual = hook_sync_refusal(path, &content).unwrap_or_default();
            let expected = python_hook("may_sync", &json!([path, content]));
            assert_eq!(Value::String(actual.to_string()), expected, "{path}");
        }
    }

    #[test]
    fn hook_install_and_remove_preserve_every_non_estelle_setting() {
        let root = tempfile::tempdir().expect("settings root");
        let path = root.path().join("settings.json");
        let original = json!({
            "model": "customer-model",
            "permissions": {"allow": ["Bash(git status)"]},
            "env": {"CUSTOMER_SETTING": "kept"},
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "customer-pre", "timeout": 9}]
                }],
                "PostToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "customer-post"}]
                }],
                "Stop": [{"hooks": [{"type": "command", "command": "customer-stop"}]}]
            }
        });
        fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&original).expect("fixture JSON")
            ),
        )
        .expect("fixture settings");

        install_hooks_at(&path, "'/Applications/Estelle CLI/estelle'").expect("install hooks");
        let installed: Value =
            serde_json::from_slice(&fs::read(&path).expect("installed settings"))
                .expect("installed JSON");
        assert_eq!(installed["model"], original["model"]);
        assert_eq!(installed["permissions"], original["permissions"]);
        assert_eq!(installed["env"], original["env"]);
        assert_eq!(
            installed["hooks"]["PreToolUse"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            installed["hooks"]["PostToolUse"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(installed["hooks"]["Stop"], original["hooks"]["Stop"]);
        assert!(backup_path(&path).exists());

        assert!(uninstall_hooks_at(&path).expect("remove hooks"));
        let removed: Value = serde_json::from_slice(&fs::read(&path).expect("removed settings"))
            .expect("removed JSON");
        assert_eq!(removed, original);
    }

    #[test]
    fn hook_install_refuses_unparseable_settings_without_rewriting_them() {
        let root = tempfile::tempdir().expect("settings root");
        let path = root.path().join("settings.json");
        let invalid = b"{\"permissions\": [}\n";
        fs::write(&path, invalid).expect("invalid settings");

        let error = install_hooks_at(&path, "estelle").expect_err("must refuse invalid settings");

        assert!(error.contains("refusing to overwrite unreadable"));
        assert_eq!(fs::read(&path).expect("unchanged settings"), invalid);
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn generated_hook_file_is_accepted_by_the_maintained_codex_hooks_schema() {
        let mut value = json!({});
        merge_estelle_hooks(&mut value, "estelle").expect("hook declaration");

        let parsed: codex_config::HooksFile =
            serde_json::from_value(value).expect("Codex hooks schema");

        assert_eq!(parsed.hooks.pre_tool_use.len(), 1);
        assert_eq!(parsed.hooks.post_tool_use.len(), 1);
        assert_eq!(parsed.hooks.handler_count(), 2);
    }

    #[test]
    fn every_claimed_command_has_an_explicit_execution_contract() {
        for name in crate::commands::top_level_command_names() {
            let args = crate::Args::try_parse_from(["estelle", name]).expect("claimed command");
            let command = args.command.expect("command");
            let actual = contract(&command);
            let expected = match name {
                "login" | "connect" | "remove" | "hook" | "install-hooks" | "uninstall-hooks"
                | "acp" | "mcp" | "mcp-server" => Contract::Local,
                "init" | "sweep" | "reindex" | "github" | "research" => Contract::Compound,
                _ => Contract::Remote,
            };
            assert_eq!(actual, expected, "wrong contract for {name}");
        }
    }

    #[test]
    fn customer_rendering_redacts_secret_bearing_fields_but_keeps_provider_names() {
        let reply = json!({
            "provider": "openai",
            "api_key": "estelle_live_aaaaaaaaaaaaaaaaaaaaaaaa",
            "nested": {"secret": "sk-aaaaaaaaaaaaaaaaaaaaaaaa", "model": "gpt-5"}
        });
        let rendered = summary_lines(&reply).join("\n");
        assert!(rendered.contains("openai"));
        assert!(rendered.contains("gpt-5"));
        assert!(!rendered.contains("estelle_live_"));
        assert!(!rendered.contains("sk-"));
    }

    #[test]
    fn explicit_file_collection_refuses_paths_outside_the_repo() {
        let repo = tempfile::tempdir().expect("repo");
        let outside = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("outside file");
        fs::write(outside.path(), "fn escaped() {}\n").expect("write fixture");

        let (files, skipped) = collect_files(repo.path(), &[outside.path().to_path_buf()])
            .expect("collection decision");

        assert!(files.is_empty(), "outside source content must stay local");
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains("outside repo"));
    }

    #[test]
    fn embedded_credentials_are_hidden_even_in_non_secret_fields() {
        let secret = "estelle_live_aaaaaaaaaaaaaaaaaaaaaaaa";
        let rendered = concise_value(&json!({"message": format!("received {secret}")}));
        assert!(!rendered.contains(secret));
        assert!(rendered.contains("credential hidden"));
    }

    #[test]
    #[cfg(unix)]
    fn editor_config_merge_preserves_settings_and_writes_credential_file_as_0600() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("config root");
        let path = root.path().join("mcp.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "theme": "dark",
                "mcpServers": {"other": {"url": "https://other.test/mcp"}}
            }))
            .expect("fixture json"),
        )
        .expect("fixture write");

        write_editor_config(&path, "mcpServers", "test-secret", false).expect("safe merge");

        let merged: Value =
            serde_json::from_slice(&fs::read(&path).expect("read merged")).expect("parse merged");
        assert_eq!(merged["theme"], "dark");
        assert_eq!(
            merged["mcpServers"]["other"]["url"],
            "https://other.test/mcp"
        );
        assert_eq!(
            merged["mcpServers"]["estelle"]["headers"]["Authorization"],
            "Bearer test-secret"
        );
        assert!(path.with_extension("json.bak").exists());
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn suite_renderer_discloses_new_server_fields_instead_of_rendering_only_a_header() {
        let lines = render_suite_reply(
            "monitor",
            "status",
            &json!({"active_alerts": 3, "services_seen": 8}),
        )
        .expect("render");
        let rendered = lines.join("\n");
        assert!(rendered.contains("active_alerts: 3"));
        assert!(rendered.contains("services_seen: 8"));
    }

    #[test]
    fn empty_suite_views_say_what_was_measured_instead_of_claiming_clean() {
        let issues = render_suite_reply(
            "monitor",
            "issues",
            &json!({"issues": [], "counts": {"events": 0}}),
        )
        .expect("issues")
        .join("\n");
        assert!(issues.contains("No errors have reached Estelle yet"));

        let alerts = render_suite_reply("monitor", "alerts", &json!({"rules": [], "active": []}))
            .expect("alerts")
            .join("\n");
        assert!(alerts.contains("Nothing will page you"));

        let uptime = render_suite_reply(
            "monitor",
            "uptime",
            &json!({"status": [], "counts": {"checks": 0}}),
        )
        .expect("uptime")
        .join("\n");
        assert!(uptime.contains("No uptime checks"));

        let research = render_suite_reply(
            "research",
            "status",
            &json!({"enrolled": false, "cadence": "off", "vendors": []}),
        )
        .expect("research")
        .join("\n");
        assert!(research.contains("NOT enrolled"));

        let learned = render_suite_reply("memory", "learned", &json!({"instincts": []}))
            .expect("learned")
            .join("\n");
        assert!(learned.contains("nothing is being applied on its own"));
    }

    #[test]
    fn deletion_receipts_never_render_even_a_server_redacted_key_prefix() {
        let lines = render_suite_reply(
            "memory",
            "receipts",
            &json!({
                "receipts": [{
                    "namespace": "estelle_live_0b95827…",
                    "scope": "source",
                    "target": "key:deploy-target",
                    "rows": 1
                }]
            }),
        )
        .expect("render");
        let rendered = lines.join("\n");
        assert!(!rendered.contains("estelle_live_"));
        assert!(rendered.contains("target=key:deploy-target"));
        assert!(rendered.contains("rows=1"));
    }

    #[test]
    fn github_callback_requires_the_registered_path_code_and_state() {
        assert_eq!(
            parse_github_callback("/github/callback?code=abc&state=xyz"),
            Some(Ok(("abc".to_string(), "xyz".to_string())))
        );
        assert_eq!(
            parse_github_callback(
                "/github/callback?error=access_denied&error_description=User+said+no"
            ),
            Some(Err("User said no".to_string()))
        );
        assert_eq!(
            parse_github_callback("/github/callback?state=only"),
            Some(Err("GitHub redirected without a code".to_string()))
        );
        assert_eq!(parse_github_callback("/favicon.ico"), None);
        assert_eq!(parse_github_callback("::::"), None);
        assert_eq!(
            parse_github_callback(
                "http://attacker.invalid/github/callback?code=stolen&state=wrong"
            ),
            None
        );
    }

    #[test]
    fn github_installation_choice_never_guesses_when_identity_has_more_than_one() {
        let rows = json!([
            {"id": 7, "account": "acme", "type": "Organization"},
            {"id": 9, "account": "other", "type": "User"}
        ]);
        assert!(matches!(
            pick_github_installation(rows.as_array().expect("rows"), None),
            GithubInstallationChoice::Ambiguous(_)
        ));
        assert!(matches!(
            pick_github_installation(rows.as_array().expect("rows"), Some("other")),
            GithubInstallationChoice::Chosen(ref row) if row["id"] == 9
        ));
        assert!(matches!(
            pick_github_installation(rows.as_array().expect("rows"), Some("missing")),
            GithubInstallationChoice::Unknown(_)
        ));
    }

    #[test]
    fn github_status_reports_the_bound_installation_and_only_the_next_step() {
        let lines = github_status_lines(
            &json!({"linked": true, "login": "uqeu"}),
            &json!({"installations": [{"id": 7, "account": "acme", "type": "Organization"}]}),
            &json!({"connected": true, "installations": [7]}),
        );
        let rendered = lines.join("\n");
        assert!(rendered.contains("linked as uqeu"));
        assert!(rendered.contains("7  acme (Organization)  connected"));
        assert!(rendered.contains("estelle github repos"));
        assert!(!rendered.contains("estelle github connect"));
    }

    #[test]
    fn sweep_walks_a_plain_source_directory_when_git_has_no_inventory() {
        let root = tempfile::tempdir().expect("plain source root");
        fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("source");

        let (files, skipped) = collect_files(root.path(), &[]).expect("plain collection");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "main.rs");
        assert!(skipped.is_empty());
    }

    #[test]
    fn git_inventory_excludes_ignored_secrets_and_allowlisted_source_trees() {
        let root = tempfile::tempdir().expect("git source root");
        let init = ProcessCommand::new("git")
            .arg("init")
            .arg(root.path())
            .output()
            .expect("git is required for the inventory contract");
        assert!(
            init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        fs::create_dir(root.path().join("testbed")).expect("ignored source tree");
        fs::write(root.path().join(".gitignore"), ".env\ntestbed/\n").expect("gitignore");
        fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("source");
        fs::write(root.path().join(".env"), "ESTELLE_KEY_ULTRA=live-secret\n")
            .expect("ignored secret");
        fs::write(
            root.path().join("testbed/vendor.js"),
            "export const customerFixture = true;\n",
        )
        .expect("ignored allowlisted source");

        let (files, _) = collect_files(root.path(), &[]).expect("git collection");
        let paths = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, ["main.rs"]);

        let (explicit, skipped) = collect_files(
            root.path(),
            &[PathBuf::from(".env"), PathBuf::from("testbed/vendor.js")],
        )
        .expect("explicit collection still respects Git inventory");
        assert!(explicit.is_empty());
        assert!(
            skipped
                .iter()
                .all(|path| path.contains("outside Git inventory"))
        );
    }

    #[test]
    #[cfg(unix)]
    fn git_inventory_keeps_a_newline_filename_as_one_path() {
        let root = tempfile::tempdir().expect("git source root");
        let init = ProcessCommand::new("git")
            .arg("init")
            .arg(root.path())
            .output()
            .expect("git is required for the inventory contract");
        assert!(init.status.success());
        fs::write(root.path().join("odd\nname.rs"), "fn odd_name() {}\n").expect("source");

        let (files, skipped) = collect_files(root.path(), &[]).expect("NUL-delimited inventory");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "odd\nname.rs");
        assert!(skipped.is_empty());
    }

    #[test]
    fn working_memory_contains_only_changed_staged_and_untracked_source_files() {
        let root = tempfile::tempdir().expect("git source root");
        let git = |arguments: &[&str]| {
            let output = ProcessCommand::new("git")
                .arg("-C")
                .arg(root.path())
                .args(arguments)
                .output()
                .expect("git command");
            assert!(
                output.status.success(),
                "git {:?}: {}",
                arguments,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init"]);
        fs::write(root.path().join("main.rs"), "fn baseline() {}\n").expect("baseline");
        fs::write(root.path().join("stable.rs"), "fn stable() {}\n").expect("stable");
        fs::write(root.path().join(".gitignore"), ".env\n").expect("gitignore");
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Estelle Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "baseline",
        ]);
        fs::write(root.path().join("main.rs"), "fn changed() {}\n").expect("changed");
        fs::write(root.path().join("new.rs"), "fn untracked() {}\n").expect("untracked");
        fs::write(root.path().join(".env"), "ESTELLE_KEY_ULTRA=live-secret\n")
            .expect("ignored secret");

        let files = working_memory_files(root.path()).expect("working memory");
        let paths = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, ["main.rs", "new.rs"]);
        assert!(
            files
                .iter()
                .all(|file| !file.content.contains("live-secret"))
        );
    }

    #[test]
    fn sweep_uses_background_ingest_at_the_measured_200_file_boundary() {
        assert_eq!(sweep_transport(199), SweepTransport::Sync);
        assert_eq!(sweep_transport(200), SweepTransport::Background);
        assert_eq!(sweep_transport(4_000), SweepTransport::Background);
    }

    #[test]
    fn research_watch_does_not_turn_an_empty_update_into_daily_enrolment() {
        assert_eq!(research_watch_body(&[]).expect("empty update"), json!({}));
        assert_eq!(
            research_watch_body(&["stripe".to_string()]).expect("default cadence"),
            json!({"cadence": "daily", "vendors": [{"name": "stripe"}]})
        );
        assert_eq!(
            research_watch_body(&[
                "stripe".to_string(),
                "--cadence".to_string(),
                "weekly".to_string(),
                "--api".to_string(),
                "charges.create,refunds.create".to_string(),
            ])
            .expect("custom vendor"),
            json!({
                "cadence": "weekly",
                "vendors": [{"name": "stripe", "apis": ["charges.create", "refunds.create"]}]
            })
        );
    }

    #[test]
    fn memory_erasure_discloses_the_true_radius_and_waits_for_yes() {
        let blocked = erasure_gate("forget", &["billing/charge.rs".to_string()], false)
            .expect("unconfirmed erasure must be blocked");
        let text = blocked.join("\n");
        assert!(text.contains("ALL namespaces"), "true radius hidden\n{text}");
        assert!(text.contains("--yes"), "no remedy named\n{text}");
        assert!(text.contains("Nothing was sent"), "no nothing-sent line\n{text}");
        assert!(text.contains("billing/charge.rs"), "target missing\n{text}");

        assert!(
            erasure_gate("forget", &["billing/charge.rs".to_string()], true).is_none(),
            "a confirmed erasure must proceed"
        );
        assert!(
            erasure_gate("retract", &["key:deploy-target".to_string()], false).is_some(),
            "retract skips the gate"
        );
        assert!(
            erasure_gate("receipts", &[], false).is_none(),
            "a read must never hit the gate"
        );
    }

    #[test]
    fn memory_flags_are_not_mistaken_for_the_thing_being_erased() {
        // S2: erasure is account-wide on the server; the client no longer demands or injects a
        // repo the server never reads — a field demanded by the client and ignored by the
        // server is the blast-radius lie itself.
        assert_eq!(memory_scope(Endpoint::Retract), MemoryScope::Account);
        assert_eq!(memory_scope(Endpoint::Forget), MemoryScope::Account);
        assert_eq!(memory_scope(Endpoint::Unlearn), MemoryScope::Account);
        assert_eq!(
            memory_request("receipts", &["--limit".to_string(), "5".to_string()])
                .expect("receipts"),
            (
                Endpoint::DeletionReceipts,
                MemoryMethod::Get,
                json!({"limit": 5})
            )
        );
        assert_eq!(
            memory_request(
                "retract",
                &[
                    "key:deploy-target".to_string(),
                    "--reason".to_string(),
                    "withdrawn".to_string(),
                ],
            )
            .expect("retract"),
            (
                Endpoint::Retract,
                MemoryMethod::Post,
                json!({"subject": "key:deploy-target", "reason": "withdrawn"})
            )
        );
        assert_eq!(
            memory_request(
                "unlearn",
                &["when touching auth".to_string(), "run security".to_string()],
            )
            .expect("reflex"),
            (
                Endpoint::Unlearn,
                MemoryMethod::Post,
                json!({"instinct": {"trigger": "when touching auth", "response": "run security"}})
            )
        );
        assert_eq!(
            memory_request("unlearn", &["--skill".to_string(), "test-gen".to_string()],)
                .expect("skill"),
            (
                Endpoint::Unlearn,
                MemoryMethod::Post,
                json!({"skill": "test-gen"})
            )
        );
    }
}
