//! Tests for the SessionEnd handoff.
//!
//! Every test here asserts on the INNERMOST observable it can reach. `record` returning `Ok`
//! is a claim about the CALL, never about a note being on disk, so no test stops at the
//! return value — each one reads the store back and asserts on its contents.

use super::*;

fn at(minute: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-09-05T04:{minute:02}:00Z"))
        .expect("fixture timestamp")
        .with_timezone(&Utc)
}

fn note(session: &str) -> PendingCheckpoint {
    PendingCheckpoint {
        session_id: session.to_string(),
        transcript_path: format!("/tmp/rollout-{session}.jsonl"),
        cwd: "/Users/someone/repo".to_string(),
        event: DEFERRED_EVENT.to_string(),
        at: String::new(),
    }
}

fn store_on_disk(path: &Path) -> HandoffStore {
    read_store(path).expect("the store must parse")
}

#[tokio::test]
async fn a_recorded_handoff_survives_to_disk_and_names_the_transcript() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("nested").join("pending-checkpoints.json");

    record(note("session-1"), state.clone(), at(10))
        .await
        .expect("the handoff must be written");

    // NOT `is_ok()`: the return value is a claim about the call. The note itself is the fact.
    let store = store_on_disk(&state);
    let entry = store.get("session-1").expect("the note must be keyed by session id");
    assert_eq!(entry.transcript_path, "/tmp/rollout-session-1.jsonl");
    assert_eq!(entry.cwd, "/Users/someone/repo");
    assert_eq!(entry.event, DEFERRED_EVENT);
    assert_eq!(
        entry.at, "2026-09-05T04:10:00.000Z",
        "the note records when the SESSION ENDED, not when the upload happens"
    );
}

/// 🔴 THE PAIR THAT CANNOT BOTH BE TRUE: a handoff is recorded **and** the transcript was read.
///
/// The transcript path here names a file that DOES NOT EXIST. If any future edit makes the
/// deferred path open, stat, parse or size-check the transcript, this test goes red — which is
/// the entire contract, because reading the transcript is the thing that does not fit in three
/// seconds. A timing assertion alone could not catch it on a fast machine with a small file.
#[tokio::test]
async fn the_deferred_path_never_touches_the_transcript() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    let mut pending = note("session-unreadable");
    pending.transcript_path = root
        .path()
        .join("no-such-rollout.jsonl")
        .to_string_lossy()
        .into_owned();
    assert!(
        !Path::new(&pending.transcript_path).exists(),
        "the fixture is only meaningful while this path is absent"
    );

    record(pending.clone(), state.clone(), at(10))
        .await
        .expect("a missing transcript must NOT stop the handoff — the path is a pointer, not a read");

    assert_eq!(
        store_on_disk(&state)["session-unreadable"].transcript_path,
        pending.transcript_path,
        "the note carries the path verbatim, so the next session can read what this one could not"
    );
}

/// Consuming twice must not double-write a session.
#[tokio::test]
async fn a_claimed_handoff_is_gone_so_a_second_claim_uploads_nothing() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    record(note("session-1"), state.clone(), at(10))
        .await
        .expect("record");

    let first = claim(state.clone(), at(20)).await;
    assert_eq!(first.len(), 1, "the owed checkpoint is handed over once");
    assert_eq!(first[0].session_id, "session-1");

    let second = claim(state.clone(), at(21)).await;
    assert!(
        second.is_empty(),
        "a second claim must hand over NOTHING — at-most-once is what keeps a deferred \
         checkpoint from being uploaded twice"
    );
    assert!(
        !state.exists() || store_on_disk(&state).is_empty(),
        "the claimed note is removed from the store before any upload is attempted"
    );
}

#[tokio::test]
async fn a_claim_drains_oldest_first_and_leaves_the_rest_on_disk() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    let total = MAX_CLAIM_PER_START + 2;
    for index in 0..total {
        record(
            note(&format!("session-{index:02}")),
            state.clone(),
            at(10 + u32::try_from(index).expect("small index")),
        )
        .await
        .expect("record");
    }

    let claimed = claim(state.clone(), at(50)).await;
    assert_eq!(
        claimed.len(),
        MAX_CLAIM_PER_START,
        "one returning session inherits at most MAX_CLAIM_PER_START uploads"
    );
    assert_eq!(
        claimed[0].session_id, "session-00",
        "the checkpoint owed longest is paid first"
    );

    let left = store_on_disk(&state);
    assert_eq!(
        left.len(),
        total - MAX_CLAIM_PER_START,
        "what this session will not drain stays durable for the next one"
    );
    assert!(
        !left.contains_key("session-00"),
        "a claimed note is never left behind to be claimed again"
    );
}

#[tokio::test]
async fn the_store_is_bounded_so_notes_nobody_claims_cannot_accumulate() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    let overflow = MAX_PENDING + 5;
    for index in 0..overflow {
        record(
            note(&format!("session-{index:03}")),
            state.clone(),
            at(u32::try_from(index).expect("small index")),
        )
        .await
        .expect("record");
    }

    let store = store_on_disk(&state);
    assert_eq!(
        store.len(),
        MAX_PENDING,
        "the store holds MAX_PENDING notes however many sessions end"
    );
    assert!(
        store.contains_key(&format!("session-{:03}", overflow - 1)),
        "the newest note is the one that must never be evicted"
    );
    assert!(
        !store.contains_key("session-000"),
        "the oldest note is the one evicted"
    );
}

#[tokio::test]
async fn a_note_older_than_the_age_bound_is_dropped_rather_than_uploaded() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    record(note("stale"), state.clone(), at(10))
        .await
        .expect("record");

    let long_after = at(10) + chrono::Duration::seconds(MAX_AGE_SECONDS + 1);
    assert!(
        claim(state.clone(), long_after).await.is_empty(),
        "a checkpoint owed for longer than MAX_AGE_SECONDS is abandoned, not resurrected"
    );

    // And the same bound applies on the WRITE side, so a store of stale notes self-empties.
    record(note("fresh"), state.clone(), long_after)
        .await
        .expect("record");
    let store = store_on_disk(&state);
    assert_eq!(store.len(), 1, "the stale note did not survive the next write");
    assert!(store.contains_key("fresh"));
}

/// A refusal must NAME the missing thing. This is the verb whose silence would cost a customer
/// their session memory without telling them, so "it failed" is not an acceptable answer.
#[tokio::test]
async fn every_refusal_names_the_field_it_is_missing() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");

    for (field, mutate) in [
        ("session_id", 0usize),
        ("transcript_path", 1),
        ("cwd", 2),
        ("hook_event_name", 3),
    ] {
        let mut pending = note("session-1");
        match mutate {
            0 => pending.session_id = "   ".to_string(),
            1 => pending.transcript_path = String::new(),
            2 => pending.cwd = String::new(),
            _ => pending.event = String::new(),
        }
        let error = record(pending, state.clone(), at(10))
            .await
            .expect_err("a payload missing a required field must be refused");
        assert_eq!(error, HandoffError::MissingField(field));
        assert!(
            error.to_string().contains(field),
            "the rendered reason must name {field}; got {error}"
        );
    }

    assert!(
        !state.exists(),
        "a refused handoff writes nothing — a half-note is worse than no note"
    );
}

#[tokio::test]
async fn an_unwritable_store_is_reported_with_its_reason_not_swallowed() {
    let root = tempfile::tempdir().expect("handoff root");
    // A FILE where the store's parent directory must be: `create_dir_all` cannot succeed.
    let blocker = root.path().join("blocked");
    fs::write(&blocker, b"not a directory").expect("fixture blocker");
    let state = blocker.join("pending-checkpoints.json");

    let error = record(note("session-1"), state, at(10))
        .await
        .expect_err("an unwritable store must be an error, never a silent no-op");
    assert!(
        matches!(error, HandoffError::Unwritable(_)),
        "got {error:?}"
    );
    assert!(
        !error.to_string().is_empty(),
        "the reason travels with the refusal"
    );
}

#[test]
fn a_corrupt_store_is_refused_rather_than_read_unbounded() {
    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    fs::write(&state, b"{ this is not json").expect("fixture");
    assert!(
        read_store(&state).is_err(),
        "unparseable content is an error, not an empty store"
    );

    let oversized = root.path().join("oversized.json");
    fs::write(&oversized, vec![b'x'; usize::try_from(MAX_STATE_BYTES).expect("cap") + 1])
        .expect("fixture");
    assert!(
        read_store(&oversized).is_err(),
        "the size bound is checked BEFORE the read, so a huge file is never loaded"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn the_store_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("handoff root");
    let state = root.path().join("pending-checkpoints.json");
    record(note("session-1"), state.clone(), at(10))
        .await
        .expect("record");

    let mode = fs::metadata(&state).expect("store metadata").permissions().mode();
    assert_eq!(
        mode & 0o077,
        0,
        "the store lists the customer's repositories; group and other get nothing"
    );
}
