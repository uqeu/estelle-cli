//! OpenCode session import.
//!
//! Session discovery was informed by jcode's `jcode-import-core` (MIT, Jeremy Huang), while the
//! current SQLite contract follows OpenCode's `packages/core/src/session/sql.ts` (MIT). Unlike
//! jcode's legacy JSON reader, this adapter reads the current multi-session database read-only and
//! materializes one stable, ledger-compatible record per session under the Estelle home.

use super::common::SessionFileCandidate;
use super::common::detect_recent_sessions;
use crate::model::ExternalAgentSessionImportLimits;
use crate::sessions::ExternalAgentSessionMigration;
use crate::sessions::SessionRecordFormat;
use serde_json::Value as JsonValue;
use sha2::Digest;
use sha2::Sha256;
use sqlx::Row;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) async fn detect_recent_ope_sessions_with_limits(
    external_agent_home: &Path,
    codex_home: &Path,
    limits: ExternalAgentSessionImportLimits,
) -> io::Result<Vec<ExternalAgentSessionMigration>> {
    if limits.max_sessions == 0 {
        return Ok(Vec::new());
    }
    let database_path = external_agent_home.join("opencode.db");
    if !database_path.is_file() {
        return Ok(Vec::new());
    }

    let pool = codex_state::open_read_only_sqlite(&database_path)
        .await
        .map_err(sqlite_error)?;
    let cutoff_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .saturating_sub(limits.max_age.as_millis());
    let cutoff_millis = i64::try_from(cutoff_millis).unwrap_or(i64::MIN);
    let session_limit = i64::try_from(limits.max_sessions).unwrap_or(i64::MAX);
    let rows = sqlx::query(
        "SELECT id, directory, title, time_updated \
         FROM session WHERE time_updated >= ? \
         ORDER BY time_updated DESC, id ASC LIMIT ?",
    )
    .bind(cutoff_millis)
    .bind(session_limit)
    .fetch_all(&pool)
    .await
    .map_err(sqlite_error)?;

    let cache_root = ope_session_cache_root(codex_home);
    fs::create_dir_all(&cache_root)?;
    let mut candidates = Vec::with_capacity(rows.len());
    let mut titles = HashMap::new();
    for row in rows {
        let session_id: String = row.try_get("id").map_err(sqlite_error)?;
        let cwd = PathBuf::from(
            row.try_get::<String, _>("directory")
                .map_err(sqlite_error)?,
        );
        let title: String = row.try_get("title").map_err(sqlite_error)?;
        let message_rows = sqlx::query(
            "SELECT type, time_created, data FROM session_message \
             WHERE session_id = ? ORDER BY seq ASC, id ASC",
        )
        .bind(&session_id)
        .fetch_all(&pool)
        .await
        .map_err(sqlite_error)?;
        let snapshot = render_session_snapshot(&cwd, message_rows)?;
        if snapshot.is_empty() {
            continue;
        }
        let cache_path =
            cache_root.join(format!("{:x}.jsonl", Sha256::digest(session_id.as_bytes())));
        // Refreshing this derived record lets the existing ledger compare content and
        // suppress an unchanged session without ever touching OpenCode's database.
        fs::write(&cache_path, snapshot)?;
        titles.insert(cache_path.clone(), title);
        candidates.push(SessionFileCandidate {
            path: cache_path,
            fallback_cwd: Some(cwd),
            record_format: SessionRecordFormat::Cur,
        });
    }
    pool.close().await;

    let mut migrations = detect_recent_sessions(
        codex_home, candidates, /*require_existing_cwd*/ false, limits,
    )?;
    for migration in &mut migrations {
        if let Some(title) = titles.get(&migration.path) {
            migration.title = Some(title.clone());
        }
    }
    Ok(migrations)
}

pub(crate) fn ope_session_cache_root(codex_home: &Path) -> PathBuf {
    codex_home.join("external-agent-sessions").join("opencode")
}

fn render_session_snapshot(cwd: &Path, rows: Vec<sqlx::sqlite::SqliteRow>) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    for row in rows {
        let role: String = row.try_get("type").map_err(sqlite_error)?;
        let timestamp_ms: i64 = row.try_get("time_created").map_err(sqlite_error)?;
        let data: String = row.try_get("data").map_err(sqlite_error)?;
        let data: JsonValue = serde_json::from_str(&data).map_err(invalid_data)?;
        let text = match role.as_str() {
            "user" => data
                .get("text")
                .and_then(JsonValue::as_str)
                .map(str::to_owned),
            "assistant" => assistant_text(&data),
            _ => None,
        };
        let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
            continue;
        };
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({
                "role": role,
                "cwd": cwd,
                "timestamp_ms": timestamp_ms,
                "message": { "content": text },
            }),
        )
        .map_err(invalid_data)?;
        output.push(b'\n');
    }
    Ok(output)
}

fn assistant_text(data: &JsonValue) -> Option<String> {
    let text = data
        .get("content")?
        .as_array()?
        .iter()
        .filter(|part| part.get("type").and_then(JsonValue::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(JsonValue::as_str))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn sqlite_error(error: sqlx::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use crate::ExternalAgentConfigDetectOptions;
    use crate::ExternalAgentConfigMigrationItemType;
    use crate::ExternalAgentConfigService;
    use crate::migration_source::ExternalAgentSource;
    use crate::sessions::SessionMetadataMode;
    use crate::sessions::prepare_validated_session_import_with_metadata_mode;
    use codex_protocol::protocol::EventMsg;
    use codex_protocol::protocol::RolloutItem;
    use sha2::Digest;
    use sha2::Sha256;
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::Executor;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the fixture creates a disposable source database; production opens read-only"
    )]
    async fn imports_current_opencode_sqlite_without_modifying_source() {
        assert_eq!(
            ExternalAgentSource::from_migration_source(Some("opencode")),
            ExternalAgentSource::Ope
        );
        assert_eq!(ExternalAgentSource::Ope.config_dir(), ".local/share/opencode");

        let root = TempDir::new().expect("tempdir");
        let external_agent_home = root.path().join(".local/share/opencode");
        let codex_home = root.path().join(".codex");
        let project_cwd = root.path().join("project");
        let database_path = external_agent_home.join("opencode.db");
        fs::create_dir_all(&external_agent_home).expect("create OpenCode data dir");
        fs::create_dir_all(&project_cwd).expect("create project");

        let mut connection = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true)
            .connect()
            .await
            .expect("create fixture database");
        connection
            .execute(
                "CREATE TABLE session (\
                    id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT NOT NULL, \
                    time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL\
                 ); \
                 CREATE TABLE session_message (\
                    id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, \
                    seq INTEGER NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL\
                 );",
            )
            .await
            .expect("create current OpenCode schema");
        sqlx::query(
            "INSERT INTO session (id, directory, title, time_created, time_updated) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("ses_current")
        .bind(project_cwd.to_string_lossy().as_ref())
        .bind("Fix the parser")
        .bind(1_787_000_000_000_i64)
        .bind(1_787_000_002_000_i64)
        .execute(&mut connection)
        .await
        .expect("insert session");
        sqlx::query(
            "INSERT INTO session_message \
             (id, session_id, type, seq, time_created, data) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("msg_user")
        .bind("ses_current")
        .bind("user")
        .bind(1_i64)
        .bind(1_787_000_000_000_i64)
        .bind(r#"{"text":"Please fix the parser","files":[],"agents":[]}"#)
        .execute(&mut connection)
        .await
        .expect("insert user message");
        sqlx::query(
            "INSERT INTO session_message \
             (id, session_id, type, seq, time_created, data) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("msg_assistant")
        .bind("ses_current")
        .bind("assistant")
        .bind(2_i64)
        .bind(1_787_000_001_000_i64)
        .bind(
            r#"{"agent":"build","model":{"providerID":"openai","modelID":"gpt"},"content":[{"type":"text","id":"txt_1","text":"Parser fixed."}]}"#,
        )
        .execute(&mut connection)
        .await
        .expect("insert assistant message");
        connection.close().await.expect("close fixture database");

        let before = Sha256::digest(fs::read(&database_path).expect("read database before"));
        let mut service = ExternalAgentConfigService::new_for_test(
            codex_home.clone(),
            external_agent_home.clone(),
        );
        service.source = ExternalAgentSource::Ope;
        let items = service
            .detect(ExternalAgentConfigDetectOptions {
                include_home: true,
                include_memory: false,
                cwds: None,
            })
            .await
            .expect("detect OpenCode sessions");
        let after = Sha256::digest(fs::read(&database_path).expect("read database after"));

        assert_eq!(
            before, after,
            "OpenCode source database must stay unchanged"
        );
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].item_type,
            ExternalAgentConfigMigrationItemType::Sessions
        );
        assert_eq!(
            items[0].description,
            format!("Migrate recent sessions from {}", database_path.display())
        );
        let migrations = &items[0].details.as_ref().expect("session details").sessions;
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].cwd, project_cwd);
        assert_eq!(migrations[0].title.as_deref(), Some("Fix the parser"));
        assert_eq!(
            service
                .external_agent_session_source_path(&migrations[0].path)
                .expect("validate derived source"),
            Some(fs::canonicalize(&migrations[0].path).expect("canonical cache path"))
        );

        let pending = prepare_validated_session_import_with_metadata_mode(
            &codex_home,
            migrations[0].clone(),
            SessionMetadataMode::MigrationFallback,
        )
        .expect("prepare import")
        .expect("session is importable");
        assert_eq!(
            pending.session.first_user_message.as_deref(),
            Some("Please fix the parser")
        );
        assert!(pending.session.rollout_items.iter().any(|item| matches!(
            item,
            RolloutItem::EventMsg(EventMsg::AgentMessage(event))
                if event.message == "Parser fixed."
        )));
    }
}
