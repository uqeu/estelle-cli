//! Synchronous review of the exact answer the host is submitting at Stop.
//!
//! This module does not call a model or invent a service endpoint. The caller awaits the existing
//! scoped `/verify` request, then passes the response to `parse_report`. Legacy grounding success,
//! zero extraction, transport failure, and an exhausted repair budget are never approval. Unknown
//! final prose is disclosed without a false pass; confirmed contradictions require repair.
//!
//! A host retry is reviewed again, including when `stop_hook_active` is true. At most two changed
//! repairs are requested in one turn. Repeated bytes do not earn a fresh repair grant; they pause
//! explicitly rather than creating an automatic Stop loop. Local state contains hashes only, never
//! conversation text, code, or credentials. A concurrent invocation or unreadable state pauses.

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use estelle_tui::host_transcript;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

pub const MAX_REPAIRS: u8 = 2;
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;
const MAX_STATE_BYTES: u64 = 4096;
const MAX_FINDINGS: usize = 256;
const MAX_REASON_CHARS: usize = 12_000;
const STATE_VERSION: u8 = 1;

/// Only host-provided fields and the already-resolved repository scope belong here.
/// `root` must be the checkout chosen by the runner, not an unvalidated payload path.
pub struct ReviewInput<'a> {
    pub repo: &'a str,
    pub root: &'a Path,
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub stop_hook_active: bool,
    pub last_assistant_message: Option<&'a str>,
    pub transcript_path: Option<&'a Path>,
    pub head: Option<&'a str>,
    pub working_tree_clean: bool,
}

#[derive(Clone, Debug)]
pub struct ReviewTarget {
    pub repo: String,
    pub answer: String,
    pub answer_sha256: String,
    pub latest_user_message: Option<String>,
    pub head: Option<String>,
    pub working_tree_clean: bool,
    pub scope_key: String,
    pub turn_key: String,
    pub stop_hook_active: bool,
    explicit_turn: bool,
}

impl ReviewTarget {
    /// `post_scoped(Endpoint::Verify, repo, ...)` adds the existing repo field.
    pub fn request_body(&self) -> Value {
        let mut body = json!({"verify_mode": "answer", "answer": self.answer, "working_tree_clean": self.working_tree_clean});
        if let Some(text) = &self.latest_user_message {
            body["latest_user_message"] = json!(text);
        }
        if let Some(head) = &self.head {
            body["head"] = json!(head);
        }
        body
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Issue(pub String);

impl Issue {
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// Unknown final prose is disclosed, not silently approved or blocked as a contradiction.
    pub fn envelope(&self) -> String {
        unknown_envelope(&self.0)
    }
}

fn digest(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        // Length framing prevents different tuples from hashing the same concatenation.
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

fn answer_digest(answer: &str) -> String {
    format!("{:x}", Sha256::digest(answer.as_bytes()))
}

fn identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

/// Preserve exact answer bytes. Whitespace is checked for emptiness, never stripped or silently
/// truncated before review. A payload answer is authoritative; transcript fallback requires a
/// complete, readable transcript so an older assistant turn cannot masquerade as the latest one.
pub fn prepare(input: ReviewInput<'_>) -> Result<ReviewTarget, Issue> {
    if !identifier(input.repo) || !identifier(input.session_id) {
        return Err(Issue::new(
            "the repository or host session scope is missing or invalid",
        ));
    }
    let root = input
        .root
        .canonicalize()
        .map_err(|_| Issue::new("the selected repository root could not be resolved"))?;
    if !root.is_dir() {
        return Err(Issue::new(
            "the selected repository root is not a directory",
        ));
    }
    let root_text = root
        .to_str()
        .filter(|value| identifier(value))
        .ok_or_else(|| {
            Issue::new("the selected repository root cannot be represented unambiguously")
        })?;
    let agent = input.agent_id.unwrap_or("");
    if !agent.is_empty() && !identifier(agent) {
        return Err(Issue::new("the host agent identity is invalid"));
    }
    let explicit_turn = input.turn_id.filter(|value| !value.trim().is_empty());
    if explicit_turn.is_some_and(|value| !identifier(value)) {
        return Err(Issue::new("the host turn identity is invalid"));
    }
    let direct = input
        .last_assistant_message
        .filter(|value| !value.trim().is_empty());
    // A direct answer + host turn ID needs no transcript. Do not read irrelevant conversation
    // history merely because the host supplied a path.
    let transcript = if direct.is_none() || explicit_turn.is_none() {
        let path = input.transcript_path.ok_or_else(|| {
            Issue::new(
                "the host supplied neither an exact answer and turn identity nor a transcript",
            )
        })?;
        Some(read_transcript(path)?)
    } else {
        None
    };
    let mut target = prepare_from_records(
        input.repo,
        root_text,
        input.session_id,
        agent,
        explicit_turn,
        input.stop_hook_active,
        direct,
        transcript.as_deref(),
    )?;
    if let Some(head) = input.head {
        if !matches!(head.len(), 40 | 64) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Issue::new(
                "the measured checkout revision is not a valid commit digest",
            ));
        }
        target.head = Some(head.to_string());
    }
    target.working_tree_clean = input.working_tree_clean;
    Ok(target)
}

fn read_transcript(path: &Path) -> Result<Vec<Value>, Issue> {
    let bounded = host_transcript::read_bounded(path)
        .map_err(|_| Issue::new("the host transcript could not be read"))?;
    if bounded.truncated || bounded.read_bytes != bounded.file_bytes {
        return Err(Issue::new(
            "the host transcript is partial; the latest answer and turn cannot be established",
        ));
    }
    // The shared normalizer performs a lossy UTF-8 read for checkpointing. Review cannot
    // certify replacement bytes as the original assistant artifact.
    if bounded.text.contains('\u{fffd}') {
        return Err(Issue::new(
            "the host transcript contains ambiguous replacement characters; exact answer bytes cannot be established",
        ));
    }
    let parsed = host_transcript::host_records(&bounded.text);
    if parsed.dialect.is_none() || parsed.unreadable_lines > 0 || parsed.unrecognised_lines > 0 {
        return Err(Issue::new(
            "the host transcript is malformed or contains unsupported records",
        ));
    }
    Ok(parsed.records)
}

#[allow(clippy::too_many_arguments)]
fn prepare_from_records(
    repo: &str,
    root: &str,
    session: &str,
    agent: &str,
    explicit_turn: Option<&str>,
    stop_hook_active: bool,
    direct: Option<&str>,
    records: Option<&[Value]>,
) -> Result<ReviewTarget, Issue> {
    let records = records.unwrap_or_default();
    let latest_user = records
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, record)| {
            if record.get("type").and_then(Value::as_str) != Some("user") || is_sidechain(record) {
                return None;
            }
            let text = message_text(record)?;
            // User-shaped tool results are not a human turn boundary.
            (!text.trim().is_empty()).then_some((index, record, text))
        });
    let answer = match direct {
        Some(answer) => answer.to_string(),
        None => {
            // Select the final assistant record first, THEN extract text. Skipping a textless
            // final record would silently review an older answer after a new tool operation.
            let (index, record) = records
                .iter()
                .enumerate()
                .rev()
                .find(|(_, record)| {
                    record.get("type").and_then(Value::as_str) == Some("assistant")
                        && !is_sidechain(record)
                })
                .ok_or_else(|| {
                    Issue::new("no assistant output was extracted from the host transcript")
                })?;
            if latest_user
                .as_ref()
                .is_some_and(|(user_index, _, _)| *user_index >= index)
            {
                return Err(Issue::new(
                    "the extracted assistant output predates the latest user turn",
                ));
            }
            message_text(record)
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| Issue::new("the latest assistant record has no reviewable text"))?
        }
    };
    if answer.trim().is_empty() || answer.len() > MAX_ANSWER_BYTES {
        return Err(Issue::new(
            "the latest assistant output is empty or exceeds the review size limit; nothing was truncated or approved",
        ));
    }
    let turn = match explicit_turn {
        Some(turn) => digest(&[b"host-turn", turn.as_bytes()]),
        None => {
            let (index, record, _) = latest_user
                .as_ref()
                .ok_or_else(|| Issue::new("no current user turn identity could be established"))?;
            let serialized = serde_json::to_vec(record)
                .map_err(|_| Issue::new("the user turn identity could not be encoded"))?;
            digest(&[b"transcript-turn", &index.to_be_bytes(), &serialized])
        }
    };
    let scope = digest(&[
        repo.as_bytes(),
        root.as_bytes(),
        session.as_bytes(),
        agent.as_bytes(),
    ]);
    Ok(ReviewTarget {
        repo: repo.to_string(),
        answer_sha256: answer_digest(&answer),
        answer,
        // A Stop repair prompt can be a user-shaped transcript record. It is not a new
        // instruction from the human and must not be uploaded as their latest decision.
        latest_user_message: if stop_hook_active {
            None
        } else {
            latest_user.map(|(_, _, text)| text)
        },
        head: None,
        working_tree_clean: false,
        scope_key: scope.clone(),
        turn_key: digest(&[scope.as_bytes(), turn.as_bytes()]),
        stop_hook_active,
        explicit_turn: explicit_turn.is_some(),
    })
}

fn is_sidechain(record: &Value) -> bool {
    record
        .get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn message_text(record: &Value) -> Option<String> {
    let message = record.get("message")?;
    if record.get("type").and_then(Value::as_str) == Some("assistant") {
        if message
            .get("channel")
            .is_some_and(|channel| channel.as_str() != Some("final"))
        {
            return None;
        }
        if message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
            })
        {
            return None;
        }
    }
    match message.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => Some(
            blocks
                .iter()
                .filter_map(|block| {
                    (block.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| block.get("text").and_then(Value::as_str))
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Pass,
    Block,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    Supported,
    Contradicted,
    Unverifiable,
}

#[derive(Clone, Debug, Deserialize)]
struct Finding {
    claim: String,
    verdict: Verdict,
    reason: String,
    evidence: Vec<String>,
    repair: String,
    checked_by: String,
}

impl Finding {
    fn deterministic(&self) -> bool {
        matches!(
            self.checked_by.as_str(),
            "index" | "registry" | "literal_decision"
        )
    }
}

#[derive(Deserialize)]
struct SemanticReview {
    status: String,
    reason: String,
}

#[derive(Deserialize)]
struct WireReport {
    schema_version: u8,
    disposition: Disposition,
    answer_sha256: String,
    repo: String,
    coverage_complete: bool,
    evidence_revision: String,
    indexed_head: String,
    material_claims: usize,
    checked_claims: usize,
    findings: Vec<Finding>,
    reason: String,
    // Additive metadata is optional for earlier deterministic-only servers. It must never
    // turn a model's agreement into deterministic evidence or authorize a paid request.
    #[serde(default)]
    probabilistic_assessment: bool,
    #[serde(default)]
    semantic_review: Option<SemanticReview>,
}

impl WireReport {
    fn has_probabilistic_assessment(&self) -> bool {
        self.probabilistic_assessment
            || self
                .findings
                .iter()
                .any(|finding| finding.checked_by == "model")
            || self
                .semantic_review
                .as_ref()
                .is_some_and(|review| review.status != "unavailable")
    }
}

/// A normalized result constructed only after exact artifact, scope and coverage validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewOutcome {
    Pass { checked_claims: usize },
    NotApplicable { reason: String },
    NeedsRepair { detail: String },
    Unknown { detail: String },
}

impl ReviewOutcome {
    /// Timeouts, authentication failures and malformed responses enter the same non-approval path.
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unknown {
            detail: detail.into(),
        }
    }
}

/// Parse only the additive answer-review contract. A legacy `grounded: true` is not a Stop pass.
pub fn parse_report(target: &ReviewTarget, response: &Value) -> ReviewOutcome {
    let invalid = |detail: &str| ReviewOutcome::unavailable(detail);
    if response.get("error").is_some_and(|error| {
        !error.is_null() && error != &Value::Bool(false) && error != &Value::String(String::new())
    }) {
        return invalid("the server reported an error alongside its answer review");
    }
    let Some(raw) = response.get("answer_review") else {
        return invalid(
            "the server did not return the answer_review contract; legacy grounding is not final-answer review",
        );
    };
    let Ok(report) = serde_json::from_value::<WireReport>(raw.clone()) else {
        return invalid("the answer-review response is malformed or missing required fields");
    };
    if report.schema_version != 1
        || report.repo != target.repo
        || report.answer_sha256 != target.answer_sha256
    {
        return invalid(
            "the review schema, repository scope, or exact answer fingerprint does not match this output",
        );
    }
    if report.findings.len() > MAX_FINDINGS
        || report.material_claims > MAX_FINDINGS
        || report.checked_claims > report.material_claims
    {
        return invalid(
            "the answer-review coverage counters are invalid or exceed the supported limit",
        );
    }
    let structurally_valid = report.findings.iter().all(|finding| {
        !finding.claim.trim().is_empty()
            && !finding.reason.trim().is_empty()
            && matches!(
                finding.checked_by.as_str(),
                "index" | "registry" | "literal_decision" | "none" | "model"
            )
            && (finding.verdict == Verdict::Unverifiable
                || (finding.checked_by != "none"
                    && finding.evidence.iter().any(|e| !e.trim().is_empty())))
            && (finding.verdict != Verdict::Contradicted || !finding.repair.trim().is_empty())
    });
    if !structurally_valid {
        return invalid(
            "a review finding lacks a claim, reason, provenance, evidence, or required repair instructions",
        );
    }
    let checked = report
        .findings
        .iter()
        .filter(|f| f.verdict != Verdict::Unverifiable)
        .count();
    if checked != report.checked_claims {
        return invalid("the checked-claim count does not match the review evidence");
    }
    let has_index_evidence = report
        .findings
        .iter()
        .any(|finding| finding.checked_by == "index" && finding.verdict != Verdict::Unverifiable);
    if has_index_evidence
        && (!target.working_tree_clean
            || target.head.as_deref() != Some(report.indexed_head.as_str()))
    {
        return invalid("repository evidence is not bound to the measured checkout revision");
    }
    if checked > 0 && report.evidence_revision.trim().is_empty() {
        return invalid("checked claims have no bound evidence revision");
    }
    let complete = report.coverage_complete && report.findings.len() == report.material_claims;
    let probabilistic = report.has_probabilistic_assessment();
    match report.disposition {
        Disposition::Pass
            if complete
                && !probabilistic
                && report.material_claims > 0
                && report.checked_claims == report.material_claims
                && report
                    .findings
                    .iter()
                    .all(|f| f.verdict == Verdict::Supported && f.deterministic()) =>
        {
            ReviewOutcome::Pass {
                checked_claims: checked,
            }
        }
        Disposition::NotApplicable
            if complete
                && !probabilistic
                && report.material_claims == 0
                && report.checked_claims == 0
                && report.findings.is_empty()
                && !report.reason.trim().is_empty() =>
        {
            ReviewOutcome::NotApplicable {
                reason: report.reason,
            }
        }
        Disposition::Block
            if report
                .findings
                .iter()
                .any(|f| f.verdict == Verdict::Contradicted && f.deterministic()) =>
        {
            ReviewOutcome::NeedsRepair {
                detail: findings_detail(&report),
            }
        }
        Disposition::Unknown => ReviewOutcome::Unknown {
            detail: findings_detail(&report),
        },
        _ if probabilistic => ReviewOutcome::Unknown {
            detail: findings_detail(&report),
        },
        _ => invalid(
            "the server did not establish complete, supported coverage of this answer; zero checked claims is not a pass",
        ),
    }
}

fn findings_detail(report: &WireReport) -> String {
    let mut lines = Vec::new();
    if report.has_probabilistic_assessment() {
        lines.push("Separate probabilistic assessment: this native hook did not request semantic review or authorize a provider call. Model judgments are not deterministic verification and do not independently require repair.".to_string());
        if let Some(review) = &report.semantic_review {
            lines.push(format!(
                "Semantic assessment status: {}. {}",
                review.status, review.reason
            ));
        }
    }
    if !report.reason.trim().is_empty() {
        lines.push(report.reason.clone());
    }
    if !report.coverage_complete {
        lines.push("Review coverage is incomplete.".to_string());
    }
    for finding in &report.findings {
        if finding.verdict == Verdict::Supported {
            continue;
        }
        lines.push(format!("Claim: {}\nFinding ({}): {}\nRepair: {}", finding.claim,
            if finding.checked_by == "model" {
                "probabilistic model judgment, not a deterministic contradiction"
            } else {
                finding.checked_by.as_str()
            },
            finding.reason, if finding.repair.trim().is_empty() {
                "Obtain supporting evidence or remove/qualify this unsupported claim. Do not invent evidence."
            } else { finding.repair.as_str() }));
    }
    if lines.is_empty() {
        lines.push("The reviewer supplied no supported conclusion or actionable findings. Obtain evidence before presenting this output as verified.".to_string());
    }
    bounded_reason(&lines.join("\n"))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewState {
    version: u8,
    scope_key: String,
    turn_key: String,
    last_artifact: String,
    repair_grants: u8,
    last_stop_hook_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub state: ReviewState,
    /// Exactly one host JSON envelope. No second diagnostic JSON can overwrite the decision.
    pub envelope: String,
    pub approved: bool,
    pub unresolved: bool,
}

/// Pure state transition. Rechecking is the caller's job BEFORE this is invoked; no branch here
/// consults a cached approval or skips the current report because a Stop hook is already active.
pub fn decide(
    target: &ReviewTarget,
    previous: Option<&ReviewState>,
    outcome: ReviewOutcome,
) -> Result<Transition, Issue> {
    let previous = current_state(target, previous)?;
    let missing_retry_state = target.stop_hook_active && previous.is_none();
    let grants = previous.map_or(if missing_retry_state { MAX_REPAIRS } else { 0 }, |state| {
        state.repair_grants
    });
    let unchanged = previous.is_some_and(|state| state.last_artifact == target.answer_sha256);
    let mut state = ReviewState {
        version: STATE_VERSION,
        scope_key: target.scope_key.clone(),
        turn_key: target.turn_key.clone(),
        last_artifact: target.answer_sha256.clone(),
        repair_grants: grants,
        last_stop_hook_active: target.stop_hook_active,
    };
    let success = match &outcome {
        ReviewOutcome::Pass { checked_claims } if *checked_claims > 0 => Some(format!(
            "Estelle Stop review checked {checked_claims} material claim(s) in this exact answer under the declared narrow checks. This is not certification of unreviewed code, tests, or deployment."
        )),
        ReviewOutcome::NotApplicable { reason } => Some(format!(
            "Estelle Stop review: not applicable to this nonfactual answer ({reason}). No code or factual claims were certified."
        )),
        _ => None,
    };
    if let Some(message) = success {
        return Ok(Transition {
            state,
            envelope: json!({"systemMessage": bounded_reason(&message)}).to_string(),
            approved: true,
            unresolved: false,
        });
    }
    let detail = match outcome {
        ReviewOutcome::NeedsRepair { detail } => detail,
        ReviewOutcome::Unknown { detail } => {
            return Ok(Transition {
                state,
                envelope: unknown_envelope(&detail),
                approved: false,
                unresolved: true,
            });
        }
        _ => {
            return Ok(Transition {
                state,
                envelope: unknown_envelope("no material claims were checked"),
                approved: false,
                unresolved: true,
            });
        }
    };
    let (unresolved, instruction) = if missing_retry_state {
        (true, "The host is already retrying Stop but the matching repair state is unavailable. The current output was reviewed again; no automatic repair allowance can safely be granted. Pause and explain these confirmed contradictions to the user. This is unresolved, not approval.".to_string())
    } else if grants >= MAX_REPAIRS {
        (true, "Two repair attempts have been requested and the result remains unresolved. Do not attempt another automatic repair or claim approval. Pause and explain the remaining findings and missing evidence to the user; ask for a decision or additional evidence.".to_string())
    } else if unchanged && grants > 0 {
        (true, "The exact same answer was rechecked and is still unresolved. No additional repair attempt was granted. Do not repeat this Stop loop or claim approval. Pause and explain the unchanged findings to the user, or complete the already requested repair before seeking review again.".to_string())
    } else {
        state.repair_grants += 1;
        (
            false,
            format!(
                "Repair attempt {}/{MAX_REPAIRS}: address the findings above using the cited evidence and exact repair instructions. Correct or remove contradicted claims; pause unverifiable code and obtain evidence or clearly qualify unsupported assertions. Do not invent APIs, evidence, test results, or completion claims. Return the changed answer for a new synchronous review; the previous result does not apply to changed output.",
                state.repair_grants
            ),
        )
    };
    let detail = if detail.chars().count() > 9000 {
        format!(
            "{}\n[Additional findings omitted; this excerpt is not a complete repair list.]",
            detail.chars().take(9000).collect::<String>()
        )
    } else {
        detail
    };
    let reason = format!(
        "Estelle Stop review {}.\nFindings:\n{}\n{}",
        if unresolved {
            "is UNRESOLVED"
        } else {
            "blocked this output"
        },
        detail,
        instruction
    );
    let envelope = if unresolved {
        unresolved_envelope(&reason)
    } else {
        block_envelope(&reason)
    };
    Ok(Transition {
        state,
        envelope,
        approved: false,
        unresolved,
    })
}

fn current_state<'a>(
    target: &ReviewTarget,
    previous: Option<&'a ReviewState>,
) -> Result<Option<&'a ReviewState>, Issue> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    if previous.version != STATE_VERSION
        || previous.scope_key != target.scope_key
        || previous.repair_grants > MAX_REPAIRS
    {
        return Err(Issue::new(
            "the saved Stop review state has invalid version, scope, or repair counters",
        ));
    }
    if previous.turn_key != target.turn_key && (!target.stop_hook_active || target.explicit_turn) {
        return Ok(None);
    }
    Ok(Some(previous))
}

fn bounded_reason(reason: &str) -> String {
    if reason.chars().count() <= MAX_REASON_CHARS {
        return reason.to_string();
    }
    let mut bounded = reason.chars().take(MAX_REASON_CHARS).collect::<String>();
    bounded.push_str("\n[Additional review details omitted from this hook message; this is not full certification.]");
    bounded
}

fn block_envelope(reason: &str) -> String {
    json!({"decision": "block", "reason": bounded_reason(reason)}).to_string()
}

fn unknown_envelope(detail: &str) -> String {
    json!({"systemMessage": bounded_reason(&format!(
        "Estelle Stop review is UNVERIFIED: {detail}. This is not a pass or certification. \
         Verification remains incomplete; unsupported factual claims are not certified. Unverifiable code-changing actions remain subject to their separate gate."
    ))}).to_string()
}

fn unresolved_envelope(reason: &str) -> String {
    let reason = bounded_reason(reason);
    // A block would schedule another model continuation. A terminal Stop must not do that.
    json!({"continue": false, "stopReason": reason, "systemMessage": reason}).to_string()
}

/// Default location is outside the repository. No test or review changes tracked project files.
pub fn state_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("stop-review"))
}

/// Cross-process local-state seam. Hold this object across the awaited HTTP request and call
/// `finish` before printing. An existing lock is not deleted or guessed stale; the output pauses.
pub struct LocalReview {
    state_path: PathBuf,
    lock_path: PathBuf,
    lock: Option<File>,
    previous: Option<ReviewState>,
    scope_key: String,
    turn_key: String,
}

impl LocalReview {
    pub fn open(directory: &Path, target: &ReviewTarget) -> Result<Self, Issue> {
        if ![
            target.scope_key.as_str(),
            target.turn_key.as_str(),
            target.answer_sha256.as_str(),
        ]
        .iter()
        .all(|key| key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(Issue::new(
                "the review scope or artifact fingerprint is invalid",
            ));
        }
        fs::create_dir_all(directory)
            .map_err(|_| Issue::new("the Stop review state directory could not be created"))?;
        if fs::symlink_metadata(directory)
            .map_or(true, |m| !m.is_dir() || m.file_type().is_symlink())
        {
            return Err(Issue::new(
                "the Stop review state directory is not a regular directory",
            ));
        }
        let stem = format!("stop-{}", target.scope_key);
        let lock_path = directory.join(format!("{stem}.lock"));
        let lock = OpenOptions::new().write(true).create_new(true).open(&lock_path)
            .map_err(|_| Issue::new("another Stop review is active or its lock could not be acquired; do not remove an active lock"))?;
        let mut session = Self {
            state_path: directory.join(format!("{stem}.json")),
            lock_path,
            lock: Some(lock),
            previous: None,
            scope_key: target.scope_key.clone(),
            turn_key: target.turn_key.clone(),
        };
        session.previous = session.read_state()?;
        current_state(target, session.previous.as_ref())?;
        Ok(session)
    }

    fn read_state(&self) -> Result<Option<ReviewState>, Issue> {
        match fs::symlink_metadata(&self.state_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(Issue::new(
                    "the saved Stop review state could not be inspected",
                ));
            }
            Ok(metadata)
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > MAX_STATE_BYTES =>
            {
                return Err(Issue::new(
                    "the saved Stop review state is not a bounded regular file",
                ));
            }
            Ok(_) => {}
        }
        let file = File::open(&self.state_path)
            .map_err(|_| Issue::new("the saved Stop review state could not be opened"))?;
        let mut bytes = Vec::new();
        file.take(MAX_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Issue::new("the saved Stop review state could not be read"))?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            return Err(Issue::new(
                "the saved Stop review state exceeded its size limit",
            ));
        }
        serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            Issue::new(
                "the saved Stop review state is corrupt; its repair budget cannot be assumed empty",
            )
        })
    }

    pub fn finish(
        self,
        target: &ReviewTarget,
        outcome: ReviewOutcome,
    ) -> Result<Transition, Issue> {
        if target.scope_key != self.scope_key || target.turn_key != self.turn_key {
            return Err(Issue::new(
                "the review target changed while its scoped state lock was held",
            ));
        }
        let transition = decide(target, self.previous.as_ref(), outcome)?;
        let directory = self
            .state_path
            .parent()
            .ok_or_else(|| Issue::new("the Stop state path has no parent"))?;
        let mut temporary = tempfile::Builder::new().prefix(".stop-review-").tempfile_in(directory)
            .map_err(|_| Issue::new("Stop review state could not be staged; no repair grant or approval was emitted"))?;
        serde_json::to_writer(&mut temporary, &transition.state)
            .map_err(|_| Issue::new("Stop review state could not be encoded"))?;
        temporary
            .flush()
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|_| Issue::new("Stop review state could not be durably written"))?;
        temporary.persist(&self.state_path).map_err(|_| {
            Issue::new(
                "Stop review state could not be committed; no repair grant or approval was emitted",
            )
        })?;
        Ok(transition)
    }
}

impl Drop for LocalReview {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(answer: &str, active: bool) -> ReviewTarget {
        prepare_from_records(
            "acme/repo",
            "/fixture",
            "session",
            "",
            Some("turn-1"),
            active,
            Some(answer),
            None,
        )
        .expect("target")
    }

    fn response(target: &ReviewTarget) -> Value {
        json!({"answer_review": {
            "schema_version": 1, "disposition": "pass", "answer_sha256": target.answer_sha256,
            "repo": target.repo, "coverage_complete": true, "evidence_revision": "snapshot-digest", "indexed_head": "", "material_claims": 1,
            "checked_claims": 1, "reason": "All material claims checked.",
            "findings": [{"claim": "The function exists.", "verdict": "supported",
                "reason": "Definition found.", "evidence": ["src/api.rs:10"],
                "repair": "", "checked_by": "literal_decision"}]
        }})
    }

    fn bad() -> ReviewOutcome {
        ReviewOutcome::NeedsRepair { detail: "Claim: foo() exists.\nFinding: no such symbol.\nRepair: replace foo() with verified bar().".to_string() }
    }

    fn envelope(transition: &Transition) -> Value {
        serde_json::from_str(&transition.envelope).expect("one JSON envelope")
    }

    #[test]
    fn exact_answer_bytes_and_digest_are_preserved() {
        let target = target("  café\n", false);
        assert_eq!(target.answer, "  café\n");
        assert_eq!(
            target.answer_sha256,
            format!("{:x}", Sha256::digest("  café\n".as_bytes()))
        );
        assert_eq!(
            target.request_body(),
            json!({"verify_mode":"answer", "answer":"  café\n", "working_tree_clean":false})
        );
    }

    #[test]
    fn complete_supported_report_allows_only_its_exact_answer() {
        let target = target("fn exists() {}", false);
        let report = parse_report(&target, &response(&target));
        assert_eq!(report, ReviewOutcome::Pass { checked_claims: 1 });
        let result = decide(&target, None, report).expect("transition");
        assert!(result.approved);
        assert!(envelope(&result).get("decision").is_none());
        assert!(result.envelope.contains("not certification"));
    }

    #[test]
    fn model_support_cannot_mint_pass_even_if_coverage_and_probability_flags_claim_otherwise() {
        let target = target("answer", false);
        for probability_flag in [None, Some(false), Some(true)] {
            let mut report = response(&target);
            report["answer_review"]["findings"][0]["checked_by"] = json!("model");
            if let Some(flag) = probability_flag {
                report["answer_review"]["probabilistic_assessment"] = json!(flag);
            }
            let outcome = parse_report(&target, &report);
            assert!(matches!(outcome, ReviewOutcome::Unknown { .. }));
            let result = decide(&target, None, outcome).expect("transition");
            assert!(!result.approved);
            assert!(result.envelope.contains("probabilistic"));
            assert!(result.envelope.contains("did not request semantic review"));
            assert!(envelope(&result).get("decision").is_none());
        }
    }

    #[test]
    fn unsolicited_model_only_contradiction_is_disclosed_without_a_repair_grant() {
        let target = target("answer", false);
        let mut report = response(&target);
        let review = &mut report["answer_review"];
        review["disposition"] = json!("block");
        review["coverage_complete"] = json!(false);
        review["probabilistic_assessment"] = json!(true);
        review["semantic_review"] = json!({
            "status":"contradicted", "reason":"Evidence-bound model judgment."
        });
        review["findings"][0]["checked_by"] = json!("model");
        review["findings"][0]["verdict"] = json!("contradicted");
        review["findings"][0]["repair"] = json!("Qualify the cited claim.");
        let outcome = parse_report(&target, &report);
        assert!(matches!(outcome, ReviewOutcome::Unknown { .. }));
        let result = decide(&target, None, outcome).expect("transition");
        assert!(!result.approved);
        assert_eq!(result.state.repair_grants, 0);
        assert!(
            result
                .envelope
                .contains("not a deterministic contradiction")
        );
        assert!(envelope(&result).get("decision").is_none());
        assert!(envelope(&result).get("continue").is_none());
    }

    #[test]
    fn deterministic_contradiction_still_repairs_when_separate_model_evidence_is_present() {
        let target = target("answer", false);
        let mut report = response(&target);
        let review = &mut report["answer_review"];
        review["disposition"] = json!("block");
        review["coverage_complete"] = json!(false);
        review["material_claims"] = json!(2);
        review["checked_claims"] = json!(2);
        review["probabilistic_assessment"] = json!(true);
        review["findings"][0]["verdict"] = json!("contradicted");
        review["findings"][0]["repair"] = json!("Use the exact recorded decision.");
        let mut model = review["findings"][0].clone();
        model["checked_by"] = json!("model");
        review["findings"]
            .as_array_mut()
            .expect("findings")
            .push(model);
        let outcome = parse_report(&target, &report);
        assert!(matches!(outcome, ReviewOutcome::NeedsRepair { .. }));
        let result = decide(&target, None, outcome).expect("transition");
        assert_eq!(envelope(&result)["decision"], "block");
        assert_eq!(result.state.repair_grants, 1);
        assert!(
            result
                .envelope
                .contains("Separate probabilistic assessment")
        );
    }

    #[test]
    fn semantic_metadata_is_separate_and_unavailable_assessment_does_not_invalidate_checks() {
        let target = target("answer", false);
        let mut report = response(&target);
        report["answer_review"]["semantic_review"] = json!({
            "status":"unavailable", "reason":"Semantic assessment was not requested."
        });
        report["answer_review"]["probabilistic_assessment"] = json!(false);
        assert_eq!(
            parse_report(&target, &report),
            ReviewOutcome::Pass { checked_claims: 1 }
        );
        for status in ["supported", "contradicted", "insufficient"] {
            report["answer_review"]["semantic_review"]["status"] = json!(status);
            // Even a false flag cannot hide an unsolicited assessment.
            assert!(matches!(
                parse_report(&target, &report),
                ReviewOutcome::Unknown { .. }
            ));
        }
        report["answer_review"]["semantic_review"]["status"] = json!("unavailable");
        report["answer_review"]["probabilistic_assessment"] = json!(true);
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        assert!(target.request_body().get("semantic_review").is_none());
        assert!(target.request_body().get("semantic_funding").is_none());
    }

    #[test]
    fn changed_output_rejects_prior_review() {
        let original = target("first", false);
        let changed = target("second", true);
        assert!(matches!(
            parse_report(&changed, &response(&original)),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn scope_and_version_mismatch_are_unknown() {
        let target = target("answer", false);
        for (field, value) in [
            ("repo", json!("other/repo")),
            ("schema_version", json!(2)),
            ("answer_sha256", json!("wrong")),
        ] {
            let mut report = response(&target);
            report["answer_review"][field] = value;
            assert!(
                matches!(
                    parse_report(&target, &report),
                    ReviewOutcome::Unknown { .. }
                ),
                "{field}"
            );
        }
    }

    #[test]
    fn legacy_success_and_zero_coverage_are_never_pass() {
        let target = target("answer", false);
        for report in [
            json!({"grounded": true}),
            json!({"ok": true}),
            json!({}),
            Value::Null,
        ] {
            assert!(matches!(
                parse_report(&target, &report),
                ReviewOutcome::Unknown { .. }
            ));
        }
        let mut report = response(&target);
        report["answer_review"]["findings"] = json!([]);
        report["answer_review"]["checked_claims"] = json!(0);
        report["answer_review"]["material_claims"] = json!(0);
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn not_applicable_is_explicit_positive_classification_not_empty_extraction() {
        let target = target("Thanks!", false);
        let mut report = response(&target);
        let review = &mut report["answer_review"];
        review["disposition"] = json!("not_applicable");
        review["material_claims"] = json!(0);
        review["checked_claims"] = json!(0);
        review["findings"] = json!([]);
        review["reason"] = json!("Nonfactual courtesy response.");
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::NotApplicable { .. }
        ));
        report["answer_review"]["reason"] = json!("");
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn incomplete_or_missing_evidence_cannot_pass() {
        let target = target("answer", false);
        let mut report = response(&target);
        report["answer_review"]["coverage_complete"] = json!(false);
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        let mut report = response(&target);
        report["answer_review"]["findings"][0]["evidence"] = json!([]);
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        let mut report = response(&target);
        report["answer_review"]["findings"][0]["checked_by"] = json!("none");
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn negative_noninteger_and_mismatched_counters_are_unknown() {
        let target = target("answer", false);
        for count in [json!(-1), json!(0.5), json!("1"), json!(2), json!(0)] {
            let mut report = response(&target);
            report["answer_review"]["checked_claims"] = count;
            assert!(matches!(
                parse_report(&target, &report),
                ReviewOutcome::Unknown { .. }
            ));
        }
    }

    #[test]
    fn every_required_response_field_is_required() {
        let target = target("answer", false);
        for key in [
            "schema_version",
            "disposition",
            "answer_sha256",
            "repo",
            "coverage_complete",
            "material_claims",
            "checked_claims",
            "findings",
            "reason",
        ] {
            let mut report = response(&target);
            report["answer_review"]
                .as_object_mut()
                .expect("object")
                .remove(key);
            assert!(
                matches!(
                    parse_report(&target, &report),
                    ReviewOutcome::Unknown { .. }
                ),
                "{key}"
            );
        }
    }

    #[test]
    fn confirmed_contradiction_returns_exact_repair_in_block() {
        let target = target("answer", false);
        let mut report = response(&target);
        report["answer_review"]["disposition"] = json!("block");
        report["answer_review"]["findings"][0]["verdict"] = json!("contradicted");
        report["answer_review"]["findings"][0]["repair"] =
            json!("Replace foo() with verified bar().");
        let result = decide(&target, None, parse_report(&target, &report)).expect("transition");
        assert_eq!(envelope(&result)["decision"], "block");
        assert!(
            result
                .envelope
                .contains("Replace foo() with verified bar().")
        );
        assert!(!result.approved);
        assert_eq!(result.state.repair_grants, 1);
    }

    #[test]
    fn unknown_final_prose_and_timeout_are_disclosed_without_false_pass_or_block() {
        let target = target("Uncertain answer.", false);
        for detail in [
            "request timed out",
            "partial coverage",
            "authentication refused",
            "malformed response",
        ] {
            let result =
                decide(&target, None, ReviewOutcome::unavailable(detail)).expect("transition");
            assert!(!result.approved);
            assert!(result.unresolved);
            assert!(envelope(&result).get("decision").is_none());
            assert!(result.envelope.contains("UNVERIFIED"));
            assert!(result.envelope.contains(detail));
            assert_eq!(result.state.repair_grants, 0);
        }
    }

    #[test]
    fn two_changed_repairs_are_rechecked_then_exhaustion_is_unresolved_not_approval() {
        let first = decide(&target("original", false), None, bad()).expect("repair 1");
        assert_eq!(first.state.repair_grants, 1);
        let second =
            decide(&target("repair one", true), Some(&first.state), bad()).expect("repair 2");
        assert_eq!(second.state.repair_grants, 2);
        assert_eq!(envelope(&second)["decision"], "block");
        let third =
            decide(&target("repair two", true), Some(&second.state), bad()).expect("exhausted");
        assert_eq!(third.state.repair_grants, 2);
        assert!(!third.approved);
        assert!(third.unresolved);
        assert_eq!(envelope(&third)["continue"], false);
        assert!(envelope(&third).get("decision").is_none());
        assert!(third.envelope.contains("UNRESOLVED"));
        let fourth = decide(
            &target("unrequested repair", true),
            Some(&third.state),
            bad(),
        )
        .expect("still exhausted");
        assert!(!fourth.approved);
        assert_eq!(fourth.state.repair_grants, 2);
    }

    #[test]
    fn unchanged_retry_is_rechecked_but_not_granted_another_repair() {
        let initial = target("original", false);
        let first = decide(&initial, None, bad()).expect("repair");
        let retry = target("original", true);
        let repeated = decide(&retry, Some(&first.state), bad()).expect("unchanged");
        assert_eq!(repeated.state.repair_grants, 1);
        assert!(repeated.unresolved);
        assert_eq!(envelope(&repeated)["continue"], false);
        // Same bytes with genuinely NEW supported evidence can pass. No cached failure or pass.
        let verified = decide(
            &retry,
            Some(&first.state),
            parse_report(&retry, &response(&retry)),
        )
        .expect("fresh review");
        assert!(verified.approved);
    }

    #[test]
    fn second_repair_can_pass_when_its_new_exact_bytes_are_supported() {
        let first = decide(&target("original", false), None, bad()).expect("repair 1");
        let second =
            decide(&target("repair one", true), Some(&first.state), bad()).expect("repair 2");
        let repaired = target("repair two", true);
        let final_result = decide(
            &repaired,
            Some(&second.state),
            parse_report(&repaired, &response(&repaired)),
        )
        .expect("review");
        assert!(final_result.approved);
        assert!(!final_result.unresolved);
        assert!(envelope(&final_result).get("decision").is_none());
    }

    #[test]
    fn new_host_turn_resets_repairs_but_changed_artifact_does_not() {
        let first = decide(&target("original", false), None, bad()).expect("repair");
        let mut next = target("new turn answer", false);
        next.turn_key = "new-turn".to_string();
        let next = decide(&next, Some(&first.state), bad()).expect("new turn");
        assert_eq!(next.state.repair_grants, 1);
        let changed = decide(
            &target("changed artifact", false),
            Some(&first.state),
            bad(),
        )
        .expect("same turn");
        assert_eq!(changed.state.repair_grants, 2);
    }

    #[test]
    fn state_scope_corruption_never_resets_the_budget_silently() {
        let current = target("answer", false);
        let first = decide(&current, None, bad()).expect("repair");
        let mut broken = first.state;
        broken.scope_key = "another scope".to_string();
        assert!(decide(&current, Some(&broken), bad()).is_err());
    }

    #[test]
    fn missing_output_and_textless_latest_record_do_not_review_an_older_answer() {
        let records = vec![
            json!({"type":"user","message":{"content":"Do the work"}}),
            json!({"type":"assistant","message":{"content":"Old answer"}}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"secret"}}]}}),
        ];
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                None,
                false,
                None,
                Some(&records)
            )
            .is_err()
        );
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                None,
                false,
                None,
                Some(&[])
            )
            .is_err()
        );
    }

    #[test]
    fn transcript_extracts_only_latest_assistant_text_and_actual_user_text() {
        let records = vec![
            json!({"type":"user","message":{"content":"Actual user request"}}),
            json!({"type":"assistant","message":{"content":"Old answer"}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","content":"secret tool output"}]}}),
            json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":"private"},{"type":"text","text":"Final answer"}]}}),
        ];
        let target = prepare_from_records(
            "acme/repo",
            "/fixture",
            "s",
            "",
            None,
            false,
            None,
            Some(&records),
        )
        .expect("target");
        assert_eq!(target.answer, "Final answer");
        assert_eq!(
            target.latest_user_message.as_deref(),
            Some("Actual user request")
        );
        assert!(!target.request_body().to_string().contains("secret"));
        assert!(!target.request_body().to_string().contains("private"));
    }

    #[test]
    fn answer_before_latest_user_is_not_current_output() {
        let records = vec![
            json!({"type":"assistant","message":{"content":"Old answer"}}),
            json!({"type":"user","message":{"content":"New request"}}),
        ];
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                None,
                false,
                None,
                Some(&records)
            )
            .is_err()
        );
    }

    #[test]
    fn oversize_output_is_not_truncated_into_a_pass() {
        let answer = "x".repeat(MAX_ANSWER_BYTES + 1);
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                Some("turn"),
                false,
                Some(&answer),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn payload_answer_with_turn_id_needs_no_transcript_read() {
        let root = tempfile::tempdir().expect("root");
        let prepared = prepare(ReviewInput {
            repo: "acme/repo",
            root: root.path(),
            session_id: "s",
            turn_id: Some("turn"),
            agent_id: None,
            stop_hook_active: false,
            last_assistant_message: Some("Exact answer"),
            transcript_path: Some(Path::new("/does/not/exist")),
            head: None,
            working_tree_clean: false,
        })
        .expect("direct input");
        assert_eq!(prepared.answer, "Exact answer");
    }

    #[test]
    fn malformed_transcript_is_not_empty_clean_output() {
        let temp = tempfile::tempdir().expect("temporary root");
        let path = temp.path().join("transcript.jsonl");
        fs::write(&path, "not json\n").expect("fixture");
        assert!(read_transcript(&path).is_err());
    }

    #[test]
    fn local_state_persists_only_hashes_and_bounds_repair_grants_across_processes() {
        let directory = tempfile::tempdir().expect("state directory");
        let original = target("private answer bytes", false);
        let first = LocalReview::open(directory.path(), &original)
            .expect("open")
            .finish(&original, bad())
            .expect("save");
        let file = directory
            .path()
            .join(format!("stop-{}.json", original.scope_key));
        let text = fs::read_to_string(&file).expect("state");
        assert!(!text.contains("private answer bytes"));
        assert!(!text.contains("foo()"));
        let retry = target("changed private answer", true);
        let second = LocalReview::open(directory.path(), &retry)
            .expect("reopen")
            .finish(&retry, bad())
            .expect("resave");
        assert_eq!(first.state.repair_grants, 1);
        assert_eq!(second.state.repair_grants, 2);
    }

    #[test]
    fn concurrent_review_cannot_race_the_same_budget() {
        let directory = tempfile::tempdir().expect("state directory");
        let target = target("answer", false);
        let first = LocalReview::open(directory.path(), &target).expect("lock");
        assert!(LocalReview::open(directory.path(), &target).is_err());
        drop(first);
        assert!(LocalReview::open(directory.path(), &target).is_ok());
    }

    #[test]
    fn corrupt_state_is_disclosed_and_never_treated_as_empty() {
        let directory = tempfile::tempdir().expect("state directory");
        let target = target("answer", false);
        let state = directory
            .path()
            .join(format!("stop-{}.json", target.scope_key));
        fs::write(state, b"broken").expect("fixture");
        assert!(LocalReview::open(directory.path(), &target).is_err());
        assert!(
            !directory
                .path()
                .join(format!("stop-{}.lock", target.scope_key))
                .exists()
        );
    }

    #[test]
    fn local_transaction_rejects_a_different_target_scope() {
        let directory = tempfile::tempdir().expect("state directory");
        let original = target("answer", false);
        let transaction = LocalReview::open(directory.path(), &original).expect("lock");
        let mut other = original;
        other.scope_key = "different".to_string();
        assert!(transaction.finish(&other, bad()).is_err());
    }

    #[test]
    fn all_envelopes_fit_the_host_stop_schema_fields() {
        let cases = [
            Issue::new("missing input").envelope(),
            unknown_envelope("timeout"),
            block_envelope("repair"),
            unresolved_envelope("UNRESOLVED"),
        ];
        for encoded in cases {
            let envelope: Value = serde_json::from_str(&encoded).expect("JSON");
            for key in envelope.as_object().expect("object").keys() {
                assert!(
                    [
                        "decision",
                        "reason",
                        "systemMessage",
                        "continue",
                        "stopReason"
                    ]
                    .contains(&key.as_str())
                );
            }
            assert!(envelope.get("async").is_none());
        }
    }

    #[test]
    fn missing_active_retry_state_still_uses_fresh_review_without_new_repair_grants() {
        let active = target("answer", true);
        let verified =
            decide(&active, None, parse_report(&active, &response(&active))).expect("fresh review");
        assert!(verified.approved);
        let contradicted = decide(&active, None, bad()).expect("fresh contradiction");
        assert!(contradicted.unresolved);
        assert!(!contradicted.approved);
        assert_eq!(envelope(&contradicted)["continue"], false);
        assert_eq!(contradicted.state.repair_grants, MAX_REPAIRS);
    }

    #[test]
    fn active_retry_with_different_turn_never_resets_repair_budget() {
        let first = decide(&target("answer", false), None, bad()).expect("repair");
        let mut other = target("different turn output", true);
        other.turn_key = "different".to_string();
        let result = decide(&other, Some(&first.state), bad()).expect("review");
        assert!(result.unresolved);
        assert_eq!(result.state.repair_grants, MAX_REPAIRS);
    }

    #[test]
    fn index_evidence_must_match_the_measured_checkout_head() {
        let mut target = target("answer", false);
        let mut report = response(&target);
        report["answer_review"]["findings"][0]["checked_by"] = json!("index");
        report["answer_review"]["indexed_head"] = json!("a".repeat(40));
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        target.head = Some("b".repeat(40));
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        target.head = Some("a".repeat(40));
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
        target.working_tree_clean = true;
        assert_eq!(
            parse_report(&target, &report),
            ReviewOutcome::Pass { checked_claims: 1 }
        );
        assert_eq!(target.request_body()["head"], "a".repeat(40));
    }

    #[test]
    fn missing_evidence_revision_cannot_certify_checked_claims() {
        let target = target("answer", false);
        let mut report = response(&target);
        report["answer_review"]["evidence_revision"] = json!("");
        assert!(matches!(
            parse_report(&target, &report),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn huge_findings_cannot_truncate_the_required_repair_instruction() {
        let current = target("answer", false);
        let result = decide(
            &current,
            None,
            ReviewOutcome::NeedsRepair {
                detail: "x".repeat(30_000),
            },
        )
        .expect("review");
        assert!(result.envelope.contains("Repair attempt 1/2"));
        assert!(result.envelope.contains("new synchronous review"));
    }

    #[test]
    fn forged_zero_checked_pass_never_approves() {
        let current = target("answer", false);
        let result =
            decide(&current, None, ReviewOutcome::Pass { checked_claims: 0 }).expect("review");
        assert!(!result.approved);
        assert!(result.unresolved);
        assert!(result.envelope.contains("UNVERIFIED"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_state_is_never_read_or_overwritten() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().expect("state directory");
        let current = target("answer", false);
        let outside = directory.path().join("unrelated.json");
        fs::write(&outside, "keep me").expect("outside fixture");
        let path = directory
            .path()
            .join(format!("stop-{}.json", current.scope_key));
        symlink(&outside, path).expect("symlink fixture");
        assert!(LocalReview::open(directory.path(), &current).is_err());
        assert_eq!(
            fs::read_to_string(outside).expect("outside file"),
            "keep me"
        );
    }

    #[test]
    fn transcript_only_host_pins_active_retry_budget_across_hook_feedback() {
        let first = decide(&target("original", false), None, bad()).expect("repair 1");
        let mut next = target("first repair", true);
        next.explicit_turn = false;
        next.turn_key = "synthetic-feedback-user-record".to_string();
        let second = decide(&next, Some(&first.state), bad()).expect("repair 2");
        assert_eq!(second.state.repair_grants, 2);
        assert!(!second.unresolved);
        next.answer_sha256 = answer_digest("second repair");
        next.turn_key = "another-feedback-record".to_string();
        let exhausted = decide(&next, Some(&second.state), bad()).expect("exhausted");
        assert!(exhausted.unresolved);
        assert_eq!(exhausted.state.repair_grants, 2);
    }

    #[test]
    fn stop_feedback_is_not_uploaded_as_latest_human_decision() {
        let records = vec![
            json!({"type":"user","message":{"content":"Actual human request"}}),
            json!({"type":"assistant","message":{"content":"Original answer"}}),
            json!({"type":"user","message":{"content":"Estelle Stop review blocked this output."}}),
            json!({"type":"assistant","message":{"content":"Repaired answer"}}),
        ];
        let result = prepare_from_records(
            "acme/repo",
            "/fixture",
            "s",
            "",
            None,
            true,
            None,
            Some(&records),
        )
        .expect("retry");
        assert!(result.latest_user_message.is_none());
        assert!(result.request_body().get("latest_user_message").is_none());
    }

    /// These fixtures were emitted by the real api_intel.handle_verify + Memory path with
    /// synthetic in-memory stores. They are not a native handwritten approximation of the API.
    #[test]
    fn real_backend_wire_fixtures_obey_the_native_answer_review_contract() {
        // Synthetic, actual-handler outputs. These assert the wire contract, not live service
        // availability, production evidence quality, or whole-answer semantic correctness.
        let fixtures: Value =
            serde_json::from_str(include_str!("../tests/fixtures/answer-review-wire.json"))
                .expect("wire fixtures");
        for (name, answer) in [
            ("block", "We do not use PostgreSQL for storage."),
            ("pass", "We use PostgreSQL for storage."),
            ("not_applicable", "Thanks!"),
            ("unknown", "I deployed the service successfully."),
            (
                "index_block",
                "There is no `storage_pool` in the repository.",
            ),
            ("index_pass", "`storage_pool` is defined in the repository."),
            (
                "absence_unknown",
                "There is no `missing_symbol` in the repository.",
            ),
        ] {
            let mut target = prepare_from_records(
                "owner/repo",
                "/fixture",
                "session",
                "",
                Some("turn"),
                false,
                Some(answer),
                None,
            )
            .expect("target");
            target.head = Some("a".repeat(40));
            target.working_tree_clean = true;
            let result = parse_report(&target, &fixtures[name]);
            assert!(
                matches!(
                    (name, result),
                    ("block", ReviewOutcome::NeedsRepair { .. })
                        | ("pass", ReviewOutcome::Pass { checked_claims: 1 })
                        | ("not_applicable", ReviewOutcome::NotApplicable { .. })
                        | ("unknown", ReviewOutcome::Unknown { .. })
                        | ("index_block", ReviewOutcome::NeedsRepair { .. })
                        | ("index_pass", ReviewOutcome::Pass { checked_claims: 1 })
                        | ("absence_unknown", ReviewOutcome::Unknown { .. })
                ),
                "backend disposition {name}"
            );
        }
    }

    #[test]
    fn top_level_error_cannot_be_hidden_by_an_embedded_pass() {
        let current = target("answer", false);
        let mut report = response(&current);
        report["error"] = json!("verification failed");
        assert!(matches!(
            parse_report(&current, &report),
            ReviewOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn invalid_utf8_transcript_is_not_certified_as_replacement_text() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let path = directory.path().join("bad-encoding.jsonl");
        let mut bytes = br#"{"type":"assistant","message":{"content":"bad"#.to_vec();
        bytes.push(0xff);
        bytes.extend_from_slice(br#"bytes"}}"#);
        fs::write(&path, bytes).expect("fixture");
        assert!(read_transcript(&path).is_err());
    }

    #[test]
    fn codex_nonfinal_channel_is_preserved_and_never_reviewed_as_final_output() {
        let text = concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"Request\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"channel\":\"analysis\",\"content\":[{\"type\":\"output_text\",\"text\":\"Private analysis text\"}]}}\n"
        );
        let parsed = host_transcript::host_records(text);
        assert_eq!(parsed.records[1]["message"]["channel"], "analysis");
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                None,
                false,
                None,
                Some(&parsed.records)
            )
            .is_err()
        );
    }

    #[test]
    fn text_before_a_tool_use_is_not_a_final_answer() {
        let records = vec![
            json!({"type":"user","message":{"content":"Request"}}),
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"I will change it."},{"type":"tool_use","name":"Edit","input":{"content":"not final"}}]}}),
        ];
        assert!(
            prepare_from_records(
                "acme/repo",
                "/fixture",
                "s",
                "",
                None,
                false,
                None,
                Some(&records)
            )
            .is_err()
        );
    }
}
