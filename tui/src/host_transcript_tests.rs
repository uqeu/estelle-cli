//! The normaliser, driven by fixtures DERIVED FROM REAL ROLLOUTS.
//!
//! 🔴 **A HAND-WRITTEN FIXTURE WOULD HAVE PASSED FOR EIGHT MONTHS.** That is not a hypothetical:
//! the checkpoint's Claude Code tests were all hand-written, they were all green, and the
//! function they covered returned empty on every Codex file that has ever existed. So the
//! fixtures here are produced from the founder's actual `~/.codex/sessions` rollouts with every
//! free-text value replaced by a placeholder and the STRUCTURE left byte-faithful — same keys,
//! same nesting, same JSON types, same record order. The discriminants the parser branches on
//! (`type`, `role`, `record_type`, tool `name`) are preserved, because they ARE the structure.
//!
//! `host_transcript_corpus_tests.rs` drives the same code against the real files themselves.

use super::Dialect;
use super::host_records;
use super::read_bounded;
use serde_json::Value;
use serde_json::json;

const CODEX_2026: &str = include_str!("../tests/fixtures/codex-rollout-derived.jsonl");
const CODEX_2025: &str = include_str!("../tests/fixtures/codex-rollout-2025-derived.jsonl");

/// Every `{role, text}` the normaliser produced, flattened for assertions.
fn turns(records: &[Value]) -> Vec<(String, String)> {
    records
        .iter()
        .filter_map(|record| {
            let role = record.get("type")?.as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let content = record.get("message")?.get("content")?;
            // Claude Code writes a user turn's content as a bare STRING and an assistant's as an
            // array of blocks. Both are real, and a helper that only knew the array shape read
            // this test's own fixture as one turn short.
            let Some(blocks) = content.as_array() else {
                return Some((role.to_string(), content.as_str()?.to_string()));
            };
            let text = blocks
                .iter()
                .filter_map(|block| match block.get("type")?.as_str()? {
                    "text" => block.get("text")?.as_str().map(str::to_string),
                    "tool_use" => Some(format!(
                        "[tool:{}]",
                        block.get("name").and_then(Value::as_str).unwrap_or("?")
                    )),
                    other => Some(format!("[{other}]")),
                })
                .collect::<Vec<_>>()
                .join(" ");
            Some((role.to_string(), text))
        })
        .collect()
}

#[test]
fn a_real_codex_rollout_is_recognised_and_yields_both_halves_of_the_conversation() {
    let parsed = host_records(CODEX_2026);
    assert_eq!(
        parsed.dialect,
        Some(Dialect::CodexRollout),
        "the 2026 rollout shape must be recognised, not shrugged at"
    );
    assert_eq!(parsed.unreadable_lines, 0);

    let turns = turns(&parsed.records);
    let users = turns.iter().filter(|(role, _)| role == "user").count();
    let assistants = turns.iter().filter(|(role, _)| role == "assistant").count();
    assert!(
        users >= 1 && assistants >= 1,
        "a rollout carrying both halves must yield both: {turns:?}"
    );
}

/// The `developer` role is the harness instructing the model. It is not a turn of the
/// conversation and it must not be checkpointed as the customer's words.
#[test]
fn the_developer_role_is_not_a_turn() {
    let parsed = host_records(CODEX_2026);
    assert!(
        !parsed
            .records
            .iter()
            .any(|record| record.get("type").and_then(Value::as_str) == Some("developer")),
        "a developer message is instructions, never a turn"
    );
    // …and the fixture really does contain one, so the assertion above is not vacuous.
    assert!(
        CODEX_2026.contains("\"role\":\"developer\""),
        "the fixture must carry a developer record for that assertion to mean anything"
    );
}

/// The 2025 dialect: bare `ResponseItem` lines with no `payload` wrapper, interleaved with
/// `{"record_type":"state"}`. One of the founder's 161 rollouts is still in this shape.
#[test]
fn the_2025_bare_rollout_dialect_is_also_recognised() {
    let parsed = host_records(CODEX_2025);
    assert_eq!(parsed.dialect, Some(Dialect::CodexRollout));
    let turns = turns(&parsed.records);
    assert!(
        turns.iter().any(|(role, _)| role == "user"),
        "the legacy shape carries user turns: {turns:?}"
    );
}

/// Claude Code's own shape must keep working — this is the half that already shipped, and a
/// normaliser that breaks it has traded one silent outage for another.
#[test]
fn the_claude_code_dialect_still_wins_its_own_records() {
    let text = [
        json!({"type": "user", "sessionId": "s", "cwd": "/repo", "gitBranch": "main",
               "message": {"role": "user", "content": "why is auth failing?"}}),
        json!({"type": "assistant", "sessionId": "s", "message": {"role": "assistant", "content": [
            {"type": "text", "text": "checking"},
            {"type": "tool_use", "name": "Edit", "input": {"file_path": "/repo/auth.rs"}},
        ]}}),
    ]
    .iter()
    .map(std::string::ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");

    let parsed = host_records(&text);
    assert_eq!(parsed.dialect, Some(Dialect::ClaudeCode));
    assert_eq!(
        turns(&parsed.records),
        vec![
            ("user".to_string(), "why is auth failing?".to_string()),
            ("assistant".to_string(), "checking [tool:Edit]".to_string()),
        ],
        "a Claude Code record passes through untouched"
    );
}

/// A Codex tool call is its own record; Claude Code nests it inside the assistant message. The
/// normaliser folds, so the 400-turn checkpoint budget is spent on conversation.
#[test]
fn a_codex_tool_call_folds_into_the_assistant_turn_it_belongs_to() {
    let text = [
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "message", "role": "assistant",
               "content": [{"type": "output_text", "text": "running it"}]}}),
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "custom_tool_call",
               "name": "exec", "call_id": "c1", "input": "ls"}}),
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "function_call",
               "name": "shell", "call_id": "c2", "arguments": "{}"}}),
    ]
    .iter()
    .map(std::string::ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");

    assert_eq!(
        turns(&host_records(&text).records),
        vec![(
            "assistant".to_string(),
            "running it [tool:exec] [tool:shell]".to_string()
        )],
        "two tool calls must not become two extra turns"
    );
}

/// A tool call AFTER the human speaks belongs to the next assistant turn, not the previous one.
#[test]
fn a_user_turn_closes_the_open_assistant_turn() {
    let text = [
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "message", "role": "assistant",
               "content": [{"type": "output_text", "text": "done"}]}}),
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "message", "role": "user",
               "content": [{"type": "input_text", "text": "now revert it"}]}}),
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "custom_tool_call",
               "name": "exec", "call_id": "c1", "input": "git revert"}}),
    ]
    .iter()
    .map(std::string::ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");

    assert_eq!(
        turns(&host_records(&text).records),
        vec![
            ("assistant".to_string(), "done".to_string()),
            ("user".to_string(), "now revert it".to_string()),
            ("assistant".to_string(), "[tool:exec]".to_string()),
        ],
        "the tool call must not be attributed to the assistant turn before the human spoke"
    );
}

/// Codex reports one applied patch twice — as `patch_apply_end` and again as a `FileChange`
/// item. That is an echo, not two edits.
#[test]
fn one_applied_patch_reported_twice_is_one_edit() {
    let text = [
        json!({"timestamp": "t", "type": "response_item", "payload": {"type": "message", "role": "assistant",
               "content": [{"type": "output_text", "text": "patching"}]}}),
        json!({"timestamp": "t", "type": "event_msg", "payload": {"type": "patch_apply_end", "success": true,
               "changes": {"/repo/a.rs": {"type": "update", "unified_diff": "…"}}}}),
        json!({"timestamp": "t", "type": "event_msg", "payload": {"type": "item_completed",
               "item": {"type": "FileChange", "changes": {"/repo/a.rs": {"type": "update", "unified_diff": "…"}}}}}),
    ]
    .iter()
    .map(std::string::ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");

    assert_eq!(
        turns(&host_records(&text).records),
        vec![(
            "assistant".to_string(),
            "patching [tool:apply_patch]".to_string()
        )],
        "the same block twice in a row is the echo, not a second edit"
    );
}

/// The session facts a resume needs are carried under Claude Code's spelling, so the ONE
/// context extractor reads both hosts without a branch.
#[test]
fn session_facts_arrive_under_the_names_the_extractor_reads() {
    let parsed = host_records(CODEX_2026);
    let field = |key: &str| -> Option<String> {
        parsed
            .records
            .iter()
            .rev()
            .find_map(|record| record.get(key)?.as_str().map(str::to_string))
    };
    assert_eq!(field("cwd").as_deref(), Some("/repo/demo"));
    assert_eq!(field("gitBranch").as_deref(), Some("main"));
    assert_eq!(field("version").as_deref(), Some("0.0.0-fixture"));
    assert_eq!(field("entrypoint").as_deref(), Some("codex_cli_rs"));
    let model = parsed.records.iter().rev().find_map(|record| {
        record
            .get("message")?
            .get("model")?
            .as_str()
            .map(str::to_string)
    });
    assert_eq!(model.as_deref(), Some("fixture-model"));
}

/// 🔴 AN UNRECOGNISED FORMAT MUST BE REPORTABLE. `None` here is what makes the caller speak
/// instead of returning empty — the silence that hid this defect for eight months.
#[test]
fn an_unknown_format_reports_no_dialect_rather_than_an_empty_success() {
    let parsed = host_records("{\"hello\":\"world\"}\n{\"another\":1}\nnot json at all\n");
    assert_eq!(parsed.dialect, None, "neither host wrote this");
    assert_eq!(parsed.lines, 3);
    assert_eq!(parsed.unreadable_lines, 1);
    assert_eq!(parsed.unrecognised_lines, 2);
    assert!(parsed.records.is_empty());
}

/// An image never carries its bytes into a checkpoint, and its `data:` URI never travels.
#[test]
fn a_codex_image_travels_as_its_type_and_never_as_its_bytes() {
    let text = json!({"timestamp": "t", "type": "response_item", "payload": {"type": "message", "role": "user",
        "content": [{"type": "input_image", "detail": "auto",
                     "image_url": "data:image/png;base64,QUJDREVGR0g="}]}})
    .to_string();
    let parsed = host_records(&text);
    let wire = serde_json::to_string(&parsed.records).expect("wire");
    assert!(
        !wire.contains("QUJDREVGR0g="),
        "the image payload must not reach the checkpoint: {wire}"
    );
    assert!(wire.contains("image/png"), "the media type does: {wire}");
}

#[test]
fn a_small_file_is_read_whole_and_says_it_was_not_truncated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("t.jsonl");
    std::fs::write(&path, CODEX_2026).expect("write");
    let bounded = read_bounded(&path).expect("read");
    assert!(!bounded.truncated);
    assert_eq!(bounded.text, CODEX_2026);
    assert_eq!(bounded.read_bytes, bounded.file_bytes);
}

#[test]
fn a_missing_file_is_an_error_and_never_an_empty_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(
        read_bounded(&dir.path().join("absent.jsonl")).is_err(),
        "an unreadable transcript must be distinguishable from an empty one"
    );
}
