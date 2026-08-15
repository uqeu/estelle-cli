use super::*;

/// The feedback upload transport was REMOVED, not disabled. The inherited fork hardcoded the
/// upstream Sentry ingest DSN (`o33249.ingest.us.sentry.io` — not an Estelle host), and an
/// `include_logs` upload attached the full session rollout: customer conversation content
/// leaving the machine for a host we do not own. No Estelle endpoint accepts this payload, so
/// every `FeedbackUpload` request fails with this explicit error and nothing is sent. The
/// Estelle TUI's `/feedback` command already renders the transport as deleted
/// (`tui/src/commands.rs`). See `docs/P0-AMPUTATION.md`.
#[derive(Clone, Copy, Default)]
pub(crate) struct FeedbackRequestProcessor;

impl FeedbackRequestProcessor {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn feedback_upload(
        &self,
        _params: FeedbackUploadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        Err(invalid_request(
            "feedback upload was removed: the inherited transport pointed at the upstream Sentry ingest host, not an Estelle endpoint — nothing was sent",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn feedback_upload_is_removed_not_disabled_and_sends_nothing() {
        let processor = FeedbackRequestProcessor::new();
        let params = FeedbackUploadParams {
            classification: "bug".to_string(),
            reason: Some("please follow up".to_string()),
            thread_id: None,
            include_logs: true,
            extra_log_files: Some(vec![std::path::PathBuf::from("/tmp/rollout.jsonl")]),
            tags: None,
        };

        let error = processor
            .feedback_upload(params)
            .await
            .expect_err("a removed transport must not accept an upload");
        let message = format!("{error:?}");
        assert!(message.contains("removed"), "unexpected error: {message}");
        assert!(
            message.contains("nothing was sent"),
            "the error must say nothing left the machine: {message}"
        );
    }
}
