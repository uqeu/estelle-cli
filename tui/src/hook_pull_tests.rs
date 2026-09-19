//! The FORCED PULL decision, proven the way this repo requires: every guard below has a mutation
//! that turns a NAMED test red, and the negatives are tested as loudly as the positives — a hook
//! that fires on `git status` gets uninstalled, and an uninstalled hook still carries the belief
//! that it ran.

use super::*;
use serde_json::json;

fn intent(tool: &str, input: serde_json::Value) -> Option<SearchIntent> {
    search_intent(tool, &input)
}

fn bash(command: &str) -> Option<SearchIntent> {
    intent("Bash", json!({"command": command}))
}

// ── the ONE OWNER: what counts as a search ───────────────────────────────────────────────────

#[test]
fn the_three_host_tools_are_recognised_with_their_own_kinds() {
    assert_eq!(
        intent("Read", json!({"file_path": "/repo/src/api.py"})),
        Some(SearchIntent {
            kind: SearchKind::ReadFile,
            needle: "/repo/src/api.py".to_string(),
            scope: Some("/repo/src/api.py".to_string()),
        })
    );
    assert_eq!(
        intent("Grep", json!({"pattern": "def ground", "path": "src"})),
        Some(SearchIntent {
            kind: SearchKind::Content,
            needle: "def ground".to_string(),
            scope: Some("src".to_string()),
        })
    );
    assert_eq!(
        intent("Glob", json!({"pattern": "**/*.py"})),
        Some(SearchIntent {
            kind: SearchKind::Names,
            needle: "**/*.py".to_string(),
            scope: None,
        })
    );
}

#[test]
fn a_tool_that_is_not_a_search_is_never_one() {
    for (tool, input) in [
        ("Write", json!({"file_path": "/repo/a.py", "content": "x"})),
        (
            "Edit",
            json!({"file_path": "/repo/a.py", "new_string": "x"}),
        ),
        ("WebFetch", json!({"url": "https://example.invalid"})),
        ("Read", json!({})),
        ("Grep", json!({"pattern": "   "})),
    ] {
        assert_eq!(
            intent(tool, input),
            None,
            "{tool} must not read as a search"
        );
    }
}

#[test]
fn the_shell_search_words_fire_and_their_neighbours_do_not() {
    for command in [
        "grep -rn ground src/",
        "rg --hidden ground",
        "ag ground",
        "ack ground",
        "/usr/bin/grep -r ground .",
        "sudo grep -rn ground /etc",
        "cd src && grep -rn ground .",
        "FOO=1 grep -rn ground .",
        "git grep -n ground",
    ] {
        assert_eq!(
            bash(command).map(|found| found.kind),
            Some(SearchKind::Content),
            "{command:?} is a content search"
        );
    }
    for command in ["find . -name '*.py'", "fd ground", "ls -R src", "ls -laR"] {
        assert_eq!(
            bash(command).map(|found| found.kind),
            Some(SearchKind::Names),
            "{command:?} is a filename search"
        );
    }
}

/// 🔴 THE NEGATIVES ARE THE POINT. Each of these has been seen in a real session and each would
/// make the hook noise rather than signal.
#[test]
fn the_commands_that_are_not_repo_searches_stay_silent() {
    for command in [
        "ls -la",
        "ls",
        "git status",
        "git status --short",
        "git diff --stat",
        "cargo test -p estelle-tui",
        "python3 -c 'print(1)'",
        "echo grep",
        "npm run find",
        // A FILTER over another command's output: the graph cannot answer it.
        "git log --oneline | grep fix",
        "cat README.md | grep install",
        "ps aux | grep estelle",
    ] {
        assert_eq!(bash(command), None, "{command:?} must stay silent");
    }
}

#[test]
fn a_search_at_the_head_of_a_pipeline_still_counts() {
    assert_eq!(
        bash("grep -rn ground src/ | head -20").map(|found| found.kind),
        Some(SearchKind::Content),
        "the head of the pipeline reads the repo even when something filters it"
    );
}

#[test]
fn recursive_ls_is_read_letter_by_letter_so_dash_la_cannot_match() {
    assert!(recursive_flag("-R"));
    assert!(recursive_flag("-laR"));
    assert!(recursive_flag("--recursive"));
    assert!(!recursive_flag("-la"));
    assert!(!recursive_flag("-l"));
    assert!(!recursive_flag("--reverse"));
    assert!(!recursive_flag("src"));
}

#[test]
fn a_command_longer_than_the_bound_is_truncated_rather_than_read() {
    let long = format!("echo {} ; grep -rn x .", "a".repeat(MAX_COMMAND_BYTES));
    assert_eq!(
        bash(&long),
        None,
        "reading stops at the bound; a missed redirect is the safe way to be wrong"
    );
}

// ── the tree the graph can speak about ───────────────────────────────────────────────────────

#[test]
fn a_path_outside_the_repository_is_not_in_the_indexed_tree() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(root.path().join("a.py"), "x").expect("write");
    std::fs::write(outside.path().join("b.py"), "x").expect("write");

    assert!(in_indexed_tree("a.py", root.path()));
    assert!(in_indexed_tree(
        root.path().join("a.py").to_str().expect("utf8"),
        root.path()
    ));
    assert!(
        !in_indexed_tree(
            outside.path().join("b.py").to_str().expect("utf8"),
            root.path()
        ),
        "a file in another tree is not in this repo's graph"
    );
    assert!(!in_indexed_tree("/etc/hosts", root.path()));
    assert!(!in_indexed_tree("", root.path()));
}

#[test]
fn a_path_inside_a_skipped_directory_is_not_in_the_indexed_tree() {
    let root = tempfile::tempdir().expect("root");
    for skipped in ["node_modules", "target", ".git"] {
        let nested = root.path().join(skipped).join("pkg");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::write(nested.join("index.js"), "x").expect("write");
        assert!(
            !in_indexed_tree(nested.join("index.js").to_str().expect("utf8"), root.path()),
            "{skipped} is skipped by the ingest, so the graph has nothing to say about it"
        );
    }
}

// ── the decision ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_stale_index_never_produces_a_redirect_in_either_mode() {
    let quiet = PullState::default();
    assert_eq!(
        decide(SearchKind::Content, &quiet, false, false),
        PullVerdict::Silent,
        "advisory mode says nothing rather than sending the model to a tool that will answer STALE"
    );
    assert_eq!(
        decide(SearchKind::Content, &quiet, false, true),
        PullVerdict::Stale,
        "strict mode must never be silently inert — it says why it did not redirect"
    );
    let told = PullState {
        told_stale: true,
        ..PullState::default()
    };
    assert_eq!(
        decide(SearchKind::Content, &told, false, true),
        PullVerdict::Silent,
        "and it says it once"
    );
}

/// 🔴 THE ANTI-LOOP, ASSERTED RATHER THAN DESCRIBED. A redirect that refuses, is retried, and
/// refuses again wedges the session. The block is spent once and the session reverts to the nudge.
#[test]
fn strict_blocks_once_and_then_reverts_to_the_nudge() {
    let dir = tempfile::tempdir().expect("state");
    let mut state = PullState::default();

    let first = decide(SearchKind::ReadFile, &state, true, true);
    assert_eq!(first, PullVerdict::Block);
    assert_eq!(
        commit(
            first,
            SearchKind::ReadFile,
            &mut state,
            Some(dir.path()),
            "s1",
            false
        ),
        Persisted::Yes
    );

    // The model retries the very same read.
    let second = decide(SearchKind::ReadFile, &state, true, true);
    assert_ne!(second, PullVerdict::Block, "a second refusal is the wedge");
    assert_eq!(second, PullVerdict::Silent);

    // A DIFFERENT kind still gets its one nudge, and still never a block.
    let other = decide(SearchKind::Content, &state, true, true);
    assert_eq!(other, PullVerdict::Nudge);
}

/// 🔴 A BLOCK WE CANNOT REMEMBER IS A BLOCK THAT FIRES FOREVER. The write decides whether the
/// refusal is allowed to exist.
#[test]
fn a_block_that_cannot_be_persisted_is_refused_by_the_commit() {
    let jail = tempfile::tempdir().expect("jail");
    let occupied = jail.path().join("not-a-directory");
    std::fs::write(&occupied, "x").expect("write");

    let mut state = PullState::default();
    assert_eq!(
        commit(
            PullVerdict::Block,
            SearchKind::ReadFile,
            &mut state,
            Some(&occupied),
            "s1",
            false
        ),
        Persisted::No,
        "a state directory that is a file cannot hold the marker, so the block must not be emitted"
    );
}

#[test]
fn a_session_with_no_id_has_nowhere_to_remember_and_so_cannot_block() {
    let dir = tempfile::tempdir().expect("state");
    let mut state = PullState::default();
    assert_eq!(
        commit(
            PullVerdict::Block,
            SearchKind::ReadFile,
            &mut state,
            Some(dir.path()),
            "   ",
            false
        ),
        Persisted::No
    );
}

#[test]
fn the_nudge_is_bounded_per_kind_and_the_bound_is_the_named_constant() {
    let mut state = PullState::default();
    for round in 0..NUDGES_PER_KIND {
        assert_eq!(
            decide(SearchKind::Content, &state, true, false),
            PullVerdict::Nudge,
            "round {round}"
        );
        state.record(SearchKind::Content);
    }
    assert_eq!(
        decide(SearchKind::Content, &state, true, false),
        PullVerdict::Silent,
        "past the bound the hook stops talking — this is the answer to the 2.2% push precision"
    );
    assert_eq!(
        decide(SearchKind::Names, &state, true, false),
        PullVerdict::Nudge,
        "one kind's budget is not another's"
    );
}

#[test]
fn the_strict_opt_in_reads_the_same_spellings_as_the_grounding_gate() {
    for accepted in ["1", "true", "on", " ON ", "True"] {
        assert!(strict_enabled_from(Some(accepted)), "{accepted:?}");
        assert_eq!(
            estelle_tui::ground_block::blocking_enabled_from(Some(accepted)),
            strict_enabled_from(Some(accepted)),
            "two opt-ins that disagree about {accepted:?} is a support ticket"
        );
    }
    for refused in ["0", "false", "off", "yes", "", "  "] {
        assert!(!strict_enabled_from(Some(refused)), "{refused:?}");
    }
    assert!(!strict_enabled_from(None), "default OFF");
}

// ── the session marker ───────────────────────────────────────────────────────────────────────

#[test]
fn state_round_trips_and_an_old_marker_is_treated_as_absent() {
    let dir = tempfile::tempdir().expect("state");
    let state = PullState {
        told: vec![("content".to_string(), 1)],
        blocked: true,
        told_stale: false,
        fresh: Some(true),
    };
    assert!(save_state(Some(dir.path()), "abc-123", &state));

    let now = unix_now();
    let read = load_state(Some(dir.path()), "abc-123", now);
    assert!(read.blocked);
    assert_eq!(read.count(SearchKind::Content), 1);
    assert_eq!(read.fresh, Some(true));

    let later = now + STATE_MAX_AGE_S + 1;
    assert_eq!(
        load_state(Some(dir.path()), "abc-123", later),
        PullState::default(),
        "yesterday's marker is not evidence about this session"
    );
}

#[test]
fn an_oversized_or_unreadable_marker_reads_as_absent_rather_than_as_blocked() {
    let dir = tempfile::tempdir().expect("state");
    std::fs::write(dir.path().join("pull-big.json"), vec![b'x'; 4096]).expect("write");
    assert_eq!(
        load_state(Some(dir.path()), "big", unix_now()),
        PullState::default()
    );
    std::fs::write(dir.path().join("pull-bad.json"), b"{not json").expect("write");
    assert_eq!(
        load_state(Some(dir.path()), "bad", unix_now()),
        PullState::default()
    );
}

#[test]
fn a_host_session_id_cannot_escape_the_state_directory() {
    assert_eq!(session_stem("../../etc/passwd"), "etcpasswd");
    assert_eq!(session_stem("a/b"), "ab");
    assert_eq!(session_stem(""), "");
    assert_eq!(session_stem("   "), "");
    assert_eq!(session_stem("ok-123_ABC"), "ok-123_ABC");
}

// ── the words that reach a customer ──────────────────────────────────────────────────────────

/// 🔴 THE HUMAN LINE MUST NOT CARRY THE CALLER'S OWN STRING. A shell search is exactly the shape
/// that sometimes names a credential, and `systemMessage` is printed to the terminal and written
/// to the on-disk transcript.
#[test]
fn the_terminal_line_never_quotes_the_query() {
    let secret = "SOME_SENTINEL_THAT_MUST_NOT_APPEAR";
    for kind in [SearchKind::ReadFile, SearchKind::Content, SearchKind::Names] {
        let line = system_line(kind);
        assert!(
            !line.contains(secret),
            "the human line is generic by construction"
        );
        assert!(line.contains("Estelle"), "it must name who is speaking");
    }
    assert!(redirect(SearchKind::Content, secret, "acme/widgets").contains(secret));
}

#[test]
fn every_redirect_names_a_real_estelle_tool_and_the_repo() {
    let expected: [(SearchKind, &str); 3] = [
        (SearchKind::ReadFile, "find_definition"),
        (SearchKind::Content, "estelle_grep"),
        (SearchKind::Names, "locate"),
    ];
    for (kind, tool) in expected {
        let text = redirect(kind, "needle", "acme/widgets");
        assert!(text.contains(tool), "{kind:?} must route to {tool}");
        assert!(text.contains("acme/widgets"), "{kind:?} must name the repo");
    }
}

/// The stale sentence must NOT name a graph tool: naming one is the exact mistake the branch
/// exists to avoid.
#[test]
fn the_stale_line_redirects_nowhere() {
    let line = stale_line("acme/widgets");
    for tool in [
        "estelle_grep",
        "find_definition",
        "locate",
        "blast_radius",
        "find_usages",
    ] {
        assert!(
            !line.contains(tool),
            "the stale branch must not send the model to {tool}, which would answer STALE"
        );
    }
    assert!(line.contains("behind"), "it must say what it does not know");
    assert!(line.contains("estelle sweep"), "and what would fix it");
}

#[test]
fn a_very_long_needle_is_bounded_before_it_reaches_the_model() {
    let huge = "x".repeat(4096);
    let text = redirect(SearchKind::Content, &huge, "acme/widgets");
    assert!(
        text.len() < 1024,
        "an unbounded echo of the caller's pattern is an unbounded write into the window"
    );
}

/// 🔴 THE FRESHNESS WALK IS BOUNDED BY SESSION, NOT BY CALL. A bounded-but-real tree walk on every
/// `Read` is a hang in the editor, so the measured answer is written even on the turns that say
/// nothing — and a turn that changed nothing writes nothing at all.
#[test]
fn a_silent_turn_persists_a_freshly_measured_index_answer_and_nothing_else() {
    let dir = tempfile::tempdir().expect("state");
    let mut measured = PullState {
        fresh: Some(false),
        ..PullState::default()
    };
    assert_eq!(
        commit(
            PullVerdict::Silent,
            SearchKind::Content,
            &mut measured,
            Some(dir.path()),
            "s1",
            true
        ),
        Persisted::Yes
    );
    assert_eq!(
        load_state(Some(dir.path()), "s1", unix_now()).fresh,
        Some(false),
        "without this the next read pays the tree walk again"
    );

    assert_eq!(
        commit(
            PullVerdict::Silent,
            SearchKind::Content,
            &mut PullState::default(),
            Some(dir.path()),
            "s2",
            false
        ),
        Persisted::Yes
    );
    assert!(
        !dir.path().join("pull-s2.json").exists(),
        "a turn that changed nothing must not write"
    );
}

/// 🔴 `prune` RUNS AGAINST A DIRECTORY AN ENVIRONMENT VARIABLE CHOSE. It must delete only files this
/// module wrote, whatever else is sitting beside them.
#[test]
fn pruning_never_touches_a_file_this_module_did_not_write() {
    let dir = tempfile::tempdir().expect("state");
    let bystander = dir.path().join("package.json");
    std::fs::write(&bystander, "{}").expect("write");
    for n in 0..(STATE_KEEP + 5) {
        assert!(save_state(
            Some(dir.path()),
            &format!("s{n:04}"),
            &PullState::default()
        ));
    }
    assert!(
        bystander.exists(),
        "a bystander .json must survive the prune"
    );
    let ours = std::fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(STATE_FILE_PREFIX)
        })
        .count();
    assert!(ours <= STATE_KEEP, "the directory is bounded: {ours} files");
}

// ── the three defects a rival reviewer found on the PR ───────────────────────────────────────

/// 🔴 EVERY SHAPE THAT NAMES A SCOPE MUST CARRY IT. Only `Read` was bounded to the indexed tree, so
/// a `Grep` aimed at `/etc` collected a redirect claiming the graph could answer it.
#[test]
fn a_search_that_named_where_to_look_carries_that_scope() {
    assert_eq!(
        intent("Grep", json!({"pattern": "x", "path": "/etc"})).and_then(|f| f.scope),
        Some("/etc".to_string())
    );
    assert_eq!(
        intent("Glob", json!({"pattern": "*.conf", "path": "/etc"})).and_then(|f| f.scope),
        Some("/etc".to_string())
    );
    // `grep PATTERN [PATH…]` — the SECOND operand is the path, the first is the pattern.
    assert_eq!(
        bash("grep -rn ground src/").and_then(|f| f.scope),
        Some("src/".to_string())
    );
    assert_eq!(
        bash("sudo grep -rn x /etc").and_then(|f| f.scope),
        Some("/etc".to_string())
    );
    assert_eq!(
        bash("find /etc -name '*.conf'").and_then(|f| f.scope),
        Some("/etc".to_string())
    );
    // 🔴 NAMING NO SCOPE IS NOT NAMING THE ROOT. A bare `grep -rn foo` searches the cwd, which the
    // caller already bounds — so `None` must NOT be spelled the same way as an explicit path.
    assert_eq!(bash("grep -rn foo").and_then(|f| f.scope), None);
    assert_eq!(
        intent("Grep", json!({"pattern": "x"})).and_then(|f| f.scope),
        None
    );
}

/// 🔴 A CACHED FRESHNESS ANSWER IS A CLAIM ABOUT A PAST. Past its TTL it must be discarded, not
/// re-presented as the present.
#[test]
fn a_freshness_answer_older_than_its_ttl_is_not_reused() {
    let dir = tempfile::tempdir().expect("state");
    let state = PullState {
        told: vec![("content".to_string(), 1)],
        blocked: true,
        told_stale: false,
        fresh: Some(true),
    };
    assert!(save_state(Some(dir.path()), "ttl", &state));
    let now = unix_now();
    assert_eq!(
        load_state(Some(dir.path()), "ttl", now).fresh,
        Some(true),
        "inside the window it is reused"
    );

    // (That the two windows differ at all is a COMPILE-TIME guard in `hook_pull.rs`; asserting it
    // here would be a constant comparison, which is decoration.)
    let aged = load_state(Some(dir.path()), "ttl", now + FRESHNESS_TTL_S + 1);
    assert_eq!(
        aged.fresh, None,
        "past the window the index answer must be re-measured, not re-asserted"
    );
    // 🔴 AND ONLY THAT FIELD EXPIRES. How often we already spoke, and whether the one block was
    // spent, are facts about THIS SESSION and do not go stale — expiring them would hand the
    // session a second refusal, which is the wedge the whole design exists to prevent.
    assert!(
        aged.blocked,
        "the spent block must survive the freshness expiry"
    );
    assert_eq!(aged.count(SearchKind::Content), 1, "so must the counters");
}

/// 🔴 ONE REFUSAL PER SESSION MUST HOLD ACROSS PROCESSES, NOT JUST ACROSS CALLS. `blocked` in a
/// JSON file is a read-modify-write; two concurrent hooks could both read `false` and both deny.
#[test]
fn only_one_caller_can_ever_claim_the_session_block() {
    let dir = tempfile::tempdir().expect("state");
    assert!(claim_block(Some(dir.path()), "race"), "the first wins");
    for attempt in 0..8 {
        assert!(
            !claim_block(Some(dir.path()), "race"),
            "attempt {attempt} must lose — O_EXCL admits exactly one"
        );
    }
    // A different session is a different claim.
    assert!(claim_block(Some(dir.path()), "other"));
    // And a session with no usable id can never claim, so it can never refuse.
    assert!(!claim_block(Some(dir.path()), "   "));
}
