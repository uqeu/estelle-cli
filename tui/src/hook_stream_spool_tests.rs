use super::*;
use crate::hook_stream_run::{Dialect, StreamInput, run};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Mutex;

const SESSION: &str = "5p001ed0-aaaa-bbbb-cccc-1234567890ab";
#[cfg(unix)] // only the unix-only permission test posts to a named timeline
const TIMELINE: &str = "claude-code-5p001ed0-aaaa-bbbb-cccc-1234567890ab";
const SAID: &str = "the founder said this during the outage";

/// A scripted server: answers from `script` in order, then accepts. Records what it ACCEPTED.
#[derive(Default)]
struct Server {
    script: Mutex<VecDeque<SinkError>>,
    accepted: Mutex<Vec<(String, Vec<Value>)>>,
    calls: Mutex<usize>,
}

impl Server {
    fn failing(errors: Vec<SinkError>) -> Self {
        Self {
            script: Mutex::new(errors.into()),
            ..Self::default()
        }
    }
    fn down() -> Self {
        Self::failing((0..10_000).map(|_| away()).collect())
    }
    fn texts(&self) -> Vec<String> {
        self.accepted
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, events)| events.iter())
            .map(Value::to_string)
            .collect()
    }
    fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

impl StreamSink for Server {
    async fn post(&self, session_id: &str, events: &[Value]) -> Result<(), SinkError> {
        *self.calls.lock().unwrap() += 1;
        if let Some(error) = self.script.lock().unwrap().pop_front() {
            return Err(error);
        }
        self.accepted
            .lock()
            .unwrap()
            .push((session_id.to_string(), events.to_vec()));
        Ok(())
    }
}

fn away() -> SinkError {
    SinkError::Unavailable(
        "error sending request for url (https://127.0.0.1:9/session/events)".into(),
    )
}

fn refused(status: u16) -> SinkError {
    SinkError::Refused {
        status,
        message: "no".into(),
    }
}

struct Fx {
    dir: tempfile::TempDir,
    transcript: PathBuf,
}

impl Fx {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join(format!("{SESSION}.jsonl"));
        let lines = [
            json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-26T10:00:00Z", "message": {"content": SAID}}),
            json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-26T10:00:01Z", "message": {"content": [{"type": "text", "text": "and the agent answered"}]}}),
        ];
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&transcript, body).unwrap();
        Self { dir, transcript }
    }
    fn state(&self) -> PathBuf {
        self.dir.path().join("state")
    }
    fn spool(&self) -> PathBuf {
        self.state().join("spool")
    }
    async fn hook<S: StreamSink>(
        &self,
        event: &str,
        sink: &S,
    ) -> crate::hook_stream_run::StreamReport {
        let state = self.state();
        let input = StreamInput {
            state_dir: &state,
            transcript: &self.transcript,
            session: SESSION,
            hook_event: event,
            repo: "uqeu/estelle",
            now: DateTime::parse_from_rfc3339("2026-10-26T10:05:00Z")
                .unwrap()
                .with_timezone(&Utc),
            dialect: Dialect::Claude,
            git_root: None,
        };
        run(&input, sink).await
    }
}

/// THE DISCRIMINATING PAIR. The outage covers the session's LAST hook and the transcript is then deleted.
/// Without the spool the words are gone (nothing ever runs that session again, and the only copy was the
/// transcript); with it they land on the next drain — triggered by any session. Same inputs, opposite answers.
#[tokio::test]
async fn an_outage_on_the_last_hook_is_replayed_even_after_the_transcript_is_deleted() {
    for spooled in [false, true] {
        let fx = Fx::new();
        let down = Server::down();
        if spooled {
            let sink = SpoolingSink::new(&down, &fx.spool());
            let report = fx.hook("Stop", &sink).await;
            assert_eq!(report.error, None, "a spooled batch counts as landed");
            assert!(sink.spooled() > 0);
        } else {
            let report = fx.hook("Stop", &down).await;
            assert!(report.error.is_some());
        }
        std::fs::remove_file(&fx.transcript).unwrap();

        let healthy = Server::default();
        let drained = drain(&fx.spool(), &healthy).await;
        let landed = healthy.texts().iter().any(|t| t.contains(SAID));
        assert_eq!(landed, spooled, "spooled={spooled}: {drained:?}");
        if spooled {
            assert!(drained.replayed > 0);
            assert_eq!(drained.still_owed, 0, "a replayed file is removed");
            assert!(
                healthy.texts().iter().any(|t| t.contains("input_required")),
                "the Stop the host reported during the outage is kept too"
            );
        }
    }
}

#[tokio::test]
async fn a_drain_while_the_server_is_still_down_keeps_every_file_and_stops_early() {
    let fx = Fx::new();
    let down = Server::down();
    fx.hook("Stop", &SpoolingSink::new(&down, &fx.spool()))
        .await;
    let owed = listing(&fx.spool()).len();
    assert!(owed > 0);

    let still_down = Server::down();
    let report = drain(&fx.spool(), &still_down).await;
    assert_eq!(report.replayed, 0);
    assert_eq!(
        report.still_owed, owed,
        "nothing is dropped while the server is away"
    );
    assert_eq!(still_down.calls(), 1, "one failed probe stops the drain");
}

#[tokio::test]
async fn a_full_spool_falls_back_to_the_offset_rule_never_to_a_drop() {
    let fx = Fx::new();
    std::fs::create_dir_all(fx.spool()).unwrap();
    for i in 0..MAX_SPOOL_FILES {
        std::fs::write(fx.spool().join(format!("{i:020}.json")), "{}").unwrap();
    }
    let down = Server::down();
    let sink = SpoolingSink::new(&down, &fx.spool());
    let report = fx.hook("Stop", &sink).await;
    assert!(
        report.error.is_some(),
        "a full spool returns the original error"
    );
    assert_eq!(sink.spooled(), 0);
    let state: Value = serde_json::from_slice(
        &std::fs::read(fx.state().join(format!("{SESSION}.stream.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        state["offset"], 0,
        "the offset stays, so the transcript is still the spool"
    );
}

#[tokio::test]
async fn a_400_naming_an_event_is_not_spooled_so_the_runner_can_drop_exactly_that_event() {
    let fx = Fx::new();
    let server = Server::failing(vec![SinkError::Refused {
        status: 400,
        message: "event 1: bad".into(),
    }]);
    let sink = SpoolingSink::new(&server, &fx.spool());
    let report = fx.hook("Stop", &sink).await;
    assert_eq!(report.error, None);
    assert_eq!(report.dropped, 1);
    assert_eq!(sink.spooled(), 0);
    assert!(listing(&fx.spool()).is_empty());
}

#[tokio::test]
async fn a_batch_the_server_keeps_refusing_is_buried_not_retried_forever_and_does_not_block_others()
{
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("spool");
    let down = Server::down();
    let sink = SpoolingSink::new(&down, &spool);
    sink.post("claude-code-a", &[json!({"id": "poison"})])
        .await
        .unwrap();
    sink.post("claude-code-b", &[json!({"id": "fine"})])
        .await
        .unwrap();
    assert_eq!(listing(&spool).len(), 2);

    for round in 1..=MAX_REFUSALS {
        let server = Server::failing(vec![refused(500)]);
        let report = drain(&spool, &server).await;
        if round == 1 {
            assert_eq!(
                report.replayed, 1,
                "the refusal of the first file did not block the second"
            );
            assert!(server.texts().iter().any(|t| t.contains("fine")));
        }
        let buried = round == MAX_REFUSALS;
        assert_eq!(report.dead, usize::from(buried), "round {round}");
    }
    assert!(listing(&spool).is_empty());
    assert_eq!(
        listing(&spool.join(DEAD_DIR)).len(),
        1,
        "kept on disk, not deleted"
    );
    assert_eq!(
        usage(&spool).1,
        1,
        "a dead file still counts against the bound"
    );
}

#[tokio::test]
async fn an_unreadable_spool_file_is_buried_and_the_rest_still_drain() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("spool");
    std::fs::create_dir_all(&spool).unwrap();
    std::fs::write(spool.join("00000000000000000000-torn.json"), "{not json").unwrap();
    let down = Server::down();
    SpoolingSink::new(&down, &spool)
        .post("claude-code-a", &[json!({"id": "ok"})])
        .await
        .unwrap();
    let server = Server::default();
    let report = drain(&spool, &server).await;
    assert_eq!((report.replayed, report.dead, report.still_owed), (1, 1, 0));
}

#[tokio::test]
async fn one_drain_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("spool");
    let down = Server::down();
    let sink = SpoolingSink::new(&down, &spool);
    for i in 0..(MAX_DRAIN_FILES + 5) {
        sink.post("claude-code-a", &[json!({"id": i})])
            .await
            .unwrap();
    }
    let report = drain(&spool, &Server::default()).await;
    assert_eq!(report.replayed, MAX_DRAIN_FILES);
    assert_eq!(report.still_owed, 5);
}

#[test]
fn the_failure_classes_are_the_ones_that_are_about_the_server() {
    assert!(retryable(&away()));
    for status in [401, 403, 408, 429, 500, 502, 503, 504] {
        assert!(retryable(&refused(status)), "{status}");
    }
    for status in [400, 404, 413, 422] {
        assert!(!retryable(&refused(status)), "{status}");
    }
    assert!(server_away(&away()));
    assert!(server_away(&refused(503)));
    assert!(
        !server_away(&refused(500)),
        "a 500 is charged to the batch that drew it"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn spool_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("spool");
    let down = Server::down();
    SpoolingSink::new(&down, &spool)
        .post(TIMELINE, &[json!({"id": "x"})])
        .await
        .unwrap();
    let file = &listing(&spool)[0];
    let mode = std::fs::metadata(file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}
