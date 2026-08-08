use estelle_client::Endpoint;
use serde_json::Value;
use serde_json::json;

pub(crate) const SESSION_COMMANDS: [&str; 24] = [
    "help",
    "init",
    "graph",
    "memory",
    "sweep",
    "sessions",
    "resume",
    "work",
    "orchestra",
    "context",
    "gate",
    "scan",
    "improve",
    "verify",
    "apply",
    "undo",
    "mode",
    "routing",
    "status",
    "skills",
    "tools",
    "shell",
    "clear",
    "exit",
];

const SESSION_HELP: [(&str, &str); 24] = [
    ("help", "what you can do here"),
    ("init", "a grounded brief of this repo"),
    ("graph", "the swept code graph as counts and roots"),
    ("memory", "what Estelle knows about this repo"),
    ("sweep", "index this repo into memory"),
    ("sessions", "your recent sessions"),
    ("resume", "pick a past session back up"),
    ("work", "plan, implement, gate and repair a change"),
    ("orchestra", "run one gated server task"),
    ("context", "toggle grounding context side panel (Alt+M)"),
    ("gate", "run the merge gate on your staged diff"),
    ("scan", "scan the staged diff for security findings"),
    ("improve", "rank grounded improvements for this repo"),
    ("verify", "check code for APIs that do not exist"),
    ("apply", "write the last /work diff to the working tree"),
    ("undo", "reverse the last explicit /apply"),
    ("mode", "read or lower the server autonomy ceiling"),
    ("routing", "show the server's model route and reason"),
    ("status", "endpoint, credential, repo and connection state"),
    ("skills", "browse Estelle playbooks"),
    ("tools", "list every MCP tool Estelle exposes"),
    ("shell", "explain the !command form"),
    ("clear", "clear the transcript"),
    ("exit", "leave the session"),
];

// Codex's maintained slash surface stays reachable even where Estelle deliberately
// deleted the local agent behind it. P5-GRAFTS.md records the disposition of each row.
const GRAFT_HELP: &[(&str, &str)] = &[
    ("prod", "toggle live production health"),
    ("todo", "toggle the server-emitted task ledger (Ctrl+T)"),
    ("settings", "open interactive Estelle settings"),
    ("model", "browse your BYOK model pool"),
    ("plan", "enter the server-enforced planning ceiling"),
    ("memories", "what Estelle knows about this repo"),
    ("mcp", "list Estelle's MCP tools"),
    ("grep", "search code with server-side structure"),
    ("ide", "IDE context status"),
    ("permissions", "view the effective autonomy boundary"),
    ("keymap", "composer keymap status"),
    ("vim", "Vim composer mode status"),
    ("setup-default-sandbox", "sandbox ownership status"),
    ("sandbox-add-read-dir", "sandbox ownership status"),
    ("experimental", "experimental feature ownership status"),
    ("approve", "approval ownership status"),
    ("import", "cross-harness import status"),
    ("hooks", "canonical Estelle hook status"),
    ("review", "run Estelle's grounded merge gate"),
    ("rename", "session-title ownership status"),
    ("new", "new-session ownership status"),
    ("archive", "archive ownership status"),
    ("delete", "delete-session ownership status"),
    ("fork", "fork-session ownership status"),
    ("app", "desktop handoff ownership status"),
    ("compact", "context compaction ownership status"),
    ("goal", "long-running goal ownership status"),
    ("agent", "agent-thread ownership status"),
    ("side", "ephemeral side-question ownership status"),
    ("btw", "ephemeral side-question ownership status"),
    ("copy", "copy the last answer"),
    ("raw", "raw scrollback status"),
    ("diff", "show the local working-tree diff"),
    ("mention", "file-mention ownership status"),
    ("usage", "account-usage ownership status"),
    ("debug-config", "configuration ownership status"),
    ("title", "terminal-title ownership status"),
    ("statusline", "status-line ownership status"),
    ("theme", "terminal theme status"),
    ("pet", "decorative terminal feature status"),
    ("apps", "application connector ownership status"),
    ("plugins", "server plugin ownership status"),
    ("logout", "credential removal status"),
    ("feedback", "feedback transport ownership status"),
    ("rollout", "local rollout ownership status"),
    ("ps", "background process ownership status"),
    ("stop", "background process ownership status"),
    ("personality", "personality ownership status"),
    ("test-approval", "approval test ownership status"),
    ("subagents", "server orchestra view status"),
    ("debug-m-drop", "deleted local-memory debug command"),
    ("debug-m-update", "deleted local-memory debug command"),
    // Kimi interaction surfaces not already present above.
    ("version", "show this Estelle build"),
    ("editor", "external-editor ownership status"),
    ("changelog", "release-note ownership status"),
    ("add-dir", "additional-directory ownership status"),
    ("export", "session export ownership status"),
    ("task", "view server orchestra work"),
    ("web", "web application ownership status"),
    ("vis", "trace visualizer ownership status"),
    ("upgrade", "upgrade ownership status"),
    ("yolo", "deleted unbounded-approval mode"),
    ("afk", "deleted unattended local-agent mode"),
];

#[cfg(test)]
pub(crate) const TOP_LEVEL_COMMANDS: [&str; 20] = [
    "login",
    "init",
    "sweep",
    "reindex",
    "connect",
    "remove",
    "github",
    "monitor",
    "research",
    "memory",
    "ask",
    "recall",
    "verify",
    "gate",
    "hook",
    "install-hooks",
    "uninstall-hooks",
    "acp",
    "mcp",
    "mcp-server",
];

#[cfg(test)]
pub(crate) fn session_command_names() -> [&'static str; 24] {
    SESSION_COMMANDS
}

#[cfg(test)]
pub(crate) fn top_level_command_names() -> [&'static str; 20] {
    TOP_LEVEL_COMMANDS
}

pub(crate) fn help_lines() -> Vec<String> {
    SESSION_HELP
        .iter()
        .chain(GRAFT_HELP.iter())
        .map(|(name, description)| {
            let surface = if *name == "shell" {
                "!<cmd>".to_string()
            } else {
                format!("/{name}")
            };
            format!("{surface:<12}{description}")
        })
        .collect()
}

pub(crate) fn palette_rows(input: &str) -> Vec<(&'static str, &'static str)> {
    let Some(command) = input.trim_start().strip_prefix('/') else {
        return Vec::new();
    };
    let query = command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if command.contains(char::is_whitespace) {
        return Vec::new();
    }
    let mut rows = SESSION_HELP
        .iter()
        .chain(GRAFT_HELP.iter())
        .filter_map(|(name, description)| {
            let tier = if query.is_empty() || name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if one_edit(name, &query) {
                2
            } else {
                return None;
            };
            Some((tier, *name, *description))
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|(tier, name, _)| (*tier, name.len(), *name));
    rows.into_iter()
        .take(8)
        .map(|(_, name, description)| (name, description))
        .collect()
}

pub(crate) fn resolve_session_name(raw: &str) -> Option<&'static str> {
    let name = raw.trim().to_ascii_lowercase();
    match name.as_str() {
        "route" => return Some("routing"),
        "quit" => return Some("exit"),
        _ => {}
    }
    if let Some(exact) = SESSION_COMMANDS
        .iter()
        .chain(GRAFT_HELP.iter().map(|(name, _)| name))
        .find(|candidate| **candidate == name)
    {
        return Some(*exact);
    }
    let mut matches = SESSION_COMMANDS
        .iter()
        .copied()
        .chain(GRAFT_HELP.iter().map(|(name, _)| *name))
        .filter(|candidate| one_edit(candidate, &name));
    let candidate = matches.next()?;
    matches.next().is_none().then_some(candidate)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParsedInput {
    Ask(String),
    Shell(String),
    Command {
        name: Option<&'static str>,
        typed_name: String,
        argument: String,
    },
}

impl ParsedInput {
    pub(crate) fn local_refusal(&self) -> Option<&'static str> {
        match self {
            Self::Command {
                name: Some("work"),
                argument,
                ..
            } if argument.trim().is_empty() => Some("/work needs a task"),
            Self::Command {
                name: Some("orchestra"),
                argument,
                ..
            } if argument.trim().is_empty() => Some("/orchestra needs a task"),
            _ => None,
        }
    }
}

pub(crate) fn parse_input(raw: &str) -> ParsedInput {
    let input = raw.trim();
    if let Some(shell) = input.strip_prefix('!') {
        return ParsedInput::Shell(shell.trim().to_string());
    }
    let Some(command) = input.strip_prefix('/') else {
        return ParsedInput::Ask(input.to_string());
    };
    let mut words = command.split_whitespace();
    let typed_name = words.next().unwrap_or_default().to_ascii_lowercase();
    let trailing = words.collect::<Vec<_>>().join(" ");
    if let Some(skill) = typed_name.strip_prefix("skill:") {
        let argument = if trailing.is_empty() {
            skill.to_string()
        } else {
            format!("{skill} {trailing}")
        };
        return ParsedInput::Command {
            name: (!skill.is_empty()).then_some("skill:"),
            typed_name,
            argument,
        };
    }
    ParsedInput::Command {
        name: resolve_session_name(&typed_name),
        typed_name,
        argument: trailing,
    }
}

pub(crate) fn inherited_command_lines(name: &str) -> Option<Vec<String>> {
    let deleted = |reason: &str| {
        vec![
            format!("/{name} was deleted from Estelle."),
            reason.to_string(),
            "No local agent action ran and no request was sent.".to_string(),
        ]
    };
    let repointed = |owner: &str, command: &str| {
        vec![
            format!("/{name} is owned by {owner}."),
            command.to_string(),
            "Nothing was inferred from an inherited Codex backend.".to_string(),
        ]
    };
    match name {
        "ide" => Some(repointed(
            "the editor MCP connection",
            "Run estelle init --client <name> to configure it.",
        )),
        "keymap" | "vim" | "raw" | "theme" | "title" | "statusline" | "editor" => Some(repointed(
            "the maintained terminal layer",
            "This compact Estelle session has no persisted setting for it yet.",
        )),
        "setup-default-sandbox" | "sandbox-add-read-dir" => Some(deleted(
            "Estelle's repair sandbox is server-side; a second client sandbox would assert the wrong security boundary.",
        )),
        "experimental" | "approve" | "test-approval" => Some(deleted(
            "OpenAI's local agent feature and approval brain were removed; Estelle's server autonomy gate is authoritative.",
        )),
        "import" => Some(repointed(
            "Estelle sessions",
            "Cross-harness session import has no server endpoint today; existing /sessions remain available.",
        )),
        "rename" | "new" | "archive" | "delete" | "fork" | "goal" => Some(repointed(
            "Estelle sessions",
            "The current server contract exposes read/resume, not this mutation; the command is visible and inert.",
        )),
        "app" | "web" => Some(repointed(
            "fatelabs.ca",
            "Browser launching is not performed from the TUI without an explicit URL contract.",
        )),
        "compact" => Some(repointed(
            "server session memory",
            "There is no compaction endpoint today; Estelle will not fake a local summary as shared memory.",
        )),
        "agent" | "subagents" | "task" => Some(repointed(
            "Estelle /orchestra",
            "Use /orchestra <task> to run one server task. The fixed fleet view opens only when the server emits revisioned live state; production does not emit it yet.",
        )),
        "side" | "btw" => Some(repointed(
            "the current Estelle session",
            "Ephemeral forks have no server owner today, so this command does not create a second local agent.",
        )),
        "copy" => Some(repointed(
            "the terminal's native selection",
            "Rendered answers remain selectable; clipboard mutation is not performed implicitly.",
        )),
        "diff" => Some(repointed(
            "the local Git working tree",
            "Use !git diff --no-color; no diff is sent until /gate, /scan, or /review is requested.",
        )),
        "mention" => Some(repointed(
            "Working memory",
            "Changed files attach automatically to ordinary questions; explicit @mention parsing has no server contract yet.",
        )),
        "usage" => Some(repointed(
            "the account API",
            "The accepted 50-endpoint CLI contract does not include /analytics; /status shows available account state.",
        )),
        "debug-config" | "rollout" | "ps" | "stop" => Some(deleted(
            "This inspected OpenAI's local agent/app-server state, which is not an Estelle runtime.",
        )),
        "apps" | "plugins" => Some(repointed(
            "Estelle's server integrations",
            "No matching endpoint exists in the accepted CLI contract; the inherited OpenAI catalog is not shown.",
        )),
        "logout" => Some(repointed(
            "Estelle credential storage",
            "Use the top-level credential workflow; this session never touches OpenAI auth.",
        )),
        "feedback" => Some(deleted(
            "OpenAI feedback transport was removed and Estelle has no replacement endpoint in this contract.",
        )),
        "pet" => Some(deleted(
            "Decorative local-agent state is not part of Estelle's working terminal surface.",
        )),
        "personality" => Some(deleted(
            "Estelle's server prompt owns response policy; a competing client-side personality brain would drift.",
        )),
        "debug-m-drop" | "debug-m-update" => Some(deleted(
            "Codex local memory generation was removed; Estelle's memory endpoints are authoritative.",
        )),
        "version" => Some(vec![format!("Estelle {}", env!("CARGO_PKG_VERSION"))]),
        "changelog" | "upgrade" => Some(repointed(
            "the public @fatelabs/estelle release",
            "This build does not fetch release metadata from inside the TUI.",
        )),
        "add-dir" => Some(deleted(
            "A repository session has one explicit root; widening local read scope needs a reviewed consent contract.",
        )),
        "export" => Some(repointed(
            "Estelle session persistence",
            "The server exposes session reads but no export endpoint in the accepted contract.",
        )),
        "vis" => Some(deleted(
            "Kimi's local trace visualizer is not an Estelle server surface.",
        )),
        "yolo" | "afk" => Some(deleted(
            "Unbounded or unattended local approval contradicts Estelle's server-enforced autonomy ceiling.",
        )),
        _ => None,
    }
}

const MODES: [&str; 4] = ["read_only", "propose", "branch", "execute"];

pub(crate) fn parse_mode(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read" | "plan" | "read_only" | "read-only" | "readonly" => Some("read_only"),
        "edit" | "propose" | "pr" => Some("propose"),
        "branch" => Some("branch"),
        "auto" | "execute" => Some("execute"),
        _ => None,
    }
}

pub(crate) fn mode_rank(value: &str) -> Option<usize> {
    MODES.iter().position(|mode| *mode == value)
}

pub(crate) fn mode_name(value: &str) -> &str {
    match value {
        "read_only" => "plan",
        "propose" => "edit",
        "execute" => "auto",
        value => value,
    }
}

pub(crate) fn effective_mode(local: &str, server: Option<&str>) -> &'static str {
    let local_rank = mode_rank(local).unwrap_or(0);
    let server_rank = server.and_then(mode_rank).unwrap_or(local_rank);
    MODES[local_rank.min(server_rank)]
}

pub(crate) fn mode_lines(local: &str, server: Option<&str>) -> Vec<String> {
    let effective = effective_mode(local, server);
    let what = match effective {
        "read_only" => "reads, answers, and verifies; nothing is written",
        "propose" => "writes a sandboxed diff for human review",
        "branch" => "may push a reviewable branch",
        "execute" => "merges only when every server guard passes; otherwise returns a PR",
        _ => "unknown capability",
    };
    vec![
        format!("local ceiling  {}", mode_name(local)),
        format!(
            "account ceiling  {}",
            server
                .map(mode_name)
                .unwrap_or("unknown (server still enforces it)")
        ),
        format!("effective  {} - {what}", mode_name(effective)),
    ]
}

fn one_edit(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    if left.len().abs_diff(right.len()) > 1 {
        return false;
    }
    let (longer, shorter) = if left.len() >= right.len() {
        (left.as_bytes(), right.as_bytes())
    } else {
        (right.as_bytes(), left.as_bytes())
    };
    let mut long_index = 0;
    let mut short_index = 0;
    let mut changes = 0;
    while long_index < longer.len() && short_index < shorter.len() {
        if longer[long_index] == shorter[short_index] {
            long_index += 1;
            short_index += 1;
            continue;
        }
        changes += 1;
        if changes > 1 {
            return false;
        }
        long_index += 1;
        if longer.len() == shorter.len() {
            short_index += 1;
        }
    }
    changes + (longer.len() - long_index) + (shorter.len() - short_index) <= 1
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RemoteMethod {
    Get,
    Post,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RemoteRequest {
    pub(crate) name: &'static str,
    pub(crate) endpoint: Endpoint,
    pub(crate) method: RemoteMethod,
    pub(crate) body: Option<Value>,
    pub(crate) query: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteError {
    MissingDiff,
}

pub(crate) fn remote_request(
    name: &'static str,
    argument: &str,
    diff: Option<&str>,
    last_question: Option<&str>,
) -> Result<Option<RemoteRequest>, RouteError> {
    let argument = argument.trim();
    let get = |endpoint, query| {
        Ok(Some(RemoteRequest {
            name,
            endpoint,
            method: RemoteMethod::Get,
            body: None,
            query,
        }))
    };
    let post = |endpoint, body| {
        Ok(Some(RemoteRequest {
            name,
            endpoint,
            method: RemoteMethod::Post,
            body: Some(body),
            query: json!({}),
        }))
    };
    match name {
        "init" => get(Endpoint::Wiki, json!({})),
        "graph" => get(Endpoint::Graph, json!({})),
        "memory" => post(
            Endpoint::DeepSearch,
            json!({"question": "what do you know about this repo?"}),
        ),
        "memories" => post(
            Endpoint::DeepSearch,
            json!({"question": "what do you know about this repo?"}),
        ),
        "model" if argument.is_empty() => get(Endpoint::Providers, json!({})),
        "grep" => post(Endpoint::Search, json!({"query": argument, "code": true})),
        "skill:" => {
            let mut parts = argument.splitn(2, char::is_whitespace);
            let skill = parts.next().unwrap_or_default();
            let task = parts.next().unwrap_or_default().trim();
            post(Endpoint::SkillRun, json!({"skill": skill, "task": task}))
        }
        "sessions" => get(Endpoint::Sessions, json!({})),
        "resume" => get(Endpoint::Session, json!({"id": argument})),
        "work" => post(Endpoint::Work, json!({"task": argument})),
        "orchestra" => post(Endpoint::Orchestra, json!({"tasks": [argument]})),
        "gate" | "scan" | "review" => {
            let diff = diff
                .filter(|value| !value.trim().is_empty())
                .ok_or(RouteError::MissingDiff)?;
            post(
                if matches!(name, "gate" | "review") {
                    Endpoint::Gate
                } else {
                    Endpoint::Scan
                },
                json!({"diff": diff}),
            )
        }
        "improve" => post(
            Endpoint::Improve,
            if argument.is_empty() {
                json!({})
            } else {
                json!({"focus": argument})
            },
        ),
        "verify" => post(Endpoint::Verify, json!({"answer": argument})),
        "routing" => {
            let subject = (!argument.is_empty())
                .then_some(argument)
                .or(last_question.filter(|value| !value.trim().is_empty()));
            post(
                Endpoint::Route,
                subject.map_or_else(
                    || json!({"task_kind": "chat"}),
                    |prompt| json!({"prompt": prompt}),
                ),
            )
        }
        "skills" => get(Endpoint::Skills, json!({})),
        "tools" => post(
            Endpoint::Mcp,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        ),
        "mcp" => post(
            Endpoint::Mcp,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        ),
        _ => Ok(None),
    }
}

pub(crate) fn render_remote_reply(name: &str, reply: &estelle_client::CommandReply) -> Vec<String> {
    match name {
        "init" => {
            let Some(wiki) = reply
                .wiki
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            else {
                return vec![format!(
                    "No repo brief exists yet for {}. Run estelle sweep first.",
                    reply.repo.as_deref().unwrap_or("this repo")
                )];
            };
            let mut lines = vec![format!(
                "{}{}",
                reply.repo.as_deref().unwrap_or("repo"),
                reply
                    .scope
                    .as_deref()
                    .map(|scope| format!("  |  {scope}"))
                    .unwrap_or_default()
            )];
            lines.extend(wiki.lines().map(str::to_string));
            lines
        }
        "graph" => {
            let repo = reply.repo.as_deref().unwrap_or("this repo");
            if reply.graph_building == Some(true) {
                return vec![format!(
                    "The repo graph for {repo} is being built — the server is warming a cold surface; ask again in a moment."
                )];
            }
            let Some(files) = reply.graph_files else {
                return vec![format!(
                    "The server returned no graph summary for {repo}. Absent is absent — no counts are invented."
                )];
            };
            if files == 0 {
                return vec![format!(
                    "No swept graph for {repo} yet. Run estelle sweep first."
                )];
            }
            let count = |value: Option<u64>| {
                value
                    .map(|number| number.to_string())
                    .unwrap_or_else(|| "not returned".to_string())
            };
            let mut lines = vec![format!(
                "{}{}",
                repo,
                reply
                    .scope
                    .as_deref()
                    .map(|scope| format!("  |  {scope}"))
                    .unwrap_or_default()
            )];
            lines.push(format!(
                "{} files  |  {} entities  |  {} subsystems  |  {} import cycles",
                count(reply.graph_files),
                count(reply.graph_entities),
                count(reply.graph_subsystems),
                count(reply.graph_cycles)
            ));
            if !reply.graph_roots.is_empty() {
                lines.push(String::new());
                lines.extend(reply.graph_roots.iter().take(8).map(|root| {
                    format!(
                        "{:>6}  {}",
                        root.files
                            .map(|files| files.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                        root.name
                    )
                }));
            }
            lines
        }
        "memory" | "memories" => reply
            .answer
            .as_deref()
            .filter(|answer| !answer.trim().is_empty())
            .map(|answer| answer.lines().map(str::to_string).collect())
            .unwrap_or_else(|| vec!["No memory recall came back for this repo.".to_string()]),
        "sessions" => {
            if reply.sessions.is_empty() {
                return vec!["No sessions yet. This one is the first.".to_string()];
            }
            let mut lines = vec![format!(
                "{} of {} sessions  |  /resume <id> to pick one up",
                reply.sessions.len(),
                reply.count.unwrap_or(reply.sessions.len() as u64)
            )];
            for session in reply.sessions.iter().take(10) {
                lines.push(format!(
                    "{}  {}{}",
                    session
                        .id
                        .as_ref()
                        .map(json_scalar)
                        .unwrap_or_else(|| "?".to_string()),
                    session.title.as_deref().unwrap_or("(untitled)"),
                    session
                        .run_count
                        .map(|count| format!("  |  {count} runs"))
                        .unwrap_or_default()
                ));
            }
            lines
        }
        "resume" => {
            let mut lines = vec![format!(
                "{}  {}",
                reply.title.as_deref().unwrap_or("(untitled session)"),
                reply.id.as_ref().map(json_scalar).unwrap_or_default()
            )];
            if let Some(count) = reply.run_count {
                lines.push(format!("{count} runs"));
            }
            if let Some(meaning) = &reply.meaning {
                lines.extend(meaning.lines().map(str::to_string));
            }
            lines
        }
        "work" => {
            let mut lines = reply
                .answer
                .as_deref()
                .map(|answer| answer.lines().map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            if reply
                .diff
                .as_deref()
                .is_some_and(|diff| !diff.trim().is_empty())
            {
                lines.push("A reviewable diff is ready. Use /apply to write it.".to_string());
            }
            nonblank_or(
                lines,
                "Work completed without a displayable answer or diff.",
            )
        }
        "orchestra" => {
            if let Some(fleet) = &reply.fleet {
                return fleet_view_lines(fleet, 160);
            }
            let mut lines = vec![format!(
                "{} agents{}",
                reply.count.unwrap_or(reply.runs.len() as u64),
                reply
                    .level
                    .as_deref()
                    .map(|level| format!("  |  at {level}"))
                    .unwrap_or_default()
            )];
            for run in reply.runs.iter().take(12) {
                let task = run
                    .task
                    .as_deref()
                    .or(run.subtask.as_deref())
                    .or(run.title.as_deref())
                    .unwrap_or("task");
                let details = [
                    run.model.as_deref(),
                    run.tier.as_deref(),
                    run.effort.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("  |  ");
                lines.push(if details.is_empty() {
                    format!("- {task}")
                } else {
                    format!("- {task}  |  {details}")
                });
                if run.grounded == Some(false) {
                    lines.push(format!(
                        "  not grounded{}",
                        run.reason
                            .as_deref()
                            .map(|reason| format!(": {reason}"))
                            .unwrap_or_default()
                    ));
                }
            }
            lines
        }
        "gate" | "review" => {
            let verdict = reply
                .verdict
                .as_ref()
                .map(json_scalar)
                .or_else(|| reply.gate.as_ref().map(json_scalar))
                .or_else(|| reply.merge.as_ref().map(json_scalar))
                .unwrap_or_else(|| "unverified".to_string());
            let mut lines = vec![format!("Verdict  {verdict}")];
            append_object_rows(&mut lines, reply.extra.get("blockers"), "BLOCKED");
            append_object_rows(&mut lines, reply.extra.get("warnings"), "warning");
            lines
        }
        "scan" => {
            if reply.findings.is_empty() {
                return vec!["Scan clean. No findings in this diff.".to_string()];
            }
            let mut lines = vec![format!(
                "{} findings",
                reply.count.unwrap_or(reply.findings.len() as u64)
            )];
            for finding in reply.findings.iter().take(20) {
                lines.push(format!(
                    "{}{}  {}  {}",
                    finding.path.as_deref().unwrap_or("?"),
                    finding
                        .line
                        .map(|line| format!(":{line}"))
                        .unwrap_or_default(),
                    finding.severity.as_deref().unwrap_or("unknown"),
                    finding
                        .body
                        .as_deref()
                        .or(finding.title.as_deref())
                        .unwrap_or("finding")
                ));
            }
            lines
        }
        "improve" => {
            if reply.proposals.is_empty() {
                return vec!["No ranked improvements came back for this repo.".to_string()];
            }
            let mut lines = vec![format!("{} ranked improvements", reply.proposals.len())];
            for proposal in reply.proposals.iter().take(10) {
                lines.push(format!(
                    "- {}{}",
                    proposal.title.as_deref().unwrap_or("improvement"),
                    proposal
                        .file
                        .as_deref()
                        .map_or_else(String::new, |file| format!(
                            "  |  {file}{}",
                            proposal
                                .line
                                .map(|line| format!(":{line}"))
                                .unwrap_or_default()
                        ))
                ));
                if let Some(action) = &proposal.suggested_action {
                    lines.push(format!("  {action}"));
                }
            }
            lines
        }
        "verify" => {
            if reply.grounded == Some(true) {
                return vec![
                    "Grounded. Every referenced API exists in the swept repo.".to_string(),
                ];
            }
            if reply.scope_ask {
                let mut lines = vec![
                    reply
                        .question
                        .clone()
                        .unwrap_or_else(|| "Which repo should I check this against?".to_string()),
                ];
                lines.extend(
                    reply
                        .candidates
                        .iter()
                        .map(|candidate| format!("- {candidate}")),
                );
                if let Some(reason) = reply.unverified_reason.as_ref().or(reply.reason.as_ref()) {
                    lines.push(reason.clone());
                }
                return lines;
            }
            if !reply.ungrounded.is_empty() {
                let mut lines = vec!["Ungrounded references:".to_string()];
                lines.extend(reply.ungrounded.iter().map(|item| format!("- {item}")));
                return lines;
            }
            if let Some(reason) = reply
                .reason
                .as_ref()
                .or(reply.unverified_reason.as_ref())
                .filter(|reason| !reason.trim().is_empty())
            {
                return vec![format!("Not verified. {reason}")];
            }
            vec!["Verification could not establish a grounded verdict.".to_string()]
        }
        "mode" => vec![format!(
            "Autonomy ceiling  {}",
            reply
                .extra
                .get("global")
                .map(json_scalar)
                .or_else(|| reply.extra.get("mode").map(json_scalar))
                .unwrap_or_else(|| "unknown".to_string())
        )],
        "routing" => vec![format!(
            "{} -> {}{}",
            reply.provider.as_deref().unwrap_or("server"),
            reply.routed.as_deref().unwrap_or("unknown"),
            reply
                .reason
                .as_deref()
                .map(|reason| format!("  |  {reason}"))
                .unwrap_or_default()
        )],
        "skills" => render_registry(reply.extra.get("skills"), "playbooks", "summary"),
        "tools" | "mcp" => render_registry(
            reply.result.as_ref().and_then(|result| result.get("tools")),
            "tools",
            "description",
        ),
        "model" => render_model_pool(reply),
        "grep" => render_structural_search(reply),
        "skill:" => render_skill_reply(reply),
        _ => render_unknown_reply(reply),
    }
}

fn render_model_pool(reply: &estelle_client::CommandReply) -> Vec<String> {
    let configured = reply
        .extra
        .get("configured")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let active = reply.extra.get("active").and_then(Value::as_object);
    let active_provider = active
        .and_then(|row| row.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let active_model = active
        .and_then(|row| row.get("model"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut lines = vec![
        "Estelle routing (auto)  |  strongest appropriate model across your pool".to_string(),
        "your pool".to_string(),
    ];
    for provider in reply
        .extra
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = provider.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !configured.contains(&id) {
            continue;
        }
        let label = provider.get("label").and_then(Value::as_str).unwrap_or(id);
        for model in provider
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let current = if id == active_provider && model == active_model {
                "  |  account active"
            } else {
                ""
            };
            lines.push(format!("{model}  |  {label}  |  your key{current}"));
        }
    }
    if lines.len() == 2 {
        lines.push("No BYOK providers are configured.".to_string());
    }
    lines.push("Session pin unavailable; model selection is account-wide.".to_string());
    lines.push(
        "Set the account-wide provider/model at https://fatelabs.ca/dashboard/provider."
            .to_string(),
    );
    lines.push(
        "Auto routing remains active: planning uses the strongest configured model; implementation uses the cheapest capable model."
            .to_string(),
    );
    lines
}

fn render_structural_search(reply: &estelle_client::CommandReply) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(recall) = reply
        .extra
        .get("recall")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        lines.extend(recall.lines().map(str::to_string));
    }
    if let Some(answer) = reply.extra.get("code_answer").and_then(Value::as_str) {
        lines.push(answer.to_string());
    }
    if let Some(rows) = reply.extra.get("code").and_then(Value::as_array) {
        for row in rows.iter().take(40) {
            let file = row
                .get("source_file")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let exact = row.get("line").and_then(Value::as_u64);
            let approximate = row.get("approx_line").and_then(Value::as_u64);
            let location = exact
                .map(|line| format!("{file}:{line}"))
                .or_else(|| approximate.map(|line| format!("{file}:~{line}")))
                .unwrap_or_else(|| file.to_string());
            let text = row.get("text").and_then(Value::as_str).unwrap_or_default();
            lines.push(format!("{location}  |  {text}"));
        }
    }
    nonblank_or(lines, "No structural matches were returned.")
}

fn render_skill_reply(reply: &estelle_client::CommandReply) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(skill) = reply.extra.get("skill").and_then(Value::as_str) {
        lines.push(format!("skill:{skill}"));
    }
    if let Some(answer) = reply.extra.get("reply").and_then(Value::as_str) {
        lines.extend(answer.lines().map(str::to_string));
    }
    if let Some(grounding) = reply.extra.get("grounding").and_then(Value::as_object) {
        let attached = grounding
            .get("attached")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let repo = grounding
            .get("repo")
            .and_then(Value::as_str)
            .unwrap_or("unresolved");
        lines.push(format!(
            "grounding  {}  |  {repo}",
            if attached { "attached" } else { "not attached" }
        ));
    }
    nonblank_or(lines, "The skill returned no displayable result.")
}

fn render_registry(value: Option<&Value>, label: &str, description_key: &str) -> Vec<String> {
    let rows = value.and_then(Value::as_array).cloned().unwrap_or_default();
    if rows.is_empty() {
        return vec![format!("No {label} were returned.")];
    }
    let mut lines = vec![format!("{} {label}", rows.len())];
    for row in rows.iter().take(40) {
        let name = row
            .get("name")
            .map(json_scalar)
            .unwrap_or_else(|| "?".to_string());
        let description = row
            .get(description_key)
            .or_else(|| row.get("short"))
            .map(json_scalar)
            .unwrap_or_default();
        lines.push(if description.is_empty() {
            name
        } else {
            format!("{name}  |  {description}")
        });
    }
    lines
}

fn append_object_rows(lines: &mut Vec<String>, value: Option<&Value>, label: &str) {
    let Some(rows) = value.and_then(Value::as_array) else {
        return;
    };
    for row in rows {
        let body = row
            .get("body")
            .or_else(|| row.get("message"))
            .map(json_scalar)
            .unwrap_or_else(|| json_scalar(row));
        lines.push(format!("{label}: {body}"));
    }
}

fn render_unknown_reply(reply: &estelle_client::CommandReply) -> Vec<String> {
    let mut fields = reply.extra.keys().cloned().collect::<Vec<_>>();
    fields.sort();
    if fields.is_empty() {
        vec!["The server replied with an empty body. Nothing can be rendered.".to_string()]
    } else {
        vec![format!(
            "This build has no renderer for fields: {}",
            fields.join(", ")
        )]
    }
}

fn nonblank_or(lines: Vec<String>, fallback: &str) -> Vec<String> {
    if lines.iter().any(|line| !line.trim().is_empty()) {
        lines
    } else {
        vec![fallback.to_string()]
    }
}

pub(crate) fn fleet_view_lines(fleet: &estelle_client::FleetSnapshot, width: u16) -> Vec<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64());
    fleet_view_lines_at(fleet, width, now)
}

fn fleet_view_lines_at(
    fleet: &estelle_client::FleetSnapshot,
    width: u16,
    now_epoch_s: f64,
) -> Vec<String> {
    const COLUMNS: usize = 5;
    let total = fleet.total.unwrap_or(fleet.agents.len() as u64);
    let total_label = fleet
        .total
        .map_or_else(|| "?".to_string(), |value| value.to_string());
    let batch = nonempty(&fleet.batch, "unnamed batch");
    let model_roster = fleet_model_roster(fleet);
    let mut lines = Vec::new();
    if let Some(narrator) = &fleet.narrator {
        let marker = match narrator.evidence {
            estelle_client::FleetEvidence::Measured | estelle_client::FleetEvidence::Observed => "",
            estelle_client::FleetEvidence::Derived => "Derived: ",
            estelle_client::FleetEvidence::Inferred => "Inferred: ",
            estelle_client::FleetEvidence::Unknown => "Unverified: ",
        };
        lines.push(format!(
            "{marker}{}",
            meaningful_cell_text(&narrator.text, "State unavailable")
        ));
        lines.push(String::new());
    }
    lines.push(format!("Estelle Orchestra · {batch} ×{total_label}"));
    lines.push(format!("Participants · {model_roster}"));
    lines.push(String::new());
    let cell_width = usize::from(width).saturating_sub(COLUMNS - 1) / COLUMNS;

    for agents in fleet.agents.chunks(COLUMNS) {
        let mut cells = agents
            .iter()
            .map(|agent| {
                truncate_cell(
                    &fleet_agent_line(agent, now_epoch_s, fleet.stale_after_s, cell_width),
                    cell_width,
                )
            })
            .collect::<Vec<_>>();
        while cells.len() < COLUMNS {
            cells.push(String::new());
        }
        lines.push(
            cells
                .into_iter()
                .map(|cell| pad_cell(&cell, cell_width))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    lines.push(String::new());

    let completed = fleet.completed.map(|value| value.min(total));
    let bar_width = usize::from(width).saturating_sub(24).clamp(8, 48);
    let filled = completed.filter(|_| total > 0).map_or(0, |completed| {
        (completed as usize * bar_width) / total as usize
    });
    let label = if matches!(fleet.state.as_str(), "complete" | "completed") {
        "Completed"
    } else {
        "Working..."
    };
    let spinner = if label == "Working..." {
        ["◐", "◓", "◑", "◒"][(now_epoch_s * 10.0) as usize % 4]
    } else {
        "✓"
    };
    let completed_label = completed.map_or_else(|| "?".to_string(), |value| value.to_string());
    lines.push(format!(
        "{spinner} {label:<10} [{}{}] {completed_label}/{total_label}",
        "━".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled))
    ));
    lines
}

fn fleet_model_roster(fleet: &estelle_client::FleetSnapshot) -> String {
    let mut models = Vec::new();
    for model in &fleet.models {
        let model = model.trim();
        if !model.is_empty() && !models.contains(&model) {
            models.push(model);
        }
    }
    if models.is_empty() {
        let fallback = fleet.model.trim();
        if fallback.is_empty() {
            "models unknown".to_string()
        } else {
            fallback.to_string()
        }
    } else {
        models.join(" · ")
    }
}

fn fleet_agent_line(
    agent: &estelle_client::FleetAgent,
    now_epoch_s: f64,
    stale_after_s: u64,
    cell_width: usize,
) -> String {
    let stale = now_epoch_s - agent.state_observed_at > stale_after_s as f64;
    let state = match agent.status {
        estelle_client::FleetAgentStatus::Unknown => format!(
            "Unknown · {}",
            agent
                .unknown_reason
                .as_deref()
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or("reason absent")
        ),
        estelle_client::FleetAgentStatus::Created => "Created".to_string(),
        estelle_client::FleetAgentStatus::Starting => "Starting".to_string(),
        estelle_client::FleetAgentStatus::Queued => "Queued...".to_string(),
        estelle_client::FleetAgentStatus::Running => agent
            .progress
            .as_ref()
            .filter(|progress| progress.total > 0)
            .map(|progress| {
                format!(
                    "[{}/{}] {}",
                    progress.completed.min(progress.total),
                    progress.total,
                    meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Working")
                )
            })
            .unwrap_or_else(|| {
                meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Working")
            }),
        estelle_client::FleetAgentStatus::AwaitingApproval => "◆ Awaiting approval".to_string(),
        estelle_client::FleetAgentStatus::Completed => format!(
            "✓ {}",
            meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Completed")
        ),
        estelle_client::FleetAgentStatus::Failed => format!(
            "× {}",
            meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Failed")
        ),
        estelle_client::FleetAgentStatus::TimedOut => format!(
            "◷ {}",
            meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Timed out")
        ),
        estelle_client::FleetAgentStatus::Killed => format!(
            "■ {}",
            meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Killed")
        ),
        estelle_client::FleetAgentStatus::Lost => format!(
            "? {}",
            meaningful_cell_text(agent.current_action.as_deref().unwrap_or(""), "Lost")
        ),
        estelle_client::FleetAgentStatus::Blocked => {
            format!("× {}", agent.current_action.as_deref().unwrap_or("Blocked"))
        }
        estelle_client::FleetAgentStatus::NeedsInput => format!(
            "? {}",
            agent.current_action.as_deref().unwrap_or("Needs input")
        ),
        estelle_client::FleetAgentStatus::Cancelled => format!(
            "− {}",
            agent.current_action.as_deref().unwrap_or("Cancelled")
        ),
    };
    let bar_width = if stale {
        3
    } else {
        cell_width.saturating_sub(13).clamp(3, 10)
    };
    let bar = fleet_agent_bar(agent, bar_width);
    if stale {
        if agent.status == estelle_client::FleetAgentStatus::Unknown {
            format!(
                "{:03} {bar} Unknown STALE {}",
                agent.index,
                agent
                    .unknown_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("reason absent")
            )
        } else {
            format!("{:03} {bar} STALE {state}", agent.index)
        }
    } else {
        format!("{:03} {bar} {state}", agent.index)
    }
}

fn fleet_agent_bar(agent: &estelle_client::FleetAgent, width: usize) -> String {
    let fill = match agent.status {
        estelle_client::FleetAgentStatus::Completed => width,
        estelle_client::FleetAgentStatus::Running => agent
            .progress
            .as_ref()
            .filter(|progress| progress.total > 0)
            .map_or(0, |progress| {
                progress.completed.min(progress.total) as usize * width / progress.total as usize
            }),
        _ => 0,
    };
    let empty = match agent.status {
        estelle_client::FleetAgentStatus::Failed => '!',
        estelle_client::FleetAgentStatus::TimedOut => '/',
        estelle_client::FleetAgentStatus::Killed => 'x',
        estelle_client::FleetAgentStatus::Lost => '?',
        _ => '·',
    };
    format!(
        "[{}{}]",
        "∷".repeat(fill),
        empty.to_string().repeat(width - fill)
    )
}

fn truncate_cell(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    use unicode_width::UnicodeWidthChar;

    let value_width = value
        .chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>();
    if value_width <= width {
        return value.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut output = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let char_width = ch.width().unwrap_or(0);
        if used + char_width > width - 1 {
            break;
        }
        output.push(ch);
        used += char_width;
    }
    output.push('…');
    output
}

fn pad_cell(value: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    let padding = width.saturating_sub(UnicodeWidthStr::width(value));
    format!("{value}{}", " ".repeat(padding))
}

fn meaningful_cell_text(value: &str, fallback: &str) -> String {
    value
        .lines()
        .filter_map(|line| {
            let normalized = normalize_cell_text(line);
            let words = normalized.split_whitespace().count();
            let label_only = normalized.ends_with(':') && words <= 2;
            (!normalized.is_empty() && normalized.chars().count() >= 8 && !label_only)
                .then_some(normalized)
        })
        .next()
        .unwrap_or_else(|| fallback.to_string())
}

fn normalize_cell_text(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches(['#', '>', '-', '*', ' '])
        .replace(['`', '*'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    estelle_client::mask_secret(&normalized)
}

pub(crate) fn todo_view_lines(todo: &estelle_client::TodoSnapshot, expanded: bool) -> Vec<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64());
    todo_view_lines_at(todo, expanded, now)
}

fn todo_view_lines_at(
    todo: &estelle_client::TodoSnapshot,
    expanded: bool,
    now_epoch_s: f64,
) -> Vec<String> {
    const COLLAPSED_ITEMS: usize = 5;
    let visible = if expanded {
        todo.items.len()
    } else {
        todo.items.len().min(COLLAPSED_ITEMS)
    };
    let stale = now_epoch_s - todo.observed_at > todo.stale_after_s as f64;
    let mut lines = vec![if stale {
        "Todo · STALE".to_string()
    } else {
        "Todo".to_string()
    }];
    for item in todo.items.iter().take(visible) {
        let glyph = match item.status {
            estelle_client::TodoStatus::Done => "✓",
            estelle_client::TodoStatus::InProgress => "●",
            estelle_client::TodoStatus::Pending => "○",
            estelle_client::TodoStatus::Unknown => "?",
        };
        let title = nonempty_owned(normalize_cell_text(&item.title), "Untitled task");
        let evidence = match item.evidence {
            estelle_client::FleetEvidence::Measured | estelle_client::FleetEvidence::Observed => "",
            estelle_client::FleetEvidence::Derived => "Derived: ",
            estelle_client::FleetEvidence::Inferred => "Inferred: ",
            estelle_client::FleetEvidence::Unknown => "Unverified: ",
        };
        let title = format!("{evidence}{title}");
        let result = item
            .result
            .as_deref()
            .map(normalize_cell_text)
            .filter(|value| !value.is_empty());
        lines.push(result.map_or_else(
            || format!("{glyph} {title}"),
            |result| format!("{glyph} {title} — {result}"),
        ));
    }
    if expanded {
        lines.push("ctrl+t to collapse".to_string());
    } else if todo.items.len() > visible {
        let hidden = &todo.items[visible..];
        let done = hidden
            .iter()
            .filter(|item| item.status == estelle_client::TodoStatus::Done)
            .count();
        lines.push(format!(
            "… +{} more ({} done) · ctrl+t to expand",
            hidden.len(),
            done
        ));
    }
    lines
}

fn nonempty_owned(value: String, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn nonempty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn json_scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        value => serde_json::to_string(value).unwrap_or_else(|_| "unrenderable".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_inventory_is_exactly_the_24_accepted_commands() {
        assert_eq!(
            session_command_names(),
            [
                "help",
                "init",
                "graph",
                "memory",
                "sweep",
                "sessions",
                "resume",
                "work",
                "orchestra",
                "context",
                "gate",
                "scan",
                "improve",
                "verify",
                "apply",
                "undo",
                "mode",
                "routing",
                "status",
                "skills",
                "tools",
                "shell",
                "clear",
                "exit",
            ]
        );
    }

    #[test]
    fn top_level_inventory_keeps_the_hook_commands_visible() {
        assert_eq!(
            top_level_command_names(),
            [
                "login",
                "init",
                "sweep",
                "reindex",
                "connect",
                "remove",
                "github",
                "monitor",
                "research",
                "memory",
                "ask",
                "recall",
                "verify",
                "gate",
                "hook",
                "install-hooks",
                "uninstall-hooks",
                "acp",
                "mcp",
                "mcp-server",
            ]
        );
    }

    #[test]
    fn aliases_and_one_edit_typos_resolve_without_guessing_unrelated_words() {
        assert_eq!(resolve_session_name("route"), Some("routing"));
        assert_eq!(resolve_session_name("quit"), Some("exit"));
        assert_eq!(resolve_session_name("sesions"), Some("sessions"));
        assert_eq!(resolve_session_name("odel"), Some("model"));
        assert_eq!(resolve_session_name("blorp"), None);
    }

    #[test]
    fn p5_keeps_the_complete_codex_slash_surface_reachable() {
        for (command, _) in GRAFT_HELP {
            assert_eq!(
                resolve_session_name(command),
                Some(*command),
                "Codex command /{command} disappeared from the Estelle dispatcher"
            );
        }
        assert_eq!(resolve_session_name("quit"), Some("exit"));
    }

    #[test]
    fn every_inherited_command_has_an_explicit_owner() {
        for (command, _) in GRAFT_HELP {
            let local = matches!(
                *command,
                "prod" | "todo" | "settings" | "plan" | "permissions" | "hooks" | "model"
            );
            let remote = remote_request(command, "task", Some("diff"), Some("question"))
                .expect("route decision")
                .is_some();
            assert!(
                local || remote || inherited_command_lines(command).is_some(),
                "/{command} has no ported, repointed, or deleted owner"
            );
        }
    }

    #[test]
    fn skill_namespace_preserves_the_skill_name_and_task() {
        assert_eq!(
            parse_input("/skill:system-design map the auth boundary"),
            ParsedInput::Command {
                name: Some("skill:"),
                typed_name: "skill:system-design".to_string(),
                argument: "system-design map the auth boundary".to_string(),
            }
        );
    }

    #[test]
    fn p5_remote_grafts_use_the_existing_server_owners() {
        let model = remote_request("model", "", None, None)
            .expect("model route")
            .expect("model request");
        assert_eq!(model.endpoint, estelle_client::Endpoint::Providers);
        assert_eq!(model.method, RemoteMethod::Get);

        let grep = remote_request("grep", "resolve_grounding_scope", None, None)
            .expect("grep route")
            .expect("grep request");
        assert_eq!(grep.endpoint, estelle_client::Endpoint::Search);
        assert_eq!(
            grep.body,
            Some(json!({"query": "resolve_grounding_scope", "code": true}))
        );

        let skill = remote_request("skill:", "system-design map the auth boundary", None, None)
            .expect("skill route")
            .expect("skill request");
        assert_eq!(skill.endpoint, estelle_client::Endpoint::SkillRun);
        assert_eq!(
            skill.body,
            Some(json!({"skill": "system-design", "task": "map the auth boundary"}))
        );
    }

    #[test]
    fn model_picker_renders_provider_and_model_but_never_any_credential_shape() {
        let secret = "sk-live-super-secret-provider-key";
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "configured": ["openai"],
            "configured_keys": [{"id": "openai", "label": "Prod", "provider_key": secret}],
            "providers": [{"id": "openai", "label": "OpenAI", "models": ["gpt-5.5"]}],
            "active": {"provider": "openai", "model": "gpt-5.5"}
        }))
        .expect("provider response");
        let rendered = render_remote_reply("model", &reply).join("\n");
        assert!(rendered.contains("OpenAI"));
        assert!(rendered.contains("gpt-5.5"));
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("sk-live"));
    }

    #[test]
    fn graph_reply_renders_counts_scope_and_roots() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "files": 42, "entities": 517, "subsystems": 6, "cycles": 2,
            "roots": [{"name": "src", "files": 30}, {"name": "tests", "files": 12}],
            "file_index": [{"path": "src/main.rs", "symbols": 88}],
            "building": false,
            "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("typed graph reply");
        let rendered = render_remote_reply("graph", &reply).join("\n");

        assert!(rendered.contains("fatelabs/estelle"), "scope missing\n{rendered}");
        assert!(rendered.contains("42 files"), "files count missing\n{rendered}");
        assert!(rendered.contains("517 entities"), "entities count missing\n{rendered}");
        assert!(rendered.contains("6 subsystems"), "subsystems count missing\n{rendered}");
        assert!(rendered.contains("2 import cycles"), "cycles count missing\n{rendered}");
        assert!(rendered.contains("src"), "roots missing\n{rendered}");
    }

    #[test]
    fn graph_building_and_unswept_are_honest_states_not_zero_counts() {
        let building: estelle_client::CommandReply = serde_json::from_value(json!({
            "files": 0, "entities": 0, "subsystems": 0, "cycles": 0,
            "file_index": [], "roots": [], "building": true, "repo": "fatelabs/estelle"
        }))
        .expect("building reply");
        let rendered = render_remote_reply("graph", &building).join("\n");
        assert!(rendered.contains("being built"), "cold graph not disclosed\n{rendered}");
        assert!(
            !rendered.contains("0 files"),
            "a warming graph rendered as a zero count\n{rendered}"
        );

        let unswept: estelle_client::CommandReply = serde_json::from_value(json!({
            "files": 0, "entities": 0, "subsystems": 0, "cycles": 0,
            "file_index": [], "roots": [], "building": false, "repo": "fatelabs/estelle"
        }))
        .expect("unswept reply");
        let rendered = render_remote_reply("graph", &unswept).join("\n");
        assert!(
            rendered.contains("estelle sweep"),
            "unswept repo did not name the remedy\n{rendered}"
        );
    }

    #[test]
    fn task_commands_refuse_empty_arguments_before_any_request() {
        let work = parse_input("/work");
        assert_eq!(work.local_refusal(), Some("/work needs a task"));
        let orchestra = parse_input("/orchestra   ");
        assert_eq!(orchestra.local_refusal(), Some("/orchestra needs a task"));
        assert_eq!(parse_input("/sessions").local_refusal(), None);
    }

    #[test]
    fn every_remote_session_command_has_one_explicit_route() {
        let routed = [
            ("init", estelle_client::Endpoint::Wiki),
            ("graph", estelle_client::Endpoint::Graph),
            ("memory", estelle_client::Endpoint::DeepSearch),
            ("sessions", estelle_client::Endpoint::Sessions),
            ("resume", estelle_client::Endpoint::Session),
            ("work", estelle_client::Endpoint::Work),
            ("orchestra", estelle_client::Endpoint::Orchestra),
            ("gate", estelle_client::Endpoint::Gate),
            ("scan", estelle_client::Endpoint::Scan),
            ("improve", estelle_client::Endpoint::Improve),
            ("verify", estelle_client::Endpoint::Verify),
            ("routing", estelle_client::Endpoint::Route),
            ("skills", estelle_client::Endpoint::Skills),
            ("tools", estelle_client::Endpoint::Mcp),
        ];
        for (name, endpoint) in routed {
            let request = remote_request(name, "subject", Some("diff body"), Some("last question"))
                .unwrap_or_else(|error| panic!("route failed for {name}: {error:?}"))
                .unwrap_or_else(|| panic!("missing route for {name}"));
            assert_eq!(request.endpoint, endpoint, "wrong route for {name}");
        }
        for local in [
            "help", "sweep", "context", "apply", "undo", "mode", "status", "shell", "clear", "exit",
        ] {
            assert!(
                remote_request(local, "", None, None)
                    .expect("local route classification")
                    .is_none(),
                "{local} must stay local"
            );
        }
    }

    #[test]
    fn gate_and_scan_refuse_without_a_measured_diff() {
        assert_eq!(
            remote_request("gate", "", None, None).unwrap_err(),
            RouteError::MissingDiff
        );
        assert_eq!(
            remote_request("scan", "", None, None).unwrap_err(),
            RouteError::MissingDiff
        );
    }

    #[test]
    fn every_remote_reply_family_renders_nonblank_customer_text() {
        let cases = [
            (
                "init",
                json!({"wiki": "Architecture\n\nAuth lives in auth.rs", "repo": "fatelabs/estelle"}),
            ),
            (
                "memory",
                json!({"answer": "The repo uses a typed auth boundary.", "grounded": true}),
            ),
            (
                "sessions",
                json!({"sessions": [{"id": "s-1", "title": "Auth work", "run_count": 2}], "count": 1}),
            ),
            (
                "resume",
                json!({"id": "s-1", "title": "Auth work", "run_count": 2, "meaning": "Key storage moved."}),
            ),
            (
                "work",
                json!({"answer": "Prepared a patch.", "diff": "diff --git a/a b/a"}),
            ),
            (
                "orchestra",
                json!({"count": 1, "runs": [{"task": "trace auth", "model": "strong", "grounded": true}]}),
            ),
            (
                "gate",
                json!({"merge": false, "verdict": "blocked", "blockers": [{"body": "missing repo"}]}),
            ),
            (
                "scan",
                json!({"count": 1, "findings": [{"path": "auth.rs", "line": 52, "severity": "high", "body": "key leak"}]}),
            ),
            (
                "improve",
                json!({"proposals": [{"title": "Centralize auth", "file": "auth.rs", "line": 52}]}),
            ),
            (
                "verify",
                json!({"grounded": false, "scope_ask": true, "question": "Which repo?", "candidates": ["fatelabs/estelle"]}),
            ),
            ("mode", json!({"global": "propose"})),
            (
                "routing",
                json!({"provider": "anthropic", "routed": "strong", "reason": "grounded code task"}),
            ),
            (
                "skills",
                json!({"skills": [{"name": "review", "summary": "review a change"}]}),
            ),
            (
                "tools",
                json!({"result": {"tools": [{"name": "find_definition", "description": "find a symbol"}]}}),
            ),
        ];
        for (name, raw) in cases {
            let reply: estelle_client::CommandReply =
                serde_json::from_value(raw).expect("typed reply");
            let rendered = render_remote_reply(name, &reply);
            assert!(!rendered.is_empty(), "{name} rendered no rows");
            assert!(
                rendered.iter().any(|line| !line.trim().is_empty()),
                "{name} rendered only whitespace"
            );
        }
    }

    #[test]
    fn verify_renders_the_servers_fail_closed_reason() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "grounded": false,
            "verified": false,
            "reason": "this repo has not been swept, so there is nothing to ground against",
            "unverified_reason": "this repo has not been swept"
        }))
        .expect("typed refusal");

        let rendered = render_remote_reply("verify", &reply).join("\n");

        assert!(rendered.contains("this repo has not been swept"));
        assert!(!rendered.contains("could not establish"));
    }

    #[test]
    fn model_refusal_names_the_account_wide_alternative_and_auto_route() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "configured": ["openai"],
            "active": {"provider": "openai", "model": "gpt-5.5"},
            "providers": [{"id": "openai", "label": "OpenAI", "models": ["gpt-5.5"]}]
        }))
        .expect("typed provider pool");

        let rendered = render_remote_reply("model", &reply).join("\n");

        assert!(rendered.contains("fatelabs.ca/dashboard/provider"));
        assert!(rendered.contains("planning uses the strongest configured model"));
        assert!(rendered.contains("implementation uses the cheapest capable model"));
    }

    #[test]
    fn orchestra_reply_renders_only_the_server_emitted_live_fleet_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-41",
                "batch": "Mutation lane detection",
                "model": "K3",
                "state": "running",
                "revision": 7,
                "observed_at": 4102444800.0,
                "stale_after_s": 60,
                "completed": 1,
                "total": 6,
                "agents": [
                    {"index": 1, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Checking kill switch invariants"},
                    {"index": 2, "status": "queued", "state_observed_at": 4102444800.0},
                    {"index": 3, "status": "done", "state_observed_at": 4102444800.0, "current_action": "Verified account isolation"},
                    {"index": 4, "status": "blocked", "state_observed_at": 4102444800.0, "current_action": "Waiting for grounded scope"},
                    {"index": 5, "status": "queued", "state_observed_at": 4102444800.0},
                    {"index": 6, "status": "needs_input", "state_observed_at": 4102444800.0, "current_action": "Needs repo selection"}
                ]
            }
        }))
        .expect("typed live fleet reply");

        let rendered = render_remote_reply("orchestra", &reply).join("\n");

        assert!(rendered.contains("Estelle Orchestra · Mutation lane detection ×6"));
        assert!(rendered.contains("Participants · K3"));
        assert!(rendered.contains("001"));
        assert!(rendered.contains("Checking kill"));
        assert!(rendered.contains("002"));
        assert!(rendered.contains("Queued"));
        assert!(rendered.contains("Working"));
        assert!(!rendered.contains("elapsed"));
    }

    #[test]
    fn orchestra_header_deduplicates_the_server_reported_participant_models() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-model-roster",
                "batch": "Grounding review",
                "model": "legacy-fallback",
                "models": ["K3", "gpt-5.5", "K3"],
                "state": "running",
                "observed_at": 4102444800.0,
                "total": 2,
                "agents": []
            }
        }))
        .expect("typed fleet model roster");

        let rendered = render_remote_reply("orchestra", &reply).join("\n");

        assert!(rendered.contains("Estelle Orchestra · Grounding review ×2"));
        assert!(rendered.contains("Participants · K3 · gpt-5.5"));
        assert_eq!(rendered.matches("K3").count(), 1, "{rendered}");
        assert!(!rendered.contains("legacy-fallback"), "{rendered}");
    }

    #[test]
    fn orchestra_cells_use_estelle_ink_and_separate_the_instrument_bands() {
        let fleet: estelle_client::FleetSnapshot = serde_json::from_value(json!({
            "id": "fleet-ink",
            "batch": "Ground checkout failures",
            "models": ["Opus", "Gemini"],
            "state": "running",
            "observed_at": 4102444800.0,
            "completed": 1,
            "total": 2,
            "narrator": {"text": "Two agents are checking the selected slices", "evidence": "observed"},
            "agents": [
                {"index": 1, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Bound checkout_timeout", "progress": {"completed": 2, "total": 2}},
                {"index": 2, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Reading the retry gate", "progress": {"completed": 1, "total": 2}}
            ]
        }))
        .expect("typed fleet");

        let lines = fleet_view_lines_at(&fleet, 120, 4102444800.0);
        let rendered = lines.join("\n");

        assert!(rendered.contains("[∷∷∷∷∷∷∷∷∷∷]"), "{rendered}");
        assert!(rendered.contains("[∷∷∷∷∷·····]"), "{rendered}");
        assert!(
            rendered.contains("Participants · Opus · Gemini"),
            "{rendered}"
        );
        assert!(
            lines.windows(2).any(|pair| pair[1].is_empty()),
            "{rendered}"
        );
        assert!(!rendered.contains("::::"), "{rendered}");
        assert!(!rendered.contains("...."), "{rendered}");
    }

    #[test]
    fn orchestra_terminal_fleet_footer_says_completed() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-completed",
                "batch": "Grounding review",
                "models": ["K3"],
                "state": "completed",
                "observed_at": 4102444800.0,
                "completed": 2,
                "total": 2,
                "agents": []
            }
        }))
        .expect("typed completed fleet");

        let rendered = render_remote_reply("orchestra", &reply).join("\n");

        assert!(rendered.contains("✓ Completed"), "{rendered}");
        assert!(!rendered.contains("✓ Complete "), "{rendered}");
    }

    #[test]
    fn orchestra_unknown_and_stale_state_are_visible_instead_of_defaulting_to_running() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-unknown",
                "batch": "Grounding audit",
                "state": "running",
                "revision": 2,
                "observed_at": 1.0,
                "stale_after_s": 60,
                "completed": 0,
                "total": 1,
                "agents": [{
                    "index": 1,
                    "status": "unknown",
                    "unknown_reason": "worker has not reported state",
                    "state_observed_at": 1.0
                }]
            }
        }))
        .expect("typed unknown fleet state");

        let rendered = render_remote_reply("orchestra", &reply).join("\n");

        assert!(rendered.contains("STALE"));
        assert!(rendered.contains("Unknown"));
        assert!(rendered.contains("worker"));
        assert!(rendered.contains("models unknown"));
        assert!(!rendered.contains("model undisclosed"));
        assert!(!rendered.contains("001 running"));
    }

    #[test]
    fn fleet_terminal_outcomes_never_turn_a_stopped_worker_into_a_success() {
        for (status, expected, forbidden) in [
            ("completed", "✓ Completed", "Timed out"),
            ("failed", "× Failed", "✓"),
            ("timed_out", "◷ Timed out", "✓"),
            ("killed", "■ Killed", "✓"),
            ("lost", "? Lost", "✓"),
        ] {
            let reply: estelle_client::CommandReply = serde_json::from_value(json!({
                "fleet": {
                    "id": "fleet-terminal",
                    "batch": "Retry missing assignments",
                    "model": "K3",
                    "state": "running",
                    "observed_at": 4102444800.0,
                    "total": 1,
                    "agents": [{
                        "index": 1,
                        "status": status,
                        "state_observed_at": 4102444800.0
                    }]
                }
            }))
            .expect("every terminal outcome must be a typed wire value");

            let rendered = render_remote_reply("orchestra", &reply).join("\n");
            assert!(rendered.contains(expected), "{status}: {rendered}");
            assert!(!rendered.contains(forbidden), "{status}: {rendered}");
        }
    }

    #[test]
    fn fleet_cell_uses_the_first_meaningful_plain_text_line() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-text",
                "batch": "Mutation lane",
                "model": "K3",
                "state": "running",
                "observed_at": 4102444800.0,
                "total": 1,
                "agents": [{
                    "index": 1,
                    "status": "running",
                    "state_observed_at": 4102444800.0,
                    "current_action": "**Report:**\n\nChecked `auth.rs` and found the missing guard."
                }]
            }
        }))
        .expect("typed fleet");

        let rendered =
            fleet_view_lines_at(reply.fleet.as_ref().unwrap(), 200, 4102444800.0).join("\n");
        assert!(rendered.contains("Checked auth.rs"), "{rendered}");
        assert!(!rendered.contains("**"), "{rendered}");
        assert!(!rendered.contains('`'), "{rendered}");
    }

    #[test]
    fn fleet_cell_hides_embedded_credentials_before_rendering() {
        let hidden = meaningful_cell_text(
            "Inspect estelle_live_1234567890abcdefghijklmnop before retrying",
            "Working",
        );
        assert_eq!(hidden, "[credential hidden]");
    }

    #[test]
    fn fleet_cell_truncation_obeys_display_width_and_unicode_boundaries() {
        let rendered = truncate_cell("001 Running 界界界 and more", 16);
        assert!(
            unicode_width::UnicodeWidthStr::width(rendered.as_str()) <= 16,
            "{rendered:?} exceeded the cell"
        );
        assert!(rendered.is_char_boundary(rendered.len()));
    }

    #[test]
    fn todo_surface_preserves_results_and_collapses_with_an_explicit_count() {
        let todo: estelle_client::TodoSnapshot = serde_json::from_value(json!({
            "observed_at": 4102444800.0,
            "items": [
                {"title": "Step 1", "status": "done", "result": "owner 10/10, cross-tenant 0/10", "evidence": "measured"},
                {"title": "Step 2", "status": "done", "result": "positive control failed before fix", "evidence": "measured"},
                {"title": "Step 3", "status": "in_progress", "evidence": "observed"},
                {"title": "Step 4", "status": "pending", "evidence": "observed"},
                {"title": "Step 5", "status": "pending", "evidence": "observed"},
                {"title": "Step 6", "status": "done", "result": "ledger written", "evidence": "measured"},
                {"title": "Step 7", "status": "pending", "evidence": "observed"}
            ]
        }))
        .expect("typed todo snapshot");

        let collapsed = todo_view_lines(&todo, false).join("\n");
        assert!(collapsed.contains("✓ Step 1 — owner 10/10, cross-tenant 0/10"));
        assert!(collapsed.contains("● Step 3"));
        assert!(collapsed.contains("○ Step 4"));
        assert!(collapsed.contains("… +2 more (1 done) · ctrl+t to expand"));
        assert!(!collapsed.contains("Step 6"));

        let expanded = todo_view_lines(&todo, true).join("\n");
        assert!(expanded.contains("Step 6 — ledger written"));
        assert!(expanded.contains("ctrl+t to collapse"));
    }

    #[test]
    fn todo_surface_marks_inferred_state_instead_of_styling_it_as_measured() {
        let todo: estelle_client::TodoSnapshot = serde_json::from_value(json!({
            "observed_at": 4102444800.0,
            "items": [{
                "title": "Probable root cause",
                "status": "in_progress",
                "evidence": "inferred"
            }]
        }))
        .expect("typed todo snapshot");

        let rendered = todo_view_lines(&todo, false).join("\n");
        assert!(
            rendered.contains("Inferred: Probable root cause"),
            "{rendered}"
        );
    }

    #[test]
    fn todo_surface_labels_an_old_snapshot_stale() {
        let todo: estelle_client::TodoSnapshot = serde_json::from_value(json!({
            "observed_at": 1.0,
            "stale_after_s": 60,
            "items": [{"title": "Old task", "status": "unknown", "evidence": "unknown"}]
        }))
        .expect("typed todo snapshot");

        let rendered = todo_view_lines_at(&todo, false, 100.0).join("\n");
        assert!(rendered.contains("Todo · STALE"), "{rendered}");
    }
}
