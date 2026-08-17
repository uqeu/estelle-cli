#![deny(clippy::print_stderr, clippy::print_stdout)]

mod claude_import;
mod commands;
mod copilot_login;
mod doctor;
mod hook_distil;
mod hook_guard;
mod local_provider;
mod login;
mod provider_catalog;
mod provider_keys;
mod provider_store;
mod session_server;
#[cfg(test)]
mod test_gallery;
mod top_level;

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use clap::Parser;
use clap::Subcommand;
use codex_tui::ComposerAction;
use codex_tui::ComposerInput;
use codex_tui::boot_scene::BootPalette;
use codex_tui::boot_scene::BootPreferences;
use codex_tui::boot_scene::BootScene;
use codex_tui::boot_scene::spider_lily_coverage;
use codex_tui::render_markdown_text;
use codex_tui::session_gap;
use crossterm::event::DisableBracketedPaste;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::EnableMouseCapture;
use crossterm::event::Event;
use crossterm::event::EventStream;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use crossterm::execute;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use estelle_client::AccountResponse;
use estelle_client::Client;
use estelle_client::CommandReply;
use estelle_client::CredentialSource;
use estelle_client::CredentialStore;
use estelle_client::DeepSearchRequest;
use estelle_client::Error;
use estelle_client::MemoryOverview;
use estelle_client::OverviewResponse;
use estelle_client::Repo;
use estelle_client::RepoResolver;
use estelle_client::ReposResponse;
use estelle_client::Source;
use estelle_client::is_secret_shaped;
use estelle_client::mask_secret;
use futures::StreamExt;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Axis;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Chart;
use ratatui::widgets::Clear;
use ratatui::widgets::Dataset;
use ratatui::widgets::Gauge;
use ratatui::widgets::GraphType;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use unicode_width::UnicodeWidthChar;

const FRAME_INTERVAL: Duration = Duration::from_millis(100);
// P6's brand palette is intentionally truecolor; the rest of the TUI remains theme-safe ANSI.
const FATE_BG: Color = Color::from_u32(0xE9_E6_DC);
const FATE_GHOST: Color = Color::from_u32(0xC8_C2_B3);
const FATE_INK: Color = Color::from_u32(0x46_43_3B);
const FATE_RED: Color = Color::from_u32(0xC9_1A_0C);
const FATE_RED_SOFT: Color = Color::from_u32(0xE2_8F_86);
const BAYER_8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Theme {
    #[default]
    Dark,
    CreamInk,
}

impl Theme {
    fn name(self) -> &'static str {
        match self {
            Self::Dark => "Estelle Dark",
            Self::CreamInk => "Estelle Cream Ink",
        }
    }

    fn background(self) -> Color {
        match self {
            // ANSI 0 is a painted colour — most terminal themes render it as a grey sheet.
            // Reset inherits the terminal's own background. Cream Ink is the deliberate
            // painted surface and stays painted.
            Self::Dark => Color::Reset,
            Self::CreamInk => FATE_BG,
        }
    }

    fn primary(self) -> Color {
        match self {
            Self::Dark => FATE_BG,
            Self::CreamInk => Color::Black,
        }
    }

    fn ghost(self) -> Color {
        match self {
            Self::Dark => FATE_GHOST,
            Self::CreamInk => Color::from_u32(0x78_72_67),
        }
    }

    fn alert(self) -> Color {
        match self {
            Self::Dark => FATE_RED_SOFT,
            Self::CreamInk => Color::from_u32(0xB8_3A_31),
        }
    }

    fn boot_palette(self) -> BootPalette {
        match self {
            Self::Dark => BootPalette::Dark,
            Self::CreamInk => BootPalette::Light,
        }
    }
}

#[derive(Clone, Debug, Parser)]
#[command(
    name = "estelle",
    version,
    about = "Estelle's grounded coding interface"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long, value_name = "OWNER/REPO", global = true)]
    repo: Option<String>,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// Store and verify an Estelle API credential; --chatgpt signs in with a ChatGPT plan.
    Login {
        /// Device-code sign-in with a ChatGPT account (headless-safe; no browser needed).
        #[arg(long, conflicts_with = "provider")]
        chatgpt: bool,
        /// Connect a model provider, subscription, API key, or local endpoint.
        #[arg(
            long,
            visible_alias = "api-key",
            value_name = "PROVIDER",
            conflicts_with = "chatgpt"
        )]
        provider: Option<String>,
        /// Override the provider API base URL (required for custom providers).
        #[arg(long, requires = "provider")]
        base_url: Option<String>,
        /// Select the provider model after storing the key.
        #[arg(long, requires = "provider")]
        model: Option<String>,
        /// Give this provider credential a non-secret account label.
        #[arg(long, requires = "provider")]
        label: Option<String>,
    },
    /// Diagnose credential stores and provider-runtime readiness without printing secrets.
    Doctor,
    /// Configure Estelle for the current repository.
    Init {
        #[arg(long)]
        client: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Ingest local repository files.
    Sweep {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Update changed or explicitly named files.
    Reindex {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        paths: Vec<PathBuf>,
    },
    /// Run the long-lived owner of Estelle sessions.
    Serve {
        /// Override the owner-only local session socket.
        #[arg(long, value_name = "PATH")]
        socket: Option<PathBuf>,
    },
    /// Attach this terminal to the session server; a client name keeps editor setup compatibility.
    Connect {
        client: Option<String>,
        /// Override the owner-only local session socket.
        #[arg(long, value_name = "PATH")]
        socket: Option<PathBuf>,
        /// Named server-owned session to create or attach.
        #[arg(long, default_value = "main", value_name = "NAME")]
        session: String,
    },
    /// Remove Estelle from local editor configurations.
    #[command(visible_aliases = ["disconnect", "off"])]
    Remove,
    /// Manage the GitHub App connection.
    Github {
        action: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        values: Vec<String>,
    },
    /// Inspect production health.
    Monitor {
        action: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        values: Vec<String>,
    },
    /// Monitor vendor drift and ground repairs.
    Research {
        action: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        values: Vec<String>,
    },
    /// Inspect and erase stored memory.
    Memory {
        action: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        values: Vec<String>,
    },
    /// Ask a grounded question about this repository.
    Ask { question: Vec<String> },
    /// Search Estelle memory and code.
    Recall { query: Vec<String> },
    /// Check a file for ungrounded API references.
    Verify { file: Option<PathBuf> },
    /// Run the merge gate on a local diff.
    Gate {
        #[arg(long)]
        base: Option<String>,
    },
    /// Estelle's harness hook runtime (P4).
    Hook {
        mode: Option<String>,
        /// Installed event identity, used to make failures attributable.
        #[arg(long)]
        event: Option<String>,
    },
    /// Install Estelle's harness hooks (P4).
    InstallHooks,
    /// Remove Estelle's harness hooks (P4).
    UninstallHooks,
    /// Serve Estelle as an Agent Client Protocol agent over stdio.
    Acp,
    /// Connect to an external MCP server over stdio.
    Mcp {
        #[arg(long)]
        call: Option<String>,
        #[arg(long, default_value = "{}")]
        arguments: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<OsString>,
    },
    /// Serve Estelle's MCP tools to external harnesses over stdio.
    McpServer,
}

struct TerminalSession;

fn enter_terminal_screen(writer: &mut impl io::Write) -> io::Result<()> {
    execute!(
        writer,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )
}

fn leave_terminal_screen(writer: &mut impl io::Write) -> io::Result<()> {
    execute!(
        writer,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let session = Self;
        enter_terminal_screen(&mut io::stdout())?;
        Ok(session)
    }

    fn suspend(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        leave_terminal_screen(&mut io::stdout())
    }

    fn resume(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        enter_terminal_screen(&mut io::stdout())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = leave_terminal_screen(&mut io::stdout());
    }
}

#[derive(Default)]
struct HeaderState {
    plan: Option<String>,
    files: Option<u64>,
    chunks: Option<u64>,
    memories: Option<u64>,
    indexed: Option<bool>,
    connected: bool,
}

enum TranscriptEntry {
    SessionHandoff(Vec<String>),
    User(String),
    Answer {
        text: String,
        grounded: Option<bool>,
        degraded: bool,
        sources: Vec<Source>,
    },
    System(String),
    Command {
        name: String,
        lines: Vec<String>,
    },
    Failure([String; 3]),
}

struct ActiveRequest {
    id: u64,
    label: String,
    started: Instant,
    cancel: CancellationToken,
}

#[derive(Clone, Debug)]
struct PendingCommand {
    name: &'static str,
    argument: String,
    last_question: Option<String>,
    /// The per-skill conversation this run continues (see `skill_threads`). Only set for
    /// "skill:" commands; the server runs an interactive skill over `messages` when present
    /// and restarts single-turn from `task` when not.
    skill_thread: Option<Vec<(String, String)>>,
}

#[derive(Clone, Debug)]
enum QueuedRequest {
    Question {
        question: String,
        session_context: Option<String>,
    },
    Command(PendingCommand),
    Sweep,
    Shell(String),
    Apply {
        diff: String,
        reverse: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingLogin {
    Estelle,
    Claude,
    Chatgpt,
    Provider(&'static str),
    EstelleThenProvider(&'static str),
}

enum InlineLoginOutcome {
    Estelle(login::LoginOutcome),
    Claude,
    Chatgpt,
    Provider(&'static str),
}

struct AuthContext {
    store: CredentialStore,
    source: CredentialSource,
}

enum UiEvent {
    SessionContext(session_gap::SessionContext),
    Credential(Result<(Client, AuthContext), Error>),
    Account(Result<AccountResponse, Error>),
    Overview(Result<OverviewResponse, Error>),
    Repos(Result<ReposResponse, Error>),
    Scope(Result<CommandReply, Error>),
    Settings(Result<CommandReply, Error>),
    SettingSaved {
        suite: String,
        key: String,
        result: Result<CommandReply, Error>,
    },
    AutonomyChanged(Result<CommandReply, Error>),
    ThemeSaved {
        theme: Theme,
        result: Result<CommandReply, Error>,
    },
    ProviderSelected {
        provider: String,
        model: String,
        result: Result<CommandReply, Error>,
    },
    ProdIssues(Result<estelle_client::MonitorIssuesResponse, Error>),
    ProdOverview(Result<estelle_client::MonitorOverviewResponse, Error>),
    ProdAgentHealth(Result<estelle_client::AgentHealthResponse, Error>),
    ProdGithub {
        status: Result<estelle_client::GithubStatusResponse, Error>,
        proposed_prs: Result<estelle_client::ProposedPrsResponse, Error>,
    },
    Answer {
        id: u64,
        result: Result<AnswerReply, Error>,
    },
    CommandAnswer {
        id: u64,
        name: &'static str,
        result: Result<RemoteCommandReply, CommandFailure>,
    },
    LocalAnswer {
        id: u64,
        name: &'static str,
        result: Result<Vec<String>, String>,
    },
    SweepProgress {
        id: u64,
        progress: top_level::SweepProgress,
    },
    SweepAnswer {
        id: u64,
        result: Result<Vec<String>, top_level::SweepFailure>,
    },
    Session(session_server::ServerMessage),
    SessionDisconnected(String),
}

struct AnswerReply {
    text: String,
    grounded: Option<bool>,
    degraded: bool,
    sources: Vec<Source>,
    working_paths: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RemoteCommandReply {
    reply: CommandReply,
    inspected_files: Vec<DiffFileStat>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct DiffFileStat {
    path: String,
    changed_lines: u64,
}

#[derive(Clone, Debug)]
struct GateModal {
    verdict: String,
    reasons: Vec<String>,
    files: Vec<DiffFileStat>,
}

const SETTINGS_SUITES: [&str; 10] = [
    "code", "monitor", "review", "repair", "prod", "guardian", "research", "memory", "agent",
    "global",
];

#[derive(Clone, Debug)]
struct PendingSettingInput {
    suite: String,
    spec: Value,
}

fn title_case_suite(suite: &str) -> String {
    let mut chars = suite.chars();
    chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

fn resolved_setting_value(
    settings: &CommandReply,
    suite: &str,
    key: &str,
    scope: &str,
    spec: &Value,
) -> Value {
    let owner = if scope == "personal" {
        "personal"
    } else {
        "team"
    };
    settings
        .extra
        .get(owner)
        .and_then(|values| values.get(suite))
        .and_then(|values| values.get(key))
        .cloned()
        .or_else(|| spec.get("default").cloned())
        .unwrap_or(Value::Null)
}

fn display_setting_value(value: &Value) -> String {
    match value {
        Value::String(value) if value.is_empty() => "unset".to_string(),
        Value::String(value) => value.clone(),
        Value::Bool(true) => "on".to_string(),
        Value::Bool(false) => "off".to_string(),
        Value::Array(values) if values.is_empty() => "none".to_string(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null => "unknown".to_string(),
        value => value.to_string(),
    }
}

fn setting_input_hint(spec: &Value) -> String {
    match spec.get("type").and_then(Value::as_str) {
        Some("int") => format!(
            "whole number · {}..{}",
            spec.get("minimum")
                .and_then(Value::as_i64)
                .map_or_else(|| "unbounded".to_string(), |value| value.to_string()),
            spec.get("maximum")
                .and_then(Value::as_i64)
                .map_or_else(|| "unbounded".to_string(), |value| value.to_string())
        ),
        Some("list") => "comma-separated values · empty clears".to_string(),
        _ => "text · empty clears".to_string(),
    }
}

fn parse_setting_input(spec: &Value, text: &str) -> Result<Value, String> {
    match spec.get("type").and_then(Value::as_str) {
        Some("int") => {
            let value = text
                .trim()
                .parse::<i64>()
                .map_err(|_| "Enter a whole number.".to_string())?;
            if spec
                .get("minimum")
                .and_then(Value::as_i64)
                .is_some_and(|minimum| value < minimum)
                || spec
                    .get("maximum")
                    .and_then(Value::as_i64)
                    .is_some_and(|maximum| value > maximum)
            {
                return Err(setting_input_hint(spec));
            }
            Ok(Value::Number(value.into()))
        }
        Some("list") => Ok(Value::Array(
            text.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| Value::String(value.to_string()))
                .collect(),
        )),
        Some("str") => Ok(Value::String(text.trim().to_string())),
        _ => Err("This setting uses a picker, not free text.".to_string()),
    }
}

#[derive(Clone, Debug)]
enum PickerAction {
    LoginEstelle,
    LoginClaude,
    LoginChatgpt,
    OpenProviderLogin,
    LoginProvider(&'static str),
    OpenLocalLogin,
    LoginLocal(&'static str),
    OpenMode,
    SelectMode(String),
    ConfirmMode(String),
    OpenTheme,
    SelectTheme(Theme),
    ToggleProductionHome,
    OpenModel,
    SelectProvider {
        provider: String,
        provider_label: String,
        model: String,
    },
    OpenSkills,
    InvokeSkill(String),
    OpenSuite(String),
    OpenSetting {
        suite: String,
        spec: Value,
    },
    SetSetting {
        suite: String,
        key: String,
        scope: String,
        value: Value,
    },
    PromptSetting {
        suite: String,
        spec: Value,
    },
    None,
}

#[derive(Clone, Debug)]
struct PickerRow {
    label: String,
    detail: String,
    action: PickerAction,
}

#[derive(Clone, Debug)]
struct PickerSurface {
    title: String,
    rows: Vec<PickerRow>,
    selected: usize,
}

impl PickerSurface {
    fn login() -> Self {
        Self::login_with_machine(estelle_machine::machine().summary_line())
    }

    fn login_with_machine(machine: String) -> Self {
        Self {
            title: "Connect Estelle".to_string(),
            rows: vec![
                PickerRow {
                    label: "Estelle account".to_string(),
                    detail: "buys grounding: memory, code graph, recall and gate; never pays for model tokens"
                        .to_string(),
                    action: PickerAction::LoginEstelle,
                },
                PickerRow {
                    label: "Claude subscription".to_string(),
                    detail: "imports the credential Claude Code stored on this machine · Pro, Max or Team"
                        .to_string(),
                    action: PickerAction::LoginClaude,
                },
                PickerRow {
                    label: "ChatGPT plan".to_string(),
                    detail: "the engine: your plan generates the answer · device code · headless-safe"
                        .to_string(),
                    action: PickerAction::LoginChatgpt,
                },
                PickerRow {
                    label: "Provider API key".to_string(),
                    detail: "Anthropic · OpenAI · Gemini · OpenRouter · DeepSeek · masked input".to_string(),
                    action: PickerAction::OpenProviderLogin,
                },
                PickerRow {
                    label: "Local model".to_string(),
                    detail: format!(
                        "{machine} · LM Studio · Ollama · any OpenAI-compatible endpoint · no token bill"
                    ),
                    action: PickerAction::OpenLocalLogin,
                },
            ],
            selected: 0,
        }
    }

    fn provider_login() -> Self {
        Self {
            title: "Provider API key".to_string(),
            rows: provider_catalog::on_surface(provider_catalog::Surface::ProviderKey)
                .map(|provider| PickerRow {
                    label: provider.display_name.to_string(),
                    detail: provider.detail.to_string(),
                    action: PickerAction::LoginProvider(provider.id),
                })
                .collect(),
            selected: 0,
        }
    }

    fn local_login() -> Self {
        Self {
            title: "Local model".to_string(),
            rows: provider_catalog::on_surface(provider_catalog::Surface::Local)
                .map(|provider| PickerRow {
                    label: provider.display_name.to_string(),
                    detail: provider.detail.to_string(),
                    action: PickerAction::LoginLocal(provider.id),
                })
                .collect(),
            selected: 0,
        }
    }

    fn settings(app: &App) -> Self {
        let mode = commands::mode_name(commands::effective_mode(
            &app.local_mode,
            app.server_mode.as_deref(),
        ));
        let mut rows = vec![
            PickerRow {
                label: "Mode".to_string(),
                detail: format!("{mode} · account ceiling · server enforced"),
                action: PickerAction::OpenMode,
            },
            PickerRow {
                label: "Theme".to_string(),
                detail: format!("{} · client display", app.theme.name()),
                action: PickerAction::OpenTheme,
            },
            PickerRow {
                label: "Production home".to_string(),
                detail: format!(
                    "{} · client-owned",
                    if app.prod_panel_visible {
                        "shown"
                    } else {
                        "hidden"
                    }
                ),
                action: PickerAction::ToggleProductionHome,
            },
            PickerRow {
                label: "Model".to_string(),
                detail: "account-wide auto route · server-owned".to_string(),
                action: PickerAction::OpenModel,
            },
            PickerRow {
                label: "Skills".to_string(),
                detail: "server registry · invoke explicitly".to_string(),
                action: PickerAction::OpenSkills,
            },
            PickerRow {
                label: "Credential".to_string(),
                detail: "secure store · managed by /login".to_string(),
                action: PickerAction::None,
            },
        ];
        if let Some(settings) = app.settings.as_ref() {
            for suite in SETTINGS_SUITES {
                let count = settings
                    .extra
                    .get("schema")
                    .and_then(|schema| schema.get(suite))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                rows.push(PickerRow {
                    label: title_case_suite(suite),
                    detail: if count == 0 {
                        "no configurable settings · server contract".to_string()
                    } else {
                        format!(
                            "{count} setting{} · server schema",
                            if count == 1 { "" } else { "s" }
                        )
                    },
                    action: PickerAction::OpenSuite(suite.to_string()),
                });
            }
        }
        Self {
            title: "Settings".to_string(),
            rows,
            selected: 0,
        }
    }

    fn suite(app: &App, suite: &str) -> Self {
        let Some(settings) = app.settings.as_ref() else {
            return Self {
                title: format!("{} settings", title_case_suite(suite)),
                rows: vec![PickerRow {
                    label: "Unavailable".to_string(),
                    detail: "settings schema has not arrived".to_string(),
                    action: PickerAction::None,
                }],
                selected: 0,
            };
        };
        let specs = settings
            .extra
            .get("schema")
            .and_then(|schema| schema.get(suite))
            .and_then(Value::as_array);
        let mut rows = specs
            .into_iter()
            .flatten()
            .filter_map(|spec| {
                let key = spec.get("key")?.as_str()?;
                let label = spec.get("label").and_then(Value::as_str).unwrap_or(key);
                let scope = spec.get("scope").and_then(Value::as_str).unwrap_or("team");
                let value = resolved_setting_value(settings, suite, key, scope, spec);
                Some(PickerRow {
                    label: label.to_string(),
                    detail: format!(
                        "{} · {scope} · {}",
                        display_setting_value(&value),
                        spec.get("reader")
                            .and_then(Value::as_str)
                            .unwrap_or("server")
                    ),
                    action: PickerAction::OpenSetting {
                        suite: suite.to_string(),
                        spec: spec.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            rows.push(PickerRow {
                label: "No configurable settings".to_string(),
                detail: "the server intentionally exposes no dial for this suite".to_string(),
                action: PickerAction::None,
            });
        }
        Self {
            title: format!("{} settings", title_case_suite(suite)),
            rows,
            selected: 0,
        }
    }

    fn setting_values(app: &App, suite: &str, spec: &Value) -> Self {
        let key = spec.get("key").and_then(Value::as_str).unwrap_or("setting");
        let label = spec.get("label").and_then(Value::as_str).unwrap_or(key);
        let scope = spec.get("scope").and_then(Value::as_str).unwrap_or("team");
        let current = app
            .settings
            .as_ref()
            .map(|settings| resolved_setting_value(settings, suite, key, scope, spec))
            .unwrap_or(Value::Null);
        let values = match spec.get("type").and_then(Value::as_str) {
            Some("bool") => vec![Value::Bool(true), Value::Bool(false)],
            Some("enum") => spec
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut rows = values
            .into_iter()
            .map(|value| PickerRow {
                label: display_setting_value(&value),
                detail: if value == current {
                    format!("current · {scope}")
                } else {
                    scope.to_string()
                },
                action: PickerAction::SetSetting {
                    suite: suite.to_string(),
                    key: key.to_string(),
                    scope: scope.to_string(),
                    value,
                },
            })
            .collect::<Vec<_>>();
        if !matches!(
            spec.get("type").and_then(Value::as_str),
            Some("bool" | "enum")
        ) {
            rows.push(PickerRow {
                label: "Enter a value".to_string(),
                detail: setting_input_hint(spec),
                action: PickerAction::PromptSetting {
                    suite: suite.to_string(),
                    spec: spec.clone(),
                },
            });
        }
        Self {
            title: label.to_string(),
            rows,
            selected: 0,
        }
    }

    fn autonomy(app: &App) -> Self {
        let current = app.server_mode.as_deref().unwrap_or(&app.local_mode);
        let definitions = [
            ("read_only", "plan", "verify · remember · retrieve · advise"),
            (
                "propose",
                "accept-edits",
                "+ sandboxed diff · reviewable PR",
            ),
            (
                "branch",
                "branch",
                "+ push to non-main branch · run CI · never merge",
            ),
            (
                "execute",
                "auto",
                "+ guarded merge · commands · otherwise reviewable PR",
            ),
        ];
        Self {
            title: "Autonomy".to_string(),
            rows: definitions
                .into_iter()
                .map(|(wire, label, detail)| PickerRow {
                    label: label.to_string(),
                    detail: format!(
                        "{detail}{}",
                        if wire == current { " · current" } else { "" }
                    ),
                    action: PickerAction::SelectMode(wire.to_string()),
                })
                .collect(),
            selected: commands::mode_rank(current).unwrap_or(0),
        }
    }

    fn confirm_mode(target: &str) -> Self {
        let label = commands::mode_name(target);
        let detail = match target {
            "propose" => "permits sandboxed diffs and reviewable PRs",
            "branch" => "permits non-main branch pushes and CI",
            "execute" => {
                "permits commands and guarded merge; otherwise opens a reviewable PR; Estelle does not deploy"
            }
            _ => "lowers the account ceiling",
        };
        Self {
            title: format!("Confirm raise to {label}"),
            rows: vec![
                PickerRow {
                    label: format!("Confirm {label}"),
                    detail: detail.to_string(),
                    action: PickerAction::ConfirmMode(target.to_string()),
                },
                PickerRow {
                    label: "Cancel".to_string(),
                    detail: "leave the account ceiling unchanged".to_string(),
                    action: PickerAction::OpenMode,
                },
            ],
            selected: 0,
        }
    }

    fn themes(app: &App) -> Self {
        Self {
            title: "Theme".to_string(),
            rows: [Theme::Dark, Theme::CreamInk]
                .into_iter()
                .map(|theme| PickerRow {
                    label: theme.name().to_string(),
                    detail: if theme == app.theme {
                        "current · cream · black · white · one red".to_string()
                    } else {
                        "cream · black · white · one red".to_string()
                    },
                    action: PickerAction::SelectTheme(theme),
                })
                .collect(),
            selected: usize::from(app.theme == Theme::CreamInk),
        }
    }

    fn model(reply: &CommandReply) -> Self {
        let active = reply.extra.get("active").and_then(Value::as_object);
        let active_provider = active
            .and_then(|row| row.get("provider"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let active_model = active
            .and_then(|row| row.get("model"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut rows = Vec::new();
        for provider in reply
            .extra
            .get("providers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = provider
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("provider");
            let label = provider.get("label").and_then(Value::as_str).unwrap_or(id);
            for model in provider
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let current = id == active_provider && model == active_model;
                let can_edit = reply
                    .extra
                    .get("can_edit")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                rows.push(PickerRow {
                    label: model.to_string(),
                    detail: format!(
                        "{label} · account-wide{}",
                        if current { " · current" } else { "" }
                    ),
                    action: if can_edit {
                        PickerAction::SelectProvider {
                            provider: id.to_string(),
                            provider_label: label.to_string(),
                            model: model.to_string(),
                        }
                    } else {
                        PickerAction::None
                    },
                });
            }
        }
        if rows.is_empty() {
            rows.push(PickerRow {
                label: "Auto routing".to_string(),
                detail: "provider pool unavailable · no setting was inferred".to_string(),
                action: PickerAction::None,
            });
        }
        Self {
            title: "Model pool · account-wide".to_string(),
            rows,
            selected: 0,
        }
    }

    fn skills(reply: &CommandReply) -> Self {
        let mut rows = reply
            .extra
            .get("skills")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|skill| {
                let name = skill.get("name").and_then(Value::as_str)?;
                let valid = !name.is_empty()
                    && name.len() <= 96
                    && name.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && !is_secret_shaped(name);
                valid.then(|| PickerRow {
                    label: name.to_string(),
                    detail: skill
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(mask_secret)
                        .unwrap_or_else(|| "server playbook".to_string()),
                    action: PickerAction::InvokeSkill(name.to_string()),
                })
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            rows.push(PickerRow {
                label: "No playbooks returned".to_string(),
                detail: "the server registry is empty".to_string(),
                action: PickerAction::None,
            });
        }
        Self {
            title: "Skills".to_string(),
            rows,
            selected: 0,
        }
    }
}

#[derive(Debug)]
enum CommandFailure {
    Client(Error),
    Local([String; 3]),
}

impl GateModal {
    fn from_reply(reply: &CommandReply, inspected_files: &[DiffFileStat]) -> Option<Self> {
        let verdict_value = reply
            .verdict
            .as_ref()
            .or(reply.gate.as_ref())
            .or(reply.merge.as_ref());
        let has_blockers = reply
            .extra
            .get("blockers")
            .and_then(Value::as_array)
            .is_some_and(|blockers| !blockers.is_empty());
        if !has_blockers && !verdict_value.is_some_and(gate_value_refuses) {
            return None;
        }

        let verdict = verdict_value
            .map(render_gate_value)
            .unwrap_or_else(|| "blocked".to_string());
        let mut reasons = reply
            .extra
            .get("blockers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(render_gate_blocker)
            .collect::<Vec<_>>();
        if reasons.is_empty()
            && let Some(reason) = reply
                .reason
                .as_ref()
                .or(reply.unverified_reason.as_ref())
                .filter(|reason| !reason.trim().is_empty())
        {
            reasons.push(reason.clone());
        }
        if reasons.is_empty() {
            reasons.push("The server returned no blocker detail.".to_string());
        }

        Some(Self {
            verdict,
            reasons,
            files: inspected_files.to_vec(),
        })
    }
}

fn gate_value_refuses(value: &Value) -> bool {
    match value {
        Value::Bool(value) => !value,
        Value::String(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "blocked"
                | "flagged"
                | "failed"
                | "refused"
                | "abstained"
                | "rejected"
                | "unsafe"
                | "unverified"
        ),
        _ => false,
    }
}

fn render_gate_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => "blocked".to_string(),
    }
}

fn render_gate_blocker(blocker: &Value) -> String {
    let Some(object) = blocker.as_object() else {
        return render_gate_value(blocker);
    };
    let location = object
        .get("file")
        .or_else(|| object.get("path"))
        .and_then(Value::as_str)
        .map(|path| {
            object
                .get("line")
                .and_then(Value::as_u64)
                .map_or_else(|| path.to_string(), |line| format!("{path}:{line}"))
        });
    let reason = ["reason", "message", "body", "title", "detail"]
        .into_iter()
        .find_map(|key| object.get(key).and_then(Value::as_str))
        .unwrap_or("blocked by the server");
    location.map_or_else(
        || reason.to_string(),
        |location| format!("{location}  {reason}"),
    )
}

struct App {
    boot: Option<BootScene>,
    boot_started: Instant,
    has_submitted_question: bool,
    composer: ComposerInput,
    transcript: Vec<TranscriptEntry>,
    queue: VecDeque<QueuedRequest>,
    active: Option<ActiveRequest>,
    header: HeaderState,
    account: Option<AccountResponse>,
    session_context: Option<session_gap::SessionContext>,
    repo: Repo,
    client: Option<Client>,
    session: Option<session_server::SessionHandle>,
    session_id: String,
    session_tabs: Vec<session_server::SessionSummary>,
    hidden_session_tabs: BTreeSet<String>,
    session_questions: BTreeSet<u64>,
    session_completed: BTreeSet<u64>,
    session_file_shifts: BTreeSet<u64>,
    auth: Option<AuthContext>,
    /// Per-skill conversations for interactive `/skill:` runs, and the run currently in flight.
    /// The server continues a skill over `messages` when they arrive and restarts from `task`
    /// when they don't — without this, every follow-up was a silent fresh start.
    skill_threads: HashMap<String, Vec<(String, String)>>,
    pending_skill: Option<(String, String)>,
    /// Routes whose responses explicitly rejected the stored credential this session. Deletion
    /// is justified only when this holds DIFFERENT routes — see `clear_rejected`.
    rejected_routes: BTreeSet<String>,
    next_request_id: u64,
    credential_input_hidden: bool,
    auth_resolved: bool,
    root: PathBuf,
    last_question: Option<String>,
    last_diff: Option<String>,
    last_applied_diff: Option<String>,
    should_exit: bool,
    local_mode: String,
    server_mode: Option<String>,
    active_model: Option<String>,
    active_model_observed_at: Option<Instant>,
    citations: Vec<Source>,
    sweep_progress: Option<top_level::SweepProgress>,
    gate_modal: Option<GateModal>,
    fleet: Option<estelle_client::FleetSnapshot>,
    todo: Option<estelle_client::TodoSnapshot>,
    todo_visible: bool,
    todo_expanded: bool,
    context_panel_visible: bool,
    diff_panel_visible: bool,
    prod_panel_visible: bool,
    prod_issues: Option<estelle_client::MonitorIssuesResponse>,
    prod_overview: Option<estelle_client::MonitorOverviewResponse>,
    prod_agent_health: Option<estelle_client::AgentHealthResponse>,
    prod_github_status: Option<estelle_client::GithubStatusResponse>,
    prod_proposed_prs: Option<estelle_client::ProposedPrsResponse>,
    prod_issue_error: Option<String>,
    prod_overview_error: Option<String>,
    prod_agent_health_error: Option<String>,
    prod_github_status_error: Option<String>,
    prod_proposed_prs_error: Option<String>,
    prod_issue_next_poll: Option<Instant>,
    prod_overview_next_poll: Option<Instant>,
    prod_agent_health_next_poll: Option<Instant>,
    prod_github_next_poll: Option<Instant>,
    prod_issue_in_flight: bool,
    prod_overview_in_flight: bool,
    prod_agent_health_in_flight: bool,
    prod_github_in_flight: bool,
    prod_issue_failures: u32,
    prod_overview_failures: u32,
    prod_agent_health_failures: u32,
    prod_github_failures: u32,
    prod_issue_since: Option<f64>,
    terminal_focused: bool,
    last_interaction: Instant,
    working_memory_paths: Vec<String>,
    transcript_scroll: usize,
    dither_wake: VecDeque<usize>,
    palette_index: usize,
    picker: Option<PickerSurface>,
    settings: Option<CommandReply>,
    pending_setting_input: Option<PendingSettingInput>,
    pending_login: Option<PendingLogin>,
    login_required: bool,
    focus: FocusSurface,
    theme: Theme,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FocusSurface {
    Composer,
    Transcript,
    Auxiliary,
}

fn estelle_composer() -> ComposerInput {
    let mut composer = ComposerInput::plain_text_with_placeholder("Ask Estelle");
    composer.set_hint_items(Vec::<(String, String)>::new());
    composer
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn whoami_lines(app: &App, local_plan_present: bool) -> Vec<String> {
    let server_plan_present = app.account.as_ref().is_some_and(|account| {
        account
            .extra
            .get("uses_plan")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    });
    let providers = match app.account.as_ref() {
        Some(account) => match account.extra.get("configured").and_then(Value::as_array) {
            Some(configured) => {
                let names = configured
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            }
            None => "not returned by server".to_string(),
        },
        None if app.auth.is_some() => "not returned yet".to_string(),
        None => "unavailable until /login".to_string(),
    };
    vec![
        format!(
            "Estelle account  {}",
            if app.auth.is_some() { "yes" } else { "no" }
        ),
        format!(
            "Model plan  {}",
            if local_plan_present || server_plan_present {
                "yes"
            } else {
                "no"
            }
        ),
        format!("Provider keys  {}", providers),
        format!(
            "GitHub Copilot  {}",
            if copilot_login::credential_present() {
                "credential present · entitlement/runtime not yet proven"
            } else {
                "no"
            }
        ),
        format!(
            "Local endpoint  {}",
            if local_provider::configured_present() {
                "configured · runtime not yet proven"
            } else {
                "no"
            }
        ),
        "Credential values are never displayed.".to_string(),
    ]
}

impl App {
    fn new(args: Args) -> Self {
        let root = std::env::current_dir().unwrap_or_default();
        let override_repo = args.repo.and_then(Repo::new);
        let repo = RepoResolver::new(override_repo, &root)
            .resolve()
            .or_else(|| {
                root.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(Repo::new)
            })
            .unwrap_or_default();
        let boot_preferences = BootPreferences {
            already_seen: false,
            force_replay: false,
            reduced_motion: env_truthy("ESTELLE_REDUCED_MOTION"),
            effects_off: std::env::var("ESTELLE_EFFECTS")
                .is_ok_and(|value| value.eq_ignore_ascii_case("off")),
            agent_mode: env_truthy("ESTELLE_AGENT_MODE"),
        };
        let tip_index = repo.as_str().bytes().fold(0_usize, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(byte.into())
        });
        Self {
            boot: boot_preferences
                .should_play()
                .then(|| BootScene::new(tip_index)),
            boot_started: Instant::now(),
            has_submitted_question: false,
            composer: estelle_composer(),
            transcript: Vec::new(),
            queue: VecDeque::new(),
            active: None,
            header: HeaderState::default(),
            account: None,
            session_context: None,
            repo,
            client: None,
            session: None,
            session_id: "main".to_string(),
            session_tabs: Vec::new(),
            hidden_session_tabs: BTreeSet::new(),
            session_questions: BTreeSet::new(),
            session_completed: BTreeSet::new(),
            session_file_shifts: BTreeSet::new(),
            auth: None,
            skill_threads: HashMap::new(),
            pending_skill: None,
            rejected_routes: BTreeSet::new(),
            // Request IDs cross process boundaries and can be created by several attached
            // terminals. A random seed avoids the reconnect collision of every client starting
            // at zero; the server also rejects any repeated ID within a session.
            next_request_id: rand::random(),
            credential_input_hidden: false,
            auth_resolved: false,
            root,
            last_question: None,
            last_diff: None,
            last_applied_diff: None,
            should_exit: false,
            local_mode: "read_only".to_string(),
            server_mode: None,
            active_model: None,
            active_model_observed_at: None,
            citations: Vec::new(),
            sweep_progress: None,
            gate_modal: None,
            fleet: None,
            todo: None,
            todo_visible: false,
            todo_expanded: false,
            context_panel_visible: false,
            diff_panel_visible: false,
            prod_panel_visible: false,
            prod_issues: None,
            prod_overview: None,
            prod_agent_health: None,
            prod_github_status: None,
            prod_proposed_prs: None,
            prod_issue_error: None,
            prod_overview_error: None,
            prod_agent_health_error: None,
            prod_github_status_error: None,
            prod_proposed_prs_error: None,
            prod_issue_next_poll: Some(Instant::now()),
            prod_overview_next_poll: Some(Instant::now()),
            prod_agent_health_next_poll: Some(Instant::now()),
            prod_github_next_poll: Some(Instant::now()),
            prod_issue_in_flight: false,
            prod_overview_in_flight: false,
            prod_agent_health_in_flight: false,
            prod_github_in_flight: false,
            prod_issue_failures: 0,
            prod_overview_failures: 0,
            prod_agent_health_failures: 0,
            prod_github_failures: 0,
            prod_issue_since: None,
            terminal_focused: true,
            last_interaction: Instant::now(),
            working_memory_paths: Vec::new(),
            transcript_scroll: 0,
            dither_wake: VecDeque::from([0]),
            palette_index: 0,
            picker: None,
            settings: None,
            pending_setting_input: None,
            pending_login: None,
            login_required: false,
            focus: FocusSurface::Composer,
            theme: Theme::Dark,
        }
    }

    fn submit(&mut self, text: String, tx: &mpsc::UnboundedSender<UiEvent>) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(pending) = self.pending_setting_input.take() {
            let key = pending
                .spec
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("setting")
                .to_string();
            let scope = pending
                .spec
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("team")
                .to_string();
            match parse_setting_input(&pending.spec, &text) {
                Ok(value) => self.save_setting(pending.suite, key, scope, value, tx),
                Err(reason) => {
                    self.transcript.push(TranscriptEntry::Failure([
                        "That value does not match the server schema.".to_string(),
                        reason,
                        "Open /settings and choose the setting again.".to_string(),
                    ]));
                }
            }
            self.composer = estelle_composer();
            return;
        }
        self.transcript_scroll = 0;
        self.dither_wake.clear();
        self.sweep_progress = None;
        if is_secret_shaped(&text) {
            self.transcript
                .push(TranscriptEntry::User(mask_secret(&text)));
            self.transcript.push(TranscriptEntry::System(
                "Credential-shaped input was masked and was not sent.".to_string(),
            ));
            self.composer = estelle_composer();
            return;
        }
        let parsed = commands::parse_input(&text);
        if matches!(parsed, commands::ParsedInput::Ask(_))
            && !self.has_submitted_question
            && let Some(lines) = session_handoff_lines(self)
        {
            self.transcript.push(TranscriptEntry::SessionHandoff(lines));
        }
        self.transcript.push(TranscriptEntry::User(text));
        match parsed {
            commands::ParsedInput::Ask(question) => {
                self.has_submitted_question = true;
                self.last_question = Some(question.clone());
                self.queue.push_back(QueuedRequest::Question {
                    question,
                    session_context: self
                        .session_context
                        .as_ref()
                        .map(session_gap::SessionContext::model_context),
                });
            }
            commands::ParsedInput::Shell(command) => {
                if command.is_empty() {
                    self.transcript.push(TranscriptEntry::System(
                        "A shell command needs text after !, for example !git status.".to_string(),
                    ));
                } else {
                    self.queue.push_back(QueuedRequest::Shell(command));
                }
            }
            commands::ParsedInput::Command {
                name: None,
                typed_name,
                ..
            } => self.transcript.push(TranscriptEntry::System(format!(
                "Unknown command /{typed_name}; nothing ran and nothing was sent. Use /help."
            ))),
            commands::ParsedInput::Command {
                name: Some(name),
                typed_name,
                argument,
            } => {
                if typed_name != name && name != "skill:" {
                    self.transcript.push(TranscriptEntry::System(format!(
                        "Interpreted /{typed_name} as /{name}."
                    )));
                }
                let parsed = commands::ParsedInput::Command {
                    name: Some(name),
                    typed_name,
                    argument: argument.clone(),
                };
                if let Some(refusal) = parsed.local_refusal() {
                    self.transcript
                        .push(TranscriptEntry::System(format!("{refusal}.")));
                } else if name == "work"
                    && let Some(refusal) = self.write_refusal()
                {
                    self.transcript.push(TranscriptEntry::System(refusal));
                } else if !self.handle_local_command(name, &argument, tx) {
                    let skill_thread = if name == "skill:" {
                        let mut parts = argument.splitn(2, char::is_whitespace);
                        let skill = parts.next().unwrap_or_default().to_string();
                        let task = parts.next().unwrap_or_default().trim().to_string();
                        self.pending_skill = Some((skill.clone(), task));
                        self.skill_threads.get(&skill).cloned()
                    } else {
                        None
                    };
                    self.queue.push_back(QueuedRequest::Command(PendingCommand {
                        name,
                        argument,
                        last_question: self.last_question.clone(),
                        skill_thread,
                    }));
                }
            }
        }
        self.start_next(tx);
    }

    fn boot_elapsed_ms(&self, now: Instant) -> u64 {
        u64::try_from(now.saturating_duration_since(self.boot_started).as_millis())
            .unwrap_or(u64::MAX)
    }

    fn boot_active(&self, now: Instant) -> bool {
        self.boot
            .as_ref()
            .is_some_and(|boot| !boot.phase(self.boot_elapsed_ms(now)).is_finished())
    }

    fn skip_boot(&mut self, now: Instant) {
        let elapsed_ms = self.boot_elapsed_ms(now);
        if let Some(boot) = self.boot.as_mut() {
            boot.skip(elapsed_ms);
        }
    }

    fn record_dither_caret(&mut self) {
        let cursor = self.composer.cursor();
        if self.dither_wake.back() == Some(&cursor) {
            return;
        }
        self.dither_wake.push_back(cursor);
        while self.dither_wake.len() > 5 {
            self.dither_wake.pop_front();
        }
    }

    fn handle_local_command(
        &mut self,
        name: &'static str,
        argument: &str,
        tx: &mpsc::UnboundedSender<UiEvent>,
    ) -> bool {
        match name {
            "login" => match argument.trim() {
                "" => self.picker = Some(PickerSurface::login()),
                "--chatgpt" => {
                    self.pending_login = Some(PendingLogin::Chatgpt);
                    self.transcript.push(TranscriptEntry::System(
                        "Starting the ChatGPT credential flow here.".to_string(),
                    ));
                }
                value if value.starts_with("--api-key ") || value.starts_with("--provider ") => {
                    let api_key_route = value.starts_with("--api-key ");
                    let provider = value
                        .strip_prefix("--api-key ")
                        .or_else(|| value.strip_prefix("--provider "))
                        .unwrap_or_default()
                        .trim();
                    self.queue_provider_login(provider, api_key_route);
                }
                _ => self.transcript.push(TranscriptEntry::System(
                    "Usage: /login, /login --chatgpt, or /login --provider <provider>.".to_string(),
                )),
            },
            "logout" => self.logout_local_credentials(),
            "whoami" => self.transcript.push(TranscriptEntry::Command {
                name: "whoami".to_string(),
                lines: whoami_lines(
                    self,
                    login::chatgpt_credential_present()
                        || claude_import::imported_credential_present(),
                ),
            }),
            "doctor" => self.transcript.push(TranscriptEntry::Command {
                name: "doctor".to_string(),
                lines: doctor::lines(doctor::Context::Tui),
            }),
            "help" => self.transcript.push(TranscriptEntry::Command {
                name: "help".to_string(),
                lines: commands::help_lines(),
            }),
            "sweep" => {
                self.sweep_progress = Some(top_level::SweepProgress {
                    state: "preparing sweep".to_string(),
                    percent: 0.0,
                    files: 0,
                    bytes: 0,
                });
                self.queue.push_back(QueuedRequest::Sweep);
            }
            "context" => self.toggle_context_panel(),
            "prod" => self.toggle_prod_panel(tx),
            "diff" => self.toggle_diff_panel(),
            "todo" => self.toggle_todo_surface(),
            "settings" => self.picker = Some(PickerSurface::settings(self)),
            "apply" => {
                if let Some(refusal) = self.write_refusal() {
                    self.transcript.push(TranscriptEntry::System(refusal));
                } else if let Some(diff) = self.last_diff.clone() {
                    self.queue.push_back(QueuedRequest::Apply {
                        diff,
                        reverse: false,
                    });
                } else {
                    self.transcript.push(TranscriptEntry::System(
                        "There is no /work diff to apply.".to_string(),
                    ));
                }
            }
            "undo" => {
                if let Some(diff) = self.last_applied_diff.clone() {
                    self.queue.push_back(QueuedRequest::Apply {
                        diff,
                        reverse: true,
                    });
                } else {
                    self.transcript.push(TranscriptEntry::System(
                        "There is no Estelle apply to undo.".to_string(),
                    ));
                }
            }
            "mode" => {
                if argument.trim().is_empty() {
                    self.picker = Some(PickerSurface::autonomy(self));
                } else {
                    let Some(mode) = commands::parse_mode(argument) else {
                        self.transcript.push(TranscriptEntry::System(format!(
                            "No mode called {argument}. Use plan, accept-edits, branch, or auto."
                        )));
                        return true;
                    };
                    self.request_autonomy(mode.to_string(), tx);
                }
            }
            "plan" => {
                let target = match argument.trim().to_ascii_lowercase().as_str() {
                    "" | "on" => "read_only",
                    "off" => "propose",
                    value => {
                        self.transcript.push(TranscriptEntry::System(format!(
                            "No /plan action called {value}. Use /plan, /plan on, or /plan off."
                        )));
                        return true;
                    }
                };
                self.request_autonomy(target.to_string(), tx);
            }
            "permissions" => self.transcript.push(TranscriptEntry::Command {
                name: "permissions".to_string(),
                lines: commands::mode_lines(&self.local_mode, self.server_mode.as_deref()),
            }),
            "model" if !argument.trim().is_empty() => {
                self.transcript.push(TranscriptEntry::Command {
                    name: "model".to_string(),
                    lines: vec![
                        format!("Requested model: {}", argument.trim()),
                        "Not pinned: the current server exposes only account-wide provider selection, not a session-scoped model override."
                            .to_string(),
                        "Auto routing remains active. No account setting was changed."
                            .to_string(),
                    ],
                });
            }
            "status" => self.transcript.push(TranscriptEntry::Command {
                name: "status".to_string(),
                lines: vec![
                    format!(
                        "endpoint  {}/",
                        estelle_client::DEFAULT_BASE_URL.trim_end_matches('/')
                    ),
                    format!(
                        "credential  {}",
                        if self.client.is_some() {
                            "configured"
                        } else {
                            "not configured"
                        }
                    ),
                    format!("repo  {}", self.repo),
                    format!(
                        "mode  {}",
                        commands::mode_name(commands::effective_mode(
                            &self.local_mode,
                            self.server_mode.as_deref()
                        ))
                    ),
                    format!(
                        "connection  {}",
                        if self.header.connected {
                            "connected"
                        } else {
                            "not confirmed"
                        }
                    ),
                ],
            }),
            "shell" => self.transcript.push(TranscriptEntry::Command {
                name: "shell".to_string(),
                lines: vec![
                    "Run a local command with a leading !, for example !git status.".to_string(),
                    "It runs on this machine; no Estelle request is sent.".to_string(),
                ],
            }),
            "clear" => self.transcript.clear(),
            "exit" => self.should_exit = true,
            _ => {
                let Some(lines) = commands::inherited_command_lines(name) else {
                    return false;
                };
                self.transcript.push(TranscriptEntry::Command {
                    name: name.to_string(),
                    lines,
                });
            }
        }
        let _ = argument;
        true
    }

    fn queue_provider_login(&mut self, name: &str, force_api_key: bool) {
        let lookup = match (force_api_key, name) {
            (true, "openai" | "chatgpt") => "openai-api",
            (true, "claude" | "anthropic-subscription") => "anthropic-api",
            _ => name,
        };
        let Some(provider) = provider_catalog::resolve(lookup) else {
            self.transcript.push(TranscriptEntry::System(
                "Choose a provider from /login; this client will not guess an unknown provider route."
                    .to_string(),
            ));
            return;
        };
        match provider.auth {
            provider_catalog::AuthKind::ClaudeImport => {
                self.transcript.push(TranscriptEntry::System(
                    "Claude import reads the credential Claude Code stored on this machine after this explicit command; it never moves or modifies Claude Code's copy."
                        .to_string(),
                ));
                self.pending_login = Some(PendingLogin::Claude);
            }
            provider_catalog::AuthKind::ChatgptDevice => {
                self.pending_login = Some(PendingLogin::Chatgpt)
            }
            _ if provider.server_provider.is_some() && self.client.is_none() => {
                self.pending_login = Some(PendingLogin::EstelleThenProvider(provider.id));
            }
            _ => self.pending_login = Some(PendingLogin::Provider(provider.id)),
        }
    }

    fn logout_local_credentials(&mut self) {
        let estelle = match self.auth.take() {
            Some(auth) if auth.source == CredentialSource::Environment => {
                "environment credential remains; unset ESTELLE_API_KEY to remove it"
            }
            Some(auth) => match auth.store.delete_stored(auth.source) {
                Ok(true) => "stored credential removed",
                Ok(false) => "no stored credential was removed",
                Err(_) => "stored credential could not be removed",
            },
            None => "no Estelle credential was present",
        };
        let plan = match (login::logout_chatgpt(), claude_import::logout()) {
            (Ok(chatgpt), Ok(claude)) if chatgpt || claude => "stored plan credentials removed",
            (Ok(_), Ok(_)) => "no stored plan credential was present",
            _ => "one or more plan credentials could not be removed",
        };
        let copilot = match copilot_login::logout() {
            Ok(true) => "stored credential removed",
            Ok(false) => "no stored credential was present",
            Err(_) => "stored credential could not be removed",
        };
        let local = match local_provider::logout() {
            Ok(true) => "local endpoint configuration removed",
            Ok(false) => "no local endpoint configuration was present",
            Err(_) => "local endpoint configuration could not be removed",
        };
        self.client = None;
        self.account = None;
        self.header.connected = false;
        self.auth_resolved = true;
        self.login_required = true;
        self.picker = Some(PickerSurface::login());
        self.transcript.push(TranscriptEntry::Command {
            name: "logout".to_string(),
            lines: vec![
                format!("Estelle account  {estelle}"),
                format!("Model plan  {plan}"),
                format!("GitHub Copilot  {copilot}"),
                format!("Local endpoint  {local}"),
                "Server-side provider keys were not deleted.".to_string(),
            ],
        });
    }

    fn write_refusal(&self) -> Option<String> {
        if commands::mode_rank(&self.local_mode).is_some_and(|rank| rank < 1) {
            return Some(format!(
                "Mode is {}; the write path is off. Use /mode accept-edits to allow a reviewable diff.",
                commands::mode_name(&self.local_mode)
            ));
        }
        if self
            .server_mode
            .as_deref()
            .and_then(commands::mode_rank)
            .is_some_and(|rank| rank < 1)
        {
            return Some(
                "The account autonomy dial is plan; Estelle will not write. Use /mode accept-edits or /settings to request a reviewable diff."
                    .to_string(),
            );
        }
        None
    }

    fn toggle_context_panel(&mut self) {
        self.context_panel_visible = !self.context_panel_visible;
        if self.context_panel_visible {
            self.prod_panel_visible = false;
            self.diff_panel_visible = false;
            self.focus = FocusSurface::Auxiliary;
        } else if self.focus == FocusSurface::Auxiliary {
            self.focus = FocusSurface::Composer;
        }
        self.transcript.push(TranscriptEntry::Command {
            name: "context".to_string(),
            lines: vec![format!(
                "Grounding context side panel {}. Alt+M or /context toggles it.",
                if self.context_panel_visible {
                    "opened"
                } else {
                    "closed"
                }
            )],
        });
    }

    fn toggle_diff_panel(&mut self) {
        let Some(diff) = self
            .last_diff
            .as_deref()
            .filter(|diff| !diff.trim().is_empty())
        else {
            self.transcript.push(TranscriptEntry::System(
                "No proposed repair is available. Run /work first; local changes remain available with !git diff --no-color."
                    .to_string(),
            ));
            return;
        };
        let file_count = diff
            .lines()
            .filter(|line| line.starts_with("diff --git "))
            .count();
        self.diff_panel_visible = !self.diff_panel_visible;
        if self.diff_panel_visible {
            self.prod_panel_visible = false;
            self.context_panel_visible = false;
            self.focus = FocusSurface::Auxiliary;
        } else if self.focus == FocusSurface::Auxiliary {
            self.focus = FocusSurface::Composer;
        }
        self.transcript.push(TranscriptEntry::System(format!(
            "Proposed repair side panel {} · {file_count} file{} · read-only until /apply.",
            if self.diff_panel_visible {
                "opened"
            } else {
                "closed"
            },
            if file_count == 1 { "" } else { "s" }
        )));
    }

    fn toggle_todo_surface(&mut self) {
        if self.todo.is_none() {
            self.transcript.push(TranscriptEntry::System(
                "Todo state unavailable: the server has not emitted a task ledger for this session."
                    .to_string(),
            ));
            return;
        }
        self.todo_visible = !self.todo_visible;
        self.transcript.push(TranscriptEntry::System(format!(
            "Todo task ledger {}. Ctrl+T expands or collapses it.",
            if self.todo_visible {
                "opened"
            } else {
                "closed"
            }
        )));
    }

    fn activate_picker(&mut self, tx: &mpsc::UnboundedSender<UiEvent>) {
        let action = self
            .picker
            .as_ref()
            .and_then(|picker| picker.rows.get(picker.selected))
            .map(|row| row.action.clone())
            .unwrap_or(PickerAction::None);
        match action {
            PickerAction::LoginEstelle => {
                self.picker = None;
                self.pending_login = Some(PendingLogin::Estelle);
            }
            PickerAction::LoginClaude => {
                self.picker = None;
                self.transcript.push(TranscriptEntry::System(
                    "Claude import reads the credential Claude Code stored on this machine after this explicit selection; it never moves or modifies Claude Code's copy."
                        .to_string(),
                ));
                self.pending_login = Some(PendingLogin::Claude);
            }
            PickerAction::LoginChatgpt => {
                self.picker = None;
                self.pending_login = Some(PendingLogin::Chatgpt);
            }
            PickerAction::OpenProviderLogin => {
                self.picker = Some(PickerSurface::provider_login());
            }
            PickerAction::LoginProvider(provider) => {
                self.picker = None;
                self.queue_provider_login(provider, false);
            }
            PickerAction::OpenLocalLogin => {
                self.picker = Some(PickerSurface::local_login());
            }
            PickerAction::LoginLocal(provider) => {
                self.picker = None;
                self.queue_provider_login(provider, false);
            }
            PickerAction::OpenMode => self.picker = Some(PickerSurface::autonomy(self)),
            PickerAction::SelectMode(target) => {
                self.request_autonomy(target, tx);
            }
            PickerAction::ConfirmMode(target) => self.change_autonomy(target, tx),
            PickerAction::OpenTheme => self.picker = Some(PickerSurface::themes(self)),
            PickerAction::SelectTheme(theme) => {
                self.theme = theme;
                self.picker = Some(PickerSurface::settings(self));
                if let Some(client) = self.client.clone() {
                    spawn_theme_save(client, theme, tx);
                } else {
                    self.transcript.push(TranscriptEntry::System(format!(
                        "{} is active for this session only. Run /login to save it to personal settings.",
                        theme.name()
                    )));
                }
            }
            PickerAction::ToggleProductionHome => {
                self.toggle_prod_panel(tx);
                let selected = self.picker.as_ref().map_or(0, |picker| picker.selected);
                let mut picker = PickerSurface::settings(self);
                picker.selected = selected.min(picker.rows.len().saturating_sub(1));
                self.picker = Some(picker);
            }
            PickerAction::OpenModel => {
                self.picker = None;
                self.submit("/model".to_string(), tx);
            }
            PickerAction::SelectProvider {
                provider,
                provider_label,
                model,
            } => {
                let Some(client) = self.client.clone() else {
                    self.picker = None;
                    self.transcript.push(TranscriptEntry::System(
                        "Model selection needs an Estelle credential. Run /login.".to_string(),
                    ));
                    return;
                };
                self.picker = None;
                spawn_provider_selection(client, provider, provider_label, model, tx);
            }
            PickerAction::OpenSkills => {
                self.picker = None;
                self.submit("/skills".to_string(), tx);
            }
            PickerAction::InvokeSkill(name) => {
                self.picker = None;
                self.submit(format!("/skill:{name}"), tx);
            }
            PickerAction::OpenSuite(suite) => {
                self.picker = Some(PickerSurface::suite(self, &suite));
            }
            PickerAction::OpenSetting { suite, spec } => {
                self.picker = Some(PickerSurface::setting_values(self, &suite, &spec));
            }
            PickerAction::SetSetting {
                suite,
                key,
                scope,
                value,
            } => self.save_setting(suite, key, scope, value, tx),
            PickerAction::PromptSetting { suite, spec } => {
                let label = spec
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("setting")
                    .to_string();
                self.pending_setting_input = Some(PendingSettingInput { suite, spec });
                self.picker = None;
                self.composer =
                    ComposerInput::plain_text_with_placeholder(format!("Enter {label}"));
            }
            PickerAction::None => {}
        }
    }

    fn save_setting(
        &mut self,
        suite: String,
        key: String,
        scope: String,
        value: Value,
        tx: &mpsc::UnboundedSender<UiEvent>,
    ) {
        let Some(client) = self.client.clone() else {
            self.picker = None;
            self.transcript.push(TranscriptEntry::System(
                "Changing a setting needs an Estelle credential. Run /login.".to_string(),
            ));
            return;
        };
        self.picker = None;
        spawn_setting_save(client, suite, key, scope, value, tx);
    }

    fn change_autonomy(&mut self, target: String, tx: &mpsc::UnboundedSender<UiEvent>) {
        let Some(client) = self.client.clone() else {
            self.picker = None;
            self.transcript.push(TranscriptEntry::System(
                "Changing autonomy needs an Estelle credential. Run /login.".to_string(),
            ));
            return;
        };
        self.picker = None;
        spawn_autonomy_change(client, target, tx);
    }

    fn request_autonomy(&mut self, target: String, tx: &mpsc::UnboundedSender<UiEvent>) {
        let current = self.server_mode.as_deref().unwrap_or(&self.local_mode);
        let raising =
            commands::mode_rank(&target).unwrap_or(0) > commands::mode_rank(current).unwrap_or(0);
        if raising {
            self.picker = Some(PickerSurface::confirm_mode(&target));
        } else {
            self.change_autonomy(target, tx);
        }
    }

    fn toggle_prod_panel(&mut self, tx: &mpsc::UnboundedSender<UiEvent>) {
        self.prod_panel_visible = !self.prod_panel_visible;
        if self.prod_panel_visible {
            self.context_panel_visible = false;
            self.diff_panel_visible = false;
            self.focus = FocusSurface::Auxiliary;
            self.prod_issue_next_poll = Some(Instant::now());
            self.prod_overview_next_poll = Some(Instant::now());
            self.prod_agent_health_next_poll = Some(Instant::now());
            self.prod_github_next_poll = Some(Instant::now());
            self.poll_production_if_due(tx);
        } else {
            self.prod_issue_next_poll = None;
            self.prod_overview_next_poll = None;
            self.prod_agent_health_next_poll = None;
            self.prod_github_next_poll = None;
            if self.focus == FocusSurface::Auxiliary {
                self.focus = FocusSurface::Composer;
            }
        }
        self.transcript.push(TranscriptEntry::System(format!(
            "Production health {}.",
            if self.prod_panel_visible {
                "opened"
            } else {
                "closed"
            }
        )));
    }

    fn has_auxiliary_surface(&self) -> bool {
        self.context_panel_visible
            || self.diff_panel_visible
            || self.prod_panel_visible
            || !self.citations.is_empty()
    }

    fn move_focus(&mut self, forward: bool) {
        self.focus = match (self.focus, forward, self.has_auxiliary_surface()) {
            (FocusSurface::Composer, true, _) => FocusSurface::Transcript,
            (FocusSurface::Transcript, true, true) => FocusSurface::Auxiliary,
            (FocusSurface::Transcript, true, false) => FocusSurface::Composer,
            (FocusSurface::Auxiliary, true, _) => FocusSurface::Composer,
            (FocusSurface::Composer, false, true) => FocusSurface::Auxiliary,
            (FocusSurface::Composer, false, false) => FocusSurface::Transcript,
            (FocusSurface::Transcript, false, _) => FocusSurface::Composer,
            (FocusSurface::Auxiliary, false, _) => FocusSurface::Transcript,
        };
    }

    fn poll_production_if_due(&mut self, tx: &mpsc::UnboundedSender<UiEvent>) {
        if !self.prod_panel_visible {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let now = Instant::now();
        if !self.prod_issue_in_flight
            && self
                .prod_issue_next_poll
                .is_some_and(|deadline| deadline <= now)
        {
            self.prod_issue_in_flight = true;
            self.prod_issue_next_poll = None;
            spawn_prod_issues_request(client.clone(), self.repo.clone(), self.prod_issue_since, tx);
        }
        if !self.prod_overview_in_flight
            && self
                .prod_overview_next_poll
                .is_some_and(|deadline| deadline <= now)
        {
            self.prod_overview_in_flight = true;
            self.prod_overview_next_poll = None;
            spawn_prod_overview_request(client.clone(), self.repo.clone(), tx);
        }
        if !self.prod_agent_health_in_flight
            && self
                .prod_agent_health_next_poll
                .is_some_and(|deadline| deadline <= now)
        {
            self.prod_agent_health_in_flight = true;
            self.prod_agent_health_next_poll = None;
            spawn_prod_agent_health_request(client.clone(), self.repo.clone(), tx);
        }
        if !self.prod_github_in_flight
            && self
                .prod_github_next_poll
                .is_some_and(|deadline| deadline <= now)
        {
            self.prod_github_in_flight = true;
            self.prod_github_next_poll = None;
            spawn_prod_github_request(client, self.repo.clone(), tx);
        }
    }

    fn production_is_inactive(&self) -> bool {
        !self.terminal_focused || self.last_interaction.elapsed() >= Duration::from_secs(300)
    }

    fn start_next(&mut self, tx: &mpsc::UnboundedSender<UiEvent>) {
        if self.active.is_some() {
            return;
        }
        let Some(pending) = self.queue.pop_front() else {
            return;
        };
        match pending {
            QueuedRequest::Shell(command) => {
                let (id, cancel) = self.begin_active("shell");
                let tx = tx.clone();
                let root = self.root.clone();
                tokio::spawn(async move {
                    let result = execute_shell(&root, &command, &cancel).await;
                    let _ = tx.send(UiEvent::LocalAnswer {
                        id,
                        name: "shell",
                        result,
                    });
                });
            }
            QueuedRequest::Apply { diff, reverse } => {
                let name = if reverse { "undo" } else { "apply" };
                let (id, cancel) = self.begin_active(name);
                let tx = tx.clone();
                let root = self.root.clone();
                tokio::spawn(async move {
                    let result = apply_diff(&root, &diff, reverse, &cancel).await;
                    let _ = tx.send(UiEvent::LocalAnswer { id, name, result });
                });
            }
            QueuedRequest::Question {
                question,
                session_context,
            } => {
                if let Some(session) = self.session.clone() {
                    let (id, _cancel) = self.begin_active("thinking");
                    self.session_questions.insert(id);
                    if let Err(error) = session.send(session_server::ClientRequest::Ask {
                        id,
                        question,
                        session_context,
                    }) {
                        self.active = None;
                        self.transcript.push(TranscriptEntry::Failure([
                            "The session request was not sent.".to_string(),
                            error.to_string(),
                            "Run estelle connect to reattach, then retry.".to_string(),
                        ]));
                        self.start_next(tx);
                    }
                    return;
                }
                let Some(client) = self.client.clone() else {
                    self.handle_missing_client(QueuedRequest::Question {
                        question,
                        session_context,
                    });
                    return;
                };
                let repo = self.repo.clone();
                let root = self.root.clone();
                let (id, cancel) = self.begin_active("thinking");
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result =
                        answer_question(client, repo, root, question, session_context, &cancel)
                            .await;
                    let _ = tx.send(UiEvent::Answer { id, result });
                });
            }
            QueuedRequest::Sweep => {
                if let Some(session) = self.session.clone() {
                    let (id, _cancel) = self.begin_active("/sweep");
                    self.session_questions.insert(id);
                    if let Err(error) = session.send(session_server::ClientRequest::Sweep { id }) {
                        self.active = None;
                        self.transcript.push(TranscriptEntry::Failure([
                            "The sweep was not sent to the session server.".to_string(),
                            error.to_string(),
                            "Run estelle connect to reattach, then retry /sweep.".to_string(),
                        ]));
                        self.start_next(tx);
                    }
                    return;
                }
                let Some(client) = self.client.clone() else {
                    self.handle_missing_client(QueuedRequest::Sweep);
                    return;
                };
                let repo = self.repo.clone();
                let root = self.root.clone();
                let (id, cancel) = self.begin_active("/sweep");
                let tx = tx.clone();
                tokio::spawn(async move {
                    let progress_tx = tx.clone();
                    let result = top_level::sweep_with_progress(
                        &client,
                        &repo,
                        &root,
                        false,
                        &cancel,
                        |progress| {
                            let _ = progress_tx.send(UiEvent::SweepProgress { id, progress });
                            Ok(())
                        },
                    )
                    .await;
                    let _ = tx.send(UiEvent::SweepAnswer { id, result });
                });
            }
            QueuedRequest::Command(command) => {
                if let Some(session) = self.session.clone() {
                    let name = command.name;
                    let (id, _cancel) = self.begin_active(&format!("/{name}"));
                    self.session_questions.insert(id);
                    if let Err(error) = session.send(session_server::ClientRequest::Command {
                        id,
                        name: name.to_string(),
                        argument: command.argument,
                        last_question: command.last_question,
                        skill_thread: command.skill_thread,
                    }) {
                        self.active = None;
                        self.transcript.push(TranscriptEntry::Failure([
                            format!("/{name} was not sent to the session server."),
                            error.to_string(),
                            "Run estelle connect to reattach, then retry.".to_string(),
                        ]));
                        self.start_next(tx);
                    }
                    return;
                }
                let Some(client) = self.client.clone() else {
                    self.handle_missing_client(QueuedRequest::Command(command));
                    return;
                };
                let repo = self.repo.clone();
                let root = self.root.clone();
                let name = command.name;
                let (id, cancel) = self.begin_active(&format!("/{name}"));
                let tx = tx.clone();
                tokio::spawn(async move {
                    let result = execute_remote_command(client, repo, root, command, &cancel).await;
                    let _ = tx.send(UiEvent::CommandAnswer { id, name, result });
                });
            }
        }
    }

    fn begin_active(&mut self, label: &str) -> (u64, CancellationToken) {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        let cancel = CancellationToken::new();
        self.active = Some(ActiveRequest {
            id,
            label: label.to_string(),
            started: Instant::now(),
            cancel: cancel.clone(),
        });
        (id, cancel)
    }

    fn handle_missing_client(&mut self, pending: QueuedRequest) {
        let Some(client) = self.client.clone() else {
            if !self.auth_resolved {
                self.queue.push_front(pending);
                return;
            }
            self.transcript.push(TranscriptEntry::Failure([
                "The request was not sent.".to_string(),
                "This client has no Estelle credential.".to_string(),
                "Set ESTELLE_API_KEY or run /login, then retry.".to_string(),
            ]));
            return;
        };
        drop(client);
    }

    fn cancel_active(&mut self) {
        if let Some(active) = self.active.take() {
            if let Some(session) = &self.session {
                let _ = session.send(session_server::ClientRequest::Cancel { id: active.id });
            }
            active.cancel.cancel();
            self.transcript
                .push(TranscriptEntry::System("Request cancelled.".to_string()));
        }
    }

    fn handle_paste(&mut self, pasted: String) {
        if is_secret_shaped(&pasted) {
            self.hide_credential_input();
        } else {
            self.composer.handle_paste(pasted);
            self.inspect_composer_for_credential();
            self.record_dither_caret();
        }
    }

    fn inspect_composer_for_credential(&mut self) {
        if self.credential_input_hidden {
            if self.composer.is_empty() {
                self.credential_input_hidden = false;
            }
            return;
        }
        if is_secret_shaped(&self.composer.text()) {
            self.hide_credential_input();
        }
    }

    fn hide_credential_input(&mut self) {
        self.composer.set_text("[credential hidden]");
        self.credential_input_hidden = true;
    }

    fn submit_composer(&mut self, text: String, tx: &mpsc::UnboundedSender<UiEvent>) {
        if std::mem::take(&mut self.credential_input_hidden) {
            self.transcript
                .push(TranscriptEntry::User("[credential hidden]".to_string()));
            self.transcript.push(TranscriptEntry::System(
                "Credential-shaped input was masked and was not sent.".to_string(),
            ));
            self.composer = estelle_composer();
            return;
        }
        self.submit(text, tx);
    }

    fn push_answer_reply(&mut self, response: AnswerReply) {
        if !response.text.trim().is_empty() {
            self.citations = response.sources.clone();
            self.working_memory_paths = response.working_paths;
            self.transcript.push(TranscriptEntry::Answer {
                text: response.text,
                grounded: response.grounded,
                degraded: response.degraded,
                sources: response.sources,
            });
        } else {
            self.transcript.push(TranscriptEntry::Failure([
                "Estelle returned no answer.".to_string(),
                "The server completed the request with an empty result.".to_string(),
                "Retry with a narrower question.".to_string(),
            ]));
        }
    }

    fn record_session_input(&mut self, id: u64, input: session_server::SessionInput) {
        if self.session_questions.insert(id) {
            let text = match input {
                session_server::SessionInput::Question { question } => {
                    self.last_question = Some(question.clone());
                    self.has_submitted_question = true;
                    question
                }
                session_server::SessionInput::Command { name, argument } => {
                    format!("/{name} {argument}").trim_end().to_string()
                }
                session_server::SessionInput::Sweep => "/sweep".to_string(),
            };
            self.transcript.push(TranscriptEntry::User(text));
        }
    }

    fn record_session_turn(&mut self, turn: session_server::SessionTurn) {
        let command_name = match &turn.input {
            session_server::SessionInput::Command { name, .. } => {
                commands::resolve_session_name(name)
            }
            _ => None,
        };
        self.record_session_input(turn.id, turn.input);
        if !self.session_completed.insert(turn.id) {
            return;
        }
        match turn.outcome {
            session_server::SessionOutcome::Answer { answer } => {
                self.push_answer_reply(answer.into());
            }
            session_server::SessionOutcome::Command { reply } => {
                if let Some(name) = command_name {
                    self.apply_command_success(name, *reply);
                } else {
                    self.transcript.push(TranscriptEntry::Failure([
                        "The server returned an unknown command result.".to_string(),
                        "The stored command name is not in this client's command inventory."
                            .to_string(),
                        "Upgrade the client and reconnect to replay this session.".to_string(),
                    ]));
                }
            }
            session_server::SessionOutcome::Sweep { lines } => {
                self.transcript.push(TranscriptEntry::Command {
                    name: "sweep".to_string(),
                    lines,
                });
            }
            session_server::SessionOutcome::Failure { lines } => {
                self.transcript.push(TranscriptEntry::Failure(lines));
            }
        }
    }

    fn record_file_shift(&mut self, notice: session_server::FileShiftNotice) {
        if !self.session_file_shifts.insert(notice.id) {
            return;
        }
        let summary = notice
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .map(|summary| format!(" · {summary}"))
            .unwrap_or_default();
        self.transcript.push(TranscriptEntry::System(format!(
            "FILE SHIFT · {}\n{} changed a file this session read{}\nInspect the diff before continuing.",
            notice.path.display(),
            notice.changed_by,
            summary,
        )));
    }

    fn apply_command_success(&mut self, name: &'static str, result: RemoteCommandReply) {
        if name == "gate" {
            self.gate_modal = GateModal::from_reply(&result.reply, &result.inspected_files);
        }
        let reply = result.reply;
        if name == "orchestra" && reply.fleet.is_some() {
            self.fleet = reply.fleet.clone();
        }
        if name == "model" {
            self.picker = Some(PickerSurface::model(&reply));
        } else if name == "skills" {
            self.picker = Some(PickerSurface::skills(&reply));
        }
        if matches!(name, "model" | "routing")
            && let Some(model) = observed_model(&reply)
        {
            self.active_model = Some(model.to_string());
            self.active_model_observed_at = Some(Instant::now());
        }
        if let Some(todo) = reply.todo.clone() {
            self.todo = Some(todo);
            self.todo_visible = true;
        }
        if name == "work" {
            self.last_diff = reply
                .diff
                .as_deref()
                .filter(|diff| !diff.trim().is_empty())
                .map(str::to_string);
        }
        if name == "skill:"
            && let Some((skill, task)) = self.pending_skill.take()
        {
            let thread = self.skill_threads.entry(skill).or_default();
            if !task.trim().is_empty() {
                thread.push(("user".to_string(), task));
            }
            if let Some(reply_text) = reply
                .extra
                .get("reply")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                thread.push(("assistant".to_string(), reply_text.to_string()));
            }
        }
        self.transcript.push(TranscriptEntry::Command {
            name: name.to_string(),
            lines: commands::render_remote_reply(name, &reply),
        });
    }

    fn handle_session_message(
        &mut self,
        message: session_server::ServerMessage,
        tx: &mpsc::UnboundedSender<UiEvent>,
    ) {
        match message {
            session_server::ServerMessage::Snapshot {
                session_id,
                sessions,
                turns,
                active,
                file_shifts,
                fleet,
            } => {
                if self.session_id != session_id {
                    self.clear_session_surface();
                }
                self.session_id = session_id;
                self.session_tabs = sessions;
                self.hidden_session_tabs.remove(&self.session_id);
                for turn in turns {
                    self.record_session_turn(turn);
                }
                self.fleet = fleet.and_then(|fleet| serde_json::from_str(&fleet).ok());
                let file_shifts_through = file_shifts.last().map(|notice| notice.id);
                for notice in file_shifts {
                    self.record_file_shift(notice);
                }
                if let (Some(session), Some(through)) = (&self.session, file_shifts_through) {
                    let _ = session
                        .send(session_server::ClientRequest::AcknowledgeFileShifts { through });
                }
                if let Some(active) = active {
                    let label = match &active.input {
                        session_server::SessionInput::Question { .. } => "thinking".to_string(),
                        session_server::SessionInput::Command { name, .. } => format!("/{name}"),
                        session_server::SessionInput::Sweep => "/sweep".to_string(),
                    };
                    self.record_session_input(active.id, active.input);
                    self.active = Some(ActiveRequest {
                        id: active.id,
                        label,
                        started: Instant::now(),
                        cancel: CancellationToken::new(),
                    });
                }
            }
            session_server::ServerMessage::Started { active } => {
                if let Some(tab) = self
                    .session_tabs
                    .iter_mut()
                    .find(|tab| tab.id == self.session_id)
                {
                    tab.active = true;
                }
                let label = match &active.input {
                    session_server::SessionInput::Question { .. } => "thinking".to_string(),
                    session_server::SessionInput::Command { name, .. } => format!("/{name}"),
                    session_server::SessionInput::Sweep => "/sweep".to_string(),
                };
                self.record_session_input(active.id, active.input);
                self.active = Some(ActiveRequest {
                    id: active.id,
                    label,
                    started: Instant::now(),
                    cancel: CancellationToken::new(),
                });
            }
            session_server::ServerMessage::Completed { turn } => {
                if let Some(tab) = self
                    .session_tabs
                    .iter_mut()
                    .find(|tab| tab.id == self.session_id)
                {
                    tab.active = false;
                    tab.turn_count = tab.turn_count.saturating_add(1);
                }
                let was_current = self
                    .active
                    .as_ref()
                    .is_some_and(|active| active.id == turn.id);
                if was_current {
                    self.active = None;
                }
                self.record_session_turn(turn);
                if was_current {
                    self.start_next(tx);
                }
            }
            session_server::ServerMessage::Cancelled { id } => {
                if let Some(tab) = self
                    .session_tabs
                    .iter_mut()
                    .find(|tab| tab.id == self.session_id)
                {
                    tab.active = false;
                }
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    self.active = None;
                }
            }
            session_server::ServerMessage::SweepProgress { id, progress } => {
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    self.sweep_progress = Some(progress);
                }
            }
            session_server::ServerMessage::Fleet { fleet } => {
                if self.fleet.as_ref().is_none_or(|current| {
                    current.id != fleet.id || fleet.revision > current.revision
                }) {
                    self.fleet = Some(fleet);
                }
            }
            session_server::ServerMessage::Rejected { id, message } => {
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    self.active = None;
                }
                self.transcript.push(TranscriptEntry::Failure([
                    "The session server rejected the request.".to_string(),
                    message,
                    "Wait for the active session work or reconnect, then retry.".to_string(),
                ]));
                self.start_next(tx);
            }
            session_server::ServerMessage::FileShift { notice } => {
                let through = notice.id;
                self.record_file_shift(notice);
                if let Some(session) = &self.session {
                    let _ = session
                        .send(session_server::ClientRequest::AcknowledgeFileShifts { through });
                }
            }
            session_server::ServerMessage::FileActivityRecorded { .. } => {}
            session_server::ServerMessage::FileActivityRejected { message } => {
                self.transcript.push(TranscriptEntry::Failure([
                    "The session server rejected file activity.".to_string(),
                    message,
                    "Use a repository-relative path and retry the tool.".to_string(),
                ]));
            }
        }
    }

    fn clear_session_surface(&mut self) {
        self.transcript.clear();
        self.queue.clear();
        self.active = None;
        self.session_questions.clear();
        self.session_completed.clear();
        self.session_file_shifts.clear();
        self.skill_threads.clear();
        self.pending_skill = None;
        self.last_question = None;
        self.last_diff = None;
        self.citations.clear();
        self.sweep_progress = None;
        self.fleet = None;
        self.todo = None;
        self.transcript_scroll = 0;
    }

    fn visible_session_ids(&self) -> Vec<String> {
        self.session_tabs
            .iter()
            .filter(|session| !self.hidden_session_tabs.contains(&session.id))
            .map(|session| session.id.clone())
            .collect()
    }

    fn switch_to_session(&mut self, session_id: String) {
        if session_id == self.session_id {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        match session.switch(session_id.clone()) {
            Ok(()) => {
                self.clear_session_surface();
                self.session_id = session_id;
            }
            Err(error) => self.transcript.push(TranscriptEntry::Failure([
                "The terminal could not switch sessions.".to_string(),
                error.to_string(),
                "Reconnect to the session server and try again.".to_string(),
            ])),
        }
    }

    fn cycle_session(&mut self, reverse: bool) {
        let ids = self.visible_session_ids();
        if ids.len() < 2 {
            return;
        }
        let current = ids
            .iter()
            .position(|id| id == &self.session_id)
            .unwrap_or(0);
        let next = if reverse {
            current.checked_sub(1).unwrap_or(ids.len() - 1)
        } else {
            (current + 1) % ids.len()
        };
        self.switch_to_session(ids[next].clone());
    }

    fn close_session_tab(&mut self) {
        let ids = self.visible_session_ids();
        if ids.len() <= 1 {
            self.should_exit = true;
            return;
        }
        let current = ids
            .iter()
            .position(|id| id == &self.session_id)
            .unwrap_or(0);
        self.hidden_session_tabs.insert(self.session_id.clone());
        self.switch_to_session(ids[(current + 1) % ids.len()].clone());
    }

    fn handle_ui_event(&mut self, event: UiEvent, tx: &mpsc::UnboundedSender<UiEvent>) {
        match event {
            UiEvent::SessionContext(context) => {
                self.session_context = (!context.is_empty()).then_some(context);
            }
            UiEvent::Credential(result) => {
                self.auth_resolved = true;
                match result {
                    Ok((client, auth)) => {
                        self.login_required = false;
                        self.client = Some(client.clone());
                        self.auth = Some(auth);
                        spawn_header_requests(Some(client), &self.repo, tx);
                        self.poll_production_if_due(tx);
                    }
                    Err(Error::NoCredential) => {
                        self.login_required = true;
                        self.picker = Some(PickerSurface::login());
                    }
                    Err(error) => self
                        .transcript
                        .push(TranscriptEntry::Failure(failure_lines(&error))),
                }
                self.start_next(tx);
            }
            UiEvent::Account(result) => match result {
                Ok(account) => {
                    self.header.connected = true;
                    self.header.plan = account.plan.clone();
                    self.account = Some(account);
                }
                Err(error) => self.handle_background_error(&error),
            },
            UiEvent::Overview(result) => match result {
                Ok(overview) => self.apply_overview(overview.memory),
                Err(error) => self.handle_background_error(&error),
            },
            UiEvent::Repos(result) => match result {
                Ok(repos) => {
                    self.header.indexed = Some(repo_is_listed(&self.repo, &repos.repos));
                }
                Err(error) => self.handle_background_error(&error),
            },
            UiEvent::Scope(result) => match result {
                Ok(scope) => {
                    if let Some(global) = scope.extra.get("global").and_then(Value::as_str)
                        && commands::mode_rank(global).is_some()
                    {
                        self.server_mode = Some(global.to_string());
                        self.local_mode = global.to_string();
                    }
                }
                Err(error) => self.handle_background_error(&error),
            },
            UiEvent::Settings(result) => match result {
                Ok(settings) => {
                    let personal_theme = settings
                        .extra
                        .get("personal")
                        .and_then(|personal| personal.get("global"))
                        .and_then(|global| global.get("theme"))
                        .and_then(Value::as_str);
                    match personal_theme {
                        Some("light") => self.theme = Theme::CreamInk,
                        Some("dark") => self.theme = Theme::Dark,
                        Some("system") | None => {}
                        Some(_) => self.transcript.push(TranscriptEntry::Failure([
                            "Personal theme has an unknown value.".to_string(),
                            "The renderer kept its current theme rather than guessing.".to_string(),
                            "Open /settings after the server setting is corrected.".to_string(),
                        ])),
                    }
                    self.settings = Some(settings);
                }
                Err(error) => self.handle_background_error(&error),
            },
            UiEvent::SettingSaved { suite, key, result } => match result {
                Ok(reply) => {
                    let scope = reply
                        .extra
                        .get("scope")
                        .and_then(Value::as_str)
                        .unwrap_or("team");
                    let value = reply.extra.get("value").cloned().unwrap_or(Value::Null);
                    if let Some(settings) = self.settings.as_mut() {
                        let owner = settings
                            .extra
                            .entry(scope.to_string())
                            .or_insert_with(|| Value::Object(Default::default()));
                        if let Some(owner) = owner.as_object_mut() {
                            let suite_values = owner
                                .entry(suite.clone())
                                .or_insert_with(|| Value::Object(Default::default()));
                            if let Some(suite_values) = suite_values.as_object_mut() {
                                suite_values.insert(key.clone(), value.clone());
                            }
                        }
                    }
                    self.transcript.push(TranscriptEntry::System(format!(
                        "Saved {suite}.{key} = {} · {scope} setting.",
                        display_setting_value(&value)
                    )));
                    self.picker = Some(PickerSurface::suite(self, &suite));
                }
                Err(error) => self.transcript.push(TranscriptEntry::Failure([
                    format!("The server refused {suite}.{key}."),
                    error.to_string(),
                    "No local fallback was written.".to_string(),
                ])),
            },
            UiEvent::AutonomyChanged(result) => match result {
                Ok(reply) => {
                    if let Some(level) = reply.extra.get("autonomy").and_then(Value::as_str)
                        && commands::mode_rank(level).is_some()
                    {
                        self.server_mode = Some(level.to_string());
                        self.local_mode = level.to_string();
                        self.transcript.push(TranscriptEntry::System(format!(
                            "Account autonomy is now {} · server enforced.",
                            commands::mode_name(level)
                        )));
                    } else {
                        self.transcript.push(TranscriptEntry::Failure([
                            "Autonomy response omitted the account ceiling.".to_string(),
                            "No local mode was inferred from the response.".to_string(),
                            "Reopen /settings to read the server state.".to_string(),
                        ]));
                    }
                }
                Err(error) => self
                    .transcript
                    .push(TranscriptEntry::Failure(failure_lines(&error))),
            },
            UiEvent::ThemeSaved { theme, result } => match result {
                Ok(_) => self.transcript.push(TranscriptEntry::System(format!(
                    "{} saved to personal settings.",
                    theme.name()
                ))),
                Err(error) => {
                    self.transcript
                        .push(TranscriptEntry::Failure(failure_lines(&error)));
                    self.transcript.push(TranscriptEntry::System(format!(
                        "{} remains active for this session only.",
                        theme.name()
                    )));
                }
            },
            UiEvent::ProviderSelected {
                provider,
                model,
                result,
            } => match result {
                Ok(reply) => {
                    let selected_provider = provider;
                    let selected_model = reply
                        .extra
                        .get("provider_model")
                        .and_then(Value::as_str)
                        .unwrap_or(&model)
                        .to_string();
                    self.transcript.push(TranscriptEntry::System(format!(
                        "{selected_provider} · {selected_model} is now the account-wide provider default. Auto routing remains active; this does not claim which model served a request."
                    )));
                }
                Err(error) => self
                    .transcript
                    .push(TranscriptEntry::Failure(failure_lines(&error))),
            },
            UiEvent::ProdIssues(result) => {
                self.prod_issue_in_flight = false;
                let mut continue_page = false;
                match result {
                    Ok(response) => {
                        continue_page = response.has_more;
                        self.prod_issue_since = response.next_since.or(self.prod_issue_since);
                        merge_issue_page(&mut self.prod_issues, response);
                        self.prod_issue_error = None;
                        self.prod_issue_failures = 0;
                    }
                    Err(error) => {
                        self.prod_issue_error = Some(production_error_message(&error));
                        self.prod_issue_failures = self.prod_issue_failures.saturating_add(1);
                    }
                }
                if self.prod_panel_visible {
                    self.prod_issue_next_poll = if continue_page {
                        Some(Instant::now())
                    } else {
                        let inactive = self.production_is_inactive();
                        Some(
                            Instant::now()
                                + production_poll_delay(
                                    Duration::from_secs(30),
                                    self.prod_issue_failures,
                                    inactive,
                                ),
                        )
                    };
                }
            }
            UiEvent::ProdOverview(result) => {
                self.prod_overview_in_flight = false;
                match result {
                    Ok(response) => {
                        self.prod_overview = Some(response);
                        self.prod_overview_error = None;
                        self.prod_overview_failures = 0;
                    }
                    Err(error) => {
                        self.prod_overview_error = Some(production_error_message(&error));
                        self.prod_overview_failures = self.prod_overview_failures.saturating_add(1);
                    }
                }
                if self.prod_panel_visible {
                    let inactive = self.production_is_inactive();
                    self.prod_overview_next_poll = Some(
                        Instant::now()
                            + production_poll_delay(
                                Duration::from_secs(60),
                                self.prod_overview_failures,
                                inactive,
                            ),
                    );
                }
            }
            UiEvent::ProdAgentHealth(result) => {
                self.prod_agent_health_in_flight = false;
                match result {
                    Ok(response) => {
                        self.prod_agent_health = Some(response);
                        self.prod_agent_health_error = None;
                        self.prod_agent_health_failures = 0;
                    }
                    Err(error) => {
                        self.prod_agent_health_error =
                            Some(format!("agent health unavailable · {error}"));
                        self.prod_agent_health_failures =
                            self.prod_agent_health_failures.saturating_add(1);
                    }
                }
                if self.prod_panel_visible {
                    let inactive = self.production_is_inactive();
                    self.prod_agent_health_next_poll = Some(
                        Instant::now()
                            + production_poll_delay(
                                Duration::from_secs(30),
                                self.prod_agent_health_failures,
                                inactive,
                            ),
                    );
                }
            }
            UiEvent::ProdGithub {
                status,
                proposed_prs,
            } => {
                self.prod_github_in_flight = false;
                let mut failed = false;
                match status {
                    Ok(response) => {
                        self.prod_github_status = Some(response);
                        self.prod_github_status_error = None;
                    }
                    Err(error) => {
                        failed = true;
                        self.prod_github_status_error =
                            Some(format!("GitHub connection unavailable · {error}"));
                    }
                }
                match proposed_prs {
                    Ok(response) => {
                        self.prod_proposed_prs = Some(response);
                        self.prod_proposed_prs_error = None;
                    }
                    Err(error) => {
                        failed = true;
                        self.prod_proposed_prs_error =
                            Some(format!("Proposed PR feed unavailable · {error}"));
                    }
                }
                self.prod_github_failures = if failed {
                    self.prod_github_failures.saturating_add(1)
                } else {
                    0
                };
                if self.prod_panel_visible {
                    let inactive = self.production_is_inactive();
                    self.prod_github_next_poll = Some(
                        Instant::now()
                            + production_poll_delay(
                                Duration::from_secs(60),
                                self.prod_github_failures,
                                inactive,
                            ),
                    );
                }
            }
            UiEvent::Answer { id, result } => {
                if self.active.as_ref().is_none_or(|active| active.id != id) {
                    return;
                }
                self.active = None;
                match result {
                    Ok(response) => self.push_answer_reply(response),
                    Err(Error::Cancelled) => {}
                    Err(error) => {
                        self.clear_rejected(&error, "/deep-search");
                        self.transcript
                            .push(TranscriptEntry::Failure(failure_lines(&error)));
                    }
                }
                self.start_next(tx);
            }
            UiEvent::CommandAnswer { id, name, result } => {
                if self.active.as_ref().is_none_or(|active| active.id != id) {
                    return;
                }
                self.active = None;
                match result {
                    Ok(result) => self.apply_command_success(name, result),
                    Err(CommandFailure::Client(Error::Cancelled)) => {
                        self.pending_skill = None;
                    }
                    Err(CommandFailure::Client(error)) => {
                        self.pending_skill = None;
                        self.clear_rejected(&error, name);
                        self.transcript
                            .push(TranscriptEntry::Failure(failure_lines(&error)));
                    }
                    Err(CommandFailure::Local(lines)) => {
                        self.transcript.push(TranscriptEntry::Failure(lines));
                    }
                }
                self.start_next(tx);
            }
            UiEvent::LocalAnswer { id, name, result } => {
                if self.active.as_ref().is_none_or(|active| active.id != id) {
                    return;
                }
                self.active = None;
                match result {
                    Ok(lines) => {
                        if name == "apply" {
                            self.last_applied_diff = self.last_diff.clone();
                        } else if name == "undo" {
                            self.last_applied_diff = None;
                        }
                        self.transcript.push(TranscriptEntry::Command {
                            name: name.to_string(),
                            lines,
                        });
                    }
                    Err(error) if error == "cancelled" => {}
                    Err(error) => self.transcript.push(TranscriptEntry::Failure([
                        format!("Local {name} failed: {error}"),
                        "The failure occurred in the local working tree.".to_string(),
                        "Correct the local state, then retry.".to_string(),
                    ])),
                }
                self.start_next(tx);
            }
            UiEvent::SweepProgress { id, progress } => {
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    self.sweep_progress = Some(progress);
                }
            }
            UiEvent::SweepAnswer { id, result } => {
                if self.active.as_ref().is_none_or(|active| active.id != id) {
                    return;
                }
                self.active = None;
                match result {
                    Ok(lines) => self.transcript.push(TranscriptEntry::Command {
                        name: "sweep".to_string(),
                        lines,
                    }),
                    Err(top_level::SweepFailure::Client(Error::Cancelled)) => {}
                    Err(top_level::SweepFailure::Client(error)) => {
                        self.clear_rejected(&error, "sweep");
                        self.transcript
                            .push(TranscriptEntry::Failure(failure_lines(&error)));
                    }
                    Err(top_level::SweepFailure::Local(error)) => {
                        self.transcript.push(TranscriptEntry::Failure([
                            format!("Sweep stopped: {error}"),
                            "The repository was not reported as fully swept.".to_string(),
                            "Correct the local or account state, then retry /sweep.".to_string(),
                        ]));
                    }
                }
                self.start_next(tx);
            }
            UiEvent::Session(message) => self.handle_session_message(message, tx),
            UiEvent::SessionDisconnected(error) => {
                self.session = None;
                self.active = None;
                self.transcript.push(TranscriptEntry::Failure([
                    "This terminal detached from the session server.".to_string(),
                    error,
                    "Server-owned work was not cancelled; run estelle connect to reattach."
                        .to_string(),
                ]));
            }
        }
    }

    fn apply_overview(&mut self, memory: Option<MemoryOverview>) {
        let Some(memory) = memory else {
            return;
        };
        self.header.files = memory.repo_files;
        self.header.memories = memory.memories;
        let local = self.repo.as_str();
        let short = local.rsplit('/').next().unwrap_or(local);
        if let Some(row) = memory
            .by_repo
            .iter()
            .find(|row| row.repo == local || row.repo == short)
        {
            self.header.files = row.files;
            self.header.chunks = row.chunks;
        }
    }

    fn handle_background_error(&mut self, error: &Error) {
        self.clear_rejected(error, "a background poll");
    }

    /// A single rejection NEVER deletes the credential: one route's 401/403/404 is route scope,
    /// not proof of a bad key (measured on prod: login verified and a question succeeded on the
    /// SAME credential that one /me 401 then wiped). The rejection is recorded against its route
    /// and reported with the credential KEPT. Only repeated rejections across DIFFERENT routes
    /// justify deletion — and the transcript says which routes before the credential is removed.
    fn clear_rejected(&mut self, error: &Error, route: &str) {
        if !error.is_explicit_auth_rejection() {
            return;
        }
        let Some(auth) = &self.auth else {
            return;
        };
        if !matches!(
            auth.source,
            CredentialSource::Stored | CredentialSource::SecureStore
        ) {
            return;
        }
        self.rejected_routes.insert(route.to_string());
        if self.rejected_routes.len() < 2 {
            self.transcript.push(TranscriptEntry::System(format!(
                "The stored credential was rejected on {route}. It was NOT removed — a single rejection can be route scope, not a bad key. Run /login only if you revoked it."
            )));
            return;
        }
        let routes = self
            .rejected_routes
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if auth.store.delete_stored(auth.source).unwrap_or(false) {
            self.client = None;
            self.header.connected = false;
            self.transcript.push(TranscriptEntry::System(format!(
                "The stored credential was rejected on {routes} — different routes, so it was removed. Run /login to store a fresh key."
            )));
        }
    }
}

async fn answer_question(
    client: Client,
    repo: Repo,
    root: PathBuf,
    question: String,
    session_context: Option<String>,
    cancel: &CancellationToken,
) -> Result<AnswerReply, Error> {
    // ONE model round-trip per question, always through /deep-search: the server owns the
    // conversational fast path (`utterance.is_conversational`), the scope decision and the
    // grounding certificate. `AnswerReply.text` is what a human reads — retrieval context is
    // model INPUT, never assistant output — so the transcript carries the rendered answer only
    // and provenance is disclosed from the typed `working_paths` field (see /context).
    //
    // THE RULE: the client sends DATA; the client never authors INSTRUCTIONS. `question` is the
    // user's message verbatim — client prose prepended to it defeats the server's
    // `is_conversational` fast path on LENGTH before vocabulary is even read (measured on prod:
    // the wrapper turned "hi" into a 15,639-citation grounded pipeline run). Working memory
    // rides a separate top-level `working_memory` key, which the server ignores until the typed
    // contract (register 14b) ships. The wording of answers, disclosure, and whether the repo
    // graph is consulted are the server's job (Guardian), never the client's prompt.
    let working_files = tokio::task::spawn_blocking(move || top_level::working_memory_files(&root))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    // The conversational gate decides BANDWIDTH, not a verdict: without it, "hi" in a dirty
    // repo would upload up to 80 KB of working memory the fast path does not need. This is
    // deliberately NOT a shared copy of the server's rule: that check decides whether retrieval
    // and the grounding gate run, where a mistake is a wrong answer; this one only decides
    // whether local files are attached, where both failure directions are safe — attach
    // needlessly and one upload is wasted, skip the attachment and the server still answers
    // from the repo graph, degraded but never wrong. Do not "fix" drift between the two
    // vocabularies by importing a rule that does not exist.
    let attach_context = !is_conversational_turn(&question)
        && (!working_files.is_empty() || session_context.is_some());
    let (request, working_paths) = if attach_context {
        let (payload, paths) = working_memory_payload(&working_files, session_context.as_deref());
        (
            DeepSearchRequest::new(&question).with_working_memory(payload),
            paths,
        )
    } else {
        (DeepSearchRequest::new(&question), Vec::new())
    };
    let response = client.deep_search(&repo, &request, cancel).await?;
    Ok(AnswerReply {
        text: response.rendered_answer().unwrap_or_default().to_string(),
        grounded: response.grounded,
        degraded: response.degraded,
        sources: response.sources,
        working_paths,
    })
}

/// Crude client-side BANDWIDTH gate — see `answer_question` for why this is not a verdict and
/// must not converge with the server's `is_conversational`. True only when the message is at
/// most eight tokens and every token is in a small closed social vocabulary; anything else,
/// including any digit, attaches working memory (the safe direction).
fn is_conversational_turn(message: &str) -> bool {
    const MAX_SOCIAL_TOKENS: usize = 8;
    const SOCIAL_TOKENS: &[&str] = &[
        "hi",
        "hii",
        "hey",
        "heya",
        "hello",
        "yo",
        "gm",
        "morning",
        "afternoon",
        "evening",
        "good",
        "thanks",
        "thank",
        "thx",
        "ty",
        "tysm",
        "cheers",
        "appreciated",
        "appreciate",
        "much",
        "ok",
        "okay",
        "k",
        "kk",
        "cool",
        "nice",
        "great",
        "perfect",
        "awesome",
        "excellent",
        "lovely",
        "got",
        "it",
        "makes",
        "sense",
        "understood",
        "gotcha",
        "right",
        "sweet",
        "brilliant",
        "helpful",
        "that",
        "thats",
        "very",
        "yes",
        "yep",
        "yeah",
        "yup",
        "no",
        "nope",
        "nah",
        "sure",
        "please",
        "bye",
        "goodbye",
        "see",
        "you",
        "later",
        "cya",
        "night",
        "sorry",
        "apologies",
        "my",
        "bad",
        "oops",
        "i",
        "im",
        "am",
        "is",
        "was",
        "and",
        "a",
        "the",
        "to",
    ];
    if message.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let stripped = message.replace(['\'', '\u{2019}'], "");
    let tokens = stripped
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    !tokens.is_empty()
        && tokens.len() <= MAX_SOCIAL_TOKENS
        && tokens
            .iter()
            .all(|token| SOCIAL_TOKENS.contains(&token.as_str()))
}

/// The working-memory payload as DATA — bounded paths + contents + optional session context.
/// No instruction prose lives here: the client sends data, the server owns the prompt.
fn working_memory_payload(
    files: &[top_level::WorkingMemoryFile],
    session_context: Option<&str>,
) -> (Value, Vec<String>) {
    const MAX_FILES: usize = 8;
    const MAX_CHARS: usize = 80_000;
    let mut remaining = MAX_CHARS;
    let mut paths = Vec::new();
    let mut body = Vec::new();
    for file in files.iter().take(MAX_FILES) {
        if remaining == 0 {
            break;
        }
        let content = file.content.chars().take(remaining).collect::<String>();
        remaining = remaining.saturating_sub(content.chars().count());
        paths.push(file.path.clone());
        body.push(serde_json::json!({"path": file.path, "content": content}));
    }
    let mut payload = serde_json::json!({"files": body});
    if let Some(context) = session_context {
        payload["session_context"] = Value::String(context.to_string());
    }
    (payload, paths)
}

async fn execute_remote_command(
    client: Client,
    repo: Repo,
    root: PathBuf,
    pending: PendingCommand,
    cancel: &CancellationToken,
) -> Result<RemoteCommandReply, CommandFailure> {
    let measured_diff = if matches!(pending.name, "gate" | "scan" | "review") {
        Some(git_diff(&root, &pending.argument, cancel).await?)
    } else {
        None
    };
    let diff = measured_diff
        .as_ref()
        .map(|measured| measured.patch.as_str());
    let mut request = commands::remote_request(
        pending.name,
        &pending.argument,
        diff,
        pending.last_question.as_deref(),
    )
    .map_err(|commands::RouteError::MissingDiff| {
        CommandFailure::Local([
            format!("/{} found no diff to inspect.", pending.name),
            "The local working tree and selected comparison are unchanged.".to_string(),
            "Make a change or pass a base revision, then retry.".to_string(),
        ])
    })?
    .ok_or_else(|| {
        CommandFailure::Local([
            format!("/{} has no remote route.", pending.name),
            "The command inventory and transport table disagree.".to_string(),
            "Report this command name; nothing was sent.".to_string(),
        ])
    })?;
    // /scan's whole-lockfile CVE pass needs the full file the diff only excerpts — attach the
    // lockfiles the measured diff touches (api_intel.py handle_scan reads "files").
    if pending.name == "scan"
        && let (Some(body), Some(diff_text)) = (request.body.as_mut(), diff)
    {
        let attachments = commands::scan_lockfile_attachments(&root, diff_text);
        if !attachments.is_empty() {
            body["files"] = Value::Array(attachments);
        }
    }
    // An interactive skill continues over "messages" (skill_run.py conversation_messages):
    // the prior thread plus the new turn. Without it every follow-up restarts single-turn.
    if pending.name == "skill:"
        && let Some(thread) = pending
            .skill_thread
            .as_ref()
            .filter(|thread| !thread.is_empty())
        && let Some(body) = request.body.as_mut()
    {
        let mut messages = thread
            .iter()
            .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
            .collect::<Vec<_>>();
        let task = pending
            .argument
            .split_once(char::is_whitespace)
            .map(|(_, task)| task)
            .unwrap_or_default()
            .trim();
        if !task.is_empty() {
            messages.push(serde_json::json!({"role": "user", "content": task}));
        }
        body["messages"] = Value::Array(messages);
    }

    let result = match (request.method, request.endpoint.requires_repo()) {
        (commands::RemoteMethod::Get, true) => {
            client
                .get_scoped(request.endpoint, &repo, &request.query, cancel)
                .await
        }
        (commands::RemoteMethod::Get, false) => {
            client.get(request.endpoint, &request.query, cancel).await
        }
        (commands::RemoteMethod::Post, true) => {
            let body = request.body.as_ref().ok_or_else(|| {
                CommandFailure::Local([
                    format!("/{} has no request body.", request.name),
                    "The command inventory and transport table disagree.".to_string(),
                    "Report this command name; nothing was sent.".to_string(),
                ])
            })?;
            client
                .post_scoped(request.endpoint, &repo, body, cancel)
                .await
        }
        (commands::RemoteMethod::Post, false) => {
            let body = request.body.as_ref().ok_or_else(|| {
                CommandFailure::Local([
                    format!("/{} has no request body.", request.name),
                    "The command inventory and transport table disagree.".to_string(),
                    "Report this command name; nothing was sent.".to_string(),
                ])
            })?;
            client.post(request.endpoint, body, cancel).await
        }
    };
    let reply = result.map_err(CommandFailure::Client)?;
    Ok(RemoteCommandReply {
        reply,
        inspected_files: measured_diff
            .map(|measured| measured.files)
            .unwrap_or_default(),
    })
}

struct MeasuredDiff {
    patch: String,
    files: Vec<DiffFileStat>,
}

async fn git_diff(
    root: &std::path::Path,
    base: &str,
    cancel: &CancellationToken,
) -> Result<MeasuredDiff, CommandFailure> {
    let mut command = TokioCommand::new("git");
    command.current_dir(root).arg("diff").arg("--no-color");
    if !base.trim().is_empty() {
        command.arg(format!("{}...HEAD", base.trim()));
    }
    let output = cancellable_output(command, cancel)
        .await
        .map_err(local_git_failure)?;
    if !output.status.success() {
        return Err(local_git_failure(output_text(&output.stderr)));
    }
    let patch = String::from_utf8_lossy(&output.stdout).into_owned();

    let mut stat_command = TokioCommand::new("git");
    stat_command
        .current_dir(root)
        .arg("diff")
        .arg("--numstat")
        .arg("-z");
    if !base.trim().is_empty() {
        stat_command.arg(format!("{}...HEAD", base.trim()));
    }
    let stat_output = cancellable_output(stat_command, cancel)
        .await
        .map_err(local_git_failure)?;
    if !stat_output.status.success() {
        return Err(local_git_failure(output_text(&stat_output.stderr)));
    }
    Ok(MeasuredDiff {
        patch,
        files: parse_git_numstat(&stat_output.stdout),
    })
}

fn parse_git_numstat(output: &[u8]) -> Vec<DiffFileStat> {
    let fields = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut index = 0;
    let mut files = Vec::new();
    while index < fields.len() {
        let record = fields[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        let mut columns = record.splitn(3, |byte| *byte == b'\t');
        let added = columns.next().unwrap_or_default();
        let deleted = columns.next().unwrap_or_default();
        let mut path = columns.next().unwrap_or_default();
        if path.is_empty() {
            // With -z, rename records carry old and new paths as separate NUL fields.
            index = index.saturating_add(1);
            path = fields.get(index).copied().unwrap_or_default();
            index = index.saturating_add(1);
        }
        let parse_count = |value: &[u8]| {
            std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
        };
        files.push(DiffFileStat {
            path: String::from_utf8_lossy(path).into_owned(),
            changed_lines: parse_count(added).saturating_add(parse_count(deleted)),
        });
    }
    files
}

async fn execute_shell(
    root: &std::path::Path,
    source: &str,
    cancel: &CancellationToken,
) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = TokioCommand::new("cmd");
        command.arg("/C").arg(source);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = TokioCommand::new("/bin/sh");
        command.arg("-lc").arg(source);
        command
    };
    command.current_dir(root);
    let output = cancellable_output(command, cancel).await?;
    let mut lines = output_lines(&output.stdout, &output.stderr);
    if !output.status.success() {
        return Err(nonblank_output(
            &lines,
            &format!("process exited with {}", output.status),
        ));
    }
    if lines.is_empty() {
        lines.push("Command completed with no output.".to_string());
    }
    Ok(lines)
}

async fn apply_diff(
    root: &std::path::Path,
    diff: &str,
    reverse: bool,
    cancel: &CancellationToken,
) -> Result<Vec<String>, String> {
    let mut command = TokioCommand::new("git");
    command
        .current_dir(root)
        .arg("apply")
        .arg("--whitespace=nowarn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if reverse {
        command.arg("--reverse");
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "git apply stdin was unavailable".to_string())?;
    stdin
        .write_all(diff.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    drop(stdin);
    let output = tokio::select! {
        () = cancel.cancelled() => return Err("cancelled".to_string()),
        output = child.wait_with_output() => output.map_err(|error| error.to_string())?,
    };
    if !output.status.success() {
        let lines = output_lines(&output.stdout, &output.stderr);
        return Err(nonblank_output(
            &lines,
            &format!("git apply exited with {}", output.status),
        ));
    }
    Ok(vec![if reverse {
        "Reversed the last Estelle-applied diff.".to_string()
    } else {
        "Applied the proposed diff to the working tree.".to_string()
    }])
}

async fn cancellable_output(
    mut command: TokioCommand,
    cancel: &CancellationToken,
) -> Result<std::process::Output, String> {
    command.kill_on_drop(true);
    tokio::select! {
        () = cancel.cancelled() => Err("cancelled".to_string()),
        output = command.output() => output.map_err(|error| error.to_string()),
    }
}

fn output_lines(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .chain(String::from_utf8_lossy(stderr).lines())
        .map(str::to_string)
        .collect()
}

fn nonblank_output(lines: &[String], fallback: &str) -> String {
    lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn local_git_failure(message: String) -> CommandFailure {
    CommandFailure::Local([
        format!("The local git comparison failed: {message}"),
        "No diff reached Estelle.".to_string(),
        "Correct the revision or working tree, then retry.".to_string(),
    ])
}

fn repo_is_listed(repo: &Repo, filed: &[String]) -> bool {
    let local = repo.as_str();
    let short = local.rsplit('/').next().unwrap_or(local);
    filed.iter().any(|item| item == local || item == short)
}

fn production_poll_delay(base: Duration, failures: u32, inactive: bool) -> Duration {
    if inactive {
        return Duration::from_secs(300);
    }
    let multiplier = 1_u64 << failures.min(4);
    Duration::from_secs(
        base.as_secs()
            .saturating_mul(multiplier)
            .min(Duration::from_secs(300).as_secs()),
    )
}

fn production_error_message(error: &Error) -> String {
    if matches!(
        error,
        Error::Http { status, .. } if *status == http::StatusCode::SERVICE_UNAVAILABLE
    ) {
        return "production health is not configured · enable Monitor for this account".to_string();
    }
    format!("production health unavailable · {error}")
}

fn spawn_prod_issues_request(
    client: Client,
    repo: Repo,
    since: Option<f64>,
    tx: &mpsc::UnboundedSender<UiEvent>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut query = serde_json::json!({
            "repo": repo.as_str(),
            "limit": 50,
        });
        if let Some(since) = since {
            query["since"] = serde_json::json!(since);
        }
        let result = client
            .get(
                estelle_client::Endpoint::Issues,
                &query,
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::ProdIssues(result));
    });
}

fn merge_issue_page(
    current: &mut Option<estelle_client::MonitorIssuesResponse>,
    mut page: estelle_client::MonitorIssuesResponse,
) {
    let Some(current) = current.as_mut() else {
        *current = Some(page);
        return;
    };
    for issue in page.issues.drain(..) {
        if let Some(existing) = current
            .issues
            .iter_mut()
            .find(|existing| existing.key == issue.key)
        {
            *existing = issue;
        } else {
            current.issues.push(issue);
        }
    }
    current.next_since = page.next_since.or(current.next_since);
    current.has_more = page.has_more;
    current.repo = page.repo.or_else(|| current.repo.clone());
}

fn spawn_prod_overview_request(client: Client, repo: Repo, tx: &mpsc::UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client
            .get(
                estelle_client::Endpoint::MonitorOverview,
                &serde_json::json!({
                    "repo": repo.as_str(),
                    "window_s": 3600,
                    "buckets": 12,
                }),
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::ProdOverview(result));
    });
}

fn spawn_prod_agent_health_request(
    client: Client,
    repo: Repo,
    tx: &mpsc::UnboundedSender<UiEvent>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client
            .get(
                estelle_client::Endpoint::AgentHealth,
                &serde_json::json!({
                    "repo": repo.as_str(),
                    "window_s": 3600,
                }),
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::ProdAgentHealth(result));
    });
}

fn spawn_prod_github_request(client: Client, repo: Repo, tx: &mpsc::UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let cancel = CancellationToken::new();
        let query = estelle_client::ProposedPrsQuery::first(&repo);
        let (status, proposed_prs) = tokio::join!(
            client.github_status(&cancel),
            client.proposed_prs(&query, &cancel),
        );
        let _ = tx.send(UiEvent::ProdGithub {
            status,
            proposed_prs,
        });
    });
}

fn spawn_header_requests(client: Option<Client>, repo: &Repo, tx: &mpsc::UnboundedSender<UiEvent>) {
    let Some(client) = client else {
        return;
    };
    let account_tx = tx.clone();
    let account_client = client.clone();
    tokio::spawn(async move {
        let result = account_client.account(&CancellationToken::new()).await;
        let _ = account_tx.send(UiEvent::Account(result));
    });
    let overview_tx = tx.clone();
    let overview_client = client.clone();
    tokio::spawn(async move {
        let result = overview_client.overview(&CancellationToken::new()).await;
        let _ = overview_tx.send(UiEvent::Overview(result));
    });
    let repos_tx = tx.clone();
    let repos_client = client.clone();
    tokio::spawn(async move {
        let result = repos_client.repos(&CancellationToken::new()).await;
        let _ = repos_tx.send(UiEvent::Repos(result));
    });
    let settings_tx = tx.clone();
    let settings_client = client.clone();
    tokio::spawn(async move {
        let result = settings_client
            .get(
                estelle_client::Endpoint::SettingsSuite,
                &serde_json::json!({}),
                &CancellationToken::new(),
            )
            .await;
        let _ = settings_tx.send(UiEvent::Settings(result));
    });
    let scope_tx = tx.clone();
    let repo = repo.clone();
    tokio::spawn(async move {
        let result = client
            .get_scoped(
                estelle_client::Endpoint::AutonomyScope,
                &repo,
                &serde_json::json!({}),
                &CancellationToken::new(),
            )
            .await;
        let _ = scope_tx.send(UiEvent::Scope(result));
    });
}

fn spawn_setting_save(
    client: Client,
    suite: String,
    key: String,
    scope: String,
    value: Value,
    tx: &mpsc::UnboundedSender<UiEvent>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client
            .post(
                estelle_client::Endpoint::SettingsSuite,
                &serde_json::json!({
                    "suite": suite,
                    "key": key,
                    "value": value,
                    "scope": scope,
                }),
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::SettingSaved { suite, key, result });
    });
}

fn spawn_theme_save(client: Client, theme: Theme, tx: &mpsc::UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let value = match theme {
            Theme::Dark => "dark",
            Theme::CreamInk => "light",
        };
        let result = client
            .post(
                estelle_client::Endpoint::SettingsSuite,
                &serde_json::json!({
                    "suite": "global",
                    "key": "theme",
                    "value": value,
                    "scope": "personal",
                }),
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::ThemeSaved { theme, result });
    });
}

fn spawn_autonomy_change(client: Client, target: String, tx: &mpsc::UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut body = serde_json::json!({"level": target});
        if matches!(
            body.get("level").and_then(Value::as_str),
            Some("branch" | "execute")
        ) {
            body["acknowledge_risk"] = Value::Bool(true);
        }
        let result = client
            .post(
                estelle_client::Endpoint::Autonomy,
                &body,
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::AutonomyChanged(result));
    });
}

fn spawn_provider_selection(
    client: Client,
    provider: String,
    provider_label: String,
    model: String,
    tx: &mpsc::UnboundedSender<UiEvent>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = client
            .post(
                estelle_client::Endpoint::ProviderSelect,
                &serde_json::json!({"provider": provider, "model": model}),
                &CancellationToken::new(),
            )
            .await;
        let _ = tx.send(UiEvent::ProviderSelected {
            provider: provider_label,
            model,
            result,
        });
    });
}

fn spawn_credential_resolution(tx: &mpsc::UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::task::spawn_blocking(move || {
        let _ = tx.send(UiEvent::Credential(resolve_credential()));
    });
}

fn resolve_credential() -> Result<(Client, AuthContext), Error> {
    let store = CredentialStore::default_location()?;
    let credential = store.resolve()?;
    let client = Client::production(credential.api_key)?;
    Ok((
        client,
        AuthContext {
            store,
            source: credential.source,
        },
    ))
}

#[derive(Debug, Eq, PartialEq)]
enum FailureView {
    AuthRejected,
    Server { status: u16, message: String },
    Request { status: u16, message: String },
    Timeout,
    Network,
    Cancelled,
    Client(String),
}

impl From<&Error> for FailureView {
    fn from(error: &Error) -> Self {
        match error {
            error if error.is_explicit_auth_rejection() => Self::AuthRejected,
            Error::Http { status, message } if status.is_server_error() => Self::Server {
                status: status.as_u16(),
                message: message.clone(),
            },
            Error::Http { status, message } => Self::Request {
                status: status.as_u16(),
                message: message.clone(),
            },
            Error::Transport(source) if source.is_timeout() => Self::Timeout,
            Error::Transport(_) => Self::Network,
            Error::Cancelled => Self::Cancelled,
            _ => Self::Client(error.to_string()),
        }
    }
}

fn failure_lines_for(view: &FailureView) -> [String; 3] {
    match view {
        FailureView::AuthRejected => [
            "Estelle rejected the stored credential.".to_string(),
            "The API reported that this credential is not authorized.".to_string(),
            "Authenticate again, then retry the question.".to_string(),
        ],
        FailureView::Server { status, message } => [
            format!("Estelle returned HTTP {status}: {message}"),
            "The failure is on the Estelle service path.".to_string(),
            "Retry once; if it repeats, narrow the question and report the status.".to_string(),
        ],
        FailureView::Request { status, message } => [
            format!("Estelle returned HTTP {status}: {message}"),
            "The API refused this request as sent.".to_string(),
            "Correct the request or account state, then retry.".to_string(),
        ],
        FailureView::Timeout => [
            "The Estelle request exceeded 300 seconds.".to_string(),
            "The server did not complete the grounded answer in time.".to_string(),
            "Retry or ask a narrower question.".to_string(),
        ],
        FailureView::Network => [
            "The Estelle request could not reach a response.".to_string(),
            "The network path failed before the server returned a result.".to_string(),
            "Check connectivity and retry.".to_string(),
        ],
        FailureView::Cancelled => [
            "The request was cancelled.".to_string(),
            "The client stopped waiting before the server answered.".to_string(),
            "Submit the question again when ready.".to_string(),
        ],
        FailureView::Client(message) => [
            format!("The Estelle request failed: {message}"),
            "The client could not accept the server result.".to_string(),
            "Retry; if it repeats, report this exact failure.".to_string(),
        ],
    }
}

fn failure_lines(error: &Error) -> [String; 3] {
    failure_lines_for(&FailureView::from(error))
}

/// The filled user-turn block, ported from Codex (`style.rs::user_message_style_for`,
/// `history_cell/messages.rs::UserHistoryCell`): a subtle tint over the terminal's own
/// background. Under Cream Ink the painted surface is known, so the tint is deterministic;
/// under Dark the background is inherited (D3), runtime detection decides, and an undetectable
/// background yields NO fill rather than a guessed one.
fn user_turn_style(theme: Theme) -> Style {
    let terminal_bg = match theme {
        Theme::CreamInk => Some((0xE9, 0xE6, 0xDC)),
        Theme::Dark => codex_tui::default_bg(),
    };
    codex_tui::user_message_style_for(terminal_bg)
}

#[cfg(test)]
fn render_transcript(entries: &[TranscriptEntry]) -> Text<'static> {
    render_transcript_with_citations(entries, true, Theme::Dark)
}

fn render_transcript_with_citations(
    entries: &[TranscriptEntry],
    include_citations: bool,
    theme: Theme,
) -> Text<'static> {
    let mut text = Text::default();
    for entry in entries {
        match entry {
            TranscriptEntry::SessionHandoff(lines) => {
                text.lines.push(Line::styled(
                    "Since your last session",
                    Style::default()
                        .fg(theme.primary())
                        .add_modifier(Modifier::BOLD),
                ));
                text.lines.extend(
                    lines.iter().map(|line| {
                        Line::styled(mask_secret(line), Style::default().fg(Color::Gray))
                    }),
                );
                text.lines.push(Line::default());
            }
            TranscriptEntry::User(message) => {
                text.lines.push(Line::from(vec![
                    Span::styled("you  ", Style::default().fg(Color::Gray)),
                    Span::styled(mask_secret(message), user_turn_style(theme)),
                ]));
                text.lines.push(Line::default());
            }
            TranscriptEntry::Answer {
                text: answer,
                grounded,
                degraded,
                sources,
            } => {
                let (label, color) = if *degraded {
                    ("degraded", Color::Yellow)
                } else if *grounded == Some(true) {
                    ("grounded", Color::Cyan)
                } else if *grounded == Some(false) {
                    ("not grounded", Color::Yellow)
                } else {
                    ("conversation", Color::Gray)
                };
                text.lines.push(Line::from(vec![
                    Span::styled(
                        "estelle",
                        Style::default()
                            .fg(theme.primary())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(label, Style::default().fg(color)),
                ]));
                text.lines
                    .extend(render_markdown_text(&mask_secret(answer)).lines);
                if include_citations {
                    for source in sources {
                        text.lines.push(Line::styled(
                            format!("cited  {}", source_label(source)),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
                text.lines.push(Line::default());
            }
            TranscriptEntry::System(message) => {
                text.lines.push(Line::styled(
                    mask_secret(message),
                    Style::default().fg(theme.ghost()),
                ));
                text.lines.push(Line::default());
            }
            TranscriptEntry::Command { name, lines } => {
                text.lines.push(Line::from(vec![
                    Span::styled(
                        "estelle",
                        Style::default()
                            .fg(theme.primary())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  /{}", mask_secret(name))),
                ]));
                for line in lines {
                    let line = if name == "skills" {
                        mask_skill_catalog_line(line)
                    } else {
                        mask_secret(line)
                    };
                    text.lines.push(Line::from(line));
                }
                text.lines.push(Line::default());
            }
            TranscriptEntry::Failure(lines) => {
                text.lines.push(Line::styled(
                    "estelle  failed",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
                for line in lines {
                    text.lines.push(Line::from(mask_secret(line)));
                }
                text.lines.push(Line::default());
            }
        }
    }
    text
}

fn mask_skill_catalog_line(line: &str) -> String {
    let mask_name = |name: &str| {
        let valid = !name.is_empty()
            && name.len() <= 96
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !is_secret_shaped(name);
        if valid {
            name.to_string()
        } else {
            mask_secret(name)
        }
    };
    if let Some((name, description)) = line.split_once("  |  ") {
        return format!("{}  |  {}", mask_name(name), mask_secret(description));
    }
    if line.ends_with(" playbooks") {
        return mask_secret(line);
    }
    mask_name(line)
}

fn source_label(source: &Source) -> String {
    source.line.map_or_else(
        || source.file.clone(),
        |line| format!("{}:{line}", source.file),
    )
}

fn header_line(app: &App, _width: u16) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            "ESTELLE",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(app.theme.ghost())),
        Span::styled(
            app.repo.to_string(),
            Style::default().fg(app.theme.primary()),
        ),
    ];
    if let Some(plan) = app.header.plan.as_deref() {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            plan.to_string(),
            Style::default().fg(Color::Gray),
        ));
    }
    if let Some(team) = app
        .account
        .as_ref()
        .and_then(|account| account.team.as_ref())
    {
        let label = team.name.as_deref().unwrap_or(team.id.as_str());
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            format!("{label} · {}", team.role.as_deref().unwrap_or("member")),
            Style::default().fg(Color::Gray),
        ));
    }
    if let Some(indexed) = app.header.indexed {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            if indexed {
                "repo graph current"
            } else {
                "repo graph absent"
            },
            Style::default().fg(if indexed {
                app.theme.primary()
            } else {
                FATE_RED
            }),
        ));
    }
    if let Some(files) = app.header.files {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            format!("{} files", commas(files)),
            Style::default().fg(Color::Gray),
        ));
    }
    Line::from(spans)
}

fn session_tabs_line(app: &App) -> Line<'static> {
    if app.session_tabs.is_empty() {
        return Line::default();
    }
    let mut spans = vec![Span::styled(
        "SESSIONS  ",
        Style::default()
            .fg(app.theme.ghost())
            .add_modifier(Modifier::BOLD),
    )];
    for session in &app.session_tabs {
        if app.hidden_session_tabs.contains(&session.id) {
            continue;
        }
        let marker = if session.active { "+" } else { "·" };
        let label = format!(" {marker} {} ", session.id);
        let style = if session.id == app.session_id {
            Style::default()
                .fg(app.theme.background())
                .bg(app.theme.primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.ghost())
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        "Alt+Left/Right switch · Ctrl+W close view",
        Style::default().fg(app.theme.ghost()),
    ));
    Line::from(spans)
}

fn value_style(resolved: bool) -> Style {
    Style::default().fg(if resolved {
        Color::Gray
    } else {
        Color::DarkGray
    })
}

fn commas(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn observed_model(reply: &CommandReply) -> Option<&str> {
    reply
        .extra
        .get("active")
        .and_then(Value::as_object)
        .and_then(|active| active.get("model"))
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .or_else(|| {
            reply
                .routed
                .as_deref()
                .filter(|model| !model.trim().is_empty())
        })
}

fn status_line(app: &App, now: Instant) -> Line<'static> {
    if let Some(active) = &app.active {
        let elapsed = now.saturating_duration_since(active.started).as_secs();
        let label = if elapsed >= 30 {
            "still waiting for Estelle".to_string()
        } else {
            active.label.clone()
        };
        let mut spans = vec![
            Span::styled(label, Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "  {}  |  Esc cancels",
                codex_tui::fmt_elapsed_compact(elapsed)
            )),
        ];
        if elapsed >= 30 {
            spans.push(Span::raw("  |  no response received yet"));
        }
        return Line::from(spans);
    }
    if !app.queue.is_empty() {
        return Line::styled(
            format!("{} queued", app.queue.len()),
            Style::default().fg(Color::Gray),
        );
    }
    let mode = commands::mode_name(commands::effective_mode(
        &app.local_mode,
        app.server_mode.as_deref(),
    ));
    let (model, model_resolved) = app.active_model.as_ref().map_or_else(
        || ("routing auto".to_string(), false),
        |model| {
            let freshness = if app
                .active_model_observed_at
                .is_some_and(|observed| now.saturating_duration_since(observed).as_secs() <= 300)
            {
                "observed"
            } else {
                "stale"
            };
            (format!("model {model} · {freshness}"), true)
        },
    );
    let mut spans = vec![
        Span::styled(mode.to_string(), Style::default().fg(Color::Gray)),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(model, value_style(model_resolved)),
    ];
    if let Some(count) = app.header.memories {
        spans.extend([
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("memory {}", commas(count)), value_style(true)),
        ]);
    }
    if app.header.connected {
        spans.extend([
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled("connected", value_style(true)),
        ]);
    }
    Line::from(spans)
}

fn footer_line(app: &App, now: Instant, width: u16) -> Line<'static> {
    if app.active.is_some() || !app.queue.is_empty() {
        return status_line(app, now);
    }
    let mut spans = vec![
        Span::styled("shift+tab", Style::default().fg(app.theme.primary())),
        Span::styled(" change mode  ·  ", Style::default().fg(app.theme.ghost())),
    ];
    if width >= 96 {
        spans.extend([
            Span::styled("tab", Style::default().fg(app.theme.primary())),
            Span::styled(" move focus  ·  ", Style::default().fg(app.theme.ghost())),
        ]);
    }
    if width >= 64 {
        spans.extend([
            Span::styled("/", Style::default().fg(app.theme.primary())),
            Span::styled(" commands  |  ", Style::default().fg(app.theme.ghost())),
        ]);
    }
    spans.extend(status_line(app, now).spans);
    Line::from(spans)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn truncate_display(value: &str, max_width: usize) -> String {
    if value
        .chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>()
        <= max_width
    {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut rendered = String::new();
    let mut width: usize = 0;
    for ch in value.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width.saturating_add(ch_width).saturating_add(1) > max_width {
            break;
        }
        rendered.push(ch);
        width += ch_width;
    }
    rendered.push('…');
    rendered
}

fn render_picker(frame: &mut Frame<'_>, picker: &PickerSurface, area: Rect, app: &App) {
    let login_context = (picker.title == "Connect Estelle").then_some([
        Line::from("Estelle grounds your coding agent in your real codebase."),
        Line::from(
            "It runs on the model plan or API key you already have — Estelle never bills you for model tokens.",
        ),
    ]);
    let context_height = login_context.as_ref().map_or(0, |lines| lines.len());
    let height = u16::try_from(
        picker
            .rows
            .len()
            .saturating_add(context_height)
            .saturating_add(3),
    )
    .unwrap_or(u16::MAX)
    .min(area.height.max(3));
    let modal = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(height),
        width: area.width,
        height,
    };
    frame.render_widget(Clear, modal);
    let inner_width = usize::from(modal.width.saturating_sub(3));
    let label_width = (inner_width / 3).clamp(12, 24);
    let detail_width = inner_width.saturating_sub(label_width.saturating_add(3));
    let mut lines = login_context.into_iter().flatten().collect::<Vec<_>>();
    lines.extend(
        picker
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == picker.selected;
                let badge = if index < 9 {
                    (index + 1).to_string()
                } else {
                    " ".to_string()
                };
                Line::from(vec![
                    Span::styled(
                        format!(
                            "{} {} {:<label_width$}  ",
                            if selected { ">" } else { " " },
                            badge,
                            truncate_display(&row.label, label_width),
                        ),
                        if selected {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                    Span::styled(
                        truncate_display(&row.detail, detail_width),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .chain(std::iter::once(Line::styled(
                "↑↓ navigate · 1-9 or Enter select · Esc close",
                Style::default().fg(Color::DarkGray),
            ))),
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(
                Style::default()
                    .fg(app.theme.primary())
                    .bg(app.theme.background()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(app.theme.primary()))
                    .title(format!(" {} ", picker.title.to_ascii_uppercase())),
            ),
        modal,
    );
}

fn dither_glyph(x: usize, y: usize) -> &'static str {
    let hash = x
        .wrapping_mul(31)
        .wrapping_add(y.wrapping_mul(17))
        .wrapping_add(x.wrapping_mul(y));
    if hash.is_multiple_of(5) { "∷" } else { "·" }
}

fn lily_coverage(x: f64, y: f64, bloom_x: f64, bloom_y: f64, radius: f64) -> f64 {
    // Terminal cells are taller than they are wide, so unit-x is compressed before
    // drawing the same shared spider-lily primitive used by the boot veil.
    let dx = (x - bloom_x) * 2.10 / radius;
    let dy = (y - bloom_y) / radius;
    if dx.abs() > 1.35 || !(-1.35..=1.25).contains(&dy) {
        return 0.0;
    }
    spider_lily_coverage(dx, dy)
}

fn red_lily_coverage(x: f64, y: f64) -> f64 {
    lily_coverage(x, y, 0.78, 0.70, 0.14) * 0.96
}

fn red_lily_braille(x: usize, y: usize, width: usize, height: usize, opacity: f64) -> Option<char> {
    const DOTS: [[u32; 4]; 2] = [[0, 1, 2, 6], [3, 4, 5, 7]];
    let mut mask = 0_u32;
    for (column, rows) in DOTS.iter().enumerate() {
        for (row, bit) in rows.iter().enumerate() {
            let unit_x = (x as f64 + (column as f64 + 0.5) / 2.0) / width.max(1) as f64;
            let unit_y = (y as f64 + (row as f64 + 0.5) / 4.0) / height.max(1) as f64;
            if red_lily_coverage(unit_x, unit_y) * opacity > 0.48 {
                mask |= 1 << bit;
            }
        }
    }
    (mask != 0).then(|| char::from_u32(0x2800 + mask).unwrap_or(' '))
}

fn scene_coverage(x: usize, y: usize, width: usize, height: usize) -> f64 {
    let u = x as f64 / width.max(1) as f64;
    let v = y as f64 / height.max(1) as f64;
    let mut coverage: f64 = 0.0;

    let sun = ((u - 0.85) / 0.07).powi(2) + ((v - 0.13) / 0.07).powi(2);
    if sun < 1.0 {
        coverage = coverage.max(0.09);
    }

    for (cloud_x, cloud_y, cloud_width) in
        [(0.15, 0.06, 0.18), (0.49, 0.11, 0.23), (0.77, 0.16, 0.28)]
    {
        let cloud = ((u - cloud_x) / cloud_width).powi(2) + ((v - cloud_y) / 0.016).powi(2);
        if cloud < 1.0 {
            coverage = coverage.max(0.05);
        }
    }

    for (base, amplitude, ink, frequency_one, frequency_two, phase) in [
        (0.60, 0.026, 0.10, 5.1, 11.7, 1.2),
        (0.70, 0.038, 0.16, 3.9, 9.3, 4.0),
        (0.80, 0.050, 0.24, 3.1, 7.9, 2.3),
        (0.91, 0.058, 0.34, 2.4, 6.1, 5.4),
    ] {
        let ridge = base
            + (u * frequency_one + phase).sin() * amplitude
            + (u * frequency_two + phase * 2.7).sin() * amplitude * 0.4;
        if v >= ridge {
            coverage = coverage.max(ink);
        }
    }

    for (bloom_x, bloom_y, radius, alpha) in [
        (0.05, 0.70, 0.050, 0.32),
        (0.20, 0.745, 0.055, 0.36),
        (0.44, 0.68, 0.050, 0.32),
        (0.60, 0.645, 0.042, 0.26),
        (0.93, 0.66, 0.050, 0.34),
        (0.33, 0.82, 0.075, 0.44),
        (0.66, 0.80, 0.065, 0.40),
        (0.88, 0.845, 0.080, 0.46),
    ] {
        coverage = coverage.max(lily_coverage(u, v, bloom_x, bloom_y, radius) * alpha);
    }
    coverage
}

#[derive(Debug)]
struct SymbolGroundLayout {
    cells: Vec<char>,
    ink: Vec<u8>,
}

type SymbolGroundCache = Mutex<HashMap<(usize, usize), Arc<SymbolGroundLayout>>>;

static SYMBOL_GROUND_CACHE: OnceLock<SymbolGroundCache> = OnceLock::new();

// No `dimmed` variant: the scene's lifecycle owner is "has the first message been submitted",
// not "is the composer empty". It renders full-strength until submission, then not at all.
fn symbol_ground_layout(width: usize, height: usize) -> Arc<SymbolGroundLayout> {
    let key = (width, height);
    let cache = SYMBOL_GROUND_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned();
    if let Some(layout) = cached {
        return layout;
    }

    let opacity = 1.0;
    let mut cells = vec![' '; width.saturating_mul(height)];
    let mut ink = vec![0_u8; width.saturating_mul(height)];
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let index = y * width + x;
            let coverage = scene_coverage(x, y, width, height) * opacity;
            let threshold = (f64::from(BAYER_8[y % 8][x % 8]) + 0.5) / 64.0;
            if let Some(symbol) = red_lily_braille(x, y, width, height, opacity) {
                cells[index] = symbol;
                ink[index] = 2;
                x += 1;
                continue;
            }
            if coverage <= threshold {
                x += 1;
                continue;
            }
            let glyph = if coverage > 0.30 {
                "∷"
            } else {
                dither_glyph(x, y)
            };
            for (offset, character) in glyph.chars().enumerate() {
                if x + offset < width {
                    cells[index + offset] = character;
                    ink[index + offset] = u8::from(coverage > 0.24);
                }
            }
            x += glyph.chars().count().saturating_add(1);
        }
    }

    let layout = Arc::new(SymbolGroundLayout { cells, ink });
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, Arc::clone(&layout));
    layout
}

fn render_symbol_ground(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    if width == 0 || height == 0 {
        return;
    }
    let layout = symbol_ground_layout(width, height);
    let mut rows = Vec::with_capacity(height);
    for y in 0..height {
        let row_start = y * width;
        let cells = &layout.cells[row_start..row_start + width];
        let ink = &layout.ink[row_start..row_start + width];
        let mut spans = Vec::new();
        let mut start = 0;
        while start < width {
            let ink_level = ink[start];
            let mut end = start + 1;
            while end < width && ink[end] == ink_level {
                end += 1;
            }
            spans.push(Span::styled(
                cells[start..end].iter().collect::<String>(),
                match ink_level {
                    2 => Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD),
                    1 => Style::default().fg(if app.theme == Theme::CreamInk {
                        Color::Black
                    } else {
                        FATE_INK
                    }),
                    _ => Style::default()
                        .fg(app.theme.ghost())
                        .add_modifier(Modifier::DIM),
                },
            ));
            start = end;
        }
        rows.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(rows), area);

    let composer_width = width.saturating_sub(4).max(1);
    for (age, cursor) in app.dither_wake.iter().rev().skip(1).enumerate() {
        let x = cursor % composer_width;
        let y = height
            .saturating_sub(1)
            .saturating_sub(cursor / composer_width);
        let glyph = if (cursor + age).is_multiple_of(3) {
            "∷"
        } else {
            "·"
        };
        frame.render_widget(
            Paragraph::new(glyph).style(Style::default().fg(FATE_RED)),
            Rect::new(
                area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX)),
                area.y.saturating_add(u16::try_from(y).unwrap_or(u16::MAX)),
                u16::try_from(glyph.len()).unwrap_or(1),
                1,
            ),
        );
    }
    let cursor = app.composer.cursor();
    let x = cursor % composer_width;
    let y = height
        .saturating_sub(1)
        .saturating_sub(cursor / composer_width);
    frame.render_widget(
        Paragraph::new("∷").style(Style::default().fg(app.theme.primary())),
        Rect::new(
            area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX)),
            area.y.saturating_add(u16::try_from(y).unwrap_or(u16::MAX)),
            1,
            1,
        ),
    );
}

fn session_handoff_lines(app: &App) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    if let Some(context) = &app.session_context {
        lines.extend(context.human_lines.iter().take(4).cloned());
    }
    if let Some(account) = &app.account {
        let identity = match (account.email.as_deref(), account.plan.as_deref()) {
            (Some(email), Some(plan)) => format!("Signed in · {email} · {plan}"),
            (Some(email), None) => format!("Signed in · {email}"),
            (None, Some(plan)) => format!("Account · {plan}"),
            (None, None) => "Account connected".to_string(),
        };
        lines.push(identity);
        if let Some(team) = &account.team {
            let name = team.name.as_deref().unwrap_or(&team.id);
            let role = team.role.as_deref().unwrap_or("role not returned");
            lines.push(format!("Team · {name} · {role}"));
        }
    }
    (!lines.is_empty()).then_some(lines)
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sweep = match app.header.indexed {
        Some(true) => "Refresh this repo's index",
        Some(false) => "Index this repo before asking grounded questions",
        None => "Index or refresh this repo",
    };
    let mut lines = vec![Line::styled(
        format!("Ask about {}", app.repo),
        Style::default()
            .fg(app.theme.primary())
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(context) = &app.session_context {
        lines.push(Line::default());
        lines.push(Line::styled(
            "Since your last session",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ));
        lines.extend(
            context
                .human_lines
                .iter()
                .take(4)
                .map(|line| Line::styled(line.clone(), Style::default().fg(Color::Gray))),
        );
    }
    if let Some(account) = &app.account {
        lines.push(Line::default());
        let identity = match (account.email.as_deref(), account.plan.as_deref()) {
            (Some(email), Some(plan)) => format!("Signed in · {email} · {plan}"),
            (Some(email), None) => format!("Signed in · {email}"),
            (None, Some(plan)) => format!("Account · {plan}"),
            (None, None) => "Account connected".to_string(),
        };
        lines.push(Line::styled(identity, Style::default().fg(Color::Gray)));
        if let Some(team) = &account.team {
            let name = team.name.as_deref().unwrap_or(&team.id);
            let role = team.role.as_deref().unwrap_or("role not returned");
            lines.push(Line::styled(
                format!("Team · {name} · {role}"),
                Style::default().fg(Color::Gray),
            ));
        }
    }
    lines.extend([
        Line::default(),
        Line::from(vec![
            Span::styled("/review  ", Style::default().fg(app.theme.primary())),
            Span::styled("Read current changes", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("/sweep   ", Style::default().fg(app.theme.primary())),
            Span::styled(sweep, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("?        ", Style::default().fg(app.theme.primary())),
            Span::styled("Show shortcuts", Style::default().fg(Color::Gray)),
        ]),
    ]);
    lines.truncate(usize::from(area.height));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_gate_modal(frame: &mut Frame<'_>, modal: &GateModal, content_area: Rect, app: &App) {
    let width = content_area.width.saturating_sub(4).min(86);
    let height = content_area.height.saturating_sub(2).min(18);
    let area = centered_rect(width, height, content_area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FATE_RED))
        .title(" gate · deterministic · no model ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 10 || inner.width < 48 {
        let total_lines = modal
            .files
            .iter()
            .map(|file| file.changed_lines)
            .sum::<u64>();
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "EDIT REFUSED",
                    Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD),
                ),
                Line::raw("Gate protected this repository. Nothing was written."),
                Line::raw(format!("Verdict  {}", modal.verdict)),
                Line::raw(format!(
                    "blast radius  {} files · {total_lines} changed lines",
                    modal.files.len()
                )),
                Line::raw(modal.reasons.join(" | ")),
                Line::styled(
                    "Enter or Esc closes · Ask Estelle",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new("EDIT REFUSED")
            .style(Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("Gate protected this repository. Nothing was written.")
            .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!("Verdict  {}", modal.verdict))
            .style(Style::default().fg(Color::Gray)),
        rows[2],
    );

    let total_lines = modal
        .files
        .iter()
        .map(|file| file.changed_lines)
        .sum::<u64>();
    let points = modal
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| (index as f64, file.changed_lines as f64))
        .collect::<Vec<_>>();
    let max_lines = modal
        .files
        .iter()
        .map(|file| file.changed_lines)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let x_max = modal.files.len().saturating_sub(1).max(1) as f64;
    let dataset = Dataset::default()
        .name("changed lines")
        .marker(Marker::Braille)
        .graph_type(GraphType::Scatter)
        .style(Style::default().fg(app.theme.primary()))
        .data(&points);
    frame.render_widget(
        Chart::new(vec![dataset])
            .block(Block::default().title(format!(
                " blast radius · {} files · {total_lines} changed lines ",
                modal.files.len()
            )))
            .x_axis(Axis::default().bounds([0.0, x_max]))
            .y_axis(Axis::default().bounds([0.0, max_lines])),
        rows[3],
    );

    let mut details = modal
        .files
        .iter()
        .map(|file| Line::from(format!("{:>6}  {}", file.changed_lines, file.path)))
        .collect::<Vec<_>>();
    details.extend(
        modal
            .reasons
            .iter()
            .map(|reason| Line::from(format!("blocked  {reason}"))),
    );
    frame.render_widget(
        Paragraph::new(details)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false }),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new("Enter or Esc closes · Ask Estelle")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        rows[5],
    );
}

fn render_context_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        Line::styled(
            "Repo graph · team's swept copy",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(format!(
            "{} files indexed",
            app.header
                .files
                .map(commas)
                .unwrap_or_else(|| "count pending".to_string())
        )),
    ];
    if app.citations.is_empty() {
        lines.push(Line::styled(
            "No grounded sources in the current answer.",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for source in app.citations.iter().take(8) {
            lines.push(Line::from(source_label(source)));
            let symbol = source
                .extra
                .get("symbol")
                .and_then(Value::as_str)
                .filter(|symbol| !symbol.trim().is_empty())
                .unwrap_or("symbol not disclosed");
            lines.push(Line::styled(
                format!("  symbol  {symbol}"),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Working memory · local request context",
        Style::default()
            .fg(app.theme.primary())
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::styled(
        "Sent through the configured Estelle model path.",
        Style::default().fg(Color::DarkGray),
    ));
    lines.push(Line::styled(
        "Not added to the team's Repo graph.",
        Style::default().fg(Color::DarkGray),
    ));
    if app.working_memory_paths.is_empty() {
        lines.push(Line::styled(
            "No eligible local files were attached to the last question.",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.extend(
            app.working_memory_paths
                .iter()
                .take(8)
                .map(|path| Line::from(path.clone())),
        );
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Alt+M or /context closes",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            app.theme.ghost()
                        }),
                    )
                    .title(" CONTEXT  Alt+M · /context "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
fn production_health_lines(
    response: &estelle_client::MonitorIssuesResponse,
    overview: Option<&estelle_client::MonitorOverviewResponse>,
) -> Vec<String> {
    let unresolved = response
        .issues
        .iter()
        .filter(|issue| issue.status != "resolved")
        .collect::<Vec<_>>();
    if unresolved.is_empty() {
        if let Some(resolved) = response.issues.first() {
            return vec![
                "prod · healthy".to_string(),
                format!("resolved · {}", issue_title(resolved)),
            ];
        }
        return vec!["prod · healthy".to_string()];
    }

    let mut lines = vec![format!(
        "prod · {} unresolved issue{}",
        unresolved.len(),
        if unresolved.len() == 1 { "" } else { "s" }
    )];
    if let Some(overview) = overview {
        let buckets = overview.error_buckets();
        if !buckets.is_empty() {
            lines.push(format!(
                "error counts · {}",
                error_count_sparkline(&buckets)
            ));
            let requests_available = overview.requests_source() != Some("unavailable")
                && buckets.iter().all(|bucket| bucket.requests.is_some());
            if requests_available {
                let errors = buckets.iter().map(|bucket| bucket.errors).sum::<u64>();
                let requests = buckets
                    .iter()
                    .filter_map(|bucket| bucket.requests)
                    .sum::<u64>();
                lines.push(format!("measured · {errors} errors / {requests} requests"));
            } else {
                lines.push("request denominator unavailable".to_string());
            }
            if let Some(p99_ms) = buckets
                .iter()
                .filter_map(|bucket| bucket.p99_ms)
                .reduce(f64::max)
            {
                lines.push(format!("p99 · {p99_ms:.0} ms"));
            }
        }
    }

    for issue in unresolved.into_iter().take(3) {
        lines.push(String::new());
        if issue.effective_repair_status().contains("sandbox") {
            let verdict = issue
                .effective_gate_verdict()
                .or(issue.gate_absent_reason.as_deref())
                .unwrap_or("verdict unavailable");
            lines.push(format!("sandbox · a clone, never production · {verdict}"));
            continue;
        }
        lines.push(format!("caught · {}", issue_title(issue)));
        let events = issue.event_count();
        lines.push(format!("grouped · {events} events"));
        if let Some(read) = issue
            .extra
            .get("read")
            .and_then(Value::as_str)
            .filter(|read| !read.trim().is_empty())
        {
            lines.push(format!("read · {read}"));
        }
        if let Some(range) = &issue.symbol_range {
            lines.push(format!(
                "traced to · {}:{}-{}",
                range.file, range.line_start, range.line_end
            ));
        } else if let Some((file, line)) = issue.bound_location() {
            lines.push(format!("traced to · {file}:{line}"));
        } else if let Some(symbol) = issue
            .bound
            .as_ref()
            .and_then(|bound| bound.symbol.as_deref())
            .filter(|symbol| !symbol.trim().is_empty())
        {
            lines.push(format!("traced to · {symbol} · range unavailable"));
        } else if !issue.symbol.is_empty() {
            lines.push(format!("traced to · {} · range unavailable", issue.symbol));
        } else if !issue.culprit.is_empty() {
            lines.push(format!("traced to · {} · range unavailable", issue.culprit));
        }
        let bind_status = issue.effective_bind_status();
        if bind_status.trim().is_empty() {
            lines.push("bind · unbound · reason not recorded".to_string());
        } else if bind_status != "bound" {
            let bind_detail = issue.effective_bind_detail();
            let detail = if bind_detail.trim().is_empty() {
                "reason not recorded"
            } else {
                bind_detail
            };
            lines.push(format!("bind · {bind_status} · {detail}"));
        }
        let repair_pr = issue.effective_repair_pr();
        let repair_status = issue.effective_repair_status();
        if !repair_pr.is_empty() {
            lines.push(format!("PR · {repair_pr}"));
        } else if repair_status == "proposed" {
            lines.push("drafted repair · awaiting human review".to_string());
        } else if !repair_status.is_empty() && repair_status != "none" {
            lines.push(format!("repair · {repair_status}"));
        }
        if let Some(verdict) = issue.effective_gate_verdict() {
            lines.push(format!("gate · {verdict}"));
        } else if let Some(reason) = issue
            .gate_absent_reason
            .as_deref()
            .filter(|reason| !reason.trim().is_empty())
        {
            lines.push(format!("gate · {reason}"));
        }
    }
    lines
}

fn production_workspace_lines(app: &App) -> Vec<Line<'static>> {
    let heading = |text: String| {
        Line::styled(
            text,
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        )
    };
    let dim = |text: String| Line::styled(text, Style::default().fg(app.theme.ghost()));
    let repo = app
        .prod_issues
        .as_ref()
        .and_then(|response| response.repo.as_deref())
        .unwrap_or_else(|| app.repo.as_str());
    let app_name = app.prod_overview.as_ref().and_then(|overview| {
        ["app", "app_name", "service", "service_name"]
            .into_iter()
            .find_map(|key| overview.extra.get(key).and_then(Value::as_str))
    });
    let org = app.prod_overview.as_ref().and_then(|overview| {
        ["org", "organization", "organization_name"]
            .into_iter()
            .find_map(|key| overview.extra.get(key).and_then(Value::as_str))
    });
    let identity = match (app_name, org) {
        (Some(app_name), Some(org)) => format!("APP HEALTH · {org}/{app_name}"),
        (Some(app_name), None) => format!("APP HEALTH · {app_name}"),
        (None, _) => format!("APP HEALTH · repo {repo}"),
    };
    let mut lines = vec![heading(identity)];

    if !app.auth_resolved {
        lines.push(dim("Connecting to Estelle...".to_string()));
    } else if app.client.is_none() {
        lines.push(dim("Live Monitor unavailable.".to_string()));
        lines.push(dim("Run /login here.".to_string()));
    } else if let Some(error) = &app.prod_issue_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
        lines.push(dim("The client will retry in the background.".to_string()));
    } else if let Some(overview) = &app.prod_overview {
        let buckets = overview.error_buckets();
        if buckets.is_empty() {
            lines.push(dim("No measured error window was returned.".to_string()));
        } else {
            let errors = buckets.iter().map(|bucket| bucket.errors).sum::<u64>();
            let requests = buckets
                .iter()
                .filter_map(|bucket| bucket.requests)
                .sum::<u64>();
            let has_denominator = overview.requests_source() != Some("unavailable")
                && buckets.iter().all(|bucket| bucket.requests.is_some());
            lines.push(Line::from(format!(
                "error counts · {}  {errors}",
                error_count_sparkline(&buckets)
            )));
            if has_denominator {
                lines.push(Line::from(format!(
                    "measured · {errors}/{requests} requests"
                )));
            } else {
                lines.push(dim("request denominator unavailable".to_string()));
            }
        }
        if overview.uptime.checks == 0 {
            lines.push(dim(
                "No uptime checks · add one with POST /monitor/uptime.".to_string()
            ));
        } else {
            lines.push(Line::from(format!(
                "uptime checks · {}/{} up",
                overview.uptime.up, overview.uptime.checks
            )));
            if overview.uptime.down > 0 {
                lines.push(Line::styled(
                    format!("{} uptime check(s) down", overview.uptime.down),
                    Style::default().fg(app.theme.alert()),
                ));
            }
        }
    } else {
        lines.push(dim("Loading a real Monitor window...".to_string()));
    }

    lines.push(Line::from(""));
    lines.push(heading("AGENT HEALTH".to_string()));
    if let Some(error) = &app.prod_agent_health_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
        lines.push(dim("The client will retry in the background.".to_string()));
    } else if let Some(health) = &app.prod_agent_health {
        match health.enabled {
            Some(false) => lines.push(dim(
                "Agent telemetry not enabled · send POST /agent/events after enabling it."
                    .to_string(),
            )),
            None => lines.push(dim(format!(
                "Agent health unknown · {}",
                health
                    .enabled_absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason")
            ))),
            Some(true) => {
                if let Some(counts) = &health.counts {
                    let count = |value: Option<u64>, label: &str| match value {
                        Some(value) => format!("{value} {label}"),
                        None => format!("{label} unknown"),
                    };
                    lines.push(Line::from(format!(
                        "{} · {} · {}",
                        count(counts.reporting, "reporting"),
                        count(counts.degraded, "degraded"),
                        count(counts.silent, "silent")
                    )));
                } else {
                    lines.push(dim(
                        "Agent counts unavailable · server returned no measurement.".to_string(),
                    ));
                }
                match (health.observed_at, health.stale_after_s) {
                    (Some(observed_at), Some(stale_after_s)) => lines.push(dim(format!(
                        "observed {observed_at:.0} · stale threshold {stale_after_s}s"
                    ))),
                    _ => lines.push(dim("Snapshot freshness unavailable.".to_string())),
                }
                for agent in health.agents.iter().take(3) {
                    let state = match agent.state {
                        estelle_client::AgentHealthState::Healthy => "healthy",
                        estelle_client::AgentHealthState::Degraded => "degraded",
                        estelle_client::AgentHealthState::Silent => "silent",
                        estelle_client::AgentHealthState::Disabled => "disabled",
                        estelle_client::AgentHealthState::Unknown => "unknown",
                    };
                    let events = agent
                        .events
                        .map(|events| format!("{events}ev"))
                        .unwrap_or_else(|| "events?".to_string());
                    let signal = agent
                        .current_signal
                        .as_deref()
                        .filter(|signal| !signal.trim().is_empty())
                        .or(agent.state_absent_reason.as_deref())
                        .unwrap_or("signal unavailable");
                    lines.push(Line::from(format!(
                        "{state} {} · {events} · {signal}",
                        agent.id
                    )));
                    if let Some(last_seen) = agent.last_seen {
                        lines.push(dim(format!("       last seen {last_seen:.0}")));
                    }
                }
                if health.agents.len() > 3 {
                    lines.push(dim(format!("+{} more agents", health.agents.len() - 3)));
                }
            }
        }
    } else {
        lines.push(dim(
            "State unavailable · no read contract · send POST /agent/events.".to_string(),
        ));
    }

    lines.push(Line::from(""));
    lines.push(heading("ESTELLE STATUS".to_string()));
    match app.prod_issues.as_ref() {
        Some(response) => {
            let unresolved = response
                .issues
                .iter()
                .filter(|issue| issue.status != "resolved")
                .collect::<Vec<_>>();
            if !unresolved.is_empty() {
                for issue in unresolved.iter().take(2) {
                    let events = issue.event_count();
                    let location = issue
                        .bound_location()
                        .map(|(file, line)| format!("{file}:{line}"))
                        .unwrap_or_else(|| "unbound · reason not recorded".to_string());
                    lines.push(Line::from(format!("caught · {}", issue_title(issue))));
                    lines.push(dim(format!(
                        "grouped · {events} event(s) · traced to · {location}"
                    )));
                    if let Some(range) = &issue.symbol_range
                        && range.line_end > range.line_start
                    {
                        lines.push(dim(format!(
                            "       range {}:{}-{}",
                            range.file, range.line_start, range.line_end
                        )));
                    }
                }
                if unresolved.len() > 2 {
                    lines.push(dim(format!(
                        "+{} more · open /monitor issues",
                        unresolved.len() - 2
                    )));
                }
            } else {
                lines.push(dim("No errors have reached Estelle yet.".to_string()));
                lines.push(dim(
                    "Point OTLP or Sentry at api.fatelabs.ca/monitor/ingest.".to_string(),
                ));
            }
        }
        None => lines.push(dim("Waiting for the live issue feed...".to_string())),
    }

    lines.push(Line::from(""));
    lines.push(heading("ESTELLE QUEUE".to_string()));
    let queued = app
        .prod_issues
        .as_ref()
        .into_iter()
        .flat_map(|response| response.issues.iter())
        .filter(|issue| issue.status != "resolved")
        .filter(|issue| {
            !issue.effective_repair_status().trim().is_empty()
                && issue.effective_repair_status() != "none"
        })
        .take(3)
        .collect::<Vec<_>>();
    if queued.is_empty() {
        lines.push(dim("Queue empty · no repair work is reported.".to_string()));
        lines.push(dim("Issue selection: /monitor issues".to_string()));
    } else {
        for issue in queued {
            let repair_pr = issue.effective_repair_pr();
            let repair_status = issue.effective_repair_status();
            let destination = if repair_pr.trim().is_empty() {
                "awaiting human review".to_string()
            } else {
                repair_pr.to_string()
            };
            let label = if repair_status == "proposed" {
                "drafted repair"
            } else {
                repair_status
            };
            lines.push(Line::from(format!(
                "{label} · {} · {destination}",
                issue_title(issue)
            )));
            if let Some(verdict) = issue.effective_gate_verdict() {
                lines.push(dim(format!("       gate · {verdict}")));
            } else if let Some(reason) = issue
                .gate_absent_reason
                .as_deref()
                .filter(|reason| !reason.trim().is_empty())
            {
                lines.push(dim(format!("       gate absent · {reason}")));
            }
            if let Some(patch) = issue.effective_repair_patch() {
                let short_sha = patch.base_sha.chars().take(12).collect::<String>();
                lines.push(dim(format!(
                    "       patch · {} · base {short_sha}",
                    patch.format
                )));
                lines.extend(github_diff_lines(&patch.text, 96, app));
            } else {
                let reason = issue
                    .effective_patch_absent_reason()
                    .unwrap_or("unavailable");
                lines.push(dim(format!("       diff unavailable - {reason}")));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(heading("GITHUB".to_string()));
    if let Some(error) = &app.prod_github_status_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
    } else if let Some(status) = &app.prod_github_status {
        match status.connected {
            Some(true) => {
                let identity = status
                    .login
                    .as_deref()
                    .filter(|login| !login.trim().is_empty())
                    .map(|login| format!(" · @{login}"))
                    .unwrap_or_default();
                lines.push(Line::from(format!("Connected{identity}")));
                if let Some(observed_at) = status.observed_at {
                    lines.push(dim(format!("binding observed {observed_at:.0}")));
                }
            }
            Some(false) => {
                lines.push(dim(
                    "Not connected · run estelle github connect.".to_string()
                ));
                lines.push(dim(
                    "Proposed PRs are not read without a measured App binding.".to_string(),
                ));
            }
            None => {
                let reason = status
                    .absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason");
                lines.push(dim(format!("Connection unknown · {reason}")));
                lines.push(dim("Proposed PR state is not inferred.".to_string()));
            }
        }
    } else {
        lines.push(dim(
            "Waiting for measured GitHub connection state...".to_string()
        ));
    }

    if app
        .prod_github_status
        .as_ref()
        .and_then(|status| status.connected)
        == Some(true)
    {
        if let Some(error) = &app.prod_proposed_prs_error {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(app.theme.alert()),
            ));
        } else if let Some(response) = &app.prod_proposed_prs {
            if response.prs.is_empty() {
                lines.push(dim("No open Estelle-proposed PRs returned.".to_string()));
            }
            for pr in response.prs.iter().take(3) {
                let title = if pr.title.trim().is_empty() {
                    "untitled PR"
                } else {
                    pr.title.as_str()
                };
                lines.push(Line::from(format!("#{} · {title}", pr.number)));
                lines.push(dim(format!("       {}", pr.url)));
                if let Some(gate) = &pr.gate {
                    let verified = if gate.verified { " · verified" } else { "" };
                    lines.push(dim(format!(
                        "       gate · {} · {} · {} blocker(s){verified}",
                        gate.state, gate.verdict, gate.blockers
                    )));
                } else {
                    let reason = pr
                        .gate_absent_reason
                        .as_deref()
                        .filter(|reason| !reason.trim().is_empty())
                        .unwrap_or("server returned no reason");
                    lines.push(dim(format!("       gate absent · {reason}")));
                }
                if !pr.updated_at.trim().is_empty() {
                    lines.push(dim(format!("       updated {}", pr.updated_at)));
                }
            }
            if response.has_more {
                lines.push(dim(
                    "More open proposed PRs exist than this page shows.".to_string()
                ));
            }
        } else {
            lines.push(dim("Waiting for the proposed-PR feed...".to_string()));
        }
    }
    lines
}

fn issue_title(issue: &estelle_client::MonitorIssue) -> &str {
    issue.display_title()
}

fn error_count_sparkline(buckets: &[estelle_client::MonitorErrorBucket]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = buckets
        .iter()
        .map(|bucket| bucket.errors)
        .max()
        .unwrap_or(0);
    buckets
        .iter()
        .map(|bucket| {
            let index = bucket
                .errors
                .saturating_mul(7)
                .checked_div(max)
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(0);
            BARS[index.min(7)]
        })
        .collect()
}

fn render_prod_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = production_workspace_lines(app);
    let has_unresolved = app.prod_issues.as_ref().is_some_and(|response| {
        response
            .issues
            .iter()
            .any(|issue| issue.status != "resolved")
    });
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            if has_unresolved {
                                app.theme.alert()
                            } else {
                                app.theme.ghost()
                            }
                        }),
                    )
                    .title(" LIVE PRODUCTION "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_diff_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![Line::styled(
        "read-only · /apply submits this exact patch",
        Style::default().fg(Color::DarkGray),
    )];
    if let Some(diff) = app.last_diff.as_deref() {
        lines.extend(github_diff_lines(
            diff,
            usize::from(area.width.saturating_sub(2)),
            app,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            app.theme.ghost()
                        }),
                    )
                    .title(" WORK DRAFT · /work · READ ONLY "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn hunk_line_numbers(header: &str) -> Option<(usize, usize)> {
    let mut old = None;
    let mut new = None;
    for token in header.split_whitespace() {
        if let Some(range) = token.strip_prefix('-') {
            old = range.split(',').next().and_then(|value| value.parse().ok());
        } else if let Some(range) = token.strip_prefix('+') {
            new = range.split(',').next().and_then(|value| value.parse().ok());
        }
    }
    old.zip(new)
}

fn github_diff_lines(diff: &str, width: usize, app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut old_line = 0_usize;
    let mut new_line = 0_usize;
    let number_width = 3_usize;
    let content_width = width.saturating_sub(number_width * 2 + 5);

    let (add_line_bg, add_gutter_bg, del_line_bg, del_gutter_bg) = match app.theme {
        Theme::Dark => (
            Color::from_u32(0x21_3A_2B),
            Color::from_u32(0x16_2E_20),
            Color::from_u32(0x4A_22_1D),
            Color::from_u32(0x36_17_14),
        ),
        Theme::CreamInk => (
            Color::from_u32(0xDA_FB_E1),
            Color::from_u32(0xAC_EE_BB),
            Color::from_u32(0xFF_EB_E9),
            Color::from_u32(0xFF_CE_CB),
        ),
    };

    for source in diff.lines() {
        if let Some(path) = source.strip_prefix("diff --git a/") {
            let path = path.split(" b/").next().unwrap_or(path);
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::styled(
                path.to_string(),
                Style::default()
                    .fg(app.theme.primary())
                    .add_modifier(Modifier::BOLD),
            ));
            old_line = 0;
            new_line = 0;
            continue;
        }
        if source.starts_with("---")
            || source.starts_with("+++")
            || source.starts_with("index ")
            || source.starts_with("new file mode ")
            || source.starts_with("deleted file mode ")
        {
            continue;
        }
        if source.starts_with("@@") {
            if let Some((old, new)) = hunk_line_numbers(source) {
                old_line = old;
                new_line = new;
            }
            lines.push(Line::styled(
                truncate_display(source, width),
                Style::default().fg(Color::Cyan),
            ));
            continue;
        }

        let (old, new, sign, content, line_bg, gutter_bg, foreground) =
            if let Some(content) = source.strip_prefix('+') {
                let row = (
                    None,
                    Some(new_line),
                    '+',
                    content,
                    add_line_bg,
                    add_gutter_bg,
                    if app.theme == Theme::CreamInk {
                        FATE_INK
                    } else {
                        Color::Green
                    },
                );
                new_line = new_line.saturating_add(1);
                row
            } else if let Some(content) = source.strip_prefix('-') {
                let row = (
                    Some(old_line),
                    None,
                    '-',
                    content,
                    del_line_bg,
                    del_gutter_bg,
                    if app.theme == Theme::CreamInk {
                        FATE_INK
                    } else {
                        FATE_BG
                    },
                );
                old_line = old_line.saturating_add(1);
                row
            } else if let Some(content) = source.strip_prefix(' ') {
                let row = (
                    Some(old_line),
                    Some(new_line),
                    ' ',
                    content,
                    app.theme.background(),
                    app.theme.background(),
                    app.theme.ghost(),
                );
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                row
            } else {
                lines.push(Line::styled(
                    truncate_display(source, width),
                    Style::default().fg(Color::DarkGray),
                ));
                continue;
            };

        let old = old.map_or_else(String::new, |value| value.to_string());
        let new = new.map_or_else(String::new, |value| value.to_string());
        let content = truncate_display(content, content_width);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{old:>number_width$} {new:>number_width$} {sign} "),
                Style::default().fg(app.theme.ghost()).bg(gutter_bg),
            ),
            Span::styled(
                format!("{content:<content_width$}"),
                Style::default().fg(foreground).bg(line_bg),
            ),
        ]));
    }
    lines
}

fn render_frame(frame: &mut Frame<'_>, app: &App, now: Instant) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(
            Style::default()
                .fg(app.theme.primary())
                .bg(app.theme.background()),
        ),
        area,
    );
    let content_area = area;
    let composer_inner_height = app.composer.desired_height(content_area.width).clamp(1, 6);
    let modal_owns_input = app.picker.is_some() || app.gate_modal.is_some();
    let composer_height = if modal_owns_input {
        0
    } else {
        composer_inner_height.saturating_add(2)
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(composer_height),
            Constraint::Length(1),
        ])
        .split(content_area);

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            header_line(app, area.width),
            session_tabs_line(app),
        ])),
        rows[0],
    );
    let palette = commands::palette_rows(&app.composer.text());
    let palette_open = !palette.is_empty();
    let surface_rows = if !palette_open {
        vec![rows[1]]
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(u16::try_from(palette.len().saturating_add(2)).unwrap_or(10)),
            ])
            .split(rows[1])
            .to_vec()
    };

    let diff_as_rail = app.diff_panel_visible && area.width >= 110;
    let prod_as_rail = !app.diff_panel_visible
        && app.prod_panel_visible
        && area.width >= 110
        && app.gate_modal.is_none()
        && app.picker.is_none()
        && !palette_open;
    let show_diff_panel = app.diff_panel_visible && !diff_as_rail;
    let show_prod_panel = app.prod_panel_visible && !prod_as_rail && !app.diff_panel_visible;
    let show_context_panel = !app.diff_panel_visible && !prod_as_rail && app.context_panel_visible;
    let show_citation_pane = !app.diff_panel_visible
        && !prod_as_rail
        && !show_context_panel
        && area.width >= 100
        && !app.citations.is_empty();
    let show_auxiliary_pane =
        diff_as_rail || prod_as_rail || show_context_panel || show_citation_pane;
    let main_areas = if show_auxiliary_pane {
        let pane_width = if diff_as_rail {
            54.min(area.width.saturating_sub(54))
        } else if prod_as_rail {
            48.min(area.width.saturating_sub(54))
        } else if show_citation_pane {
            36
        } else if area.width >= 90 {
            42.min(area.width.saturating_sub(44))
        } else {
            area.width.saturating_sub(30).max(24)
        };
        let areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(30),
                Constraint::Length(1),
                Constraint::Length(pane_width),
            ])
            .split(surface_rows[0])
            .to_vec();
        vec![areas[0], areas[2]]
    } else {
        vec![surface_rows[0]]
    };

    if show_prod_panel {
        render_prod_panel(frame, surface_rows[0], app);
    } else if show_diff_panel {
        render_diff_panel(frame, surface_rows[0], app);
    } else {
        let primary_title = if app.fleet.is_some() {
            " ORCHESTRA  Ctrl+←/→ focus "
        } else if app.todo_visible {
            " TASKS  Ctrl+T expand · Ctrl+←/→ focus "
        } else {
            " CONVERSATION  Tab · Ctrl+←/→ focus "
        };
        let primary_borders = if palette_open {
            Borders::TOP | Borders::LEFT | Borders::RIGHT
        } else {
            Borders::ALL
        };
        let primary_block = Block::default()
            .borders(primary_borders)
            .border_style(
                Style::default().fg(if app.focus == FocusSurface::Transcript {
                    app.theme.primary()
                } else {
                    app.theme.ghost()
                }),
            )
            .title(primary_title);
        let primary_area = primary_block.inner(main_areas[0]);
        frame.render_widget(primary_block, main_areas[0]);

        let transcript_band = if let Some(fleet) = &app.fleet {
            let raw_lines = commands::fleet_view_lines(fleet, primary_area.width);
            let wanted = u16::try_from(raw_lines.len()).unwrap_or(u16::MAX);
            let fleet_height = wanted.min(primary_area.height.saturating_sub(1));
            let fleet_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(fleet_height), Constraint::Min(1)])
                .split(primary_area);
            let last = raw_lines.len().saturating_sub(1);
            let lines = raw_lines
                .into_iter()
                .enumerate()
                .map(|(index, line)| {
                    if index == last {
                        styled_fleet_progress_line(line)
                    } else {
                        styled_fleet_agent_line(line)
                    }
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().fg(Color::Gray)),
                fleet_rows[0],
            );
            fleet_rows[1]
        } else {
            primary_area
        };
        let transcript_band = if app.todo_visible {
            if let Some(todo) = &app.todo {
                let lines = commands::todo_view_lines(todo, app.todo_expanded);
                let wanted = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                let height = wanted.min(transcript_band.height.saturating_sub(1));
                let bands = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(height), Constraint::Min(1)])
                    .split(transcript_band);
                let rendered = lines
                    .into_iter()
                    .map(|line| {
                        let style = if line.starts_with("✓ ") {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::CROSSED_OUT)
                        } else if line.starts_with("● ") {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else if line == "Todo" {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        };
                        Line::styled(line, style)
                    })
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(rendered), bands[0]);
                bands[1]
            } else {
                transcript_band
            }
        } else {
            transcript_band
        };
        let transcript_root = if let Some(progress) = &app.sweep_progress {
            let sweep_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(transcript_band);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "sweep  ",
                        Style::default()
                            .fg(app.theme.primary())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(progress.line(), Style::default().fg(Color::Gray)),
                ])),
                sweep_rows[0],
            );
            frame.render_widget(
                Gauge::default()
                    .gauge_style(
                        Style::default()
                            .fg(app.theme.primary())
                            .bg(app.theme.ghost()),
                    )
                    .ratio((progress.percent / 100.0).clamp(0.0, 1.0))
                    .label(format!("{:.0}%", progress.percent)),
                sweep_rows[1],
            );
            sweep_rows[2]
        } else {
            transcript_band
        };
        let transcript =
            render_transcript_with_citations(&app.transcript, !show_citation_pane, app.theme);
        let paragraph = Paragraph::new(transcript).wrap(Wrap { trim: false });
        let line_count = paragraph.line_count(transcript_root.width);
        let visible = usize::from(transcript_root.height);
        let bottom_scroll = line_count.saturating_sub(visible);
        let scroll =
            u16::try_from(bottom_scroll.saturating_sub(app.transcript_scroll.min(bottom_scroll)))
                .unwrap_or(u16::MAX);
        let show_ground = !app.has_submitted_question
            && app.transcript.is_empty()
            && app.sweep_progress.is_none()
            && app.gate_modal.is_none()
            && app.fleet.is_none()
            && !app.todo_visible
            && app.picker.is_none()
            && !show_auxiliary_pane
            && !palette_open;
        if show_ground {
            render_symbol_ground(frame, transcript_root, app);
        }
        frame.render_widget(paragraph.scroll((scroll, 0)), transcript_root);
        if show_ground {
            render_empty_state(frame, transcript_root, app);
        }
    }
    if let Some(citation_area) = main_areas.get(1).copied() {
        if diff_as_rail {
            render_diff_panel(frame, citation_area, app);
        } else if prod_as_rail {
            render_prod_panel(frame, citation_area, app);
        } else if show_context_panel {
            render_context_panel(frame, citation_area, app);
        } else {
            let lines = app
                .citations
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:>2}  ", index + 1),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            source_label(source),
                            Style::default().fg(app.theme.primary()),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(
                                if app.focus == FocusSurface::Auxiliary {
                                    app.theme.primary()
                                } else {
                                    Color::DarkGray
                                },
                            ))
                            .title(" CITED EVIDENCE "),
                    )
                    .wrap(Wrap { trim: false }),
                citation_area,
            );
        }
    }
    if let Some(area) = surface_rows.get(1).copied() {
        let lines = palette
            .into_iter()
            .enumerate()
            .map(|(index, (name, description))| {
                let selected = index == app.palette_index;
                Line::from(vec![
                    Span::styled(
                        format!("{} /{name:<11}", if selected { ">" } else { " " }),
                        if selected {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Cyan)
                        },
                    ),
                    Span::styled(
                        description,
                        if selected {
                            Style::default().fg(Color::Gray)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(app.theme.primary()))
                    .title(" COMMANDS  ↑/↓ select · Tab complete · Enter run "),
            ),
            area,
        );
    }
    let composer_area = if modal_owns_input {
        Rect::default()
    } else {
        let composer_borders = if !palette_open {
            Borders::ALL
        } else {
            Borders::LEFT | Borders::RIGHT | Borders::BOTTOM
        };
        let mut composer_block =
            Block::default()
                .borders(composer_borders)
                .border_style(Style::default().fg(if app.focus == FocusSurface::Composer {
                    app.theme.primary()
                } else {
                    app.theme.ghost()
                }));
        if !palette_open {
            composer_block = composer_block.title(" ASK ESTELLE ");
        }
        let composer_area = composer_block.inner(rows[2]);
        frame.render_widget(composer_block, rows[2]);
        app.composer.render_ref_with_background(
            composer_area,
            frame.buffer_mut(),
            app.theme.background(),
        );
        composer_area
    };
    frame.render_widget(
        Paragraph::new(footer_line(app, now, rows[3].width)),
        rows[3],
    );
    if let Some(picker) = &app.picker {
        render_picker(frame, picker, rows[1], app);
    } else if let Some(modal) = &app.gate_modal {
        render_gate_modal(frame, modal, rows[1], app);
    } else if !app.boot_active(now)
        && app.focus == FocusSurface::Composer
        && let Some(position) = app.composer.cursor_pos(composer_area)
    {
        frame.set_cursor_position(position);
    }
    if let Some(boot) = &app.boot {
        let elapsed_ms = app.boot_elapsed_ms(now);
        if !boot.phase(elapsed_ms).is_finished() {
            boot.render(
                area,
                frame.buffer_mut(),
                elapsed_ms,
                app.theme.boot_palette(),
            );
        }
    }
}

fn styled_fleet_progress_line(line: String) -> Line<'static> {
    let Some(open) = line.find('[') else {
        return Line::from(line);
    };
    let Some(relative_close) = line[open..].find(']') else {
        return Line::from(line);
    };
    let close = open + relative_close;
    let prefix = line[..open].to_string();
    let bar = &line[open + 1..close];
    let boundary = bar.find('─').unwrap_or(bar.len());
    let completed = bar[..boundary].to_string();
    let remaining = bar[boundary..].to_string();
    let suffix = line[close + 1..].to_string();
    Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::styled("[", Style::default().fg(Color::Gray)),
        Span::styled(completed, Style::default().fg(Color::Green)),
        Span::styled(remaining, Style::default().fg(Color::Blue)),
        Span::styled(format!("]{suffix}"), Style::default().fg(Color::Gray)),
    ])
}

fn styled_fleet_agent_line(line: String) -> Line<'static> {
    let markers = [
        ("✓ ", Color::Green),
        ("× ", Color::Red),
        ("◷ ", Color::Yellow),
        ("■ ", Color::Magenta),
        ("? ", Color::Cyan),
    ];
    let mut spans = Vec::new();
    let mut remaining = line.as_str();
    while let Some((offset, marker, colour)) = markers
        .iter()
        .filter_map(|(marker, colour)| {
            remaining
                .find(marker)
                .map(|offset| (offset, *marker, *colour))
        })
        .min_by_key(|(offset, _, _)| *offset)
    {
        if offset > 0 {
            spans.push(Span::styled(
                remaining[..offset].to_string(),
                Style::default().fg(Color::Gray),
            ));
        }
        spans.push(Span::styled(
            marker.to_string(),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ));
        remaining = &remaining[offset + marker.len()..];
    }
    if !remaining.is_empty() {
        spans.push(Span::styled(
            remaining.to_string(),
            Style::default().fg(Color::Gray),
        ));
    }
    Line::from(spans)
}

fn draw(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &App) -> io::Result<()> {
    let now = Instant::now();
    terminal.draw(|frame| render_frame(frame, app, now))?;
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent, tx: &mpsc::UnboundedSender<UiEvent>) -> bool {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return false;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.cancel_active();
        return true;
    }
    if let Some(picker) = app.picker.as_mut() {
        match key.code {
            KeyCode::Esc if !app.login_required => app.picker = None,
            KeyCode::Esc => {}
            KeyCode::Down => {
                picker.selected = (picker.selected + 1).min(picker.rows.len().saturating_sub(1));
            }
            KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Enter => app.activate_picker(tx),
            // Number-key direct select, ported from Codex's ListSelectionView
            // (bottom_pane/list_selection_view.rs:1054): a digit picks that row and activates
            // it in one keypress — no arrow walk, no Enter.
            KeyCode::Char(c)
                if c.is_ascii_digit()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if let Some(index) = c
                    .to_digit(10)
                    .map(|digit| digit as usize)
                    .and_then(|number| number.checked_sub(1))
                    .filter(|index| *index < picker.rows.len())
                {
                    picker.selected = index;
                    app.activate_picker(tx);
                }
            }
            _ => {}
        }
        return false;
    }
    if app.gate_modal.is_some() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
            app.gate_modal = None;
        }
        return false;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Left | KeyCode::Right)
    {
        app.move_focus(key.code == KeyCode::Right);
        return false;
    }
    if key.code == KeyCode::Esc && app.focus != FocusSurface::Composer {
        app.focus = FocusSurface::Composer;
        return false;
    }
    if key.code == KeyCode::Char('m') && key.modifiers.contains(KeyModifiers::ALT) {
        app.toggle_context_panel();
        return false;
    }
    if key.code == KeyCode::Char('t') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.todo.is_some() {
            app.todo_visible = true;
            app.todo_expanded = !app.todo_expanded;
        } else {
            app.transcript.push(TranscriptEntry::System(
                "Todo state unavailable: the server has not emitted a task ledger for this session."
                    .to_string(),
            ));
        }
        return false;
    }
    if key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.cycle_session(key.modifiers.contains(KeyModifiers::SHIFT));
        return false;
    }
    if key.modifiers.contains(KeyModifiers::ALT)
        && matches!(key.code, KeyCode::Left | KeyCode::Right)
    {
        app.cycle_session(key.code == KeyCode::Left);
        return false;
    }
    if key.code == KeyCode::Char('w')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.session.is_some()
    {
        app.close_session_tab();
        return app.should_exit;
    }
    if key.code == KeyCode::Esc && app.active.is_some() {
        app.cancel_active();
        app.start_next(tx);
        return false;
    }
    if key.code == KeyCode::Char('?') && key.modifiers.is_empty() && app.composer.is_empty() {
        app.submit("/help".to_string(), tx);
        return false;
    }
    if key.code == KeyCode::BackTab {
        app.picker = Some(PickerSurface::autonomy(app));
        return false;
    }
    let palette = commands::palette_rows(&app.composer.text());
    if !palette.is_empty() {
        match key.code {
            KeyCode::Down => {
                app.palette_index = (app.palette_index + 1).min(palette.len() - 1);
                return false;
            }
            KeyCode::Up => {
                app.palette_index = app.palette_index.saturating_sub(1);
                return false;
            }
            KeyCode::Tab => {
                let (name, _) = palette[app.palette_index.min(palette.len() - 1)];
                app.composer.set_text(format!("/{name} "));
                app.palette_index = 0;
                app.record_dither_caret();
                return false;
            }
            KeyCode::Enter => {
                let (name, _) = palette[app.palette_index.min(palette.len() - 1)];
                app.composer = estelle_composer();
                app.palette_index = 0;
                app.submit(format!("/{name}"), tx);
                return false;
            }
            _ => {}
        }
    }
    if key.code == KeyCode::Tab {
        app.move_focus(true);
        return false;
    }
    if app.focus == FocusSurface::Transcript {
        match key.code {
            KeyCode::Up => app.transcript_scroll = app.transcript_scroll.saturating_add(1),
            KeyCode::Down => app.transcript_scroll = app.transcript_scroll.saturating_sub(1),
            KeyCode::Char(_) => app.focus = FocusSurface::Composer,
            _ => return false,
        }
    } else if app.focus == FocusSurface::Auxiliary {
        if matches!(key.code, KeyCode::Char(_)) {
            app.focus = FocusSurface::Composer;
        } else {
            return false;
        }
    }
    let previous_text = app.composer.text();
    if let ComposerAction::Submitted(text) = app.composer.input(key) {
        app.submit_composer(text, tx);
    } else {
        if app.composer.text() != previous_text {
            app.palette_index = 0;
        }
        app.inspect_composer_for_credential();
        app.record_dither_caret();
    }
    false
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    const WHEEL_LINES: usize = 3;
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.transcript_scroll = app.transcript_scroll.saturating_add(WHEEL_LINES);
        }
        MouseEventKind::ScrollDown => {
            app.transcript_scroll = app.transcript_scroll.saturating_sub(WHEEL_LINES);
        }
        _ => {}
    }
}

async fn record_session_checkpoint(root: PathBuf) {
    let scan_root = root.clone();
    let files = tokio::task::spawn_blocking(move || top_level::working_memory_files(&scan_root))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .map(|file| PathBuf::from(file.path))
        .collect::<Vec<_>>();
    let _ = session_gap::record_checkpoint(&root, &files, chrono::Utc::now()).await;
}

async fn run(
    args: Args,
    session_socket: Option<PathBuf>,
    session_id: Option<String>,
) -> io::Result<()> {
    let connected = session_socket.is_some();
    // The attached terminal is a transport/rendering client. It neither resolves nor owns the
    // Estelle credential; only `serve` does. This also keeps keychain prompts out of reconnects.
    let initial_credential = (!connected).then(resolve_credential);
    let mut app = App::new(args);
    let session_connection = match session_socket {
        Some(socket) => Some(
            session_server::SessionConnection::connect_named(
                &socket,
                app.repo.clone(),
                app.root.clone(),
                session_id.as_deref().unwrap_or("main"),
            )
            .await
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "cannot connect to Estelle session server at {}: {error}",
                        socket.display()
                    ),
                )
            })?,
        ),
        None => None,
    };
    let mut session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    if let Some(connection) = session_connection {
        let (handle, mut session_events) = connection.start();
        app.session = Some(handle);
        let session_tx = tx.clone();
        tokio::spawn(async move {
            while let Some(event) = session_events.recv().await {
                let ui_event = match event {
                    Ok(message) => UiEvent::Session(message),
                    Err(error) => UiEvent::SessionDisconnected(error),
                };
                if session_tx.send(ui_event).is_err() {
                    return;
                }
            }
        });
    }
    match initial_credential {
        None => {
            app.auth_resolved = true;
            app.header.connected = true;
        }
        Some(result) => app.handle_ui_event(UiEvent::Credential(result), &tx),
    }
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(FRAME_INTERVAL);
    let mut first_frame = true;

    loop {
        draw(&mut terminal, &app)?;
        if first_frame {
            first_frame = false;
            let context_root = app.root.clone();
            let context_tx = tx.clone();
            tokio::spawn(async move {
                let context = session_gap::welcome_context(&context_root, chrono::Utc::now()).await;
                let _ = context_tx.send(UiEvent::SessionContext(context));
            });
        }
        tokio::select! {
            _ = ticker.tick() => {
                app.composer.flush_paste_burst_if_due();
                app.poll_production_if_due(&tx);
            }
            Some(event) = rx.recv() => app.handle_ui_event(event, &tx),
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => {
                    app.last_interaction = Instant::now();
                    app.skip_boot(app.last_interaction);
                    if handle_key(&mut app, key, &tx) {
                        break;
                    }
                }
                Some(Ok(Event::Paste(pasted))) => {
                    app.last_interaction = Instant::now();
                    app.skip_boot(app.last_interaction);
                    app.handle_paste(pasted);
                }
                Some(Ok(Event::Mouse(mouse))) => {
                    app.last_interaction = Instant::now();
                    app.skip_boot(app.last_interaction);
                    handle_mouse(&mut app, mouse);
                }
                Some(Ok(Event::FocusGained)) => {
                    app.terminal_focused = true;
                    app.last_interaction = Instant::now();
                }
                Some(Ok(Event::FocusLost)) => app.terminal_focused = false,
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error),
                None => break,
            },
        }
        if let Some(pending_login) = app.pending_login.take() {
            session.suspend()?;
            let result = match pending_login {
                PendingLogin::Estelle => login::run().await.map(InlineLoginOutcome::Estelle),
                PendingLogin::Claude => claude_import::run().map(|()| InlineLoginOutcome::Claude),
                PendingLogin::Chatgpt => login::run_chatgpt()
                    .await
                    .map(|()| InlineLoginOutcome::Chatgpt),
                PendingLogin::Provider(provider) => run_provider_login(provider, None, None, None)
                    .await
                    .map(|()| InlineLoginOutcome::Provider(provider)),
                PendingLogin::EstelleThenProvider(provider) => match login::run().await {
                    Ok(
                        login::LoginOutcome::StoredVerified | login::LoginOutcome::StoredUnverified,
                    ) => run_provider_login(provider, None, None, None)
                        .await
                        .map(|()| InlineLoginOutcome::Provider(provider)),
                    Ok(login::LoginOutcome::Rejected) => {
                        Ok(InlineLoginOutcome::Estelle(login::LoginOutcome::Rejected))
                    }
                    Err(error) => Err(error),
                },
            };
            session.resume()?;
            terminal.clear()?;
            match result {
                Ok(InlineLoginOutcome::Estelle(login::LoginOutcome::Rejected)) => {
                    app.transcript.push(TranscriptEntry::System(
                        "Estelle rejected the credential; the previous credential was left unchanged."
                            .to_string(),
                    ));
                }
                Ok(InlineLoginOutcome::Estelle(_)) => {
                    app.auth_resolved = false;
                    spawn_credential_resolution(&tx);
                }
                Ok(InlineLoginOutcome::Claude) => {
                    app.transcript.push(TranscriptEntry::System(
                        "Claude Code credential imported into Estelle's own store; Claude Code's source was left unchanged. Provider runtime binding is not yet proven; run /doctor."
                            .to_string(),
                    ));
                    if app.client.is_none() {
                        app.picker = Some(PickerSurface::login());
                    }
                }
                Ok(InlineLoginOutcome::Chatgpt) => {
                    app.transcript.push(TranscriptEntry::System(
                        "ChatGPT credential flow completed inside this session.".to_string(),
                    ));
                    if app.client.is_none() {
                        app.picker = Some(PickerSurface::login());
                    }
                }
                Ok(InlineLoginOutcome::Provider(provider)) => {
                    app.transcript.push(TranscriptEntry::System(format!(
                        "{provider} credential stored without exposing its value."
                    )));
                    app.auth_resolved = false;
                    spawn_credential_resolution(&tx);
                }
                Err(error) => app.transcript.push(TranscriptEntry::System(format!(
                    "Credential flow did not complete: {error}. Run /doctor."
                ))),
            }
            if app.login_required && app.client.is_none() && app.picker.is_none() {
                app.picker = Some(PickerSurface::login());
            }
        }
        if app.should_exit {
            break;
        }
    }
    record_session_checkpoint(app.root.clone()).await;
    Ok(())
}

async fn login_failure(error: &dyn std::fmt::Display) -> ExitCode {
    let mut stdout = tokio::io::stdout();
    let message = format!("Login did not complete: {error}\nRun estelle doctor.\n");
    let _ = stdout.write_all(message.as_bytes()).await;
    ExitCode::FAILURE
}

async fn run_provider_login(
    provider: &str,
    base_url: Option<&str>,
    model: Option<&str>,
    label: Option<&str>,
) -> io::Result<()> {
    let descriptor = provider_catalog::resolve(provider)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown provider"))?;
    if descriptor.auth == provider_catalog::AuthKind::LocalEndpoint {
        return local_provider::run(provider, base_url, model);
    }
    let prompted_base = if base_url.is_none() && descriptor.requires_base_url() {
        login::read_plain_value(b"Provider API base URL: ")?
    } else {
        None
    };
    let route = provider_catalog::login_route(provider, base_url.or(prompted_base.as_deref()))?;
    match route.provider.auth {
        provider_catalog::AuthKind::ClaudeImport => claude_import::run(),
        provider_catalog::AuthKind::ChatgptDevice => login::run_chatgpt().await,
        provider_catalog::AuthKind::ApiKey => {
            let server_provider = route
                .provider
                .server_provider
                .ok_or_else(|| io::Error::other("provider key route has no server identity"))?;
            provider_keys::run(server_provider, route.base_url.as_deref(), model, label).await
        }
        provider_catalog::AuthKind::CopilotDevice => copilot_login::run().await,
        provider_catalog::AuthKind::LocalEndpoint => unreachable!("handled before route dispatch"),
    }
}

async fn command_failure(message: impl std::fmt::Display) -> ExitCode {
    let mut stderr = tokio::io::stderr();
    let _ = stderr.write_all(format!("{message}\n").as_bytes()).await;
    ExitCode::FAILURE
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    if let Some(Command::Login {
        chatgpt,
        provider,
        base_url,
        model,
        label,
    }) = &args.command
    {
        if *chatgpt {
            return match login::run_chatgpt().await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => login_failure(&error).await,
            };
        }
        if let Some(provider) = provider {
            let result = run_provider_login(
                provider,
                base_url.as_deref(),
                model.as_deref(),
                label.as_deref(),
            )
            .await;
            return match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => login_failure(&error).await,
            };
        }
        return match login::run().await {
            Ok(login::LoginOutcome::Rejected) => {
                login_failure(&"Estelle rejected the credential").await
            }
            Err(error) => login_failure(&error).await,
            Ok(_) => ExitCode::SUCCESS,
        };
    }
    if matches!(args.command, Some(Command::Doctor)) {
        let lines = doctor::lines(doctor::Context::Shell);
        let mut stdout = tokio::io::stdout();
        return if stdout
            .write_all(format!("{}\n", lines.join("\n")).as_bytes())
            .await
            .is_ok()
        {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(Command::Serve { socket }) = args.command.clone() {
        let socket = match socket
            .map(Ok)
            .unwrap_or_else(session_server::default_socket_path)
        {
            Ok(socket) => socket,
            Err(error) => return command_failure(error).await,
        };
        let (client, _auth) = match resolve_credential() {
            Ok(resolved) => resolved,
            Err(error) => {
                return command_failure(format!("session server needs login: {error}")).await;
            }
        };
        let server = match session_server::SessionServer::bind(socket.clone(), client).await {
            Ok(server) => server,
            Err(error) => return command_failure(error).await,
        };
        let mut stdout = tokio::io::stdout();
        if stdout
            .write_all(
                format!("Estelle session server listening at {}\n", socket.display()).as_bytes(),
            )
            .await
            .is_err()
        {
            return ExitCode::FAILURE;
        }
        let shutdown = CancellationToken::new();
        let signal_shutdown = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                signal_shutdown.cancel();
            }
        });
        return match server.run(shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => command_failure(error).await,
        };
    }
    if let Some(Command::Connect {
        client: None,
        socket,
        session,
    }) = args.command.clone()
    {
        let socket = match socket
            .map(Ok)
            .unwrap_or_else(session_server::default_socket_path)
        {
            Ok(socket) => socket,
            Err(error) => return command_failure(error).await,
        };
        let mut tui_args = args;
        tui_args.command = None;
        return match run(tui_args, Some(socket), Some(session)).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => command_failure(error).await,
        };
    }
    if matches!(args.command, Some(Command::Acp)) {
        let result = async {
            let store = CredentialStore::default_location()?;
            let credential = store.resolve()?;
            let client = Client::production(credential.api_key)?;
            let repo_override = args.repo.as_deref().and_then(Repo::new);
            estelle_acp::run_stdio(client, repo_override)
                .await
                .map_err(|error| Error::CredentialStore(error.to_string()))
        }
        .await;
        return if result.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if matches!(args.command, Some(Command::McpServer)) {
        let result = async {
            let store = CredentialStore::default_location().map_err(|error| error.to_string())?;
            let credential = store.resolve().map_err(|error| error.to_string())?;
            let client =
                Client::production(credential.api_key).map_err(|error| error.to_string())?;
            let root = std::env::current_dir().map_err(|error| error.to_string())?;
            let repo = RepoResolver::new(args.repo.as_deref().and_then(Repo::new), root)
                .resolve()
                .ok_or_else(|| {
                    "the current directory does not resolve to a repository".to_string()
                })?;
            estelle_mcp::serve_stdio(client, repo)
                .await
                .map_err(|error| error.to_string())
        }
        .await;
        return if result.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(Command::Mcp {
        call,
        arguments,
        command,
    }) = args.command.clone()
    {
        let outcome = match call {
            Some(tool) => {
                match serde_json::from_str::<serde_json::Map<String, Value>>(&arguments) {
                    Ok(arguments) => estelle_mcp::call_stdio(command, tool, arguments)
                        .await
                        .map(|value| vec![value.to_string()]),
                    Err(error) => Err(error.into()),
                }
            }
            None => estelle_mcp::inspect_stdio(command).await.map(|names| {
                names
                    .into_iter()
                    .map(|name| format!("tool  {name}"))
                    .collect()
            }),
        };
        let (lines, code) = match outcome {
            Ok(lines) => (lines, ExitCode::SUCCESS),
            Err(error) => (
                vec![format!("MCP client failed: {error}")],
                ExitCode::FAILURE,
            ),
        };
        let mut stdout = tokio::io::stdout();
        return if stdout
            .write_all(format!("{}\n", lines.join("\n")).as_bytes())
            .await
            .is_ok()
        {
            code
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(command) = args.command.clone() {
        let root = std::env::current_dir().unwrap_or_default();
        let repo = RepoResolver::new(args.repo.as_deref().and_then(Repo::new), &root)
            .resolve()
            .or_else(|| args.repo.as_deref().and_then(Repo::new))
            .unwrap_or_default();
        let outcome = top_level::run(command, repo, &root).await;
        let (lines, code) = match outcome {
            Ok(lines) => (lines, ExitCode::SUCCESS),
            Err(error) => (
                vec![
                    format!("Estelle command failed: {error}"),
                    "The command did not complete its requested operation.".to_string(),
                    "Correct the command or account state, then retry.".to_string(),
                ],
                ExitCode::FAILURE,
            ),
        };
        let mut stdout = tokio::io::stdout();
        let body = format!("{}\n", lines.join("\n"));
        return if stdout.write_all(body.as_bytes()).await.is_ok() {
            code
        } else {
            ExitCode::FAILURE
        };
    }
    match run(args, None, None).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use serde_json::json;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_json;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    fn test_app() -> App {
        let mut app = App::new(Args {
            command: None,
            repo: Some("uqeu/estelle".to_string()),
        });
        app.boot = None;
        app
    }

    #[test]
    fn serve_and_connect_are_distinct_session_runtime_commands() {
        let served =
            Args::try_parse_from(["estelle", "serve", "--socket", "/tmp/estelle-session.sock"])
                .expect("serve command");
        assert!(matches!(
            served.command,
            Some(Command::Serve { socket: Some(path) })
                if path.as_path() == std::path::Path::new("/tmp/estelle-session.sock")
        ));

        let connected = Args::try_parse_from(["estelle", "connect", "--session", "payments"])
            .expect("connect command");
        assert!(matches!(
            connected.command,
            Some(Command::Connect {
                client: None,
                socket: None,
                session,
            })
            if session == "payments"
        ));
    }

    #[test]
    fn reconnect_snapshot_replays_completed_work_and_restores_active_work() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();
        app.handle_ui_event(
            UiEvent::Session(session_server::ServerMessage::Snapshot {
                session_id: "main".to_string(),
                sessions: vec![
                    session_server::SessionSummary {
                        id: "main".to_string(),
                        active: true,
                        turn_count: 1,
                    },
                    session_server::SessionSummary {
                        id: "retries".to_string(),
                        active: false,
                        turn_count: 4,
                    },
                ],
                turns: vec![session_server::SessionTurn {
                    id: 41,
                    input: session_server::SessionInput::Question {
                        question: "where does charge fail?".to_string(),
                    },
                    outcome: session_server::SessionOutcome::Answer {
                        answer: session_server::WireAnswer {
                            text: "The retry loop has no ceiling.".to_string(),
                            grounded: Some(true),
                            degraded: false,
                            sources: vec![session_server::WireSource {
                                file: "api/charge.ts".to_string(),
                                line: Some(52),
                                extra: serde_json::Map::new(),
                            }],
                            working_paths: Vec::new(),
                        },
                    },
                }],
                active: Some(session_server::ActiveTurn {
                    id: 42,
                    input: session_server::SessionInput::Question {
                        question: "verify the retry fix".to_string(),
                    },
                }),
                file_shifts: vec![session_server::FileShiftNotice {
                    id: 1,
                    path: PathBuf::from("api/charge.ts"),
                    changed_by: "retries".to_string(),
                    summary: Some("edited lines 48-60".to_string()),
                }],
                fleet: None,
            }),
            &tx,
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 30);
        assert!(rendered.contains("where does charge fail?"));
        assert!(rendered.contains("The retry loop has no ceiling."));
        assert!(rendered.contains("api/charge.ts:52"));
        assert!(rendered.contains("verify the retry fix"));
        assert!(rendered.contains("SESSIONS"));
        assert!(rendered.contains("main"));
        assert!(rendered.contains("retries"));
        assert!(rendered.contains("Alt+Left/Right switch"));
        assert!(rendered.contains("FILE SHIFT"));
        assert!(rendered.contains("retries changed a file this session read"));
        assert!(app.transcript.iter().any(
            |entry| matches!(entry, TranscriptEntry::System(text) if text.contains("edited lines 48-60"))
        ));
        assert_eq!(app.active.as_ref().map(|active| active.id), Some(42));
    }

    #[test]
    fn five_worker_tabs_render_and_closing_one_sends_only_a_view_switch() {
        let mut app = test_app();
        let (session, mut frames) = session_server::SessionHandle::test_channel();
        app.session = Some(session);
        app.session_id = "worker-3".to_string();
        app.session_tabs = (1..=5)
            .map(|index| session_server::SessionSummary {
                id: format!("worker-{index}"),
                active: true,
                turn_count: 0,
            })
            .collect();
        app.active = Some(ActiveRequest {
            id: 73,
            label: "thinking".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });

        let before = rendered_frame_at_size(&app, Instant::now(), 160, 30);
        for index in 1..=5 {
            assert!(before.contains(&format!("worker-{index}")));
        }

        app.close_session_tab();

        assert!(app.hidden_session_tabs.contains("worker-3"));
        assert_eq!(app.session_id, "worker-4");
        assert!(matches!(
            frames.try_recv(),
            Ok(session_server::ClientFrame::Switch { session_id }) if session_id == "worker-4"
        ));
        assert!(
            frames.try_recv().is_err(),
            "closing a view must not send a cancel request"
        );
        let after = rendered_frame_at_size(&app, Instant::now(), 160, 30);
        assert!(!after.contains("worker-3"));
        for index in [1, 2, 4, 5] {
            assert!(after.contains(&format!("worker-{index}")));
        }
    }

    #[test]
    fn provider_login_parses_the_provider_and_optional_routing_metadata() {
        let args = Args::try_parse_from([
            "estelle",
            "login",
            "--provider",
            "anthropic",
            "--model",
            "claude-opus",
            "--label",
            "production",
        ])
        .expect("keys set command");

        assert!(matches!(
            args.command,
            Some(Command::Login {
                chatgpt: false,
                provider: Some(provider),
                base_url: None,
                model: Some(model),
                label: Some(label),
            }) if provider == "anthropic" && model == "claude-opus" && label == "production"
        ));
    }

    #[test]
    fn provider_login_routes_are_explicit_and_unknown_names_never_reach_the_key_api() {
        assert_eq!(
            provider_catalog::login_route("claude", None)
                .expect("Claude route")
                .provider
                .auth,
            provider_catalog::AuthKind::ClaudeImport
        );
        assert_eq!(
            provider_catalog::login_route("openai", None)
                .expect("ChatGPT route")
                .provider
                .auth,
            provider_catalog::AuthKind::ChatgptDevice
        );
        assert_eq!(
            provider_catalog::login_route("openai-api", None)
                .expect("OpenAI key route")
                .provider
                .server_provider,
            Some("openai")
        );
        assert!(provider_catalog::login_route("openai-compatible", None).is_err());
        assert_eq!(
            provider_catalog::login_route("copilot", None)
                .expect("Copilot route")
                .provider
                .auth,
            provider_catalog::AuthKind::CopilotDevice
        );
        assert!(provider_catalog::login_route("made-up-provider", None).is_err());
    }

    fn rendered_frame(app: &App, now: Instant) -> String {
        rendered_frame_at_size(app, now, 80, 24)
    }

    fn rendered_frame_at_size(app: &App, now: Instant, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_frame(frame, app, now))
            .expect("render frame");
        format!("{}", terminal.backend())
    }

    fn rendered_buffer_at_size(
        app: &App,
        now: Instant,
        width: u16,
        height: u16,
    ) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_frame(frame, app, now))
            .expect("render frame");
        terminal.backend().buffer().clone()
    }

    #[tokio::test]
    async fn actual_renderer_gallery_covers_the_product_surfaces() {
        let output = std::env::var_os("ESTELLE_ACTUAL_GALLERY_DIR").map(|path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
            }
        });
        let now = Instant::now();
        let mut names = Vec::new();
        let mut capture = |name: &'static str, app: &App, width: u16, height: u16, needle: &str| {
            let buffer = rendered_buffer_at_size(app, now, width, height);
            let text = test_gallery::buffer_text(&buffer);
            assert!(
                text.contains(needle),
                "{name} did not render expected text {needle:?}\n{text}"
            );
            if let Some(output) = output.as_deref() {
                test_gallery::write_frame(output, name, &buffer);
            }
            names.push(name);
        };

        let mut boot = test_app();
        boot.boot = Some(BootScene::new(0));
        boot.boot_started = now
            .checked_sub(Duration::from_millis(codex_tui::boot_scene::CONDENSE_MS))
            .expect("gallery boot clock");
        capture("00-boot", &boot, 120, 34, "by Fate Labs");

        let mut home = test_app();
        home.auth_resolved = true;
        home.account = Some(
            serde_json::from_value(json!({
                "email": "khai@fatelabs.ca",
                "plan": "team",
                "seats": 6,
                "team": {
                    "id": "team-fate",
                    "name": "Fate Labs",
                    "role": "owner",
                    "is_admin": true,
                    "is_owner": true
                }
            }))
            .expect("gallery account"),
        );
        home.session_context = Some(session_gap::SessionContext {
            human_lines: vec![
                "Welcome back - you were last here about 3 hours ago.".to_string(),
                "Code you touched has changed since:".to_string(),
                "- billing/charge.rs - by Dana, about 2 hours ago - bound retry telemetry"
                    .to_string(),
            ],
            model_context: "Welcome back - you were last here about 3 hours ago.\nCode you touched has changed since:\n- billing/charge.rs - by Dana, about 2 hours ago - bound retry telemetry".to_string(),
        });
        home.client = Some(
            Client::new(
                "http://127.0.0.1:9/",
                estelle_client::ApiKey::new("estelle_test_key").expect("gallery key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("gallery client"),
        );
        home.prod_overview = Some(
            serde_json::from_value(json!({
                "series": {
                    "window_s": 3600,
                    "bucket_s": 300,
                    "requests_source": "monitor_ingest",
                    "buckets": [
                        {"t": 1, "errors": 1, "requests": 812},
                        {"t": 2, "errors": 4, "requests": 829},
                        {"t": 3, "errors": 2, "requests": 807},
                        {"t": 4, "errors": 7, "requests": 844}
                    ]
                },
                "uptime": {"checks": 4, "up": 4, "down": 0}
            }))
            .expect("gallery overview"),
        );
        home.prod_issues = Some(
            serde_json::from_value(json!({"issues": [], "has_more": false}))
                .expect("gallery empty issues"),
        );
        capture("01-startup-home", &home, 160, 38, "ASK ESTELLE");

        let mut waiting = test_app();
        waiting.auth_resolved = true;
        waiting.account = home.account.clone();
        waiting.session_context = home.session_context.clone();
        waiting.client = home.client.clone();
        let (tx, _rx) = mpsc::unbounded_channel();
        waiting.submit("What changed while I was away?".to_string(), &tx);
        waiting.active = Some(ActiveRequest {
            id: 83,
            label: "thinking".to_string(),
            started: now
                .checked_sub(Duration::from_secs(83))
                .expect("gallery waiting clock"),
            cancel: CancellationToken::new(),
        });
        capture(
            "01b-waiting-answer",
            &waiting,
            160,
            38,
            "no response received yet",
        );

        let mut calm = test_app();
        calm.prod_panel_visible = false;
        calm.header.indexed = Some(true);
        capture(
            "11-empty-state-dither",
            &calm,
            100,
            32,
            "Ask about uqeu/estelle",
        );

        let mut orchestra = test_app();
        orchestra.prod_panel_visible = false;
        orchestra.context_panel_visible = true;
        orchestra.header.indexed = Some(true);
        orchestra.header.files = Some(1_993);
        orchestra.citations = vec![Source {
            file: "billing/charge.rs".to_string(),
            line: Some(82),
            extra: serde_json::Map::from_iter([(
                "symbol".to_string(),
                Value::String("charge_card".to_string()),
            )]),
        }];
        orchestra.working_memory_paths = vec![
            "billing/charge.rs · local, not pushed".to_string(),
            "billing/retry.rs · local, not pushed".to_string(),
        ];
        orchestra.fleet = Some(
            serde_json::from_value(json!({
                "id": "orch-8",
                "batch": "Trace checkout failures",
                "models": ["Claude Opus 4.1", "GPT-5.5", "Gemini 2.5 Pro"],
                "state": "running",
                "revision": 17,
                "observed_at": 4102444800.0,
                "completed": 11,
                "total": 24,
                "attempt": "first",
                "narrator": {
                    "text": "8 agents tracing 24 production assignments across three model pools",
                    "evidence": "observed"
                },
                "agents": [
                    {"index": 1, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Bound checkout_timeout to billing/charge.rs:82", "progress": {"completed": 3, "total": 3}},
                    {"index": 2, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Reading the retry gate", "progress": {"completed": 2, "total": 4}},
                    {"index": 3, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Grouping deploy-correlated events", "progress": {"completed": 1, "total": 3}},
                    {"index": 4, "status": "queued", "state_observed_at": 4102444800.0, "current_action": null, "progress": {"completed": 0, "total": 2}},
                    {"index": 5, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Verified the symbol range", "progress": {"completed": 4, "total": 4}},
                    {"index": 6, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Comparing the proposed patch", "progress": {"completed": 1, "total": 3}},
                    {"index": 7, "status": "unknown", "state_observed_at": 4102444800.0, "unknown_reason": "worker state not reported", "current_action": null},
                    {"index": 8, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Checking the regression suite", "progress": {"completed": 0, "total": 2}}
                ]
            }))
            .expect("active orchestra"),
        );
        capture(
            "02-orchestra-active",
            &orchestra,
            180,
            34,
            "Estelle Orchestra",
        );

        let mut completed = test_app();
        completed.prod_panel_visible = false;
        completed.context_panel_visible = true;
        completed.header.indexed = Some(true);
        completed.header.files = Some(1_993);
        completed.citations = orchestra.citations.clone();
        completed.working_memory_paths = orchestra.working_memory_paths.clone();
        completed.fleet = Some(
            serde_json::from_value(json!({
                "id": "orch-8",
                "batch": "Trace checkout failures",
                "models": ["Claude Opus 4.1", "GPT-5.5", "Gemini 2.5 Pro"],
                "state": "completed",
                "revision": 24,
                "observed_at": 4102444800.0,
                "completed": 8,
                "total": 8,
                "attempt": "first",
                "narrator": {"text": "All 8 agents reported terminal outcomes", "evidence": "measured"},
                "agents": [
                    {"index": 1, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Bound checkout_timeout", "progress": {"completed": 3, "total": 3}},
                    {"index": 2, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Verified the retry gate", "progress": {"completed": 4, "total": 4}},
                    {"index": 3, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Grouped the production events", "progress": {"completed": 3, "total": 3}},
                    {"index": 4, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Checked the proposed repair", "progress": {"completed": 2, "total": 2}},
                    {"index": 5, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Resolved the symbol range", "progress": {"completed": 4, "total": 4}},
                    {"index": 6, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Compared the proposed patch", "progress": {"completed": 3, "total": 3}},
                    {"index": 7, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Verified the worker result", "progress": {"completed": 2, "total": 2}},
                    {"index": 8, "status": "completed", "state_observed_at": 4102444800.0, "current_action": "Checked the regression suite", "progress": {"completed": 2, "total": 2}}
                ]
            }))
            .expect("completed orchestra"),
        );
        capture("03-orchestra-completed", &completed, 180, 30, "Completed");

        let mut issues = home;
        issues.prod_panel_visible = true;
        issues.prod_issues = Some(
            serde_json::from_value(json!({
                "issues": [{
                    "key": "checkout-timeout",
                    "status": "unresolved",
                    "events_in_window": 47,
                    "signal": {"title": "Checkout timeout after deploy", "error_type": "TimeoutError", "count": 47},
                    "bound": {"symbol": "charge_card", "status": "bound", "file": "billing/charge.rs", "line": 82},
                    "repair": {"status": "proposed", "detail": "bounded retry", "pr": null},
                    "gate": null,
                    "gate_absent_reason": "awaiting the proposed patch"
                }]
            }))
            .expect("gallery issue"),
        );
        capture(
            "04-production-issues",
            &issues,
            160,
            38,
            "billing/charge.rs:82",
        );

        let mut diff = test_app();
        diff.prod_panel_visible = false;
        diff.diff_panel_visible = true;
        diff.last_diff = Some(
            "diff --git a/billing/charge.rs b/billing/charge.rs\n@@ -82 +82 @@\n-old()\n+retry_after()\n"
                .to_string(),
        );
        capture("05-proposed-diff", &diff, 150, 34, "WORK DRAFT");

        let mut slash = test_app();
        slash.prod_panel_visible = false;
        slash.composer.set_text("/m");
        // "/me" is the shortest "m" command, so it heads the palette; the needle pins the
        // palette-open state with the first match selected, not a particular command's rank.
        capture("06-slash-palette", &slash, 130, 38, "> /me");

        let mut settings = test_app();
        settings.prod_panel_visible = false;
        settings.settings = Some(
            serde_json::from_value(json!({
                "schema": {
                    "code": [{
                        "key": "formatter", "scope": "team", "type": "enum",
                        "default": "repository", "label": "Formatter",
                        "options": ["repository", "disabled"], "reader": "server"
                    }],
                    "monitor": [{
                        "key": "retention_days", "scope": "team", "type": "int",
                        "default": 30, "label": "Data retention (days)",
                        "minimum": 1, "maximum": 3650, "reader": "server"
                    }],
                    "review": [],
                    "repair": [{
                        "key": "draft_pr", "scope": "team", "type": "bool",
                        "default": true, "label": "Draft reviewable PRs", "reader": "server"
                    }],
                    "prod": [],
                    "guardian": [],
                    "research": [],
                    "memory": [],
                    "agent": [],
                    "global": [{
                        "key": "theme", "scope": "personal", "type": "enum",
                        "default": "dark", "label": "Theme",
                        "options": ["dark", "cream"], "reader": "server"
                    }]
                },
                "team": {
                    "monitor": {"retention_days": 45},
                    "repair": {"draft_pr": true}
                },
                "personal": {"global": {"theme": "dark"}}
            }))
            .expect("gallery settings"),
        );
        settings.picker = Some(PickerSurface::settings(&settings));
        capture("07-settings", &settings, 130, 38, "Monitor");

        let mut monitor_settings = test_app();
        monitor_settings.prod_panel_visible = false;
        monitor_settings.settings = settings.settings.clone();
        monitor_settings.picker = Some(PickerSurface::suite(&monitor_settings, "monitor"));
        capture(
            "07b-monitor-settings",
            &monitor_settings,
            130,
            34,
            "Data retention (days)",
        );

        let model_reply: CommandReply = serde_json::from_value(json!({
            "providers": [
                {"id": "anthropic", "label": "Anthropic", "models": ["claude-opus-4.1", "claude-sonnet-4.5"]},
                {"id": "openai", "label": "OpenAI", "models": ["gpt-5.5", "gpt-5.5-codex"]}
            ],
            "active": {"provider": "anthropic", "model": "claude-opus-4.1"}
        }))
        .expect("gallery model pool");
        let mut models = test_app();
        models.prod_panel_visible = false;
        models.picker = Some(PickerSurface::model(&model_reply));
        capture(
            "08-model-picker",
            &models,
            130,
            34,
            "MODEL POOL · ACCOUNT-WIDE",
        );

        let mut cream = test_app();
        cream.prod_panel_visible = false;
        cream.header.indexed = Some(true);
        cream.theme = Theme::CreamInk;
        capture("13-cream-ink", &cream, 120, 34, "ASK ESTELLE");

        let mut autonomy = test_app();
        autonomy.prod_panel_visible = false;
        autonomy.server_mode = Some("read_only".to_string());
        autonomy.picker = Some(PickerSurface::autonomy(&autonomy));
        capture("14-autonomy", &autonomy, 130, 34, "guarded merge");

        let skills_reply: CommandReply = serde_json::from_value(json!({
            "skills": [
                {"name": "review", "summary": "Review the current change against production evidence"},
                {"name": "trace", "summary": "Trace an issue to a bound repository symbol"},
                {"name": "ground", "summary": "Check an answer against the current repo graph"}
            ]
        }))
        .expect("gallery skills");
        let mut skills = test_app();
        skills.prod_panel_visible = false;
        skills.picker = Some(PickerSurface::skills(&skills_reply));
        capture("12-skills", &skills, 130, 34, "SKILLS");

        let todo: estelle_client::TodoSnapshot = serde_json::from_value(json!({
            "observed_at": 4102444800.0,
            "items": [
                {"title": "Bind checkout timeout", "status": "done", "result": "billing/charge.rs:82", "evidence": "measured"},
                {"title": "Run the gate regression", "status": "done", "result": "18/18 passed", "evidence": "measured"},
                {"title": "Inspect the proposed diff", "status": "in_progress", "result": "2 files", "evidence": "observed"},
                {"title": "Open a reviewable PR", "status": "pending", "evidence": "observed"},
                {"title": "Confirm the account model", "status": "unknown", "evidence": "unknown"},
                {"title": "Trace the deploy", "status": "done", "result": "deploy-8e17", "evidence": "observed"},
                {"title": "Check GitHub connection", "status": "pending", "evidence": "observed"},
                {"title": "Record the final verdict", "status": "pending", "evidence": "observed"}
            ]
        }))
        .expect("gallery todo");
        let mut todo_expanded = test_app();
        todo_expanded.prod_panel_visible = false;
        todo_expanded.todo = Some(todo.clone());
        todo_expanded.todo_visible = true;
        todo_expanded.todo_expanded = true;
        capture(
            "09-todo-expanded",
            &todo_expanded,
            130,
            36,
            "ctrl+t to collapse",
        );

        let mut todo_collapsed = test_app();
        todo_collapsed.prod_panel_visible = false;
        todo_collapsed.todo = Some(todo);
        todo_collapsed.todo_visible = true;
        capture(
            "10-todo-collapsed",
            &todo_collapsed,
            130,
            34,
            "ctrl+t to expand",
        );

        if let Some(output) = output.as_deref() {
            test_gallery::write_index(output, &names);
        }
    }

    #[test]
    fn snapshot_empty_composer() {
        let app = test_app();
        insta::assert_snapshot!(rendered_frame(&app, Instant::now()));
    }

    #[test]
    fn first_frame_teaches_real_estelle_actions_without_codex_copy() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.header.indexed = Some(true);

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);

        assert!(rendered.contains("Ask Estelle"));
        assert!(rendered.contains("Ask about uqeu/estelle"));
        assert!(rendered.contains("/sweep"));
        assert!(rendered.contains("/review"));
        assert!(rendered.contains("?"));
        assert!(!rendered.contains("Compose new task"));
    }

    #[test]
    fn ctrl_t_expands_the_server_emitted_todo_surface() {
        let mut app = test_app();
        app.todo = Some(serde_json::from_value(json!({
            "observed_at": 4102444800.0,
            "items": [{"title": "Keep the measured result", "status": "done", "result": "10/10", "evidence": "measured"}]
        }))
        .expect("typed todo"));
        app.todo_visible = true;
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            &tx,
        );

        assert!(app.todo_expanded);
        assert!(
            rendered_frame_at_size(&app, Instant::now(), 120, 30)
                .contains("Keep the measured result — 10/10")
        );
    }

    #[test]
    fn fleet_progress_colour_boundary_encodes_the_completed_fraction() {
        let line = styled_fleet_progress_line("◐ Working... [━━━━────] 2/4".to_string());
        assert!(
            line.spans
                .iter()
                .any(|span| span.style.fg == Some(Color::Green) && span.content.contains("━━━━"))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.style.fg == Some(Color::Blue) && span.content.contains("────"))
        );
    }

    #[test]
    fn fleet_terminal_glyphs_have_distinct_colours_as_well_as_shapes() {
        let line = styled_fleet_agent_line(
            "001 ✓ Completed  002 × Failed  003 ◷ Timed out  004 ■ Killed  005 ? Lost".to_string(),
        );
        let colours = line
            .spans
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<std::collections::HashSet<_>>();
        assert!(colours.contains(&Color::Green));
        assert!(colours.contains(&Color::Red));
        assert!(colours.contains(&Color::Yellow));
        assert!(colours.contains(&Color::Magenta));
        assert!(colours.contains(&Color::Cyan));
    }

    #[test]
    fn question_mark_opens_real_shortcuts_without_enter() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
            &tx,
        );

        let rendered = format!("{:?}", render_transcript(&app.transcript));
        assert!(rendered.contains("/help"));
        assert!(rendered.contains("/sweep"));
        assert!(app.composer.is_empty());
    }

    #[test]
    fn slash_palette_arrow_keys_move_the_selected_command_and_tab_uses_it() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();
        app.composer.set_text("/");
        let rows = commands::palette_rows("/");
        assert!(rows.len() >= 3);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(rendered.contains(&format!("> /{}", rows[2].0)));

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &tx,
        );
        assert_eq!(app.composer.text(), format!("/{} ", rows[2].0));
    }

    #[test]
    fn settings_is_an_arrow_key_picker_with_explicit_setting_owners() {
        let mut app = test_app();
        app.auth_resolved = true;
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("/settings".to_string(), &tx);
        let opened = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(opened.contains("SETTINGS"));
        assert!(opened.contains("> 1 Mode"));
        assert!(opened.contains("server enforced"));
        assert!(opened.contains("client display"));

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );
        let moved = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(moved.contains("> 2 Theme"));
    }

    #[test]
    fn settings_front_door_lists_all_ten_server_suites_even_when_some_are_empty() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();
        let schema = json!({
            "code": [],
            "monitor": [{
                "key": "retention_days", "scope": "team", "type": "int",
                "default": 30, "label": "Data retention (days)",
                "options": [], "minimum": 1, "maximum": 3650, "reader": "server"
            }],
            "review": [], "repair": [], "prod": [], "guardian": [],
            "research": [], "memory": [], "agent": [], "global": []
        });
        let reply = serde_json::from_value(json!({
            "schema": schema,
            "team": {},
            "personal": {}
        }))
        .expect("settings response");
        app.handle_ui_event(UiEvent::Settings(Ok(reply)), &tx);

        let picker = PickerSurface::settings(&app);
        let labels = picker
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>();

        for suite in [
            "Code", "Monitor", "Review", "Repair", "Prod", "Guardian", "Research", "Memory",
            "Agent", "Global",
        ] {
            assert!(labels.contains(&suite), "missing {suite}: {labels:?}");
        }
    }

    #[test]
    fn auto_mode_names_guarded_merge_without_claiming_deployment() {
        let mut app = test_app();
        app.server_mode = Some("read_only".to_string());
        let picker = PickerSurface::autonomy(&app);
        let auto = picker
            .rows
            .iter()
            .find(|row| row.label == "auto")
            .expect("auto autonomy row");

        assert!(auto.detail.contains("guarded merge"));
        assert!(auto.detail.contains("reviewable PR"));
        assert!(!auto.detail.contains("deploy"));

        let confirmation = PickerSurface::confirm_mode("execute");
        assert!(confirmation.rows[0].detail.contains("does not deploy"));
    }

    #[test]
    fn header_leaves_the_active_surface_name_to_its_bordered_pane() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.fleet = Some(
            serde_json::from_value(json!({
                "id": "orch-header",
                "batch": "Trace checkout failures",
                "models": ["Claude Opus 4.1", "Gemini 2.5 Pro"],
                "state": "running",
                "revision": 1,
                "observed_at": 4102444800.0,
                "completed": 0,
                "total": 2,
                "agents": []
            }))
            .expect("header fleet"),
        );

        let header = header_line(&app, 160)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(header.contains("ESTELLE  ·  uqeu/estelle"));
        assert!(!header.contains("ORCHESTRA"));
        assert!(!header.contains("CONVERSATION"));
        assert!(!header.contains("Claude"));
        assert!(!header.contains("Gemini"));
    }

    #[test]
    fn account_identity_never_claims_a_prepaid_balance() {
        let mut app = test_app();
        app.account = Some(
            serde_json::from_value(json!({
                "email": "dev@fatelabs.ca",
                "plan": "ultra",
                "balance_usd": 84.20
            }))
            .expect("account fixture"),
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 34);
        assert!(!rendered.to_ascii_lowercase().contains("prepaid"));
        assert!(!rendered.contains("$84.20"));
    }

    #[test]
    fn header_never_repeats_transient_surface_names() {
        let mut app = test_app();
        app.composer.set_text("/mo");

        let transient = header_line(&app, 120)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(!transient.contains("CONVERSATION"));
        assert!(!transient.contains("PRODUCTION"));

        app.composer.set_text("");
        app.picker = Some(PickerSurface::settings(&app));
        let settings = header_line(&app, 120)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(!settings.contains("CONVERSATION"));
        assert!(!settings.contains("PRODUCTION"));
    }

    #[tokio::test]
    async fn raising_autonomy_requires_confirmation_then_posts_the_account_ceiling() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/autonomy"))
            .and(body_json(json!({"level": "propose"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "autonomy": "propose"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut app = test_app();
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("estelle_live_picker-test").expect("key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("client"),
        );
        app.server_mode = Some("read_only".to_string());
        app.local_mode = "read_only".to_string();
        app.picker = Some(PickerSurface::settings(&app));
        let (tx, mut rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.title.as_str()),
            Some("Autonomy")
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );

        assert_eq!(app.server_mode.as_deref(), Some("read_only"));
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.title.as_str()),
            Some("Confirm raise to accept-edits")
        );

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("autonomy response")
            .expect("event");
        app.handle_ui_event(event, &tx);
        assert_eq!(app.server_mode.as_deref(), Some("propose"));
        assert_eq!(app.local_mode, "propose");
    }

    #[tokio::test]
    async fn model_picker_selection_posts_the_exact_account_provider_choice() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/provider/select"))
            .and(body_json(json!({
                "provider": "anthropic",
                "model": "claude-opus"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "provider": "anthropic",
                "provider_model": "claude-opus"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut app = test_app();
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("estelle_live_model-test").expect("key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("client"),
        );
        let reply = serde_json::from_value(json!({
            "providers": [{"id": "anthropic", "label": "Anthropic", "models": ["claude-opus"]}],
            "active": {"provider": "openai", "model": "gpt-5.5"},
            "can_edit": true
        }))
        .expect("model pool");
        app.picker = Some(PickerSurface::model(&reply));
        let (tx, mut rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("provider response")
            .expect("event");
        app.handle_ui_event(event, &tx);

        assert_eq!(app.active_model, None);
        assert!(app.transcript.iter().any(|entry| {
            matches!(entry, TranscriptEntry::System(text) if text.contains("Anthropic") && text.contains("account-wide"))
        }));
    }

    #[test]
    fn theme_picker_switches_the_real_renderer_to_cream_ink() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.picker = Some(PickerSurface::settings(&app));
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &tx,
        );
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );

        assert_eq!(app.theme, Theme::CreamInk);
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 100, 28);
        assert!(buffer.content.iter().any(|cell| cell.bg == FATE_BG));
        assert!(buffer.content.iter().any(|cell| cell.fg == Color::Black));
    }

    #[test]
    fn cream_theme_never_paints_visible_text_in_its_background_colour() {
        let mut app = test_app();
        app.theme = Theme::CreamInk;
        app.prod_panel_visible = false;
        app.context_panel_visible = true;
        app.focus = FocusSurface::Auxiliary;
        app.citations = vec![Source {
            file: "billing/charge.rs".to_string(),
            line: Some(82),
            extra: serde_json::Map::new(),
        }];

        let buffer = rendered_buffer_at_size(&app, Instant::now(), 120, 32);
        let invisible = buffer.content.iter().filter(|cell| {
            !cell.symbol().trim().is_empty()
                && cell.fg == app.theme.background()
                && cell.bg == app.theme.background()
        });

        assert_eq!(invisible.count(), 0, "visible text used the canvas colour");
    }

    #[tokio::test]
    async fn cream_theme_persists_to_the_personal_global_settings_suite() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/settings/suite"))
            .and(body_json(json!({
                "suite": "global",
                "key": "theme",
                "value": "light",
                "scope": "personal"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "suite": "global",
                "key": "theme",
                "value": "light",
                "scope": "personal"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut app = test_app();
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("estelle_live_theme-test").expect("key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("client"),
        );
        app.picker = Some(PickerSurface::themes(&app));
        app.picker.as_mut().expect("theme picker").selected = 1;
        let (tx, mut rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("theme response")
            .expect("event");
        app.handle_ui_event(event, &tx);

        assert_eq!(app.theme, Theme::CreamInk);
        assert!(app.transcript.iter().any(|entry| {
            matches!(entry, TranscriptEntry::System(text) if text.contains("saved to personal settings"))
        }));
    }

    #[test]
    fn remote_model_and_skill_catalogues_open_walkable_picker_surfaces() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();
        app.active = Some(ActiveRequest {
            id: 51,
            label: "/model".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let model_reply = serde_json::from_value(json!({
            "providers": [
                {"id": "anthropic", "label": "Anthropic", "models": ["claude-opus"]},
                {"id": "openai", "label": "OpenAI", "models": ["gpt-5.5"]}
            ],
            "active": {"provider": "anthropic", "model": "claude-opus"}
        }))
        .expect("model pool");
        app.handle_ui_event(
            UiEvent::CommandAnswer {
                id: 51,
                name: "model",
                result: Ok(RemoteCommandReply {
                    reply: model_reply,
                    inspected_files: Vec::new(),
                }),
            },
            &tx,
        );

        let model = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(model.contains("MODEL POOL · ACCOUNT-WIDE"));
        assert!(model.contains("> 1 claude-opus"));
        assert!(model.contains("current"));
        assert!(model.contains("gpt-5.5"));

        app.picker = None;
        app.active = Some(ActiveRequest {
            id: 52,
            label: "/skills".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let skills_reply = serde_json::from_value(json!({
            "skills": [
                {"name": "review", "summary": "Review the current change"},
                {"name": "trace", "summary": "Trace a production signal"}
            ]
        }))
        .expect("skills");
        app.handle_ui_event(
            UiEvent::CommandAnswer {
                id: 52,
                name: "skills",
                result: Ok(RemoteCommandReply {
                    reply: skills_reply,
                    inspected_files: Vec::new(),
                }),
            },
            &tx,
        );

        let skills = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(skills.contains("SKILLS"));
        assert!(skills.contains("> 1 review"));
        assert!(skills.contains("trace"));
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        assert!(
            app.transcript.iter().any(
                |entry| matches!(entry, TranscriptEntry::User(text) if text == "/skill:review")
            )
        );
    }

    #[test]
    fn status_line_names_only_a_server_observed_model_and_marks_staleness() {
        let mut app = test_app();
        let observed = Instant::now();
        app.active_model = Some("claude-opus".to_string());
        app.active_model_observed_at = Some(observed);

        let fresh = format!("{:?}", status_line(&app, observed));
        assert!(fresh.contains("model claude-opus"));
        assert!(fresh.contains("observed"));
        assert!(!fresh.contains("model auto"));

        let stale = format!(
            "{:?}",
            status_line(&app, observed + Duration::from_secs(301))
        );
        assert!(stale.contains("stale"));
    }

    #[test]
    fn status_line_omits_unresolved_memory_and_connection_noise() {
        let app = test_app();
        let rendered = format!("{:?}", status_line(&app, Instant::now()));

        assert!(rendered.contains("plan"));
        assert!(rendered.contains("routing auto"));
        assert!(!rendered.contains("unavailable"));
        assert!(!rendered.contains("connecting"));
        assert!(!rendered.contains("    "));
    }

    #[test]
    fn long_running_request_reports_observed_wait_without_cache_speculation() {
        let mut app = test_app();
        let now = Instant::now();
        app.active = Some(ActiveRequest {
            id: 83,
            label: "thinking".to_string(),
            started: now - Duration::from_secs(83),
            cancel: CancellationToken::new(),
        });

        let rendered = format!("{:?}", status_line(&app, now));
        assert!(rendered.contains("still waiting for Estelle"));
        assert!(rendered.contains("1m 23s"));
        assert!(rendered.contains("no response received yet"));
        assert!(!rendered.contains("cache"));
    }

    #[test]
    fn live_fleet_grid_stays_fixed_above_a_scrolling_transcript() {
        let mut app = test_app();
        app.active = Some(ActiveRequest {
            id: 41,
            label: "/orchestra".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let reply: CommandReply = serde_json::from_value(json!({
            "fleet": {
                "id": "fleet-41",
                "batch": "Mutation lane detection",
                "model": "K3",
                "state": "running",
                "revision": 7,
                "observed_at": 4102444800.0,
                "stale_after_s": 60,
                "completed": 1,
                "total": 5,
                "agents": [
                    {"index": 1, "status": "running", "state_observed_at": 4102444800.0, "current_action": "Checking kill switch"},
                    {"index": 2, "status": "queued", "state_observed_at": 4102444800.0},
                    {"index": 3, "status": "done", "state_observed_at": 4102444800.0, "current_action": "Verified isolation"},
                    {"index": 4, "status": "blocked", "state_observed_at": 4102444800.0, "current_action": "Grounding refused"},
                    {"index": 5, "status": "needs_input", "state_observed_at": 4102444800.0, "current_action": "Needs repo"}
                ]
            }
        }))
        .expect("typed fleet snapshot");
        let (tx, _rx) = mpsc::unbounded_channel();
        app.handle_ui_event(
            UiEvent::CommandAnswer {
                id: 41,
                name: "orchestra",
                result: Ok(RemoteCommandReply {
                    reply,
                    inspected_files: Vec::new(),
                }),
            },
            &tx,
        );
        for index in 0..80 {
            app.transcript.push(TranscriptEntry::System(format!(
                "later transcript row {index}"
            )));
        }

        let rendered = rendered_frame_at_size(&app, Instant::now(), 180, 30);

        assert!(rendered.contains("Estelle Orchestra · Mutation lane detection ×5"));
        assert!(rendered.contains("Participants · K3"));
        assert!(rendered.contains("001"));
        assert!(rendered.contains("005"));
        assert!(rendered.contains("Working..."));
    }

    #[test]
    fn context_command_summons_a_persistent_grounding_side_panel() {
        let mut app = test_app();
        app.citations = vec![Source {
            file: "billing.py".to_string(),
            line: Some(88),
            extra: serde_json::Map::from_iter([(
                "symbol".to_string(),
                Value::String("charge_card".to_string()),
            )]),
        }];
        let (tx, _rx) = mpsc::unbounded_channel();
        app.submit("/context".to_string(), &tx);
        for index in 0..80 {
            app.transcript.push(TranscriptEntry::System(format!(
                "later transcript row {index}"
            )));
        }

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 30);

        assert!(rendered.contains("CONTEXT"));
        assert!(rendered.contains("Repo graph"));
        assert!(rendered.contains("billing.py:88"));
        assert!(rendered.contains("charge_card"));
        assert!(rendered.contains("Alt+M"));
        assert!(rendered.contains("/context"));
    }

    #[test]
    fn production_home_is_opt_in_and_every_empty_section_has_an_action() {
        let mut app = test_app();
        app.auth_resolved = true;

        let calm = rendered_frame_at_size(&app, Instant::now(), 140, 36);
        assert!(!calm.contains("LIVE PRODUCTION"));

        app.prod_panel_visible = true;

        let rendered = rendered_frame_at_size(&app, Instant::now(), 140, 36);

        assert!(rendered.contains("LIVE PRODUCTION"));
        assert!(rendered.contains("APP HEALTH"));
        assert!(rendered.contains("AGENT HEALTH"));
        assert!(rendered.contains("ESTELLE STATUS"));
        assert!(rendered.contains("ESTELLE QUEUE"));
        assert!(rendered.contains("Run /login"));
        assert!(!rendered.contains("0 errors"));
        assert!(!rendered.contains("healthy"));
    }

    #[test]
    fn production_home_renders_agent_health_without_inventing_null_counts() {
        let mut app = test_app();
        app.auth_resolved = true;
        app.prod_panel_visible = true;
        app.prod_agent_health = Some(
            serde_json::from_value(json!({
                "enabled": true,
                "observed_at": 1785203400.0,
                "stale_after_s": 120,
                "counts": {"reporting": 7, "degraded": 1, "silent": null},
                "agents": [{
                    "id": "checkout-agent",
                    "state": "degraded",
                    "events": 19,
                    "last_seen": 1785203370.0,
                    "current_signal": "tool timeout"
                }]
            }))
            .expect("agent health"),
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 140, 36);
        assert!(rendered.contains("7 reporting"), "{rendered}");
        assert!(rendered.contains("1 degraded"), "{rendered}");
        assert!(rendered.contains("silent unknown"), "{rendered}");
        assert!(rendered.contains("checkout-agent"), "{rendered}");
        assert!(rendered.contains("tool timeout"), "{rendered}");
        assert!(!rendered.contains("0 silent"), "{rendered}");
    }

    #[test]
    fn production_home_names_disabled_and_unknown_agent_measurements() {
        let mut app = test_app();
        app.auth_resolved = true;
        app.prod_panel_visible = true;
        app.prod_agent_health = Some(
            serde_json::from_value(json!({
                "enabled": false,
                "counts": null,
                "agents": []
            }))
            .expect("disabled health"),
        );
        let disabled = production_workspace_lines(&app)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(disabled.contains("Agent telemetry not enabled"));

        app.prod_agent_health = Some(
            serde_json::from_value(json!({
                "enabled": null,
                "enabled_absent_reason": "event store unavailable",
                "counts": null,
                "agents": []
            }))
            .expect("unknown health"),
        );
        let unknown = production_workspace_lines(&app)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(unknown.contains("event store unavailable"));
        assert!(!unknown.contains("0 reporting"));
    }

    #[test]
    fn production_home_renders_github_connection_and_pr_gate_without_invention() {
        let mut app = test_app();
        app.auth_resolved = true;
        app.prod_panel_visible = true;
        app.prod_github_status = Some(
            serde_json::from_value(json!({
                "connected": true,
                "provider": "github",
                "login": "acme-owner",
                "observed_at": 1785203400.0,
                "absent_reason": null
            }))
            .expect("github status"),
        );
        app.prod_proposed_prs = Some(
            serde_json::from_value(json!({
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
                "next_cursor": null,
                "has_more": false
            }))
            .expect("proposed PRs"),
        );

        let rendered = production_workspace_lines(&app)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Connected · @acme-owner"), "{rendered}");
        assert!(rendered.contains("#17 · Repair checkout"), "{rendered}");
        assert!(
            rendered.contains("gate absent · no gate verdict has been recorded"),
            "{rendered}"
        );
        assert!(
            rendered.contains("https://github.com/acme/shop/pull/17"),
            "{rendered}"
        );
        assert!(!rendered.contains("verified"), "{rendered}");
    }

    #[test]
    fn production_home_keeps_unknown_github_state_unknown() {
        let mut app = test_app();
        app.auth_resolved = true;
        app.prod_panel_visible = true;
        app.prod_github_status = Some(
            serde_json::from_value(json!({
                "connected": null,
                "provider": "github",
                "login": null,
                "observed_at": 1785203400.0,
                "absent_reason": "installation store unavailable: RuntimeError"
            }))
            .expect("unknown github status"),
        );

        let rendered = production_workspace_lines(&app)
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Connection unknown"), "{rendered}");
        assert!(rendered.contains("RuntimeError"), "{rendered}");
        assert!(!rendered.contains("Not connected"), "{rendered}");
        assert!(!rendered.contains("0 proposed"), "{rendered}");
    }

    #[test]
    fn production_and_review_are_auxiliary_rails_that_preserve_the_work_surface() {
        let mut production = test_app();
        production.auth_resolved = true;
        production.prod_panel_visible = true;
        let rendered = rendered_frame_at_size(&production, Instant::now(), 120, 32);
        assert!(rendered.contains(" LIVE PRODUCTION "));
        assert!(rendered.contains(" CONVERSATION "));
        assert!(rendered.contains(" ASK ESTELLE "));

        let mut review = test_app();
        review.prod_panel_visible = false;
        review.diff_panel_visible = true;
        review.last_diff = Some(
            "diff --git a/billing/charge.rs b/billing/charge.rs\n@@ -82 +82 @@\n-old()\n+retry_after()\n"
                .to_string(),
        );
        let rendered = rendered_frame_at_size(&review, Instant::now(), 120, 32);
        assert!(rendered.contains(" WORK DRAFT · /work · READ ONLY "));
        assert!(rendered.contains(" CONVERSATION "));
        assert!(rendered.contains(" ASK ESTELLE "));
    }

    #[test]
    fn work_draft_renders_github_style_old_and_new_line_gutters() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.diff_panel_visible = true;
        app.last_diff = Some(
            "diff --git a/billing/charge.rs b/billing/charge.rs\n--- a/billing/charge.rs\n+++ b/billing/charge.rs\n@@ -82,2 +82,2 @@ fn charge()\n-    old()\n+    retry_after()\n     bill()\n"
                .to_string(),
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 150, 32);

        assert!(
            rendered.contains(" 82     -"),
            "old gutter missing:\n{rendered}"
        );
        assert!(
            rendered.contains("     82 +"),
            "new gutter missing:\n{rendered}"
        );
        assert!(
            rendered.contains(" 83  83  "),
            "context gutters missing:\n{rendered}"
        );
    }

    #[test]
    fn bottom_dock_owns_one_separator_and_closes_each_visible_edge() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.composer.set_text("/m");
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 100, 30);
        let rendered = test_gallery::buffer_text(&buffer);

        let has_adjacent_rules = rendered
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|rows| rows[0].contains("└──") && rows[1].contains("┌ COMMANDS"));
        assert!(!has_adjacent_rules, "{rendered}");
        let command_rule = rendered
            .lines()
            .find(|line| line.contains(" COMMANDS "))
            .expect("command dock rule");
        assert!(command_rule.ends_with('┐'), "{rendered}");
        assert!(
            rendered
                .lines()
                .any(|line| line.starts_with('└') && line.ends_with('┘')),
            "{rendered}"
        );
        assert!(
            rendered
                .lines()
                .filter(|line| line.contains('│'))
                .all(|line| line.ends_with('│')),
            "right edges must share one terminal column:\n{rendered}"
        );
    }

    #[test]
    fn settings_picker_is_one_closed_bottom_dock_not_a_nested_window() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        let (tx, _rx) = mpsc::unbounded_channel();
        app.submit("/settings".to_string(), &tx);

        let buffer = rendered_buffer_at_size(&app, Instant::now(), 100, 30);
        let rendered = test_gallery::buffer_text(&buffer);
        let settings_rule = rendered
            .lines()
            .find(|line| line.contains(" SETTINGS "))
            .expect("settings rule");

        assert!(settings_rule.ends_with('┐'), "{rendered}");
        assert!(
            rendered
                .lines()
                .any(|line| line.starts_with('└') && line.ends_with('┘')),
            "{rendered}"
        );
        assert!(!rendered.contains(" ASK ESTELLE "), "{rendered}");
    }

    #[test]
    fn production_view_uses_real_counts_ranges_and_absent_gate_reason() {
        let issues: estelle_client::MonitorIssuesResponse = serde_json::from_value(json!({
            "issues": [{
                "key": "iss-1",
                "symbol": "charge_card",
                "symbol_range": {
                    "symbol": "charge_card",
                    "file": "billing.py",
                    "line_start": 82,
                    "line_end": 119,
                    "repo": "uqeu/estelle",
                    "resolved_by": "line-range"
                },
                "title": "TimeoutError in charge_card",
                "count": 47,
                "events_in_window": 12,
                "status": "unresolved",
                "bind_status": "bound",
                "repair_status": "proposed",
                "gate_absent_reason": "repair has not reached the gate"
            }],
            "counts": {"unresolved": 1},
            "window_s": 3600
        }))
        .expect("issues");
        let overview: estelle_client::MonitorOverviewResponse = serde_json::from_value(json!({
            "series": {
                "window_s": 3600,
                "bucket_s": 300,
                "requests_source": "unavailable",
                "buckets": [
                    {"t": 1, "errors": 1, "requests": null, "p99_ms": null},
                    {"t": 2, "errors": 4, "requests": null, "p99_ms": null}
                ]
            }
        }))
        .expect("overview");

        let rendered = production_health_lines(&issues, Some(&overview)).join("\n");

        assert!(rendered.contains("caught · TimeoutError in charge_card"));
        assert!(rendered.contains("grouped · 12 events"));
        assert!(rendered.contains("request denominator unavailable"));
        assert!(rendered.contains("traced to · billing.py:82-119"));
        assert!(rendered.contains("gate · repair has not reached the gate"));
        assert!(!rendered.contains("error rate"));
        assert!(!rendered.contains("YOU ARE EDITING"));
    }

    #[test]
    fn production_view_discloses_missing_binding_and_propose_only_repair() {
        let issues: estelle_client::MonitorIssuesResponse = serde_json::from_value(json!({
            "issues": [{
                "key": "iss-2",
                "title": "TimeoutError in charge_card",
                "count": 7,
                "status": "unresolved",
                "bind_status": "",
                "bind_detail": "",
                "repair_status": "proposed",
                "gate_absent_reason": "repair has not reached the gate"
            }]
        }))
        .expect("issues");

        let rendered = production_health_lines(&issues, None).join("\n");

        assert!(rendered.contains("bind · unbound · reason not recorded"));
        assert!(rendered.contains("drafted repair · awaiting human review"));
        assert!(!rendered.contains("repair · proposed"));
    }

    #[test]
    fn production_queue_renders_the_exact_patch_or_a_named_unavailable_reason() {
        let mut app = test_app();
        app.prod_issues = Some(
            serde_json::from_value(json!({
                "issues": [{
                    "key": "with-patch",
                    "status": "unresolved",
                    "signal": {"title": "Patch ready"},
                    "repair": {"status": "proposed", "pr": null,
                        "patch": {"format": "unified_diff", "base_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                                  "text": "--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n", "observed_at": 42.5},
                        "patch_absent_reason": null}
                }, {
                    "key": "without-patch",
                    "status": "unresolved",
                    "signal": {"title": "Old proposal"},
                    "repair": {"status": "proposed", "pr": null, "patch": null,
                               "patch_absent_reason": "not_persisted"}
                }]
            }))
            .expect("issues"),
        );

        let rendered = production_workspace_lines(&app)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("base aaaaaaaaaaaa"), "{rendered}");
        assert!(rendered.contains("+ new"), "{rendered}");
        assert!(
            rendered.contains("diff unavailable - not_persisted"),
            "{rendered}"
        );
        assert!(
            rendered.contains("drafted repair · Old proposal"),
            "{rendered}"
        );
    }

    #[test]
    fn boot_is_a_transient_real_renderer_surface_and_does_not_move_the_composer() {
        let now = Instant::now();
        let mut app = test_app();
        app.boot = Some(BootScene::new(0));
        app.boot_started = now
            .checked_sub(Duration::from_millis(codex_tui::boot_scene::CONDENSE_MS))
            .expect("boot clock");

        let boot = rendered_frame_at_size(&app, now, 120, 34);
        assert!(boot.contains("Estelle"));
        assert!(boot.contains("by Fate Labs"));

        let finished = rendered_frame_at_size(
            &app,
            now + Duration::from_millis(codex_tui::boot_scene::FAIL_MS),
            120,
            34,
        );
        assert!(finished.contains("ASK ESTELLE"));
        assert!(finished.contains("shift+tab"));
        assert!(!finished.contains("enter ask"));
        assert!(!finished.contains("shift+enter newline"));
    }

    #[test]
    fn first_input_skips_boot_without_eating_the_key() {
        let now = Instant::now();
        let mut app = test_app();
        app.boot = Some(BootScene::new(0));
        app.boot_started = now;
        let (tx, _rx) = mpsc::unbounded_channel();

        app.skip_boot(now + Duration::from_millis(50));
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
            &tx,
        );
        std::thread::sleep(Duration::from_millis(10));
        app.composer.flush_paste_burst_if_due();

        assert_eq!(app.composer.text(), "h");
        assert!(matches!(
            app.boot.as_ref().map(|boot| boot.phase(50)),
            Some(codex_tui::boot_scene::BootPhase::Dissolving { skipped: true, .. })
        ));
    }

    #[test]
    fn work_diff_opens_a_read_only_side_panel_with_the_exact_patch() {
        let mut app = test_app();
        app.last_diff = Some(
            "diff --git a/src/charge.rs b/src/charge.rs\n@@ -1 +1 @@\n-old()\n+retry_after()\n"
                .to_string(),
        );
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("/diff".to_string(), &tx);

        let rendered = rendered_frame_at_size(&app, Instant::now(), 140, 34);
        assert!(rendered.contains("WORK DRAFT"));
        assert!(rendered.contains("src/charge.rs"));
        assert!(
            rendered
                .lines()
                .any(|line| line.contains('-') && line.contains("old()"))
        );
        assert!(
            rendered
                .lines()
                .any(|line| line.contains('+') && line.contains("retry_after()"))
        );
        assert!(rendered.contains("read-only"));
    }

    #[tokio::test]
    async fn prod_fetches_cursor_issue_feed_and_monitor_overview_off_the_render_thread() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/issues"))
            .and(wiremock::matchers::query_param("repo", "uqeu/estelle"))
            .and(wiremock::matchers::query_param("limit", "50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issues": [{
                    "key": "iss-1",
                    "cursor": 12.5,
                    "signal": {
                        "title": "TimeoutError in charge_card",
                        "count": 3
                    },
                    "bound": {
                        "symbol": "charge_card",
                        "status": "bound",
                        "file": "billing.py",
                        "line": 82
                    },
                    "repair": {"status": "proposed", "pr": null},
                    "gate": null,
                    "gate_absent_reason": "repair has not reached the gate"
                }],
                "next_since": 12.5,
                "has_more": false
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/monitor/overview"))
            .and(wiremock::matchers::query_param("repo", "uqeu/estelle"))
            .and(wiremock::matchers::query_param("window_s", "3600"))
            .and(wiremock::matchers::query_param("buckets", "12"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error_rate": {
                    "window_s": 3600,
                    "buckets": 2,
                    "total": 3,
                    "series": [{"start": 1, "count": 1}, {"start": 2, "count": 2}]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/agent/health"))
            .and(wiremock::matchers::query_param("repo", "uqeu/estelle"))
            .and(wiremock::matchers::query_param("window_s", "3600"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "enabled": true,
                "observed_at": 1785203400.0,
                "stale_after_s": 120,
                "counts": {"reporting": 1, "degraded": 1, "silent": 0},
                "agents": [{
                    "id": "checkout-agent",
                    "state": "degraded",
                    "events": 19,
                    "last_seen": 1785203370.0,
                    "current_signal": "tool timeout"
                }]
            })))
            .mount(&server)
            .await;
        let mut app = test_app();
        app.auth_resolved = true;
        app.prod_panel_visible = true;
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("estelle_test_key").expect("key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("client"),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();

        app.poll_production_if_due(&tx);
        for _ in 0..3 {
            let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("monitor request completed")
                .expect("monitor event");
            app.handle_ui_event(event, &tx);
        }

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 34);
        assert!(rendered.contains("caught · TimeoutError in charge_card"));
        assert!(rendered.contains("billing.py:82"));
        assert!(rendered.contains("drafted repair"), "{rendered}");
        assert!(rendered.contains("awaiting human review"), "{rendered}");
        assert!(rendered.contains("error counts"));
        assert!(rendered.contains("request denominator unavailable"));
        assert!(rendered.contains("checkout-agent"), "{rendered}");
        assert!(rendered.contains("tool timeout"), "{rendered}");
        assert!(app.active.is_none());
    }

    #[test]
    fn cursor_pages_replace_matching_issues_without_discarding_older_rows() {
        let mut current = Some(
            serde_json::from_value(json!({
                "issues": [
                    {"key": "old", "signal": {"title": "Old", "count": 1}},
                    {"key": "same", "signal": {"title": "Before", "count": 1}}
                ],
                "next_since": 10.0,
                "has_more": true
            }))
            .expect("first page"),
        );
        let page = serde_json::from_value(json!({
            "issues": [
                {"key": "same", "signal": {"title": "After", "count": 2}},
                {"key": "new", "signal": {"title": "New", "count": 1}}
            ],
            "next_since": 20.0,
            "has_more": false
        }))
        .expect("next page");

        merge_issue_page(&mut current, page);

        let current = current.expect("merged feed");
        assert_eq!(current.issues.len(), 3);
        assert_eq!(current.next_since, Some(20.0));
        assert!(!current.has_more);
        assert_eq!(current.issues[1].display_title(), "After");
    }

    #[test]
    fn sandbox_state_degrades_to_one_real_verdict_line_without_a_stream() {
        let issues: estelle_client::MonitorIssuesResponse = serde_json::from_value(json!({
            "issues": [{
                "key": "iss-1",
                "title": "TimeoutError in charge_card",
                "events_in_window": 3,
                "status": "unresolved",
                "bind_status": "bound",
                "repair_status": "sandbox_complete",
                "repair_gate_verdict": "abstained"
            }]
        }))
        .expect("issues");

        let lines = production_health_lines(&issues, None);
        let state_lines = lines
            .iter()
            .filter(|line| line.contains('·') && !line.starts_with("prod ·"))
            .collect::<Vec<_>>();

        assert_eq!(
            state_lines,
            ["sandbox · a clone, never production · abstained"]
        );
    }

    #[test]
    fn production_polling_backs_off_when_idle_unfocused_or_failing() {
        assert_eq!(
            production_poll_delay(Duration::from_secs(30), 0, false),
            Duration::from_secs(30)
        );
        assert_eq!(
            production_poll_delay(Duration::from_secs(60), 0, false),
            Duration::from_secs(60)
        );
        assert_eq!(
            production_poll_delay(Duration::from_secs(30), 0, true),
            Duration::from_secs(300)
        );
        assert_eq!(
            production_poll_delay(Duration::from_secs(30), 4, false),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn skill_catalog_preserves_valid_names_while_real_credentials_stay_hidden() {
        let reply: CommandReply = serde_json::from_value(json!({
            "skills": [{
                "name": "change-deploy-risk-gate",
                "summary": "gate deploys with sk-abcdefghijklmnop"
            }]
        }))
        .expect("typed skill catalogue");
        let rendered_lines = commands::render_remote_reply("skills", &reply);
        let rendered = format!(
            "{:?}",
            render_transcript(&[TranscriptEntry::Command {
                name: "skills".to_string(),
                lines: rendered_lines,
            }])
        );

        assert!(rendered.contains("change-deploy-risk-gate"));
        assert!(rendered.contains("credential hidden"));
        assert!(!rendered.contains("sk-abcdefghijklmnop"));
    }

    #[test]
    fn snapshot_composer_with_text() {
        let mut app = test_app();
        app.handle_paste("trace the charge path".to_string());
        insta::assert_snapshot!(rendered_frame(&app, Instant::now()));
    }

    #[test]
    fn snapshot_slash_menu_open() {
        let mut app = test_app();
        app.handle_paste("/mo".to_string());
        insta::assert_snapshot!(rendered_frame(&app, Instant::now()));
    }

    #[test]
    fn slash_palette_discovers_estelle_and_inherited_commands() {
        let mut app = test_app();
        app.handle_paste("/mo".to_string());

        let rendered = rendered_frame(&app, Instant::now());

        assert!(rendered.contains("/mode"));
        assert!(rendered.contains("/model"));
    }

    #[test]
    fn shift_tab_opens_the_server_backed_autonomy_ladder() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &tx,
        );
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.title.as_str()),
            Some("Autonomy")
        );
        assert_eq!(app.local_mode, "read_only");
        assert!(app.active.is_none());
        assert!(app.queue.is_empty());
    }

    #[test]
    fn long_waits_render_compact_elapsed_ported_from_codex_status_indicator() {
        let mut app = test_app();
        let started = Instant::now();
        app.active = Some(ActiveRequest {
            id: 1,
            label: "thinking".to_string(),
            started,
            cancel: CancellationToken::new(),
        });

        let rendered = rendered_frame(&app, started + Duration::from_secs(93));

        assert!(
            rendered.contains("1m 33s"),
            "the wait did not render as compact elapsed time\n{rendered}"
        );
        assert!(
            !rendered.contains("93s"),
            "raw seconds survived\n{rendered}"
        );
    }

    #[test]
    fn early_wait_reports_only_the_observed_request_state() {
        let mut app = test_app();
        let started = Instant::now();
        app.active = Some(ActiveRequest {
            id: 1,
            label: "thinking".to_string(),
            started,
            cancel: CancellationToken::new(),
        });

        let rendered = rendered_frame(&app, started + Duration::from_secs(6));

        assert!(rendered.contains("thinking  6s"));
        assert!(!rendered.contains("cache"));
    }

    #[test]
    fn snapshot_long_running_query_with_elapsed_timer() {
        let mut app = test_app();
        let started = Instant::now();
        app.transcript.push(TranscriptEntry::User(
            "Which repair changed charge.ts?".to_string(),
        ));
        app.active = Some(ActiveRequest {
            id: 4,
            label: "thinking".to_string(),
            started,
            cancel: CancellationToken::new(),
        });
        insta::assert_snapshot!(rendered_frame(&app, started + Duration::from_secs(93)));
    }

    #[test]
    fn snapshot_every_failure_screen() {
        let mut app = test_app();
        for view in [
            FailureView::AuthRejected,
            FailureView::Server {
                status: 502,
                message: "the server returned a non-Estelle error body".to_string(),
            },
            FailureView::Request {
                status: 400,
                message: "repo is required".to_string(),
            },
            FailureView::Timeout,
            FailureView::Network,
            FailureView::Cancelled,
            FailureView::Client("the response body was empty".to_string()),
        ] {
            app.transcript
                .push(TranscriptEntry::Failure(failure_lines_for(&view)));
        }
        insta::assert_snapshot!(rendered_frame_at_size(&app, Instant::now(), 80, 52));
    }

    #[test]
    fn all_explicit_auth_rejections_share_the_safe_failure_screen() {
        for status in [
            http::StatusCode::UNAUTHORIZED,
            http::StatusCode::FORBIDDEN,
            http::StatusCode::NOT_FOUND,
        ] {
            let error = Error::Http {
                status,
                message: "rejected".to_string(),
            };
            assert_eq!(FailureView::from(&error), FailureView::AuthRejected);
        }
    }

    #[test]
    fn failures_have_what_who_and_next_without_foreign_body() {
        let lines = failure_lines(&Error::Http {
            status: http::StatusCode::BAD_GATEWAY,
            message: "the server returned a non-Estelle error body".to_string(),
        });
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("HTTP 502"));
        assert!(lines[1].contains("Estelle service"));
        assert!(lines[2].contains("Retry"));
        assert!(!lines.join(" ").contains("Application failed to respond"));
    }

    #[test]
    fn repo_match_accepts_owner_name_and_bare_name() {
        let repo = Repo::new("fatelabs/estelle").expect("repo");
        assert!(repo_is_listed(&repo, &["fatelabs/estelle".to_string()]));
        assert!(repo_is_listed(&repo, &["estelle".to_string()]));
        assert!(!repo_is_listed(&repo, &["another".to_string()]));
    }

    #[test]
    fn stale_completion_cannot_clear_the_current_request() {
        let mut app = App::new(Args {
            command: None,
            repo: Some("fatelabs/estelle".to_string()),
        });
        app.active = Some(ActiveRequest {
            id: 7,
            label: "thinking".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        app.handle_ui_event(
            UiEvent::Answer {
                id: 6,
                result: Err(Error::Cancelled),
            },
            &tx,
        );
        assert_eq!(app.active.as_ref().map(|active| active.id), Some(7));
    }

    #[test]
    fn pasted_credentials_are_hidden_before_the_next_frame() {
        let mut app = App::new(Args {
            command: None,
            repo: Some("fatelabs/estelle".to_string()),
        });
        let secret = format!("estelle_live_{}", "a1b2c3d4e5f6".repeat(2));

        app.handle_paste(format!("my key is {secret}"));

        let area = ratatui::layout::Rect::new(0, 0, 80, 6);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        app.composer.render_ref(area, &mut buffer);
        let rendered = format!("{buffer:?}");
        assert!(!rendered.contains(&secret), "credential reached the frame");
        assert!(rendered.contains("credential hidden"));

        let mut ordinary = App::new(Args {
            command: None,
            repo: Some("fatelabs/estelle".to_string()),
        });
        ordinary.handle_paste("what changed in auth?".to_string());
        let mut ordinary_buffer = ratatui::buffer::Buffer::empty(area);
        ordinary.composer.render_ref(area, &mut ordinary_buffer);
        assert!(format!("{ordinary_buffer:?}").contains("what changed in auth?"));
    }

    #[test]
    fn credentials_assembled_across_pastes_are_hidden_before_the_next_frame() {
        let mut app = test_app();
        let suffix = "a1b2c3d4e5f6".repeat(2);
        let secret = format!("estelle_live_{suffix}");

        app.handle_paste("estelle_live_".to_string());
        app.handle_paste(suffix);

        let rendered = rendered_frame(&app, Instant::now());
        assert!(
            !rendered.contains(&secret),
            "assembled credential reached the frame"
        );
        assert!(rendered.contains("credential hidden"));
    }

    #[test]
    fn session_help_is_local_and_lists_the_exact_denominator() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("/help".to_string(), &tx);

        assert!(app.queue.is_empty(), "/help must not queue an HTTP request");
        let rendered = format!("{:?}", render_transcript(&app.transcript));
        assert!(rendered.contains("/orchestra"));
        assert!(rendered.contains("/exit"));
    }

    #[test]
    fn an_unknown_slash_command_costs_zero_requests_and_says_nothing_ran() {
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("/blorp".to_string(), &tx);

        assert!(
            app.queue.is_empty(),
            "unknown slash command must stay local"
        );
        let rendered = format!("{:?}", render_transcript(&app.transcript));
        assert!(rendered.contains("nothing ran"));
        assert!(rendered.contains("/blorp"));
    }

    #[test]
    fn login_opens_the_five_way_credential_picker_before_any_request() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = test_app();
        app.auth_resolved = true;

        app.submit("/login".to_string(), &tx);

        assert!(app.queue.is_empty(), "/login must stay local");
        assert!(
            app.active.is_none(),
            "/login must not start an HTTP request"
        );
        let picker = app.picker.as_ref().expect("credential picker");
        assert_eq!(picker.title, "Connect Estelle");
        assert_eq!(picker.rows.len(), 5);
        assert_eq!(picker.rows[0].label, "Estelle account");
        assert!(picker.rows[0].detail.contains("grounding"));
        assert!(
            picker.rows[0]
                .detail
                .contains("never pays for model tokens")
        );
        assert_eq!(picker.rows[1].label, "Claude subscription");
        assert!(picker.rows[1].detail.contains("Claude Code"));
        assert_eq!(picker.rows[2].label, "ChatGPT plan");
        assert!(picker.rows[2].detail.contains("device code"));
        assert_eq!(picker.rows[3].label, "Provider API key");
        assert!(picker.rows[3].detail.contains("Anthropic"));
        assert_eq!(picker.rows[4].label, "Local model");
        assert!(picker.rows[4].detail.contains("LM Studio"));
        assert!(picker.rows[4].detail.contains("Ollama"));
        assert!(picker.rows[4].detail.contains("This machine"));
        assert!(picker.rows[4].detail.contains("RAM"));
    }

    #[test]
    fn slash_provider_routes_match_the_shell_provider_meanings() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = test_app();

        app.submit("/login --provider openai".to_string(), &tx);
        assert_eq!(app.pending_login, Some(PendingLogin::Chatgpt));

        app.pending_login = None;
        app.submit("/login --provider claude".to_string(), &tx);
        assert_eq!(app.pending_login, Some(PendingLogin::Claude));

        app.pending_login = None;
        app.submit("/login --api-key openai".to_string(), &tx);
        assert_eq!(
            app.pending_login,
            Some(PendingLogin::EstelleThenProvider("openai-api"))
        );
    }

    #[test]
    fn first_run_absence_opens_login_before_the_conversation_surface() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut app = test_app();

        app.handle_ui_event(UiEvent::Credential(Err(Error::NoCredential)), &tx);

        assert!(app.auth_resolved);
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.title.as_str()),
            Some("Connect Estelle")
        );
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 30);
        assert!(rendered.contains("CONNECT ESTELLE"));
        assert!(rendered.contains("grounds your coding agent in your real codebase"));
        assert!(rendered.contains("never bills you for model tokens"));
        assert!(rendered.contains("This machine"));
        assert!(rendered.contains("RAM"));
        assert!(!rendered.contains("ASK ESTELLE"));
        assert!(!rendered.contains("Run estelle login"));
    }

    #[test]
    fn whoami_reports_credential_kinds_and_provider_names_but_never_values() {
        let secret = "provider-secret-must-not-render";
        let mut app = test_app();
        app.auth = Some(AuthContext {
            store: CredentialStore::new("/tmp/whoami-test-auth.json"),
            source: CredentialSource::Stored,
        });
        app.account = Some(
            serde_json::from_value(json!({
                "configured": ["anthropic", "openrouter"],
                "provider_key": secret
            }))
            .expect("account response"),
        );

        let rendered = whoami_lines(&app, true).join("\n");

        assert!(rendered.contains("Estelle account  yes"));
        assert!(rendered.contains("Model plan  yes"));
        assert!(rendered.contains("Provider keys  anthropic, openrouter"));
        assert!(!rendered.contains(secret));
    }

    #[test]
    fn whoami_does_not_turn_an_unfetched_account_into_no_provider_keys() {
        let mut app = test_app();
        app.auth = Some(AuthContext {
            store: CredentialStore::new("/tmp/whoami-unfetched-auth.json"),
            source: CredentialSource::Stored,
        });

        let rendered = whoami_lines(&app, false).join("\n");

        assert!(rendered.contains("Provider keys  not returned yet"));
        assert!(!rendered.contains("Provider keys  none"));
    }

    #[tokio::test]
    async fn typed_mode_raise_uses_the_same_confirmation_and_account_post_as_settings() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/autonomy"))
            .and(body_json(json!({"level": "propose"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "autonomy": "propose"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut app = test_app();
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("estelle_live_mode-command").expect("key"),
                estelle_client::MINIMUM_TIMEOUT,
            )
            .expect("client"),
        );
        app.server_mode = Some("read_only".to_string());
        let (tx, mut rx) = mpsc::unbounded_channel();

        app.submit("/mode edit".to_string(), &tx);
        assert_eq!(
            app.picker.as_ref().map(|picker| picker.title.as_str()),
            Some("Confirm raise to accept-edits")
        );
        assert_eq!(app.server_mode.as_deref(), Some("read_only"));

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &tx,
        );
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("autonomy response")
            .expect("event");
        app.handle_ui_event(event, &tx);

        assert_eq!(app.server_mode.as_deref(), Some("propose"));
    }

    #[test]
    fn every_top_level_name_is_claimed_before_the_tui_fallback() {
        for name in commands::top_level_command_names() {
            let parsed = Args::try_parse_from(["estelle", name]);
            assert!(parsed.is_ok(), "top-level command {name} was not claimed");
            assert!(
                parsed.expect("claimed command").command.is_some(),
                "top-level command {name} fell through to the TUI"
            );
        }
        for alias in ["disconnect", "off"] {
            assert!(Args::try_parse_from(["estelle", alias]).is_ok());
        }
    }

    #[test]
    fn server_text_is_masked_at_the_shared_renderer_boundary() {
        let secret = "estelle_live_aaaaaaaaaaaaaaaaaaaaaaaa";
        let rendered = format!(
            "{:?}",
            render_transcript(&[
                TranscriptEntry::Answer {
                    text: format!("answer echoed {secret}"),
                    grounded: Some(true),
                    degraded: false,
                    sources: Vec::new(),
                },
                TranscriptEntry::Command {
                    name: "status".to_string(),
                    lines: vec![format!("server returned {secret}")],
                },
                TranscriptEntry::Failure([
                    format!("request included {secret}"),
                    "server side".to_string(),
                    "retry".to_string(),
                ]),
            ])
        );
        assert!(!rendered.contains(secret));
        assert!(rendered.matches("credential hidden").count() >= 3);
    }

    #[tokio::test]
    async fn work_submission_crosses_the_real_app_client_renderer_seam() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/work"))
            .and(body_json(json!({
                "repo": "fatelabs/estelle",
                "task": "repair charge"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "Changed the grounded call site.",
                "diff": "diff --git a/a.rs b/a.rs\n"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let mut app = test_app();
        app.repo = Repo::new("fatelabs/estelle").expect("repo");
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("test-key").expect("key"),
                Duration::from_secs(120),
            )
            .expect("client"),
        );
        app.auth_resolved = true;
        app.local_mode = "propose".to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();

        app.submit("/work repair charge".to_string(), &tx);
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("completion deadline")
            .expect("completion event");
        app.handle_ui_event(event, &tx);

        assert_eq!(app.last_diff.as_deref(), Some("diff --git a/a.rs b/a.rs\n"));
        let rendered = format!("{:?}", render_transcript(&app.transcript));
        assert!(rendered.contains("Changed the grounded call site"));
        assert!(rendered.contains("reviewable diff is ready"));
    }

    #[tokio::test]
    async fn grounded_answer_keeps_citations_visible_without_moving_the_composer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .and(body_json(json!({
                "repo": "fatelabs/estelle",
                "question": "where does charge fail?"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "The retry loop has no ceiling.",
                "grounded": true,
                "sources": [{"file": "api/charge.ts", "line": 52}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let root = tempfile::tempdir().expect("working tree");
        let response = answer_question(
            client,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "where does charge fail?".to_string(),
            None,
            &CancellationToken::new(),
        )
        .await
        .expect("answer");
        let mut app = test_app();
        app.active = Some(ActiveRequest {
            id: 9,
            label: "thinking".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        app.handle_ui_event(
            UiEvent::Answer {
                id: 9,
                result: Ok(response),
            },
            &tx,
        );

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 30);

        assert!(rendered.contains("api/charge.ts:52"));
        assert!(rendered.contains("Ask Estelle"));
    }

    fn dirty_working_tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("working tree");
        let git = |arguments: &[&str]| {
            let status = std::process::Command::new("git")
                .args(arguments)
                .current_dir(root.path())
                .status()
                .expect("git invocation");
            assert!(status.success(), "git {arguments:?} failed");
        };
        git(&["init"]);
        std::fs::write(root.path().join("main.rs"), "fn baseline() {}\n").expect("baseline");
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
        std::fs::write(
            root.path().join("main.rs"),
            "fn changed() -> &'static str { \"local sentinel content\" }\n",
        )
        .expect("changed");
        root
    }

    #[tokio::test]
    async fn answer_turn_shows_the_answer_only_never_the_retrieval_plumbing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "TEAM SENTINEL ANSWER",
                "grounded": true,
                "sources": []
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "model": "estelle",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "LOCAL SENTINEL ANSWER"},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let root = dirty_working_tree();
        let reply = answer_question(
            client,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "what does the changed function return?".to_string(),
            None,
            &CancellationToken::new(),
        )
        .await
        .expect("answer");

        // The transcript string is the answer, and only the answer. Retrieval context is model
        // input; it never becomes assistant output.
        assert_eq!(reply.text, "TEAM SENTINEL ANSWER");
        assert!(!reply.text.contains("Working memory ("));
        assert!(
            !reply.text.contains("LOCAL SENTINEL ANSWER"),
            "the working-memory leg must not surface as a second spliced answer"
        );
        for working_path in &reply.working_paths {
            assert!(
                !reply.text.contains(working_path),
                "provenance path {working_path} leaked into the transcript text"
            );
        }
        // Provenance is disclosed from the typed field, not from prose in the transcript.
        assert_eq!(reply.working_paths, ["main.rs"]);

        // One model round-trip per question, to /deep-search. THE RULE: the client sends data;
        // the client never authors instructions. `question` is BYTE-IDENTICAL to what the user
        // typed — a contains-check would pass on a wrapper, so this is equality — and working
        // memory rides a separate top-level key as data, never smuggled through prose.
        let requests = server.received_requests().await.expect("request recording");
        assert_eq!(
            requests.len(),
            1,
            "exactly one model round-trip may fire per question"
        );
        assert_eq!(requests[0].url.path(), "/deep-search");
        let body: Value = serde_json::from_slice(&requests[0].body).expect("json body");
        assert_eq!(
            body["question"].as_str(),
            Some("what does the changed function return?"),
            "the outbound question must be byte-identical to what the user typed"
        );
        let working_memory = body
            .get("working_memory")
            .expect("working memory rides its own top-level key");
        let files = working_memory["files"].as_array().expect("files array");
        assert_eq!(files[0]["path"].as_str(), Some("main.rs"));
        assert!(
            files[0]["content"]
                .as_str()
                .is_some_and(|content| content.contains("local sentinel content")),
            "working memory must still reach the server as data"
        );
        assert!(
            working_memory.get("instruction").is_none() && working_memory.get("prompt").is_none(),
            "the working-memory payload carries data, never instructions"
        );

        // And the rendered frame shows the answer without the plumbing.
        let mut app = test_app();
        app.active = Some(ActiveRequest {
            id: 9,
            label: "thinking".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        app.handle_ui_event(
            UiEvent::Answer {
                id: 9,
                result: Ok(reply),
            },
            &tx,
        );
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 30);
        assert!(rendered.contains("TEAM SENTINEL ANSWER"));
        assert!(!rendered.contains("Working memory ("));
        assert!(!rendered.contains("main.rs"));
    }

    #[tokio::test]
    async fn a_second_skill_run_sends_the_conversation_the_first_one_built() {
        // The server runs an interactive skill over body["messages"] when present and restarts
        // single-turn from "task" when not — so a CLI that never sends messages makes every
        // follow-up a fresh start. The first run sends no messages key; the SECOND run of the
        // same skill carries the whole prior exchange plus the new turn.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/skill/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "skill": "grill-me",
                "reply": "R1 SENTINEL REPLY"
            })))
            .mount(&server)
            .await;
        let root = tempfile::tempdir().expect("working tree");
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();

        // First run: no thread exists yet.
        app.submit("/skill:grill-me first claim".to_string(), &tx);
        let first = app.queue.pop_front().expect("first run queued");
        let QueuedRequest::Command(first_pending) = first else {
            panic!("expected a command")
        };
        let client = || {
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("test-key").expect("key"),
                Duration::from_secs(120),
            )
            .expect("client")
        };
        let repo = || Repo::new("fatelabs/estelle").expect("repo");
        let first_reply = execute_remote_command(
            client(),
            repo(),
            root.path().to_path_buf(),
            first_pending,
            &CancellationToken::new(),
        )
        .await
        .expect("first run");
        app.active = Some(ActiveRequest {
            id: 9,
            label: "/skill:grill-me".to_string(),
            started: Instant::now(),
            cancel: CancellationToken::new(),
        });
        app.handle_ui_event(
            UiEvent::CommandAnswer {
                id: 9,
                name: "skill:",
                result: Ok(first_reply),
            },
            &tx,
        );

        // Second run of the same skill: the thread must ride the request.
        app.submit("/skill:grill-me second claim".to_string(), &tx);
        let second = app.queue.pop_front().expect("second run queued");
        let QueuedRequest::Command(second_pending) = second else {
            panic!("expected a command")
        };
        execute_remote_command(
            client(),
            repo(),
            root.path().to_path_buf(),
            second_pending,
            &CancellationToken::new(),
        )
        .await
        .expect("second run");

        let requests = server.received_requests().await.expect("request recording");
        assert_eq!(requests.len(), 2);
        let first_body: Value = serde_json::from_slice(&requests[0].body).expect("first body");
        assert!(
            first_body.get("messages").is_none(),
            "the first run must not invent a history: {first_body}"
        );
        let second_body: Value = serde_json::from_slice(&requests[1].body).expect("second body");
        let messages = second_body["messages"]
            .as_array()
            .expect("the second run carries the conversation");
        let turns = messages
            .iter()
            .map(|turn| {
                format!(
                    "{}:{}",
                    turn["role"].as_str().unwrap_or("?"),
                    turn["content"].as_str().unwrap_or("?")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            turns,
            [
                "user:first claim",
                "assistant:R1 SENTINEL REPLY",
                "user:second claim"
            ],
            "the second run did not carry the prior exchange plus the new turn"
        );
    }

    #[tokio::test]
    async fn conversational_question_rides_the_fast_path_with_no_working_memory_upload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "HI SENTINEL ANSWER",
                "grounded": true,
                "sources": []
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "model": "estelle",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "LOCAL SENTINEL ANSWER"},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let root = dirty_working_tree();
        let reply = answer_question(
            client,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "hi".to_string(),
            None,
            &CancellationToken::new(),
        )
        .await
        .expect("answer");

        assert_eq!(reply.text, "HI SENTINEL ANSWER");
        assert!(reply.working_paths.is_empty());
        // A conversational turn uploads the question ALONE, so the server's is_conversational
        // fast path can fire; 80 KB of working memory would defeat it. One request, no chat leg.
        let requests = server.received_requests().await.expect("request recording");
        assert_eq!(
            requests.len(),
            1,
            "a conversational turn must not fire the raw chat endpoint"
        );
        assert_eq!(requests[0].url.path(), "/deep-search");
        let body: Value = serde_json::from_slice(&requests[0].body).expect("json body");
        assert_eq!(body["question"].as_str(), Some("hi"));
        assert!(
            body.get("working_memory").is_none(),
            "a conversational turn attaches no working-memory payload"
        );
    }

    #[test]
    fn conversational_gate_decides_bandwidth_not_a_verdict() {
        assert!(is_conversational_turn("hi"));
        assert!(is_conversational_turn("thanks, that's helpful"));
        assert!(is_conversational_turn("good morning"));
        assert!(!is_conversational_turn(""));
        assert!(!is_conversational_turn("how does the auth flow work"));
        assert!(!is_conversational_turn("ok 500"));
        assert!(!is_conversational_turn("hi what does charge_card do"));
        assert!(!is_conversational_turn(
            "yes yes yes yes yes yes yes yes yes"
        ));
    }

    #[tokio::test]
    async fn slash_sweep_starts_a_measured_gauge_instead_of_printing_instructions() {
        let mut app = test_app();
        app.client = Some(
            Client::new(
                "http://127.0.0.1:1/",
                estelle_client::ApiKey::new("test-key").expect("key"),
                Duration::from_secs(120),
            )
            .expect("client"),
        );
        app.auth_resolved = true;
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("/sweep".to_string(), &tx);

        assert!(
            app.active.is_some(),
            "/sweep did not start the real ingest path"
        );
        let rendered = rendered_frame(&app, Instant::now());
        assert!(rendered.contains("preparing sweep"));
        assert!(rendered.contains('%'));
    }

    #[tokio::test]
    async fn sweep_gauge_follows_the_real_estimate_and_sync_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/sweep/estimate"))
            .and(body_json(json!({
                "repo": "fatelabs/estelle",
                "files": [{"path": "main.rs", "bytes": 13}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fits": true})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/sync"))
            .and(body_json(json!({
                "repo": "fatelabs/estelle",
                "files": [{"path": "main.rs", "content": "fn main() {}\n"}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accepted": 1})))
            .expect(1)
            .mount(&server)
            .await;
        let root = tempfile::tempdir().expect("source root");
        std::fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("source");
        let mut app = test_app();
        app.root = root.path().to_path_buf();
        app.repo = Repo::new("fatelabs/estelle").expect("repo");
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("test-key").expect("key"),
                Duration::from_secs(120),
            )
            .expect("client"),
        );
        app.auth_resolved = true;
        let (tx, mut rx) = mpsc::unbounded_channel();

        app.submit("/sweep".to_string(), &tx);
        let mut measured = Vec::new();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("sweep deadline")
                .expect("sweep event");
            if let UiEvent::SweepProgress { progress, .. } = &event {
                measured.push(progress.percent as u16);
            }
            let done = matches!(&event, UiEvent::SweepAnswer { .. });
            app.handle_ui_event(event, &tx);
            if done {
                break;
            }
        }

        assert_eq!(measured, [10, 20, 35, 100]);
        assert_eq!(
            app.sweep_progress.as_ref().map(|progress| progress.percent),
            Some(100.0)
        );
        let rendered = format!("{:?}", render_transcript(&app.transcript));
        assert!(rendered.contains("server accepted the complete source set"));
    }

    #[tokio::test]
    async fn gate_refusal_is_a_competence_modal_with_actual_diff_blast_radius() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "verdict": "blocked",
                "blockers": [{
                    "file": "a.rs",
                    "line": 2,
                    "reason": "invented call rotate_all_keys does not exist"
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let root = tempfile::tempdir().expect("git root");
        let run_git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(args)
                .status()
                .expect("git command");
            assert!(status.success(), "git {args:?}");
        };
        run_git(&["init", "-q"]);
        run_git(&["config", "user.email", "p6@example.invalid"]);
        run_git(&["config", "user.name", "P6 Test"]);
        std::fs::write(root.path().join("a.rs"), "fn a() {}\n").expect("a baseline");
        std::fs::write(root.path().join("b.rs"), "fn b() {}\n").expect("b baseline");
        run_git(&["add", "a.rs", "b.rs"]);
        run_git(&["commit", "-qm", "baseline"]);
        std::fs::write(
            root.path().join("a.rs"),
            "fn a() {\n    one();\n    two();\n}\n",
        )
        .expect("a change");
        std::fs::write(root.path().join("b.rs"), "").expect("b change");
        let mut app = test_app();
        app.root = root.path().to_path_buf();
        app.repo = Repo::new("fatelabs/estelle").expect("repo");
        app.client = Some(
            Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new("test-key").expect("key"),
                Duration::from_secs(120),
            )
            .expect("client"),
        );
        app.auth_resolved = true;
        let (tx, mut rx) = mpsc::unbounded_channel();

        app.submit("/gate".to_string(), &tx);
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("gate deadline")
            .expect("gate event");
        app.handle_ui_event(event, &tx);
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);

        assert!(rendered.contains("EDIT REFUSED"));
        assert!(rendered.contains("Gate protected this repository"));
        assert!(rendered.contains("blast radius"));
        assert!(rendered.contains("2 files"));
        assert!(rendered.contains("6 changed lines"));
        assert!(rendered.contains("Ask Estelle"));
    }

    #[test]
    fn calm_frame_composes_symbol_art_behind_content_without_moving_the_composer() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        let started = Instant::now();
        let first = rendered_frame_at_size(&app, started, 120, 32);
        let second = rendered_frame_at_size(&app, started + Duration::from_secs(1), 120, 32);

        assert_eq!(first, second, "the calm scene moved between render ticks");
        assert!(first.contains("Ask about uqeu/estelle"));
        assert!(first.contains("/review"));
        assert!(first.contains('·') || first.contains('∷'));
        for rejected in ["err", "NaN", "EOF", "0x", "404", "500"] {
            assert!(
                !first.contains(rejected),
                "rejected dither fragment returned: {rejected}"
            );
        }
        let buffer = rendered_buffer_at_size(&app, started, 120, 32);
        for cell in &buffer.content {
            let is_braille = cell
                .symbol()
                .chars()
                .next()
                .is_some_and(|character| ('\u{2801}'..='\u{28ff}').contains(&character));
            if is_braille {
                assert_eq!(
                    cell.fg, FATE_RED,
                    "only the earned red spider lily may use Braille material"
                );
            }
        }
        assert!(first.contains("Ask Estelle"));
    }

    #[test]
    fn composer_caret_moves_the_separate_dither_wake_without_moving_the_transcript() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        let (tx, _rx) = mpsc::unbounded_channel();
        let transcript_before = render_transcript(&app.transcript);
        app.composer.set_text("charge path");
        for _ in 0..6 {
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                &tx,
            );
        }
        let first = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        for _ in 0..5 {
            handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                &tx,
            );
        }
        let second = rendered_frame_at_size(&app, Instant::now(), 120, 32);

        assert_ne!(first, second, "the caret wake did not follow the caret");
        assert_eq!(render_transcript(&app.transcript), transcript_before);
        assert_eq!(app.composer.text(), "charge path");
        assert_eq!(
            app.dither_wake.iter().copied().collect::<Vec<_>>(),
            [7, 8, 9, 10, 11]
        );
    }

    #[test]
    fn calm_canvas_render_cost_stays_below_the_frame_cadence() {
        const RUNS: u32 = 500;
        let dither = test_app();
        let mut plain = test_app();
        plain
            .transcript
            .push(TranscriptEntry::System("plain baseline".to_string()));
        let average_micros = |app: &App| {
            let started = Instant::now();
            for _ in 0..RUNS {
                std::hint::black_box(rendered_frame_at_size(app, Instant::now(), 120, 32));
            }
            started.elapsed().as_micros() / u128::from(RUNS)
        };
        let plain_micros = average_micros(&plain);
        let dither_micros = average_micros(&dither);
        assert!(
            dither_micros < FRAME_INTERVAL.as_micros() / 10,
            "symbol ground consumed more than 10% of the frame cadence \
             (plain={plain_micros}us, dither={dither_micros}us, cadence=100000us)"
        );
    }

    #[test]
    fn numstat_keeps_newlines_and_tabs_inside_one_filename() {
        let output = b"3\t2\tdir/name\nwith\ttabs.rs\0";
        assert_eq!(
            parse_git_numstat(output),
            [DiffFileStat {
                path: "dir/name\nwith\ttabs.rs".to_string(),
                changed_lines: 5,
            }]
        );
    }

    #[test]
    fn mouse_wheel_scrolls_the_transcript_without_recalling_composer_history() {
        let mut app = test_app();
        for index in 0..40 {
            app.transcript
                .push(TranscriptEntry::System(format!("message {index}")));
        }
        app.composer.set_text("current draft");
        let before = rendered_frame(&app, Instant::now());
        assert!(before.contains("message 39"));

        handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 4,
                row: 8,
                modifiers: KeyModifiers::NONE,
            },
        );

        assert!(
            app.transcript_scroll > 0,
            "wheel input did not move the viewport"
        );
        assert_eq!(app.composer.text(), "current draft");
        let scrolled = rendered_frame(&app, Instant::now());
        assert!(!scrolled.contains("message 39"));
        assert!(scrolled.contains("message 37"));
    }

    #[test]
    fn terminal_session_requests_mouse_events_instead_of_wheel_to_arrow_translation() {
        let mut enter = Vec::new();
        enter_terminal_screen(&mut enter).expect("enter commands");
        let enter = String::from_utf8(enter).expect("ANSI enter sequence");
        assert!(
            enter.contains("\u{1b}[?1000h"),
            "mouse reporting was not enabled"
        );
        assert!(
            enter.contains("\u{1b}[?1006h"),
            "SGR mouse mode was not enabled"
        );

        let mut leave = Vec::new();
        leave_terminal_screen(&mut leave).expect("leave commands");
        let leave = String::from_utf8(leave).expect("ANSI leave sequence");
        assert!(
            leave.contains("\u{1b}[?1000l"),
            "mouse reporting was not disabled"
        );
        assert!(
            leave.contains("\u{1b}[?1006l"),
            "SGR mouse mode was not disabled"
        );
    }

    #[test]
    fn snapshot_p6_citation_pane() {
        let mut app = test_app();
        let sources = vec![
            Source {
                file: "api/charge.ts".to_string(),
                line: Some(52),
                extra: serde_json::Map::new(),
            },
            Source {
                file: "billing/retry.ts".to_string(),
                line: Some(118),
                extra: serde_json::Map::new(),
            },
        ];
        app.citations = sources.clone();
        app.transcript.push(TranscriptEntry::User(
            "Why is this charge retried?".to_string(),
        ));
        app.transcript.push(TranscriptEntry::Answer {
            text: "The retry is bounded by the idempotency guard.".to_string(),
            grounded: Some(true),
            degraded: false,
            sources,
        });
        insta::assert_snapshot!(rendered_frame_at_size(&app, Instant::now(), 120, 30));
    }

    #[test]
    fn snapshot_p6_sweep_gauge() {
        let mut app = test_app();
        app.sweep_progress = Some(top_level::SweepProgress {
            state: "sending complete source set".to_string(),
            percent: 35.0,
            files: 184,
            bytes: 912_408,
        });
        insta::assert_snapshot!(rendered_frame(&app, Instant::now()));
    }

    #[test]
    fn snapshot_p6_gate_refusal() {
        let mut app = test_app();
        app.gate_modal = Some(GateModal {
            verdict: "blocked".to_string(),
            reasons: vec![
                "api/charge.ts:52  invented call rotate_all_keys does not exist".to_string(),
            ],
            files: vec![
                DiffFileStat {
                    path: "api/charge.ts".to_string(),
                    changed_lines: 5,
                },
                DiffFileStat {
                    path: "billing/retry.ts".to_string(),
                    changed_lines: 1,
                },
            ],
        });
        insta::assert_snapshot!(rendered_frame_at_size(&app, Instant::now(), 120, 32));
    }

    #[test]
    fn empty_state_uses_lily_ink_not_error_or_binary_fragments() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 34);

        for forbidden in ["err", "500", "NaN", "0x", "EOF", "404"] {
            assert!(
                !rendered.contains(forbidden),
                "empty-state ground leaked forbidden fragment {forbidden:?}\n{rendered}"
            );
        }
        assert!(rendered.contains('∷'), "lily ink was absent\n{rendered}");
    }

    #[test]
    fn welcome_scene_stays_while_the_first_message_is_composed_and_leaves_on_submission() {
        let mut app = test_app();
        app.composer.set_text("where does charge fail?");
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 34);
        assert!(
            rendered.contains("Ask about"),
            "welcome copy was erased while the first message was still being composed\n{rendered}"
        );

        // The scene's lifecycle owner is "has the first message been submitted", not "is the
        // composer empty" — typing must not dim the ground either.
        let ground = |app: &App| {
            let backend = TestBackend::new(120, 34);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| render_symbol_ground(frame, frame.area(), app))
                .expect("render ground");
            format!("{}", terminal.backend())
        };
        let empty_composer = ground(&app);
        app.composer.set_text("where does charge fail? cont");
        let typed_composer = ground(&app);
        assert_eq!(
            empty_composer, typed_composer,
            "the ground scene dimmed merely because the composer is non-empty"
        );

        let (tx, _rx) = mpsc::unbounded_channel();
        app.submit("where does charge fail?".to_string(), &tx);
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 34);
        assert!(
            !rendered.contains("Ask about"),
            "welcome copy survived the first submitted turn\n{rendered}"
        );
    }

    #[test]
    fn terminal_lily_uses_the_websites_scene_anchors_and_full_petal_mass() {
        assert!(spider_lily_coverage(0.00, 0.00) > 0.90, "heart");
        assert!(spider_lily_coverage(-0.58, -0.34) > 0.80, "left ribbon");
        assert!(spider_lily_coverage(0.70, -0.22) > 0.80, "right ribbon");
        assert!(spider_lily_coverage(0.17, -0.98) > 0.80, "upper anther");
        assert!(spider_lily_coverage(-0.01, 0.35) > 0.70, "short stem");
        assert_eq!(spider_lily_coverage(-1.30, 0.80), 0.0, "negative space");
        assert!(scene_coverage(85, 13, 100, 100) > 0.05);
        assert!(scene_coverage(50, 95, 100, 100) > 0.30);
    }

    #[test]
    fn persistent_lily_stays_subtle_at_a_wide_terminal_size() {
        let app = test_app();
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 190, 50);
        let points = buffer
            .content
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.fg == FATE_RED && !cell.symbol().trim().is_empty())
            .map(|(index, _)| (index % 190, index / 190))
            .collect::<Vec<_>>();
        assert!(!points.is_empty(), "the earned red lily did not paint");
        let min_x = points.iter().map(|(x, _)| *x).min().unwrap_or(0);
        let max_x = points.iter().map(|(x, _)| *x).max().unwrap_or(0);
        let min_y = points.iter().map(|(_, y)| *y).min().unwrap_or(0);
        let max_y = points.iter().map(|(_, y)| *y).max().unwrap_or(0);
        assert!(
            max_x - min_x <= 42,
            "lily was too wide: {} cells",
            max_x - min_x
        );
        assert!(
            max_y - min_y <= 14,
            "lily was too tall: {} cells",
            max_y - min_y
        );
    }

    #[test]
    fn first_question_keeps_the_session_handoff_in_the_transcript() {
        let mut app = test_app();
        app.session_context = Some(session_gap::SessionContext {
            human_lines: vec![
                "Welcome back. You were away about 5 hours.".to_string(),
                "Elsewhere while you were away: 48 committed file changes.".to_string(),
            ],
            model_context: "session context".to_string(),
        });
        let (tx, _rx) = mpsc::unbounded_channel();

        app.submit("what changed?".to_string(), &tx);

        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        assert!(rendered.contains("Since your last session"));
        assert!(rendered.contains("Welcome back. You were away about 5 hours."));
        assert!(rendered.contains("you  what changed?"));
    }

    #[test]
    fn unresolved_header_values_are_omitted_instead_of_rendered_as_ellipses() {
        let app = test_app();
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);

        assert!(
            rendered
                .lines()
                .next()
                .is_some_and(|line| !line.contains("..."))
        );
        assert!(!format!("{:?}", status_line(&app, Instant::now())).contains("..."));
    }

    #[test]
    fn composer_is_a_bounded_bottom_input_surface() {
        let app = test_app();
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        let lines = rendered.lines().collect::<Vec<_>>();
        let composer = lines
            .iter()
            .position(|line| line.contains("ASK ESTELLE"))
            .expect("bounded composer title");

        assert!(composer >= lines.len().saturating_sub(7));
        assert!(lines[composer].contains('┌'));
        assert!(rendered.contains("› Ask Estelle"));
    }

    #[test]
    fn focused_composer_keeps_the_estelle_canvas_background() {
        let app = test_app();
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 120, 32);
        let placeholder_row = buffer
            .content
            .chunks(120)
            .find(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
                    .contains("› Ask Estelle")
            })
            .expect("composer placeholder row");

        assert!(
            placeholder_row
                .iter()
                .all(|cell| cell.bg == app.theme.background()),
            "focused composer inherited a foreign message-fill background"
        );
    }

    #[test]
    fn dark_theme_inherits_the_terminal_background_instead_of_painting_ansi_black() {
        // Color::Black is ANSI 0 — a painted colour most terminal themes render as a grey
        // sheet. Color::Reset inherits the terminal's own background. Cream Ink is the
        // deliberate painted surface and stays painted.
        assert_eq!(Theme::Dark.background(), Color::Reset);
        assert_eq!(Theme::CreamInk.background(), FATE_BG);

        let app = test_app();
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 120, 32);
        assert!(
            buffer.content.iter().all(|cell| cell.bg == Color::Reset),
            "dark theme painted a background instead of inheriting the terminal"
        );
    }

    #[tokio::test]
    async fn a_single_rejection_keeps_the_credential_and_only_two_different_routes_remove_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {"message": "rejected"}
            })))
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("estelle_live_test-only").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let rejected = || async {
            client
                .account(&CancellationToken::new())
                .await
                .expect_err("the mock always 401s")
        };

        let home = tempfile::tempdir().expect("temp home");
        let store = CredentialStore::new(home.path().join(".estelle/auth.json"));
        store
            .write(&estelle_client::ApiKey::new("estelle_live_test-only").expect("key"))
            .expect("write credential");
        let path = store.path().to_path_buf();
        let mut app = test_app();
        app.auth = Some(AuthContext {
            store,
            source: CredentialSource::Stored,
        });

        app.clear_rejected(&rejected().await, "/me");
        assert!(
            path.exists(),
            "a single rejection deleted the stored credential"
        );
        let text = format!("{:?}", render_transcript(&app.transcript));
        assert!(text.contains("/me"), "the rejecting route was not named");
        assert!(text.contains("NOT removed"), "the keep was not disclosed");

        app.clear_rejected(&rejected().await, "/me");
        assert!(
            path.exists(),
            "the same route twice is still one route's word against the key"
        );

        app.clear_rejected(&rejected().await, "/deep-search");
        assert!(
            !path.exists(),
            "two different routes rejecting did not remove the credential"
        );
        let text = format!("{:?}", render_transcript(&app.transcript));
        assert!(
            text.contains("/me") && text.contains("/deep-search"),
            "the deletion did not name both routes"
        );
    }

    #[test]
    fn user_turns_render_as_filled_blocks_ported_from_codex_history_cell() {
        let mut app = test_app();
        app.theme = Theme::CreamInk;
        app.transcript
            .push(TranscriptEntry::User("trace the charge path".to_string()));
        app.transcript.push(TranscriptEntry::Answer {
            text: "at the retry loop.".to_string(),
            grounded: Some(true),
            degraded: false,
            sources: Vec::new(),
        });
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 120, 32);
        let expected_bg = codex_tui::user_message_style_for(Some((0xE9, 0xE6, 0xDC)))
            .bg
            .expect("a known terminal background yields a fill");
        let row_with = |needle: &str| {
            buffer
                .content
                .chunks(120)
                .find(|row| {
                    row.iter()
                        .map(ratatui::buffer::Cell::symbol)
                        .collect::<String>()
                        .contains(needle)
                })
                .unwrap_or_else(|| panic!("no rendered row for {needle:?}"))
        };
        let user_row = row_with("trace the charge path");
        let filled = user_row
            .iter()
            .filter(|cell| cell.bg == expected_bg)
            .count();
        assert!(
            filled >= "trace the charge path".len(),
            "the user turn did not render as a filled block (ported fill missing)"
        );
        let estelle_row = row_with("at the retry loop.");
        assert!(
            estelle_row.iter().all(|cell| cell.bg != expected_bg),
            "the assistant turn borrowed the user's fill — turns must stay distinguishable"
        );
    }

    #[tokio::test]
    async fn a_pasted_image_never_reaches_the_server_because_no_image_path_exists() {
        // PROBE, not a read. Part one: the inherited lib's image chord (Ctrl+V runs
        // paste_image_to_temp_png in Codex's chatwidget) is not wired here at all.
        let mut app = test_app();
        let (tx, _rx) = mpsc::unbounded_channel();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
            &tx,
        );
        assert!(
            app.composer.is_empty(),
            "Ctrl+V put something in the composer — an image path exists in this binary"
        );
        assert!(app.picker.is_none() && app.active.is_none());

        // Part two: a terminal that delivers an image paste as a file-path string sends TEXT.
        // The question goes verbatim (D16) and no image bytes, no image field, and no read of
        // the pasted file ever occur — the server has zero multimodal handling, and the client
        // must not invent one.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "answer": "IMAGE PROBE SENTINEL",
                "grounded": true,
                "sources": []
            })))
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let root = tempfile::tempdir().expect("working tree");
        let typed = "what does this show? /tmp/screenshot-with-key.png".to_string();
        let reply = answer_question(
            client,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            typed.clone(),
            None,
            &CancellationToken::new(),
        )
        .await
        .expect("answer");

        let requests = server.received_requests().await.expect("request recording");
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_slice(&requests[0].body).expect("json body");
        assert_eq!(
            body["question"].as_str(),
            Some(typed.as_str()),
            "the pasted path text must go verbatim, nothing more"
        );
        let raw = String::from_utf8_lossy(&requests[0].body);
        let key_count = body
            .as_object()
            .expect("body object")
            .keys()
            .map(String::as_str)
            .count();
        assert_eq!(
            key_count, 2,
            "only question and repo may ride the request: {raw}"
        );
        assert!(body.get("repo").is_some());
        assert_eq!(reply.text, "IMAGE PROBE SENTINEL");
    }

    #[test]
    fn transcript_turns_carry_distinguishable_speaker_labels() {
        let mut app = test_app();
        app.transcript
            .push(TranscriptEntry::User("where does charge fail?".to_string()));
        app.transcript.push(TranscriptEntry::Answer {
            text: "at the retry loop in billing/charge.rs.".to_string(),
            grounded: Some(true),
            degraded: false,
            sources: Vec::new(),
        });
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);
        let row_count = |needle: &str| {
            rendered
                .lines()
                .filter(|line| line.contains(needle))
                .count()
        };
        assert_eq!(
            row_count("you  where does charge fail?"),
            1,
            "exactly one user-labelled turn\n{rendered}"
        );
        assert_eq!(
            row_count("estelle  grounded"),
            1,
            "exactly one assistant-labelled turn\n{rendered}"
        );

        // The labels must be distinguishable in the rendered buffer, not merely present in
        // the model: different ink, and only the assistant label is bold.
        let buffer = rendered_buffer_at_size(&app, Instant::now(), 120, 32);
        let label_cell = |needle: &str| {
            buffer
                .content
                .chunks(120)
                .find_map(|row| {
                    let text = row
                        .iter()
                        .map(ratatui::buffer::Cell::symbol)
                        .collect::<String>();
                    text.contains(needle).then(|| row[1].clone())
                })
                .unwrap_or_else(|| panic!("no rendered row for {needle:?}"))
        };
        let user_label = label_cell("you  where does charge fail?");
        let estelle_label = label_cell("estelle  grounded");
        assert_ne!(
            user_label.fg, estelle_label.fg,
            "speaker labels share ink and are not glanceable"
        );
        assert!(!user_label.modifier.contains(Modifier::BOLD));
        assert!(estelle_label.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn picker_number_keys_select_directly_ported_from_codex_list_selection() {
        let mut app = test_app();
        app.picker = Some(PickerSurface::themes(&app));
        assert_eq!(app.theme, Theme::Dark);

        // The numbered badge is visible before any key is pressed.
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 40);
        assert!(
            rendered.contains("2 Estelle Cream Ink"),
            "the second row did not carry its number badge\n{rendered}"
        );

        // One digit selects and activates — no arrow keys, no Enter.
        let (tx, _rx) = mpsc::unbounded_channel();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
            &tx,
        );
        assert_eq!(
            app.theme,
            Theme::CreamInk,
            "pressing 2 did not select the second row"
        );
    }

    #[test]
    fn picker_replaces_the_composer_in_one_bottom_anchored_dock() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.picker = Some(PickerSurface::settings(&app));
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 40);
        let lines = rendered.lines().collect::<Vec<_>>();
        let picker = lines
            .iter()
            .position(|line| line.contains("SETTINGS"))
            .expect("settings picker title");
        assert!(!rendered.contains("ASK ESTELLE"));
        assert!(
            picker >= lines.len().saturating_sub(20),
            "picker was not bottom-anchored\n{rendered}"
        );
    }

    #[test]
    fn orchestra_owns_the_primary_surface_without_empty_state_or_dither() {
        let mut app = test_app();
        app.prod_panel_visible = false;
        app.fleet = Some(
            serde_json::from_value(json!({
                "id": "orch-test",
                "batch": "Trace checkout failures",
                "models": ["GPT-5.5"],
                "state": "running",
                "revision": 1,
                "observed_at": 4102444800.0,
                "completed": 0,
                "total": 1,
                "attempt": "first",
                "narrator": {"text": "One agent is tracing checkout", "evidence": "observed"},
                "agents": [{
                    "index": 1,
                    "status": "running",
                    "state_observed_at": 4102444800.0,
                    "current_action": "Reading billing/charge.rs",
                    "progress": {"completed": 0, "total": 1}
                }]
            }))
            .expect("fleet fixture"),
        );
        let rendered = rendered_frame_at_size(&app, Instant::now(), 120, 32);

        assert!(rendered.contains("Estelle Orchestra"));
        assert!(!rendered.contains("Ask about"));
        assert!(!rendered.contains("/sweep another repo"));
    }
}
