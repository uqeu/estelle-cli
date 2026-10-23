//! `estelle hook stream` — the session's REAL changes, read from git: `workspace_diff`, `commit`,
//! `workspace_tree`. A port of the stand-in bridge's `snapshot` / `commits` / `tree` (`transcript-bridge.py`),
//! with the same event ids, payload shapes and caps.
//!
//! Why git and not the transcript: a transcript carries a diff only for an Edit/Write tool call. A file a
//! script, a formatter or `sed` changed inside a shell command leaves no diff there, so the session's
//! changes are read from the working tree itself — every file changed since the session started (mtime)
//! AND named by something the session ran or edited ([`mentioned`]), diffed against `HEAD`.
//!
//! 🔴 TIME. Every event is stamped no later than the transcript's last entry (`hook_stream::stamp`), in
//! UTC. A commit made more than [`COMMIT_GRACE_S`] after that entry is another session's and is skipped.
//!
//! ⛔ BOUNDS, all declared here: each git call is killed after [`GIT_CALL_TIMEOUT`] and its output is read
//! to at most [`MAX_GIT_OUTPUT_BYTES`]; one run spends at most [`GIT_BUDGET`] on git in total; files per
//! snapshot, diff bytes, tree paths and remembered mentions are each capped. A file whose diff did not
//! fit in the budget is NOT marked published, so the next run sends it — a deferral, never a drop.
//! `GIT_OPTIONAL_LOCKS=0` keeps `git status` from taking the index lock under the person's own git.
//!
//! ⚠️ LIMITS, STATED: diffs are redacted with the same secret shapes as every other field (the bridge did
//! not redact them); an untracked file larger than [`MAX_UNTRACKED_BYTES`] or not UTF-8 is skipped; the
//! mention set keeps the newest [`MAX_MENTIONS`] path-like tokens, so a file named only by a very early
//! tool call in a very long session can fall out of it. Only the hook's own repository root is read (the
//! bridge accepted several `--git-root`s).

use crate::hook_stream::{event, redact, stamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::time::Instant;

/// Files diffed per snapshot (most recently modified first).
pub(crate) const MAX_SNAPSHOT_FILES: usize = 80;
/// Characters of a diff sent; `full` is sent only when it fits too.
pub(crate) const MAX_DIFF_BYTES: usize = 120_000;
/// Tracked paths listed in the tree.
pub(crate) const MAX_TREE_PATHS: usize = 20_000;
/// Paths per `workspace_tree` event, which keeps each one well under the server's 256 KB payload bound.
const TREE_PART: usize = 2_500;
/// A commit this long after the session's last transcript entry is not the session's.
pub(crate) const COMMIT_GRACE_S: i64 = 300;
/// Seconds between snapshots on the per-tool-call doors; Stop and SessionEnd always take one.
pub(crate) const SNAPSHOT_EVERY_S: i64 = 10;
/// Kill one git call after this long.
const GIT_CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// All git work in one run, together.
const GIT_BUDGET: Duration = Duration::from_secs(8);
/// Bytes of one git call's output read; the rest is not read (a capped read is "cannot answer").
const MAX_GIT_OUTPUT_BYTES: u64 = 16_000_000;
/// An untracked file bigger than this is not read into a diff.
const MAX_UNTRACKED_BYTES: u64 = 4_000_000;
/// Path-like tokens remembered from the session's tool calls.
pub(crate) const MAX_MENTIONS: usize = 4_000;
/// Longest single token remembered.
const MAX_MENTION_CHARS: usize = 512;
/// `published` entries kept (path -> digest); oldest-inserted beyond this are forgotten (a re-send dedupes).
const MAX_PUBLISHED: usize = 5_000;
/// Directories whose files are never the session's work.
const SKIP: &[&str] = &[
    "node_modules/",
    ".next/",
    "__pycache__/",
    ".pytest_cache/",
    "coverage/",
];
/// The server's event-id alphabet and bound (`session_timeline._ID`, `MAX_EVENT_ID`).
const MAX_EVENT_ID: usize = 128;

/// What this module remembers between runs, in `<session>.git.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct GitState {
    #[serde(default)]
    pub(crate) last_snapshot: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(crate) first_at: Option<DateTime<Utc>>,
    /// Newest last: path-like tokens the session's tool calls named.
    #[serde(default)]
    pub(crate) mentions: Vec<String>,
    /// path -> digest of the last diff sent for it.
    #[serde(default)]
    pub(crate) published: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) commits: Vec<String>,
    #[serde(default)]
    pub(crate) tree: Option<String>,
}

fn hex16(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))[..16].to_string()
}

/// The bridge's id when the server would accept it; otherwise the same id with the path hashed.
fn safe_id(id: String, fallback: impl FnOnce() -> String) -> String {
    let ok = id.len() <= MAX_EVENT_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:#@/-".contains(c));
    if ok { id } else { fallback() }
}

/// Path-like tokens in `text`: maximal runs of path characters that contain `/` or `.`, or are a known
/// extensionless file name. A path made only of those characters is a substring of `text` exactly when it
/// is a substring of one of these tokens, which is what [`mentioned`] asks.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || "._/@+-~".contains(c)))
        .filter(|t| t.len() >= 3 && t.len() <= MAX_MENTION_CHARS)
        .filter(|t| {
            t.contains('/') || t.contains('.') || t.chars().next().is_some_and(char::is_uppercase)
        })
        .map(str::to_string)
        .collect()
}

/// Remember `texts`' tokens, newest last, deduplicated, bounded at [`MAX_MENTIONS`] (oldest dropped).
pub(crate) fn remember(state: &mut GitState, texts: &[String]) {
    for token in texts.iter().flat_map(|t| tokens(t)) {
        state.mentions.retain(|m| *m != token);
        state.mentions.push(token);
    }
    let over = state.mentions.len().saturating_sub(MAX_MENTIONS);
    state.mentions.drain(..over);
    assert!(
        state.mentions.len() <= MAX_MENTIONS,
        "the mention set is bounded"
    );
}

/// A changed file belongs to this session only if the session named it: its relative path, or its
/// `parent/name` tail, appears in something the session ran or edited (the bridge's rule).
pub(crate) fn mentioned(path: &str, mentions: &[String]) -> bool {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    let tail = parts.join("/");
    mentions
        .iter()
        .any(|m| m.contains(path) || m.contains(&tail))
}

/// The strings a Claude Code tool call names (`file_path`, `path`, `command`, `content`).
pub(crate) fn claude_mentions(entry: &Value) -> Vec<String> {
    let Some(Value::Array(blocks)) = entry.pointer("/message/content") else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .flat_map(|b| {
            ["file_path", "path", "command", "content"]
                .iter()
                .filter_map(|k| b.pointer(&format!("/input/{k}")).and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// git's file headers dropped, `@@` hunks and their lines kept.
pub(crate) fn hunks_only(diff: &str) -> String {
    const HEADERS: &[&str] = &[
        "diff --git",
        "index ",
        "--- ",
        "+++ ",
        "new file mode",
        "deleted file mode",
        "similarity",
        "rename ",
    ];
    let mut started = false;
    diff.lines()
        .filter(|line| {
            started |= line.starts_with("@@");
            started && !HEADERS.iter().any(|h| line.starts_with(h))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `git status --porcelain -uall` → `(code, relative path)`, renames resolved to their new name.
pub(crate) fn parse_status(status: &str) -> Vec<(String, String)> {
    status
        .lines()
        .filter(|line| line.len() > 3)
        .map(|line| {
            let rel = line[3..]
                .rsplit(" -> ")
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            (line[..2].to_string(), rel.to_string())
        })
        .filter(|(_, rel)| !rel.is_empty() && !SKIP.iter().any(|s| rel.contains(s)))
        .collect()
}

/// One `workspace_diff`, or `None` when the diff is empty or already sent with this digest.
pub(crate) fn diff_event(
    state: &mut GitState,
    path: &str,
    added: bool,
    (diff, full): (String, String),
    at: &str,
) -> Option<Value> {
    if diff.trim().is_empty() {
        return None;
    }
    let digest = hex16(&diff);
    if state.published.get(path) == Some(&digest) {
        return None;
    }
    let truncated = diff.chars().count() > MAX_DIFF_BYTES;
    let shown: String = diff.chars().take(MAX_DIFF_BYTES).collect();
    let full = (full.chars().count() <= MAX_DIFF_BYTES).then(|| redact(&full));
    if state.published.len() >= MAX_PUBLISHED && !state.published.contains_key(path) {
        let oldest = state.published.keys().next().cloned();
        oldest.map(|k| state.published.remove(&k));
    }
    state.published.insert(path.to_string(), digest.clone());
    let id = safe_id(format!("git:{path}:{digest}"), || {
        format!("git:{}:{digest}", hex16(path))
    });
    Some(event(
        id,
        "workspace_diff",
        json!({"kind": "system", "id": "git"}),
        at,
        json!({"path": path, "status": if added { "added" } else { "modified" }, "diff": redact(&shown), "full": full, "truncated": truncated}),
    ))
}

/// `git log --format=%H%x1f%an%x1f%aI%x1f%s --name-status` → commit events, skipping any made more than
/// [`COMMIT_GRACE_S`] after `last` and any already sent. `at` is UTC and never later than `last`.
pub(crate) fn commit_events(
    log: &str,
    repo: &str,
    last: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    sent: &[String],
) -> Vec<Value> {
    let mut events: Vec<Value> = Vec::new();
    let mut open = false;
    for line in log.lines() {
        let fields: Vec<&str> = line.splitn(4, '\x1f').collect();
        if let [sha, author, at, subject] = fields[..] {
            let when = DateTime::parse_from_rfc3339(at)
                .ok()
                .map(|t| t.with_timezone(&Utc));
            let late = matches!((when, last), (Some(w), Some(l)) if (w - l).num_seconds() > COMMIT_GRACE_S);
            open = !late && when.is_some() && !sent.iter().any(|s| s == sha);
            if open {
                events.push(event(
                    format!("commit:{sha}"),
                    "commit",
                    json!({"kind": "agent", "id": author.chars().take(60).collect::<String>()}),
                    &stamp(when, last, now),
                    json!({"sha": sha, "subject": redact(subject), "repo": repo, "files": []}),
                ));
            }
        } else if open
            && !line.trim().is_empty()
            && let Some(files) = events
                .last_mut()
                .and_then(|e| e["payload"]["files"].as_array_mut())
        {
            files.push(json!(line.rsplit('\t').next().unwrap_or("")));
        }
    }
    events
}

/// The tracked paths as `workspace_tree` parts, or nothing when the list has not changed since last sent.
pub(crate) fn tree_events(
    listing: &str,
    root_name: &str,
    state: &mut GitState,
    at: &str,
) -> Vec<Value> {
    let paths: Vec<&str> = listing.lines().take(MAX_TREE_PATHS).collect();
    let digest = hex16(&paths.join("\n"));
    if state.tree.as_deref() == Some(digest.as_str()) {
        return Vec::new();
    }
    state.tree = Some(digest.clone());
    let parts = paths.len().div_ceil(TREE_PART).max(1);
    let name = safe_id(root_name.to_string(), || hex16(root_name));
    (0..parts)
        .map(|i| {
            let chunk: Vec<&str> = paths.iter().skip(i * TREE_PART).take(TREE_PART).copied().collect();
            event(
                format!("tree:{name}:{digest}:{i}"),
                "workspace_tree",
                json!({"kind": "system", "id": "git"}),
                at,
                json!({"root": root_name, "digest": digest, "part": i, "parts": parts, "paths": chunk, "truncated": paths.len() >= MAX_TREE_PATHS}),
            )
        })
        .collect()
}

/// One bounded git call: `None` when it failed, timed out, ran past `deadline`, or printed more than
/// [`MAX_GIT_OUTPUT_BYTES`] (a capped read cannot answer, so it is not passed off as the whole output).
pub(crate) async fn git(root: &Path, args: &[&str], deadline: Instant) -> Option<String> {
    let stop = deadline.min(Instant::now() + GIT_CALL_TIMEOUT);
    let mut child = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut out = Vec::new();
    let stdout = child.stdout.take()?;
    let mut limited = stdout.take(MAX_GIT_OUTPUT_BYTES + 1);
    tokio::time::timeout_at(stop, limited.read_to_end(&mut out))
        .await
        .ok()?
        .ok()?;
    let status = tokio::time::timeout_at(stop, child.wait())
        .await
        .ok()?
        .ok()?;
    (status.success() && out.len() as u64 <= MAX_GIT_OUTPUT_BYTES)
        .then(|| String::from_utf8_lossy(&out).into_owned())
}

/// Everything one run needs from the caller.
pub(crate) struct GitInput<'a> {
    pub(crate) root: &'a Path,
    pub(crate) state_file: &'a Path,
    /// The session's first entry (the snapshot's mtime floor and the log's `--since`).
    pub(crate) first_at: Option<DateTime<Utc>>,
    pub(crate) last: Option<DateTime<Utc>>,
    pub(crate) now: DateTime<Utc>,
    /// Stop / SessionEnd: snapshot regardless of [`SNAPSHOT_EVERY_S`].
    pub(crate) force: bool,
    pub(crate) mentions: Vec<String>,
}

/// The saved state, or a fresh one when there is none or it does not parse.
pub(crate) fn load(path: &Path) -> GitState {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The state to keep when the run's events did NOT land: the old record of what was sent, plus what this
/// run LEARNED (mentions, the session's start) — those came from transcript lines that will not be read
/// again, so dropping them would hide the session's changes from every later run.
pub(crate) fn unsent(path: &Path, learned: GitState) -> GitState {
    GitState {
        mentions: learned.mentions,
        first_at: learned.first_at,
        ..load(path)
    }
}

/// The git events this run owes, and the state to save once the server has taken them.
pub(crate) async fn collect(input: &GitInput<'_>) -> (Vec<Value>, GitState) {
    let mut state = load(input.state_file);
    remember(&mut state, &input.mentions);
    state.first_at = state.first_at.or(input.first_at);
    let due = input.force
        || state
            .last_snapshot
            .is_none_or(|t| (input.now - t).num_seconds() >= SNAPSHOT_EVERY_S);
    let Some(since) = state.first_at.filter(|_| due) else {
        return (Vec::new(), state);
    };
    let deadline = Instant::now() + GIT_BUDGET;
    // The hook may run from a subdirectory; every path below is relative to the repository's top level.
    let Some(top) = git(input.root, &["rev-parse", "--show-toplevel"], deadline)
        .await
        .map(|out| std::path::PathBuf::from(out.trim()))
        .filter(|top| top.is_dir())
    else {
        return (Vec::new(), state);
    };
    state.last_snapshot = Some(input.now);
    let top = top.as_path();
    let mut events = snapshot(top, input, &mut state, since, deadline).await;
    let log_since = format!("--since=@{}", since.timestamp());
    let format = "--format=%H%x1f%an%x1f%aI%x1f%s";
    if let Some(log) = git(top, &["log", &log_since, format, "--name-status"], deadline).await {
        let repo = root_name(top);
        let commits = commit_events(&log, &repo, input.last, input.now, &state.commits);
        state.commits.extend(
            commits
                .iter()
                .filter_map(|c| c["payload"]["sha"].as_str().map(str::to_string)),
        );
        let over = state.commits.len().saturating_sub(MAX_PUBLISHED);
        state.commits.drain(..over);
        events.extend(commits);
    }
    if let Some(listing) = git(top, &["ls-files"], deadline).await {
        let at = stamp(None, input.last, input.now);
        events.extend(tree_events(&listing, &root_name(top), &mut state, &at));
    }
    (events, state)
}

fn root_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn mtime(path: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
}

/// The changed files the session named, newest first, diffed against HEAD (untracked = a whole new file).
async fn snapshot(
    top: &Path,
    input: &GitInput<'_>,
    state: &mut GitState,
    since: DateTime<Utc>,
    deadline: Instant,
) -> Vec<Value> {
    let Some(status) = git(top, &["status", "--porcelain", "-uall"], deadline).await else {
        return Vec::new();
    };
    let mut changed: Vec<(String, String, DateTime<Utc>)> = parse_status(&status)
        .into_iter()
        .filter_map(|(code, rel)| {
            let full = top.join(&rel);
            let when = mtime(&full).filter(|_| full.is_file())?;
            (when >= since && mentioned(&rel, &state.mentions)).then_some((code, rel, when))
        })
        .collect();
    changed.sort_by_key(|c| std::cmp::Reverse(c.2));
    let mut events = Vec::new();
    for (code, rel, when) in changed.into_iter().take(MAX_SNAPSHOT_FILES) {
        let added = code.trim() == "??";
        let Some(pair) = file_diff(top, &rel, added, deadline).await else {
            continue; // out of budget or unreadable: not marked published, so the next run retries
        };
        let at = stamp(Some(when), input.last, input.now);
        events.extend(diff_event(state, &rel, added, pair, &at));
    }
    events
}

async fn file_diff(
    root: &Path,
    rel: &str,
    added: bool,
    deadline: Instant,
) -> Option<(String, String)> {
    if added {
        let full = root.join(rel);
        if std::fs::metadata(&full).ok()?.len() > MAX_UNTRACKED_BYTES {
            return None;
        }
        let body = std::fs::read_to_string(&full).ok()?;
        let lines: Vec<&str> = body.lines().collect();
        let mut diff = format!("@@ -0,0 +1,{} @@", lines.len());
        for line in &lines {
            diff.push_str("\n+");
            diff.push_str(line);
        }
        return Some((diff.clone(), diff));
    }
    let short = git(root, &["diff", "HEAD", "-U3", "--", rel], deadline).await?;
    let long = git(root, &["diff", "HEAD", "-U100000", "--", rel], deadline).await?;
    Some((hunks_only(&short), hunks_only(&long)))
}

#[cfg(test)]
#[path = "hook_stream_git_tests.rs"]
mod tests;
