//! ONE OWNER FOR "WHAT DOES THIS HOST'S TRANSCRIPT SAY".
//!
//! # The defect this module exists to fix
//!
//! Estelle has **never captured a single Codex session**, and nothing said so. The checkpoint's
//! extractors were written against Claude Code's transcript — records shaped
//! `{"type":"user"|"assistant","message":{"role","content"}}` — and Codex writes a completely
//! different file to `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`:
//!
//! ```text
//! {"timestamp":"…","ordinal":1,"type":"session_meta","payload":{…}}
//! {"timestamp":"…","ordinal":2,"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"…"}]}}
//! ```
//!
//! No `"type":"user"`, no `message` object, no `content` where the old code looked. So
//! `transcript_messages` returned EMPTY, `checkpoint_local` returned `None`, and every Codex
//! checkpoint was a no-op **that exited zero and printed nothing**. Measured over the founder's
//! 161 real rollouts (2026-09-05): `response_item` records 64_241, `event_msg` 57_172,
//! `turn_context` 711, `world_state` 390, `compacted` 209, `session_meta` 195,
//! `inter_agent_communication_metadata` 114, `token_usage_record` 12 — and **zero** records of
//! type `user` or `assistant`.
//!
//! # Why this is a NORMALISER and not a second parser
//!
//! Two extractors that each branch on format is how a fork drifts: one grows a rule the other
//! never learns. So there is exactly ONE extraction pipeline — the host-record extractors in
//! `top_level.rs` — and this module is the only place that knows either on-disk vocabulary. A
//! Codex record is TRANSLATED into the host-record shape those extractors already read; nothing
//! downstream knows which host wrote the file, except [`Dialect`], which is reported once.
//!
//! # Three formats, measured, not assumed
//!
//! | dialect | detected by | files in the founder's corpus |
//! |---|---|---|
//! | Claude Code | top-level `sessionId`, or `message` object beside `type` | 3_573 under `~/.claude/projects` |
//! | Codex rollout (2026) | `payload` object beside a string `type` | 160 of 161 |
//! | Codex rollout (2025, legacy) | bare `ResponseItem` lines, `{"record_type":"state"}` | 1 of 161 |
//!
//! # The size problem
//!
//! A rollout is not a chat log; it is every byte the agent read and wrote. Measured over the
//! same 161 files: **median 19.3 MB, p75 100.4 MB, p95 530.8 MB, max 35.3 GB**. The old code did
//! `fs::read_to_string` on the whole thing and then parsed every line three times, which on the
//! p95 file is a multi-second stall and on the largest is an out-of-memory abort. [`read_bounded`]
//! reads a bounded HEAD and a bounded TAIL and says so — see its docs for why the tail, and for
//! what a truncated read is allowed to claim.

use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;

use serde_json::Map;
use serde_json::Value;
use serde_json::json;

/// How much of the END of a transcript is read.
///
/// 🔴 **THIS BOUND IS A MEASUREMENT, NOT A GUESS.** Counting the user and assistant turns inside
/// the last N bytes of all 161 of the founder's rollouts:
///
/// | window | files yielding ≥1 turn | files yielding ≥1 USER turn |
/// |---|---|---|
/// | 1 MiB | 140 / 161 | 40 |
/// | 8 MiB | 141 / 161 | 93 |
/// | **32 MiB** | **141 / 161** | **120** |
///
/// Turn coverage saturates at 4 MiB (141 of 161 — the other 20 are aborted sessions that contain
/// no conversation anywhere), but USER turns keep climbing, because Codex writes megabytes of
/// command output between two consecutive human sentences. 32 MiB buys 27 more sessions that
/// keep the human's half at a read cost of ~0.1 s. That is the whole reason for the number.
pub const TAIL_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// How much of the START of a transcript is read when the tail alone is not the whole file.
///
/// The first line is where both hosts put the session's identity — Codex's `session_meta`
/// (cwd, `cli_version`, `originator`, `git.branch`) and Claude Code's first record (`cwd`,
/// `gitBranch`, `version`). In a truncated read that line is a million bytes behind the window,
/// so a checkpoint of a large session would silently lose its branch and its client version.
/// Measured over the 161 rollouts, the first line is **at most 22_550 bytes** and is
/// `session_meta` in **161 of 161** files, so 64 KiB clears it with three times the margin.
pub const HEAD_MAX_BYTES: u64 = 64 * 1024;

/// How many changed paths one file-change event may contribute.
///
/// A single `apply_patch` can touch an unbounded number of files and this runs inside a hook, so
/// the loop that walks them has a stated bound (Power of Ten #2). It is deliberately larger than
/// `session_gap::MAX_TRACKED_FILES`, which does the real trimming after de-duplication.
pub const MAX_PATHS_PER_CHANGE_EVENT: usize = 64;

/// 🔴 THE BOUNDS' OWN INVARIANTS, CHECKED BY THE COMPILER.
///
/// These were written first as `#[test] assert!(…)` over two constants, and clippy correctly
/// called that what it is: an assertion with a constant value, which cannot fire at run time and
/// is therefore decoration (Power of Ten #5). As `const` items they fail the BUILD instead, which
/// is a guard that can actually go red. Each number below is measured, not chosen:
/// `MEASURED_P75_ROLLOUT_BYTES` and `MEASURED_LARGEST_FIRST_LINE_BYTES` come from the founder's
/// 161 rollouts on 2026-09-05.
const MEASURED_P75_ROLLOUT_BYTES: u64 = 100_404_907;
const MEASURED_LARGEST_FIRST_LINE_BYTES: u64 = 22_550;
const _: () = assert!(
    TAIL_MAX_BYTES < MEASURED_P75_ROLLOUT_BYTES,
    "a bound larger than the files it bounds bounds nothing"
);
const _: () = assert!(
    HEAD_MAX_BYTES > MEASURED_LARGEST_FIRST_LINE_BYTES,
    "the head window exists to read the session_meta line; it must clear the largest one measured"
);
const _: () = assert!(HEAD_MAX_BYTES < TAIL_MAX_BYTES);

/// Which host wrote the transcript. Reported so the checkpoint can name its own source instead
/// of asserting `"claude-code"` over a Codex session, which is the "a name that overclaims its
/// body" defect in one field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    ClaudeCode,
    CodexRollout,
}

impl Dialect {
    /// The `client.name` this dialect travels under.
    #[must_use]
    pub fn client_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::CodexRollout => "codex",
        }
    }

    /// How this dialect is named to a human reading stderr.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code transcript",
            Self::CodexRollout => "Codex rollout",
        }
    }
}

/// A bounded read of a transcript, and the honest account of what it left out.
#[derive(Debug)]
pub struct Bounded {
    /// The bytes read, as text. When [`Self::truncated`] is set this is the file's head followed
    /// by its tail, with the middle absent.
    pub text: String,
    /// The size of the file on disk at the moment it was opened.
    pub file_bytes: u64,
    /// How much of it was actually read.
    pub read_bytes: u64,
    /// ⚠️ `true` means **THERE IS MORE**, never "that is all there is". Every consumer of a
    /// truncated read owes the reader that distinction — see `Truncation` in `top_level.rs`,
    /// which is what puts it in the stored checkpoint.
    pub truncated: bool,
}

/// Read a transcript without ever holding more than [`HEAD_MAX_BYTES`] + [`TAIL_MAX_BYTES`].
///
/// **The TAIL, deliberately.** A checkpoint answers "where did this session stop", so the most
/// recent turns are the load-bearing ones; the head is read only for the session identity that
/// lives on the first line. The middle of a 35 GB rollout is dropped, and [`Bounded::truncated`]
/// is how the caller learns it was.
///
/// A partial record at either seam is dropped WHOLE rather than handed to the parser as a
/// fragment: the head is cut back to its last newline and the tail forward past its first one.
/// Invalid UTF-8 is replaced rather than fatal — a transcript is data, and data does not get to
/// panic this process.
pub fn read_bounded(path: &Path) -> io::Result<Bounded> {
    let mut file = File::open(path)?;
    let file_bytes = file.metadata()?.len();
    if file_bytes <= TAIL_MAX_BYTES {
        let mut buf = Vec::new();
        let read = (&mut file).take(TAIL_MAX_BYTES).read_to_end(&mut buf)?;
        return Ok(Bounded {
            text: String::from_utf8_lossy(&buf).into_owned(),
            file_bytes,
            read_bytes: as_u64(read),
            truncated: false,
        });
    }

    let mut head = Vec::new();
    let head_read = (&mut file).take(HEAD_MAX_BYTES).read_to_end(&mut head)?;
    let mut text = String::from_utf8_lossy(&head).into_owned();
    // Keep only whole records from the head: the window closed mid-line.
    match text.rfind('\n') {
        Some(nl) => text.truncate(nl + 1),
        None => text.clear(),
    }

    let mut tail = Vec::new();
    file.seek(SeekFrom::Start(file_bytes - TAIL_MAX_BYTES))?;
    let tail_read = (&mut file).take(TAIL_MAX_BYTES).read_to_end(&mut tail)?;
    let tail = String::from_utf8_lossy(&tail).into_owned();
    // …and only whole records from the tail: the window opened mid-line. A window with no
    // newline in it at all is one enormous partial record and contributes nothing.
    if let Some(nl) = tail.find('\n') {
        text.push_str(&tail[nl + 1..]);
    }

    Ok(Bounded {
        text,
        file_bytes,
        read_bytes: as_u64(head_read) + as_u64(tail_read),
        truncated: true,
    })
}

/// Saturating `usize` → `u64` for a byte count that is only ever reported, never indexed with.
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Every record in a transcript, normalised to ONE shape, plus what was learned reading it.
#[derive(Debug, Default)]
pub struct HostRecords {
    /// Host-shaped records, ready for the extractors. Codex records have been translated.
    pub records: Vec<Value>,
    /// `None` when nothing in the text was recognisable as either host's transcript — which is a
    /// REASON TO SPEAK, not a reason to return empty. That silence is the defect this module
    /// exists to fix.
    pub dialect: Option<Dialect>,
    /// Non-blank lines seen.
    pub lines: usize,
    /// Lines that were not JSON at all.
    pub unreadable_lines: usize,
    /// Lines that were JSON and matched neither host's shape.
    pub unrecognised_lines: usize,
}

/// Parse a transcript into host-shaped records, detecting which host wrote it.
///
/// This is the single place either on-disk vocabulary is known. A malformed line is skipped and
/// COUNTED — never fatal, and never invisible.
#[must_use]
pub fn host_records(text: &str) -> HostRecords {
    let mut fold = Fold::default();
    let mut out = HostRecords::default();
    let (mut claude, mut codex) = (0usize, 0usize);
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.lines += 1;
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            out.unreadable_lines += 1;
            continue;
        };
        match classify(&record) {
            Shape::ClaudeCode => {
                claude += 1;
                fold.push_host(record);
            }
            Shape::Codex => {
                codex += 1;
                fold.push_codex(&record);
            }
            Shape::Unknown => out.unrecognised_lines += 1,
        }
    }
    out.records = fold.records;
    out.dialect = match (claude, codex) {
        (0, 0) => None,
        (host, rollout) if rollout > host => Some(Dialect::CodexRollout),
        _ => Some(Dialect::ClaudeCode),
    };
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    ClaudeCode,
    Codex,
    Unknown,
}

/// Which host wrote ONE record. Order is load-bearing: the Claude Code tests run first because
/// its records carry the discriminant that cannot collide (`sessionId`), and only then do the
/// broader Codex shapes get a look.
fn classify(record: &Value) -> Shape {
    let kind = record.get("type").and_then(Value::as_str);
    if record.get("sessionId").is_some()
        || (record.get("message").is_some_and(Value::is_object) && kind.is_some())
    {
        return Shape::ClaudeCode;
    }
    if record.get("payload").is_some_and(Value::is_object) && kind.is_some() {
        return Shape::Codex; // {timestamp, ordinal, type, payload} — every 2026 rollout
    }
    if record.get("record_type").is_some() {
        return Shape::Codex; // 2025 rollouts interleave {"record_type":"state"}
    }
    if matches!(
        kind,
        Some("message" | "reasoning" | "function_call" | "function_call_output")
    ) {
        return Shape::Codex; // 2025 rollouts wrote bare ResponseItems
    }
    if kind.is_none() && record.get("git").is_some() && record.get("id").is_some() {
        return Shape::Codex; // the 2025 head line, which predates `session_meta`
    }
    Shape::Unknown
}

/// Builds the normalised record list. Stateful for exactly one reason: a Codex tool call is its
/// OWN record, while Claude Code nests `tool_use` inside the assistant message that made it. So a
/// tool call is folded into the assistant turn it belongs to, which keeps the 400-message
/// checkpoint budget spent on conversation instead of on 5.6 tool lines per turn (measured:
/// 18_121 tool calls against 3_247 assistant messages).
#[derive(Debug, Default)]
struct Fold {
    records: Vec<Value>,
    /// Index of the assistant record a tool call currently belongs to. Cleared by a user turn,
    /// because a tool call after the human speaks belongs to the NEXT assistant turn.
    open_assistant: Option<usize>,
}

impl Fold {
    /// A record already in host shape passes through untouched.
    fn push_host(&mut self, record: Value) {
        self.records.push(record);
        self.open_assistant = None;
    }

    fn push_codex(&mut self, record: &Value) {
        let kind = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let payload = record.get("payload").unwrap_or(record);
        match kind {
            "response_item"
            | "message"
            | "reasoning"
            | "function_call"
            | "function_call_output" => self.response_item(payload),
            "event_msg" => self.event_msg(payload),
            "session_meta" => self.context(session_meta_context(payload)),
            "turn_context" => self.context(turn_context_context(payload)),
            _ if record.get("git").is_some() => self.context(session_meta_context(record)),
            _ => {}
        }
    }

    /// The model-visible conversation. This is the ONLY family messages are read from: measured
    /// across all 161 rollouts, `event_msg` carries a turn that `response_item` does not in
    /// **0** files, so reading both would trip-count every sentence.
    fn response_item(&mut self, payload: &Value) {
        match payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "message" | "agent_message" => self.message(payload),
            "custom_tool_call" | "function_call" | "local_shell_call" | "mcp_tool_call"
            | "web_search_call" => {
                let name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("tool")
                    .to_string();
                self.tool(&name, None);
            }
            _ => {}
        }
    }

    fn message(&mut self, payload: &Value) {
        let default_role = match payload.get("type").and_then(Value::as_str) {
            Some("agent_message") => "assistant", // no `role` field; it is one by construction
            _ => "",
        };
        let role = payload
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or(default_role);
        // `developer` is the harness talking to the model, not a turn of the conversation.
        if role != "user" && role != "assistant" {
            return;
        }
        let blocks = content_blocks(payload.get("content"));
        if blocks.is_empty() {
            return;
        }
        self.records.push(json!({
            "type": role,
            "message": {"role": role, "content": blocks},
        }));
        self.open_assistant = (role == "assistant").then(|| self.records.len() - 1);
    }

    /// One tool call, as the `tool_use` block the host-record extractors already understand.
    fn tool(&mut self, name: &str, file: Option<&str>) {
        let mut input = Map::new();
        if let Some(file) = file {
            input.insert("file_path".to_string(), json!(file));
        }
        let block = json!({"type": "tool_use", "name": name, "input": Value::Object(input)});
        if let Some(open) = self
            .open_assistant
            .and_then(|index| self.records.get_mut(index))
            && let Some(content) = open
                .get_mut("message")
                .and_then(|message| message.get_mut("content"))
                .and_then(Value::as_array_mut)
        {
            // Codex reports one applied patch twice (`patch_apply_end` and the `FileChange`
            // item); the same block twice in a row is that echo, not two edits.
            if content.last() != Some(&block) {
                content.push(block);
            }
            return;
        }
        self.records.push(json!({
            "type": "assistant",
            "message": {"role": "assistant", "content": [block]},
        }));
        self.open_assistant = Some(self.records.len() - 1);
    }

    /// Events carry no turn `response_item` lacks — except the applied file changes, which have
    /// no `response_item` form at all and are the only way to know what the session WROTE.
    fn event_msg(&mut self, payload: &Value) {
        let changes = match payload.get("type").and_then(Value::as_str) {
            Some("patch_apply_end") => payload.get("changes"),
            Some("item_completed") => payload
                .get("item")
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("FileChange"))
                .and_then(|item| item.get("changes")),
            _ => None,
        };
        let Some(changes) = changes.and_then(Value::as_object) else {
            return;
        };
        for path in changes.keys().take(MAX_PATHS_PER_CHANGE_EVENT) {
            self.tool("apply_patch", Some(path));
        }
    }

    /// Session facts ride as a record with no `type`, so the message and file extractors skip it
    /// and only `transcript_context` — which reads any record — picks it up.
    fn context(&mut self, fields: Map<String, Value>) {
        if !fields.is_empty() {
            self.records.push(Value::Object(fields));
        }
    }
}

/// Codex's content vocabulary, translated into Claude Code's.
///
/// ⚠️ AN IMAGE TRAVELS AS ITS TYPE AND NOTHING ELSE. Codex puts the whole image in the record as
/// a `data:` URI; copying that into a checkpoint block would cost megabytes to render six words,
/// and the URI itself never travels. The size therefore reads "unknown size" for a Codex image
/// where a Claude Code image reports real bytes — a stated loss, not an oversight.
fn content_blocks(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![json!({"type": "text", "text": text})]
        }
        Some(Value::Array(blocks)) => blocks.iter().filter_map(content_block).collect(),
        _ => Vec::new(),
    }
}

fn content_block(block: &Value) -> Option<Value> {
    match block.get("type").and_then(Value::as_str)? {
        "input_text" | "output_text" | "text" | "summary_text" => {
            let text = block.get("text").and_then(Value::as_str)?;
            (!text.trim().is_empty()).then(|| json!({"type": "text", "text": text}))
        }
        "input_image" | "output_image" => {
            let mut source = Map::new();
            if let Some(media_type) = data_uri_media_type(block.get("image_url")) {
                source.insert("media_type".to_string(), json!(media_type));
            }
            Some(json!({"type": "image", "source": Value::Object(source)}))
        }
        // `encrypted_content` is opaque by design and `refusal` is not the customer's words.
        _ => None,
    }
}

/// `data:image/png;base64,AAA…` → `image/png`, WITHOUT copying the payload.
fn data_uri_media_type(image_url: Option<&Value>) -> Option<&str> {
    let url = image_url.and_then(Value::as_str)?;
    let rest = url.strip_prefix("data:")?;
    let end = rest.find([';', ','])?;
    let media_type = rest.get(..end)?;
    (!media_type.is_empty()).then_some(media_type)
}

/// Codex's `session_meta`, under the field names `transcript_context` already reads. One owner
/// for the mapping: the key names below are the contract, and they are Claude Code's spelling.
fn session_meta_context(payload: &Value) -> Map<String, Value> {
    let mut fields = Map::new();
    put_str(&mut fields, "cwd", payload.get("cwd"));
    put_str(&mut fields, "version", payload.get("cli_version"));
    put_str(&mut fields, "entrypoint", payload.get("originator"));
    put_str(
        &mut fields,
        "gitBranch",
        payload.get("git").and_then(|git| git.get("branch")),
    );
    fields
}

/// Codex's `turn_context` — the only place `cwd`, `model` and `effort` survive into the TAIL of
/// a large rollout, because `session_meta` is a million bytes behind the window.
fn turn_context_context(payload: &Value) -> Map<String, Value> {
    let mut fields = Map::new();
    put_str(&mut fields, "cwd", payload.get("cwd"));
    put_str(&mut fields, "effort", payload.get("effort"));
    if let Some(model) = payload
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
    {
        // `transcript_context` reads the model off `message.model`, so that is where it goes.
        fields.insert("message".to_string(), json!({"model": model}));
    }
    fields
}

fn put_str(fields: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(text) = value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        fields.insert(key.to_string(), json!(text));
    }
}

#[cfg(test)]
#[path = "host_transcript_tests.rs"]
mod host_transcript_tests;
