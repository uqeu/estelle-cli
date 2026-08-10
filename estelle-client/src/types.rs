use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct NoQuery;

#[derive(Clone, Debug, Deserialize)]
pub struct AccountResponse {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub plan_active: Option<bool>,
    #[serde(default)]
    pub plan_cap: Option<u64>,
    #[serde(default)]
    pub extra_capacity: Option<u64>,
    #[serde(default)]
    pub seats: Option<u64>,
    #[serde(default)]
    pub team: Option<TeamIdentity>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TeamIdentity {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_owner: bool,
    #[serde(default)]
    pub owner_email: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OverviewResponse {
    #[serde(default)]
    pub memory: Option<MemoryOverview>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryOverview {
    #[serde(default)]
    pub memories: Option<u64>,
    #[serde(default)]
    pub repo_files: Option<u64>,
    #[serde(default)]
    pub entities: Option<u64>,
    #[serde(default)]
    pub by_repo: Vec<RepoOverview>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RepoOverview {
    pub repo: String,
    #[serde(default)]
    pub files: Option<u64>,
    #[serde(default)]
    pub chunks: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReposResponse {
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

impl ChatCompletionRequest {
    pub fn question(question: impl Into<String>) -> Self {
        Self {
            model: "estelle".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: question.into(),
            }],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeepSearchRequest {
    pub question: String,
    /// Local working memory as DATA (paths + contents + optional session context), never
    /// prose instructions — the server ignores this key until its typed `working_memory`
    /// contract (register item 14b) ships. `question` always carries the user's message
    /// verbatim; nothing is ever prepended to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_memory: Option<Value>,
}

impl DeepSearchRequest {
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            working_memory: None,
        }
    }

    pub fn with_working_memory(mut self, working_memory: Value) -> Self {
        self.working_memory = Some(working_memory);
        self
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeepSearchResponse {
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub grounded: Option<bool>,
    #[serde(default)]
    pub ungrounded: Vec<String>,
    #[serde(default)]
    pub citations: String,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub conversational: bool,
    #[serde(default)]
    pub scope_ask: bool,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub candidates: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl DeepSearchResponse {
    pub fn rendered_answer(&self) -> Option<&str> {
        self.answer
            .as_deref()
            .filter(|answer| !answer.trim().is_empty())
            .or_else(|| {
                self.scope_ask
                    .then_some(self.question.as_deref())
                    .flatten()
                    .filter(|question| !question.trim().is_empty())
            })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Source {
    pub file: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CommandReply {
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub wiki: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub meaning: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub unverified_reason: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub routed: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(default)]
    pub grounded: Option<bool>,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub scope_ask: bool,
    #[serde(default)]
    pub candidates: Vec<String>,
    #[serde(default)]
    pub ungrounded: Vec<String>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub run_count: Option<u64>,
    #[serde(default)]
    pub skill_count: Option<u64>,
    /// `/repos` sends a list of repo names; `/analytics` sends a per-repo tally OBJECT. Same key,
    /// two shapes — raw `Value`, arms take their own shape.
    #[serde(default)]
    pub repos: Option<serde_json::Value>,
    /// `/sessions` sends a list of session summaries; `/analytics` sends a COUNT. Same key, two
    /// shapes — raw `Value`; typed access via `session_summaries()`.
    #[serde(default)]
    pub sessions: Option<serde_json::Value>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    /// `/orchestra` and `/runs` send `runs` as a LIST of agent runs; `/analytics` sends it as a
    /// COUNT. Same key, two shapes, one envelope — kept as a raw `Value` so neither parse can
    /// fail or silently absorb the other; typed access goes through `agent_runs()`.
    #[serde(default)]
    pub runs: Option<serde_json::Value>,
    #[serde(default)]
    pub fleet: Option<FleetSnapshot>,
    #[serde(default)]
    pub todo: Option<TodoSnapshot>,
    #[serde(default)]
    pub sources: Vec<Source>,
    /// `GET /graph` summary counts and indexes. Counts are `Option` so a server that omits one
    /// renders "not returned" — unknown is never rendered as zero. `building` is the server's
    /// explicit cold-graph signal; `None` means the server did not disclose the state.
    #[serde(default, rename = "files")]
    pub graph_files: Option<u64>,
    /// `entities` arrives as a COUNT from `GET /graph` and as a LIST of `{symbol, files}` rows
    /// from `GET /entities` — same key, two shapes, one envelope type. Kept as a raw `Value` so
    /// neither parse can fail or silently absorb the other; each render arm takes its own shape
    /// and treats the wrong one as "not returned", never as zero or an empty list.
    #[serde(default, rename = "entities")]
    pub graph_entities: Option<serde_json::Value>,
    #[serde(default, rename = "subsystems")]
    pub graph_subsystems: Option<u64>,
    #[serde(default, rename = "cycles")]
    pub graph_cycles: Option<u64>,
    #[serde(default, rename = "building")]
    pub graph_building: Option<bool>,
    #[serde(default, rename = "file_index")]
    pub graph_file_index: Vec<GraphFileEntry>,
    #[serde(default, rename = "roots")]
    pub graph_roots: Vec<GraphRoot>,
    /// `GET /graph/nodes` payload — the drawable dependency graph itself. `truncated` doubles as
    /// the discriminator between this view and the `/graph` summary: only `/graph/nodes` sends it.
    #[serde(default, rename = "nodes")]
    pub graph_nodes: Vec<GraphNode>,
    #[serde(default, rename = "edges")]
    pub graph_edges: Vec<GraphEdge>,
    #[serde(default, rename = "truncated")]
    pub graph_truncated: Option<bool>,
    /// `GET /me/keys` — the caller's API keys. The server never returns a raw key (id + prefix +
    /// label + expiry state only), and this struct has no field that could hold one.
    #[serde(default, rename = "keys")]
    pub me_keys: Vec<KeySummary>,
    /// `GET /me/team` — `None` means the server sent `"team": null` (the caller is on no team) OR
    /// the key was absent; both render as an explicit state, never an invented roster.
    #[serde(default, rename = "team")]
    pub me_team: Option<TeamView>,
    /// `GET /memory/cards` — the learned-knowledge folders view. `folders` is the full taxonomy
    /// (zeros included) straight from the server; `None` means the server did not send it.
    #[serde(default, rename = "cards")]
    pub memory_cards: Vec<MemoryCard>,
    #[serde(default, rename = "folders")]
    pub memory_folders: Option<Map<String, Value>>,
    /// `GET /usage` — requests + tokens bucketed by day, oldest to newest.
    #[serde(default, rename = "series")]
    pub usage_series: Vec<UsagePoint>,
    /// `GET /activity` — calls + tokens rolled up by endpoint, busiest first. `models` splits
    /// tokens by the model that ACTUALLY served them, so a fallback reads as its own line.
    #[serde(default, rename = "by_endpoint")]
    pub activity_rows: Vec<ActivityRow>,
    /// `GET /memories` — the "what do you know about my code" listing. `trust`/`may_ground`/
    /// `externally_authored` are the server's content-trust labels: which held items may certify
    /// an answer is the first fact about a memory, not a footnote.
    #[serde(default, rename = "memories")]
    pub memory_items: Vec<MemoryItem>,
    /// `GET /audit` — the tamper-evident trail of privileged actions. `actor` fields are
    /// non-secret key prefixes by server contract; the chain's `state`/`reason`/`verification`
    /// ride the envelope (typed `reason`, `extra` for the rest).
    #[serde(default, rename = "entries")]
    pub audit_entries: Vec<AuditEntry>,
    /// `GET /requests` — the billable engine-call stream, newest first. `count` (typed) is the
    /// log's TOTAL, so the view can say "N of total" rather than implying the page is all.
    #[serde(default, rename = "requests")]
    pub request_records: Vec<RequestRecord>,
    /// `/leaderboard` sends skill rows; `/team/leaderboard` sends MEMBER rows — same key,
    /// different shapes. Raw `Value` like `entities`/`runs`/`sessions`: each arm parses its own
    /// shape and the wrong one can never be silently absorbed into all-None fields.
    #[serde(default, rename = "leaderboard")]
    pub leaderboard: Option<serde_json::Value>,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub verdict: Option<serde_json::Value>,
    #[serde(default)]
    pub gate: Option<serde_json::Value>,
    #[serde(default)]
    pub merge: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl CommandReply {
    /// The `runs` key as typed agent runs — non-empty only when the server sent a LIST
    /// (`/orchestra`, `/runs`). `/analytics` sends a count in the same key; that arm reads it
    /// with `as_u64` instead. Anything unparseable yields an empty vec, never a failed reply.
    pub fn agent_runs(&self) -> Vec<AgentRun> {
        self.runs
            .as_ref()
            .and_then(|runs| serde_json::from_value(runs.clone()).ok())
            .unwrap_or_default()
    }

    /// The `sessions` key as typed session summaries — non-empty only when the server sent a
    /// LIST (`/sessions`). `/analytics` sends a count in the same key; that arm reads it with
    /// `as_u64` instead.
    pub fn session_summaries(&self) -> Vec<SessionSummary> {
        self.sessions
            .as_ref()
            .and_then(|sessions| serde_json::from_value(sessions.clone()).ok())
            .unwrap_or_default()
    }

    /// The `leaderboard` key as typed SKILL rows — non-empty only for `/leaderboard`.
    pub fn skill_leaderboard_rows(&self) -> Vec<LeaderboardRow> {
        self.leaderboard
            .as_ref()
            .and_then(|rows| serde_json::from_value(rows.clone()).ok())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphFileEntry {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub symbols: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphRoot {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub files: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub symbols: Option<u64>,
    #[serde(default)]
    pub subsystem: Option<u64>,
    #[serde(default)]
    pub weight: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GraphEdge {
    #[serde(default, rename = "from")]
    pub from_path: String,
    #[serde(default, rename = "to")]
    pub to_path: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct KeySummary {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub expired: Option<bool>,
    #[serde(default)]
    pub revoked: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TeamView {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub you: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub you_are_owner: Option<bool>,
    #[serde(default)]
    pub seats: Option<u64>,
    #[serde(default)]
    pub members: Vec<TeamMember>,
    #[serde(default)]
    pub seat_ledger: Option<SeatLedger>,
    #[serde(default)]
    pub invites: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TeamMember {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SeatLedger {
    #[serde(default)]
    pub purchased: Option<u64>,
    #[serde(default)]
    pub used: Option<u64>,
    #[serde(default)]
    pub pending: Option<u64>,
    #[serde(default)]
    pub available: Option<u64>,
    #[serde(default)]
    pub full: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MemoryCard {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub edited: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UsagePoint {
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub requests: Option<u64>,
    #[serde(default)]
    pub tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityRow {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub models: Option<Map<String, Value>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MemoryItem {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub chunks: Option<u64>,
    #[serde(default)]
    pub trust: Option<String>,
    #[serde(default)]
    pub may_ground: Option<bool>,
    #[serde(default)]
    pub externally_authored: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AuditEntry {
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RequestRecord {
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LeaderboardRow {
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub uses: Option<u64>,
    #[serde(default)]
    pub successes: Option<u64>,
    #[serde(default)]
    pub success_rate: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FleetSnapshot {
    pub id: String,
    pub batch: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub model: String,
    pub state: String,
    #[serde(default)]
    pub revision: u64,
    pub observed_at: f64,
    #[serde(default = "default_fleet_stale_after_s")]
    pub stale_after_s: u64,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub agents: Vec<FleetAgent>,
    #[serde(default)]
    pub narrator: Option<FleetObservedText>,
    #[serde(default)]
    pub attempt: FleetAttempt,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FleetAgent {
    pub index: u64,
    pub status: FleetAgentStatus,
    pub state_observed_at: f64,
    #[serde(default)]
    pub unknown_reason: Option<String>,
    #[serde(default)]
    pub current_action: Option<String>,
    #[serde(default)]
    pub progress: Option<FleetAgentProgress>,
    #[serde(default)]
    pub assignments: FleetAssignmentCounts,
    #[serde(default)]
    pub failure_cause: Option<FleetObservedText>,
    #[serde(default)]
    pub attempt: FleetAttempt,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FleetAssignmentCounts {
    #[serde(default)]
    pub attempted: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub lost: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FleetObservedText {
    pub text: String,
    #[serde(default)]
    pub evidence: FleetEvidence,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FleetEvidence {
    Measured,
    Observed,
    Derived,
    Inferred,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FleetAttempt {
    First,
    Retry,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FleetAgentProgress {
    pub completed: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TodoSnapshot {
    pub observed_at: f64,
    #[serde(default = "default_fleet_stale_after_s")]
    pub stale_after_s: u64,
    #[serde(default)]
    pub items: Vec<TodoItem>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TodoItem {
    pub title: String,
    pub status: TodoStatus,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub evidence: FleetEvidence,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Done,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorIssuesResponse {
    #[serde(default)]
    pub issues: Vec<MonitorIssue>,
    #[serde(default)]
    pub counts: Map<String, Value>,
    #[serde(default)]
    pub excluded_unbound: u64,
    #[serde(default)]
    pub window_s: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub next_since: Option<f64>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorIssue {
    pub key: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub symbol_range: Option<SymbolRange>,
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub culprit: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub sample: String,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub events_in_window: Option<u64>,
    #[serde(default)]
    pub first_seen: f64,
    #[serde(default)]
    pub last_seen: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub bind_status: String,
    #[serde(default)]
    pub bind_detail: String,
    #[serde(default)]
    pub repair_status: String,
    #[serde(default)]
    pub repair_detail: String,
    #[serde(default)]
    pub repair_pr: String,
    #[serde(default)]
    pub repair_gate_state: Option<String>,
    #[serde(default)]
    pub repair_gate_verdict: Option<String>,
    #[serde(default)]
    pub gate_absent_reason: Option<String>,
    #[serde(default)]
    pub tickets: Vec<Value>,
    #[serde(default)]
    pub cursor: Option<f64>,
    #[serde(default)]
    pub signal: Option<IssueSignal>,
    #[serde(default)]
    pub bound: Option<IssueBinding>,
    #[serde(default)]
    pub repair: Option<IssueRepair>,
    #[serde(default)]
    pub gate: Option<IssueGate>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MonitorIssue {
    pub fn display_title(&self) -> &str {
        self.signal
            .as_ref()
            .map(|signal| signal.title.as_str())
            .filter(|title| !title.trim().is_empty())
            .or_else(|| (!self.title.trim().is_empty()).then_some(self.title.as_str()))
            .or_else(|| {
                self.signal
                    .as_ref()
                    .map(|signal| signal.error_type.as_str())
                    .filter(|kind| !kind.trim().is_empty())
            })
            .or_else(|| (!self.error_type.trim().is_empty()).then_some(self.error_type.as_str()))
            .unwrap_or("production issue")
    }

    pub fn event_count(&self) -> u64 {
        self.events_in_window.unwrap_or_else(|| {
            self.signal
                .as_ref()
                .map_or(self.count, |signal| signal.count)
        })
    }

    pub fn bound_location(&self) -> Option<(&str, u64)> {
        self.bound
            .as_ref()
            .and_then(|bound| bound.file.as_deref().zip(bound.line))
            .or_else(|| {
                self.symbol_range
                    .as_ref()
                    .map(|range| (range.file.as_str(), range.line_start))
            })
    }

    pub fn effective_bind_status(&self) -> &str {
        self.bound
            .as_ref()
            .map(|bound| bound.status.as_str())
            .filter(|status| !status.trim().is_empty())
            .unwrap_or(self.bind_status.as_str())
    }

    pub fn effective_bind_detail(&self) -> &str {
        self.bound
            .as_ref()
            .map(|bound| bound.detail.as_str())
            .filter(|detail| !detail.trim().is_empty())
            .unwrap_or(self.bind_detail.as_str())
    }

    pub fn effective_repair_status(&self) -> &str {
        self.repair
            .as_ref()
            .map(|repair| repair.status.as_str())
            .filter(|status| !status.trim().is_empty())
            .unwrap_or(self.repair_status.as_str())
    }

    pub fn effective_repair_pr(&self) -> &str {
        self.repair
            .as_ref()
            .and_then(|repair| repair.pr.as_deref())
            .filter(|pr| !pr.trim().is_empty())
            .unwrap_or(self.repair_pr.as_str())
    }

    pub fn effective_gate_verdict(&self) -> Option<&str> {
        self.gate
            .as_ref()
            .map(|gate| gate.verdict.as_str())
            .filter(|verdict| !verdict.trim().is_empty())
            .or_else(|| {
                self.repair_gate_verdict
                    .as_deref()
                    .filter(|verdict| !verdict.trim().is_empty())
            })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct IssueSignal {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub culprit: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub first_seen: f64,
    #[serde(default)]
    pub last_seen: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct IssueBinding {
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct IssueRepair {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub pr: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct IssueGate {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub blockers: u64,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SymbolRange {
    pub symbol: String,
    pub file: String,
    pub line_start: u64,
    pub line_end: u64,
    pub repo: String,
    pub resolved_by: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorOverviewResponse {
    #[serde(default)]
    pub series: Option<MonitorSeries>,
    #[serde(default)]
    pub error_rate: Option<LegacyErrorSeries>,
    #[serde(default)]
    pub counts: Map<String, Value>,
    #[serde(default)]
    pub uptime_checks: Vec<Value>,
    #[serde(default)]
    pub uptime: MonitorUptimeCounts,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorUptimeCounts {
    #[serde(default)]
    pub checks: u64,
    #[serde(default)]
    pub up: u64,
    #[serde(default)]
    pub down: u64,
}

impl MonitorOverviewResponse {
    pub fn error_buckets(&self) -> Vec<MonitorErrorBucket> {
        if let Some(series) = &self.series {
            return series.buckets.clone();
        }
        self.error_rate
            .as_ref()
            .map(|series| {
                series
                    .series
                    .iter()
                    .map(|bucket| MonitorErrorBucket {
                        t: bucket.start,
                        errors: bucket.count,
                        requests: None,
                        p99_ms: None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn requests_source(&self) -> Option<&str> {
        self.series
            .as_ref()
            .and_then(|series| series.requests_source.as_deref())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorSeries {
    #[serde(default)]
    pub window_s: u64,
    #[serde(default)]
    pub bucket_s: u64,
    #[serde(default)]
    pub buckets: Vec<MonitorErrorBucket>,
    #[serde(default)]
    pub requests_source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MonitorErrorBucket {
    #[serde(default, alias = "start")]
    pub t: f64,
    #[serde(default, alias = "count")]
    pub errors: u64,
    #[serde(default)]
    pub requests: Option<u64>,
    #[serde(default)]
    pub p99_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LegacyErrorSeries {
    #[serde(default)]
    pub window_s: f64,
    #[serde(default)]
    pub buckets: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub series: Vec<LegacyErrorBucket>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LegacyErrorBucket {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub count: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FleetAgentStatus {
    #[default]
    Unknown,
    Created,
    Starting,
    Queued,
    Running,
    AwaitingApproval,
    #[serde(alias = "done")]
    Completed,
    Failed,
    TimedOut,
    Killed,
    Lost,
    Blocked,
    NeedsInput,
    Cancelled,
}

const fn default_fleet_stale_after_s() -> u64 {
    60
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SessionSummary {
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub run_count: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Finding {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Proposal {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub suggested_action: Option<String>,
    #[serde(default)]
    pub verdict: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentRun {
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub subtask: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub grounded: Option<bool>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

impl ChatCompletionResponse {
    pub fn answer(&self) -> Option<&str> {
        self.choices
            .first()
            .map(|choice| choice.message.content.as_str())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ErrorBody {
    pub message: String,
}
