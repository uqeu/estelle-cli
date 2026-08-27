use crate::{CommandFailure, WorkProgressSink};
use estelle_client::{Client, CommandReply, Error, JobSnapshot};
use tokio_util::sync::CancellationToken;

const LEGACY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

pub(crate) async fn watch(
    client: &Client,
    job_id: &str,
    cancel: &CancellationToken,
    progress_sink: Option<WorkProgressSink>,
) -> Result<CommandReply, CommandFailure> {
    let sink = progress_sink.clone();
    let terminal = match client
        .stream_job(job_id, cancel, move |snapshot| {
            if let Some(progress) = snapshot.progress.clone()
                && let Some(sink) = &sink
            {
                sink(progress);
            }
        })
        .await
    {
        Ok(snapshot) => snapshot,
        // An older server has only the durable snapshot door. Keep that compatibility path explicit;
        // every server carrying the event route uses the stream above and does not wake every 500 ms.
        Err(Error::Http { status, .. }) if status.as_u16() == 404 => {
            return poll_legacy(client, job_id, cancel, progress_sink).await;
        }
        Err(error) => return Err(CommandFailure::Client(error)),
    };
    terminal_reply(terminal, job_id)
}

async fn poll_legacy(
    client: &Client,
    job_id: &str,
    cancel: &CancellationToken,
    progress_sink: Option<WorkProgressSink>,
) -> Result<CommandReply, CommandFailure> {
    loop {
        let snapshot = client
            .job(job_id, cancel)
            .await
            .map_err(CommandFailure::Client)?;
        if let Some(progress) = snapshot.progress.clone()
            && let Some(sink) = &progress_sink
        {
            sink(progress);
        }
        if snapshot.terminal {
            return terminal_reply(snapshot, job_id);
        }
        tokio::select! {
            () = cancel.cancelled() => return Err(CommandFailure::Client(Error::Cancelled)),
            () = tokio::time::sleep(LEGACY_POLL_INTERVAL) => {}
        }
    }
}

fn terminal_reply(snapshot: JobSnapshot, job_id: &str) -> Result<CommandReply, CommandFailure> {
    if snapshot.state == "done" {
        let result = snapshot.result.ok_or_else(|| {
            CommandFailure::Local([
                "/work finished without a result.".to_string(),
                format!("Durable job {job_id} reported state=done but no result body."),
                "The job was read from the server; retry /work or inspect its server receipt."
                    .to_string(),
            ])
        })?;
        return serde_json::from_value(result).map_err(|error| {
            CommandFailure::Local([
                "/work returned an unreadable terminal result.".to_string(),
                error.to_string(),
                format!("The durable job is {job_id}; no local result was invented."),
            ])
        });
    }
    Err(CommandFailure::Local([
        format!("/work stopped in durable state {}.", snapshot.state),
        if snapshot.reason.trim().is_empty() {
            "The server returned no failure reason.".to_string()
        } else {
            snapshot.reason
        },
        format!("The caller-bound job stream was GET /jobs/{job_id}/events."),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PendingCommand, execute_remote_command};
    use estelle_client::Repo;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn receipt_streams_phase_events_to_a_terminal_result() {
        let server = MockServer::start().await;
        let job_id = "job_0123456789abcdef01234567";
        Mock::given(method("POST"))
            .and(path("/work"))
            .respond_with(ResponseTemplate::new(202).set_body_json(json!({
                "accepted": true,
                "job_id": job_id,
                "poll": format!("GET /jobs/{job_id}")
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/jobs/{job_id}/events")))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                "{}\n{}\n",
                json!({"event": "progress", "snapshot": {
                    "job_id": job_id, "state": "running", "terminal": false,
                    "progress": {"revision": 1,
                        "work": {"phase": "scope", "phases": {"scope": 0.4}, "elapsed_s": 0.4},
                        "plan": {"revision": 1, "steps": [{
                            "id": "inspect", "step": "Inspect parser", "status": "active",
                            "evidence": "parser.py:parse"
                        }]}
                    }
                }}),
                json!({"event": "complete", "snapshot": {
                    "job_id": job_id, "state": "done", "terminal": true,
                    "progress": {"revision": 6, "work": {
                        "phase": "gate",
                        "phases": {"scope": 0.4, "recall": 1.2, "conventions": 0.2, "prompt": 0.1, "implement": 3.0, "gate": 0.7},
                        "elapsed_s": 5.6
                    }},
                    "result": {"answer": "work complete", "diff": "diff --git a/a b/a"}
                }})
            )))
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: WorkProgressSink = Arc::new(move |progress| {
            sink_seen.lock().expect("progress sink").push(progress);
        });

        let result = execute_remote_command(
            client,
            Repo::new("fatelabs/estelle").expect("repo"),
            tempfile::tempdir().expect("root").path().to_path_buf(),
            PendingCommand {
                name: "work",
                argument: "repair parser".to_string(),
                last_question: None,
                skill_thread: None,
            },
            &CancellationToken::new(),
            Some(sink),
        )
        .await
        .expect("terminal work reply");

        assert_eq!(result.reply.answer.as_deref(), Some("work complete"));
        let seen = seen.lock().expect("progress samples");
        assert_eq!(seen.len(), 2);
        assert_eq!(
            (seen[0].revision, seen[0].work.phase.as_str()),
            (1, "scope")
        );
        assert_eq!(
            seen[0].plan.as_ref().expect("streamed plan").steps[0].id,
            "inspect"
        );
        assert_eq!((seen[1].revision, seen[1].work.phase.as_str()), (6, "gate"));
    }
}
