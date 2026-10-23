use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;

/// A scripted `/session/events`: answers from `script` in order (then 200), refuses any NUL the way Postgres
/// does (503, the whole batch), and records every request it accepted or refused.
#[derive(Default)]
struct FakeServer {
    script: Mutex<VecDeque<Result<(), (u16, String)>>>,
    posts: Mutex<Vec<(String, Vec<Value>)>>,
    accepted: Mutex<Vec<(String, Vec<Value>)>>,
}

impl FakeServer {
    fn then(self, answer: Result<(), (u16, &str)>) -> Self {
        self.script
            .lock()
            .unwrap()
            .push_back(answer.map_err(|(s, m)| (s, m.to_string())));
        self
    }
    fn accepted(&self) -> Vec<(String, Vec<Value>)> {
        self.accepted.lock().unwrap().clone()
    }
    fn accepted_for(&self, session: &str) -> Vec<Value> {
        self.accepted()
            .into_iter()
            .filter(|(s, _)| s == session)
            .flat_map(|(_, e)| e)
            .collect()
    }
}

impl StreamSink for FakeServer {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        self.posts
            .lock()
            .unwrap()
            .push((session_id.to_string(), events.to_vec()));
        if events.iter().any(|e| e.to_string().contains("\\u0000")) {
            return Err(SinkError::Refused {
                status: 503,
                message: "the session timeline store is unavailable".into(),
            });
        }
        if let Some(Err((status, message))) = self.script.lock().unwrap().pop_front() {
            return Err(SinkError::Refused { status, message });
        }
        self.accepted
            .lock()
            .unwrap()
            .push((session_id.to_string(), events.to_vec()));
        Ok(())
    }
}

const SESSION: &str = "0b1c2d3e-aaaa-bbbb-cccc-1234567890ab";
const TIMELINE: &str = "claude-code-0b1c2d3e-aaaa-bbbb-cccc-1234567890ab";

struct Fixture {
    dir: tempfile::TempDir,
    transcript: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let transcript = dir.path().join(format!("{SESSION}.jsonl"));
        std::fs::write(&transcript, "").unwrap();
        Self { dir, transcript }
    }
    fn state_dir(&self) -> PathBuf {
        self.dir.path().join("state")
    }
    fn append(&self, entries: &[Value]) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.transcript)
            .unwrap();
        for entry in entries {
            writeln!(file, "{entry}").unwrap();
        }
    }
    async fn run(&self, hook_event: &str, sink: &FakeServer) -> StreamReport {
        let state_dir = self.state_dir();
        let input = StreamInput {
            state_dir: &state_dir,
            transcript: &self.transcript,
            session: SESSION,
            hook_event,
            repo: "uqeu/estelle",
            now: DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            dialect: Dialect::Claude,
            git_root: None,
        };
        run(&input, sink).await
    }
    fn offset(&self) -> u64 {
        let bytes = std::fs::read(self.state_dir().join(format!("{SESSION}.stream.json"))).unwrap();
        serde_json::from_slice::<Value>(&bytes).unwrap()["offset"]
            .as_u64()
            .unwrap()
    }
}

fn prompt(uuid: &str, at: &str, text: &str) -> Value {
    json!({"type": "user", "uuid": uuid, "timestamp": at, "message": {"content": text}})
}

fn tool_result(uuid: &str, at: &str, output: &str) -> Value {
    json!({"type": "user", "uuid": uuid, "timestamp": at, "message": {"content": [{"type": "tool_result", "tool_use_id": "c1", "content": output}]}})
}

#[tokio::test]
async fn one_nul_no_longer_blocks_the_stream_forever() {
    let fx = Fixture::new();
    fx.append(&[
        prompt("u1", "2026-10-21T10:00:00Z", "show me the png"),
        tool_result("r1", "2026-10-21T10:00:01Z", "\u{89}PNG\u{0}\u{0}IHDR"),
    ]);
    let server = FakeServer::default();
    let report = fx.run("PostToolUse", &server).await;
    assert_eq!(
        report.error, None,
        "the server rejects any NUL with 503, so this only passes if it was stripped"
    );
    let sent = server.accepted_for(TIMELINE);
    let output = sent.iter().find(|e| e["kind"] == "tool_result").unwrap()["payload"]["output"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(output, "\u{89}PNGIHDR");
    assert_eq!(
        fx.offset(),
        std::fs::metadata(&fx.transcript).unwrap().len(),
        "the offset moved past the line"
    );
}

#[tokio::test]
async fn the_offset_moves_only_after_the_server_took_the_events() {
    let fx = Fixture::new();
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "hello")]);
    let refusing = FakeServer::default().then(Err((503, "store down")));
    let report = fx.run("PostToolUse", &refusing).await;
    assert_eq!(report.error.as_deref(), Some("503: store down"));
    assert_eq!(fx.offset(), 0, "a refused batch is resent next run");
    let server = FakeServer::default();
    fx.run("PostToolUse", &server).await;
    let sent = server.accepted_for(TIMELINE);
    assert_eq!(
        sent[0]["event_id"],
        format!("{TIMELINE}:started"),
        "the start was not lost with the refused batch"
    );
    assert_eq!(sent[0]["payload"]["title"], "hello");
    assert!(fx.offset() > 0);
    // A run with nothing new sends nothing at all.
    let quiet = FakeServer::default();
    fx.run("PostToolUse", &quiet).await;
    assert!(quiet.posts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_refusal_naming_one_event_drops_that_event_and_delivers_the_rest() {
    let fx = Fixture::new();
    fx.append(&[
        prompt("u1", "2026-10-21T10:00:00Z", "a"),
        prompt("u2", "2026-10-21T10:00:01Z", "b"),
    ]);
    let server =
        FakeServer::default().then(Err((400, "event 1: customer_message needs a text string")));
    let report = fx.run("PostToolUse", &server).await;
    assert_eq!((report.error, report.dropped), (None, 1));
    let ids: Vec<String> = server
        .accepted_for(TIMELINE)
        .iter()
        .map(|e| e["event_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids.len(),
        4,
        "started + 2 messages + 2 inputs, minus the one refused: {ids:?}"
    );
    assert!(
        !ids.contains(&"u1:0".to_string()),
        "event 1 of the batch (u1's message, after `started`) was the one dropped"
    );
    assert!(fx.offset() > 0);
    // A 400 that names no event is not guessed at: nothing advances.
    let fx2 = Fixture::new();
    fx2.append(&[prompt("u1", "2026-10-21T10:00:00Z", "a")]);
    let bad = FakeServer::default().then(Err((400, "session_id must be 1-256 characters")));
    assert!(fx2.run("PostToolUse", &bad).await.error.is_some());
    assert_eq!(fx2.offset(), 0);
}

#[tokio::test]
async fn host_lifecycle_is_stamped_at_the_transcripts_last_entry_never_the_clock() {
    let fx = Fixture::new();
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "a"),
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-21T10:00:07Z", "message": {"content": [{"type": "text", "text": "done"}]}})]);
    let server = FakeServer::default();
    fx.run("Stop", &server).await;
    let sent = server.accepted_for(TIMELINE);
    let stop = sent.last().unwrap();
    assert_eq!(
        (
            stop["payload"]["state"].as_str(),
            stop["payload"]["reason"].as_str()
        ),
        (Some("waiting_for_review"), Some("input_required"))
    );
    assert_eq!(
        stop["at"], "2026-10-21T10:00:07.000Z",
        "the run's clock (2026-10-22) must not leak into `at`"
    );
    let first_input = sent
        .iter()
        .find(|e| e["payload"]["reason"] == "input_received")
        .unwrap();
    assert_eq!(
        first_input["at"], "2026-10-21T10:00:00Z",
        "a transcript event keeps its entry's time"
    );
    let end = FakeServer::default();
    fx.run("SessionEnd", &end).await;
    let ended = end.accepted_for(TIMELINE);
    assert_eq!(
        (
            ended.len(),
            ended[0]["payload"]["state"].as_str(),
            ended[0]["at"].as_str()
        ),
        (1, Some("ended"), Some("2026-10-21T10:00:07.000Z"))
    );
}

#[tokio::test]
async fn a_prompt_before_anything_streamed_does_not_claim_input_on_a_session_that_never_started() {
    let fx = Fixture::new();
    let server = FakeServer::default();
    fx.run("UserPromptSubmit", &server).await;
    assert!(server.posts.lock().unwrap().is_empty());
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "a")]);
    fx.run("PostToolUse", &server).await;
    fx.run("UserPromptSubmit", &server).await;
    let last = server.accepted_for(TIMELINE).pop().unwrap();
    assert_eq!(
        (last["payload"]["reason"].as_str(), last["at"].as_str()),
        (Some("input_received"), Some("2026-10-22T12:00:00.000Z")),
        "a prompt is stamped when the host fired it, not at the previous turn's last entry"
    );
}

#[test]
fn a_prompt_is_stamped_at_the_clock_and_never_before_the_last_entry() {
    let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    let (earlier, later) = (at("2026-10-21T10:00:00Z"), at("2026-10-22T12:00:00Z"));
    assert_eq!(
        prompt_stamp(Some(earlier), later),
        "2026-10-22T12:00:00.000Z"
    );
    assert_eq!(
        prompt_stamp(Some(later), earlier),
        "2026-10-22T12:00:00.000Z",
        "a slow clock is floored"
    );
    assert_eq!(prompt_stamp(None, earlier), "2026-10-21T10:00:00.000Z");
}

#[tokio::test]
async fn a_pre_040_offset_file_is_read_as_a_started_session() {
    let fx = Fixture::new();
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "old")]);
    let len = std::fs::metadata(&fx.transcript).unwrap().len();
    std::fs::create_dir_all(fx.state_dir()).unwrap();
    std::fs::write(
        fx.state_dir().join(format!("{SESSION}.offset")),
        len.to_string(),
    )
    .unwrap();
    fx.append(&[prompt("u2", "2026-10-21T10:01:00Z", "new")]);
    let server = FakeServer::default();
    fx.run("PostToolUse", &server).await;
    let ids: Vec<String> = server
        .accepted_for(TIMELINE)
        .iter()
        .map(|e| e["event_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["u2:0", "u2:input"],
        "no second `started`, no replay of what 0.3.x already sent"
    );
}

#[tokio::test]
async fn the_per_run_bound_cuts_on_a_line_and_the_next_run_continues() {
    let fx = Fixture::new();
    let lines: Vec<Value> = (0..150)
        .map(|i| prompt(&format!("u{i}"), "2026-10-21T10:00:00Z", "x"))
        .collect();
    fx.append(&lines);
    let server = FakeServer::default();
    fx.run("PostToolUse", &server).await;
    let first = server.accepted_for(TIMELINE).len();
    assert!(first <= MAX_EVENTS_PER_RUN + 2, "{first}");
    assert!(fx.offset() < std::fs::metadata(&fx.transcript).unwrap().len());
    fx.run("PostToolUse", &server).await;
    assert_eq!(
        server.accepted_for(TIMELINE).len(),
        1 + 150 * 2,
        "every event exactly once across the two runs"
    );
    assert_eq!(
        fx.offset(),
        std::fs::metadata(&fx.transcript).unwrap().len()
    );
}

#[test]
fn a_line_longer_than_the_window_is_skipped_once_and_a_partial_one_waits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.jsonl");
    let huge = format!(
        "{{\"x\":\"{}\"}}\n",
        "a".repeat(MAX_READ_BYTES as usize + 10)
    );
    std::fs::write(&path, format!("{huge}{{\"y\":1}}\n")).unwrap();
    let lines = read_lines(&path, 0, 10).unwrap();
    assert_eq!(
        (lines.entries.len(), lines.skipped, lines.consumed),
        (0, 1, huge.len() as u64)
    );
    let next = read_lines(&path, lines.consumed, 10).unwrap();
    assert_eq!(next.entries[0].0, Some(json!({"y": 1})));
    std::fs::write(&path, "{\"partial\":").unwrap();
    assert_eq!(read_lines(&path, 0, 10).unwrap().consumed, 0);
}

fn write_agent(fx: &Fixture, id: &str, call: &str, finished: bool, entries: &[Value]) {
    let dir = fx.transcript.with_extension("").join("subagents");
    std::fs::create_dir_all(&dir).unwrap();
    let body: String = entries.iter().map(|e| format!("{e}\n")).collect();
    std::fs::write(dir.join(format!("agent-{id}.jsonl")), body).unwrap();
    std::fs::write(
        dir.join(format!("agent-{id}.meta.json")),
        json!({"toolUseId": call, "description": "Find the bug", "finished": finished}).to_string(),
    )
    .unwrap();
}

fn sub_entries() -> Vec<Value> {
    vec![
        json!({"type": "user", "isSidechain": true, "uuid": "s0", "timestamp": "2026-10-21T10:00:01Z", "message": {"content": "Find the bug"}}),
        json!({"type": "assistant", "isSidechain": true, "uuid": "s1", "timestamp": "2026-10-21T10:00:02Z", "message": {"content": [
            {"type": "tool_use", "id": "g1", "name": "Grep", "input": {"pattern": "retry"}}]}}),
    ]
}

#[tokio::test]
async fn a_subagent_streams_into_its_own_timeline_and_ends_when_the_host_says_so() {
    let fx = Fixture::new();
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "go")]);
    write_agent(&fx, "abc", "toolu_01X", false, &sub_entries());
    let server = FakeServer::default();
    let report = fx.run("PostToolUse", &server).await;
    let child = format!("{TIMELINE}/agent-toolu_01X");
    let sent = server.accepted_for(&child);
    assert_eq!(report.subagent_events, sent.len());
    assert_eq!(sent[0]["event_id"], format!("{child}:started"));
    let p = &sent[0]["payload"];
    assert_eq!(
        (
            p["title"].as_str(),
            p["harness"].as_str(),
            p["parent"].as_str(),
            p["call_id"].as_str()
        ),
        (
            Some("Find the bug"),
            Some("subagent"),
            Some(TIMELINE),
            Some("toolu_01X")
        )
    );
    assert_eq!(sent[0]["at"], "2026-10-21T10:00:01Z");
    assert!(
        sent.iter().all(|e| e["payload"]["state"] != "ended"),
        "still calling tools, not finished"
    );
    // The host marks it finished: one `ended`, at its last reply, then nothing more is ever read.
    write_agent(&fx, "abc", "toolu_01X", true, &sub_entries());
    let later = FakeServer::default();
    fx.run("PostToolUse", &later).await;
    let ended = later.accepted_for(&child);
    assert_eq!(ended.len(), 1);
    assert_eq!(
        (
            ended[0]["payload"]["state"].as_str(),
            ended[0]["payload"]["reason"].as_str(),
            ended[0]["at"].as_str()
        ),
        (
            Some("ended"),
            Some("completed"),
            Some("2026-10-21T10:00:02Z")
        )
    );
    let after = FakeServer::default();
    fx.run("PostToolUse", &after).await;
    assert!(after.accepted_for(&child).is_empty());
}

#[tokio::test]
async fn a_failed_task_notification_in_the_parent_ends_the_subagent_as_failed() {
    let fx = Fixture::new();
    write_agent(&fx, "def", "toolu_02Y", false, &sub_entries());
    fx.append(&[json!({"type": "attachment", "uuid": "n1", "timestamp": "2026-10-21T10:00:09Z", "attachment": {"type": "queued_command",
        "prompt": "<task-notification><tool-use-id>toolu_02Y</tool-use-id><status>killed</status></task-notification>"}})]);
    let server = FakeServer::default();
    fx.run("PostToolUse", &server).await;
    let child = server.accepted_for(&format!("{TIMELINE}/agent-toolu_02Y"));
    assert_eq!(child.last().unwrap()["payload"]["reason"], "failed");
    assert_eq!(
        server.accepted_for(TIMELINE).last().unwrap()["payload"]["reason"],
        "control_returned"
    );
}

#[tokio::test]
async fn a_subagent_with_no_parent_call_or_an_unsafe_one_is_not_streamed() {
    let fx = Fixture::new();
    write_agent(&fx, "bad", "../../etc", false, &sub_entries());
    let server = FakeServer::default();
    assert_eq!(fx.run("PostToolUse", &server).await.subagent_events, 0);
    assert!(server.posts.lock().unwrap().is_empty());
}

#[test]
fn session_ids_are_validated_before_they_name_a_file() {
    assert!(safe_session(SESSION));
    assert!(!safe_session("../x") && !safe_session("") && !safe_session(&"a".repeat(201)));
}

/// The real client against a real HTTP server: the wire shape, and that a server's `400 event N:` reaches the
/// isolation path as a typed refusal rather than as prose.
async fn run_as(
    fx: &Fixture,
    hook_event: &str,
    dialect: Dialect,
    git_root: Option<&Path>,
    sink: &FakeServer,
) -> StreamReport {
    let state_dir = fx.state_dir();
    let input = StreamInput {
        state_dir: &state_dir,
        transcript: &fx.transcript,
        session: SESSION,
        hook_event,
        repo: "uqeu/estelle",
        now: DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        dialect,
        git_root,
    };
    run(&input, sink).await
}

fn rollout(payload_type: &str, at: &str, payload: Value) -> Value {
    json!({"timestamp": at, "type": payload_type, "payload": payload})
}

/// A Codex rollout streams into `codex-<session>`, starts with harness `codex`, and its Stop-door run
/// lands the whole turn; a second Stop with nothing new sends nothing (it would only repeat the waiting row).
#[tokio::test]
async fn a_codex_rollout_streams_into_its_own_timeline_from_the_stop_door() {
    let fx = Fixture::new();
    fx.append(&[
        rollout("session_meta", "2026-10-21T09:59:59Z", json!({"id": SESSION, "cwd": "/x"})),
        rollout("response_item", "2026-10-21T10:00:00Z", json!({"type": "message", "role": "user",
            "content": [{"type": "input_text", "text": "<environment_context>x</environment_context>"},
                        {"type": "input_text", "text": "run the tests"}]})),
        rollout("response_item", "2026-10-21T10:00:01Z", json!({"type": "function_call", "name": "exec_command",
            "call_id": "c1", "arguments": "{\"cmd\":\"cargo test\"}"})),
        rollout("response_item", "2026-10-21T10:00:05Z", json!({"type": "function_call_output", "call_id": "c1", "output": "ok"})),
        rollout("response_item", "2026-10-21T10:00:06Z", json!({"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": "All green."}]})),
        rollout("event_msg", "2026-10-21T10:00:07Z", json!({"type": "task_complete", "turn_id": "t1"})),
    ]);
    let server = FakeServer::default();
    let report = run_as(&fx, "Stop", Dialect::Codex, None, &server).await;
    assert_eq!(report.error, None);
    let timeline = format!("codex-{SESSION}");
    let sent = server.accepted_for(&timeline);
    assert!(
        server.accepted_for(TIMELINE).is_empty(),
        "nothing goes to a claude-code timeline"
    );
    let kinds: Vec<&str> = sent.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "lifecycle",
            "customer_message",
            "lifecycle",
            "tool_call",
            "tool_result",
            "agent_message",
            "lifecycle"
        ],
        "started, prompt, input, call, result, reply, then ONE turn end: task_complete and the Stop \
         both say waiting_for_review, and one turn end is one row (the pair was the 2026-10-23 bug)"
    );
    assert_eq!(sent[0]["event_id"], format!("{timeline}:started"));
    assert_eq!(sent[0]["payload"]["harness"], "codex");
    assert_eq!(sent[0]["payload"]["title"], "run the tests");
    let end = sent.last().unwrap();
    assert_eq!(end["payload"]["state"], "waiting_for_review");
    assert!(
        end["at"]
            .as_str()
            .unwrap()
            .starts_with("2026-10-21T10:00:07"),
        "the turn end is stamped at the last entry, never the clock: {end}"
    );
    assert_eq!(report.subagent_events, 0);
    assert_eq!(
        fx.offset(),
        std::fs::metadata(&fx.transcript).unwrap().len()
    );
    let again = FakeServer::default();
    run_as(&fx, "Stop", Dialect::Codex, None, &again).await;
    let resent = again.accepted_for(&timeline);
    assert!(
        resent.is_empty(),
        "a second Stop with nothing new would only repeat waiting_for_review: {resent:?}"
    );
}

/// One Codex Stop run catches up a turn bigger than the Claude per-run event bound.
#[tokio::test]
async fn a_codex_turn_larger_than_the_claude_bound_is_caught_up_in_one_stop() {
    let fx = Fixture::new();
    let entries: Vec<Value> = (0..MAX_EVENTS_PER_RUN + 50)
        .map(|i| {
            rollout(
                "response_item",
                "2026-10-21T10:00:00Z",
                json!({"type": "function_call_output", "call_id": format!("c{i}"), "output": "ok"}),
            )
        })
        .collect();
    fx.append(&entries);
    let server = FakeServer::default();
    let report = run_as(&fx, "Stop", Dialect::Codex, None, &server).await;
    assert_eq!(report.error, None);
    assert_eq!(
        fx.offset(),
        std::fs::metadata(&fx.transcript).unwrap().len()
    );
    let results = server
        .accepted_for(&format!("codex-{SESSION}"))
        .iter()
        .filter(|e| e["kind"] == "tool_result")
        .count();
    assert_eq!(results, MAX_EVENTS_PER_RUN + 50);
}

/// The git pass rides the same timeline and the same send; a refused git batch is resent next run.
#[tokio::test]
async fn git_events_reach_the_session_timeline_and_are_resent_after_a_refusal() {
    let fx = Fixture::new();
    let repo = fx.dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "T"],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(repo.join("notes.md"), "hello\n").unwrap();
    fx.append(&[
        prompt("u1", "2026-10-21T10:00:00Z", "write notes"),
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-21T10:00:01Z", "message": {"content": [
            {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "echo hello > notes.md"}}]}}),
    ]);
    // The transcript lands; the git batch (second POST) is refused once.
    let refusing = FakeServer::default()
        .then(Ok(()))
        .then(Err((503, "store down")));
    let report = run_as(&fx, "Stop", Dialect::Claude, Some(&repo), &refusing).await;
    assert_eq!(report.error.as_deref(), Some("503: store down"));
    assert_eq!(report.git_events, 0);
    let server = FakeServer::default();
    let report = run_as(&fx, "Stop", Dialect::Claude, Some(&repo), &server).await;
    assert_eq!(report.error, None);
    let sent = server.accepted_for(TIMELINE);
    let diff = sent
        .iter()
        .find(|e| e["kind"] == "workspace_diff")
        .expect("the refused diff is resent");
    assert_eq!(diff["payload"]["path"], "notes.md");
    assert_eq!(diff["payload"]["status"], "added");
    assert!(diff["at"].as_str().unwrap() <= "2026-10-21T10:00:01.000Z");
    assert!(report.git_events >= 1);
}

#[tokio::test]
async fn the_client_sink_speaks_the_server_contract() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("POST"))
        .and(path("/session/events"))
        .respond_with(
            wiremock::ResponseTemplate::new(400)
                .set_body_json(json!({"error": {"message": "event 0: bad"}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    wiremock::Mock::given(method("POST"))
        .and(path("/session/events"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({"receipts": []})))
        .mount(&server)
        .await;
    let key = estelle_client::ApiKey::new("test-key").unwrap();
    let client = Client::new(
        &format!("{}/", server.uri()),
        key,
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let sink = ClientSink::new(client, CancellationToken::new());
    let dropped = send(
        &sink,
        "claude-code-s",
        vec![
            json!({"event_id": "a", "x": "1\u{0}"}),
            json!({"event_id": "b"}),
        ],
    )
    .await;
    assert_eq!(dropped, Ok(1));
    let requests = server.received_requests().await.unwrap();
    let last: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(
        last,
        json!({"session_id": "claude-code-s", "events": [{"event_id": "b"}]})
    );
    let first = String::from_utf8(requests[0].body.clone()).unwrap();
    assert!(
        !first.contains("\\u0000"),
        "scrubbed before the wire: {first}"
    );
}

/// SMOKE, against a running local API. Ignored by default. Run with
/// `ESTELLE_STREAM_SMOKE_URL=http://localhost:8787 ESTELLE_STREAM_SMOKE_KEY_FILE=<path> ESTELLE_STREAM_SMOKE_TRANSCRIPT=<jsonl>
/// cargo test -p estelle-tui --bin estelle smoke_stream -- --ignored --nocapture`. The key is read from the file and
/// never printed; the session id is the transcript's own name.
#[tokio::test]
#[ignore = "needs a running Estelle API"]
#[allow(clippy::print_stdout)]
async fn smoke_stream_against_a_local_api() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} is required"));
    let key = std::fs::read_to_string(var("ESTELLE_STREAM_SMOKE_KEY_FILE")).unwrap();
    let client = Client::new(
        &format!("{}/", var("ESTELLE_STREAM_SMOKE_URL").trim_end_matches('/')),
        estelle_client::ApiKey::new(key.trim().to_string()).unwrap(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let transcript = PathBuf::from(var("ESTELLE_STREAM_SMOKE_TRANSCRIPT"));
    let session = transcript
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let state = tempfile::tempdir().unwrap();
    let sink = ClientSink::new(client, CancellationToken::new());
    let mut total = (0, 0);
    for _ in 0..5 {
        let input = StreamInput {
            state_dir: state.path(),
            transcript: &transcript,
            session: &session,
            hook_event: "Stop",
            repo: "uqeu/estelle",
            now: Utc::now(),
            dialect: Dialect::Claude,
            git_root: None,
        };
        let report = run(&input, &sink).await;
        println!(
            "run: sent={} dropped={} subagent_events={} error={:?}",
            report.sent, report.dropped, report.subagent_events, report.error
        );
        assert_eq!(report.error, None);
        total = (total.0 + report.sent, total.1 + report.subagent_events);
    }
    println!("total sent={} subagent_events={}", total.0, total.1);
}

fn turn_duration(uuid: &str, at: &str) -> Value {
    json!({"type": "system", "subtype": "turn_duration", "uuid": uuid, "timestamp": at, "durationMs": 7000})
}

#[tokio::test]
async fn one_turn_end_shows_one_waiting_even_when_the_transcript_reports_it_after_stop() {
    let fx = Fixture::new();
    fx.append(&[prompt("u1", "2026-10-21T10:00:00Z", "a"),
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-21T10:00:07Z", "message": {"content": [{"type": "text", "text": "done"}]}})]);
    let server = FakeServer::default();
    fx.run("Stop", &server).await;
    // Claude Code writes `system/turn_duration` AFTER the Stop hook ran; the next run reads it.
    fx.append(&[turn_duration("t1", "2026-10-21T10:00:07.500Z")]);
    fx.run("PostToolUse", &server).await;
    let waiting = server
        .accepted_for(TIMELINE)
        .iter()
        .filter(|e| e["kind"] == "lifecycle" && e["payload"]["state"] == "waiting_for_review")
        .count();
    assert_eq!(
        waiting, 1,
        "one turn end is one waiting row, not one per source"
    );
    // and the NEXT turn's end is reported again: the dedupe is on repeats, not on the state forever.
    fx.append(&[
        prompt("u2", "2026-10-21T10:01:00Z", "b"),
        turn_duration("t2", "2026-10-21T10:01:09Z"),
    ]);
    fx.run("Stop", &server).await;
    let waiting = server
        .accepted_for(TIMELINE)
        .iter()
        .filter(|e| e["kind"] == "lifecycle" && e["payload"]["state"] == "waiting_for_review")
        .count();
    assert_eq!(
        waiting, 2,
        "a second turn end after new input is a second waiting row"
    );
}

#[test]
fn collapse_lifecycle_drops_only_a_repeated_waiting_and_carries_the_last_across_runs() {
    let ev = |s: &str, r: &str| json!({"kind": "lifecycle", "payload": {"state": s, "reason": r}});
    let msg = json!({"kind": "customer_message", "payload": {}});
    let (kept, last) = collapse_lifecycle(
        vec![
            ev("running", "started"),
            ev("running", "input_received"),
            msg.clone(),
            ev("waiting_for_review", "input_required"),
            ev("waiting_for_review", "input_required"),
        ],
        None,
    );
    assert_eq!(
        kept.len(),
        4,
        "start and input differ in reason, so both stay; the repeat waiting goes"
    );
    let (kept, _) = collapse_lifecycle(
        vec![
            ev("running", "input_received"),
            ev("running", "input_received"),
        ],
        None,
    );
    assert_eq!(
        kept.len(),
        2,
        "only a repeated WAITING is a duplicate; two inputs in a row are two rows"
    );
    assert_eq!(last.as_deref(), Some("waiting_for_review/input_required"));
    let (kept, _) = collapse_lifecycle(
        vec![ev("waiting_for_review", "input_required")],
        last.clone(),
    );
    assert!(
        kept.is_empty(),
        "a repeat of what an EARLIER run landed is dropped too"
    );
    let (kept, _) = collapse_lifecycle(vec![msg], last);
    assert_eq!(kept.len(), 1, "non-lifecycle events are never touched");
}
