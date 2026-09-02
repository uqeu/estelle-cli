//! The half of the grounding gate that can REFUSE, and the half that says "I did not look".
//!
//! 🔴 **THE RUNNER CUSTOMERS EXECUTE COULD NOT BLOCK — NOT "DID NOT", *COULD NOT*.** Before this
//! module existed, `grep -rn "permissionDecision" tui/src` returned nothing outside tests: every
//! branch of `ground_hook` returned `Ok(vec![hook_message(..)])`, and `hook_message` emitted only
//! `systemMessage` + `hookSpecificOutput.additionalContext`. Measured against a real wrong-arity
//! finding, the hook printed *"Estelle flagged probe_arity.py: signature mismatch: tokenize()
//! takes at most 1 positional argument(s), 6 given. Edit not blocked."* and exited 0. It saw the
//! defect, named it correctly, and let the edit through — while the product's whole claim is that
//! it refuses what it cannot prove. The Python twin (`scripts/hooks/estelle_hook.py:349,469`) had
//! the path and the opt-in; the shipped runner had neither.
//!
//! 🔴 **AND "NOT CHECKED" WAS BYTE-IDENTICAL TO "CLEAN".** `ground_hook` returned an EMPTY vector
//! for every non-`.py` file while the installed matcher is `Write|Edit` — every file type. A
//! TypeScript write produced exit 0 and empty stdout, which is exactly what a clean pass produces.
//! That is this repo's absent-vs-zero rule turned on the gate itself. A capped or out-of-scope
//! check must say **"cannot answer"**; silence is a claim it has no evidence for.
//!
//! Everything here is pure or filesystem-only — no socket, no credential — so the decision logic
//! is tested without a server, and each guard below has a mutant that turns it red.

use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// The file extensions `estelle hook ground` actually analyses.
///
/// ⚠️ **PINNED ON PURPOSE, AND THE TEST THAT PINS IT IS THE POINT.** The analysis genuinely only
/// understands Python today; widening this list is a capability claim, so it must be a deliberate
/// edit that turns a test red rather than something a reader discovers from behaviour. Everything
/// NOT in this list gets an explicit abstention — never silence.
pub const GROUND_EXTENSIONS: &[&str] = &["py"];

/// Opt-in for the refusal, default OFF, same name and same accepted values as the Python twin
/// (`estelle_hook.py:469`). A gate that starts refusing edits on an existing install without the
/// operator asking for it is a support incident, and every other autonomy dial in this product is
/// opt-in. The values are matched case-insensitively after trimming: `1`, `true`, `on`.
pub const BLOCK_ENV: &str = "ESTELLE_HOOK_BLOCK";

/// Test seam for the freshness stamp. Production resolves `~/.estelle/reindex-stamp.json`.
pub const STAMP_ENV: &str = "ESTELLE_REINDEX_STAMP";

/// Directories the freshness walk skips.
///
/// 🔴 **THE DIRECTION OF ERROR IS NOT SYMMETRIC, AND THIS LIST IS WHERE IT IS DECIDED.** Skipping
/// a directory the ingest DOES index hides a newly-written file, which makes a stale index look
/// current — a FALSE BLOCK on real code. Failing to skip a directory the ingest skips only makes a
/// current index look behind, which degrades to advisory. So this list must stay a **subset** of
/// what the ingest skips (`top_level::SKIP_DIRECTORIES`), and it is deliberately not one entry
/// longer. A `.venv` full of freshly-installed packages therefore keeps a repo "behind" and the
/// gate advisory; that is the safe way to be wrong, and it is stated rather than hidden.
pub const FRESHNESS_SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

/// Bound the resource BEFORE taking it (Power of Ten, rule 3). A PreToolUse hook that walks an
/// unbounded tree is a hang in the editor. Exceeding either bound means **"cannot answer"** —
/// which resolves to stale, which resolves to advisory — never "that is all there is".
pub const FRESHNESS_MAX_ENTRIES: usize = 20_000;
/// Wall-clock ceiling on the same walk. Only reached on the flagged-and-opted-in path, which is
/// the rare one; a clean or abstaining verdict never walks at all.
pub const FRESHNESS_DEADLINE_MS: u128 = 2_000;

/// Whether the grounding analysis can speak about this edit at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroundScope {
    /// In scope: run the gate.
    Analysable,
    /// Out of scope, and this is the sentence that must be SAID rather than swallowed.
    Abstain(String),
}

/// What happened to a FLAGGED finding, named so the three cases cannot be collapsed into one
/// word. The Python twin's advisory message blames freshness on both allow-branches; here the
/// message names the branch it actually took, because "we chose not to block" and "we could not
/// be sure" send a reader to different places.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlaggedOutcome {
    /// Refused. The symbol is absent AND the index is current for this repo, so there is nothing
    /// left for it to be except wrong.
    Blocked,
    /// Allowed: refusing is not switched on for this install (`ESTELLE_HOOK_BLOCK`).
    NotOptedIn,
    /// Allowed: the index is behind this repo, so a flagged symbol may be one it has not seen.
    /// **This is the branch that stops the gate refusing real code just because we are behind.**
    IndexBehind,
}

/// The one owner of "did this hook refuse". Pure, exhaustive, and mutation-proven: flipping either
/// input turns a specific test red.
pub fn flagged_outcome(opted_in: bool, index_current: bool) -> FlaggedOutcome {
    match (opted_in, index_current) {
        (false, _) => FlaggedOutcome::NotOptedIn,
        (true, false) => FlaggedOutcome::IndexBehind,
        (true, true) => FlaggedOutcome::Blocked,
    }
}

/// `.py` — the supported set rendered for a human-facing sentence.
pub fn supported_extensions_phrase() -> String {
    GROUND_EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Can the gate speak about this edit? Every `Abstain` string is a customer-facing reason, and
/// each one names what was NOT done rather than implying a pass.
pub fn ground_scope(path: &str, code: &str) -> GroundScope {
    if path.trim().is_empty() {
        return GroundScope::Abstain(
            "the host payload carried no file path, so there was nothing to locate or check"
                .to_string(),
        );
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension {
        Some(extension) if GROUND_EXTENSIONS.contains(&extension.as_str()) => {}
        Some(extension) => {
            return GroundScope::Abstain(format!(
                "Estelle's grounding analysis covers {} only, and this is a .{extension} file, so NOTHING in this edit was checked",
                supported_extensions_phrase()
            ));
        }
        None => {
            return GroundScope::Abstain(format!(
                "Estelle's grounding analysis covers {} only, and this file has no extension to match, so NOTHING in this edit was checked",
                supported_extensions_phrase()
            ));
        }
    }
    if code.trim().is_empty() {
        return GroundScope::Abstain(
            "the edit carried no code to check (an empty write or a pure deletion)".to_string(),
        );
    }
    GroundScope::Analysable
}

/// Is refusing switched on for this install? Reads the process environment.
pub fn blocking_enabled() -> bool {
    blocking_enabled_from(std::env::var(BLOCK_ENV).ok().as_deref())
}

/// The pure half, so the accepted spellings are pinned without touching the environment.
pub fn blocking_enabled_from(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "on")
    )
}

/// `~/.estelle/reindex-stamp.json`, or `$ESTELLE_REINDEX_STAMP` when set.
///
/// ⚠️ **DELIBERATELY THE SAME FILE THE PYTHON TWIN WRITES** (`estelle_hook.py::_stamp_path`), in
/// the same `{repo: unix_seconds}` shape. Two hooks with two freshness signals would disagree, and
/// a disagreement here is a block on one host and a pass on the other for the same edit.
pub fn stamp_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(STAMP_ENV) {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".estelle").join("reindex-stamp.json"))
}

/// Record that `repo`'s index was just updated. **Best effort by design**: a stamp we cannot write
/// degrades to "stale", which costs a block and never causes one.
pub fn mark_indexed(repo: &str) {
    let Some(path) = stamp_path() else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or_default();
    let _ = mark_indexed_at(&path, repo, now);
}

/// The testable half. Returns `Err` only so tests can assert the write happened; production
/// ignores it on purpose (see `mark_indexed`).
pub fn mark_indexed_at(path: &Path, repo: &str, now: f64) -> Result<(), String> {
    if repo.trim().is_empty() {
        return Err("no repo to stamp".to_string());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut stamps = std::fs::read(path)
        .ok()
        .and_then(|bytes| {
            serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes).ok()
        })
        .unwrap_or_default();
    stamps.insert(repo.to_string(), serde_json::json!(now));
    let body = serde_json::to_vec(&serde_json::Value::Object(stamps))
        .map_err(|error| error.to_string())?;
    std::fs::write(path, body).map_err(|error| error.to_string())
}

/// Whether Estelle's index can be trusted to know this repo's current Python symbols.
///
/// **THE SECOND HALF OF "FLAGGED".** A not-found verdict is two signals: *the symbol is absent
/// from the index* AND *how fresh that index is*. Only the pair justifies refusing an edit; the
/// first alone refuses real code whenever we have simply not caught up.
///
/// ⚠️ **THIS PROXY WAS MEASURED INSUFFICIENT AND THAT IS WHY THE OPT-IN STAYS.** A repo-level
/// stamp records that a reindex was POSTED, not that the index HOLDS this file's current content —
/// the twin blocked a real symbol (`resolve_grounding_scope`) with the stamp reading "current",
/// because the server's index held an older revision of its module. The opt-in comes off when the
/// freshness signal comes from the SERVER, not before. Stated here rather than in a footnote.
///
/// ⚠️ **THE CONCRETE SHAPE OF THAT HOLE, SO NOBODY HAS TO REDISCOVER IT.** The stamp is written
/// when a reindex of *one* file succeeds, but it is compared against the *newest* mtime in the
/// tree. Edit `b.py` outside the hook (a shell `sed`, a rebase, an editor with no PreToolUse), then
/// edit `a.py` through the agent: the sync hook reindexes `a.py` and stamps *now*, `b.py`'s older
/// mtime clears the comparison, and the repo reads "current" while the index has never seen
/// `b.py`. A symbol defined there would be flagged and refused. This is the exact reason the
/// refusal is opt-in and default OFF, and the exact thing a server-supplied per-file signal fixes.
///
/// **Every failure mode returns `false` (= stale = advisory)**: no stamp, unreadable stamp, no
/// repo, an unwalkable tree, a walk that hit either bound. Being unsure must never produce a
/// refusal.
pub fn index_is_current(repo: &str, root: &Path) -> bool {
    let Some(path) = stamp_path() else {
        return false;
    };
    index_is_current_at(&path, repo, root)
}

/// The testable half of `index_is_current`.
pub fn index_is_current_at(stamp: &Path, repo: &str, root: &Path) -> bool {
    if repo.trim().is_empty() {
        return false;
    }
    let Some(stamped) = read_stamp(stamp, repo) else {
        return false;
    };
    let Some(newest) = newest_source_mtime(root) else {
        return false;
    };
    newest <= stamped
}

fn read_stamp(path: &Path, repo: &str) -> Option<f64> {
    let bytes = std::fs::read(path).ok()?;
    let stamps = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    stamps.get(repo)?.as_f64()
}

/// The newest mtime, in unix seconds, of any file this gate could analyse under `root`.
///
/// `None` means **"cannot answer"** — an unreadable entry, a bound reached, a clock before the
/// epoch. It is never "there are none": a tree with no `.py` files answers `Some(0.0)`, which
/// reads as "nothing here is newer than any stamp", and that is a fact rather than a shrug.
fn newest_source_mtime(root: &Path) -> Option<f64> {
    let started = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    let mut newest = 0.0f64;
    // BOUNDED LOOP (Power of Ten, rule 2): the stack only grows from directories that were
    // themselves counted against `FRESHNESS_MAX_ENTRIES`, so the walk cannot outlive that bound.
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).ok()? {
            visited += 1;
            if visited > FRESHNESS_MAX_ENTRIES
                || started.elapsed().as_millis() > FRESHNESS_DEADLINE_MS
            {
                return None;
            }
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if file_type.is_symlink() {
                // Not followed: a symlinked tree can loop, and an unbounded walk is the one thing
                // a PreToolUse hook must never do.
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !FRESHNESS_SKIP_DIRECTORIES.contains(&name.as_ref()) {
                    stack.push(entry.path());
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            let analysable = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|extension| GROUND_EXTENSIONS.contains(&extension.as_str()));
            if !analysable {
                continue;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            let seconds = modified.duration_since(UNIX_EPOCH).ok()?.as_secs_f64();
            if seconds > newest {
                newest = seconds;
            }
        }
    }
    Some(newest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, body).expect("write file");
    }

    /// 🔒 THE PINNED SET. Widening the gate to another language must be a deliberate edit that
    /// turns THIS red — never a capability a reader discovers from behaviour.
    #[test]
    fn the_supported_extension_set_is_exactly_python() {
        assert_eq!(GROUND_EXTENSIONS, &["py"]);
        assert_eq!(supported_extensions_phrase(), ".py");
    }

    #[test]
    fn a_python_edit_with_code_is_analysable() {
        assert_eq!(
            ground_scope("/repo/thing.py", "def go():\n    return 1\n"),
            GroundScope::Analysable
        );
    }

    /// 🔴 THE D3 REGRESSION. Before this, a TypeScript write produced an EMPTY vector — byte
    /// identical to a clean pass. The abstention must name Python, name the file's own extension,
    /// and say NOTHING was checked.
    #[test]
    fn a_non_python_edit_abstains_out_loud_instead_of_going_silent() {
        let GroundScope::Abstain(detail) = ground_scope("/repo/app.ts", "export const a = 1;")
        else {
            panic!("a .ts edit must abstain, not be analysed");
        };
        assert!(
            detail.contains(".py"),
            "must name what IS covered: {detail}"
        );
        assert!(
            detail.contains(".ts"),
            "must name what was skipped: {detail}"
        );
        assert!(
            detail.contains("NOTHING in this edit was checked"),
            "must deny the clean reading: {detail}"
        );
    }

    #[test]
    fn an_extensionless_file_abstains_and_says_why() {
        let GroundScope::Abstain(detail) = ground_scope("/repo/Makefile", "all:\n\techo hi\n")
        else {
            panic!("an extensionless edit must abstain");
        };
        assert!(detail.contains("no extension"), "{detail}");
    }

    #[test]
    fn a_missing_file_path_abstains_rather_than_naming_an_empty_file() {
        let GroundScope::Abstain(detail) = ground_scope("", "def go(): pass") else {
            panic!("a payload with no path must abstain");
        };
        assert!(detail.contains("no file path"), "{detail}");
    }

    #[test]
    fn an_empty_python_edit_abstains_rather_than_reporting_clean() {
        let GroundScope::Abstain(detail) = ground_scope("/repo/thing.py", "   \n  ") else {
            panic!("an empty edit must abstain");
        };
        assert!(detail.contains("no code to check"), "{detail}");
    }

    #[test]
    fn the_extension_match_is_case_insensitive() {
        assert_eq!(
            ground_scope("/repo/Thing.PY", "def go(): pass"),
            GroundScope::Analysable
        );
    }

    /// The opt-in spellings, pinned. Anything else — including `yes`, `0` and unset — is OFF.
    #[test]
    fn blocking_is_off_unless_explicitly_switched_on() {
        for on in ["1", "true", "on", "  TRUE  ", "On"] {
            assert!(blocking_enabled_from(Some(on)), "{on:?} should enable");
        }
        for off in ["", "0", "false", "off", "yes", "2", "no"] {
            assert!(
                !blocking_enabled_from(Some(off)),
                "{off:?} should not enable"
            );
        }
        assert!(
            !blocking_enabled_from(None),
            "an unset variable is the DEFAULT, and the default is advisory"
        );
    }

    /// 🔴 THE THREE-WAY LOGIC, exhaustively. Every row is a different message to the customer.
    #[test]
    fn a_flagged_finding_blocks_only_when_opted_in_and_the_index_is_current() {
        assert_eq!(flagged_outcome(true, true), FlaggedOutcome::Blocked);
        assert_eq!(flagged_outcome(true, false), FlaggedOutcome::IndexBehind);
        assert_eq!(flagged_outcome(false, true), FlaggedOutcome::NotOptedIn);
        assert_eq!(flagged_outcome(false, false), FlaggedOutcome::NotOptedIn);
    }

    #[test]
    fn a_stamp_newer_than_every_source_file_reads_as_current() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("pkg/module.py"), "def go(): pass\n");
        let stamp = home.path().join("reindex-stamp.json");
        let future = SystemTime::now()
            .checked_add(Duration::from_secs(600))
            .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
            .expect("future")
            .as_secs_f64();
        mark_indexed_at(&stamp, "uqeu/estelle", future).expect("stamp");

        assert!(index_is_current_at(&stamp, "uqeu/estelle", &repo_root));
    }

    /// 🔴 THE FALSE-BLOCK GUARD. A file newer than the stamp means the index may not have seen it,
    /// and an unseen symbol is not an absent one.
    #[test]
    fn a_source_file_newer_than_the_stamp_reads_as_behind() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("pkg/module.py"), "def go(): pass\n");
        let stamp = home.path().join("reindex-stamp.json");
        let past = SystemTime::now()
            .checked_sub(Duration::from_secs(600))
            .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
            .expect("past")
            .as_secs_f64();
        mark_indexed_at(&stamp, "uqeu/estelle", past).expect("stamp");

        assert!(!index_is_current_at(&stamp, "uqeu/estelle", &repo_root));
    }

    #[test]
    fn a_stamp_for_another_repo_never_makes_this_one_current() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("module.py"), "def go(): pass\n");
        let stamp = home.path().join("reindex-stamp.json");
        let future = SystemTime::now()
            .checked_add(Duration::from_secs(600))
            .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
            .expect("future")
            .as_secs_f64();
        mark_indexed_at(&stamp, "someone/else", future).expect("stamp");

        assert!(!index_is_current_at(&stamp, "uqeu/estelle", &repo_root));
        assert!(
            !index_is_current_at(&stamp, "", &repo_root),
            "an unresolved repo is never current"
        );
    }

    #[test]
    fn a_missing_or_unreadable_stamp_is_stale_never_current() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("module.py"), "def go(): pass\n");
        let absent = home.path().join("nothing-here.json");
        assert!(!index_is_current_at(&absent, "uqeu/estelle", &repo_root));

        let corrupt = home.path().join("corrupt.json");
        write(&corrupt, "{not json");
        assert!(!index_is_current_at(&corrupt, "uqeu/estelle", &repo_root));
    }

    #[test]
    fn an_unwalkable_root_is_stale_never_current() {
        let home = tempfile::tempdir().expect("tempdir");
        let stamp = home.path().join("reindex-stamp.json");
        mark_indexed_at(&stamp, "uqeu/estelle", 1.0e12).expect("stamp");
        assert!(!index_is_current_at(
            &stamp,
            "uqeu/estelle",
            &home.path().join("does-not-exist")
        ));
    }

    /// A tree the ingest also skips must not hold freshness hostage — but the skip list may never
    /// grow past the ingest's, because THAT direction is a false block.
    #[test]
    fn a_build_output_directory_is_not_walked() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("keep.py"), "def go(): pass\n");
        for skipped in FRESHNESS_SKIP_DIRECTORIES {
            write(&repo_root.join(skipped).join("vendored.py"), "x = 1\n");
        }
        let newest = newest_source_mtime(&repo_root).expect("walkable");
        let kept = fs::metadata(repo_root.join("keep.py"))
            .and_then(|meta| meta.modified())
            .expect("mtime")
            .duration_since(UNIX_EPOCH)
            .expect("epoch")
            .as_secs_f64();
        assert!(
            (newest - kept).abs() < 1e-6,
            "the walk must report the tracked file's mtime, got {newest} vs {kept}"
        );
    }

    #[test]
    fn a_tree_with_no_python_answers_zero_rather_than_cannot_answer() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        write(&repo_root.join("README.md"), "# hi\n");
        assert_eq!(newest_source_mtime(&repo_root), Some(0.0));
    }

    #[test]
    fn the_stamp_file_keeps_other_repos_and_matches_the_python_twins_shape() {
        let home = tempfile::tempdir().expect("tempdir");
        let stamp = home.path().join("reindex-stamp.json");
        mark_indexed_at(&stamp, "one/repo", 100.0).expect("stamp one");
        mark_indexed_at(&stamp, "two/repo", 200.0).expect("stamp two");
        let parsed: serde_json::Value =
            serde_json::from_slice(&fs::read(&stamp).expect("read")).expect("parse");
        assert_eq!(parsed["one/repo"].as_f64(), Some(100.0));
        assert_eq!(parsed["two/repo"].as_f64(), Some(200.0));
    }

    #[test]
    fn stamping_without_a_repo_is_refused_rather_than_writing_an_empty_key() {
        let home = tempfile::tempdir().expect("tempdir");
        let stamp = home.path().join("reindex-stamp.json");
        assert!(mark_indexed_at(&stamp, "   ", 100.0).is_err());
        assert!(!stamp.exists());
    }

    /// 🔴 A CAPPED READ MEANS "CANNOT ANSWER", NEVER "THAT IS ALL THERE IS". The bound has to be
    /// reachable to be a bound, so this walks a tree wider than it.
    #[test]
    fn a_tree_past_the_entry_bound_cannot_answer_and_so_stays_advisory() {
        let home = tempfile::tempdir().expect("tempdir");
        let repo_root = home.path().join("repo");
        fs::create_dir_all(&repo_root).expect("root");
        for index in 0..=FRESHNESS_MAX_ENTRIES {
            fs::write(repo_root.join(format!("f{index}.txt")), b"x").expect("write");
        }
        assert_eq!(newest_source_mtime(&repo_root), None);

        let stamp = home.path().join("reindex-stamp.json");
        mark_indexed_at(&stamp, "uqeu/estelle", 1.0e12).expect("stamp");
        assert!(
            !index_is_current_at(&stamp, "uqeu/estelle", &repo_root),
            "a walk that hit its bound must degrade to advisory, not to current"
        );
    }
}
