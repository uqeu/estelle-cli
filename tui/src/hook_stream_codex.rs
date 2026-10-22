//! `estelle hook stream` for CODEX — one rollout record in, the session-timeline events it stands for out.
//!
//! Codex writes its session to `~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<thread id>.jsonl`, one record
//! per line: `{"timestamp", "type", "payload"}` (2026; `ordinal` on most lines). This maps the families the
//! dashboard draws onto the SAME event vocabulary the Claude Code mapper ([`crate::hook_stream`]) emits, so
//! the timeline does not know which host wrote it beyond the `codex` author id:
//!
//! | rollout record | event |
//! |---|---|
//! | `response_item/message` role `user` (not an injection) | `customer_message` + `lifecycle running/input_received` |
//! | `response_item/message` role `assistant` | `agent_message` (author `codex`) |
//! | `response_item/agent_message` (inter-agent) | `agent_message`, `subagent: true` |
//! | `response_item/reasoning` with a readable `summary` | `thinking` (`encrypted_content` is never read) |
//! | `response_item/function_call` · `custom_tool_call` · `local_shell_call` | `tool_call` (`apply_patch` → `edit`) |
//! | `response_item/*_call_output` | `tool_result` |
//! | `event_msg/task_complete` · `turn_aborted` | `lifecycle waiting_for_review/input_required` |
//!
//! ⛔ Messages are read from `response_item` ONLY. `event_msg/user_message` and `event_msg/agent_message`
//! repeat the same words (measured on this machine's rollouts: 45 `user_message` beside 46 user
//! `response_item`s, 2,448 `agent_message` beside 2,450 assistant ones), so reading both would double
//! every line. `developer` messages are the harness talking to the model and are skipped.
//!
//! 🔑 EVENT IDS ARE `r<byte offset of the line>:<n>`. A rollout record has no uuid and `ordinal` is missing
//! on some lines, but a rollout is append-only, so a line's byte offset is stable for the life of the
//! file and a replay (or a retried POST) is a no-op on the server, which dedupes by id.
//!
//! ⚠️ LIMITS, STATED: `ok` on a `tool_result` is `null` — the rollout's output record carries no success
//! flag, and inferring one from the output text would be a guess. The 2025 legacy rollout (bare
//! `ResponseItem` lines) is read through the same table when its line carries a `timestamp`; a line without
//! one is skipped, never stamped with the clock. Codex subagent threads are NOT mirrored as child timelines.

use crate::hook_stream::{Notes, cap, event, lifecycle};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// Day directories (`YYYY/MM/DD`, newest first) searched for a session's rollout when the hook payload
/// carries no `transcript_path`.
const MAX_ROLLOUT_DAYS: usize = 14;
/// Entries read per directory during that search.
const MAX_DIR_ENTRIES: usize = 2_000;

/// Who the main Codex agent's events are attributed to.
pub(crate) const CODEX_AGENT: &str = "codex";

/// User-role text the HARNESS wrote, not the person: Codex's environment block, injected instruction
/// files and skills, plus the injections Claude Code shares (task notifications, reminders).
const CODEX_INJECTED: &[&str] = &[
    "<environment_context",
    "<user_instructions",
    "# AGENTS.md instructions",
    "<skill",
    "<turn_aborted",
    "<user_shell_command",
    "<system-reminder",
    "<task-notification",
    "<command-",
];

fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn injected(text: &str) -> bool {
    let text = text.trim_start();
    CODEX_INJECTED.iter().any(|prefix| text.starts_with(prefix))
}

/// The record's payload and its kind: a 2026 record nests it under `payload`; a 2025 line IS the item.
fn item_of(record: &Value) -> Option<(&str, &Value)> {
    match (s(record, "type"), record.get("payload")) {
        ("response_item", Some(payload)) => Some(("item", payload)),
        ("event_msg", Some(payload)) => Some(("event", payload)),
        ("message" | "reasoning" | "function_call" | "function_call_output", None) => {
            Some(("item", record))
        }
        _ => None,
    }
}

/// Every event one rollout line stands for (often none). `offset` is the line's byte offset in the file.
pub(crate) fn codex_events_of(record: &Value, offset: u64, notes: &mut Notes) -> Vec<Value> {
    let at = s(record, "timestamp");
    let Some((family, payload)) = item_of(record) else {
        return Vec::new();
    };
    if at.is_empty() {
        return Vec::new();
    }
    let id = |tag: &str| format!("r{offset}:{tag}");
    let out = match (family, s(payload, "type")) {
        ("item", "message") => message(payload, &id, at, notes),
        ("item", "agent_message") => text_of(payload.get("content"))
            .map(|text| {
                vec![event(
                    id("0"),
                    "agent_message",
                    json!({"kind": "agent", "id": "subagent"}),
                    at,
                    json!({"text": cap(&text), "subagent": true}),
                )]
            })
            .unwrap_or_default(),
        ("item", "reasoning") => text_of(payload.get("summary"))
            .map(|text| {
                vec![event(
                    id("0"),
                    "thinking",
                    json!({"kind": "agent", "id": CODEX_AGENT}),
                    at,
                    json!({"text": cap(&text)}),
                )]
            })
            .unwrap_or_default(),
        ("item", "function_call" | "custom_tool_call" | "local_shell_call") => {
            vec![tool_call(payload, id("0"), at)]
        }
        ("item", "function_call_output" | "custom_tool_call_output") => vec![event(
            id("0"),
            "tool_result",
            json!({"kind": "agent", "id": CODEX_AGENT}),
            at,
            json!({
                "call_id": payload.get("call_id").cloned().unwrap_or(Value::Null),
                "ok": Value::Null,
                "output": cap(&output_text(payload.get("output"))),
            }),
        )],
        ("event", "task_complete" | "turn_aborted") => vec![lifecycle(
            id("turn-end"),
            at,
            "waiting_for_review",
            "input_required",
        )],
        _ => Vec::new(),
    };
    assert!(
        out.iter()
            .all(|e| e["event_id"].as_str().is_some_and(|i| i.starts_with('r'))),
        "every Codex event id is anchored to its line's byte offset"
    );
    out
}

fn message(
    payload: &Value,
    id: &dyn Fn(&str) -> String,
    at: &str,
    notes: &mut Notes,
) -> Vec<Value> {
    let role = s(payload, "role");
    let blocks: Vec<Value> = match payload.get("content") {
        Some(Value::String(text)) => vec![json!({"type": "input_text", "text": text})],
        Some(Value::Array(blocks)) => blocks.clone(),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let text = s(block, "text").trim();
        match (role, s(block, "type")) {
            ("user", "input_text" | "text") if !text.is_empty() && !injected(text) => {
                if notes.title.is_none() {
                    notes.title =
                        Some(text.lines().next().unwrap_or("").chars().take(90).collect());
                }
                out.push(event(
                    id(&index.to_string()),
                    "customer_message",
                    json!({"kind": "customer"}),
                    at,
                    json!({"text": cap(text)}),
                ));
            }
            ("user", "input_image") => out.push(event(
                id(&index.to_string()),
                "artifact",
                json!({"kind": "customer"}),
                at,
                json!({"title": "Image you attached", "media": "image", "stored": false}),
            )),
            ("assistant", "output_text" | "text") if !text.is_empty() => out.push(event(
                id(&index.to_string()),
                "agent_message",
                json!({"kind": "agent", "id": CODEX_AGENT}),
                at,
                json!({"text": cap(s(block, "text"))}),
            )),
            _ => {}
        }
    }
    if role == "user" && out.iter().any(|e| e["kind"] == "customer_message") {
        out.push(lifecycle(id("input"), at, "running", "input_received"));
    }
    out
}

/// The non-empty texts of a content/summary array (or a bare string), joined; `None` when there are none.
fn text_of(content: Option<&Value>) -> Option<String> {
    let parts: Vec<&str> = match content {
        Some(Value::String(text)) => vec![text.as_str()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .map(|b| s(b, "text"))
            .filter(|t| !t.trim().is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let joined = parts.join("\n");
    (!joined.trim().is_empty()).then_some(joined)
}

fn output_text(output: Option<&Value>) -> String {
    match output {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(_)) => text_of(output).unwrap_or_default(),
        Some(Value::Object(fields)) => fields
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| Value::Object(fields.clone()).to_string()),
        _ => String::new(),
    }
}

/// The call's arguments as an object: a `function_call`'s JSON-encoded `arguments` parsed, a custom tool's
/// free-text `input` under `input`, a shell call's `action`. Every string is capped and redacted.
fn arguments(payload: &Value) -> Value {
    let raw = match s(payload, "type") {
        "function_call" => serde_json::from_str::<Value>(s(payload, "arguments"))
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({"arguments": s(payload, "arguments")})),
        "local_shell_call" => payload.get("action").cloned().unwrap_or(Value::Null),
        _ => json!({"input": s(payload, "input")}),
    };
    let mut out = Map::new();
    if let Value::Object(fields) = raw {
        for (key, value) in fields {
            let kept = match value {
                Value::String(text) => Value::String(cap(&text)),
                Value::Array(items) => Value::String(cap(&Value::Array(items).to_string())),
                other => other,
            };
            out.insert(key, kept);
        }
    }
    Value::Object(out)
}

/// The first file an `apply_patch` names (`*** Update File: <path>` and its Add/Delete siblings).
fn patched_path(patch: &str) -> &str {
    patch
        .lines()
        .find_map(|line| {
            ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix))
        })
        .map(str::trim)
        .unwrap_or("")
}

fn tool_call(payload: &Value, id: String, at: &str) -> Value {
    let name = match (s(payload, "namespace"), s(payload, "name")) {
        (_, "") => "shell".to_string(),
        ("", name) => name.to_string(),
        (namespace, name) => format!("{namespace}.{name}"),
    };
    let call_id = payload.get("call_id").cloned().unwrap_or(Value::Null);
    if name == "apply_patch" {
        let patch = s(payload, "input");
        return event(
            id,
            "edit",
            json!({"kind": "agent", "id": CODEX_AGENT}),
            at,
            json!({"path": patched_path(patch), "diff": cap(patch), "call_id": call_id}),
        );
    }
    let input = arguments(payload);
    let what = ["cmd", "command", "path", "input"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(|text| text.chars().take(80).collect::<String>())
        .unwrap_or_default();
    let author = if name.contains("estelle") {
        "estelle"
    } else {
        "agent"
    };
    event(
        id,
        "tool_call",
        json!({"kind": author, "id": CODEX_AGENT}),
        at,
        json!({"call_id": call_id, "tool": name, "summary": cap(format!("Ran {name} {what}").trim_end()), "input": input}),
    )
}

/// The strings a Codex tool call names — what [`crate::hook_stream_git`] matches changed files against.
pub(crate) fn codex_mentions(record: &Value) -> Vec<String> {
    let Some(("item", payload)) = item_of(record) else {
        return Vec::new();
    };
    match s(payload, "type") {
        "function_call" => vec![s(payload, "arguments").to_string()],
        "custom_tool_call" => vec![s(payload, "input").to_string()],
        "local_shell_call" => vec![
            payload
                .get("action")
                .map(Value::to_string)
                .unwrap_or_default(),
        ],
        _ => Vec::new(),
    }
}

fn newest_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .take(MAX_DIR_ENTRIES)
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by(|a, b| b.cmp(a));
    dirs
}

/// The rollout Codex is writing for `session` under `<sessions>/YYYY/MM/DD/rollout-<time>-<session>.jsonl`,
/// searching the newest [`MAX_ROLLOUT_DAYS`] day directories only. `None` = not found within that bound,
/// which is "cannot answer", never "this session has no rollout".
pub(crate) fn find_rollout(sessions: &Path, session: &str) -> Option<PathBuf> {
    assert!(
        !session.contains('/'),
        "a session id is a file-name fragment, never a path"
    );
    let suffix = format!("-{session}.jsonl");
    let days = newest_dirs(sessions)
        .iter()
        .flat_map(|year| newest_dirs(year))
        .flat_map(|month| newest_dirs(&month))
        .take(MAX_ROLLOUT_DAYS)
        .collect::<Vec<_>>();
    days.iter().find_map(|day| {
        std::fs::read_dir(day)
            .ok()?
            .take(MAX_DIR_ENTRIES)
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix))
            })
    })
}

#[cfg(test)]
#[path = "hook_stream_codex_tests.rs"]
mod tests;
