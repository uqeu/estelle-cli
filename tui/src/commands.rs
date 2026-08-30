use estelle_client::Endpoint;
use serde_json::Value;
use serde_json::json;

pub(crate) const SESSION_COMMANDS: [&str; 48] = [
    "help",
    "login",
    "logout",
    "whoami",
    "doctor",
    "init",
    "graph",
    "me",
    "keys",
    "team",
    "cards",
    "entities",
    "usage",
    "activity",
    "runs",
    "outcomes",
    "analytics",
    "audit",
    "requests",
    "presence",
    "leaderboard",
    "billing",
    "marketplace",
    "automations",
    "suites",
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
    "presets",
    "hardware",
    "status",
    "skills",
    "tools",
    "shell",
    "clear",
    "exit",
];

const SESSION_HELP: [(&str, &str); 48] = [
    ("help", "what you can do here"),
    (
        "login",
        "connect grounding plus the model plan, API key, or local engine you already have",
    ),
    ("logout", "remove local Estelle and plan credentials"),
    (
        "whoami",
        "which credential kinds are present, never their values",
    ),
    ("doctor", "why a provider login cannot generate an answer"),
    ("init", "a grounded brief of this repo"),
    (
        "graph",
        "the swept code graph; /graph nodes draws the dependency view",
    ),
    (
        "me",
        "your account: plan, balance, budget, provider, invites",
    ),
    (
        "keys",
        "your API keys — prefixes and expiry state, never raw keys",
    ),
    (
        "team",
        "your team: role, seats, members, invites; /team board ranks members",
    ),
    (
        "cards",
        "learned-knowledge cards with folder counts and provenance",
    ),
    (
        "entities",
        "every symbol the swept repo defines, with defining files",
    ),
    ("usage", "requests and tokens by day"),
    (
        "activity",
        "calls and tokens by endpoint, with serving models",
    ),
    ("runs", "the team's agent-run history with grounding flags"),
    (
        "outcomes",
        "how the team's applied changes fared: accept/revert/reject",
    ),
    ("analytics", "your usage analytics derived from run history"),
    (
        "audit",
        "the tamper-evident trail of privileged actions on your account",
    ),
    (
        "requests",
        "the billable engine-call stream with the log total",
    ),
    (
        "presence",
        "who's active, files in flight, pending handoffs",
    ),
    ("leaderboard", "skills ranked by verified grounded outcome"),
    (
        "billing",
        "settings catalog with monthly pricing and current choices",
    ),
    ("marketplace", "the team's published plugins"),
    (
        "automations",
        "stored gated agents — with their live/firing state",
    ),
    ("suites", "your custom suites with draft/active status"),
    (
        "memory",
        "an answered question: what Estelle knows about this repo",
    ),
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
    (
        "presets",
        "show or set the server-owned plan/implement/review routing table",
    ),
    (
        "hardware",
        "estimate which local models fit customer-declared hardware",
    ),
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
    ("memories", "the held-memory listing with trust tiers"),
    ("mcp", "list Estelle's MCP tools"),
    ("grep", "search code with server-side structure"),
    ("permissions", "view the effective autonomy boundary"),
    ("keymap", "composer keymap status"),
    ("approve", "approval ownership status"),
    ("review", "run Estelle's grounded merge gate"),
    ("rename", "session-title ownership status"),
    ("new", "new-session ownership status"),
    ("archive", "archive ownership status"),
    ("delete", "delete-session ownership status"),
    ("fork", "fork-session ownership status"),
    (
        "compact",
        "ask Guardian for a bounded replacement projection",
    ),
    ("goal", "long-running goal ownership status"),
    ("side", "ephemeral side-question ownership status"),
    ("btw", "ephemeral side-question ownership status"),
    ("diff", "show the local working-tree diff"),
    ("feedback", "feedback transport ownership status"),
    ("ps", "background process ownership status"),
    ("stop", "background process ownership status"),
    ("task", "view server orchestra work"),
    // Kimi interaction surfaces not already present above.
    ("version", "show this Estelle build"),
    ("editor", "external-editor ownership status"),
    ("changelog", "release-note ownership status"),
    ("add-dir", "additional-directory ownership status"),
    ("export", "session export ownership status"),
    ("web", "web application ownership status"),
    ("vis", "trace visualizer ownership status"),
    ("upgrade", "upgrade ownership status"),
    ("yolo", "deleted unbounded-approval mode"),
    ("afk", "deleted unattended local-agent mode"),
];

#[cfg(test)]
pub(crate) const TOP_LEVEL_COMMANDS: [&str; 22] = [
    "login",
    "doctor",
    "init",
    "sweep",
    "reindex",
    "serve",
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
pub(crate) fn session_command_names() -> [&'static str; 48] {
    SESSION_COMMANDS
}

#[cfg(test)]
pub(crate) fn top_level_command_names() -> [&'static str; 22] {
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

pub(crate) fn composer_commands() -> Vec<(&'static str, &'static str)> {
    SESSION_HELP
        .iter()
        .chain(GRAFT_HELP.iter())
        .copied()
        .collect()
}

/// Codex-only names REMOVED from the Estelle surface (the founder's DROP list, 2026-08-07).
/// A dropped name never resolves — not even through the one-edit typo matcher, which would
/// otherwise guess a wrong neighbor (`/vim` → `/vis`). Unknown commands send zero requests.
const DROPPED_COMMANDS: &[&str] = &[
    "pet",
    "vim",
    "theme",
    "statusline",
    "title",
    "raw",
    "copy",
    "mention",
    "ide",
    "apps",
    "plugins",
    "experimental",
    "app",
    "import",
    "logout",
    "rollout",
    "debug-config",
    "test-approval",
    "debug-m-drop",
    "debug-m-update",
    "setup-default-sandbox",
    "sandbox-add-read-dir",
    // COLLIDES deletions (founder, 2026-08-07): a toggleable trust layer is broken by design;
    // style comes from the repo, not a picker; agent surfaces re-add WITH the Orchestra client
    // surface, not before.
    "hooks",
    "personality",
    "agent",
    "subagents",
];

pub(crate) fn resolve_session_name(raw: &str) -> Option<&'static str> {
    let name = raw.trim().to_ascii_lowercase();
    match name.as_str() {
        "route" => return Some("routing"),
        "quit" => return Some("exit"),
        _ => {}
    }
    if DROPPED_COMMANDS.contains(&name.as_str()) {
        return None;
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
        "keymap" | "editor" => Some(repointed(
            "the maintained terminal layer",
            "This compact Estelle session has no persisted setting for it yet.",
        )),
        "approve" => Some(deleted(
            "OpenAI's local agent approval brain was removed; Estelle's server autonomy gate is authoritative.",
        )),
        "rename" | "new" | "archive" | "delete" | "fork" | "goal" => Some(repointed(
            "Estelle sessions",
            "The current server contract exposes read/resume, not this mutation; the command is visible and inert.",
        )),
        "web" => Some(repointed(
            "fatelabs.ca",
            "Browser launching is not performed from the TUI without an explicit URL contract.",
        )),
        "task" => Some(repointed(
            "Estelle /orchestra",
            "Use /orchestra <task> to run one server task. The fixed fleet view opens only when the server emits revisioned live state; production does not emit it yet.",
        )),
        "side" | "btw" => Some(repointed(
            "the current Estelle session",
            "Ephemeral forks have no server owner today, so this command does not create a second local agent.",
        )),
        "diff" => Some(repointed(
            "the local Git working tree",
            "Use !git diff --no-color; no diff is sent until /gate, /scan, or /review is requested.",
        )),
        "ps" | "stop" => Some(deleted(
            "This inspected OpenAI's local agent/app-server state, which is not an Estelle runtime.",
        )),
        "feedback" => Some(deleted(
            "The inherited feedback upload transport pointed at the upstream Sentry host and was removed end to end (P0-AMPUTATION.md, 2026-08-07). No replacement endpoint exists.",
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
        "accept-edits" | "accept_edits" | "edit" | "propose" | "pr" => Some("propose"),
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
        "propose" => "accept-edits",
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
        "propose" => "accepts sandboxed edits as a reviewable diff",
        "branch" => "may push a non-main branch and run CI; never merges",
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
    Put,
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
    InvalidPresetArguments,
    InvalidHardwareArguments,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompactionView {
    pub(crate) line: String,
    pub(crate) status: String,
    pub(crate) generation_after: u64,
}

/// Interpret the content-free `/govern` receipt. HTTP 200 is transport success only: blocked and
/// unchanged projections retain their generation and are rendered as such, never as compaction.
pub(crate) fn compaction_view(
    reply: &estelle_client::CommandReply,
    expected_generation: u64,
) -> Result<CompactionView, String> {
    let receipt = reply
        .extra
        .get("compaction")
        .and_then(Value::as_object)
        .ok_or_else(|| "the server omitted the compaction receipt".to_string())?;
    let field = |name: &str| receipt.get(name).and_then(Value::as_u64);
    let status = receipt
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "the compaction receipt omitted status".to_string())?;
    let reason = receipt
        .get("reason")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "the compaction receipt omitted reason".to_string())?;
    let before = field("generation_before")
        .ok_or_else(|| "the compaction receipt omitted generation_before".to_string())?;
    let after = field("generation_after")
        .ok_or_else(|| "the compaction receipt omitted generation_after".to_string())?;
    if before != expected_generation {
        return Err(format!(
            "the server described generation {before}, but the client requested {expected_generation}"
        ));
    }
    let expected_after = if status == "compacted" {
        before.saturating_add(1)
    } else {
        before
    };
    if after != expected_after {
        return Err(format!(
            "{status} returned generation {before}→{after}; expected {before}→{expected_after}"
        ));
    }
    if !matches!(status, "blocked" | "unchanged" | "compacted") {
        return Err(format!("unknown compaction status {status:?}"));
    }
    Ok(CompactionView {
        line: format!("compact {}  {reason}", status.to_ascii_uppercase()),
        status: status.to_string(),
        generation_after: after,
    })
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
    let put = |endpoint, body| {
        Ok(Some(RemoteRequest {
            name,
            endpoint,
            method: RemoteMethod::Put,
            body: Some(body),
            query: json!({}),
        }))
    };
    match name {
        "init" => get(Endpoint::Wiki, json!({})),
        "graph" if argument == "nodes" => get(Endpoint::GraphNodes, json!({})),
        "graph" => get(Endpoint::Graph, json!({})),
        "me" => get(Endpoint::Me, json!({})),
        "keys" => get(Endpoint::MeKeys, json!({})),
        "team" if argument == "board" => get(Endpoint::TeamLeaderboard, json!({})),
        "team" => get(Endpoint::MeTeam, json!({})),
        "cards" => get(Endpoint::MemoryCards, json!({})),
        "entities" => get(Endpoint::Entities, json!({})),
        "usage" => get(Endpoint::Usage, json!({})),
        "activity" => get(Endpoint::Activity, json!({})),
        "runs" => get(Endpoint::Runs, json!({})),
        "outcomes" => get(Endpoint::Outcomes, json!({})),
        "memory" => post(
            Endpoint::DeepSearch,
            json!({"question": "what do you know about this repo?"}),
        ),
        "memories" => get(Endpoint::Memories, json!({})),
        "marketplace" => get(Endpoint::Marketplace, json!({})),
        "automations" => get(Endpoint::Automations, json!({})),
        "suites" => get(Endpoint::Suites, json!({})),
        "analytics" => get(Endpoint::Analytics, json!({})),
        "audit" => get(Endpoint::Audit, json!({})),
        "requests" => get(Endpoint::Requests, json!({})),
        "presence" => get(Endpoint::Presence, json!({})),
        "leaderboard" => get(Endpoint::Leaderboard, json!({})),
        "billing" => get(Endpoint::BillingCatalog, json!({})),
        "model" if argument.is_empty() => get(Endpoint::Providers, json!({})),
        "presets" if argument.is_empty() => get(Endpoint::AgentPresets, json!({})),
        "presets" => put(Endpoint::AgentPresets, preset_update_body(argument)?),
        "hardware" => post(Endpoint::HardwareAdvice, hardware_advice_body(argument)?),
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
        "orchestra" => post(Endpoint::OrchestraRun, json!({"task": argument})),
        "gate" | "scan" | "review" => {
            let diff = diff
                .filter(|value| !value.trim().is_empty())
                .ok_or(RouteError::MissingDiff)?;
            // /review is Estelle Review's DEEP mode — opt-in via body["deep"], computed after
            // the deterministic verdict so a slow/failed review can't disturb it. /gate and
            // /scan stay the deterministic pass.
            let body = if name == "review" {
                json!({"diff": diff, "deep": true})
            } else {
                json!({"diff": diff})
            };
            post(
                if matches!(name, "gate" | "review") {
                    Endpoint::Gate
                } else {
                    Endpoint::Scan
                },
                body,
            )
        }
        "improve" => post(
            Endpoint::Improve,
            if argument.is_empty() {
                json!({})
            } else {
                // The server reads body["path"] for the focus (api_intel.py handle_improve) —
                // the class sweep found the old "focus" key was never read and the argument
                // silently dropped.
                json!({"path": argument})
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

fn preset_update_body(argument: &str) -> Result<Value, RouteError> {
    let mut words = argument.split_whitespace();
    if words.next() != Some("set") {
        return Err(RouteError::InvalidPresetArguments);
    }
    let preset = words
        .next()
        .filter(|value| !value.is_empty())
        .ok_or(RouteError::InvalidPresetArguments)?;
    let mut rows = std::collections::BTreeMap::new();
    for assignment in words {
        let (role, selection) = assignment
            .split_once('=')
            .ok_or(RouteError::InvalidPresetArguments)?;
        if !matches!(role, "plan" | "implement" | "review") || rows.contains_key(role) {
            return Err(RouteError::InvalidPresetArguments);
        }
        let row = if selection == "auto" {
            json!({"provider": "*", "task_kind": role, "mode": "auto"})
        } else {
            let (provider, model) = selection
                .split_once(':')
                .filter(|(provider, model)| !provider.is_empty() && !model.is_empty())
                .ok_or(RouteError::InvalidPresetArguments)?;
            json!({
                "provider": provider,
                "task_kind": role,
                "mode": "pinned",
                "model": model,
            })
        };
        rows.insert(role, row);
    }
    if rows.len() != 3 {
        return Err(RouteError::InvalidPresetArguments);
    }
    let routing_table = ["plan", "implement", "review"]
        .into_iter()
        .map(|role| rows.remove(role))
        .collect::<Option<Vec<_>>>()
        .ok_or(RouteError::InvalidPresetArguments)?;
    Ok(json!({"preset": preset, "routing_table": routing_table}))
}

fn hardware_advice_body(argument: &str) -> Result<Value, RouteError> {
    let mut hardware = serde_json::Map::new();
    let mut body = serde_json::Map::new();
    let mut seen = std::collections::HashSet::new();
    for assignment in argument.split_whitespace() {
        let (name, raw) = assignment
            .split_once('=')
            .filter(|(name, raw)| !name.is_empty() && !raw.is_empty())
            .ok_or(RouteError::InvalidHardwareArguments)?;
        if !seen.insert(name) {
            return Err(RouteError::InvalidHardwareArguments);
        }
        match name {
            "ram" | "vram" | "bandwidth" => {
                let value = raw
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or(RouteError::InvalidHardwareArguments)?;
                if name == "ram" && value <= 0.0 {
                    return Err(RouteError::InvalidHardwareArguments);
                }
                let field = match name {
                    "ram" => "ram_gb",
                    "vram" => "gpu_vram_gb",
                    _ => "gpu_bandwidth_gbps",
                };
                hardware.insert(field.to_string(), json!(value));
            }
            "unified" => {
                let value = match raw {
                    "true" => true,
                    "false" => false,
                    _ => return Err(RouteError::InvalidHardwareArguments),
                };
                hardware.insert("unified_memory".to_string(), json!(value));
            }
            "backend" if matches!(raw, "metal" | "cuda" | "rocm" | "vulkan") => {
                hardware.insert("gpu_backend".to_string(), json!(raw));
            }
            "cpu" if matches!(raw, "arm64" | "x86_64") => {
                hardware.insert("cpu_arch".to_string(), json!(raw));
            }
            "models" => {
                let models = raw
                    .split(',')
                    .filter(|model| !model.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if models.is_empty() || models.len() > 64 || models.join(",") != raw {
                    return Err(RouteError::InvalidHardwareArguments);
                }
                body.insert("models".to_string(), json!(models));
            }
            "context" => {
                let value = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| (1..=1_000_000).contains(value))
                    .ok_or(RouteError::InvalidHardwareArguments)?;
                body.insert("context_limit".to_string(), json!(value));
            }
            _ => return Err(RouteError::InvalidHardwareArguments),
        }
    }
    if !hardware.contains_key("ram_gb") {
        return Err(RouteError::InvalidHardwareArguments);
    }
    body.insert("hardware".to_string(), Value::Object(hardware));
    Ok(Value::Object(body))
}

/// Whole-lockfile CVE attachments for /scan. When the measured diff TOUCHES a lockfile, the
/// transitive-dep risk changed — and a per-added-line diff scan can't pair (name, version)
/// across lines, so the server's whole-lockfile path exists (api_intel.py handle_scan).
/// yarn.lock/pnpm-lock.yaml are excluded here too: the server ignores them silently, and sending
/// them would pretend coverage that does not exist. A lockfile named in the diff but unreadable
/// on disk yields NO entry — never fabricated content.
pub(crate) fn scan_lockfile_attachments(root: &std::path::Path, diff: &str) -> Vec<Value> {
    const LOCKFILES: &[&str] = &[
        "poetry.lock",
        "uv.lock",
        "pipfile.lock",
        "package-lock.json",
        "go.sum",
        "go.mod",
        "cargo.lock",
    ];
    let mut touched = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
            if LOCKFILES.contains(&name.as_str()) {
                touched.push(path.to_string());
            }
        }
    }
    touched.sort();
    touched.dedup();
    let Ok(canonical_root) = root.canonicalize() else {
        return Vec::new();
    };
    touched
        .into_iter()
        .filter_map(|path| {
            let full = root.join(&path);
            let canonical = full.canonicalize().ok()?;
            if !canonical.starts_with(&canonical_root) {
                return None;
            }
            let content = std::fs::read_to_string(&full).ok()?;
            if estelle_client::is_secret_shaped(&content) {
                return None;
            }
            Some(json!({"path": path, "content": content}))
        })
        .collect()
}

pub(crate) fn render_remote_reply(name: &str, reply: &estelle_client::CommandReply) -> Vec<String> {
    match name {
        "presets" => render_agent_presets(reply),
        "hardware" => render_hardware_advice(reply),
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
            if reply.graph_truncated.is_some() {
                return render_graph_nodes_reply(reply, repo);
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
                count(reply.graph_entities.as_ref().and_then(Value::as_u64)),
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
        "me" => {
            let scalar = |key: &str| {
                reply
                    .extra
                    .get(key)
                    .map(json_scalar)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "not returned".to_string())
            };
            let money = |key: &str| {
                reply
                    .extra
                    .get(key)
                    .and_then(Value::as_f64)
                    .map(|value| format!("${value:.2}"))
                    .unwrap_or_else(|| "not returned".to_string())
            };
            let identity = scalar("email");
            let mut lines = vec![format!(
                "{identity}{}",
                reply
                    .extra
                    .get("company")
                    .map(json_scalar)
                    .filter(|company| !company.is_empty())
                    .map(|company| format!("  |  {company}"))
                    .unwrap_or_default()
            )];
            lines.push(format!(
                "plan {}  |  {}{}",
                scalar("plan"),
                match reply.extra.get("plan_active").and_then(Value::as_bool) {
                    Some(true) => "active",
                    Some(false) => "INACTIVE",
                    None => "activity not returned",
                },
                reply
                    .extra
                    .get("seats")
                    .and_then(Value::as_u64)
                    .map(|seats| format!("  |  {seats} seats"))
                    .unwrap_or_default()
            ));
            lines.push(format!(
                "balance {}  |  budget {}  |  spent this period {}",
                money("balance_usd"),
                money("budget_usd"),
                money("period_spend_usd")
            ));
            match reply.extra.get("has_provider_key").and_then(Value::as_bool) {
                Some(true) => lines.push(format!(
                    "provider {}  |  {}",
                    reply.provider.as_deref().unwrap_or("not returned"),
                    scalar("provider_model")
                )),
                Some(false) => {
                    lines.push("no provider key — set one (BYOK) before grounded calls".to_string())
                }
                None => lines.push("provider state not returned".to_string()),
            }
            if let Some(invites) = reply
                .extra
                .get("pending_invites")
                .and_then(Value::as_array)
                .filter(|invites| !invites.is_empty())
            {
                lines.push(format!(
                    "{} pending team {} — joining is explicit (POST /me/team/invite/accept), never a side effect",
                    invites.len(),
                    if invites.len() == 1 { "invite" } else { "invites" }
                ));
            }
            if let Some(entitlements) = reply.extra.get("entitlements").and_then(Value::as_object) {
                let toggle = |key: &str| {
                    entitlements
                        .get(key)
                        .and_then(Value::as_bool)
                        .map(|on| if on { "on" } else { "off" })
                        .unwrap_or("not returned")
                };
                lines.push(format!(
                    "entitlements  |  persist_index {}  |  best_retrieval {}  |  memory packs {}",
                    toggle("persist_index"),
                    toggle("best_retrieval"),
                    entitlements
                        .get("memory_pack_qty")
                        .map(json_scalar)
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "not returned".to_string())
                ));
            }
            lines
        }
        "keys" => {
            if reply.me_keys.is_empty() {
                return vec![
                    "No keys on this account. New keys are created on the dashboard and shown once."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!(
                "{} keys  |  raw keys are never returned — prefixes only",
                reply.me_keys.len()
            )];
            for key in &reply.me_keys {
                let mut row = format!(
                    "{}  |  {}  |  {}",
                    key.label.as_deref().unwrap_or("(unlabelled)"),
                    key.prefix.as_deref().unwrap_or("prefix not returned"),
                    key.id.as_deref().unwrap_or("id not returned")
                );
                if let Some(created) = key.created_at.as_deref() {
                    row.push_str(&format!("  |  created {created}"));
                }
                row.push_str(&match key.expires_at.as_deref() {
                    Some(expires) => format!("  |  expires {expires}"),
                    None => "  |  never expires".to_string(),
                });
                if key.expired == Some(true) {
                    row.push_str("  |  expired");
                }
                if key.revoked == Some(true) {
                    row.push_str("  |  revoked");
                }
                lines.push(row);
            }
            lines
        }
        "team" => {
            if let Some(board) = reply.leaderboard.as_ref().and_then(Value::as_array) {
                // /team board — the honest per-actor board. Zero-activity members are included
                // by the server on purpose; the window and metric head the view.
                if board.is_empty() && reply.me_team.is_none() {
                    return vec![
                        "You're not on a team yet. Teams are created on the dashboard.".to_string(),
                    ];
                }
                let mut lines = vec![format!(
                    "team board  |  {}  |  by {}",
                    reply
                        .extra
                        .get("window")
                        .map(json_scalar)
                        .filter(|window| !window.is_empty())
                        .unwrap_or_else(|| "window not returned".to_string()),
                    reply
                        .extra
                        .get("metric")
                        .map(json_scalar)
                        .filter(|metric| !metric.is_empty())
                        .unwrap_or_else(|| "metric not returned".to_string())
                )];
                for member in board.iter().take(10) {
                    lines.push(format!(
                        "{}  {}  |  {}",
                        member
                            .get("rank")
                            .map(json_scalar)
                            .filter(|rank| !rank.is_empty())
                            .unwrap_or_else(|| "?".to_string()),
                        member
                            .get("display_name")
                            .and_then(Value::as_str)
                            .filter(|name| !name.is_empty())
                            .or_else(|| member.get("email").and_then(Value::as_str))
                            .unwrap_or("member not returned"),
                        member
                            .get("value")
                            .map(json_scalar)
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| "value not returned".to_string())
                    ));
                }
                return lines;
            }
            let Some(team) = &reply.me_team else {
                return vec![
                    "You're not on a team yet. Teams are created on the dashboard.".to_string(),
                ];
            };
            let mut lines = vec![format!(
                "{}  |  you are {}{}",
                team.name.as_deref().unwrap_or("name not returned"),
                team.role.as_deref().unwrap_or("role not returned"),
                if team.you_are_owner == Some(true) {
                    " (owner)"
                } else {
                    ""
                }
            )];
            if let Some(ledger) = &team.seat_ledger {
                let seat = |value: Option<u64>| {
                    value
                        .map(|number| number.to_string())
                        .unwrap_or_else(|| "?".to_string())
                };
                lines.push(format!(
                    "{} of {} seats used  |  {} pending  |  {} available{}",
                    seat(ledger.used),
                    seat(ledger.purchased),
                    seat(ledger.pending),
                    seat(ledger.available),
                    if ledger.full == Some(true) {
                        "  |  full"
                    } else {
                        ""
                    }
                ));
            } else {
                lines.push("seat ledger not returned".to_string());
            }
            if !team.invites.is_empty() {
                lines.push(format!(
                    "{} pending invite{}",
                    team.invites.len(),
                    if team.invites.len() == 1 { "" } else { "s" }
                ));
            }
            if !team.members.is_empty() {
                lines.push(String::new());
                lines.extend(team.members.iter().map(|member| {
                    let email = member.email.as_deref().unwrap_or("email not returned");
                    format!(
                        "{}{}  |  {}{}",
                        member
                            .display_name
                            .as_deref()
                            .map(|name| format!("{name} · "))
                            .unwrap_or_default(),
                        email,
                        member.role.as_deref().unwrap_or("role not returned"),
                        if team.owner.as_deref() == member.email.as_deref() {
                            "  |  owner"
                        } else {
                            ""
                        }
                    )
                }));
            }
            lines
        }
        "cards" => {
            if reply.memory_cards.is_empty() {
                return vec![
                    "No learned knowledge cards yet. Cards are distilled from sessions (dreaming is not wired into the CLI)."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!("{} cards", reply.memory_cards.len())];
            if let Some(folders) = &reply.memory_folders {
                let counts = folders
                    .iter()
                    .filter_map(|(name, count)| {
                        count
                            .as_u64()
                            .filter(|count| *count > 0)
                            .map(|count| format!("{name}: {count}"))
                    })
                    .collect::<Vec<_>>();
                if !counts.is_empty() {
                    lines.push(counts.join("  |  "));
                }
            } else {
                lines.push("folder counts not returned".to_string());
            }
            lines.push(String::new());
            for card in reply.memory_cards.iter().take(10) {
                lines.push(format!(
                    "{}  |  {}{}",
                    card.title.as_deref().unwrap_or("title not returned"),
                    card.category.as_deref().unwrap_or("folder not returned"),
                    if card.edited == Some(true) {
                        "  |  edited"
                    } else {
                        ""
                    }
                ));
                if let Some(body) = card
                    .body
                    .as_deref()
                    .and_then(|body| body.lines().next())
                    .filter(|line| !line.trim().is_empty())
                {
                    lines.push(format!("  {body}"));
                }
                if !card.sources.is_empty() {
                    lines.push(format!("  provenance  {}", card.sources.join(", ")));
                }
            }
            if reply.memory_cards.len() > 10 {
                lines.push(format!("… {} more cards", reply.memory_cards.len() - 10));
            }
            lines
        }
        "entities" => {
            let repo = reply.repo.as_deref().unwrap_or("this repo");
            let rows = reply
                .graph_entities
                .as_ref()
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                return vec![format!(
                    "No entities returned for {repo} — nothing swept here yet. Run estelle sweep first."
                )];
            }
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
                "{} entities",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| rows.len().to_string())
            ));
            lines.push(String::new());
            for row in rows.iter().take(12) {
                let files = row
                    .get("files")
                    .and_then(Value::as_array)
                    .map(|files| files.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .unwrap_or_default();
                lines.push(format!(
                    "{}{}",
                    row.get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or("symbol not returned"),
                    if files.is_empty() {
                        "  |  defining files not returned".to_string()
                    } else {
                        format!("  |  {}", files.join(", "))
                    }
                ));
            }
            if rows.len() > 12 {
                lines.push(format!("… {} more", rows.len() - 12));
            }
            lines
        }
        "usage" => {
            if reply.usage_series.is_empty() {
                return vec!["No usage recorded for this account yet.".to_string()];
            }
            let total_requests = reply
                .usage_series
                .iter()
                .map(|point| point.requests.unwrap_or(0))
                .sum::<u64>();
            let total_tokens = reply
                .usage_series
                .iter()
                .map(|point| point.tokens.unwrap_or(0))
                .sum::<u64>();
            let mut lines = vec![format!(
                "{} days  |  {} requests  |  {} tokens",
                reply.usage_series.len(),
                total_requests,
                total_tokens
            )];
            for point in &reply.usage_series {
                lines.push(format!(
                    "{}  |  {}  |  {}",
                    point.date.as_deref().unwrap_or("date not returned"),
                    point
                        .requests
                        .map(|requests| format!("{requests} requests"))
                        .unwrap_or_else(|| "requests not returned".to_string()),
                    point
                        .tokens
                        .map(|tokens| format!("{tokens} tokens"))
                        .unwrap_or_else(|| "tokens not returned".to_string())
                ));
            }
            lines
        }
        "activity" => {
            if reply.activity_rows.is_empty() {
                return vec!["No activity recorded for this account yet.".to_string()];
            }
            let mut lines = vec![format!("{} endpoints", reply.activity_rows.len())];
            for row in &reply.activity_rows {
                lines.push(format!(
                    "{}  |  {}  |  {}",
                    row.endpoint.as_deref().unwrap_or("endpoint not returned"),
                    row.count
                        .map(|count| format!("{count} calls"))
                        .unwrap_or_else(|| "calls not returned".to_string()),
                    row.tokens
                        .map(|tokens| format!("{tokens} tokens"))
                        .unwrap_or_else(|| "tokens not returned".to_string())
                ));
                if let Some(models) = row.models.as_ref().filter(|models| !models.is_empty()) {
                    let split = models
                        .iter()
                        .map(|(model, tokens)| format!("{model} {tokens}"))
                        .collect::<Vec<_>>()
                        .join("  |  ");
                    lines.push(format!("  served by  {split}"));
                }
            }
            lines
        }
        "runs" => {
            let runs = reply.agent_runs();
            if runs.is_empty() {
                return vec!["No agent runs recorded for this team yet.".to_string()];
            }
            let mut lines = vec![format!(
                "{} runs",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| runs.len().to_string())
            )];
            for run in runs.iter().take(12) {
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
        "outcomes" => {
            let number = |key: &str| {
                reply
                    .extra
                    .get(key)
                    .map(json_scalar)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "not returned".to_string())
            };
            if reply.extra.get("total").and_then(Value::as_u64) == Some(0) {
                return vec![
                    "No outcomes recorded yet — accept/revert signal accrues as the team applies Estelle's changes."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!(
                "{} outcomes  |  {} accepted  |  {} reverted  |  {} rejected",
                number("total"),
                number("accepted"),
                number("reverted"),
                number("rejected")
            )];
            lines.push(format!(
                "accept rate {}  |  revert rate {}",
                number("accept_rate"),
                number("revert_rate")
            ));
            lines.push(
                "A high revert rate is the server's cue to be more conservative here — surfaced, not silently applied."
                    .to_string(),
            );
            lines
        }
        "memories" => {
            let repo = reply.repo.as_deref().unwrap_or("this repo");
            if reply.memory_items.is_empty() {
                return vec![format!(
                    "No memories held for {repo} — nothing swept yet. Run estelle sweep first."
                )];
            }
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
                "{} memories in this response  |  cap {}",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| reply.memory_items.len().to_string()),
                reply
                    .extra
                    .get("limit")
                    .map(json_scalar)
                    .filter(|limit| !limit.is_empty())
                    .unwrap_or_else(|| "not returned".to_string())
            ));
            if reply.graph_truncated == Some(true) {
                lines.push(
                    "truncated — more is held than shown; the count is rows in this response, not the total."
                        .to_string(),
                );
            }
            lines.push(String::new());
            for item in reply.memory_items.iter().take(12) {
                lines.push(format!(
                    "{}  |  {}{}{}",
                    item.source.as_deref().unwrap_or("source not returned"),
                    item.trust.as_deref().unwrap_or("trust not returned"),
                    item.chunks
                        .map(|chunks| format!("  |  {chunks} chunks"))
                        .unwrap_or_default(),
                    if item.externally_authored == Some(true) {
                        "  |  externally authored"
                    } else {
                        ""
                    }
                ));
            }
            if reply.memory_items.len() > 12 {
                lines.push(format!("… {} more", reply.memory_items.len() - 12));
            }
            lines
        }
        "analytics" => {
            let number = |key: &str| {
                reply
                    .extra
                    .get(key)
                    .map(json_scalar)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "not returned".to_string())
            };
            let run_count = reply.runs.as_ref().and_then(Value::as_u64);
            if run_count == Some(0) {
                return vec![
                    "No usage analytics for this account yet — they derive from run history as it accrues."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!(
                "{} runs  |  {} sessions  |  {} turns",
                run_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "not returned".to_string()),
                reply
                    .sessions
                    .as_ref()
                    .and_then(Value::as_u64)
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "not returned".to_string()),
                number("turns")
            )];
            let tally = |value: Option<&Value>| {
                value
                    .and_then(Value::as_object)
                    .map(|rows| {
                        rows.iter()
                            .map(|(name, count)| format!("{name}: {}", json_scalar(count)))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            let repo_rows = tally(reply.repos.as_ref());
            if !repo_rows.is_empty() {
                lines.push(format!("repos  {}", repo_rows.join("  |  ")));
            }
            for (key, label) in [("skills", "skills"), ("outcomes", "outcomes")] {
                let rows = tally(reply.extra.get(key));
                if !rows.is_empty() {
                    lines.push(format!("{label}  {}", rows.join("  |  ")));
                }
            }
            lines
        }
        "audit" => {
            if reply.audit_entries.is_empty() {
                return vec!["No audit entries yet — privileged actions record here.".to_string()];
            }
            let mut lines = vec![format!(
                "{} entries  |  chain {}{}",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| reply.audit_entries.len().to_string()),
                reply
                    .extra
                    .get("state")
                    .map(json_scalar)
                    .filter(|state| !state.is_empty())
                    .unwrap_or_else(|| "state not returned".to_string()),
                reply
                    .reason
                    .as_deref()
                    .map(|reason| format!("  |  {reason}"))
                    .unwrap_or_default()
            )];
            for entry in reply.audit_entries.iter().take(12) {
                lines.push(format!(
                    "{}  {}{}",
                    entry.at.as_deref().unwrap_or("at not returned"),
                    entry.action.as_deref().unwrap_or("action not returned"),
                    entry
                        .detail
                        .as_deref()
                        .map(|detail| format!("  |  {detail}"))
                        .unwrap_or_default()
                ));
            }
            if reply.audit_entries.len() > 12 {
                lines.push(format!("… {} more", reply.audit_entries.len() - 12));
            }
            lines
        }
        "requests" => {
            if reply.request_records.is_empty() {
                return vec!["No requests recorded for this account yet.".to_string()];
            }
            let mut lines = vec![format!(
                "{} of {} requests  |  newest first",
                reply.request_records.len(),
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "total not returned".to_string())
            )];
            for record in reply.request_records.iter().take(12) {
                lines.push(format!(
                    "{}  {}  |  {} tokens{}",
                    record.ts.as_deref().unwrap_or("ts not returned"),
                    record
                        .endpoint
                        .as_deref()
                        .unwrap_or("endpoint not returned"),
                    record
                        .tokens
                        .map(|tokens| tokens.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    record
                        .model
                        .as_deref()
                        .map(|model| format!("  |  {model}"))
                        .unwrap_or_default()
                ));
            }
            if reply.request_records.len() > 12 {
                lines.push(format!(
                    "… {} more in this page",
                    reply.request_records.len() - 12
                ));
            }
            lines
        }
        "presence" => {
            let rows = |key: &str| {
                reply
                    .extra
                    .get(key)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            };
            let active = rows("active");
            let overnight = rows("overnight");
            let files = rows("files_in_use");
            let handoffs = rows("handoffs");
            if active.is_empty() && overnight.is_empty() && files.is_empty() && handoffs.is_empty()
            {
                return vec![
                    "No team presence — nobody active, no overnight work, no handoffs pending."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!(
                "{} active  |  {} overnight",
                active.len(),
                overnight.len()
            )];
            for member in &active {
                let member_files = member
                    .get("files")
                    .and_then(Value::as_array)
                    .map(|files| {
                        files
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                lines.push(format!(
                    "{}  |  since {}{}",
                    member
                        .get("member")
                        .and_then(Value::as_str)
                        .unwrap_or("member not returned"),
                    member
                        .get("since")
                        .and_then(Value::as_str)
                        .unwrap_or("since not returned"),
                    if member_files.is_empty() {
                        String::new()
                    } else {
                        format!("  |  {member_files}")
                    }
                ));
            }
            for member in &overnight {
                lines.push(format!(
                    "overnight  {}  |  at {}",
                    member
                        .get("member")
                        .and_then(Value::as_str)
                        .unwrap_or("member not returned"),
                    member
                        .get("at")
                        .and_then(Value::as_str)
                        .unwrap_or("at not returned")
                ));
            }
            if !files.is_empty() {
                lines.push(format!(
                    "files in flight  {}",
                    files
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for handoff in &handoffs {
                lines.push(format!(
                    "handoff  {}: {}  |  {}",
                    handoff
                        .get("member")
                        .and_then(Value::as_str)
                        .unwrap_or("member not returned"),
                    handoff
                        .get("note")
                        .and_then(Value::as_str)
                        .unwrap_or("note not returned"),
                    handoff
                        .get("at")
                        .and_then(Value::as_str)
                        .unwrap_or("at not returned")
                ));
            }
            lines
        }
        "leaderboard" => {
            let rows = reply.skill_leaderboard_rows();
            if rows.is_empty() {
                return vec![
                    "No verified skill outcomes yet — the board fills as skills complete grounded work."
                        .to_string(),
                ];
            }
            let mut lines = vec![format!(
                "{} skills ranked by verified grounded outcome",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| rows.len().to_string())
            )];
            for row in &rows {
                lines.push(format!(
                    "{}  |  {} uses  |  {} verified  |  {}",
                    row.skill.as_str(),
                    row.uses
                        .map(|uses| uses.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    row.successes
                        .map(|successes| successes.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    row.success_rate
                        .map(|rate| rate.to_string())
                        .unwrap_or_else(|| "rate not returned".to_string())
                ));
            }
            if let Some(affinity) = reply.extra.get("affinity").and_then(Value::as_object) {
                let worked = affinity
                    .get("worked")
                    .and_then(Value::as_array)
                    .map(|models| {
                        models
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let pick = affinity
                    .get("would_pick")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !worked.is_empty() || !pick.is_empty() {
                    lines.push(format!(
                        "affinity (advisory — nothing routes on this yet)  worked: {}{}",
                        if worked.is_empty() {
                            "not returned".to_string()
                        } else {
                            worked
                        },
                        if pick.is_empty() {
                            String::new()
                        } else {
                            format!("  |  would pick: {pick}")
                        }
                    ));
                }
            }
            lines
        }
        "billing" => {
            let mut lines = Vec::new();
            match reply.extra.get("settings").and_then(Value::as_object) {
                Some(settings) if !settings.is_empty() => {
                    for (key, value) in settings {
                        lines.push(format!("{key}  |  {}", json_scalar(value)));
                    }
                }
                _ => lines.push("current settings not returned".to_string()),
            }
            match reply.extra.get("pricing").and_then(Value::as_object) {
                Some(pricing) => {
                    lines.push(format!(
                        "adds {}/month",
                        pricing
                            .get("total_monthly_usd")
                            .and_then(Value::as_f64)
                            .map(|total| format!("${total:.2}"))
                            .unwrap_or_else(|| "not returned".to_string())
                    ));
                    for row in pricing
                        .get("breakdown")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let label = row
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or("setting not returned");
                        let value = row
                            .get("value")
                            .and_then(Value::as_str)
                            .unwrap_or("value not returned");
                        if row.get("included").and_then(Value::as_bool) == Some(true) {
                            lines.push(format!("  {label} = {value}  |  included in plan"));
                        } else {
                            lines.push(format!(
                                "  {label} = {value}  |  +{}/month",
                                row.get("monthly_usd")
                                    .and_then(Value::as_f64)
                                    .map(|usd| format!("${usd:.2}"))
                                    .unwrap_or_else(|| "not returned".to_string())
                            ));
                        }
                    }
                }
                None => lines.push("pricing not returned".to_string()),
            }
            if let Some(catalog) = reply.extra.get("catalog").and_then(Value::as_array) {
                lines.push(format!(
                    "{} configurable settings in the catalog",
                    catalog.len()
                ));
            }
            lines
        }
        "marketplace" => {
            if reply.marketplace_plugins.is_empty() {
                return vec!["No published plugins for this team yet.".to_string()];
            }
            let mut lines = vec![format!(
                "{} plugins",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| reply.marketplace_plugins.len().to_string())
            )];
            for plugin in reply.marketplace_plugins.iter().take(10) {
                lines.push(format!(
                    "{}  |  {}{}",
                    plugin.name.as_deref().unwrap_or("name not returned"),
                    plugin.mode.as_deref().unwrap_or("mode not returned"),
                    if plugin.skills.is_empty() {
                        String::new()
                    } else {
                        format!("  |  skills: {}", plugin.skills.join(", "))
                    }
                ));
            }
            lines
        }
        "automations" => {
            if reply.automation_rows.is_empty() {
                return vec!["No automations stored for this team.".to_string()];
            }
            let mut lines = Vec::new();
            // The trigger bus is not live; the server says so in the envelope and it leads here
            // — a stored automation must never read as a firing one.
            if reply.extra.get("active").and_then(Value::as_bool) == Some(false) {
                lines.push(
                    reply
                        .reason
                        .as_deref()
                        .unwrap_or("automations are stored but not firing")
                        .to_string(),
                );
            }
            lines.push(format!(
                "{} automations",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| reply.automation_rows.len().to_string())
            ));
            for row in reply.automation_rows.iter().take(10) {
                lines.push(format!(
                    "{}  |  {}{}{}",
                    row.name.as_deref().unwrap_or("name not returned"),
                    match row.enabled {
                        Some(true) => "enabled",
                        Some(false) => "disabled",
                        None => "state not returned",
                    },
                    row.model
                        .as_deref()
                        .map(|model| format!("  |  {model}"))
                        .unwrap_or_default(),
                    row.repo
                        .as_deref()
                        .map(|repo| format!("  |  {repo}"))
                        .unwrap_or_default()
                ));
            }
            lines
        }
        "suites" => {
            if reply.suite_rows.is_empty() {
                return vec!["No custom suites for this namespace yet.".to_string()];
            }
            let mut lines = vec![format!(
                "{} custom suites",
                reply
                    .count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| reply.suite_rows.len().to_string())
            )];
            for suite in reply.suite_rows.iter().take(10) {
                lines.push(format!(
                    "{}  |  {}{}  |  {} playbooks",
                    suite.name.as_deref().unwrap_or("name not returned"),
                    suite.status.as_deref().unwrap_or("status not returned"),
                    suite
                        .version
                        .map(|version| format!("  |  v{version}"))
                        .unwrap_or_default(),
                    suite.playbooks.len()
                ));
            }
            lines
        }
        "memory" => reply
            .answer
            .as_deref()
            .filter(|answer| !answer.trim().is_empty())
            .map(|answer| answer.lines().map(str::to_string).collect())
            .unwrap_or_else(|| vec!["No memory recall came back for this repo.".to_string()]),
        "sessions" => {
            let sessions = reply.session_summaries();
            if sessions.is_empty() {
                return vec!["No sessions yet. This one is the first.".to_string()];
            }
            let mut lines = vec![format!(
                "{} of {} sessions  |  /resume <id> to pick one up",
                sessions.len(),
                reply.count.unwrap_or(sessions.len() as u64)
            )];
            for session in sessions.iter().take(10) {
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
            if let Some(completion) = reply.completion.as_ref() {
                lines.push(render_work_completion(completion));
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
            let runs = reply.agent_runs();
            let mut lines = vec![format!(
                "{} agents{}",
                reply.count.unwrap_or(runs.len() as u64),
                reply
                    .level
                    .as_deref()
                    .map(|level| format!("  |  at {level}"))
                    .unwrap_or_default()
            )];
            for run in runs.iter().take(12) {
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
            if let Some(deterministic) = reply.extra.get("deterministic") {
                // The deep pass changed the outcome; the pre-deep deterministic verdict is
                // preserved server-side under this key. Say which pass produced the block.
                let pre_deep = deterministic
                    .get("verdict")
                    .map(json_scalar)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "not blocked".to_string());
                lines.push(format!(
                    "deep review changed the outcome — the deterministic pass said: {pre_deep}"
                ));
            }
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
        "monitor" => {
            let value = reply
                .extra
                .get("checks")
                .or_else(|| reply.extra.get("logs"))
                .or_else(|| reply.extra.get("issues"));
            value.map_or_else(
                || vec!["Monitor returned no rows.".to_string()],
                |rows| vec![json_scalar(rows)],
            )
        }
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

fn render_work_completion(completion: &estelle_client::WorkCompletion) -> String {
    let elapsed = if completion.elapsed_s.is_finite() && completion.elapsed_s >= 0.0 {
        estelle_tui::fmt_elapsed_compact(completion.elapsed_s.round() as u64)
    } else {
        "elapsed unavailable".to_string()
    };
    let finished = chrono::DateTime::parse_from_rfc3339(&completion.finished_at)
        .map(|timestamp| {
            timestamp
                .with_timezone(&chrono::Utc)
                .format("%H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|_| "finish time unavailable".to_string());
    let spend = match completion
        .spend_usd
        .filter(|value| value.is_finite() && *value >= 0.0)
    {
        Some(value) if completion.spend_is_lower_bound && completion.spend_is_upper_bound => {
            format!("spend ${value:.6} (upper and lower bounds unresolved)")
        }
        Some(value) if completion.spend_is_lower_bound => format!("spend ≥${value:.6}"),
        Some(value) if completion.spend_is_upper_bound => format!("spend ≤${value:.6}"),
        Some(value) if completion.spend_known => format!("spend ${value:.6}"),
        _ => "spend unknown".to_string(),
    };
    let gate = if completion.gate_refused {
        let noun = if completion.gate_refused_count == 1 {
            "finding"
        } else {
            "findings"
        };
        format!("gate refused {} {noun}", completion.gate_refused_count)
    } else {
        "gate accepted".to_string()
    };

    format!("✳ Worked for {elapsed} · done {finished} · {spend} · {gate}")
}

/// The `/graph nodes` view — the drawable dependency graph (`GET /graph/nodes`). The server's
/// own honesty states lead: `truncated` is named, never silently capped; counts come from the
/// payload itself, never derived from timing or absence.
fn render_graph_nodes_reply(reply: &estelle_client::CommandReply, repo: &str) -> Vec<String> {
    let mut lines = vec![format!(
        "{}{}",
        repo,
        reply
            .scope
            .as_deref()
            .map(|scope| format!("  |  {scope}"))
            .unwrap_or_default()
    )];
    let cycle_edges = reply
        .graph_edges
        .iter()
        .filter(|edge| edge.kind == "cycle")
        .count();
    lines.push(format!(
        "{} nodes  |  {} edges ({} cycle {})  |  {} files total",
        reply.graph_nodes.len(),
        reply.graph_edges.len(),
        cycle_edges,
        if cycle_edges == 1 { "leg" } else { "legs" },
        reply
            .graph_files
            .map(|files| files.to_string())
            .unwrap_or_else(|| "not returned".to_string())
    ));
    if reply.graph_truncated == Some(true) {
        lines.push(
            "truncated — the repo is larger than the node limit; the highest-weight files are shown."
                .to_string(),
        );
    }
    if reply.graph_nodes.is_empty() {
        lines.push("No nodes returned for this repo.".to_string());
        return lines;
    }
    lines.push(String::new());
    for node in reply.graph_nodes.iter().take(12) {
        lines.push(format!(
            "{}{}{}",
            node.path,
            node.subsystem
                .map(|subsystem| format!("  |  subsystem {subsystem}"))
                .unwrap_or_default(),
            node.symbols
                .map(|symbols| format!("  |  {symbols} symbols"))
                .unwrap_or_default()
        ));
    }
    if reply.graph_nodes.len() > 12 {
        lines.push(format!("… {} more nodes", reply.graph_nodes.len() - 12));
    }
    if cycle_edges > 0 {
        lines.push(String::new());
        lines.push("import cycle legs:".to_string());
        lines.extend(
            reply
                .graph_edges
                .iter()
                .filter(|edge| edge.kind == "cycle")
                .take(6)
                .map(|edge| format!("{} -> {}", edge.from_path, edge.to_path)),
        );
    }
    lines
}

fn render_agent_presets(reply: &estelle_client::CommandReply) -> Vec<String> {
    let Some(bundle) = reply.extra.get("bundle").and_then(Value::as_object) else {
        return vec![
            "The server returned no agent-preset bundle; no routing state was inferred."
                .to_string(),
        ];
    };
    let scalar = |key: &str| {
        bundle
            .get(key)
            .map(json_scalar)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "not returned".to_string())
    };
    let mut lines = vec![format!(
        "Agent preset: {}  |  schema {}",
        scalar("name"),
        scalar("schema_version")
    )];
    for role in ["plan", "implement", "review"] {
        let row = bundle
            .get("routing_table")
            .and_then(Value::as_array)
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row.get("task_kind").and_then(Value::as_str) == Some(role))
            });
        let mode = row
            .and_then(|row| row.get("mode"))
            .and_then(Value::as_str)
            .unwrap_or("not returned");
        if mode == "pinned" {
            let provider = row
                .and_then(|row| row.get("provider"))
                .and_then(Value::as_str)
                .unwrap_or("not returned");
            let model = row
                .and_then(|row| row.get("model"))
                .and_then(Value::as_str)
                .unwrap_or("not returned");
            lines.push(format!("{role:<10} PINNED  {provider} / {model}"));
        } else {
            lines.push(format!("{role:<10} {}", mode.to_ascii_uppercase()));
        }
    }
    let tools = bundle
        .get("exposed_tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    lines.push(format!(
        "Tools: {}",
        if tools.is_empty() {
            "not returned"
        } else {
            &tools
        }
    ));
    lines.push(format!("Autonomy ceiling: {}", scalar("autonomy_ceiling")));
    lines.push(format!("Context budget: {}", scalar("context_budget")));
    lines.push(format!("System overlay: {}", scalar("system_overlay")));
    let presets = reply
        .extra
        .get("presets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ");
    if !presets.is_empty() {
        lines.push(format!("Available presets: {presets}"));
    }
    let configured = reply
        .extra
        .get("configured_providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    lines.push(format!(
        "Configured providers: {}",
        if configured.is_empty() {
            "none"
        } else {
            &configured
        }
    ));
    lines
}

fn render_hardware_advice(reply: &estelle_client::CommandReply) -> Vec<String> {
    let source = reply
        .extra
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("not returned");
    let mut lines = vec![format!(
        "Local-model fit  |  {source} hardware  |  ADVISORY ONLY"
    )];
    let advisories = reply.extra.get("advisories").and_then(Value::as_array);
    match advisories {
        Some(rows) if !rows.is_empty() => {
            for row in rows {
                let field = |name: &str| {
                    row.get(name)
                        .map(json_scalar)
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "?".to_string())
                };
                lines.push(format!(
                    "{}  |  {} / {}  |  {} GB of {} GB  |  {} tok/s  |  ctx {}  |  {}",
                    field("model"),
                    field("fit").to_ascii_uppercase(),
                    field("run_mode"),
                    field("memory_required_gb"),
                    field("memory_available_gb"),
                    field("estimated_tps"),
                    field("usable_context"),
                    field("best_quant")
                ));
            }
        }
        _ => lines.push("No known model advisory was returned.".to_string()),
    }
    let unknown = reply
        .extra
        .get("unknown_models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        lines.push(format!(
            "Unknown models (not guessed): {}",
            unknown.join(", ")
        ));
    }
    if let Some(note) = reply
        .extra
        .get("note")
        .and_then(Value::as_str)
        .filter(|note| !note.trim().is_empty())
    {
        lines.push(note.to_string());
    }
    lines
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
    if let Some(plan_floor) = fleet.plan_floor_line() {
        lines.push(plan_floor);
    }
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
    fn blocked_compaction_is_not_rendered_as_http_success() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "governed": [{"role": "user", "content": "original"}],
            "compaction": {
                "status": "blocked",
                "reason": "latest_turn_exceeds_usable_window",
                "generation_before": 2,
                "generation_after": 2
            }
        }))
        .expect("govern reply");

        let view = compaction_view(&reply, 2).expect("blocked is a valid receipt");

        assert_eq!(
            view.line,
            "compact BLOCKED  latest_turn_exceeds_usable_window"
        );
        assert_eq!(view.generation_after, 2);
    }

    #[test]
    fn blocked_compaction_cannot_advance_the_generation() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "compaction": {
                "status": "blocked",
                "reason": "latest_turn_exceeds_usable_window",
                "generation_before": 2,
                "generation_after": 3
            }
        }))
        .expect("govern reply");

        assert!(compaction_view(&reply, 2).is_err());
    }

    #[test]
    fn session_inventory_is_exactly_the_48_accepted_commands() {
        assert_eq!(
            session_command_names(),
            [
                "help",
                "login",
                "logout",
                "whoami",
                "doctor",
                "init",
                "graph",
                "me",
                "keys",
                "team",
                "cards",
                "entities",
                "usage",
                "activity",
                "runs",
                "outcomes",
                "analytics",
                "audit",
                "requests",
                "presence",
                "leaderboard",
                "billing",
                "marketplace",
                "automations",
                "suites",
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
                "presets",
                "hardware",
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
                "doctor",
                "init",
                "sweep",
                "reindex",
                "serve",
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
    fn login_and_chatgpt_login_are_local_slash_commands() {
        assert_eq!(
            parse_input("/login"),
            ParsedInput::Command {
                name: Some("login"),
                typed_name: "login".to_string(),
                argument: String::new(),
            }
        );
        assert_eq!(
            parse_input("/login --chatgpt"),
            ParsedInput::Command {
                name: Some("login"),
                typed_name: "login".to_string(),
                argument: "--chatgpt".to_string(),
            }
        );
        assert!(
            remote_request("login", "", None, None)
                .expect("local login classification")
                .is_none(),
            "login must never become a server request"
        );
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
                "prod" | "todo" | "settings" | "plan" | "permissions" | "model" | "compact"
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
    fn presets_read_and_write_the_complete_server_owned_role_table() {
        let read = remote_request("presets", "", None, None)
            .expect("preset read route")
            .expect("preset read request");
        assert_eq!(read.endpoint, estelle_client::Endpoint::AgentPresets);
        assert_eq!(read.method, RemoteMethod::Get);

        let write = remote_request(
            "presets",
            "set coding plan=auto implement=openai:gpt-5.5 review=anthropic:claude-opus",
            None,
            None,
        )
        .expect("preset write route")
        .expect("preset write request");
        assert_eq!(write.endpoint, estelle_client::Endpoint::AgentPresets);
        assert_eq!(write.method, RemoteMethod::Put);
        assert_eq!(
            write.body,
            Some(json!({
                "preset": "coding",
                "routing_table": [
                    {"provider": "*", "task_kind": "plan", "mode": "auto"},
                    {"provider": "openai", "task_kind": "implement", "mode": "pinned", "model": "gpt-5.5"},
                    {"provider": "anthropic", "task_kind": "review", "mode": "pinned", "model": "claude-opus"}
                ]
            }))
        );
    }

    #[test]
    fn presets_refuse_a_partial_role_table_before_sending_any_request() {
        assert_eq!(
            remote_request(
                "presets",
                "set coding plan=auto implement=openai:gpt-5.5",
                None,
                None,
            ),
            Err(RouteError::InvalidPresetArguments)
        );
    }

    #[test]
    fn hardware_command_sends_only_the_customer_declaration() {
        let request = remote_request(
            "hardware",
            "ram=32 vram=12 unified=false backend=cuda bandwidth=504 cpu=x86_64 models=qwen2.5:7b,llama3.3:70b context=16384",
            None,
            None,
        )
        .expect("valid hardware declaration")
        .expect("hardware request");
        assert_eq!(request.endpoint, Endpoint::HardwareAdvice);
        assert_eq!(request.method, RemoteMethod::Post);
        assert_eq!(
            request.body,
            Some(json!({
                "hardware": {
                    "ram_gb": 32.0,
                    "gpu_vram_gb": 12.0,
                    "unified_memory": false,
                    "gpu_backend": "cuda",
                    "gpu_bandwidth_gbps": 504.0,
                    "cpu_arch": "x86_64"
                },
                "models": ["qwen2.5:7b", "llama3.3:70b"],
                "context_limit": 16384
            }))
        );
    }

    #[test]
    fn hardware_command_refuses_missing_ram_guesses_and_non_finite_numbers() {
        for argument in ["", "vram=16", "ram=auto", "ram=NaN", "ram=32 mystery=yes"] {
            assert_eq!(
                remote_request("hardware", argument, None, None),
                Err(RouteError::InvalidHardwareArguments),
                "accepted {argument:?}"
            );
        }
    }

    #[test]
    fn hardware_reply_names_fit_unknowns_and_advisory_limit() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "source": "customer-declared",
            "advisory_only": true,
            "unknown_models": ["invented-90b"],
            "advisories": [{
                "model": "qwen2.5:7b",
                "fit": "comfortable",
                "run_mode": "gpu",
                "memory_required_gb": 6.2,
                "memory_available_gb": 12.0,
                "estimated_tps": 42.1,
                "usable_context": 16384,
                "best_quant": "Q6_K"
            }],
            "note": "Fit estimates never remove a model from Affinity."
        }))
        .expect("hardware reply");
        let rendered = render_remote_reply("hardware", &reply).join("\n");
        assert!(rendered.contains("ADVISORY ONLY"));
        assert!(rendered.contains("qwen2.5:7b"));
        assert!(rendered.contains("COMFORTABLE / gpu"));
        assert!(rendered.contains("Unknown models (not guessed): invented-90b"));
        assert!(rendered.contains("never remove a model"));
    }

    #[test]
    fn presets_render_every_server_field_without_computing_a_pick() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "bundle": {
                "name": "coding",
                "schema_version": 1,
                "routing_table": [
                    {"provider": "*", "task_kind": "plan", "mode": "auto"},
                    {"provider": "openai", "task_kind": "implement", "mode": "pinned", "model": "gpt-5.5"},
                    {"provider": "*", "task_kind": "review", "mode": "auto"}
                ],
                "exposed_tools": ["repo_read", "run_tests"],
                "autonomy_ceiling": "propose",
                "context_budget": 32000,
                "system_overlay": "Plan, implement, and review production code."
            },
            "presets": [{"name": "coding"}, {"name": "research"}, {"name": "review"}],
            "configured_providers": ["openai", "anthropic"]
        }))
        .expect("agent preset response");
        let rendered = render_remote_reply("presets", &reply).join("\n");
        for fact in [
            "coding",
            "schema 1",
            "plan",
            "AUTO",
            "implement",
            "PINNED",
            "openai",
            "gpt-5.5",
            "review",
            "repo_read",
            "run_tests",
            "propose",
            "32000",
            "Plan, implement, and review production code.",
            "research",
            "anthropic",
        ] {
            assert!(rendered.contains(fact), "missing {fact}:\n{rendered}");
        }
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

        assert!(
            rendered.contains("fatelabs/estelle"),
            "scope missing\n{rendered}"
        );
        assert!(
            rendered.contains("42 files"),
            "files count missing\n{rendered}"
        );
        assert!(
            rendered.contains("517 entities"),
            "entities count missing\n{rendered}"
        );
        assert!(
            rendered.contains("6 subsystems"),
            "subsystems count missing\n{rendered}"
        );
        assert!(
            rendered.contains("2 import cycles"),
            "cycles count missing\n{rendered}"
        );
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
        assert!(
            rendered.contains("being built"),
            "cold graph not disclosed\n{rendered}"
        );
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
    fn graph_nodes_argument_routes_to_the_drawable_graph() {
        let request = remote_request("graph", "nodes", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(request.endpoint, estelle_client::Endpoint::GraphNodes);
        assert!(matches!(request.method, RemoteMethod::Get));
        let summary = remote_request("graph", "", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(summary.endpoint, estelle_client::Endpoint::Graph);
    }

    #[test]
    fn graph_nodes_reply_renders_nodes_edges_and_the_explicit_truncated_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "nodes": [
                {"id": "src/main.rs", "path": "src/main.rs", "symbols": 12, "subsystem": 0, "weight": 0.31},
                {"id": "src/lib.rs", "path": "src/lib.rs", "symbols": 8, "subsystem": 0, "weight": 0.22}
            ],
            "edges": [
                {"from": "src/main.rs", "to": "src/lib.rs", "kind": "import"},
                {"from": "src/a.rs", "to": "src/b.rs", "kind": "cycle"}
            ],
            "files": 57, "truncated": true, "building": false,
            "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("typed nodes reply");
        let rendered = render_remote_reply("graph", &reply).join("\n");

        assert!(
            rendered.contains("fatelabs/estelle"),
            "scope missing\n{rendered}"
        );
        assert!(
            rendered.contains("2 nodes"),
            "node count missing\n{rendered}"
        );
        assert!(
            rendered.contains("2 edges"),
            "edge count missing\n{rendered}"
        );
        assert!(
            rendered.contains("57 files"),
            "total files missing\n{rendered}"
        );
        assert!(
            rendered.contains("truncated"),
            "a cut graph did not say so — a silently capped graph lies about the codebase\n{rendered}"
        );
        assert!(
            rendered.contains("src/main.rs"),
            "top node missing\n{rendered}"
        );
        assert!(
            rendered.contains("cycle"),
            "cycle edge kind invisible\n{rendered}"
        );
    }

    #[test]
    fn graph_nodes_building_is_a_warming_notice_not_an_empty_graph() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "nodes": [], "edges": [], "files": 0, "truncated": false, "building": true,
            "repo": "fatelabs/estelle"
        }))
        .expect("building reply");
        let rendered = render_remote_reply("graph", &reply).join("\n");
        assert!(
            rendered.contains("being built"),
            "cold graph not disclosed\n{rendered}"
        );
        assert!(
            !rendered.contains("0 nodes"),
            "a warming graph rendered as zero nodes\n{rendered}"
        );
    }

    #[test]
    fn me_reply_renders_plan_balance_budget_and_pending_invites_honestly() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "email": "dev@example.com", "account_id": "acct-1", "company": "Fate Labs",
            "plan": "pro", "plan_active": true, "seats": 3,
            "balance_usd": 12.5, "budget_usd": 50.0, "period_spend_usd": 4.25,
            "has_provider_key": true, "provider": "anthropic", "provider_model": "claude-opus-4-8",
            "pending_invites": [{"team": "core", "from": "founder@example.com"}],
            "entitlements": {"persist_index": true, "best_retrieval": false, "memory_pack_qty": 2}
        }))
        .expect("typed me reply");
        let rendered = render_remote_reply("me", &reply).join("\n");

        assert!(
            rendered.contains("dev@example.com"),
            "identity missing\n{rendered}"
        );
        assert!(rendered.contains("pro"), "plan missing\n{rendered}");
        assert!(rendered.contains("12.5"), "balance missing\n{rendered}");
        assert!(rendered.contains("50"), "budget missing\n{rendered}");
        assert!(
            rendered.contains("anthropic"),
            "provider missing\n{rendered}"
        );
        assert!(
            rendered.contains("1 pending team invite"),
            "a pending invite was not surfaced — joining must be visible and explicit\n{rendered}"
        );
    }

    #[test]
    fn me_reply_omitted_fields_render_not_returned_never_zero() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"email": "dev@example.com"})).expect("sparse reply");
        let rendered = render_remote_reply("me", &reply).join("\n");
        assert!(
            rendered.contains("not returned"),
            "absent state invented\n{rendered}"
        );
        assert!(
            !rendered.contains("$0"),
            "absent balance rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn keys_reply_lists_keys_with_expiry_state_and_never_a_raw_key() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "keys": [
                {"id": "k1", "prefix": "estelle_live_ab", "label": "laptop",
                 "created_at": "2026-01-01", "expires_at": null, "expired": false, "revoked": false},
                {"id": "k2", "prefix": "estelle_live_cd", "label": "ci",
                 "created_at": "2025-06-01", "expires_at": "2025-07-01", "expired": true, "revoked": false},
                {"id": "k3", "prefix": "estelle_live_ef", "label": "old",
                 "created_at": "2025-01-01", "expires_at": null, "expired": false, "revoked": true}
            ]
        }))
        .expect("typed keys reply");
        let rendered = render_remote_reply("keys", &reply).join("\n");

        assert!(rendered.contains("3 keys"), "count missing\n{rendered}");
        assert!(rendered.contains("laptop"), "label missing\n{rendered}");
        assert!(
            rendered.contains("estelle_live_ab"),
            "prefix missing\n{rendered}"
        );
        assert!(
            rendered.contains("expired"),
            "expired flag missing\n{rendered}"
        );
        assert!(
            rendered.contains("revoked"),
            "revoked flag missing\n{rendered}"
        );
        assert!(
            !rendered.contains("estelle_live_abcdef"),
            "a raw key appeared — the server sends prefixes only"
        );
    }

    #[test]
    fn keys_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"keys": []})).expect("empty keys");
        let rendered = render_remote_reply("keys", &reply).join("\n");
        assert!(
            rendered.contains("No keys"),
            "empty state missing\n{rendered}"
        );
    }

    #[test]
    fn team_reply_renders_role_seat_ledger_members_and_owner_honestly() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "team": {
                "id": "team-1", "name": "Fate Labs", "you": "dev@example.com", "role": "admin",
                "owner": "founder@example.com", "you_are_owner": false, "seats": 4,
                "members": [
                    {"email": "founder@example.com", "role": "admin", "since": "2026-01-01"},
                    {"email": "dev@example.com", "role": "admin", "since": "2026-02-01"},
                    {"email": "intern@example.com", "role": "member", "since": "2026-03-01",
                     "display_name": "Intern"}
                ],
                "seat_ledger": {"purchased": 4, "used": 3, "pending": 1, "available": 0, "full": true},
                "invites": [{"email": "new@example.com"}]
            }
        }))
        .expect("typed team reply");
        let rendered = render_remote_reply("team", &reply).join("\n");

        assert!(
            rendered.contains("Fate Labs"),
            "team name missing\n{rendered}"
        );
        assert!(rendered.contains("admin"), "role missing\n{rendered}");
        assert!(
            rendered.contains("3 of 4 seats used"),
            "seat ledger missing\n{rendered}"
        );
        assert!(
            rendered.contains("full"),
            "a full ledger did not say so\n{rendered}"
        );
        assert!(
            rendered.contains("founder@example.com"),
            "member missing\n{rendered}"
        );
        assert!(rendered.contains("owner"), "owner not marked\n{rendered}");
        assert!(
            rendered.contains("1 pending invite"),
            "admin-visible invites not surfaced\n{rendered}"
        );
    }

    #[test]
    fn team_reply_null_team_is_an_explicit_absent_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"team": null})).expect("null team");
        let rendered = render_remote_reply("team", &reply).join("\n");
        assert!(
            rendered.contains("not on a team"),
            "a null team was not rendered as absent\n{rendered}"
        );
        assert!(
            !rendered.contains("0 members"),
            "absence rendered as an empty roster\n{rendered}"
        );
    }

    #[test]
    fn cards_reply_renders_folder_counts_cards_and_provenance() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "cards": [
                {"id": "decisions:abc", "category": "decisions", "title": "Chose Postgres for memory",
                 "body": "Because JSON files do not scale.", "sources": ["session 2026-08-01"],
                 "created_at": "2026-08-01", "edited": true},
                {"id": "entities:def", "category": "entities", "title": "billing/charge.rs",
                 "body": "Owns the retry loop.", "sources": ["sweep"],
                 "created_at": "2026-08-02", "edited": false}
            ],
            "folders": {"episodic": 0, "projects": 0, "entities": 1, "people": 0, "decisions": 1, "concepts": 0}
        }))
        .expect("typed cards reply");
        let rendered = render_remote_reply("cards", &reply).join("\n");

        assert!(rendered.contains("2 cards"), "count missing\n{rendered}");
        assert!(
            rendered.contains("decisions: 1"),
            "folder count missing\n{rendered}"
        );
        assert!(
            !rendered.contains("episodic: 0"),
            "empty folders rendered as noise\n{rendered}"
        );
        assert!(
            rendered.contains("Chose Postgres for memory"),
            "card title missing\n{rendered}"
        );
        assert!(
            rendered.contains("edited"),
            "edited flag missing\n{rendered}"
        );
        assert!(
            rendered.contains("session 2026-08-01"),
            "provenance missing\n{rendered}"
        );
    }

    #[test]
    fn cards_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "cards": [],
            "folders": {"episodic": 0, "projects": 0, "entities": 0, "people": 0, "decisions": 0, "concepts": 0}
        }))
        .expect("empty cards");
        let rendered = render_remote_reply("cards", &reply).join("\n");
        assert!(
            rendered.contains("No learned knowledge"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 cards"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn entities_reply_renders_symbols_defining_files_and_scope() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "entities": [
                {"symbol": "charge_card", "files": ["billing/charge.rs"]},
                {"symbol": "retry_after", "files": ["billing/retry.rs", "billing/charge.rs"]}
            ],
            "count": 2, "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("typed entities reply");
        let rendered = render_remote_reply("entities", &reply).join("\n");

        assert!(
            rendered.contains("fatelabs/estelle"),
            "scope missing\n{rendered}"
        );
        assert!(rendered.contains("2 entities"), "count missing\n{rendered}");
        assert!(
            rendered.contains("charge_card"),
            "symbol missing\n{rendered}"
        );
        assert!(
            rendered.contains("billing/charge.rs"),
            "defining file missing\n{rendered}"
        );
    }

    #[test]
    fn entities_reply_empty_names_the_remedy() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "entities": [], "count": 0, "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("empty entities");
        let rendered = render_remote_reply("entities", &reply).join("\n");
        assert!(
            rendered.contains("estelle sweep"),
            "remedy missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 entities"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn usage_reply_renders_the_daily_series_with_real_denominators() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "series": [
                {"date": "2026-08-05", "requests": 12, "tokens": 45231},
                {"date": "2026-08-06", "requests": 30, "tokens": 120500}
            ]
        }))
        .expect("typed usage reply");
        let rendered = render_remote_reply("usage", &reply).join("\n");

        assert!(rendered.contains("2026-08-06"), "day missing\n{rendered}");
        assert!(
            rendered.contains("30 requests"),
            "requests missing\n{rendered}"
        );
        assert!(rendered.contains("120500"), "tokens missing\n{rendered}");
        assert!(
            rendered.contains("42 requests"),
            "total wrong or missing\n{rendered}"
        );
    }

    #[test]
    fn usage_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"series": []})).expect("empty usage");
        let rendered = render_remote_reply("usage", &reply).join("\n");
        assert!(
            rendered.contains("No usage"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 requests"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn activity_reply_renders_endpoints_calls_tokens_and_serving_models() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "by_endpoint": [
                {"endpoint": "deep-search", "count": 14, "tokens": 90210,
                 "models": {"claude-opus-4-8": 80000, "kimi-k2.7": 10210}},
                {"endpoint": "sweep/estimate", "count": 3, "tokens": 0}
            ]
        }))
        .expect("typed activity reply");
        let rendered = render_remote_reply("activity", &reply).join("\n");

        assert!(
            rendered.contains("deep-search"),
            "endpoint missing\n{rendered}"
        );
        assert!(
            rendered.contains("14 calls"),
            "call count missing\n{rendered}"
        );
        assert!(rendered.contains("90210"), "tokens missing\n{rendered}");
        assert!(
            rendered.contains("kimi-k2.7"),
            "the model that actually served the tokens is invisible\n{rendered}"
        );
    }

    #[test]
    fn activity_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"by_endpoint": []})).expect("empty activity");
        let rendered = render_remote_reply("activity", &reply).join("\n");
        assert!(
            rendered.contains("No activity"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 calls"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn runs_reply_renders_run_history_with_models_and_grounding_flags() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "runs": [
                {"task": "trace auth", "model": "claude-opus-4-8", "grounded": true},
                {"task": "retry the charge path", "model": "kimi-k2.7", "grounded": false,
                 "reason": "cited a symbol the repo does not have"}
            ],
            "count": 2, "report": "# Runs\n\n…"
        }))
        .expect("typed runs reply");
        let rendered = render_remote_reply("runs", &reply).join("\n");

        assert!(rendered.contains("2 runs"), "count missing\n{rendered}");
        assert!(rendered.contains("trace auth"), "task missing\n{rendered}");
        assert!(
            rendered.contains("claude-opus-4-8"),
            "model missing\n{rendered}"
        );
        assert!(
            rendered.contains("not grounded"),
            "an ungrounded run was not flagged\n{rendered}"
        );
        assert!(
            rendered.contains("cited a symbol the repo does not have"),
            "the ungrounded reason was dropped\n{rendered}"
        );
    }

    #[test]
    fn runs_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"runs": [], "count": 0, "report": ""}))
                .expect("empty runs");
        let rendered = render_remote_reply("runs", &reply).join("\n");
        assert!(
            rendered.contains("No agent runs"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 runs"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn outcomes_reply_renders_the_accept_revert_reject_signal() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "total": 12, "accepted": 8, "reverted": 3, "rejected": 1,
            "accept_rate": 0.667, "revert_rate": 0.25
        }))
        .expect("typed outcomes reply");
        let rendered = render_remote_reply("outcomes", &reply).join("\n");

        assert!(
            rendered.contains("12 outcomes"),
            "total missing\n{rendered}"
        );
        assert!(
            rendered.contains("8 accepted"),
            "accepted missing\n{rendered}"
        );
        assert!(
            rendered.contains("3 reverted"),
            "reverted missing\n{rendered}"
        );
        assert!(
            rendered.contains("1 rejected"),
            "rejected missing\n{rendered}"
        );
        assert!(
            rendered.contains("0.667"),
            "accept rate missing\n{rendered}"
        );
        assert!(rendered.contains("0.25"), "revert rate missing\n{rendered}");
    }

    #[test]
    fn outcomes_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "total": 0, "accepted": 0, "reverted": 0, "rejected": 0,
            "accept_rate": 0.0, "revert_rate": 0.0
        }))
        .expect("empty outcomes");
        let rendered = render_remote_reply("outcomes", &reply).join("\n");
        assert!(
            rendered.contains("No outcomes"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0.0"),
            "no signal rendered as a zero rate\n{rendered}"
        );
    }

    #[test]
    fn memories_reply_renders_trust_tiers_and_the_explicit_truncated_cap() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "memories": [
                {"source": "billing/charge.rs", "kind": "code", "chunks": 12,
                 "trust": "grounded", "may_ground": true, "externally_authored": false},
                {"source": "slack:#eng", "kind": "slack", "chunks": 4,
                 "trust": "acquired", "may_ground": false, "externally_authored": true}
            ],
            "count": 2, "limit": 200, "truncated": true,
            "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("typed memories reply");
        let rendered = render_remote_reply("memories", &reply).join("\n");

        assert!(
            rendered.contains("fatelabs/estelle"),
            "scope missing\n{rendered}"
        );
        assert!(rendered.contains("2 memories"), "count missing\n{rendered}");
        assert!(
            rendered.contains("billing/charge.rs"),
            "source missing\n{rendered}"
        );
        assert!(
            rendered.contains("grounded"),
            "the trust tier is invisible — which held items may certify is the first fact\n{rendered}"
        );
        assert!(
            rendered.contains("acquired"),
            "acquired tier missing\n{rendered}"
        );
        assert!(
            rendered.contains("externally authored"),
            "an attacker-reachable source was not marked\n{rendered}"
        );
        assert!(
            rendered.contains("truncated"),
            "a capped listing did not say so — count is rows in this response, not the total\n{rendered}"
        );
    }

    #[test]
    fn memories_reply_empty_names_the_remedy() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "memories": [], "count": 0, "limit": 200, "truncated": false,
            "repo": "fatelabs/estelle", "scope": "repo:fatelabs/estelle"
        }))
        .expect("empty memories");
        let rendered = render_remote_reply("memories", &reply).join("\n");
        assert!(
            rendered.contains("estelle sweep"),
            "remedy missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 memories"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn analytics_reply_renders_usage_tallies_without_inventing_zeros() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "namespace": "dev@example.com",
            "runs": 12,
            "sessions": 5,
            "turns": 31,
            "repos": {"fatelabs/estelle": 4},
            "skills": {"review": 2},
            "artifacts": 7,
            "outcomes": {"accepted": 8, "reverted": 1},
            "events": {"gate.completed": 3}
        }))
        .expect("typed analytics reply");
        let rendered = render_remote_reply("analytics", &reply).join("\n");

        assert!(
            rendered.contains("12 runs"),
            "run count missing\n{rendered}"
        );
        assert!(
            rendered.contains("5 sessions"),
            "session count missing\n{rendered}"
        );
        assert!(
            rendered.contains("31 turns"),
            "turn count missing\n{rendered}"
        );
        assert!(
            rendered.contains("fatelabs/estelle"),
            "repo tally missing\n{rendered}"
        );
        assert!(
            rendered.contains("accepted"),
            "outcome tally missing\n{rendered}"
        );
    }

    #[test]
    fn analytics_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "namespace": "dev@example.com", "runs": 0, "sessions": 0, "turns": 0,
            "repos": {}, "skills": {}, "artifacts": 0, "outcomes": {}, "events": {}
        }))
        .expect("empty analytics");
        let rendered = render_remote_reply("analytics", &reply).join("\n");
        assert!(
            rendered.contains("No usage analytics"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 sessions"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn audit_reply_renders_entries_and_the_chain_state_with_its_reason() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "entries": [
                {"at": "2026-08-05T10:00Z", "action": "key.issue", "detail": "laptop"},
                {"at": "2026-08-04T09:00Z", "action": "provider.set", "detail": "anthropic"}
            ],
            "count": 2, "verified": true, "state": "verified",
            "reason": "chain intact", "verification": {"checked": 2}
        }))
        .expect("typed audit reply");
        let rendered = render_remote_reply("audit", &reply).join("\n");

        assert!(rendered.contains("2 entries"), "count missing\n{rendered}");
        assert!(rendered.contains("key.issue"), "action missing\n{rendered}");
        assert!(rendered.contains("laptop"), "detail missing\n{rendered}");
        assert!(
            rendered.contains("verified"),
            "the integrity badge is invisible on an integrity surface\n{rendered}"
        );
        assert!(
            rendered.contains("chain intact"),
            "the reason was dropped\n{rendered}"
        );
    }

    #[test]
    fn audit_reply_broken_chain_states_the_reason_not_a_bare_negative() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "entries": [{"at": "2026-08-05T10:00Z", "action": "key.revoke", "detail": "old"}],
            "count": 1, "verified": false, "state": "broken",
            "reason": "segment written under a retired key", "verification": {"checked": 1}
        }))
        .expect("broken audit");
        let rendered = render_remote_reply("audit", &reply).join("\n");
        assert!(
            rendered.contains("broken"),
            "broken state hidden\n{rendered}"
        );
        assert!(
            rendered.contains("segment written under a retired key"),
            "a bare negative shipped without the reason\n{rendered}"
        );
    }

    #[test]
    fn audit_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "entries": [], "count": 0, "verified": true, "state": "empty",
            "reason": "no privileged actions yet", "verification": {"checked": 0}
        }))
        .expect("empty audit");
        let rendered = render_remote_reply("audit", &reply).join("\n");
        assert!(
            rendered.contains("No audit entries"),
            "empty state missing\n{rendered}"
        );
    }

    #[test]
    fn requests_reply_renders_the_stream_with_the_log_total_as_denominator() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "requests": [
                {"ts": "2026-08-06T22:00Z", "endpoint": "deep-search", "tokens": 8100, "model": "claude-opus-4-8"},
                {"ts": "2026-08-06T21:00Z", "endpoint": "sweep/estimate", "tokens": 0}
            ],
            "count": 47
        }))
        .expect("typed requests reply");
        let rendered = render_remote_reply("requests", &reply).join("\n");

        assert!(
            rendered.contains("2 of 47"),
            "the page was implied to be the whole log — count is the total, not the page\n{rendered}"
        );
        assert!(
            rendered.contains("deep-search"),
            "endpoint missing\n{rendered}"
        );
        assert!(rendered.contains("8100"), "tokens missing\n{rendered}");
        assert!(
            rendered.contains("claude-opus-4-8"),
            "serving model missing\n{rendered}"
        );
    }

    #[test]
    fn requests_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"requests": [], "count": 0})).expect("empty requests");
        let rendered = render_remote_reply("requests", &reply).join("\n");
        assert!(
            rendered.contains("No requests"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 of 0"),
            "absence rendered as a zero fraction\n{rendered}"
        );
    }

    #[test]
    fn presence_reply_renders_active_overnight_files_and_handoffs() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "active": [{"member": "dana@example.com", "files": ["billing/charge.rs"],
                        "since": "2026-08-06T20:00Z"}],
            "overnight": [{"member": "kai@example.com", "at": "2026-08-06T02:00Z"}],
            "files_in_use": ["billing/charge.rs", "api/routes.py"],
            "handoffs": [{"member": "dana@example.com", "note": "check the retry ceiling",
                          "at": "2026-08-06T22:00Z"}]
        }))
        .expect("typed presence reply");
        let rendered = render_remote_reply("presence", &reply).join("\n");

        assert!(
            rendered.contains("1 active"),
            "active count missing\n{rendered}"
        );
        assert!(
            rendered.contains("dana@example.com"),
            "member missing\n{rendered}"
        );
        assert!(
            rendered.contains("1 overnight"),
            "overnight count missing\n{rendered}"
        );
        assert!(
            rendered.contains("billing/charge.rs"),
            "file in flight missing — the collision guard is invisible\n{rendered}"
        );
        assert!(
            rendered.contains("check the retry ceiling"),
            "pending handoff dropped\n{rendered}"
        );
    }

    #[test]
    fn presence_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "active": [], "overnight": [], "files_in_use": [], "handoffs": []
        }))
        .expect("empty presence");
        let rendered = render_remote_reply("presence", &reply).join("\n");
        assert!(
            rendered.contains("No team presence"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 active"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn leaderboard_reply_ranks_by_verified_outcome_and_marks_affinity_advisory() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "leaderboard": [
                {"skill": "review", "uses": 9, "successes": 8, "success_rate": 0.889},
                {"skill": "trace", "uses": 4, "successes": 2, "success_rate": 0.5}
            ],
            "count": 2,
            "affinity": {"worked": ["claude-opus-4-8"], "would_pick": "claude-opus-4-8"}
        }))
        .expect("typed leaderboard reply");
        let rendered = render_remote_reply("leaderboard", &reply).join("\n");

        assert!(rendered.contains("review"), "skill missing\n{rendered}");
        assert!(rendered.contains("9 uses"), "uses missing\n{rendered}");
        assert!(
            rendered.contains("8 verified"),
            "successes missing\n{rendered}"
        );
        assert!(rendered.contains("0.889"), "rate missing\n{rendered}");
        assert!(
            rendered.contains("claude-opus-4-8"),
            "affinity invisible\n{rendered}"
        );
        assert!(
            rendered.contains("advisory"),
            "affinity rendered without the 'nothing routes on this yet' caveat\n{rendered}"
        );
    }

    #[test]
    fn leaderboard_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"leaderboard": [], "count": 0})).expect("empty board");
        let rendered = render_remote_reply("leaderboard", &reply).join("\n");
        assert!(
            rendered.contains("No verified skill outcomes"),
            "empty state missing\n{rendered}"
        );
        assert!(
            !rendered.contains("0 uses"),
            "absence rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn billing_reply_renders_current_choices_pricing_and_included_flags() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "settings": {"autonomy_ceiling": "propose", "rerank_quality": "best",
                         "rerank_quality_locked": true},
            "catalog": [{"key": "rerank_quality", "label": "Retrieval quality", "default": "standard",
                         "options": [{"value": "standard", "monthly_usd": 0, "note": ""},
                                     {"value": "best", "monthly_usd": 20, "note": "reranker"}]}],
            "pricing": {"total_monthly_usd": 0.0,
                        "breakdown": [{"setting": "rerank_quality", "label": "Retrieval quality",
                                       "value": "best", "monthly_usd": 0.0, "base_usd": 20.0,
                                       "included": true, "note": "reranker"}]}
        }))
        .expect("typed billing reply");
        let rendered = render_remote_reply("billing", &reply).join("\n");

        assert!(
            rendered.contains("propose"),
            "current setting missing\n{rendered}"
        );
        assert!(
            rendered.contains("best"),
            "rerank choice missing\n{rendered}"
        );
        assert!(rendered.contains("$0.00"), "total missing\n{rendered}");
        assert!(
            rendered.contains("included in plan"),
            "a plan-included option did not say so — it must not read as a new charge\n{rendered}"
        );
    }

    #[test]
    fn billing_reply_absent_sections_render_as_absent() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({})).expect("sparse billing reply");
        let rendered = render_remote_reply("billing", &reply).join("\n");
        assert!(
            rendered.contains("not returned"),
            "absent state invented\n{rendered}"
        );
        assert!(
            !rendered.contains("$0.00"),
            "absent pricing rendered as zero\n{rendered}"
        );
    }

    #[test]
    fn dropped_codex_only_commands_are_unknown_and_kept_names_still_resolve() {
        // The DROP list (founder, 2026-08-07): Codex-only or wrong-branded names must not exist
        // on the Estelle surface at all — an unknown name sends zero requests.
        for dropped in [
            "pet",
            "vim",
            "theme",
            "statusline",
            "title",
            "raw",
            "copy",
            "mention",
            "ide",
            "apps",
            "plugins",
            "experimental",
            "app",
            "import",
            "logout",
            "rollout",
            "debug-config",
            "test-approval",
            "debug-m-drop",
            "debug-m-update",
            "setup-default-sandbox",
            "sandbox-add-read-dir",
            // COLLIDES deletions (founder, 2026-08-07): a toggleable trust layer is broken by
            // design; style comes from the repo, not a picker; agent surfaces re-add WITH the
            // Orchestra client surface, not before.
            "hooks",
            "personality",
            "agent",
            "subagents",
        ] {
            assert!(
                resolve_session_name(dropped).is_none(),
                "/{dropped} should be dropped, not reachable"
            );
            assert!(
                remote_request(dropped, "", None, None)
                    .expect("route classification")
                    .is_none(),
                "/{dropped} must not reach a remote route"
            );
        }
        // The wired reads must never fall back to a graft stub again.
        assert!(
            inherited_command_lines("usage").is_none(),
            "/usage was shadowed by a graft stub; it must route to GET /usage"
        );
        // The KEEP list still resolves.
        for kept in [
            "new",
            "clear",
            "resume",
            "fork",
            "rename",
            "archive",
            "delete",
            "diff",
            "status",
            "keymap",
            "permissions",
            "ps",
            "stop",
            "goal",
            "side",
            "btw",
            "quit",
            "exit",
        ] {
            assert!(
                resolve_session_name(kept).is_some(),
                "/{kept} must stay reachable"
            );
        }
    }

    #[test]
    fn no_graft_stub_shadows_a_wired_remote_route() {
        // The /usage lesson: handle_local_command consults graft dispositions before remote
        // routing, so any leftover stub silently shadows the real wire. Every remote-routed
        // command must have NO graft disposition. Class-wide, not a one-off.
        let routed = [
            ("init", estelle_client::Endpoint::Wiki),
            ("graph", estelle_client::Endpoint::Graph),
            ("me", estelle_client::Endpoint::Me),
            ("keys", estelle_client::Endpoint::MeKeys),
            ("team", estelle_client::Endpoint::MeTeam),
            ("cards", estelle_client::Endpoint::MemoryCards),
            ("entities", estelle_client::Endpoint::Entities),
            ("usage", estelle_client::Endpoint::Usage),
            ("activity", estelle_client::Endpoint::Activity),
            ("runs", estelle_client::Endpoint::Runs),
            ("outcomes", estelle_client::Endpoint::Outcomes),
            ("analytics", estelle_client::Endpoint::Analytics),
            ("audit", estelle_client::Endpoint::Audit),
            ("requests", estelle_client::Endpoint::Requests),
            ("presence", estelle_client::Endpoint::Presence),
            ("leaderboard", estelle_client::Endpoint::Leaderboard),
            ("billing", estelle_client::Endpoint::BillingCatalog),
            ("memory", estelle_client::Endpoint::DeepSearch),
            ("memories", estelle_client::Endpoint::Memories),
        ];
        for (name, endpoint) in routed {
            let request = remote_request(name, "", None, None)
                .expect("route classification")
                .unwrap_or_else(|| panic!("missing route for {name}"));
            assert_eq!(request.endpoint, endpoint, "wrong route for {name}");
            assert!(
                inherited_command_lines(name).is_none(),
                "/{name} is shadowed by a graft stub — the wire never runs"
            );
        }
    }

    #[test]
    fn improve_sends_the_focus_in_the_key_the_server_reads() {
        // The class sweep (S1): the server reads body["path"]; the client sent "focus", so the
        // user's argument was silently dropped and the whole repo scanned.
        let request = remote_request("improve", "src/auth", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(
            request.body,
            Some(json!({"path": "src/auth"})),
            "the server reads body.path — any other key drops the focus"
        );
        let bare = remote_request("improve", "", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(bare.body, Some(json!({})));
    }

    #[test]
    fn review_invokes_the_deep_mode_gate_and_scan_do_not() {
        // Estelle Review's deep mode is opt-in via body["deep"] — the sweep found /review
        // wire-identical to /gate, so the deep pass was unreachable from this client.
        let review = remote_request("review", "", Some("diff body"), None)
            .expect("route")
            .expect("route present");
        assert_eq!(review.endpoint, estelle_client::Endpoint::Gate);
        assert_eq!(
            review.body,
            Some(json!({"diff": "diff body", "deep": true})),
            "/review must opt into the deep pass"
        );
        for shallow in ["gate", "scan"] {
            let request = remote_request(shallow, "", Some("diff body"), None)
                .expect("route")
                .expect("route present");
            assert!(
                request
                    .body
                    .as_ref()
                    .and_then(|body| body.get("deep"))
                    .is_none(),
                "/{shallow} must stay the deterministic pass"
            );
        }
    }

    #[test]
    fn review_reply_discloses_when_the_deep_pass_changed_the_outcome() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "merge": false, "verdict": "blocked-logic",
            "deterministic": {"verdict": "merge"},
            "blockers": [{"body": "deep finding: retry loop drops the error"}]
        }))
        .expect("typed review reply");
        let rendered = render_remote_reply("review", &reply).join("\n");
        assert!(
            rendered.contains("deep review changed the outcome"),
            "a model-authored block must say it is not the deterministic gate's\n{rendered}"
        );
        assert!(
            rendered.contains("merge"),
            "the pre-deep verdict was dropped\n{rendered}"
        );
        assert!(
            rendered.contains("retry loop drops the error"),
            "the deep finding is invisible\n{rendered}"
        );
    }

    #[test]
    fn scan_attachments_follow_lockfiles_the_diff_touches() {
        let root = tempfile::tempdir().expect("repo root");
        std::fs::write(
            root.path().join("Cargo.lock"),
            "[[package]]\nname = \"openssl-sys\"\nversion = \"0.9.0\"\n",
        )
        .expect("lockfile");
        let touching = "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n@@\n+openssl\n";
        let not_touching =
            "diff --git a/src/main.rs b/src/main.rs\n+++ b/src/main.rs\n@@\n+fn main() {}\n";

        let attachments = scan_lockfile_attachments(root.path(), touching);
        assert_eq!(attachments.len(), 1, "a touched lockfile was not attached");
        assert_eq!(attachments[0]["path"].as_str(), Some("Cargo.lock"));
        assert!(
            attachments[0]["content"]
                .as_str()
                .is_some_and(|content| content.contains("openssl-sys")),
            "the attachment does not carry the lockfile content"
        );

        assert!(
            scan_lockfile_attachments(root.path(), not_touching).is_empty(),
            "an untouched lockfile must not upload"
        );
        // Touched in the diff but gone from disk: no entry, no invented content.
        let missing = "+++ b/package-lock.json\n@@\n+lodash\n";
        assert!(
            scan_lockfile_attachments(root.path(), missing).is_empty(),
            "a lockfile absent on disk must not be fabricated"
        );
    }

    #[test]
    fn team_board_argument_routes_to_the_team_leaderboard() {
        let board = remote_request("team", "board", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(board.endpoint, estelle_client::Endpoint::TeamLeaderboard);
        let plain = remote_request("team", "", None, None)
            .expect("route")
            .expect("route present");
        assert_eq!(plain.endpoint, estelle_client::Endpoint::MeTeam);
    }

    #[test]
    fn team_board_reply_renders_member_rows_with_window_and_metric() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "window": "week", "metric": "runs",
            "leaderboard": [
                {"email": "dana@example.com", "display_name": "Dana", "metric_key": "runs",
                 "value": 12, "rank": 1, "usage": {}},
                {"email": "kai@example.com", "display_name": null, "metric_key": "runs",
                 "value": 3, "rank": 2, "usage": {}}
            ]
        }))
        .expect("typed team board reply");
        let rendered = render_remote_reply("team", &reply).join("\n");

        assert!(rendered.contains("week"), "window missing\n{rendered}");
        assert!(rendered.contains("runs"), "metric missing\n{rendered}");
        assert!(rendered.contains("Dana"), "member missing\n{rendered}");
        assert!(rendered.contains("12"), "value missing\n{rendered}");
        assert!(
            rendered.contains("kai@example.com"),
            "email fallback missing\n{rendered}"
        );
    }

    #[test]
    fn team_board_empty_membership_is_explicit_not_zero() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"team": null, "leaderboard": []})).expect("no team");
        let rendered = render_remote_reply("team", &reply).join("\n");
        assert!(
            rendered.contains("not on a team"),
            "absent state missing\n{rendered}"
        );
    }

    #[test]
    fn marketplace_reply_renders_plugins_with_contents() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "plugins": [
                {"name": "fatelabs/core", "mode": "curated", "groups": ["eng"],
                 "skills": ["review", "trace"], "mcp_servers": ["estelle"]}
            ],
            "count": 1
        }))
        .expect("typed marketplace reply");
        let rendered = render_remote_reply("marketplace", &reply).join("\n");

        assert!(
            rendered.contains("fatelabs/core"),
            "plugin name missing\n{rendered}"
        );
        assert!(rendered.contains("curated"), "mode missing\n{rendered}");
        assert!(
            rendered.contains("review"),
            "skill list missing\n{rendered}"
        );
    }

    #[test]
    fn marketplace_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"plugins": [], "count": 0})).expect("empty marketplace");
        let rendered = render_remote_reply("marketplace", &reply).join("\n");
        assert!(
            rendered.contains("No published plugins"),
            "empty state missing\n{rendered}"
        );
    }

    #[test]
    fn automations_reply_shows_the_inactive_trigger_bus_reason_not_a_live_claim() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "automations": [
                {"id": "a1", "name": "nightly triage", "enabled": true,
                 "model": "claude-opus-4-8", "repo": "fatelabs/estelle",
                 "autonomy_ceiling": "propose"}
            ],
            "count": 1, "active": false,
            "reason": "trigger bus not yet live — the automation is stored but nothing fires it yet"
        }))
        .expect("typed automations reply");
        let rendered = render_remote_reply("automations", &reply).join("\n");

        assert!(
            rendered.contains("nightly triage"),
            "name missing\n{rendered}"
        );
        assert!(
            rendered.contains("claude-opus-4-8"),
            "model missing\n{rendered}"
        );
        assert!(
            rendered.contains("nothing fires it yet"),
            "a stored-but-never-fires automation rendered as live — the server's reason must lead\n{rendered}"
        );
    }

    #[test]
    fn automations_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "automations": [], "count": 0, "active": false,
            "reason": "trigger bus not yet live"
        }))
        .expect("empty automations");
        let rendered = render_remote_reply("automations", &reply).join("\n");
        assert!(
            rendered.contains("No automations"),
            "empty state missing\n{rendered}"
        );
    }

    #[test]
    fn suites_reply_renders_custom_suites_with_status_and_version() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "suites": [
                {"id": "s1", "name": "Estelle Billing", "description": "charge-path expertise",
                 "status": "draft", "version": 2, "playbooks": [{"name": "review"}]}
            ],
            "count": 1
        }))
        .expect("typed suites reply");
        let rendered = render_remote_reply("suites", &reply).join("\n");

        assert!(
            rendered.contains("Estelle Billing"),
            "name missing\n{rendered}"
        );
        assert!(
            rendered.contains("draft"),
            "a DRAFT suite did not say so — proposals are never auto-applied\n{rendered}"
        );
        assert!(rendered.contains("v2"), "version missing\n{rendered}");
        assert!(
            rendered.contains("1 playbooks"),
            "playbook count missing\n{rendered}"
        );
    }

    #[test]
    fn suites_reply_empty_is_an_honest_empty_state() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"suites": [], "count": 0})).expect("empty suites");
        let rendered = render_remote_reply("suites", &reply).join("\n");
        assert!(
            rendered.contains("No custom suites"),
            "empty state missing\n{rendered}"
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
            ("me", estelle_client::Endpoint::Me),
            ("keys", estelle_client::Endpoint::MeKeys),
            ("team", estelle_client::Endpoint::MeTeam),
            ("cards", estelle_client::Endpoint::MemoryCards),
            ("entities", estelle_client::Endpoint::Entities),
            ("usage", estelle_client::Endpoint::Usage),
            ("activity", estelle_client::Endpoint::Activity),
            ("runs", estelle_client::Endpoint::Runs),
            ("outcomes", estelle_client::Endpoint::Outcomes),
            ("memories", estelle_client::Endpoint::Memories),
            ("marketplace", estelle_client::Endpoint::Marketplace),
            ("automations", estelle_client::Endpoint::Automations),
            ("suites", estelle_client::Endpoint::Suites),
            ("analytics", estelle_client::Endpoint::Analytics),
            ("audit", estelle_client::Endpoint::Audit),
            ("requests", estelle_client::Endpoint::Requests),
            ("presence", estelle_client::Endpoint::Presence),
            ("leaderboard", estelle_client::Endpoint::Leaderboard),
            ("billing", estelle_client::Endpoint::BillingCatalog),
            ("memory", estelle_client::Endpoint::DeepSearch),
            ("sessions", estelle_client::Endpoint::Sessions),
            ("resume", estelle_client::Endpoint::Session),
            ("work", estelle_client::Endpoint::Work),
            ("orchestra", estelle_client::Endpoint::OrchestraRun),
            ("gate", estelle_client::Endpoint::Gate),
            ("review", estelle_client::Endpoint::Gate),
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
            "help", "login", "logout", "whoami", "doctor", "sweep", "context", "apply", "undo",
            "mode", "status", "shell", "clear", "exit",
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
                "plan_floor_usd": 0.00447,
                "plan_floor_basis": "initial worker prompt before grounded context or retries",
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
        assert!(rendered.contains("Plan floor · $0.004470"));
        assert!(rendered.contains("not expected or final spend"));
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

    #[test]
    fn work_ends_with_the_server_owned_completion_line() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "answer": "Proposal ready.",
            "completion": {
                "elapsed_s": 357.0,
                "finished_at": "2026-08-27T07:37:00+00:00",
                "spend_usd": 0.012345,
                "spend_known": true,
                "spend_is_upper_bound": false,
                "spend_is_lower_bound": false,
                "gate_refused": true,
                "gate_refused_count": 2
            }
        }))
        .expect("work response");

        let rendered = render_remote_reply("work", &reply);

        assert_eq!(
            rendered.last().map(String::as_str),
            Some(
                "✳ Worked for 5m 57s · done 07:37 UTC · spend $0.012345 · gate refused 2 findings"
            )
        );
    }

    #[test]
    fn work_completion_keeps_unpriced_usage_unknown() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "completion": {
                "elapsed_s": 1.0,
                "finished_at": "2026-08-27T07:37:00Z",
                "spend_usd": null,
                "spend_known": false,
                "gate_refused": false,
                "gate_refused_count": 0
            }
        }))
        .expect("unpriced work response");

        let rendered = render_remote_reply("work", &reply).join("\n");

        assert!(rendered.contains("spend unknown"), "{rendered}");
        assert!(!rendered.contains("$0.000000"), "{rendered}");
        assert!(rendered.contains("gate accepted"), "{rendered}");
    }

    #[test]
    fn legacy_work_response_does_not_invent_a_client_timed_receipt() {
        let reply: estelle_client::CommandReply =
            serde_json::from_value(json!({"answer": "Legacy server answer."}))
                .expect("legacy work response");

        let rendered = render_remote_reply("work", &reply).join("\n");

        assert_eq!(rendered, "Legacy server answer.");
        assert!(!rendered.contains("Worked for"), "{rendered}");
    }

    #[test]
    fn work_completion_does_not_call_a_two_direction_spend_error_exact() {
        let reply: estelle_client::CommandReply = serde_json::from_value(json!({
            "completion": {
                "elapsed_s": 1.0,
                "finished_at": "2026-08-27T07:37:00Z",
                "spend_usd": 0.5,
                "spend_known": false,
                "spend_is_upper_bound": true,
                "spend_is_lower_bound": true,
                "gate_refused": true,
                "gate_refused_count": 1
            }
        }))
        .expect("bounded work response");

        let rendered = render_remote_reply("work", &reply).join("\n");

        assert!(
            rendered.contains("spend $0.500000 (upper and lower bounds unresolved)"),
            "{rendered}"
        );
        assert!(rendered.contains("gate refused 1 finding"), "{rendered}");
    }
}
