//! `estelle hook stream` — the PURE half: one transcript entry in, the session-timeline events it stands for out.
//!
//! Every hook event the host fires carries `transcript_path`. [`crate::hook_stream_run`] reads the lines
//! appended since the last run and hands each parsed entry here; this module maps it, redacts secrets and
//! strips what Postgres `jsonb` cannot hold. No I/O happens in this file.
//!
//! ⛔ Mapping is by the transcript's RECORDED structure, never by reading text: a `user` text block is the
//! customer, an `assistant` text block the agent, `tool_use`/`tool_result` the tools, `thinking` the agent's
//! recorded reasoning, `system/turn_duration` the host's own "this turn ended". Event ids are
//! `<entry uuid>:<block index>` (or `<entry uuid>:<tag>`), so a re-run, an overlapping read or a retried POST
//! is a no-op on the server, which dedupes by id.
//!
//! 🔑 PARITY OWNER. This is a port of the local stand-in bridge (`transcript-bridge.py`) that fed the
//! dashboard before this hook shipped; where the two once differed (tool `input` as a string, `Ran` missing
//! from a described Bash call, the main agent's id spelled `agent`) the bridge's shape is the one the
//! dashboard was built against, and this file now emits it.
//!
//! ⚠️ What this covers: Claude Code's JSONL transcript. Codex's rollout is a different format and is NOT
//! read here — a Codex session streams nothing rather than something guessed (see `hook_stream_run`).

use chrono::{DateTime, SecondsFormat, Utc};
use regex_lite::Regex;
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

/// Characters kept per text field; the rest is replaced by a stated truncation.
pub(crate) const FIELD_CAP: usize = 20_000;
/// Events per `POST /session/events` — under the server's `MAX_EVENTS_PER_BATCH` (200).
pub(crate) const MAX_BATCH_EVENTS: usize = 50;
/// Bytes per request, so one batch of large tool outputs stays small on the wire.
pub(crate) const MAX_BATCH_BYTES: usize = 2_000_000;
/// The longest tag value read out of a `<task-notification>` (an id or a status word).
const MAX_NOTE_TAG: usize = 200;
/// Who the transport-level events (lifecycle) are attributed to.
pub(crate) const HOOK_AUTHOR: &str = "estelle-hook";
/// The only hook whose `hook_additional_context` is "what Estelle added to this turn". SessionStart skills
/// and PreToolUse notes are other plugins' or other purposes' output and are left out.
const CONTEXT_HOOKS: &[&str] = &["UserPromptSubmit"];

/// Injections the host adds to the user role that the customer did not type.
const INJECTED: &[&str] = &[
    "<system-reminder",
    "<command-",
    "<local-command",
    "<task-notification",
    "[SYSTEM NOTIFICATION",
    "Caveat:",
];

static SECRETS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\b(sk|rk|pk)_(live|test)_[A-Za-z0-9]{8,}", "[redacted stripe key]"),
        (r"\bestelle_(live|test)_[A-Za-z0-9_-]{8,}", "[redacted estelle key]"),
        (r"\bgh[pousr]_[A-Za-z0-9]{20,}", "[redacted github token]"),
        (r"\bxox[abprs]-[A-Za-z0-9-]{10,}", "[redacted slack token]"),
        (r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", "[redacted jwt]"),
        (r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----", "[redacted private key]"),
        (r"(postgres(?:ql)?://[^:\s/]+:)[^@\s]+@", "${1}[redacted]@"),
        (r"\b(AKIA|ASIA)[A-Z0-9]{16}\b", "[redacted aws key]"),
        (
            r#"(?i)\b([A-Z0-9_]*(?:SECRET|TOKEN|PASSWORD|API_KEY|PRIVATE_KEY)[A-Z0-9_]*)\s*=\s*["']?[^\s"']{8,}"#,
            "${1}=[redacted]",
        ),
    ]
    .into_iter()
    // A pattern that failed to compile would silently stop redacting its shape, so the count is pinned by
    // `every_secret_pattern_compiles` rather than trusted.
    .filter_map(|(pattern, replacement)| Regex::new(pattern).ok().map(|re| (re, replacement)))
    .collect()
});

/// How many secret shapes [`redact`] knows; a test holds the compiled count to it.
#[cfg(test)]
pub(crate) const SECRET_SHAPES: usize = 9;

static NOTE_TAG: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"<(tool-use-id|status)>([^<]{1,200})</(?:tool-use-id|status)>").ok()
});

/// Remove credential-shaped values before anything leaves the machine.
pub(crate) fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for (pattern, replacement) in SECRETS.iter() {
        out = pattern.replace_all(&out, *replacement).into_owned();
    }
    out
}

/// Redacted, then capped at [`FIELD_CAP`] characters with the truncation stated in the text itself.
pub(crate) fn cap(raw: &str) -> String {
    let text = redact(raw);
    let total = text.chars().count();
    if total <= FIELD_CAP {
        return text;
    }
    let kept: String = text.chars().take(FIELD_CAP).collect();
    format!(
        "{kept}\n… [truncated: {} more characters not shown]",
        total - FIELD_CAP
    )
}

/// 🔴 Postgres `jsonb` cannot hold U+0000. A tool that printed binary (a PNG through `head`) put one NUL in
/// one event, the server answered 503 for the WHOLE batch, and because the offset only advances on success
/// the hook resent the same poisoned batch on every run, forever. Stripped from every string and every key.
pub(crate) fn scrub(value: Value) -> Value {
    match value {
        Value::String(text) if text.contains('\0') => Value::String(text.replace('\0', "")),
        Value::Array(items) => Value::Array(items.into_iter().map(scrub).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k.replace('\0', ""), scrub(v)))
                .collect(),
        ),
        other => other,
    }
}

/// Facts one pass over a transcript learns besides its events.
#[derive(Debug, Default)]
pub(crate) struct Notes {
    /// The first line of the first thing the customer typed — the session's title.
    pub(crate) title: Option<String>,
    /// Background tasks the host reported finished: `(tool_use_id, status)`, from `<task-notification>`.
    pub(crate) notified: Vec<(String, String)>,
}

fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}

fn injected(text: &str) -> bool {
    INJECTED.iter().any(|prefix| text.starts_with(prefix))
}

pub(crate) fn event(id: String, kind: &str, author: Value, at: &str, payload: Value) -> Value {
    json!({"event_id": id, "kind": kind, "author": author, "at": at, "payload": payload})
}

/// A lifecycle transition, attributed to the hook. Only ever built from something the HOST recorded.
pub(crate) fn lifecycle(id: String, at: &str, state: &str, reason: &str) -> Value {
    event(
        id,
        "lifecycle",
        json!({"kind": "system", "id": HOOK_AUTHOR}),
        at,
        json!({"state": state, "reason": reason}),
    )
}

/// The first event a session sends: its lifecycle start, carrying the title and where it runs.
pub(crate) fn started(session_id: &str, at: &str, title: &str, repo: &str, harness: &str) -> Value {
    event(
        format!("{session_id}:started"),
        "lifecycle",
        json!({"kind": "system", "id": HOOK_AUTHOR}),
        at,
        json!({"state": "running", "reason": "started", "title": redact(title), "repo": repo, "origin": harness, "harness": harness}),
    )
}

fn note_title(notes: &mut Notes, text: &str) {
    if notes.title.is_none() {
        notes.title = Some(text.lines().next().unwrap_or("").chars().take(90).collect());
    }
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match s(b, "type") {
                "image" => "[image]".to_string(),
                _ => s(b, "text").to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn summary(name: &str, input: &Value) -> String {
    match name {
        "Read" => format!("Read {}", s(input, "file_path")),
        "Bash" => {
            let described = s(input, "description");
            let what = if described.is_empty() {
                s(input, "command").chars().take(80).collect()
            } else {
                described.to_string()
            };
            format!("Ran {what}")
        }
        "Grep" | "Glob" => format!("Searched for {}", s(input, "pattern")),
        "Agent" | "Task" => format!("Delegated: {}", s(input, "description")),
        "Skill" => format!("Loaded skill {}", s(input, "skill")),
        other => other
            .strip_prefix("mcp__Estelle__")
            .unwrap_or(other)
            .to_string(),
    }
}

fn diff_of(name: &str, input: &Value) -> String {
    let lines = |prefix: char, text: &str| {
        text.lines()
            .map(|l| format!("{prefix}{l}"))
            .collect::<Vec<_>>()
    };
    if name == "Write" {
        return lines('+', s(input, "content")).join("\n");
    }
    let edits: Vec<&Value> = if name == "MultiEdit" {
        input
            .get("edits")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default()
    } else {
        vec![input]
    };
    edits
        .iter()
        .flat_map(|e| {
            let mut out = lines('-', s(e, "old_string"));
            out.extend(lines('+', s(e, "new_string")));
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The tool's input as an object: every string capped and redacted, plus `path` when it named a file.
fn tool_input(input: &Value) -> Value {
    let mut out = Map::new();
    if let Value::Object(fields) = input {
        for (key, value) in fields {
            out.insert(
                key.clone(),
                match value {
                    Value::String(text) => Value::String(cap(text)),
                    other => other.clone(),
                },
            );
        }
        if let Some(path) = fields.get("file_path") {
            out.insert("path".to_string(), path.clone());
        }
    }
    Value::Object(out)
}

/// The timeline events one Claude Code transcript entry stands for (often none).
///
/// `child` is true when the entry comes from a SUBAGENT's own transcript, which is mirrored as its own
/// timeline: there its words are the conversation (not an aside), its handback is its report, and the
/// parent-only signals (turn end, estelle context, control returned) do not apply.
pub(crate) fn events_of(entry: &Value, notes: &mut Notes, child: bool) -> Vec<Value> {
    let kind = s(entry, "type");
    let (at, uuid) = (s(entry, "timestamp"), s(entry, "uuid"));
    if at.is_empty() || uuid.is_empty() {
        return Vec::new();
    }
    match kind {
        // Claude Code writes `system/turn_duration` when the agent's turn ENDS: from here it waits on the person.
        "system"
            if s(entry, "subtype") == "turn_duration" && !child && !flag(entry, "isSidechain") =>
        {
            vec![lifecycle(
                format!("{uuid}:turn-end"),
                at,
                "waiting_for_review",
                "input_required",
            )]
        }
        "attachment" => attachment_events(entry, uuid, at, notes, child),
        "user" | "assistant" if !flag(entry, "isMeta") && !flag(entry, "isCompactSummary") => {
            let mut out = block_events(entry, kind, uuid, at, notes, child);
            attach_usage(entry, kind, &mut out);
            out
        }
        _ => Vec::new(),
    }
}

fn attachment_events(
    entry: &Value,
    uuid: &str,
    at: &str,
    notes: &mut Notes,
    child: bool,
) -> Vec<Value> {
    let attachment = entry.get("attachment").unwrap_or(&Value::Null);
    match s(attachment, "type") {
        // A message typed while the agent is mid-turn is stored as a queued command, not a user entry.
        "queued_command" => queued_events(attachment, uuid, at, notes, child),
        "hook_additional_context" if !child => estelle_context(attachment, uuid, at),
        _ => Vec::new(),
    }
}

fn queued_events(
    attachment: &Value,
    uuid: &str,
    at: &str,
    notes: &mut Notes,
    child: bool,
) -> Vec<Value> {
    let prompt = attachment.get("prompt").unwrap_or(&Value::Null);
    let text = match prompt {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| s(b, "type") == "text")
            .map(|b| s(b, "text"))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let text = text.trim();
    if text.starts_with("<task-notification") {
        // A background task (a subagent, a shell) reported back: the host hands control back to the agent.
        let mut tool_use_id = None;
        let mut status = "completed".to_string();
        for capture in NOTE_TAG.iter().flat_map(|re| re.captures_iter(text)) {
            let value: String = capture[2].chars().take(MAX_NOTE_TAG).collect();
            if &capture[1] == "tool-use-id" {
                tool_use_id = Some(value)
            } else {
                status = value
            }
        }
        if let Some(id) = tool_use_id {
            notes.notified.push((id, status));
        }
        return if child {
            Vec::new()
        } else {
            vec![lifecycle(
                format!("{uuid}:notified"),
                at,
                "running",
                "control_returned",
            )]
        };
    }
    if text.is_empty() || injected(text) {
        return Vec::new();
    }
    note_title(notes, text);
    vec![
        event(
            format!("{uuid}:queued"),
            "customer_message",
            json!({"kind": "customer"}),
            at,
            json!({"text": cap(text)}),
        ),
        lifecycle(format!("{uuid}:input"), at, "running", "input_received"),
    ]
}

/// What the Estelle hook injected into one prompt, EXACTLY as the host recorded it (`hook_additional_context`).
///
/// ⛔ The timeline vocabulary is CLOSED and has no `context` kind, so this rides as a `tool_result` with NO
/// `call_id` and a `source: "estelle-hook"` marker; the dashboard draws it as one "Estelle added …" row per
/// turn. Other plugins' UserPromptSubmit output is dropped: only parts that name Estelle are Estelle's.
fn estelle_context(attachment: &Value, uuid: &str, at: &str) -> Vec<Value> {
    let hook = [s(attachment, "hookEvent"), s(attachment, "hookName")]
        .into_iter()
        .find(|h| !h.is_empty())
        .unwrap_or("");
    if !CONTEXT_HOOKS.contains(&hook) {
        return Vec::new();
    }
    let parts: Vec<&str> = match attachment.get("content") {
        Some(Value::String(text)) => vec![text.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let mine: Vec<&str> = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && p.to_lowercase().contains("estelle"))
        .collect();
    if mine.is_empty() {
        return Vec::new();
    }
    let joined = mine.join("\n\n");
    vec![event(
        format!("{uuid}:estelle-context"),
        "tool_result",
        json!({"kind": "estelle", "id": HOOK_AUTHOR}),
        at,
        json!({
            "source": "estelle-hook", "hook": hook, "output": cap(&joined), "chars": joined.chars().count(), "parts": mine.len(),
        }),
    )]
}

fn block_events(
    entry: &Value,
    kind: &str,
    uuid: &str,
    at: &str,
    notes: &mut Notes,
    child: bool,
) -> Vec<Value> {
    let sidechain = flag(entry, "isSidechain");
    let blocks = match entry.pointer("/message/content") {
        Some(Value::String(text)) => vec![json!({"type": "text", "text": text})],
        Some(Value::Array(blocks)) => blocks.clone(),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let id = format!("{uuid}:{index}");
        match (kind, s(block, "type")) {
            ("user", "text") => user_text(block, id, uuid, at, sidechain, child, notes, &mut out),
            ("user", "image") => out.push(event(
                id,
                "artifact",
                json!({"kind": "customer"}),
                at,
                json!({"title": "Image you attached", "media": "image", "stored": false}),
            )),
            ("user", "tool_result") => out.push(event(
                id,
                "tool_result",
                json!({"kind": "agent", "id": "claude-code"}),
                at,
                json!({
                    "call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                    "ok": !flag(block, "is_error"),
                    "output": cap(&result_text(block.get("content").unwrap_or(&Value::Null))),
                }),
            )),
            ("assistant", "thinking") => {
                let thought = s(block, "thinking");
                if !thought.trim().is_empty() {
                    let who = if sidechain { "subagent" } else { "claude-code" };
                    out.push(event(
                        id,
                        "thinking",
                        json!({"kind": "agent", "id": who}),
                        at,
                        json!({"text": cap(thought)}),
                    ));
                }
            }
            ("assistant", "text") => assistant_text(block, id, at, sidechain, child, &mut out),
            ("assistant", "tool_use") => out.push(tool_use(block, id, at, sidechain, child)),
            _ => {}
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn user_text(
    block: &Value,
    id: String,
    uuid: &str,
    at: &str,
    sidechain: bool,
    child: bool,
    notes: &mut Notes,
    out: &mut Vec<Value>,
) {
    let text = s(block, "text").trim();
    if text.is_empty() || injected(text) {
        return;
    }
    if sidechain {
        // The parent's brief to a subagent is written into the subagent's transcript as a `user` turn.
        out.push(event(
            id,
            "agent_message",
            json!({"kind": "agent", "id": "claude-code"}),
            at,
            json!({"text": cap(text), "brief_to_subagent": true}),
        ));
        return;
    }
    note_title(notes, text);
    out.push(event(
        id,
        "customer_message",
        json!({"kind": "customer"}),
        at,
        json!({"text": cap(text)}),
    ));
    if !child && !out.iter().any(|e| e["kind"] == "lifecycle") {
        out.push(lifecycle(
            format!("{uuid}:input"),
            at,
            "running",
            "input_received",
        ));
    }
}

fn assistant_text(
    block: &Value,
    id: String,
    at: &str,
    sidechain: bool,
    child: bool,
    out: &mut Vec<Value>,
) {
    let text = s(block, "text");
    if text.trim().is_empty() {
        return;
    }
    let (who, payload) = if child {
        // Inside a subagent's own timeline its words are the conversation, not a collapsed aside.
        ("subagent", json!({"text": cap(text)}))
    } else if sidechain {
        ("subagent", json!({"text": cap(text), "subagent": true}))
    } else {
        ("claude-code", json!({"text": cap(text)}))
    };
    out.push(event(
        id,
        "agent_message",
        json!({"kind": "agent", "id": who}),
        at,
        payload,
    ));
}

fn tool_use(block: &Value, id: String, at: &str, sidechain: bool, child: bool) -> Value {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let input = block.get("input").cloned().unwrap_or(Value::Null);
    let call_id = block.get("id").cloned().unwrap_or(Value::Null);
    if child && name == "SubagentHandback" {
        return event(
            id,
            "agent_message",
            json!({"kind": "agent", "id": "subagent"}),
            at,
            json!({"text": cap(s(&input, "message")), "report": true}),
        );
    }
    if matches!(name, "Edit" | "Write" | "MultiEdit") {
        let who = if sidechain { "subagent" } else { "claude-code" };
        return event(
            id,
            "edit",
            json!({"kind": "agent", "id": who}),
            at,
            json!({"path": s(&input, "file_path"), "diff": cap(&diff_of(name, &input)), "call_id": call_id}),
        );
    }
    let author = if name.starts_with("mcp__Estelle__") {
        "estelle"
    } else {
        "agent"
    };
    event(
        id,
        "tool_call",
        json!({"kind": author, "id": "claude-code"}),
        at,
        json!({"call_id": call_id, "tool": name, "summary": cap(&summary(name, &input)), "input": tool_input(&input)}),
    )
}

/// The model's own token accounting for this reply, attached once (to the entry's first event) so totals are a
/// plain sum and never double-count a reply split into several blocks.
fn attach_usage(entry: &Value, kind: &str, out: &mut [Value]) {
    let (Some(usage), Some(first)) = (
        entry
            .pointer("/message/usage")
            .filter(|u| u.is_object() && kind == "assistant"),
        out.first_mut(),
    ) else {
        return;
    };
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    first["payload"]["usage"] = json!({
        "input": n("input_tokens"), "output": n("output_tokens"),
        "cache_read": n("cache_read_input_tokens"), "cache_write": n("cache_creation_input_tokens"),
        "model": entry.pointer("/message/model").cloned().unwrap_or(Value::Null),
    });
}

/// The instant a transcript entry was recorded, when it carries a parseable one.
pub(crate) fn entry_time(entry: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s(entry, "timestamp"))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// 🔴 An event the hook derives (a Stop, a SessionEnd, anything not copied from a transcript entry) is stamped
/// no later than the transcript's LAST entry — never the hook's clock. Stamped with `now`, a session that
/// stopped hours ago read as freshly active on every replay, because the listing's `last_at` is the newest `at`.
/// `wanted` is clamped to `last`; with no `wanted` the answer is `last`; with neither, `now` (stated fallback:
/// a transcript with no timestamped entry at all).
pub(crate) fn stamp(
    wanted: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    let chosen = match (wanted, last) {
        (Some(w), Some(l)) => w.min(l),
        (Some(t), None) | (None, Some(t)) => t,
        (None, None) => now,
    };
    chosen.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Split into requests bounded by [`MAX_BATCH_EVENTS`] and [`MAX_BATCH_BYTES`] (an oversize single event goes alone).
pub(crate) fn batches(events: Vec<Value>) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = Vec::new();
    let mut chunk: Vec<Value> = Vec::new();
    let mut size = 0usize;
    for event in events {
        let n = event.to_string().len();
        if !chunk.is_empty() && (chunk.len() >= MAX_BATCH_EVENTS || size + n > MAX_BATCH_BYTES) {
            out.push(std::mem::take(&mut chunk));
            size = 0;
        }
        chunk.push(event);
        size += n;
    }
    if !chunk.is_empty() {
        out.push(chunk);
    }
    out
}

#[cfg(test)]
#[path = "hook_stream_tests.rs"]
mod tests;
