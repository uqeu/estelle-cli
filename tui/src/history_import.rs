//! Customer-facing history import for the server-owned Estelle session surface.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use clap::ValueEnum;
use codex_external_agent_migration::sessions::SessionMetadataMode;
use codex_external_agent_migration::sessions::detect_recent_cla_sessions;
use codex_external_agent_migration::sessions::detect_recent_ope_sessions;
use codex_external_agent_migration::sessions::prepare_validated_session_import_with_metadata_mode;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::RolloutItem;
use codex_rollout::RolloutRecorder;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

const IMPORTED_MARKER: &str = "<EXTERNAL SESSION IMPORTED>";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExternalHistorySource {
    Codex,
    ClaudeCode,
    #[value(name = "opencode")]
    OpenCode,
}

impl ExternalHistorySource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::OpenCode => "OpenCode",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ImportedHistory {
    pub(crate) source: ExternalHistorySource,
    pub(crate) title: String,
    pub(crate) cwd: PathBuf,
    pub(crate) source_path: PathBuf,
    pub(crate) source_sha256: String,
    pub(crate) turns: Vec<ImportedTurn>,
}

impl ImportedHistory {
    /// A NEW history with every credential-shaped value in its transcript text replaced by the
    /// shared redaction marker — the ingest half of the rule below. Provenance fields (`cwd`,
    /// `source_path`, `source_sha256`) are carried through byte-identical: they are not
    /// transcript text, and `load_latest_history` compares the sha before and after the read.
    pub(crate) fn redacted(self) -> Self {
        Self {
            title: estelle_client::redact_secrets(&self.title),
            turns: self.turns.into_iter().map(ImportedTurn::redacted).collect(),
            ..self
        }
    }

    /// The imported conversation as one block of model context.
    ///
    /// 🔴 THIS IS A NETWORK WRITE OF SOMEBODY ELSE'S TRANSCRIPT. The returned string is stored
    /// as `imported_context` (`session_server.rs:797`), prepended to `session_context`
    /// (`session_server.rs:846`) and posted to the API by `answer_question` on the next
    /// question. Another harness's session is one of the likelier places for a pasted
    /// credential to be sitting, so the same rule the checkpoint wire follows applies here:
    /// the SHAPE is named so the loss is visible downstream, the VALUE never leaves.
    ///
    /// Redacted here as well as at ingest ON PURPOSE, and it is not belt-and-braces: the
    /// session server receives `ImportedHistory` deserialized off the socket
    /// (`ClientRequest::ImportHistory`), so it cannot assume any caller already scrubbed it.
    /// A guard that only runs on the path you remembered to instrument is a guard on that path.
    /// Redaction is idempotent, so the second pass cannot corrupt the first's markers — which
    /// is asserted, not assumed.
    pub(crate) fn model_context(&self) -> String {
        let mut lines = vec![format!(
            "Imported {} session: {}",
            self.source.label(),
            self.title
        )];
        for turn in &self.turns {
            lines.push(format!("User: {}", turn.question));
            lines.push(format!("Assistant: {}", turn.answer));
        }
        estelle_client::redact_secrets(&lines.join("\n"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ImportedTurn {
    pub(crate) question: String,
    pub(crate) answer: String,
}

impl ImportedTurn {
    fn redacted(self) -> Self {
        Self {
            question: estelle_client::redact_secrets(&self.question),
            answer: estelle_client::redact_secrets(&self.answer),
        }
    }
}

pub(crate) fn turns_from_rollout_items(items: &[RolloutItem]) -> io::Result<Vec<ImportedTurn>> {
    let mut turns = Vec::new();
    let mut pending_question: Option<String> = None;
    let mut pending_answers = Vec::new();

    let finish = |question: Option<String>, answers: &mut Vec<String>| {
        question.and_then(|question| {
            let answer = std::mem::take(answers).join("\n");
            (!answer.is_empty()).then_some(ImportedTurn { question, answer })
        })
    };

    for item in items {
        let RolloutItem::EventMsg(event) = item else {
            continue;
        };
        match event {
            EventMsg::UserMessage(event) => {
                if let Some(turn) = finish(pending_question.take(), &mut pending_answers) {
                    turns.push(turn);
                }
                pending_question = Some(event.message.clone());
            }
            EventMsg::AgentMessage(event)
                if event.message != IMPORTED_MARKER && pending_question.is_some() =>
            {
                pending_answers.push(event.message.clone());
            }
            _ => {}
        }
    }
    if let Some(turn) = finish(pending_question, &mut pending_answers) {
        turns.push(turn);
    }
    if turns.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "history contained no complete user/assistant turn",
        ));
    }
    Ok(turns)
}

pub(crate) async fn load_latest_history(
    source: ExternalHistorySource,
    repository_root: &Path,
) -> io::Result<ImportedHistory> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))?;
    let repository_root = fs::canonicalize(repository_root)?;
    let history = match source {
        ExternalHistorySource::Codex => {
            load_latest_codex_history(&home.join(".codex"), &repository_root).await
        }
        ExternalHistorySource::ClaudeCode => {
            let cache_home = home.join(".estelle/history-import-cache");
            let migrations = detect_recent_cla_sessions(&home.join(".claude"), &cache_home)?;
            load_latest_migration(
                source,
                &cache_home,
                migrations,
                SessionMetadataMode::Embedded,
                &repository_root,
            )
        }
        ExternalHistorySource::OpenCode => {
            let cache_home = home.join(".estelle/history-import-cache");
            let source_home = home.join(".local/share/opencode");
            let source_path = source_home.join("opencode.db");
            let before = sha256_file(&source_path)?;
            let migrations = detect_recent_ope_sessions(&source_home, &cache_home).await?;
            let history = load_latest_migration(
                source,
                &cache_home,
                migrations,
                SessionMetadataMode::MigrationFallback,
                &repository_root,
            )?;
            let after = sha256_file(&source_path)?;
            if before != after {
                return Err(io::Error::other(
                    "OpenCode source database changed during read-only import",
                ));
            }
            Ok(ImportedHistory {
                source_path,
                source_sha256: before,
                ..history
            })
        }
    }?;
    // The ingest boundary, and the only entry point into this module — so all three sources are
    // scrubbed by ONE line rather than three. From here on nothing downstream (the session
    // server's in-memory turns, the local socket, the rendered transcript) holds the raw value.
    Ok(history.redacted())
}

fn load_latest_migration(
    source: ExternalHistorySource,
    cache_home: &Path,
    migrations: Vec<codex_external_agent_migration::sessions::ExternalAgentSessionMigration>,
    metadata_mode: SessionMetadataMode,
    repository_root: &Path,
) -> io::Result<ImportedHistory> {
    let migration = migrations
        .into_iter()
        .find(|migration| canonical_eq(&migration.cwd, repository_root))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no recent history matched the current repository",
            )
        })?;
    let source_path = migration.path.clone();
    let source_sha256 = sha256_file(&source_path)?;
    let pending =
        prepare_validated_session_import_with_metadata_mode(cache_home, migration, metadata_mode)?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the matching history was empty or already imported",
                )
            })?;
    let turns = turns_from_rollout_items(&pending.session.rollout_items)?;
    let title = pending
        .session
        .title
        .or(pending.session.first_user_message)
        .unwrap_or_else(|| "Imported session".to_string());
    Ok(ImportedHistory {
        source,
        title,
        cwd: pending.session.cwd,
        source_path,
        source_sha256,
        turns,
    })
}

async fn load_latest_codex_history(
    codex_home: &Path,
    repository_root: &Path,
) -> io::Result<ImportedHistory> {
    let mut candidates = jsonl_files(&codex_home.join("sessions"));
    candidates.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    candidates.reverse();
    for source_path in candidates {
        let before = sha256_file(&source_path)?;
        let (items, _thread_id, parse_errors) =
            RolloutRecorder::load_rollout_items(&source_path).await?;
        if parse_errors != 0 {
            continue;
        }
        let cwd = items.iter().find_map(|item| match item {
            RolloutItem::SessionMeta(line) => Some(line.meta.cwd.clone()),
            _ => None,
        });
        let Some(cwd) = cwd.filter(|cwd| canonical_eq(cwd, repository_root)) else {
            continue;
        };
        let turns = match turns_from_rollout_items(&items) {
            Ok(turns) => turns,
            Err(_) => continue,
        };
        let after = sha256_file(&source_path)?;
        if before != after {
            return Err(io::Error::other(
                "Codex source rollout changed during read-only import",
            ));
        }
        return Ok(ImportedHistory {
            source: ExternalHistorySource::Codex,
            title: turns[0].question.clone(),
            cwd,
            source_path,
            source_sha256: before,
            turns,
        });
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no recent history matched the current repository",
    ))
}

fn jsonl_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => pending.push(path),
                Ok(file_type)
                    if file_type.is_file()
                        && path.extension().and_then(|extension| extension.to_str())
                            == Some("jsonl") =>
                {
                    files.push(path);
                }
                _ => {}
            }
        }
    }
    files
}

fn canonical_eq(left: &Path, right: &Path) -> bool {
    fs::canonicalize(left)
        .and_then(|left| fs::canonicalize(right).map(|right| left == right))
        .unwrap_or(false)
}

fn sha256_file(path: &Path) -> io::Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::AgentMessageEvent;
    use codex_protocol::protocol::RolloutLine;
    use codex_protocol::protocol::SessionMeta;
    use codex_protocol::protocol::SessionMetaLine;
    use codex_protocol::protocol::SessionSource;
    use codex_protocol::protocol::ThreadHistoryMode;
    use codex_protocol::protocol::UserMessageEvent;

    /// A credential-shaped string that the shared redactor is PROVEN to catch.
    ///
    /// The assertion inside is the instrument check, not decoration: if the fixture were
    /// allowlisted (gitleaks ships published examples like `AKIAIOSFODNN7EXAMPLE`) or the shape
    /// were dropped from the rule set, every redaction test below would pass over a string
    /// nothing was ever going to redact. That is the inert-guard failure, so it fails here first.
    ///
    /// Assembled from two literals so no scanner-shaped token appears verbatim in this source —
    /// the same reason `estelle-client/src/secret_engine.rs` assembles its own fixtures. It is
    /// invented, and is not and never was a real credential.
    fn detectable_credential_fixture() -> String {
        let fixture = format!("{}{}", "AKIA", "Q3XMPLDEADBEEF42");
        assert_ne!(
            estelle_client::redact_secrets(&fixture),
            fixture,
            "the fixture must be detectable by the shared redactor, or the tests using it \
             cannot fail"
        );
        fixture
    }

    fn history_carrying(secret: &str) -> ImportedHistory {
        ImportedHistory {
            source: ExternalHistorySource::ClaudeCode,
            title: format!("debugging {secret} against staging"),
            cwd: PathBuf::from("/tmp/repository"),
            source_path: PathBuf::from("/tmp/session.jsonl"),
            source_sha256: "0".repeat(64),
            turns: vec![ImportedTurn {
                question: format!("why does {secret} return 403?"),
                answer: format!("that key is revoked; rotate {secret} and retry"),
            }],
        }
    }

    /// THE NETWORK SINK. `model_context()` is the only value from an imported transcript that
    /// leaves this process for `api.fatelabs.ca` (`session_server.rs:797` stores it, `:846`
    /// prepends it to `session_context`, and `answer_question` posts it). The session server
    /// receives `ImportedHistory` deserialized off the socket, so it cannot assume any caller
    /// already redacted: this is the fail-closed pass.
    #[test]
    fn model_context_never_emits_a_credential_shaped_string() {
        let secret = detectable_credential_fixture();
        let context = history_carrying(&secret).model_context();

        assert!(
            !context.contains(&secret),
            "model_context() forwarded the credential verbatim to the deep-search sink"
        );
        assert!(
            context.to_lowercase().contains("redacted"),
            "expected a redaction marker naming the loss, got: {context}"
        );
        // Redaction, not truncation: the surrounding conversation must still travel, or the
        // import feature stops being worth having.
        assert!(context.contains("return 403?"), "context: {context}");
        assert!(
            context.contains("Imported Claude Code session"),
            "context: {context}"
        );
    }

    /// THE INGEST BOUNDARY. Redacting only at the sink would leave the raw credential in the
    /// session server's in-memory turns and on the local socket, so the value is scrubbed the
    /// moment it is read off another harness's disk. `load_latest_history` is the module's only
    /// entry point, so this is one owner for all three sources.
    #[test]
    fn redaction_at_ingest_scrubs_every_field_that_carries_transcript_text() {
        let secret = detectable_credential_fixture();
        let redacted = history_carrying(&secret).redacted();

        assert!(!redacted.title.contains(&secret), "title leaked");
        assert!(
            !redacted.turns[0].question.contains(&secret),
            "question leaked"
        );
        assert!(!redacted.turns[0].answer.contains(&secret), "answer leaked");
        // Provenance is not transcript text and must survive byte-identical, or the read-only
        // source check in `load_latest_history` starts comparing against a rewritten value.
        assert_eq!(redacted.source_sha256, "0".repeat(64));
        assert_eq!(redacted.source_path, PathBuf::from("/tmp/session.jsonl"));
        assert_eq!(redacted.cwd, PathBuf::from("/tmp/repository"));
    }

    /// The two passes above both run on a real import. If redaction were not idempotent the
    /// second would corrupt the first's markers, so the double pass is asserted, not assumed.
    #[test]
    fn redaction_is_idempotent_so_the_two_passes_cannot_corrupt_each_other() {
        let secret = detectable_credential_fixture();
        let once = history_carrying(&secret).redacted();
        let twice = once.clone().redacted();
        assert_eq!(once, twice);
        assert_eq!(once.model_context(), twice.model_context());
    }

    #[test]
    fn converts_complete_turns_and_ignores_the_import_marker() {
        let items = vec![
            RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
                message: "Fix the parser".to_string(),
                ..Default::default()
            })),
            RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
                message: "Parser fixed.".to_string(),
                phase: None,
                memory_citation: None,
            })),
            RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
                message: IMPORTED_MARKER.to_string(),
                phase: None,
                memory_citation: None,
            })),
        ];

        assert_eq!(
            turns_from_rollout_items(&items).expect("complete transcript"),
            vec![ImportedTurn {
                question: "Fix the parser".to_string(),
                answer: "Parser fixed.".to_string(),
            }]
        );
    }

    #[test]
    fn fails_closed_when_no_complete_turn_exists() {
        let items = vec![RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "unfinished".to_string(),
                ..Default::default()
            },
        ))];

        let error = turns_from_rollout_items(&items).expect_err("must fail closed");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("no complete"));
    }

    #[tokio::test]
    async fn loads_matching_codex_rollout_read_only_and_rejects_a_malformed_control() {
        let root = tempfile::tempdir().expect("root");
        let codex_home = root.path().join(".codex");
        let repository = root.path().join("repository");
        let sessions = codex_home.join("sessions/2026/08/18");
        fs::create_dir_all(&repository).expect("repository");
        fs::create_dir_all(&sessions).expect("sessions");
        let malformed = sessions.join("rollout-2026-08-18T00-00-00-bad.jsonl");
        fs::write(&malformed, "not-json\n").expect("malformed control");
        let error = load_latest_codex_history(&codex_home, &repository)
            .await
            .expect_err("malformed-only source must fail closed");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);

        let thread_id = ThreadId::new();
        let source = sessions.join(format!("rollout-2026-08-18T00-00-01-{thread_id}.jsonl"));
        let records = [
            RolloutLine {
                timestamp: "2026-08-18T00:00:01Z".to_string(),
                ordinal: Some(0),
                item: RolloutItem::SessionMeta(SessionMetaLine {
                    meta: SessionMeta {
                        session_id: thread_id.into(),
                        id: thread_id,
                        timestamp: "2026-08-18T00:00:01Z".to_string(),
                        cwd: repository.clone(),
                        originator: "fixture".to_string(),
                        cli_version: "fixture".to_string(),
                        source: SessionSource::Cli,
                        history_mode: ThreadHistoryMode::Paginated,
                        ..SessionMeta::default()
                    },
                    git: None,
                }),
            },
            RolloutLine {
                timestamp: "2026-08-18T00:00:02Z".to_string(),
                ordinal: Some(1),
                item: RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
                    message: "Resume the exact parser task".to_string(),
                    ..Default::default()
                })),
            },
            RolloutLine {
                timestamp: "2026-08-18T00:00:03Z".to_string(),
                ordinal: Some(2),
                item: RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
                    message: "The parser task is resumable.".to_string(),
                    phase: None,
                    memory_citation: None,
                })),
            },
        ];
        let bytes = records
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .expect("serialize rollout")
            .join("\n")
            .into_bytes();
        fs::write(&source, &bytes).expect("write rollout");

        let (loaded, _, parse_errors) = RolloutRecorder::load_rollout_items(&source)
            .await
            .expect("load fixture rollout");
        assert_eq!(parse_errors, 0);
        assert_eq!(loaded.len(), 3);

        let history = load_latest_codex_history(&codex_home, &repository)
            .await
            .expect("matching Codex history");
        assert_eq!(history.source, ExternalHistorySource::Codex);
        assert_eq!(history.cwd, repository);
        assert_eq!(history.source_path, source);
        assert_eq!(
            history.source_sha256,
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(fs::read(&history.source_path).expect("read after"), bytes);
        assert_eq!(
            history.turns,
            vec![ImportedTurn {
                question: "Resume the exact parser task".to_string(),
                answer: "The parser task is resumable.".to_string(),
            }]
        );
    }
}
