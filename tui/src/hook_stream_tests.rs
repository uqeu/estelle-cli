use super::*;

const AT: &str = "2026-10-21T10:00:00Z";

fn user(uuid: &str, content: Value) -> Value {
    json!({"type": "user", "uuid": uuid, "timestamp": AT, "message": {"content": content}})
}

fn run(entry: &Value) -> Vec<Value> {
    events_of(entry, &mut Notes::default(), false)
}

#[test]
fn a_typed_prompt_is_the_customer_and_starts_work_and_an_injection_is_neither() {
    let mut notes = Notes::default();
    let out = events_of(
        &user("u1", json!("fix the bug\nsecond line")),
        &mut notes,
        false,
    );
    let kinds: Vec<_> = out
        .iter()
        .map(|e| (e["kind"].as_str().unwrap(), e["payload"]["state"].as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![("customer_message", None), ("lifecycle", Some("running"))]
    );
    assert_eq!(out[1]["payload"]["reason"], "input_received");
    assert_eq!(out[1]["event_id"], "u1:input");
    assert_eq!(
        out[1]["at"], AT,
        "the lifecycle carries the ENTRY's time, not the hook's clock"
    );
    assert_eq!(notes.title.as_deref(), Some("fix the bug"));
    assert!(run(&user("u2", json!("<system-reminder>x"))).is_empty());
}

#[test]
fn a_turn_end_is_the_host_waiting_on_the_person_but_never_inside_a_subagent() {
    let end = json!({"type": "system", "subtype": "turn_duration", "uuid": "t1", "timestamp": AT});
    let out = run(&end);
    assert_eq!(out.len(), 1);
    assert_eq!(
        (
            out[0]["event_id"].as_str(),
            out[0]["payload"]["state"].as_str(),
            out[0]["payload"]["reason"].as_str()
        ),
        (
            Some("t1:turn-end"),
            Some("waiting_for_review"),
            Some("input_required")
        )
    );
    assert!(events_of(&end, &mut Notes::default(), true).is_empty());
    let mut sidechain = end;
    sidechain["isSidechain"] = json!(true);
    assert!(run(&sidechain).is_empty());
    // A different system subtype says nothing about the turn.
    assert!(run(&json!({"type": "system", "subtype": "stop_hook_summary", "uuid": "t2", "timestamp": AT})).is_empty());
}

#[test]
fn a_task_notification_returns_control_and_records_the_status() {
    let note = json!({"type": "attachment", "uuid": "n1", "timestamp": AT, "attachment": {"type": "queued_command",
        "prompt": "<task-notification>\n<task-id>b1</task-id>\n<tool-use-id>toolu_01ABC</tool-use-id>\n<status>failed</status>\n</task-notification>"}});
    let mut notes = Notes::default();
    let out = events_of(&note, &mut notes, false);
    assert_eq!(
        (out.len(), out[0]["payload"]["reason"].as_str()),
        (1, Some("control_returned"))
    );
    assert_eq!(out[0]["payload"]["state"], "running");
    assert_eq!(
        notes.notified,
        vec![("toolu_01ABC".to_string(), "failed".to_string())]
    );
    let mut child_notes = Notes::default();
    assert!(
        events_of(&note, &mut child_notes, true).is_empty(),
        "control returns to the PARENT, not a subagent"
    );
    assert_eq!(
        child_notes.notified.len(),
        1,
        "but the status is still learned"
    );
}

#[test]
fn a_message_queued_mid_turn_is_the_customer_and_input() {
    let queued = json!({"type": "attachment", "uuid": "q1", "timestamp": AT, "attachment": {"type": "queued_command",
        "prompt": [{"type": "text", "text": "also update the docs"}, {"type": "image"}]}});
    let out = run(&queued);
    assert_eq!(out[0]["kind"], "customer_message");
    assert_eq!(out[0]["payload"]["text"], "also update the docs");
    assert_eq!(
        (
            out[1]["event_id"].as_str(),
            out[1]["payload"]["reason"].as_str()
        ),
        (Some("q1:input"), Some("input_received"))
    );
}

fn context(hook: &str, content: Value) -> Value {
    json!({"type": "attachment", "uuid": "c1", "timestamp": AT, "attachment": {"type": "hook_additional_context",
        "hookEvent": hook, "hookName": hook, "content": content}})
}

#[test]
fn estelle_context_on_a_prompt_is_one_event_with_only_estelles_parts() {
    let entry = context(
        "UserPromptSubmit",
        json!([
            "another plugin's note",
            "ESTELLE recall: retry lives in backend.py sk_live_abcdefghijkl",
            "  "
        ]),
    );
    let out = run(&entry);
    assert_eq!(out.len(), 1);
    let e = &out[0];
    assert_eq!(
        (e["event_id"].as_str(), e["kind"].as_str()),
        (Some("c1:estelle-context"), Some("tool_result"))
    );
    assert_eq!(
        e["author"],
        json!({"kind": "estelle", "id": "estelle-hook"})
    );
    assert_eq!(e["at"], AT);
    let p = &e["payload"];
    assert_eq!(
        (
            p["source"].as_str(),
            p["hook"].as_str(),
            p["parts"].as_u64()
        ),
        (Some("estelle-hook"), Some("UserPromptSubmit"), Some(1))
    );
    let output = p["output"].as_str().unwrap();
    assert!(
        output.starts_with("ESTELLE recall")
            && !output.contains("another plugin")
            && !output.contains("abcdefghijkl"),
        "{output}"
    );
    assert_eq!(
        p["chars"].as_u64(),
        Some(
            "ESTELLE recall: retry lives in backend.py sk_live_abcdefghijkl"
                .chars()
                .count() as u64
        )
    );
    assert!(
        p.get("call_id").is_none(),
        "it is not the answer to any tool call"
    );
}

#[test]
fn estelle_context_ignores_other_hooks_other_plugins_and_subagents() {
    assert!(run(&context("SessionStart", json!(["Estelle skills loaded"]))).is_empty());
    assert!(run(&context("UserPromptSubmit", json!(["superpowers only"]))).is_empty());
    assert!(
        events_of(
            &context("UserPromptSubmit", json!("Estelle")),
            &mut Notes::default(),
            true
        )
        .is_empty()
    );
    let long = "estelle ".repeat(FIELD_CAP);
    let out = run(&context("UserPromptSubmit", json!(long)));
    assert!(
        out[0]["payload"]["output"]
            .as_str()
            .unwrap()
            .contains("[truncated:"),
        "capped like every other output"
    );
}

#[test]
fn tools_edits_and_results_keep_their_ids_pairing_and_the_bridge_shape() {
    let call = json!({"type": "assistant", "uuid": "a1", "timestamp": AT, "message": {"content": [
        {"type": "tool_use", "id": "c1", "name": "Bash", "input": {"command": "ls", "description": "List files"}},
        {"type": "tool_use", "id": "c2", "name": "Edit", "input": {"file_path": "a.rs", "old_string": "x", "new_string": "y"}},
        {"type": "tool_use", "id": "c3", "name": "Read", "input": {"file_path": "b.rs"}},
        {"type": "tool_use", "id": "c4", "name": "mcp__Estelle__verify", "input": {}}]}});
    let out = run(&call);
    assert_eq!(
        (
            out[0]["kind"].as_str(),
            out[0]["payload"]["summary"].as_str()
        ),
        (Some("tool_call"), Some("Ran List files"))
    );
    assert_eq!(
        out[0]["author"],
        json!({"kind": "agent", "id": "claude-code"})
    );
    assert_eq!(
        (out[1]["kind"].as_str(), out[1]["payload"]["diff"].as_str()),
        (Some("edit"), Some("-x\n+y"))
    );
    assert_eq!(out[1]["event_id"], "a1:1");
    assert_eq!(
        out[2]["payload"]["input"],
        json!({"file_path": "b.rs", "path": "b.rs"}),
        "input is an object, path copied"
    );
    assert_eq!(
        (
            out[3]["author"]["kind"].as_str(),
            out[3]["payload"]["summary"].as_str()
        ),
        (Some("estelle"), Some("verify"))
    );
    let result = user(
        "r1",
        json!([{"type": "tool_result", "tool_use_id": "c1", "is_error": true, "content": [{"type": "text", "text": "boom"}]}]),
    );
    let out = run(&result);
    assert_eq!(
        (
            out[0]["payload"]["call_id"].as_str(),
            out[0]["payload"]["ok"].as_bool(),
            out[0]["payload"]["output"].as_str()
        ),
        (Some("c1"), Some(false), Some("boom"))
    );
    assert_eq!(
        out.len(),
        1,
        "a tool result is not the customer starting work"
    );
}

#[test]
fn a_subagent_timeline_speaks_in_its_own_voice_and_reports_on_handback() {
    let brief = json!({"type": "user", "isSidechain": true, "uuid": "s0", "timestamp": AT, "message": {"content": "Find the bug"}});
    let reply = json!({"type": "assistant", "isSidechain": true, "uuid": "s1", "timestamp": AT, "message": {"content": [
        {"type": "text", "text": "Looking"},
        {"type": "tool_use", "id": "h1", "name": "SubagentHandback", "input": {"message": "Found it"}}]}});
    let mut notes = Notes::default();
    let b = events_of(&brief, &mut notes, true);
    assert_eq!(
        (b.len(), b[0]["payload"]["brief_to_subagent"].as_bool()),
        (1, Some(true)),
        "no lifecycle inside a child"
    );
    let r = events_of(&reply, &mut notes, true);
    assert_eq!(r[0]["author"], json!({"kind": "agent", "id": "subagent"}));
    assert!(
        r[0]["payload"].get("subagent").is_none(),
        "inside its own timeline it is the conversation, not an aside"
    );
    assert_eq!(
        (
            r[1]["kind"].as_str(),
            r[1]["payload"]["text"].as_str(),
            r[1]["payload"]["report"].as_bool()
        ),
        (Some("agent_message"), Some("Found it"), Some(true))
    );
    // In the PARENT's transcript a sidechain reply is an aside.
    let aside = events_of(&reply, &mut Notes::default(), false);
    assert_eq!(aside[0]["payload"]["subagent"], json!(true));
    assert_eq!(
        aside[1]["kind"], "tool_call",
        "outside the child a handback is just a call"
    );
}

#[test]
fn usage_is_attached_once_per_reply() {
    let reply = json!({"type": "assistant", "uuid": "a1", "timestamp": AT, "message": {"model": "m", "usage": {"input_tokens": 3, "output_tokens": 4},
        "content": [{"type": "text", "text": "one"}, {"type": "text", "text": "two"}]}});
    let out = run(&reply);
    assert_eq!(out[0]["payload"]["usage"]["output"], 4);
    assert!(out[1]["payload"].get("usage").is_none());
}

#[test]
fn secrets_are_redacted_before_they_leave() {
    let out = redact(
        "key sk_live_abcdefghijklmnop and postgres://u:hunter2@host/db and DB_PASSWORD=supersecret1",
    );
    assert!(
        !out.contains("abcdefghijklmnop")
            && !out.contains("hunter2")
            && !out.contains("supersecret1"),
        "{out}"
    );
    assert!(out.contains("DB_PASSWORD=[redacted]"), "{out}");
}

#[test]
fn nul_is_stripped_from_every_string_and_key_and_nothing_else_changes() {
    let dirty = json!({"a\u{0}b": ["x\u{0}y", 1, {"k": "\u{0}"}], "n": null, "t": true});
    assert_eq!(
        scrub(dirty),
        json!({"ab": ["xy", 1, {"k": ""}], "n": null, "t": true})
    );
    assert!(!scrub(json!("\u{0}")).to_string().contains("\\u0000"));
}

#[test]
fn a_derived_time_is_never_later_than_the_transcript() {
    let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    let (last, now) = (t("2026-10-21T10:00:05Z"), t("2026-10-22T00:00:00Z"));
    assert_eq!(stamp(None, Some(last), now), "2026-10-21T10:00:05.000Z");
    assert_eq!(
        stamp(Some(now), Some(last), now),
        "2026-10-21T10:00:05.000Z",
        "clamped to the last entry"
    );
    assert_eq!(
        stamp(Some(t("2026-10-21T09:00:00Z")), Some(last), now),
        "2026-10-21T09:00:00.000Z",
        "earlier stays"
    );
    assert_eq!(
        stamp(None, None, now),
        "2026-10-22T00:00:00.000Z",
        "the stated fallback"
    );
    assert_eq!(
        entry_time(&json!({"timestamp": "2026-10-21T10:00:05.250Z"})),
        Some(t("2026-10-21T10:00:05.250Z"))
    );
    assert_eq!(entry_time(&json!({"timestamp": "yesterday"})), None);
}

#[test]
fn batches_are_bounded_by_count_and_by_bytes() {
    let small: Vec<Value> = (0..120).map(|i| json!({"i": i})).collect();
    assert_eq!(
        batches(small).iter().map(Vec::len).collect::<Vec<_>>(),
        vec![50, 50, 20]
    );
    let big = "x".repeat(MAX_BATCH_BYTES / 2 + 10);
    let out = batches(vec![json!(big.clone()), json!(big.clone()), json!(big)]);
    assert_eq!(out.len(), 3, "two halves-plus never share a request");
    assert!(batches(Vec::new()).is_empty());
}

#[test]
fn every_secret_pattern_compiles() {
    assert_eq!(
        SECRETS.len(),
        SECRET_SHAPES,
        "a pattern that does not compile redacts nothing"
    );
    assert!(NOTE_TAG.is_some());
}
