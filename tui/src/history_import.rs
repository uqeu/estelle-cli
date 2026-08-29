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
use estelle_client::redact_secrets;
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
        redact_secrets(&lines.join("\n"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ImportedTurn {
    pub(crate) question: String,
    pub(crate) answer: String,
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
    match source {
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
    }
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

    #[test]
    fn model_context_redacts_credentials_without_erasing_clean_imported_prose() {
        let github_token = format!("ghp_{}", "A".repeat(20));
        let api_key = format!("sk-{}", "b".repeat(16));
        let aws_key = format!("AKIA{}", "C".repeat(16));
        let history = ImportedHistory {
            source: ExternalHistorySource::ClaudeCode,
            title: format!("Debug {github_token}"),
            cwd: PathBuf::from("/fixture"),
            source_path: PathBuf::from("/fixture/session.jsonl"),
            source_sha256: "fixture-sha".to_string(),
            turns: vec![ImportedTurn {
                question: format!("Why did {api_key} fail?"),
                answer: format!("Rotate {aws_key}, then retry the clean parser task."),
            }],
        };

        let context = history.model_context();

        for credential in [&github_token, &api_key, &aws_key] {
            assert!(
                !context.contains(credential),
                "credential survived: {context}"
            );
        }
        assert!(context.contains("[redacted: a GitHub token]"), "{context}");
        assert!(context.contains("[redacted: an sk- API key]"), "{context}");
        assert!(
            context.contains("[redacted: an AWS access key]"),
            "{context}"
        );
        assert!(context.contains("retry the clean parser task"), "{context}");
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
