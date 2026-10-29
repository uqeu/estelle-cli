use super::*;
use serde_json::json;

const AT: &str = "2026-10-21T10:00:00.000Z";

fn item(payload: Value) -> Value {
    json!({"timestamp": AT, "type": "response_item", "payload": payload})
}

fn map(record: Value) -> Vec<Value> {
    codex_events_of(&record, 4096, &mut Notes::default())
}

#[test]
fn a_typed_prompt_is_the_customer_and_the_harness_injections_are_not() {
    let mut notes = Notes::default();
    let record = item(json!({"type": "message", "role": "user", "content": [
        {"type": "input_text", "text": "<environment_context>\n<cwd>/x</cwd>\n</environment_context>"},
        {"type": "input_text", "text": "# AGENTS.md instructions for /x\nbe terse"},
        {"type": "input_text", "text": "fix the flaky test\nsecond line"},
    ]}));
    let out = codex_events_of(&record, 77, &mut notes);
    let kinds: Vec<&str> = out.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["customer_message", "lifecycle"]);
    assert_eq!(out[0]["event_id"], "r77:2");
    assert_eq!(out[0]["at"], AT);
    assert_eq!(
        out[1]["payload"],
        json!({"state": "running", "reason": "input_received"})
    );
    assert_eq!(notes.title.as_deref(), Some("fix the flaky test"));
}

#[test]
fn an_injection_only_user_record_emits_nothing_not_even_input_received() {
    let out = map(item(json!({"type": "message", "role": "user", "content": [
        {"type": "input_text", "text": "<environment_context>x</environment_context>"}]})));
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn the_assistant_is_codex_and_developer_messages_are_skipped() {
    let out = map(item(
        json!({"type": "message", "role": "assistant", "content": [
        {"type": "output_text", "text": "Done."}]}),
    ));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["kind"], "agent_message");
    assert_eq!(out[0]["author"], json!({"kind": "agent", "id": "codex"}));
    assert!(
        map(item(
            json!({"type": "message", "role": "developer", "content": [
        {"type": "input_text", "text": "you are codex"}]})
        ))
        .is_empty()
    );
}

#[test]
fn event_msg_echoes_of_messages_are_not_read_twice() {
    for payload in [
        json!({"type": "user_message", "message": "fix it"}),
        json!({"type": "agent_message", "message": "ok"}),
    ] {
        let out = map(json!({"timestamp": AT, "type": "event_msg", "payload": payload}));
        assert!(out.is_empty(), "{out:?}");
    }
}

#[test]
fn a_function_call_and_its_output_pair_by_call_id_with_parsed_arguments() {
    let call = map(item(
        json!({"type": "function_call", "name": "exec_command", "namespace": "",
        "call_id": "call_1", "arguments": "{\"cmd\":\"cargo test -p estelle-tui\",\"token\":\"ghp_abcdefghijklmnopqrstuvwxyz0123\"}"}),
    ));
    assert_eq!(call[0]["kind"], "tool_call");
    assert_eq!(call[0]["payload"]["tool"], "exec_command");
    assert_eq!(call[0]["payload"]["call_id"], "call_1");
    assert_eq!(
        call[0]["payload"]["input"]["cmd"],
        "cargo test -p estelle-tui"
    );
    assert_eq!(
        call[0]["payload"]["input"]["token"],
        "[redacted github token]"
    );
    assert_eq!(
        call[0]["payload"]["summary"],
        "Ran exec_command cargo test -p estelle-tui"
    );
    let result = map(item(
        json!({"type": "function_call_output", "call_id": "call_1", "output": "ok 3 passed"}),
    ));
    assert_eq!(result[0]["kind"], "tool_result");
    assert_eq!(result[0]["payload"]["call_id"], "call_1");
    assert_eq!(result[0]["payload"]["output"], "ok 3 passed");
    assert!(
        result[0]["payload"]["ok"].is_null(),
        "the rollout records no success flag; none is invented"
    );
}

#[test]
fn a_namespaced_call_and_a_custom_tool_output_list_are_read() {
    let call = map(item(
        json!({"type": "function_call", "name": "send_message", "namespace": "collaboration",
        "call_id": "c2", "arguments": "not json"}),
    ));
    assert_eq!(call[0]["payload"]["tool"], "collaboration.send_message");
    assert_eq!(call[0]["payload"]["input"]["arguments"], "not json");
    let out = map(item(
        json!({"type": "custom_tool_call_output", "call_id": "c3",
        "output": [{"type": "input_text", "text": "line one"}, {"type": "input_text", "text": "line two"}]}),
    ));
    assert_eq!(out[0]["payload"]["output"], "line one\nline two");
}

#[test]
fn apply_patch_is_an_edit_with_the_patched_path() {
    let patch = "*** Begin Patch\n*** Update File: tui/src/lib.rs\n@@\n-old\n+new\n*** End Patch";
    let out = map(item(
        json!({"type": "custom_tool_call", "name": "apply_patch", "call_id": "p1", "input": patch}),
    ));
    assert_eq!(out[0]["kind"], "edit");
    assert_eq!(out[0]["payload"]["path"], "tui/src/lib.rs");
    assert!(out[0]["payload"]["diff"].as_str().unwrap().contains("+new"));
}

#[test]
fn readable_reasoning_is_thinking_and_encrypted_only_reasoning_is_nothing() {
    let out = map(item(
        json!({"type": "reasoning", "summary": [{"type": "summary_text", "text": "check the offset"}], "encrypted_content": "gAAAA"}),
    ));
    assert_eq!(out[0]["kind"], "thinking");
    assert_eq!(out[0]["payload"]["text"], "check the offset");
    assert!(
        map(item(
            json!({"type": "reasoning", "summary": [], "encrypted_content": "gAAAA"})
        ))
        .is_empty()
    );
}

#[test]
fn a_finished_or_aborted_turn_waits_for_the_person() {
    for kind in ["task_complete", "turn_aborted"] {
        let out = map(
            json!({"timestamp": AT, "type": "event_msg", "payload": {"type": kind, "turn_id": "t"}}),
        );
        assert_eq!(out[0]["event_id"], "r4096:turn-end");
        assert_eq!(
            out[0]["payload"],
            json!({"state": "waiting_for_review", "reason": "input_required"})
        );
    }
    let started =
        map(json!({"timestamp": AT, "type": "event_msg", "payload": {"type": "task_started"}}));
    assert!(started.is_empty());
}

#[test]
fn a_line_without_a_timestamp_is_skipped_never_stamped_with_the_clock() {
    let mut record = item(
        json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "hi"}]}),
    );
    record.as_object_mut().unwrap().remove("timestamp");
    assert!(map(record).is_empty());
}

#[test]
fn the_2025_legacy_bare_item_is_read_through_the_same_table() {
    let out = map(
        json!({"timestamp": AT, "type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "legacy"}]}),
    );
    assert_eq!(out[0]["payload"]["text"], "legacy");
}

#[test]
fn the_same_line_maps_to_the_same_ids_so_a_replay_is_a_no_op() {
    let record = item(
        json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "again"}]}),
    );
    assert_eq!(map(record.clone()), map(record));
}

#[test]
fn mentions_come_from_tool_arguments_only() {
    let call = item(
        json!({"type": "function_call", "name": "exec_command", "arguments": "{\"cmd\":\"sed -i s/a/b/ src/x.rs\"}"}),
    );
    assert_eq!(
        codex_mentions(&call),
        vec!["{\"cmd\":\"sed -i s/a/b/ src/x.rs\"}".to_string()]
    );
    let said = item(
        json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "src/x.rs"}]}),
    );
    assert!(codex_mentions(&said).is_empty());
}

#[test]
fn find_rollout_finds_the_session_by_id_in_the_newest_days() {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("2026/10/21");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::create_dir_all(dir.path().join("2026/10/20")).unwrap();
    let want = day.join("rollout-2026-10-21T02-43-14-01a15d33-2257-7392-8ad4-7fdc1a52900a.jsonl");
    std::fs::write(&want, "").unwrap();
    std::fs::write(day.join("rollout-2026-10-21T01-00-00-other.jsonl"), "").unwrap();
    assert_eq!(
        find_rollout(dir.path(), "01a15d33-2257-7392-8ad4-7fdc1a52900a"),
        Some(want)
    );
    assert_eq!(find_rollout(dir.path(), "nope"), None);
}

/// Maps a REAL rollout and prints only the count of each event kind (never its text):
/// `ESTELLE_CODEX_ROLLOUT=<rollout.jsonl> cargo test -p estelle-tui --bin estelle real_rollout -- --ignored --nocapture`
#[test]
#[ignore = "reads a local Codex rollout named by ESTELLE_CODEX_ROLLOUT"]
#[allow(clippy::print_stdout)] // a manual measurement: its output IS the result
fn real_rollout_kind_counts() {
    let Some(path) = std::env::var_os("ESTELLE_CODEX_ROLLOUT") else {
        return;
    };
    let text = std::fs::read(path).expect("rollout readable");
    let (mut offset, mut notes) = (0u64, Notes::default());
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut ids = std::collections::BTreeSet::new();
    for line in text.split_inclusive(|b| *b == b'\n') {
        if let Ok(record) = serde_json::from_slice::<Value>(line) {
            for e in codex_events_of(&record, offset, &mut notes) {
                assert!(
                    ids.insert(e["event_id"].to_string()),
                    "duplicate id {}",
                    e["event_id"]
                );
                *counts
                    .entry(e["kind"].as_str().unwrap().to_string())
                    .or_default() += 1;
            }
        }
        offset += line.len() as u64;
    }
    println!("kinds: {counts:?}; titled: {}", notes.title.is_some());
}

/// 🔴 THE PAIR THAT MUST DIFFER: a never-streamed Codex session is due on its FIRST tool call (the fix:
/// it used to wait for a Stop that a long or aborted turn never sends), and the same session streamed a
/// moment ago is NOT due on its next tool call (the bound on a synchronous door). With no throttle both
/// would be `true`; with an inverted one the first would fail.
#[test]
fn a_codex_tool_call_streams_a_new_session_at_once_and_then_at_most_once_per_interval() {
    let just_now = std::time::Duration::from_secs(5);
    assert!(codex_stream_due("PostToolUse", None));
    assert!(!codex_stream_due("PostToolUse", Some(just_now)));
    assert!(codex_stream_due("PostToolUse", Some(CODEX_MIDTURN_EVERY)));
}

/// Once-per-prompt and once-per-turn doors are never throttled: a prompt right after a tool call still
/// streams, so a turn that never reached Stop is caught up by the next prompt.
#[test]
fn the_prompt_and_stop_doors_are_never_throttled() {
    let just_now = Some(std::time::Duration::from_secs(1));
    for event in ["UserPromptSubmit", "Stop", "SessionEnd", ""] {
        assert!(codex_stream_due(event, just_now), "{event:?} was throttled");
    }
}

#[test]
fn a_state_file_age_is_read_and_an_absent_file_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::hook_stream_run::state_file_under(dir.path(), "s");
    assert_eq!(since_written(&state), None);
    std::fs::write(&state, b"{}").unwrap();
    let age = since_written(&state).expect("a written file has an age");
    assert!(
        age < CODEX_MIDTURN_EVERY,
        "a file written now is inside the interval: {age:?}"
    );
}
