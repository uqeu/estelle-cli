//! 🔴 THE PARSER, DRIVEN BY THE REAL FILES — NOT BY A FIXTURE THIS REPO WROTE.
//!
//! # Why this file exists at all
//!
//! Estelle had never captured a single Codex session, and every checkpoint test was green
//! throughout. All of them were hand-written Claude Code records, so they modelled a host that
//! was not the one shipping, and the function under test returned EMPTY on every Codex file that
//! has ever existed. **A test double friendlier than production certifies code production
//! rejects.** So this file opens the founder's actual rollouts.
//!
//! # The pair that cannot both be true
//!
//! > *a rollout file contained user turns* **AND** *the parser returned empty*
//!
//! [`every_rollout_with_a_user_record_yields_a_user_turn`] makes that pair unreachable: an
//! independent oracle counts the records that ARE user messages, and any file the oracle scores
//! above zero must produce turns. It names no cause, so it catches variants nobody has thought
//! of yet.
//!
//! ⚠️ **THE ORACLE IS STRUCTURAL BECAUSE A SUBSTRING ORACLE CANNOT NOT-FIRE.** The obvious
//! cheap oracle — "does the file contain `\"role\":\"user\"`?" — was written first and measured:
//! it fired on **21 of the 161 rollouts** that contain no top-level user record at all, because
//! a rollout stores command output verbatim and that output frequently contains JSON. Asserting
//! on an echo of your own needle is the attack_19 defect. The oracle here parses each LINE and
//! asks about the RECORD's own fields, which is a different question from the one the parser
//! answers (it does not decode content, roles, blocks, redaction or caps).
//!
//! # What a skip means
//!
//! A machine with no `~/.codex/sessions` cannot run this, and the run says so loudly rather than
//! passing quietly. The committed derived fixtures in `host_transcript_tests.rs` still run there.

use super::*;
use std::path::Path;
use std::path::PathBuf;

/// How many real rollouts one run opens. The corpus is 161 files totalling well over 100 GB;
/// a test is not allowed to read all of it, and a bound with no name is not a bound.
const CORPUS_SAMPLE_MAX_FILES: usize = 40;

/// Where Codex keeps them. `CODEX_HOME` wins, exactly as the host resolves it.
fn corpus_root() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))?;
    let sessions = home.join("sessions");
    sessions.is_dir().then_some(sessions)
}

/// Every `rollout-*.jsonl` under the corpus root, deepest-first order made deterministic by
/// sorting, then thinned to [`CORPUS_SAMPLE_MAX_FILES`] with an even stride so the sample spans
/// the whole date range instead of one week of it.
fn corpus_sample() -> Vec<PathBuf> {
    let Some(root) = corpus_root() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    collect_rollouts(&root, &mut found, 0);
    found.sort();
    if found.len() <= CORPUS_SAMPLE_MAX_FILES {
        return found;
    }
    let stride = found.len().div_ceil(CORPUS_SAMPLE_MAX_FILES);
    found.into_iter().step_by(stride).collect()
}

/// Bounded recursion (Power of Ten #1/#2): the layout is `YYYY/MM/DD/`, so four levels is the
/// whole tree and anything deeper is not ours to walk.
fn collect_rollouts(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, out, depth + 1);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

/// THE INDEPENDENT ORACLE. Counts records that are, on their own top-level fields, a user
/// message — in either Codex dialect. It deliberately shares no code with the parser and asks a
/// narrower question: it never decodes content, never resolves a role default, never folds a
/// tool call and never applies a cap.
fn oracle_user_records(text: &str) -> usize {
    let mut found = 0;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let node = record.get("payload").unwrap_or(&record);
        if node.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if node.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        // A record whose content is entirely empty strings is not a turn for anyone.
        let has_text = match node.get("content") {
            Some(Value::Array(blocks)) => blocks.iter().any(|block| {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty())
            }),
            Some(Value::String(text)) => !text.trim().is_empty(),
            _ => false,
        };
        if has_text {
            found += 1;
        }
    }
    found
}

#[expect(
    clippy::print_stderr,
    reason = "a corpus-dependent test that skips quietly is the inert guard this file exists to prevent"
)]
fn announce(line: &str) {
    eprintln!("{line}");
}

/// 🔴 THE LOAD-BEARING ASSERTION.
///
/// For every sampled rollout: if the oracle says the file holds user records, the parser must
/// produce user turns. The converse direction is deliberately NOT asserted — the parser may
/// legitimately find turns the oracle's narrow question misses (an `agent_message`, a folded
/// tool call), and demanding equality would make the test a second copy of the parser.
#[test]
fn every_rollout_with_a_user_record_yields_a_user_turn() {
    let sample = corpus_sample();
    if sample.is_empty() {
        announce(
            "SKIPPED every_rollout_with_a_user_record_yields_a_user_turn: no ~/.codex/sessions on \
             this machine. The derived fixtures in host_transcript_tests.rs still ran; the real \
             corpus did not.",
        );
        return;
    }

    let (mut files_with_records, mut user_turns, mut assistant_turns, mut files_parsed) =
        (0usize, 0usize, 0usize, 0usize);
    let mut truncated_files = 0usize;
    let mut violations = Vec::new();

    for path in &sample {
        let Ok(bounded) = host_transcript::read_bounded(path) else {
            continue;
        };
        if bounded.truncated {
            truncated_files += 1;
        }
        let parsed = host_transcript::host_records(&bounded.text);
        let conversation = transcript_messages(&parsed.records);
        let users = conversation
            .messages
            .iter()
            .filter(|message| message["role"] == "user")
            .count();
        let assistants = conversation.messages.len() - users;
        user_turns += users;
        assistant_turns += assistants;
        if !conversation.messages.is_empty() {
            files_parsed += 1;
        }

        let expected = oracle_user_records(&bounded.text);
        if expected > 0 {
            files_with_records += 1;
            if users == 0 {
                // Report the SHAPE, never the content — these are private conversations.
                violations.push(format!(
                    "{} user record(s) present, 0 user turns parsed (dialect {:?}, {} records)",
                    expected, parsed.dialect, parsed.lines
                ));
            }
        }
    }

    announce(&format!(
        "real corpus: {} file(s) sampled, {} truncated by the byte bound, {} yielded turns, \
         {} user + {} assistant turns, {} file(s) held user records",
        sample.len(),
        truncated_files,
        files_parsed,
        user_turns,
        assistant_turns,
        files_with_records,
    ));

    assert!(
        violations.is_empty(),
        "UNREACHABLE PAIR REACHED — a rollout held user records and the parser returned none:\n{}",
        violations.join("\n")
    );
    // …and the assertion above is worthless if the oracle never fired, so the run must also show
    // that the corpus actually exercised it. A green over zero checks is `api_checks: 0`.
    assert!(
        files_with_records > 0,
        "the oracle scored zero on every sampled file, so the pair above proved nothing"
    );
    assert!(
        user_turns > 0 && assistant_turns > 0,
        "the parser must extract BOTH halves of a real conversation, got {user_turns} user / \
         {assistant_turns} assistant"
    );
}

/// The whole hook path, on a real rollout, through the same `checkpoint_local` the deferred
/// SessionEnd handoff replays. No network is reachable from this function by construction.
#[tokio::test]
async fn checkpoint_local_builds_a_body_from_a_real_rollout() {
    // 🔴 THE FILE IS CHOSEN BY THE ORACLE, NOT BY THE PARSER — AND THAT DISTINCTION IS THE WHOLE
    // TEST. The first version searched for "a rollout the parser can read", which is circular:
    // under the mutation that un-recognises the 2026 rollout shape, the search simply walked back
    // to the one 2025-dialect file in the corpus and passed. The mutation run reported it as a
    // SURVIVOR, which is the only reason this is written the way it is now. Selection by a
    // criterion the code under test cannot influence is what makes the assertion mean anything.
    //
    // ⚠️ ONLY AN ABSENT CORPUS IS A SKIP. A corpus that is present and holds a conversation the
    // hook cannot checkpoint is the defect itself, and it fails here.
    let sample = corpus_sample();
    if sample.is_empty() {
        announce(
            "SKIPPED checkpoint_local_builds_a_body_from_a_real_rollout: no ~/.codex/sessions on \
             this machine.",
        );
        return;
    }
    let path = sample
        .into_iter()
        .rev()
        .find(|path| {
            host_transcript::read_bounded(path)
                .is_ok_and(|bounded| oracle_user_records(&bounded.text) > 0)
        })
        .expect(
            "the corpus is present, so some rollout must hold a user record; if none does, this \
             machine's corpus cannot exercise the hook at all",
        );

    let root = tempfile::tempdir().expect("tempdir");
    let payload: HookPayload = serde_json::from_value(json!({
        "session_id": "corpus-session",
        "transcript_path": path.to_string_lossy(),
        "cwd": root.path().to_string_lossy(),
        "hook_event_name": "Stop",
    }))
    .expect("payload");

    let body = checkpoint_local(&payload, Some(root.path().join("gap.json")))
        .await
        .expect("a real rollout must produce a checkpoint body");

    assert_eq!(
        body["client"]["name"],
        json!("codex"),
        "the checkpoint must name the host that wrote the file, not the one we wrote the parser \
         against"
    );
    let messages = body["messages"].as_array().expect("messages");
    assert!(
        !messages.is_empty(),
        "the defect being fixed is precisely an empty messages array here"
    );
    assert!(
        messages.len() <= CHECKPOINT_MAX_MESSAGES,
        "the turn cap still holds on real data"
    );
    assert!(
        body["client"]["transcript_bytes"].as_u64().unwrap_or(0) > 0,
        "the checkpoint records how big the transcript it read was"
    );
    // Truncation is a CLAIM the body has to make about itself, so the two fields must agree.
    let partial = body["client"]["transcript_partial"]
        .as_bool()
        .expect("transcript_partial is always present");
    let read = body["client"]["transcript_read_bytes"]
        .as_u64()
        .expect("transcript_read_bytes");
    let total = body["client"]["transcript_bytes"].as_u64().expect("bytes");
    assert_eq!(
        partial,
        read < total || body["client"]["turns_dropped"].as_u64().unwrap_or(0) > 0,
        "a body that read {read} of {total} bytes must not claim to be whole"
    );
}

/// 🔴 THE BYTE BOUND, ON THE REAL FILES IT EXISTS FOR — not on a synthetic one sized to pass.
///
/// The corpus contains 71 rollouts larger than [`host_transcript::TAIL_MAX_BYTES`] (p95 530.8 MB,
/// max 35.3 GB). Every one of them must be read as head + tail, must SAY it was truncated, and
/// must still parse.
#[test]
fn a_rollout_larger_than_the_bound_is_truncated_and_says_so() {
    let Some((path, size)) = corpus_sample()
        .into_iter()
        .filter_map(|path| {
            let size = std::fs::metadata(&path).ok()?.len();
            (size > host_transcript::TAIL_MAX_BYTES).then_some((path, size))
        })
        .max_by_key(|(_, size)| *size)
    else {
        announce(
            "SKIPPED a_rollout_larger_than_the_bound_is_truncated_and_says_so: no rollout above \
             the bound on this machine.",
        );
        return;
    };

    let bounded =
        host_transcript::read_bounded(&path).expect("a large rollout must still be readable");
    assert!(
        bounded.truncated,
        "a {size}-byte file read under a {}-byte bound is truncated",
        host_transcript::TAIL_MAX_BYTES
    );
    assert_eq!(bounded.file_bytes, size);
    assert!(
        bounded.read_bytes <= host_transcript::TAIL_MAX_BYTES + host_transcript::HEAD_MAX_BYTES,
        "the bound is what stops a 35 GB file from being loaded into memory"
    );
    assert!(bounded.read_bytes < bounded.file_bytes);

    // Both seams must have been cut at a record boundary, or the parser is handed a fragment.
    let parsed = host_transcript::host_records(&bounded.text);
    assert_eq!(
        parsed.dialect,
        Some(host_transcript::Dialect::CodexRollout),
        "a truncated read must still be recognisable"
    );
    assert_eq!(
        parsed.unreadable_lines, 0,
        "a partial record at either seam must be dropped whole, not parsed as a fragment"
    );

    // And the head really was read: the session identity lives on the first line, a very long
    // way behind the tail window.
    assert!(
        parsed
            .records
            .iter()
            .any(|record| record.get("version").is_some() || record.get("gitBranch").is_some()),
        "the head window exists so a large session keeps its identity"
    );
}

/// The whole path from a Codex file-change event to a tracked written file, across the seam
/// between the normaliser and the extractor. A rollout has no `Write`/`Edit` tool call at all —
/// `apply_patch` is the only spelling — so the extractor's name list and the normaliser's block
/// have to agree or a Codex session records that it wrote nothing.
#[test]
fn a_codex_patch_reaches_transcript_files_as_a_written_path() {
    let text = [
        json!({"timestamp": "t", "type": "event_msg", "payload": {"type": "patch_apply_end", "success": true,
               "changes": {"/repo/first.rs": {"type": "update", "unified_diff": "…"}}}}),
        json!({"timestamp": "t", "type": "event_msg", "payload": {"type": "item_completed",
               "item": {"type": "FileChange", "changes": {"/repo/second.rs": {"type": "add", "content": "…"}}}}}),
    ]
    .iter()
    .map(std::string::ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");

    let records = host_transcript::host_records(&text).records;
    assert_eq!(
        transcript_files(&records),
        vec![
            PathBuf::from("/repo/second.rs"),
            PathBuf::from("/repo/first.rs"),
        ],
        "most-recently-touched first, and both Codex change events count"
    );
}

/// A truncated checkpoint must SAY it is partial inside the conversation itself, because a
/// `client` field a consumer does not know about is a field a consumer drops.
#[test]
fn a_partial_checkpoint_declares_itself_inside_the_messages() {
    let mut conversation = Conversation {
        messages: vec![json!({"role": "user", "content": "the oldest surviving turn"})],
        dropped: 12,
    };
    disclose_truncation(
        &mut conversation,
        Truncation {
            window: true,
            dropped_turns: 12,
            file_bytes: 35_293_766_776,
            read_bytes: host_transcript::TAIL_MAX_BYTES + host_transcript::HEAD_MAX_BYTES,
        },
    );
    let first = conversation.messages[0]["content"]
        .as_str()
        .expect("content");
    assert!(first.contains("PARTIAL"), "{first}");
    assert!(first.contains("12 older turn(s)"), "{first}");
    assert!(
        first.contains("the oldest surviving turn"),
        "the marker prefixes the turn, it does not replace it: {first}"
    );

    // …and a whole session must never claim to be partial.
    let mut whole = Conversation {
        messages: vec![json!({"role": "user", "content": "all of it"})],
        dropped: 0,
    };
    disclose_truncation(&mut whole, Truncation::default());
    assert_eq!(whole.messages[0]["content"], json!("all of it"));
}
