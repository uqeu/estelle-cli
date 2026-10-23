//! Subagents, each mirrored as its OWN timeline: `<parent timeline>/agent-<toolUseId>`.
//!
//! Claude Code writes every subagent's transcript beside the session — `<session>/subagents/agent-<id>.jsonl`
//! plus `agent-<id>.meta.json` naming the parent's Agent tool call (`toolUseId`) and the task's
//! `description`. Keying the child by `toolUseId` is what lets the dashboard open it from the Agent call it
//! already draws in the parent, and keeps a subagent's hundreds of steps out of the parent's conversation.
//!
//! Lifecycle: `started` on its first event; `ended` when the HOST says so — the meta's `finished: true`, the
//! subagent's own `SubagentHandback`, a `<task-notification>` for its call (`failed`/`killed` → `failed`) —
//! or when its last reply took no tool and the file has been quiet for [`SUB_QUIET_S`].
//!
//! Every bound is declared here: subagents per run, idle age, lines and events per run, notified ids kept.

use crate::hook_stream::{HOOK_AUTHOR, Notes, event, events_of, redact};
use crate::hook_stream_run::{StreamInput, StreamSink, read_lines, save_json, send};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Most recently written subagents looked at per run.
pub(crate) const MAX_SUBAGENTS: usize = 40;
/// A subagent never seen before and untouched for longer than this is not started now.
const SUB_ACTIVE: Duration = Duration::from_secs(24 * 3600);
/// Transcript lines and events across ALL subagents per run, so the parent's own stream keeps moving.
const MAX_SUB_LINES_PER_RUN: usize = 3000;
const MAX_SUB_EVENTS_PER_RUN: usize = 400;
/// An assistant reply with no tool call, then this long with nothing written: finished.
pub(crate) const SUB_QUIET_S: u64 = 120;
/// Task-notification statuses remembered across runs (one per background call).
const MAX_NOTIFIED: usize = 500;
/// Longest Agent call id accepted into a session id and an event id.
const MAX_CALL_ID: usize = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AgentState {
    offset: u64,
    started: bool,
    done: bool,
    last_at: Option<String>,
    /// The last reply called a tool (other than the handback): the agent is still working.
    #[serde(default = "yes")]
    open_tool: bool,
    #[serde(default)]
    handback: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SubState {
    #[serde(default)]
    agents: BTreeMap<String, AgentState>,
    #[serde(default)]
    notified: BTreeMap<String, String>,
}

/// What the subagent pass did.
#[derive(Debug, Default)]
pub(crate) struct SubReport {
    pub(crate) events: usize,
    pub(crate) error: Option<String>,
}

fn safe_call_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_CALL_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `<transcript minus .jsonl>/subagents`, where Claude Code writes them.
pub(crate) fn subagent_dir(transcript: &Path) -> PathBuf {
    transcript.with_extension("").join("subagents")
}

fn meta_of(agent_file: &Path) -> Option<Value> {
    let bytes = std::fs::read(agent_file.with_extension("meta.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// The newest [`MAX_SUBAGENTS`] `agent-*.jsonl` files, newest first.
fn agent_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with("agent-") && name.ends_with(".jsonl")
        })
        .map(|p| (mtime(&p), p))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files
        .into_iter()
        .take(MAX_SUBAGENTS)
        .map(|(_, p)| p)
        .collect()
}

fn child_started(
    child: &str,
    at: &str,
    meta: &Value,
    parent: &str,
    repo: &str,
    call_id: &str,
) -> Value {
    let title: String = meta
        .get("description")
        .and_then(Value::as_str)
        .filter(|d| !d.trim().is_empty())
        .unwrap_or("Subagent")
        .chars()
        .take(90)
        .collect();
    event(
        format!("{child}:started"),
        "lifecycle",
        json!({"kind": "system", "id": HOOK_AUTHOR}),
        at,
        json!({
            "state": "running", "reason": "started", "title": redact(&title), "repo": repo, "origin": "claude-code",
            "harness": "subagent", "parent": parent, "call_id": call_id,
        }),
    )
}

/// Mirror every subagent's new lines. Runs after the parent; `notified` is what the parent pass just learned.
pub(crate) async fn sync<S: StreamSink>(
    input: &StreamInput<'_>,
    parent: &str,
    notified: &[(String, String)],
    sink: &S,
) -> SubReport {
    let mut report = SubReport::default();
    let dir = subagent_dir(input.transcript);
    if !dir.is_dir() {
        return report;
    }
    let state_file = input
        .state_dir
        .join(format!("{}.subagents.json", input.session));
    let mut state: SubState = std::fs::read(&state_file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    for (id, status) in notified {
        if state.notified.len() < MAX_NOTIFIED || state.notified.contains_key(id) {
            state.notified.insert(id.clone(), status.clone());
        }
    }
    let now = SystemTime::from(input.now);
    let (mut lines_left, mut events_left) = (MAX_SUB_LINES_PER_RUN, MAX_SUB_EVENTS_PER_RUN);
    for file in agent_files(&dir) {
        if lines_left == 0 || events_left == 0 {
            break;
        }
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let known = state.agents.get(&name).cloned();
        let age = now.duration_since(mtime(&file)).unwrap_or_default();
        if known.as_ref().is_some_and(|a| a.done) || (known.is_none() && age > SUB_ACTIVE) {
            continue;
        }
        let Some(meta) = meta_of(&file) else { continue };
        let Some(call_id) = meta
            .get("toolUseId")
            .and_then(Value::as_str)
            .filter(|id| safe_call_id(id))
            .map(str::to_string)
        else {
            continue; // no parent call recorded: nothing in the parent conversation to hang it on
        };
        let agent = Agent {
            file: &file,
            meta: &meta,
            call_id: &call_id,
            parent,
            repo: input.repo,
            quiet: age.as_secs() >= SUB_QUIET_S,
        };
        let mut st = known.unwrap_or_else(|| AgentState {
            open_tool: true,
            ..AgentState::default()
        });
        let (events, lines) = match agent.step(&mut st, &state.notified, lines_left) {
            Ok(step) => step,
            Err(error) => {
                report.error = Some(error);
                continue;
            }
        };
        lines_left = lines_left.saturating_sub(lines);
        let count = events.len();
        if count > 0
            && let Err(error) = send(sink, &format!("{parent}/agent-{call_id}"), events).await
        {
            report.error = Some(error);
            break; // the server is refusing: keep this agent's old offset and stop for this run
        }
        events_left = events_left.saturating_sub(count);
        report.events += count;
        state.agents.insert(name, st); // only after the send landed
    }
    save_json(&state_file, &state);
    report
}

struct Agent<'a> {
    file: &'a Path,
    meta: &'a Value,
    call_id: &'a str,
    parent: &'a str,
    repo: &'a str,
    quiet: bool,
}

impl Agent<'_> {
    /// Read this agent's new whole lines into events, advancing `st` (the caller keeps it only if the send lands).
    fn step(
        &self,
        st: &mut AgentState,
        notified: &BTreeMap<String, String>,
        max_lines: usize,
    ) -> Result<(Vec<Value>, usize), String> {
        let child = format!("{}/agent-{}", self.parent, self.call_id);
        let lines = read_lines(self.file, st.offset, max_lines.max(1))
            .map_err(|_| "a subagent transcript is unreadable".to_string())?;
        let mut out = Vec::new();
        let mut notes = Notes::default();
        for (entry, _) in &lines.entries {
            let Some(entry) = entry else { continue };
            let evs = events_of(entry, &mut notes, true);
            if !st.started
                && let Some(at) = evs.first().and_then(|e| e["at"].as_str())
            {
                out.push(child_started(
                    &child,
                    at,
                    self.meta,
                    self.parent,
                    self.repo,
                    self.call_id,
                ));
                st.started = true;
            }
            out.extend(evs);
            note_reply(st, entry);
        }
        st.offset += lines.consumed;
        let status = notified.get(self.call_id);
        let finished = self.meta.get("finished").and_then(Value::as_bool) == Some(true);
        let done = finished || st.handback || status.is_some() || (self.quiet && !st.open_tool);
        if let (true, true, Some(last_at)) = (st.started, done, st.last_at.clone()) {
            let reason = if matches!(status.map(String::as_str), Some("failed" | "killed")) {
                "failed"
            } else {
                "completed"
            };
            out.push(event(
                format!("{child}:ended"),
                "lifecycle",
                json!({"kind": "system", "id": HOOK_AUTHOR}),
                &last_at,
                json!({"state": "ended", "reason": reason}),
            ));
            st.done = true;
        }
        Ok((out, lines.entries.len() + lines.skipped))
    }
}

/// Track the last assistant reply: when it was, whether it called a tool, whether it handed back.
fn note_reply(st: &mut AgentState, entry: &Value) {
    let Some(at) = entry
        .get("timestamp")
        .and_then(Value::as_str)
        .filter(|_| entry.get("type").and_then(Value::as_str) == Some("assistant"))
    else {
        return;
    };
    st.last_at = Some(at.to_string());
    let names: Vec<&str> = entry
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                .filter_map(|b| b.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let handed_back = names.contains(&"SubagentHandback");
    st.open_tool = !names.is_empty() && !handed_back;
    st.handback |= handed_back;
}
