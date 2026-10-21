//! `estelle hook stream` — mirror the host session into the customer's Estelle dashboard.
//!
//! Every hook event the host fires carries `transcript_path`. This module reads the transcript lines
//! appended since the last run (offset kept per session under `~/.estelle/stream/`), maps each typed
//! entry to a session-timeline event, redacts secrets, and returns the batch for `POST /session/events`.
//!
//! ⛔ Mapping is by the transcript's RECORDED structure, never by reading text: a `user` text block is the
//! customer, an `assistant` text block the agent, `tool_use`/`tool_result` the tools, `thinking` the
//! agent's recorded reasoning. Event ids are `<entry uuid>:<block index>`, so a re-run, an overlapping
//! read or a retried POST is a no-op on the server (which dedupes by id).
//!
//! ⚠️ What this covers today: Claude Code's JSONL transcript. Codex sends no `transcript_path` on most
//! events; its rollout format is a separate reader and is not here yet — a Codex session streams nothing
//! rather than something guessed.

use regex_lite::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// Characters kept per text field; the rest is replaced by a stated truncation.
pub(crate) const FIELD_CAP: usize = 20_000;
/// Most events one hook run sends; the remainder goes on the next run (the offset only advances past
/// what was sent).
pub(crate) const MAX_EVENTS_PER_RUN: usize = 200;

/// Injections the host adds to the user role that the customer did not type.
const INJECTED: &[&str] = &[
    "<system-reminder", "<command-", "<local-command", "<task-notification", "[SYSTEM NOTIFICATION", "Caveat:",
];

static SECRETS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\b(sk|rk|pk)_(live|test)_[A-Za-z0-9]{8,}", "[redacted stripe key]"),
        (r"\bestelle_(live|test)_[A-Za-z0-9_-]{8,}", "[redacted estelle key]"),
        (r"\bgh[pousr]_[A-Za-z0-9]{20,}", "[redacted github token]"),
        (r"\bxox[abprs]-[A-Za-z0-9-]{10,}", "[redacted slack token]"),
        (r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", "[redacted jwt]"),
        (r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----", "[redacted private key]"),
        (r"\b(AKIA|ASIA)[A-Z0-9]{16}\b", "[redacted aws key]"),
        (r"(postgres(?:ql)?://[^:\s/]+:)[^@\s]+@", "${1}[redacted]@"),
    ]
    .into_iter()
    .map(|(pattern, replacement)| (Regex::new(pattern).expect("static secret pattern"), replacement))
    .collect()
});

/// Remove credential-shaped values before anything leaves the machine.
pub(crate) fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for (pattern, replacement) in SECRETS.iter() {
        out = pattern.replace_all(&out, *replacement).into_owned();
    }
    out
}

fn capped(value: &Value) -> String {
    let raw = match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let text = redact(&raw);
    if text.chars().count() <= FIELD_CAP {
        return text;
    }
    let kept: String = text.chars().take(FIELD_CAP).collect();
    format!("{kept}\n… [truncated: {} more characters not shown]", text.chars().count() - FIELD_CAP)
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("image") => "[image]".to_string(),
                _ => b.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn summary(name: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "Read" => format!("Read {}", field("file_path")),
        "Bash" => {
            let described = field("description");
            if described.is_empty() { format!("Ran {}", field("command").chars().take(80).collect::<String>()) } else { described }
        }
        "Grep" | "Glob" => format!("Searched for {}", field("pattern")),
        "Agent" | "Task" => format!("Delegated: {}", field("description")),
        other => other.strip_prefix("mcp__Estelle__").map(str::to_string).unwrap_or_else(|| other.to_string()),
    }
}

fn diff_of(name: &str, input: &Value) -> String {
    let lines = |prefix: char, text: &str| text.lines().map(|l| format!("{prefix}{l}")).collect::<Vec<_>>();
    if name == "Write" {
        return lines('+', input.get("content").and_then(Value::as_str).unwrap_or("")).join("\n");
    }
    let edits: Vec<&Value> = if name == "MultiEdit" {
        input.get("edits").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default()
    } else {
        vec![input]
    };
    edits
        .iter()
        .flat_map(|e| {
            let mut out = lines('-', e.get("old_string").and_then(Value::as_str).unwrap_or(""));
            out.extend(lines('+', e.get("new_string").and_then(Value::as_str).unwrap_or("")));
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn event(id: String, kind: &str, author: Value, at: &str, payload: Value) -> Value {
    json!({"event_id": id, "kind": kind, "author": author, "at": at, "payload": payload})
}

/// The timeline events one Claude Code transcript entry stands for (often none).
pub(crate) fn events_of(entry: &Value) -> Vec<Value> {
    let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
    let (Some(at), Some(uuid)) = (entry.get("timestamp").and_then(Value::as_str), entry.get("uuid").and_then(Value::as_str)) else {
        return Vec::new();
    };
    // A message typed while the agent is mid-turn arrives as a queued-command attachment.
    if kind == "attachment" {
        let attachment = entry.get("attachment").cloned().unwrap_or(Value::Null);
        if attachment.get("type").and_then(Value::as_str) != Some("queued_command") {
            return Vec::new();
        }
        let text = result_text(attachment.get("prompt").unwrap_or(&Value::Null));
        let text = text.trim();
        if text.is_empty() || INJECTED.iter().any(|p| text.starts_with(p)) {
            return Vec::new();
        }
        return vec![event(format!("{uuid}:queued"), "customer_message", json!({"kind": "customer"}), at, json!({"text": capped(&Value::String(text.into()))}))];
    }
    if (kind != "user" && kind != "assistant") || entry.get("isMeta").and_then(Value::as_bool) == Some(true)
        || entry.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let sidechain = entry.get("isSidechain").and_then(Value::as_bool) == Some(true);
    let who = if sidechain { "subagent" } else { "agent" };
    let content = entry.pointer("/message/content").cloned().unwrap_or(Value::Null);
    let blocks = match content {
        Value::String(text) => vec![json!({"type": "text", "text": text})],
        Value::Array(blocks) => blocks,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let id = format!("{uuid}:{index}");
        let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
        let text = block.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
        match (kind, btype) {
            ("user", "text") if !text.is_empty() && !INJECTED.iter().any(|p| text.starts_with(p)) => {
                if sidechain {
                    out.push(event(id, "agent_message", json!({"kind": "agent", "id": who}), at, json!({"text": capped(&Value::String(text)), "brief_to_subagent": true})));
                } else {
                    out.push(event(id, "customer_message", json!({"kind": "customer"}), at, json!({"text": capped(&Value::String(text))})));
                }
            }
            ("user", "image") => out.push(event(id, "artifact", json!({"kind": "customer"}), at, json!({"title": "Image you attached", "media": "image", "stored": false}))),
            ("assistant", "text") if !text.is_empty() => {
                out.push(event(id, "agent_message", json!({"kind": "agent", "id": who}), at, json!({"text": capped(&Value::String(text)), "subagent": sidechain})));
            }
            ("assistant", "thinking") => {
                let thought = block.get("thinking").and_then(Value::as_str).unwrap_or("").trim();
                if !thought.is_empty() {
                    out.push(event(id, "thinking", json!({"kind": "agent", "id": who}), at, json!({"text": capped(&Value::String(thought.into()))})));
                }
            }
            ("assistant", "tool_use") => {
                let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let call_id = block.get("id").cloned().unwrap_or(Value::Null);
                if matches!(name, "Edit" | "Write" | "MultiEdit") {
                    let path = input.get("file_path").and_then(Value::as_str).unwrap_or("");
                    out.push(event(id, "edit", json!({"kind": "agent", "id": who}), at,
                        json!({"path": path, "diff": capped(&Value::String(diff_of(name, &input))), "call_id": call_id})));
                } else {
                    let estelle = name.starts_with("mcp__Estelle__");
                    out.push(event(id, "tool_call", json!({"kind": if estelle { "estelle" } else { "agent" }, "id": who}), at,
                        json!({"call_id": call_id, "tool": name, "summary": capped(&Value::String(summary(name, &input))), "input": capped(&input)})));
                }
            }
            ("user", "tool_result") => {
                out.push(event(id, "tool_result", json!({"kind": "agent", "id": who}), at, json!({
                    "call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                    "ok": block.get("is_error").and_then(Value::as_bool) != Some(true),
                    "output": capped(&Value::String(result_text(block.get("content").unwrap_or(&Value::Null)))),
                })));
            }
            _ => {}
        }
    }
    // The model's own token accounting for this reply, attached once (to the entry's first event) so totals
    // are a plain sum and never double-count a reply split into several blocks.
    if let (Some(usage), Some(first)) = (entry.pointer("/message/usage"), out.first_mut()) {
        let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        first["payload"]["usage"] = json!({
            "input": n("input_tokens"), "output": n("output_tokens"),
            "cache_read": n("cache_read_input_tokens"), "cache_write": n("cache_creation_input_tokens"),
            "model": entry.pointer("/message/model").cloned().unwrap_or(Value::Null),
        });
    }
    out
}

/// The first event a session sends: its lifecycle start, carrying the title and where it runs.
pub(crate) fn started(session_id: &str, at: &str, title: &str, repo: &str, harness: &str) -> Value {
    event(format!("{session_id}:started"), "lifecycle", json!({"kind": "system", "id": "estelle-hook"}), at,
        json!({"state": "running", "reason": "started", "title": redact(title), "repo": repo, "origin": harness, "harness": harness}))
}

/// A lifecycle transition the HOST reported (a prompt, a Stop, a session end) — never inferred from text.
pub(crate) fn lifecycle(id: &str, at: &str, state: &str, reason: &str) -> Value {
    event(id.to_string(), "lifecycle", json!({"kind": "system", "id": "estelle-hook"}), at, json!({"state": state, "reason": reason}))
}

/// Parse the appended transcript text into events, up to the per-run bound. Returns the events and how
/// many BYTES of `text` they consumed (only whole lines; the offset advances by exactly this).
pub(crate) fn parse_appended(text: &str) -> (Vec<Value>, usize) {
    let mut events = Vec::new();
    let mut consumed = 0usize;
    for line in text.split_inclusive('\n') {
        if !line.ends_with('\n') || events.len() >= MAX_EVENTS_PER_RUN {
            break;
        }
        if let Ok(entry) = serde_json::from_str::<Value>(line.trim_end()) {
            events.extend(events_of(&entry));
        }
        consumed += line.len();
    }
    (events, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_prompt_is_the_customer_and_an_injection_is_not() {
        let typed = json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-21T10:00:00Z", "message": {"content": "fix the bug"}});
        let injected = json!({"type": "user", "uuid": "u2", "timestamp": "2026-10-21T10:00:00Z", "message": {"content": "<system-reminder>x"}});
        assert_eq!(events_of(&typed)[0]["kind"], "customer_message");
        assert!(events_of(&injected).is_empty());
    }

    #[test]
    fn tools_edits_and_results_keep_their_ids_and_pairing() {
        let call = json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-21T10:00:00Z", "message": {"content": [
            {"type": "tool_use", "id": "c1", "name": "Bash", "input": {"command": "ls", "description": "List files"}},
            {"type": "tool_use", "id": "c2", "name": "Edit", "input": {"file_path": "a.rs", "old_string": "x", "new_string": "y"}}]}});
        let out = events_of(&call);
        assert_eq!((out[0]["kind"].as_str(), out[0]["payload"]["summary"].as_str()), (Some("tool_call"), Some("List files")));
        assert_eq!((out[1]["kind"].as_str(), out[1]["payload"]["diff"].as_str()), (Some("edit"), Some("-x\n+y")));
        assert_eq!(out[1]["event_id"], "a1:1");
    }

    #[test]
    fn secrets_are_redacted_before_they_leave() {
        let text = "key sk_live_abcdefghijklmnop and postgres://u:hunter2@host/db";
        let out = redact(text);
        assert!(!out.contains("abcdefghijklmnop") && !out.contains("hunter2"), "{out}");
    }

    #[test]
    fn only_whole_lines_are_consumed() {
        let full = "{\"type\":\"user\",\"uuid\":\"u\",\"timestamp\":\"2026-10-21T10:00:00Z\",\"message\":{\"content\":\"hi\"}}\n";
        let (events, used) = parse_appended(&format!("{full}{{\"partial"));
        assert_eq!((events.len(), used), (1, full.len()));
    }
}
