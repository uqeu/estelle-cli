//! The Bash guard — the classic shell foot-guns, ported clause-for-clause from the Python hook
//! (`scripts/hooks/estelle_hook.py::dangerous_command`) and pinned to it, clause by clause, by
//! `top_level::tests::rust_guard_matches_the_python_hook_contract`. Advisory in every branch: the
//! caller names the risk and the command still proceeds, because a false-positive hard block is
//! its own kind of damage. Blocking is a founder decision, not a guard's.
//!
//! 🔴 THE RM RULE DEFINES THE GOOD REGION; IT DOES NOT ENUMERATE THE BAD ONE. This file used to
//! carry a hand-written list of catastrophic paths — `/`, `~`, `$HOME`, `/etc`, `/usr`, `/Users` —
//! and `rm -rf ~/Desktop` sailed straight through it in silence. So did `~/Documents`, `~/.ssh`,
//! `./src` and `../sibling-repo`. The founder found the hole on his first guess, which is how long
//! such a list survives contact. **A guard written as a list of bad things guards exactly the bad
//! things somebody already imagined**, and the fix is never to append one more row.
//!
//! So the rule is inverted: a recursive force-delete is worth a second look ALWAYS, unless every
//! one of its targets is a regenerable build artifact ([`DISPOSABLE`]) or lives under a scratch
//! root ([`SCRATCH`]). Those two sets are finite and knowable; the set of paths that matter is
//! not. A path nobody anticipated now lands on the WARNED side of the fence rather than the silent
//! one — being wrong costs a warning, where before it cost the directory.
//!
//! ⚠️ THE COST IS REAL AND ACCEPTED: this fires more often than the rule it replaced. An advisory
//! guard that cries wolf is muted within a day and then misses the one that mattered, which is the
//! entire reason the disposable list exists — `rm -rf node_modules` is the most common recursive
//! delete in this repo and it stays silent. If it proves noisy, the fix is to GROW [`DISPOSABLE`],
//! never to re-enumerate the dangerous paths.
//!
//! LIMITS, out loud. This is a SHELL-STRING matcher: it reasons about the text of a command, never
//! about what the command will do. An alias, a variable that expands to a path, a wrapper script, a
//! heredoc or a `$(…)` substitution all defeat it, and nothing here changes that. It reads the
//! command it is shown; it does not read the filesystem.

use std::sync::LazyLock;

use regex_lite::Regex;

/// How many targets one `rm` line is read to. Bounded because a guard must never be the slow part
/// of the tool call it guards (Power of Ten #2: every loop has a stated bound). Reading STOPS
/// here — it does not conclude; see [`rm_targets`].
const MAX_RM_TARGETS: usize = 32;
/// How many leading `-flags` are classified before the rest of the line is read as targets.
const MAX_RM_FLAGS: usize = 16;
/// How many `rm` words on one command line are examined.
const MAX_RM_WORDS: usize = 8;
/// How many `git restore` words on one command line are examined.
const MAX_GIT_WORDS: usize = 8;

/// Worded exactly as the Python hook words it, because the contract test compares the two strings.
const NOT_DISPOSABLE: &str = "a recursive force-delete of something that is not a build artifact";
/// ⚠️ "I STOPPED LOOKING" IS NOT "THERE WAS NOTHING ELSE", and it must not be spelled the same
/// way. A bounded read that reported silence would let `rm -rf` + 32 copies of `node_modules` buy
/// quiet for the directory listed after them, which is a bypass anybody could type by accident.
const UNREAD_TARGETS: &str = "a recursive force-delete with more targets than this guard can read";

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid guard pattern {pattern:?}: {error}"),
    }
}

/// One clause of the table. Two of them need a NEGATIVE LOOKAHEAD that `regex_lite` does not have
/// (`--force` but not `--force-with-lease`; `git restore` but not `git restore --staged`), so those
/// are spelled out as predicates rather than quietly dropped — a clause of the contract with no
/// line enforcing it is a silent exemption, and this repo has paid for that species before.
enum Clause {
    Pattern(Regex),
    Predicate(fn(&str) -> bool),
}

impl Clause {
    fn fires(&self, command: &str) -> bool {
        match self {
            Clause::Pattern(pattern) => pattern.is_match(command),
            Clause::Predicate(predicate) => predicate(command),
        }
    }
}

fn pattern(source: &str, reason: &'static str) -> (Clause, &'static str) {
    (Clause::Pattern(compile(source)), reason)
}

fn predicate(test: fn(&str) -> bool, reason: &'static str) -> (Clause, &'static str) {
    (Clause::Predicate(test), reason)
}

/// The irreversible shapes, in the Python hook's order because the first match wins and two
/// clauses can overlap. Each carries its reason at the clause rather than as a bare pattern.
static DANGER: LazyLock<Vec<(Clause, &'static str)>> = LazyLock::new(|| {
    vec![
        pattern(r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:", "a fork bomb"),
        pattern(
            r"\bcurl\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b",
            "piping a download straight into a shell",
        ),
        pattern(
            r"\bwget\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b",
            "piping a download straight into a shell",
        ),
        pattern(
            r"\b(dd|mkfs\.\w+)\b.*\bof=/dev/(disk|sd|nvme)",
            "writing directly to a disk device",
        ),
        pattern(r">\s*/dev/(disk|sd|nvme)\w*", "overwriting a disk device"),
        pattern(
            r"\bchmod\s+-R\s+777\s+/",
            "making a broad path world-writable",
        ),
        pattern(r"\bshred\b", "an unrecoverable overwrite of file contents"),
        pattern(
            r"\bfind\b.*(-delete\b|-exec\s+rm\b)",
            "a find that deletes what it matches",
        ),
        // ── GIT: destroys work that was never committed, so no remote can give it back ────────
        // `git checkout -- <path>` is named in this repo's own standing brief: it silently deleted
        // a file's worth of freshly written tests, and in a tree several lanes share it can discard
        // another lane's staged work with no prompt and no undo.
        pattern(
            r"\bgit\s+checkout\s+(--\s+\S|\.$|\.\s)",
            "a git checkout that DISCARDS uncommitted work",
        ),
        predicate(
            unstaged_restore,
            "a git restore that DISCARDS uncommitted changes",
        ),
        pattern(
            r"\bgit\s+reset\s+--hard\b",
            "a hard reset that DISCARDS uncommitted work",
        ),
        pattern(
            r"\bgit\s+clean\s+-\S*[fd]",
            "a git clean that DELETES untracked files",
        ),
        pattern(
            r"\bgit\s+branch\s+-D\b",
            "force-deleting a branch that may be unmerged",
        ),
        pattern(
            r"\bgit\s+stash\s+(drop|clear)\b",
            "dropping stashed work that has no other copy",
        ),
        predicate(force_push, "a force-push that can overwrite pushed history"),
        // ── DATA: irreversible against a real database ────────────────────────────────────────
        pattern(
            r"(?i)\bDROP\s+(TABLE|DATABASE|SCHEMA)\b",
            "a DROP against a database",
        ),
        pattern(
            r"(?i)\bTRUNCATE\s+(TABLE\s+)?\w",
            "a TRUNCATE that empties a table",
        ),
        pattern(
            r"(?i)\bDELETE\s+FROM\s+\w+\s*(;|$)",
            "a DELETE FROM with no WHERE clause",
        ),
        // ── INFRASTRUCTURE: takes real things down ────────────────────────────────────────────
        pattern(r"\bterraform\s+destroy\b", "tearing down infrastructure"),
        pattern(
            r"\bkubectl\s+delete\b",
            "deleting a live Kubernetes resource",
        ),
        pattern(
            r"\bdocker\s+(system\s+prune|volume\s+(rm|prune))\b",
            "removing docker volumes or images",
        ),
        pattern(
            r"\baws\s+s3\s+(rm\b.*--recursive|rb\b)",
            "a recursive delete of an S3 bucket or prefix",
        ),
        // ── PUBLISHING: cannot be taken back once it leaves ───────────────────────────────────
        pattern(
            r"\b(npm|cargo)\s+publish\b",
            "publishing a package version, which cannot be unpublished",
        ),
        pattern(
            r"\bgh\s+(repo|release)\s+delete\b",
            "deleting a GitHub repository or release",
        ),
        // ── THIS REPO'S OWN HARD RULES, so the guard enforces what the brief already forbids ──
        // `railway variables` printed a GitHub PAT, a live Stripe key and an RSA private key into
        // an agent's context on 2026-08-17 and forced a four-credential rotation. No flag is safe.
        pattern(
            r"\brailway\s+variables\b",
            "a command that PRINTS SECRET VALUES (forbidden — ask instead)",
        ),
        // A bare `railway up` from an unlinked directory creates a NEW project and prod never moves.
        pattern(
            r"\brailway\s+up\b",
            "a bare railway up (use scripts/deploy.sh, which asserts the link)",
        ),
        pattern(
            r"\bhistory\s+-c\b",
            "clearing shell history, which destroys the record of what ran",
        ),
    ]
});

/// The `rm` WORD, wherever it appears — `\b` so `/bin/rm` and `sudo rm` both count and `alarm`
/// does not. The flags are read after it rather than matched inside one pattern, because a single
/// regex can only recognise the spellings its author thought of: `-rf` and `-fr` were in, and
/// `-r -f`, `--recursive --force` and `-Rf` were out. That is the enumerating defect again, one
/// level down.
static RM_WORD: LazyLock<Regex> = LazyLock::new(|| compile(r"\brm\s+"));

/// THE GOOD REGION. Directories a developer recreates with one well-known command, so deleting
/// them recursively is routine rather than a decision. Matched against the LAST path segment, so
/// `./web/node_modules` and `/tmp/x/target` are both covered.
static DISPOSABLE: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?:^|/)(?:node_modules|target|dist|build|out|coverage|\.next|\.nuxt|\.turbo|\.cache",
        r"|\.parcel-cache|__pycache__|\.pytest_cache|\.mypy_cache|\.ruff_cache|\.tox|\.venv|venv",
        r"|\.gradle|\.terraform|\.svelte-kit|\.angular|\.sass-cache|htmlcov|\.nyc_output)/?$",
    ))
});

/// Scratch roots. Anything beneath them is by definition temporary.
static SCRATCH: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(?:/tmp/|/var/tmp/|/private/tmp/|\$TMPDIR/|\./tmp/)"));

static GIT_PUSH: LazyLock<Regex> = LazyLock::new(|| compile(r"\bgit\s+push\b"));
static SHORT_FORCE: LazyLock<Regex> = LazyLock::new(|| compile(r"\s-f\b"));
static GIT_RESTORE: LazyLock<Regex> = LazyLock::new(|| compile(r"\bgit\s+restore\b"));
static RESTORE_STAGED: LazyLock<Regex> = LazyLock::new(|| compile(r"^\s+--staged\b"));

const LONG_FORCE: &str = "--force";

/// `git push` carrying a force flag — except `--force-with-lease`, which is the safe one and stays
/// silent so the rule is worth obeying at all. Only the rest of THAT line counts, because the
/// Python clause joins the two halves with `.*`, which cannot cross a newline.
fn force_push(command: &str) -> bool {
    let Some(found) = GIT_PUSH.find(command) else {
        return false;
    };
    let tail = command[found.end()..]
        .split('\n')
        .next()
        .unwrap_or_default();
    SHORT_FORCE.is_match(tail)
        || tail.match_indices(LONG_FORCE).any(|(at, _)| {
            let after = &tail[at + LONG_FORCE.len()..];
            // `--forced` is not `--force` (the Python clause ends in `\b`), and
            // `--force-with-lease` is the one force-push that cannot overwrite someone else's work.
            !after.starts_with("-with-lease")
                && !after.starts_with(|glyph: char| glyph.is_alphanumeric() || glyph == '_')
        })
}

/// `git restore` that is not `git restore --staged`. Restoring the INDEX only unstages; restoring
/// the working tree throws away edits that were never committed anywhere.
fn unstaged_restore(command: &str) -> bool {
    GIT_RESTORE
        .find_iter(command)
        .take(MAX_GIT_WORDS)
        .any(|found| !RESTORE_STAGED.is_match(&command[found.end()..]))
}

/// `(recursive, force, the text after the leading flags)` for one `rm` word.
///
/// A LONG flag is read as one word and a SHORT one letter by letter, and the difference matters:
/// `--force` contains an `r`, so reading it as a cluster would call it recursive and every
/// `rm --force one-file` would fire.
fn rm_flags(rest: &str) -> (bool, bool, &str) {
    let (mut recursive, mut force) = (false, false);
    let mut tail = rest.trim_start();
    for _ in 0..MAX_RM_FLAGS {
        if !tail.starts_with('-') {
            break;
        }
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let (flag, remainder) = tail.split_at(end);
        match flag.strip_prefix("--") {
            Some(long) => {
                recursive |= long == "recursive";
                force |= long == "force";
            }
            None => {
                let letters = &flag[1..];
                // `-R` is the recursive flag on BSD and GNU alike; a lowercase-only reading of the
                // cluster missed `rm -Rf` entirely.
                recursive |= letters.contains('r') || letters.contains('R');
                force |= letters.contains('f');
            }
        }
        tail = remainder.trim_start();
    }
    debug_assert!(
        !tail.starts_with(char::is_whitespace),
        "targets are trimmed"
    );
    (recursive, force, tail)
}

/// The paths a recursive `rm` would delete, flags and shell operators stripped.
///
/// The second field is FALSE when the line carried more targets than [`MAX_RM_TARGETS`] — "I
/// stopped looking", which the caller must never spell the same way as "every target was fine".
fn rm_targets(raw: &str) -> (Vec<&str>, bool) {
    let mut out: Vec<&str> = Vec::new();
    for token in raw.split_whitespace() {
        if token.starts_with('-')
            || matches!(token, "&&" | "||" | ";" | "|" | ">" | ">>" | "&" | "2>&1")
        {
            continue;
        }
        if out.len() == MAX_RM_TARGETS {
            return (out, false);
        }
        out.push(token.trim_matches(|glyph| glyph == '"' || glyph == '\''));
    }
    debug_assert!(
        out.len() <= MAX_RM_TARGETS,
        "the read stays inside its bound"
    );
    (out, true)
}

/// Whether one target is something a developer regenerates with a single well-known command.
fn regenerable(target: &str) -> bool {
    SCRATCH.is_match(target) || DISPOSABLE.is_match(target)
}

/// The reason a recursive force-`rm` deserves a second look, or `None` when every target it would
/// delete is regenerable. ONE target outside the good region is enough: the rule is *every*
/// target, so padding a line with `node_modules` can never buy silence for the path beside it.
fn rm_danger(command: &str) -> Option<&'static str> {
    for found in RM_WORD.find_iter(command).take(MAX_RM_WORDS) {
        let (recursive, force, rest) = rm_flags(&command[found.end()..]);
        if !recursive || !force {
            continue;
        }
        let (targets, read_to_end) = rm_targets(rest);
        if targets.is_empty() {
            continue; // nothing to delete: say nothing rather than invent a reason
        }
        if !targets.iter().all(|target| regenerable(target)) {
            return Some(NOT_DISPOSABLE);
        }
        if !read_to_end {
            return Some(UNREAD_TARGETS);
        }
    }
    None
}

/// The reason `command` looks dangerous, or `None` when it doesn't. Pure, and advisory at every
/// call site: it names the risk and the caller lets the human decide.
pub fn dangerous_command(command: &str) -> Option<&'static str> {
    DANGER
        .iter()
        .find(|(clause, _)| clause.fires(command))
        .map(|(_, reason)| *reason)
        .or_else(|| rm_danger(command))
}
#[cfg(test)]
mod tests {
    use super::dangerous_command;

    /// The one reason a recursive force-delete earns, worded exactly as the Python hook words it
    /// (`scripts/hooks/estelle_hook.py::_rm_danger`) so the two cannot drift apart quietly.
    const NOT_DISPOSABLE: &str =
        "a recursive force-delete of something that is not a build artifact";

    /// 🔴 THE DEFECT THIS FILE EXISTS TO CLOSE. The rule used to ENUMERATE the paths whose deletion
    /// was scary — `/`, `~`, `/etc`, `/usr`, `/Users` — so `rm -rf ~/Desktop` went through in
    /// silence, and so did every other path nobody had listed. The founder found the hole on his
    /// first guess. A guard written as a list of bad things guards the bad things somebody already
    /// imagined; the fix is to stop enumerating, not to append one more row.
    #[test]
    fn a_recursive_force_delete_outside_the_good_region_is_flagged() {
        for command in [
            "rm -rf ~/Desktop",
            "rm -rf ~/Documents",
            "rm -rf ~/.ssh",
            "rm -rf ./src",
            "rm -rf ../sibling-repo",
            "rm -rf /",
            "rm -rf ~",
            "sudo rm -rf /*",
            "rm -rf 'my dir'",
        ] {
            assert_eq!(
                dangerous_command(command),
                Some(NOT_DISPOSABLE),
                "{command} must be flagged"
            );
        }
    }

    /// THE GOOD REGION, and the whole reason it exists. An advisory guard that cries wolf is muted
    /// within a day and then misses the one that mattered, so the deletes a developer does every
    /// day — a regenerable build directory, anything under a scratch root — stay silent. That set
    /// is finite and knowable; the set of dangerous paths is not.
    #[test]
    fn regenerable_build_artifacts_and_scratch_paths_stay_silent() {
        for command in [
            "rm -rf node_modules",
            "rm -rf ./node_modules",
            "rm -rf ./target",
            "rm -rf dist/",
            "rm -rf web/.next",
            "rm -rf __pycache__",
            "rm -rf .venv",
            "rm -rf /Users/khai/proj/dist",
            "rm -rf ~/Downloads/build",
            "rm -rf /tmp/foo",
            "rm -rf /tmp/scratch",
            "rm -rf /var/tmp/x",
            "rm -rf /private/tmp/claude/x",
            "rm -rf $TMPDIR/scratch",
            "rm -rf ./tmp/x",
        ] {
            assert_eq!(
                dangerous_command(command),
                None,
                "{command} is ordinary cleanup and must stay silent"
            );
        }
    }

    /// EVERY name in the good region, written out rather than derived, because a derived list
    /// can never catch a regression in the thing it was derived from. This test was written after
    /// the first draft of `DISPOSABLE` was assembled from a multi-line RAW string: a backslash
    /// before a newline is a LINE CONTINUATION in a normal Rust string and a LITERAL BACKSLASH in
    /// a raw one, so two alternatives at the seams (`.cache`, `venv`) silently required a newline
    /// that no path can contain. The other twenty-two names matched, so the suite was green and
    /// two clauses of the contract were inert.
    #[test]
    fn every_name_in_the_good_region_actually_matches() {
        for name in [
            "node_modules",
            "target",
            "dist",
            "build",
            "out",
            "coverage",
            ".next",
            ".nuxt",
            ".turbo",
            ".cache",
            ".parcel-cache",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            ".tox",
            ".venv",
            "venv",
            ".gradle",
            ".terraform",
            ".svelte-kit",
            ".angular",
            ".sass-cache",
            "htmlcov",
            ".nyc_output",
        ] {
            for spelling in [
                format!("rm -rf {name}"),
                format!("rm -rf ./web/{name}"),
                format!("rm -rf /Users/khai/proj/{name}/"),
            ] {
                assert_eq!(
                    dangerous_command(&spelling),
                    None,
                    "{spelling} is a regenerable build artifact"
                );
            }
            // The name is a whole SEGMENT, never a prefix: `build0` and `targets` are somebody's
            // real directories and deleting them recursively is a decision, not routine.
            assert!(
                dangerous_command(&format!("rm -rf {name}0")).is_some(),
                "{name}0 is not {name}"
            );
        }
    }

    /// ONE non-disposable target is enough. The rule is *every* target is regenerable, not *some*
    /// — padding a command with `node_modules` must never buy silence for the directory beside it.
    #[test]
    fn one_target_outside_the_good_region_is_enough() {
        for command in [
            "rm -rf ~/Desktop node_modules",
            "rm -rf node_modules ~/Desktop",
            "rm -rf dist build ~/Desktop target",
        ] {
            assert_eq!(
                dangerous_command(command),
                Some(NOT_DISPOSABLE),
                "{command} deletes something that is not a build artifact"
            );
        }
    }

    /// A recognizer that only knows `-rf` and `-fr` is the enumerating defect again, one level
    /// down: `rm -r -f`, `rm --recursive --force` and `rm -Rf` are the same command typed
    /// differently, and every one of them is a spelling somebody uses.
    #[test]
    fn every_spelling_of_recursive_force_is_recognised() {
        for command in [
            "rm -rf ~/Desktop",
            "rm -fr ~/Desktop",
            "rm -Rf ~/Desktop",
            "rm -r -f ~/Desktop",
            "rm -f -r ~/Desktop",
            "rm --recursive --force ~/Desktop",
            "rm -v -rf ~/Desktop",
            "rm -rf -- ~/Desktop",
            "/bin/rm -rf ~/Desktop",
            "docker rm x && rm -rf ~/Desktop",
        ] {
            assert_eq!(
                dangerous_command(command),
                Some(NOT_DISPOSABLE),
                "{command} is a recursive force-delete"
            );
        }
        // Half the pair is not the pair. A non-recursive delete, or a recursive one that will stop
        // and ask, is ordinary work — and `docker rm -f` is not `rm` at all.
        for command in [
            "rm -r ~/Desktop",
            "rm -f ~/Desktop",
            "rm ~/Desktop",
            "rm build/tmp.o",
            "docker rm -f mycontainer",
            "grep -rf pattern src/",
            "rm -rf",
        ] {
            assert_eq!(
                dangerous_command(command),
                None,
                "{command} must stay silent"
            );
        }
    }

    /// ⚠️ A CAPPED READ FAILS THE GATE OPEN. The guard reads a bounded number of targets, and the
    /// bound is a real one — but "I stopped looking" must never be spelled the same way as "every
    /// target was a build artifact", or padding the line with 32 copies of `node_modules` buys
    /// silence for the directory after them.
    #[test]
    fn more_targets_than_the_guard_can_read_is_not_a_pass() {
        let padded = format!("rm -rf {}~/Desktop", "node_modules ".repeat(40));
        assert!(
            dangerous_command(&padded).is_some(),
            "a target list longer than the cap cannot be certified clean"
        );
        // The bound itself is not a warning: a command the guard read to the end, whose every
        // target was disposable, is exactly as silent as one with a single target.
        let at_cap = ["node_modules"; 32].join(" ");
        assert_eq!(
            dangerous_command(&format!("rm -rf {at_cap}")),
            None,
            "a fully-read, fully-disposable target list must stay silent"
        );
    }
}
