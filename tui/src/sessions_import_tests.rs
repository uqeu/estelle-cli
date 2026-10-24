use super::*;
use crate::hook_stream::{MAX_BATCH_EVENTS, Notes, events_of};
use crate::hook_stream_codex::codex_events_of;
use serde_json::json;
use std::sync::Mutex;

const CLAUDE_SESSION: &str = "0b1c2d3e-aaaa-bbbb-cccc-1234567890ab";
const CODEX_SESSION: &str = "01a0fea7-5551-7e22-908f-61a903725688";

/// A GitHub-token-shaped string, assembled at run time so no credential-shaped literal is committed.
fn token() -> String {
    format!("gh{}_{}", "p", "Q7".repeat(18))
}

/// Records every batch it is sent, accepts all of them.
#[derive(Default)]
struct Recorder {
    posts: Mutex<Vec<(String, Vec<Value>)>>,
}

impl Recorder {
    fn events(&self) -> Vec<(String, Value)> {
        self.posts
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(s, e)| e.iter().map(|e| (s.clone(), e.clone())))
            .collect()
    }
}

impl StreamSink for Recorder {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        self.posts
            .lock()
            .unwrap()
            .push((session_id.to_string(), events.to_vec()));
        Ok(())
    }
}

/// A server that is down.
struct Down;

impl StreamSink for Down {
    async fn post(&self, _session_id: &str, _events: &[Value]) -> Result<(), SinkError> {
        Err(SinkError::Unavailable("connection refused".into()))
    }
}

struct Machine {
    home: tempfile::TempDir,
    roots: Roots,
    state: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let roots = Roots {
            claude_projects: home.path().join(".claude/projects"),
            codex_sessions: home.path().join(".codex/sessions"),
        };
        let state = home.path().join(".estelle/stream");
        Self { home, roots, state }
    }
    fn state_dir(&self) -> PathBuf {
        self.state.clone()
    }
    fn write(path: &Path, lines: &[Value]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(path, body).unwrap();
    }
    fn claude(&self, session: &str, lines: &[Value]) -> PathBuf {
        let path = self
            .roots
            .claude_projects
            .join("-Users-me-repo")
            .join(format!("{session}.jsonl"));
        Self::write(&path, lines);
        path
    }
    fn codex(&self, session: &str, lines: &[Value]) -> PathBuf {
        let path = self
            .roots
            .codex_sessions
            .join("2026/10/02")
            .join(format!("rollout-2026-10-02T18-06-14-{session}.jsonl"));
        Self::write(&path, lines);
        path
    }
    fn input(&self, dry_run: bool) -> ImportInput<'_> {
        ImportInput {
            roots: &self.roots,
            state_dir: &self.state,
            source: Source::All,
            since: None,
            dry_run,
            now: "2026-10-24T12:00:00Z".parse().unwrap(),
            limits: Limits::DEFAULT,
        }
    }
}

fn claude_lines(cwd: &Path) -> Vec<Value> {
    let cwd = cwd.to_string_lossy();
    vec![
        json!({"type": "user", "uuid": "u-1", "timestamp": "2026-09-01T09:00:00.000Z", "cwd": cwd,
               "message": {"role": "user", "content": format!("deploy with {} please", token())}}),
        json!({"type": "assistant", "uuid": "a-1", "timestamp": "2026-09-01T09:00:05.000Z", "cwd": cwd,
        "message": {"role": "assistant", "content": [
            {"type": "text", "text": "Reading the config."},
            {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": "/repo/a.rs"}},
        ]}}),
        json!({"type": "user", "uuid": "u-2", "timestamp": "2026-09-01T09:00:06.000Z", "cwd": cwd,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "content": format!("TOKEN={}", token())},
        ]}}),
        json!({"type": "system", "subtype": "turn_duration", "uuid": "s-1", "timestamp": "2026-09-01T09:00:09.000Z"}),
    ]
}

fn codex_lines(cwd: &Path) -> Vec<Value> {
    let cwd = cwd.to_string_lossy();
    vec![
        json!({"timestamp": "2026-10-02T18:06:14.000Z", "type": "session_meta",
               "payload": {"id": CODEX_SESSION, "cwd": cwd, "source": "vscode"}}),
        json!({"timestamp": "2026-10-02T18:06:20.000Z", "type": "response_item",
               "payload": {"type": "message", "role": "user",
                           "content": [{"type": "input_text", "text": format!("why is {} in the log", token())}]}}),
        json!({"timestamp": "2026-10-02T18:06:30.000Z", "type": "response_item",
               "payload": {"type": "message", "role": "assistant",
                           "content": [{"type": "output_text", "text": "Rotating it."}]}}),
        json!({"timestamp": "2026-10-02T18:06:40.000Z", "type": "event_msg", "payload": {"type": "task_complete"}}),
    ]
}

/// The ids and instants the LIVE hook's mappers produce for these lines — the contract the import must meet.
fn hook_ids(path: &Path, codex: bool) -> Vec<(String, String)> {
    let body = std::fs::read(path).unwrap();
    let mut notes = Notes::default();
    let mut offset = 0u64;
    let mut out = Vec::new();
    for line in body.split_inclusive(|b| *b == b'\n') {
        let entry: Value = serde_json::from_slice(&line[..line.len() - 1]).unwrap();
        let events = if codex {
            codex_events_of(&entry, offset, &mut notes)
        } else {
            events_of(&entry, &mut notes, false)
        };
        out.extend(events.iter().map(|e| {
            (
                e["event_id"].as_str().unwrap().to_string(),
                e["at"].as_str().unwrap().to_string(),
            )
        }));
        offset += line.len() as u64;
    }
    assert!(!out.is_empty(), "the fixture maps to events");
    out
}

fn sent_ids(recorder: &Recorder, timeline: &str) -> Vec<(String, String)> {
    recorder
        .events()
        .into_iter()
        // `started` is the runner's own first event, not a mapped entry; everything else must match.
        .filter(|(s, e)| s == timeline && e["event_id"] != format!("{timeline}:started"))
        .map(|(_, e)| {
            (
                e["event_id"].as_str().unwrap().to_string(),
                e["at"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn import_posts_the_hooks_own_ids_under_the_hooks_timeline_id_with_real_timestamps() {
    let machine = Machine::new();
    let claude = machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    let codex = machine.codex(CODEX_SESSION, &codex_lines(machine.home.path()));
    let recorder = Recorder::default();
    let summary = import(&machine.input(false), &recorder).await;

    let claude_tl = format!("claude-code-{CLAUDE_SESSION}");
    let codex_tl = format!("codex-{CODEX_SESSION}");
    assert_eq!(sent_ids(&recorder, &claude_tl), hook_ids(&claude, false));
    assert_eq!(sent_ids(&recorder, &codex_tl), hook_ids(&codex, true));
    for (timeline, first) in [
        (&claude_tl, "2026-09-01T09:00:00.000Z"),
        (&codex_tl, "2026-10-02T18:06:20.000Z"),
    ] {
        let started: Vec<Value> = recorder
            .events()
            .into_iter()
            .filter(|(s, e)| s == timeline && e["event_id"] == format!("{timeline}:started"))
            .map(|(_, e)| e)
            .collect();
        assert_eq!(started.len(), 1, "one started event for {timeline}");
        assert_eq!(
            started[0]["at"], first,
            "started at the first entry, not the clock"
        );
    }
    // Nothing is stamped with the import's own clock.
    assert!(
        recorder
            .events()
            .iter()
            .all(|(_, e)| !e["at"].as_str().unwrap().starts_with("2026-10-24"))
    );
    assert!(
        summary
            .rows
            .iter()
            .all(|r| matches!(r.outcome, Outcome::Imported { .. })),
        "{summary:?}"
    );
}

#[tokio::test]
async fn a_second_import_posts_nothing_and_a_later_live_run_repeats_nothing() {
    let machine = Machine::new();
    let claude = machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    machine.codex(CODEX_SESSION, &codex_lines(machine.home.path()));
    let first = Recorder::default();
    import(&machine.input(false), &first).await;
    assert!(!first.events().is_empty());

    let second = Recorder::default();
    let summary = import(&machine.input(false), &second).await;
    assert!(second.events().is_empty(), "re-sent: {:?}", second.events());
    assert!(
        summary
            .rows
            .iter()
            .all(|r| r.outcome == Outcome::AlreadyPresent),
        "{summary:?}"
    );

    // The live hook fires Stop on the same session: it sends its own derived Stop and no transcript event.
    let live = Recorder::default();
    let state_dir = machine.state_dir();
    let input = StreamInput {
        state_dir: &state_dir,
        transcript: &claude,
        session: CLAUDE_SESSION,
        hook_event: "Stop",
        repo: "me/repo",
        now: "2026-10-24T12:00:00Z".parse().unwrap(),
        dialect: Dialect::Claude,
        git_root: None,
    };
    run(&input, &live).await;
    let imported: Vec<String> = first
        .events()
        .iter()
        .map(|(_, e)| e["event_id"].to_string())
        .collect();
    let repeated: Vec<_> = live
        .events()
        .into_iter()
        .filter(|(_, e)| imported.contains(&e["event_id"].to_string()))
        .collect();
    assert!(
        repeated.is_empty(),
        "the live hook resent imported events: {repeated:?}"
    );
}

#[tokio::test]
async fn no_secret_in_a_transcript_leaves_the_machine() {
    let machine = Machine::new();
    machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    machine.codex(CODEX_SESSION, &codex_lines(machine.home.path()));
    let recorder = Recorder::default();
    import(&machine.input(false), &recorder).await;
    let wire = serde_json::to_string(&recorder.events()).unwrap();
    assert!(!wire.contains(&token()), "a raw token was posted");
    assert!(
        wire.matches("[redacted github token]").count() >= 3,
        "{wire}"
    );
}

#[tokio::test]
async fn a_dry_run_sends_nothing_and_moves_no_offset() {
    let machine = Machine::new();
    machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    machine.codex(CODEX_SESSION, &codex_lines(machine.home.path()));
    let dry = import(&machine.input(true), &Down).await;
    assert!(
        dry.rows
            .iter()
            .all(|r| matches!(r.outcome, Outcome::Imported { events } if events > 0)),
        "{dry:?}"
    );
    assert!(
        !machine.state_dir().exists(),
        "a dry run wrote stream state"
    );
    assert!(summary_lines(&dry, true)[0].starts_with("Would import 2 sessions"));

    let recorder = Recorder::default();
    import(&machine.input(false), &recorder).await;
    assert!(
        !recorder.events().is_empty(),
        "the dry run consumed what the real import should send"
    );
}

#[tokio::test]
async fn a_failed_send_keeps_the_offset_so_a_retry_sends_everything() {
    let machine = Machine::new();
    let claude = machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    let failed = import(&machine.input(false), &Down).await;
    assert!(
        matches!(failed.rows[0].outcome, Outcome::Failed(_)),
        "{failed:?}"
    );
    assert_eq!(streamed_offset(&machine.state_dir(), CLAUDE_SESSION), 0);
    let recorder = Recorder::default();
    import(&machine.input(false), &recorder).await;
    assert_eq!(
        sent_ids(&recorder, &format!("claude-code-{CLAUDE_SESSION}")),
        hook_ids(&claude, false)
    );
}

#[tokio::test]
async fn the_bounds_hold_and_a_partial_session_resumes_where_it_stopped() {
    let machine = Machine::new();
    let many: Vec<Value> = (0..500)
        .map(|i| json!({"type": "assistant", "uuid": format!("a-{i}"), "timestamp": "2026-09-02T10:00:00.000Z",
                        "message": {"role": "assistant", "content": [{"type": "text", "text": format!("step {i}")}]}}))
        .collect();
    machine.codex(CODEX_SESSION, &codex_lines(machine.home.path()));
    // Newest first: the large session is written last so the one-session bound picks it.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let claude = machine.claude(CLAUDE_SESSION, &many);
    let mut input = machine.input(false);
    input.limits = Limits {
        sessions: 1,
        events_per_session: 150,
    };
    let recorder = Recorder::default();
    let summary = import(&input, &recorder).await;
    assert_eq!(summary.rows.len(), 1, "one session per the bound");
    assert_eq!(summary.not_attempted, 1);
    let Outcome::Partial { events } = summary.rows[0].outcome else {
        panic!("expected partial: {summary:?}");
    };
    // Stops at the first hook run that crosses the bound: never more than one run's worth past it.
    assert!(
        (150..150 + crate::hook_stream_run::MAX_EVENTS_PER_RUN + 1).contains(&events),
        "{events}"
    );
    assert!(
        recorder
            .posts
            .lock()
            .unwrap()
            .iter()
            .all(|(_, b)| b.len() <= MAX_BATCH_EVENTS)
    );

    input.limits = Limits::DEFAULT;
    let rest = Recorder::default();
    import(&input, &rest).await;
    let mut all: Vec<_> = sent_ids(&recorder, &format!("claude-code-{CLAUDE_SESSION}"));
    all.extend(sent_ids(&rest, &format!("claude-code-{CLAUDE_SESSION}")));
    assert_eq!(
        all,
        hook_ids(&claude, false),
        "the resumed import sent exactly the remainder"
    );
}

#[tokio::test]
async fn subagent_threads_old_sessions_and_misnamed_files_are_skipped_and_counted() {
    let machine = Machine::new();
    let mut sub = codex_lines(machine.home.path());
    sub[0]["payload"]["source"] = json!({"subagent": {"thread_spawn": {}}});
    sub[0]["payload"]["agent_path"] = json!("/root/worker");
    machine.codex(CODEX_SESSION, &sub);
    machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    machine.claude("not a session id", &claude_lines(machine.home.path()));
    let mut input = machine.input(false);
    input.since = Some(parse_since("2026-10-01").unwrap());
    let recorder = Recorder::default();
    let summary = import(&input, &recorder).await;
    assert!(recorder.events().is_empty());
    let reasons: Vec<&Outcome> = summary.rows.iter().map(|r| &r.outcome).collect();
    assert!(reasons.contains(&&Outcome::Skipped(
        "a Codex subagent thread (the live hook does not mirror these either)"
    )));
    assert!(reasons.contains(&&Outcome::Skipped("last activity is before --since")));
    assert_eq!(summary.unnamed, 1);
    let lines = summary_lines(&summary, false).join("\n");
    assert!(lines.contains("3 skipped."), "{lines}");
}

#[test]
fn a_codex_thread_id_is_read_from_the_rollout_name_only_when_uuid_shaped() {
    let path = |n: &str| PathBuf::from(format!("/s/2026/10/02/{n}"));
    assert_eq!(
        codex_session(&path(&format!(
            "rollout-2026-10-02T18-06-14-{CODEX_SESSION}.jsonl"
        )))
        .as_deref(),
        Some(CODEX_SESSION)
    );
    assert_eq!(
        codex_session(&path("rollout-2026-10-02T18-06-14-short.jsonl")),
        None
    );
    assert_eq!(codex_session(&path("notes.jsonl")), None);
}

#[test]
fn since_takes_a_day_or_an_instant_and_refuses_anything_else() {
    assert_eq!(
        parse_since("2026-10-01").unwrap().to_rfc3339(),
        "2026-10-01T00:00:00+00:00"
    );
    assert_eq!(
        parse_since("2026-10-01T05:00:00-04:00")
            .unwrap()
            .to_rfc3339(),
        "2026-10-01T09:00:00+00:00"
    );
    assert!(parse_since("last tuesday").is_err());
}

#[tokio::test]
async fn setup_offers_the_import_only_while_something_is_unsent() {
    let machine = Machine::new();
    assert_eq!(setup_offer(&machine.roots, &machine.state_dir()), None);
    machine.claude(CLAUDE_SESSION, &claude_lines(machine.home.path()));
    let offer = setup_offer(&machine.roots, &machine.state_dir()).expect("an offer");
    assert!(offer.starts_with("1 session transcripts"), "{offer}");
    assert!(offer.contains("estelle sessions import --dry-run"));
    import(&machine.input(false), &Recorder::default()).await;
    assert_eq!(setup_offer(&machine.roots, &machine.state_dir()), None);
}
