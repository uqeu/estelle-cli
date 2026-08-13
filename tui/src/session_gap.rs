//! Bounded, local-only context for the gap between two TUI sessions.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const MIN_GAP_SECONDS: i64 = 1_800;
const NEWS_FREE_GAP_SECONDS: i64 = 3_600;
const MAX_FILES: usize = 3;
const MAX_ACTORS: usize = 3;
const MAX_WHAT_CHARS: usize = 60;
pub const MAX_TRACKED_FILES: usize = 40;
const MAX_STATE_ENTRIES: usize = 64;
const MAX_STATE_BYTES: u64 = 1024 * 1024;
const MAX_COMMITS: usize = 60;
const MAX_CHANGES: usize = 240;
const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_millis(1_500);
const RECORD_SEPARATOR: char = '\u{1}';
const FIELD_SEPARATOR: char = '\u{1f}';
const UNKNOWN_GIT_LINE: &str =
    "Committed repository changes are unknown; local Git history could not be verified.";

static STATE_LOCK: Mutex<()> = Mutex::new(());

/// One source of truth for both readers of the returning-session summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionContext {
    /// Lines rendered verbatim for the person using the TUI.
    pub human_lines: Vec<String>,
    /// The exact same claims, joined for injection into the model context.
    pub model_context: String,
}

impl SessionContext {
    fn from_lines(human_lines: Vec<String>) -> Self {
        let model_context = human_lines.join("\n");
        Self {
            human_lines,
            model_context,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.human_lines.is_empty()
    }

    pub fn model_context(&self) -> String {
        self.model_context.clone()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct SessionCheckpoint {
    #[serde(default)]
    cwd: String,
    at: String,
    head: String,
    files: Vec<String>,
}

type SessionState = BTreeMap<String, SessionCheckpoint>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitChange {
    at: DateTime<Utc>,
    actor: String,
    path: String,
    what: String,
}

enum GitEvidence {
    Known(Vec<GitChange>),
    Unknown,
}

/// Load returning-session context without blocking the async caller.
pub async fn welcome_context(cwd: &Path, now: DateTime<Utc>) -> SessionContext {
    let Some(path) = state_path() else {
        return SessionContext::default();
    };
    welcome_context_from(cwd.to_path_buf(), now, path).await
}

/// Best-effort local checkpoint. Failure only suppresses the next welcome.
pub async fn record_checkpoint(cwd: &Path, files: &[PathBuf], now: DateTime<Utc>) -> bool {
    let Some(path) = state_path() else {
        return false;
    };
    record_checkpoint_to(cwd.to_path_buf(), files.to_vec(), now, path).await
}

pub fn state_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("last-session.json"))
}

async fn welcome_context_from(
    cwd: PathBuf,
    now: DateTime<Utc>,
    state_path: PathBuf,
) -> SessionContext {
    let Some(cwd_key) = path_text(&cwd) else {
        return SessionContext::default();
    };
    let checkpoint = tokio::task::spawn_blocking(move || {
        let _guard = STATE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        read_state(&state_path).ok()?.remove(&cwd_key)
    })
    .await
    .ok()
    .flatten();
    let Some(checkpoint) = checkpoint else {
        return SessionContext::default();
    };
    let Ok(last_seen) = DateTime::parse_from_rfc3339(&checkpoint.at) else {
        return SessionContext::default();
    };
    let last_seen = last_seen.with_timezone(&Utc);
    let gap_seconds = now.signed_duration_since(last_seen).num_seconds();
    if !(MIN_GAP_SECONDS..).contains(&gap_seconds) {
        return SessionContext::default();
    }

    let evidence = collect_changes(&cwd, &checkpoint.head).await;
    build_context(checkpoint, last_seen, now, gap_seconds, evidence)
}

/// The injectable half of `record_checkpoint` — the hook's checkpoint mode calls this with the
/// default state path, tests call it with a temporary one.
pub async fn record_checkpoint_to(    cwd: PathBuf,
    files: Vec<PathBuf>,
    now: DateTime<Utc>,
    state_path: PathBuf,
) -> bool {
    let Some(cwd_key) = path_text(&cwd) else {
        return false;
    };
    let head = git_output(&cwd, ["rev-parse", "HEAD"])
        .await
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let files = tracked_files(&cwd, files);
    let checkpoint = SessionCheckpoint {
        cwd: cwd_key.clone(),
        at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        head,
        files,
    };

    tokio::task::spawn_blocking(move || {
        let _guard = STATE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = read_state(&state_path).unwrap_or_default();
        state.insert(cwd_key.clone(), checkpoint);
        prune_state(&mut state, &cwd_key);
        write_state(&state_path, &state).is_ok()
    })
    .await
    .unwrap_or(false)
}

fn path_text(path: &Path) -> Option<String> {
    let value = path.to_str()?.trim();
    (!value.is_empty() && value.len() <= 4_096 && !value.chars().any(char::is_control))
        .then(|| value.to_string())
}

fn tracked_files(cwd: &Path, files: Vec<PathBuf>) -> Vec<String> {
    let mut seen = HashSet::new();
    files
        .into_iter()
        .filter_map(|path| {
            let relative = if path.is_absolute() {
                path.strip_prefix(cwd).ok()?.to_path_buf()
            } else {
                path
            };
            if relative.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            }) {
                return None;
            }
            let text = path_text(&relative)?;
            seen.insert(text.clone()).then_some(text)
        })
        .take(MAX_TRACKED_FILES)
        .collect()
}

fn read_state(path: &Path) -> io::Result<SessionState> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_STATE_BYTES => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "session checkpoint exceeds the local size bound",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(SessionState::new()),
        Err(error) => return Err(error),
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn prune_state(state: &mut SessionState, current: &str) {
    if state.len() <= MAX_STATE_ENTRIES {
        return;
    }
    let mut oldest = state
        .iter()
        .filter(|(cwd, _)| cwd.as_str() != current)
        .map(|(cwd, entry)| (entry.at.clone(), cwd.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, cwd) in oldest
        .into_iter()
        .take(state.len().saturating_sub(MAX_STATE_ENTRIES))
    {
        state.remove(&cwd);
    }
}

fn write_state(path: &Path, state: &SessionState) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "session checkpoint has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".last-session-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut temporary, state).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

async fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let read = async move {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_GIT_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() > MAX_GIT_OUTPUT_BYTES {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return None;
        }
        child.wait().await.ok()?.success().then_some(())?;
        String::from_utf8(bytes).ok()
    };
    tokio::time::timeout(GIT_TIMEOUT, read).await.ok().flatten()
}

async fn collect_changes(cwd: &Path, since_head: &str) -> GitEvidence {
    let since_head = since_head.trim();
    if since_head.is_empty() {
        return GitEvidence::Unknown;
    }
    let Some(head) = git_output(cwd, ["rev-parse", "HEAD"]).await else {
        return GitEvidence::Unknown;
    };
    if head.trim() == since_head {
        return GitEvidence::Known(Vec::new());
    }
    if git_output(cwd, ["merge-base", "--is-ancestor", since_head, "HEAD"])
        .await
        .is_none()
    {
        return GitEvidence::Unknown;
    }
    let range = format!("{since_head}..HEAD");
    let limit = MAX_COMMITS.to_string();
    let format = format!(
        "--pretty=format:{RECORD_SEPARATOR}%H{FIELD_SEPARATOR}%an{FIELD_SEPARATOR}%aI{FIELD_SEPARATOR}%s"
    );
    let Some(log) = git_output(cwd, ["log", &range, "-n", &limit, "--name-only", &format]).await
    else {
        return GitEvidence::Unknown;
    };
    parse_log(&log).map_or(GitEvidence::Unknown, GitEvidence::Known)
}

fn parse_log(text: &str) -> Option<Vec<GitChange>> {
    let mut changes = Vec::new();
    for block in text.split(RECORD_SEPARATOR) {
        let mut lines = block.lines();
        let Some(header) = lines.next().filter(|line| !line.trim().is_empty()) else {
            continue;
        };
        let fields = header.split(FIELD_SEPARATOR).collect::<Vec<_>>();
        if fields.len() != 4 {
            return None;
        }
        let Ok(at) = DateTime::parse_from_rfc3339(fields[2]) else {
            continue;
        };
        let actor = clean_git_text(fields[1]);
        let what = clean_git_text(fields[3]);
        for path in lines.map(clean_git_text).filter(|path| !path.is_empty()) {
            changes.push(GitChange {
                at: at.with_timezone(&Utc),
                actor: actor.clone(),
                path,
                what: what.clone(),
            });
            if changes.len() == MAX_CHANGES {
                return Some(changes);
            }
        }
    }
    Some(changes)
}

fn clean_git_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn build_context(
    checkpoint: SessionCheckpoint,
    last_seen: DateTime<Utc>,
    now: DateTime<Utc>,
    gap_seconds: i64,
    evidence: GitEvidence,
) -> SessionContext {
    let first = format!(
        "Welcome back. You were away {}. It is now {} UTC.",
        humanize(gap_seconds),
        now.format("%H:%M %A")
    );
    let GitEvidence::Known(changes) = evidence else {
        return if gap_seconds >= NEWS_FREE_GAP_SECONDS {
            SessionContext::from_lines(vec![first, UNKNOWN_GIT_LINE.to_string()])
        } else {
            SessionContext::default()
        };
    };
    let recent = changes
        .into_iter()
        .filter(|change| change.at > last_seen && change.at <= now)
        .collect::<Vec<_>>();
    if recent.is_empty() {
        return if gap_seconds >= NEWS_FREE_GAP_SECONDS {
            SessionContext::from_lines(vec![
                first,
                "No committed changes were found while you were away.".to_string(),
            ])
        } else {
            SessionContext::default()
        };
    }

    let mut moved_indexes = HashSet::new();
    let mut moved = Vec::new();
    for file in &checkpoint.files {
        if let Some((index, change)) = recent
            .iter()
            .enumerate()
            .find(|(index, change)| change.path == *file && !moved_indexes.contains(index))
        {
            moved_indexes.insert(index);
            moved.push(change);
        }
    }
    let rest = recent
        .iter()
        .enumerate()
        .filter_map(|(index, change)| (!moved_indexes.contains(&index)).then_some(change))
        .collect::<Vec<_>>();
    let mut lines = vec![first];
    if !moved.is_empty() {
        lines.push("Code you touched has changed since:".to_string());
        lines.extend(
            moved
                .iter()
                .take(MAX_FILES)
                .map(|change| moved_line(change, now)),
        );
        let extra = moved.len().saturating_sub(MAX_FILES);
        if extra > 0 {
            lines.push(format!(
                "(+{} you touched also changed)",
                plural(extra, "more file")
            ));
        }
    }
    if !rest.is_empty() {
        lines.push(rest_line(&rest));
    }
    SessionContext::from_lines(lines)
}

fn moved_line(change: &GitChange, now: DateTime<Utc>) -> String {
    let actor = if change.actor.is_empty() {
        "author not recorded".to_string()
    } else {
        format!("by {}", change.actor)
    };
    let what = truncate_chars(&change.what, MAX_WHAT_CHARS);
    let suffix = if what.is_empty() {
        String::new()
    } else {
        format!(" — {what}")
    };
    format!(
        "- {} — {}, {} ago{}",
        change.path,
        actor,
        humanize(now.signed_duration_since(change.at).num_seconds()),
        suffix
    )
}

fn rest_line(rest: &[&GitChange]) -> String {
    let mut actors = Vec::new();
    for change in rest {
        if !change.actor.is_empty() && !actors.contains(&change.actor) {
            actors.push(change.actor.clone());
        }
    }
    let who = actors_phrase(&actors);
    format!(
        "Elsewhere while you were away: {}{}.",
        plural(rest.len(), "committed file change"),
        if who.is_empty() {
            String::new()
        } else {
            format!(", {who}")
        }
    )
}

fn actors_phrase(actors: &[String]) -> String {
    if actors.is_empty() {
        return String::new();
    }
    let shown = actors.iter().take(MAX_ACTORS).cloned().collect::<Vec<_>>();
    let Some((last, leading)) = shown.split_last() else {
        return String::new();
    };
    let joined = if leading.is_empty() {
        last.clone()
    } else {
        format!("{} and {last}", leading.join(", "))
    };
    let extra = actors.len().saturating_sub(MAX_ACTORS);
    format!(
        "by {joined}{}",
        if extra == 0 {
            String::new()
        } else {
            format!(" and {extra} more")
        }
    )
}

fn humanize(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        "under a minute".to_string()
    } else if seconds < 3_600 {
        plural(((seconds as f64) / 60.0).round() as usize, "minute")
    } else if seconds < 172_800 {
        format!(
            "about {}",
            plural(((seconds as f64) / 3_600.0).round() as usize, "hour")
        )
    } else {
        format!(
            "about {}",
            plural(((seconds as f64) / 86_400.0).round() as usize, "day")
        )
    }
}

fn plural(count: usize, unit: &str) -> String {
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    if value.chars().count() <= maximum {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(maximum.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use chrono::TimeZone;
    use tempfile::TempDir;

    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 31, hour, minute, 0)
            .single()
            .expect("valid fixture time")
    }

    struct Repo {
        root: TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("temp repo");
            let repo = Self { root };
            repo.git(&["init", "-q"]);
            repo.git(&["config", "user.email", "dana@example.com"]);
            repo.git(&["config", "user.name", "dana"]);
            fs::write(repo.path().join("a.rs"), "one\n").expect("write fixture");
            repo.git(&["add", "--", "a.rs"]);
            repo.commit("first", at(3, 0));
            repo
        }

        fn path(&self) -> &Path {
            self.root.path()
        }

        fn git(&self, args: &[&str]) {
            let status = Command::new("git")
                .current_dir(self.path())
                .args(args)
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?}");
        }

        fn commit(&self, subject: &str, when: DateTime<Utc>) {
            let stamp = when.to_rfc3339();
            let status = Command::new("git")
                .current_dir(self.path())
                .env("GIT_AUTHOR_DATE", &stamp)
                .env("GIT_COMMITTER_DATE", &stamp)
                .args(["commit", "-qm", subject])
                .status()
                .expect("commit fixture");
            assert!(status.success());
        }
    }

    #[tokio::test]
    async fn session_gap_uses_one_context_for_the_human_and_model() {
        let repo = Repo::new();
        let state = repo.path().join("state/last-session.json");
        assert!(
            record_checkpoint_to(
                repo.path().to_path_buf(),
                vec![PathBuf::from("a.rs")],
                at(3, 5),
                state.clone(),
            )
            .await
        );
        fs::write(repo.path().join("a.rs"), "two\n").expect("write change");
        repo.git(&["add", "--", "a.rs"]);
        repo.commit("batch the per-file reads", at(8, 0));

        let context = welcome_context_from(repo.path().to_path_buf(), at(11, 12), state).await;

        assert!(context.human_lines.iter().any(|line| line.contains("a.rs")));
        assert!(context.human_lines.iter().any(|line| line.contains("dana")));
        assert!(
            context
                .human_lines
                .iter()
                .any(|line| line.contains("batch the per-file reads"))
        );
        assert_eq!(context.model_context, context.human_lines.join("\n"));
    }

    #[tokio::test]
    async fn session_gap_checkpoint_is_bounded_private_and_concurrency_safe() {
        let first = Repo::new();
        let second = Repo::new();
        let state = first.path().join("state/last-session.json");
        let many = (0..100)
            .map(|index| PathBuf::from(format!("src/f{index}.rs")))
            .collect::<Vec<_>>();

        let (one, two) = tokio::join!(
            record_checkpoint_to(first.path().to_path_buf(), many, at(3, 5), state.clone()),
            record_checkpoint_to(
                second.path().to_path_buf(),
                vec![PathBuf::from("a.rs")],
                at(4, 5),
                state.clone()
            )
        );
        assert!(one && two);

        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&state).expect("checkpoint file"))
                .expect("valid json");
        assert_eq!(value.as_object().expect("state map").len(), 2);
        assert_eq!(
            value[first.path().to_string_lossy().as_ref()]["files"]
                .as_array()
                .expect("files")
                .len(),
            MAX_TRACKED_FILES
        );
        assert_eq!(
            value[first.path().to_string_lossy().as_ref()]["cwd"],
            first.path().to_string_lossy().as_ref()
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&state).expect("metadata").permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn session_gap_is_silent_for_short_or_missing_history_and_explicit_when_git_is_unknown() {
        let repo = Repo::new();
        let state = repo.path().join("state/last-session.json");
        assert!(
            record_checkpoint_to(repo.path().to_path_buf(), vec![], at(3, 5), state.clone()).await
        );
        assert!(
            welcome_context_from(repo.path().to_path_buf(), at(3, 20), state.clone())
                .await
                .is_empty()
        );
        fs::remove_dir_all(repo.path().join(".git")).expect("remove git evidence");

        let unknown = welcome_context_from(repo.path().to_path_buf(), at(11, 12), state).await;
        assert!(
            unknown
                .model_context
                .contains("unknown; local Git history could not be verified")
        );
        assert_eq!(unknown.model_context, unknown.human_lines.join("\n"));
    }
}
