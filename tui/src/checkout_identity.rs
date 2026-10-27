//! Which checkout an edit came from, so the server can keep lane edits out of the default-branch graph.
//!
//! 🔴 **EVERY LANE WAS A WRITER OF THE ONE SHARED GRAPH.** The `sync` hook POSTs `/reindex` after every
//! agent write, from whatever checkout the agent is in, with no branch identity. On uqeu/estelle
//! (2026-10-27) dozens of `git worktree` lanes wrote feature-branch bytes into the graph the merge gate
//! grades every pull request against: 9,676 graphed files against 7,686 on `main`.
//!
//! The server owns the rule (`estelle.serve.checkout_identity.off_default_reason`); this module only
//! DECLARES the facts it needs, and reads the server's `skipped_off_default` answer back so a skipped
//! write never stamps the local index as current.
//!
//! Bounded: three short `git` reads per flush batch, never per file.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use serde_json::json;

/// What git says about this checkout. `None` fields mean "git could not say" and are sent as absent,
/// which the server reads as permissive — the behaviour of every client before this field existed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Checkout {
    pub(crate) branch: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) linked_worktree: bool,
}

impl Checkout {
    /// The `checkout` object for a `/reindex` body.
    pub(crate) fn to_json(&self) -> Value {
        let mut out = json!({ "linked_worktree": self.linked_worktree });
        if let Some(branch) = &self.branch {
            out["branch"] = json!(branch);
        }
        if let Some(default) = &self.default_branch {
            out["default_branch"] = json!(default);
        }
        out
    }
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// `origin/main` → `main`. `None` for anything that is not a remote-tracking short name.
pub(crate) fn default_from_origin_head(short: &str) -> Option<String> {
    let name = short.strip_prefix("origin/")?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// A linked worktree's git dir differs from the repository's common dir; the primary checkout's do not.
pub(crate) fn is_linked(git_dir: &Path, common_dir: &Path) -> bool {
    let canon = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    canon(git_dir) != canon(common_dir)
}

/// Read this checkout's identity. Never fails: an unreadable fact is absent, not guessed.
pub(crate) fn read(root: &Path) -> Checkout {
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|name| name != "HEAD");
    let default_branch = git(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .and_then(|short| default_from_origin_head(&short));
    let resolve = |dir: String| {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let linked = match (
        git(root, &["rev-parse", "--git-dir"]).map(resolve),
        git(root, &["rev-parse", "--git-common-dir"]).map(resolve),
    ) {
        (Some(dir), Some(common)) => is_linked(&dir, &common),
        _ => false,
    };
    Checkout {
        branch,
        default_branch,
        linked_worktree: linked,
    }
}

/// Whether the server declined to write this batch into the default-branch graph.
pub(crate) fn skipped_off_default(response: &Value) -> bool {
    response.get("skipped_off_default").and_then(Value::as_bool) == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn git_in(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn origin_head_names_the_default_branch() {
        assert_eq!(
            default_from_origin_head("origin/main"),
            Some("main".to_string())
        );
        assert_eq!(
            default_from_origin_head("origin/trunk"),
            Some("trunk".to_string())
        );
        assert_eq!(default_from_origin_head("main"), None);
        assert_eq!(default_from_origin_head("origin/"), None);
    }

    #[test]
    fn absent_facts_are_absent_on_the_wire() {
        let body = Checkout::default().to_json();
        assert_eq!(body, json!({"linked_worktree": false}));
    }

    #[test]
    fn the_server_skip_is_read_by_name_only() {
        assert!(skipped_off_default(
            &json!({"skipped_off_default": true, "indexed": 0})
        ));
        assert!(!skipped_off_default(&json!({"indexed": 0})));
        assert!(!skipped_off_default(
            &json!({"skipped_off_default": "true"})
        ));
    }

    #[test]
    fn a_primary_checkout_and_a_linked_lane_read_differently() {
        let root = tempfile::tempdir().expect("root");
        let main = root.path().join("main");
        std::fs::create_dir_all(&main).expect("dir");
        git_in(&main, &["init", "-q", "-b", "main"]);
        std::fs::write(main.join("a.py"), "x = 1\n").expect("write");
        git_in(&main, &["add", "a.py"]);
        git_in(&main, &["commit", "-q", "-m", "init"]);
        let lane = root.path().join("lane");
        git_in(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "lane/x",
                lane.to_str().expect("utf8"),
            ],
        );

        let primary = read(&main);
        assert_eq!(primary.branch.as_deref(), Some("main"));
        assert!(
            !primary.linked_worktree,
            "the primary checkout is not a linked worktree"
        );

        let linked = read(&lane);
        assert_eq!(linked.branch.as_deref(), Some("lane/x"));
        assert!(
            linked.linked_worktree,
            "a `git worktree add` checkout must say so"
        );
    }
}
