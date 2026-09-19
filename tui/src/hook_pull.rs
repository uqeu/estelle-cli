//! FORCED PULL — the redirect that fires when the model has ALREADY PROVEN it needs context.
//!
//! Estelle had two ways to get a model grounded and both miss the moment of need:
//!
//!   * **PUSH** — inject chunks at `UserPromptSubmit`. Measured on this repo's own traffic:
//!     **97.3% of injected bytes were never touched, precision 2.2%**, and it was turned OFF by
//!     default on 2026-09-18. It guesses what will be needed before the turn has a shape.
//!   * **PULL** — tell the model the tools exist and hope it elects to call them. That is what
//!     ships today, and it depends entirely on the model volunteering.
//!
//! This is the third one. A `Read`, a `Grep`, a `Glob` or a shell `grep`/`rg`/`find` is the model
//! saying *"I do not know this repo and I am about to go look"* — evidence, not a guess. So the
//! redirect costs nothing on a turn that never searches, and it lands on exactly the turn that did.
//!
//! 🔴 **NUDGE BY DEFAULT, NEVER BLOCK — AND THE REASON IS THIS REPO'S OWN HISTORY.** A guard people
//! fight is a guard people uninstall, and an uninstalled guard still carries the belief that it
//! ran. Blocking is opt-in behind [`STRICT_ENV`], default OFF, exactly like `ESTELLE_HOOK_BLOCK`.
//!
//! 🔴 **A REDIRECT LOOP IS THE FAILURE MODE, SO THE BLOCK IS SPENT ONCE AND THEN GONE.** If strict
//! mode refused the read, the model re-reads, and it is refused again, the session is wedged and
//! the hook has to be removed to make progress. Two independent things stop that: the block fires
//! at most once per session ([`PullState::blocked`]), **and it is not emitted at all unless that
//! fact was durably written first** — see [`Persisted`]. A block we could not remember is a block
//! that would fire forever, which is worse than never blocking at all.
//!
//! 🔴 **A NUDGE TOWARD A TOOL THAT WILL REFUSE IS WORSE THAN SILENCE.** `find_definition` answers
//! *"STALE — indexed at X, repo is now Y"* when the index is behind, and sending the model there to
//! collect that refusal costs a round trip and teaches it the graph is useless. So freshness is
//! checked BEFORE the redirect is written, and a stale index produces **no redirect in either
//! mode** — in strict mode it says so once instead of denying, which is also what keeps strict from
//! being silently stuck on a repo that was never swept.
//!
//! LIMITS, out loud:
//!
//!   * The Bash half is a SHELL-STRING matcher, with the same limits as [`crate::hook_guard`]: an
//!     alias, a `$(…)`, a wrapper script or a variable that expands to `grep` all defeat it. It
//!     reads the command it is shown.
//!   * Freshness is `ground_block::index_is_current_*`, which is a REPO-LEVEL proxy with a known
//!     hole (documented at its own definition). It is used here in the safe direction only: a
//!     false "stale" costs a redirect, a false "current" costs a redirect that may be answered
//!     from a slightly old index. Neither refuses anybody's code.
//!   * A nudge fires at most [`NUDGES_PER_KIND`] time(s) per kind per session. That is a deliberate
//!     answer to the 2.2%-precision finding above — the model needs to be told the graph path
//!     exists at the moment it is searching, not on all 300 of its reads. Raising it is a one-line
//!     change to one constant, and the constant is named so the tradeoff is arguable rather than
//!     buried.

use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::Value;

/// Opt-in for the BLOCKING half, default OFF. Same accepted spellings as
/// `ground_block::BLOCK_ENV`, because two opt-ins in one product that disagree about whether
/// `yes` counts is a support ticket.
pub const STRICT_ENV: &str = "ESTELLE_PULL_STRICT";

/// Test seam for the per-session state directory. Production resolves
/// `~/.estelle/hook-pull/`.
pub const STATE_DIR_ENV: &str = "ESTELLE_PULL_STATE";

/// How many times one KIND of search may be redirected in one session.
///
/// 🔴 **THIS IS THE CONSTANT THAT ANSWERS THE PUSH FAILURE.** Estelle's per-turn push was measured
/// at 2.2% precision and switched off; a redirect emitted on all of a session's reads would be the
/// same defect wearing a different event. One redirect per kind tells the model the graph path
/// exists at the moment it is searching, which a static system prompt cannot do, and then stops.
pub const NUDGES_PER_KIND: usize = 1;

/// A state file older than this is not evidence about THIS session, and is treated as absent. A
/// laptop that slept for a day must not resume with yesterday's "already redirected" marker.
pub const STATE_MAX_AGE_S: u64 = 12 * 3600;

/// The most a state file may ever be. It holds four small fields; anything larger is not ours and
/// is read no further (Power of Ten #3 — bound the resource before you take it).
pub const MAX_STATE_BYTES: u64 = 1024;

/// The prefix on every file this module writes.
///
/// 🔴 **IT EXISTS SO [`prune`] CANNOT DELETE A FILE THIS MODULE DID NOT WRITE.** `state_dir` honours
/// `ESTELLE_PULL_STATE`, and a prune that matched bare `*.json` would delete whatever JSON happened
/// to be in whatever directory that pointed at. Bounding the blast radius to our own filenames costs
/// one constant; enumerating the directories it would have been safe in costs a future incident.
pub const STATE_FILE_PREFIX: &str = "pull-";

/// The suffix on the sentinel that makes "this session has spent its one block" true ACROSS
/// PROCESSES. See [`claim_block`].
pub const BLOCK_FILE_SUFFIX: &str = ".block";

/// How long a measured freshness answer is reused before it is measured again.
///
/// 🔴 **A CACHE WITH NO TTL IS A CLAIM ABOUT A PAST THAT KEEPS BEING PRESENTED AS THE PRESENT.**
/// The first version cached the answer for the whole session, so a repo that went stale mid-session
/// — an edit outside the hook, a rebase, a `git pull` — kept collecting redirects promising a
/// current graph. Found by a rival reviewer on the PR. Five minutes is the trade: the 2 s-capped
/// walk costs at most once per window rather than once per read, and the window is short enough
/// that a repo that went stale stops being advertised as current within one.
pub const FRESHNESS_TTL_S: u64 = 300;

/// The freshness window must sit strictly inside the marker's lifetime, or the marker would be
/// discarded whole before its index answer ever expired and the TTL would be unreachable code.
/// Checked by the COMPILER, not by a test: a bound that only one reader knows about is a bound.
const _: () = assert!(
    FRESHNESS_TTL_S < STATE_MAX_AGE_S,
    "the freshness TTL must expire before the whole marker does, or it can never fire"
);

/// How many session state files are kept. Bounded so an always-on hook cannot grow a directory
/// without limit on a machine nobody cleans.
pub const STATE_KEEP: usize = 64;

/// How much of a shell command is read. A command longer than this is truncated BEFORE matching;
/// the cost of being wrong is a missed redirect, never a hang.
pub const MAX_COMMAND_BYTES: usize = 4096;

/// How many `;`/`&&`/`||`/newline-separated statements of one command line are examined.
pub const MAX_SEGMENTS: usize = 16;

/// How many leading words (`sudo`, `time`, `env`…) are stripped before the command word is read.
pub const MAX_LEADING_WORDS: usize = 4;

/// How many arguments are read looking for the needle, and for `ls`'s recursive flag.
pub const MAX_ARGS: usize = 24;

/// The needle is echoed back to the model so the redirect names the actual query rather than a
/// generic sentence. Bounded, and it goes ONLY into `additionalContext` (model-facing, and the
/// model typed it a moment ago) — never into `systemMessage`, which is the human's terminal line.
pub const MAX_NEEDLE_BYTES: usize = 80;

/// What the model is trying to find out. Three kinds because three different Estelle tools answer
/// them, and a redirect that names the wrong tool is worse than none.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SearchKind {
    /// Opening a source file to read it — `Read`, or a shell `cat`/`sed -n` (not matched today).
    ReadFile,
    /// Searching file CONTENTS — `Grep`, or a shell `grep`/`rg`/`ag`/`ack`/`git grep`.
    Content,
    /// Searching file NAMES or paths — `Glob`, or a shell `find`/`fd`/`ls -R`.
    Names,
}

impl SearchKind {
    /// The wire spelling kept in the session state file. Written out rather than derived from
    /// `Debug`, because a rename of the variant would then silently invalidate every marker on
    /// every customer's disk and the redirect would start firing twice.
    pub fn tag(self) -> &'static str {
        match self {
            SearchKind::ReadFile => "read",
            SearchKind::Content => "content",
            SearchKind::Names => "names",
        }
    }
}

/// One recognised search, with the thing being looked for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchIntent {
    pub kind: SearchKind,
    /// The caller's own pattern or path, for the redirect to quote. May be empty.
    pub needle: String,
    /// Where the search was aimed, when the caller said — `Grep`/`Glob`'s `path`, `find`'s
    /// directory, `grep`'s trailing operand. `None` means "the caller named no scope", which is
    /// NOT the same as "the repository root" and must not be read as clearance.
    ///
    /// 🔴 **THIS FIELD EXISTS BECAUSE A REVIEWER FOUND THE HOLE.** Only the `Read` shape was
    /// bounded to the indexed tree, so `sudo grep -rn x /etc` and `Grep{path:"/etc"}` collected a
    /// redirect telling the model Estelle's graph could answer them. It cannot: nothing outside
    /// the repository is in it.
    pub scope: Option<String>,
}

/// 🔑 **THE ONE OWNER OF "IS THIS TOOL CALL A SEARCH".** Both doors call this and nothing else
/// decides it: the `PreToolUse` `Read|Grep|Glob` door (`top_level::pull_hook`) and the
/// `PreToolUse` `Bash` door (`top_level::guard_hook`). Two copies of this predicate would disagree
/// within a week, and the one that disagreed would be the one nobody was reading.
///
/// Returns `None` for everything it is not sure about. A missed redirect costs nothing; a redirect
/// on `git status` costs the customer's trust in the whole hook.
pub fn search_intent(tool_name: &str, tool_input: &Value) -> Option<SearchIntent> {
    let text = |key: &str| {
        tool_input
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    match tool_name {
        "Read" => {
            let path = text("file_path");
            (!path.is_empty()).then_some(SearchIntent {
                kind: SearchKind::ReadFile,
                scope: Some(path.clone()),
                needle: path,
            })
        }
        "Grep" => {
            let needle = text("pattern");
            let path = text("path");
            (!needle.is_empty()).then_some(SearchIntent {
                kind: SearchKind::Content,
                needle,
                scope: (!path.is_empty()).then_some(path),
            })
        }
        "Glob" => {
            let needle = text("pattern");
            let path = text("path");
            (!needle.is_empty()).then_some(SearchIntent {
                kind: SearchKind::Names,
                needle,
                scope: (!path.is_empty()).then_some(path),
            })
        }
        "Bash" => shell_search(&text("command")),
        _ => None,
    }
}

/// The Bash half. Only the FIRST stage of each statement counts: `grep -rn x src/ | head` is a
/// search of the repo, and `git log | grep x` is a filter over another command's output, which the
/// graph cannot answer and must not be redirected.
pub fn shell_search(command: &str) -> Option<SearchIntent> {
    let command = truncate_bytes(command, MAX_COMMAND_BYTES);
    for segment in statements(command).take(MAX_SEGMENTS) {
        // Only the head of the pipeline: everything after a `|` consumes stdout, not the repo.
        let stage = segment.split('|').next().unwrap_or_default();
        if let Some(intent) = stage_search(stage) {
            return Some(intent);
        }
    }
    None
}

/// Split a command line into statements on `;`, `&&`, `||` and newlines.
///
/// ⚠️ **QUOTE-BLIND, AND THE FIRST VERSION OF THIS COMMENT LIED ABOUT WHICH WAY THAT FAILS.** It
/// claimed "the worst that produces is a fragment whose leading word is not a search word, i.e.
/// silence". That is exactly backwards: `echo "; grep foo ."` splits into a synthetic segment
/// beginning with `grep`, so a command that runs no search reads as one. Caught by a rival
/// reviewer on the PR, which is the point of asking one.
///
/// The consequence is bounded and is a FALSE NUDGE, never a false block: the redirect is capped at
/// [`NUDGES_PER_KIND`] per kind per session, and the blocking path additionally needs the opt-in,
/// a current index and an atomic claim. A real shell parser is the fix if this proves noisy; the
/// limit is stated here rather than hidden so the noise can be attributed when it appears.
fn statements(command: &str) -> impl Iterator<Item = &str> {
    command
        .split(['\n', ';'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
}

/// One pipeline stage: strip the leading wrappers, then read the command word.
fn stage_search(stage: &str) -> Option<SearchIntent> {
    let mut words = stage.split_whitespace().skip_while(|word| {
        // A leading `VAR=value` is an environment assignment, not the command.
        word.contains('=') && !word.starts_with('-')
    });
    let mut command = words.next()?;
    for _ in 0..MAX_LEADING_WORDS {
        if matches!(command, "sudo" | "time" | "command" | "nohup") {
            command = words.next()?;
        } else {
            break;
        }
    }
    let base = command.rsplit('/').next().unwrap_or(command);
    let args: Vec<&str> = words.take(MAX_ARGS).collect();
    match base {
        "grep" | "egrep" | "fgrep" | "rg" | "ripgrep" | "ag" | "ack" => Some(SearchIntent {
            kind: SearchKind::Content,
            needle: first_operand(&args),
            scope: grep_scope(&args),
        }),
        "find" | "fd" => Some(SearchIntent {
            kind: SearchKind::Names,
            needle: first_operand(&args),
            scope: operands(&args).first().map(|arg| (*arg).to_string()),
        }),
        // `ls -R` walks the tree; `ls -la` lists one directory and is not a search.
        "ls" if args.iter().any(|arg| recursive_flag(arg)) => Some(SearchIntent {
            kind: SearchKind::Names,
            needle: first_operand(&args),
            scope: operands(&args).first().map(|arg| (*arg).to_string()),
        }),
        "git" if args.first() == Some(&"grep") => {
            let rest = args.get(1..).unwrap_or_default();
            Some(SearchIntent {
                kind: SearchKind::Content,
                needle: first_operand(rest),
                scope: grep_scope(rest),
            })
        }
        _ => None,
    }
}

/// Every argument that is not a flag, in order, bounded by the caller's own `MAX_ARGS` slice.
fn operands<'a>(args: &[&'a str]) -> Vec<&'a str> {
    args.iter()
        .filter(|arg| !arg.starts_with('-'))
        .copied()
        .collect()
}

/// `grep PATTERN [PATH...]` — the SECOND operand onward are paths, the first is the pattern. With
/// only one operand the caller named no scope, which is `None` and NOT "the repository": a
/// bare `grep -rn foo` searches the cwd, and the cwd is already the root this hook is handed.
fn grep_scope(args: &[&str]) -> Option<String> {
    let found = operands(args);
    found.get(1).map(|arg| (*arg).to_string())
}

/// `-R`, `-lR`, `--recursive`. Never `-la`, which is the whole point of reading the letters rather
/// than substring-matching the flag.
fn recursive_flag(arg: &str) -> bool {
    if arg == "--recursive" {
        return true;
    }
    match arg.strip_prefix('-') {
        Some(letters) if !letters.starts_with('-') => letters.contains('R'),
        _ => false,
    }
}

/// The first argument that is not a flag — the pattern for `grep`, the path for `find`. Options
/// that TAKE a value (`-e pat`, `--include=x`) are not modelled: the cost of getting this wrong is
/// a redirect that quotes the wrong word, never a wrong decision.
fn first_operand(args: &[&str]) -> String {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .map(|arg| truncate_bytes(arg.trim_matches(['"', '\'']), MAX_NEEDLE_BYTES).to_string())
        .unwrap_or_default()
}

/// Truncate on a char boundary, never mid-UTF-8.
fn truncate_bytes(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

/// Is a `Read` of this path something Estelle's graph could speak about at all?
///
/// A path outside the repository root, or inside a directory the ingest skips, is not in the graph
/// — redirecting there would send the model to a tool with nothing to say.
pub fn in_indexed_tree(path: &str, root: &Path) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    let candidate = candidate.canonicalize().unwrap_or(candidate);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Ok(relative) = candidate.strip_prefix(&root) else {
        return false;
    };
    !relative.components().any(|component| {
        estelle_tui::ground_block::FRESHNESS_SKIP_DIRECTORIES
            .contains(&component.as_os_str().to_string_lossy().as_ref())
    })
}

/// Is the BLOCKING half switched on for this install? Reads the process environment.
pub fn strict_enabled() -> bool {
    strict_enabled_from(std::env::var(STRICT_ENV).ok().as_deref())
}

/// The pure half, so the accepted spellings are pinned without touching the environment.
pub fn strict_enabled_from(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "on")
    )
}

/// What one session has already been told. Small, and every field is a fact we must not repeat.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PullState {
    /// Kinds already redirected this session, with how many times.
    pub told: Vec<(String, usize)>,
    /// Whether the one strict block has already been spent.
    pub blocked: bool,
    /// Whether the stale-index notice has already been given.
    pub told_stale: bool,
    /// The cached freshness answer. `None` = never measured, or measured longer ago than
    /// [`FRESHNESS_TTL_S`] and therefore discarded on load.
    pub fresh: Option<bool>,
}

impl PullState {
    fn count(&self, kind: SearchKind) -> usize {
        self.told
            .iter()
            .find(|(tag, _)| tag == kind.tag())
            .map_or(0, |(_, count)| *count)
    }

    fn record(&mut self, kind: SearchKind) {
        if let Some(entry) = self.told.iter_mut().find(|(tag, _)| tag == kind.tag()) {
            entry.1 = entry.1.saturating_add(1);
        } else {
            self.told.push((kind.tag().to_string(), 1));
        }
    }
}

/// The decision. Four outcomes, and the three that are not `Silent` each carry a different
/// obligation, which is why they are variants rather than a bool plus a string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullVerdict {
    /// Say nothing. Not a search, out of the indexed tree, or already said.
    Silent,
    /// It IS a search, and we cannot promise the graph is current. Say so; redirect nowhere.
    Stale,
    /// Advise. No permission decision, the tool call proceeds.
    Nudge,
    /// Refuse this ONE call and redirect. Strict only, once per session, index current only.
    Block,
}

/// 🔑 **THE ONE OWNER OF THE DECISION.** Pure and total: every input is a parameter, so each guard
/// below has a mutant that turns a specific test red, and nothing here reads a clock, an
/// environment or a disk.
///
/// The order is the fail-safe one and it matters:
///
/// 1. **Staleness first.** A stale index can never produce a redirect, in either mode — that is
///    the rule that stops the hook sending the model to a tool that will answer "STALE".
/// 2. **Block second, and only once.** `blocked` is the anti-loop, and `strict` is the opt-in.
/// 3. **Nudge third, and only while under [`NUDGES_PER_KIND`].**
pub fn decide(kind: SearchKind, state: &PullState, fresh: bool, strict: bool) -> PullVerdict {
    if !fresh {
        // Strict mode must never be silently inert: if it cannot redirect, it says why, once.
        return if strict && !state.told_stale {
            PullVerdict::Stale
        } else {
            PullVerdict::Silent
        };
    }
    if strict && !state.blocked {
        return PullVerdict::Block;
    }
    if state.count(kind) < NUDGES_PER_KIND {
        return PullVerdict::Nudge;
    }
    PullVerdict::Silent
}

/// Did the new state reach the disk? A `Block` is emitted ONLY on `Persisted::Yes`.
///
/// 🔴 **THIS ENUM IS THE ANTI-LOOP.** `blocked` lives in a file, and a file can fail to be written
/// — a read-only `$HOME`, a full disk, a sandbox with no home at all. If the block were emitted
/// anyway, every subsequent read would re-read a state that still says `blocked: false` and be
/// refused again, forever, with no way out but uninstalling the hook. So the write happens FIRST
/// and its result decides whether the refusal is allowed to exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Persisted {
    Yes,
    No,
}

/// Apply a verdict to the state and try to persist it. Returns whether it landed.
///
/// `already_dirty` is how the CACHED FRESHNESS ANSWER survives a `Silent` verdict. Measuring
/// freshness costs a bounded tree walk (`ground_block::FRESHNESS_DEADLINE_MS`, 2 s); paying it
/// once per session is the whole reason this hook can sit on the read path at all, and it is only
/// paid once if the answer is written even on the turns that say nothing.
pub fn commit(
    verdict: PullVerdict,
    kind: SearchKind,
    state: &mut PullState,
    dir: Option<&Path>,
    session: &str,
    already_dirty: bool,
) -> Persisted {
    let mut changed = already_dirty;
    match verdict {
        PullVerdict::Silent => {}
        PullVerdict::Stale => {
            state.told_stale = true;
            changed = true;
        }
        PullVerdict::Nudge => {
            state.record(kind);
            changed = true;
        }
        PullVerdict::Block => {
            state.blocked = true;
            state.record(kind);
            changed = true;
        }
    }
    // Nothing changed means nothing to write — and a `Block` always changes something, so this
    // shortcut can never hand a refusal an unearned `Persisted::Yes`.
    if !changed {
        return Persisted::Yes;
    }
    if save_state(dir, session, state) {
        Persisted::Yes
    } else {
        Persisted::No
    }
}

fn state_dir(dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = dir {
        return Some(dir.to_path_buf());
    }
    if let Some(path) = std::env::var_os(STATE_DIR_ENV) {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".estelle").join("hook-pull"))
}

/// A session id is host-supplied and reaches a path, so it is reduced to a bounded, safe stem.
/// Anything that is not `[A-Za-z0-9_-]` is dropped; an id that reduces to nothing is refused by
/// the caller, which is the same "no session = say nothing" rule `file_shift_hook` already uses.
fn session_stem(session: &str) -> String {
    session
        .chars()
        .filter(|glyph| glyph.is_ascii_alphanumeric() || *glyph == '-' || *glyph == '_')
        .take(64)
        .collect()
}

/// Read this session's state. Every failure mode returns the default, which means "nothing said
/// yet" — the safe direction, because it can only cost one extra nudge and can never cost a
/// second block (the block also needs `strict`, and its own write must succeed).
pub fn load_state(dir: Option<&Path>, session: &str, now: u64) -> PullState {
    let Some(path) = state_path(dir, session) else {
        return PullState::default();
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        return PullState::default();
    };
    if meta.len() > MAX_STATE_BYTES {
        return PullState::default();
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return PullState::default();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return PullState::default();
    };
    let at = value.get("at").and_then(Value::as_u64).unwrap_or(0);
    if now.saturating_sub(at) > STATE_MAX_AGE_S {
        return PullState::default();
    }
    // The freshness answer expires on its OWN clock, independently of the rest of the marker: how
    // often we have already spoken is a fact about this session and does not go stale, but whether
    // the index is current is a fact about the working tree a minute ago.
    let measured_at = value.get("fresh_at").and_then(Value::as_u64).unwrap_or(0);
    let fresh_expired = now.saturating_sub(measured_at) > FRESHNESS_TTL_S;
    PullState {
        told: value
            .get("told")
            .and_then(Value::as_object)
            .map(|told| {
                told.iter()
                    .map(|(tag, count)| {
                        (
                            tag.clone(),
                            usize::try_from(count.as_u64().unwrap_or(0)).unwrap_or(usize::MAX),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        blocked: value
            .get("blocked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        told_stale: value.get("stale").and_then(Value::as_bool).unwrap_or(false),
        fresh: if fresh_expired {
            None
        } else {
            value.get("fresh").and_then(Value::as_bool)
        },
    }
}

fn state_path(dir: Option<&Path>, session: &str) -> Option<PathBuf> {
    let stem = session_stem(session);
    if stem.is_empty() {
        return None;
    }
    Some(state_dir(dir)?.join(format!("{STATE_FILE_PREFIX}{stem}.json")))
}

/// Write this session's state. Returns `false` on ANY failure, and the caller treats that as
/// "the block may not be emitted" (see [`Persisted`]).
pub fn save_state(dir: Option<&Path>, session: &str, state: &PullState) -> bool {
    let Some(path) = state_path(dir, session) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
    }
    let told = state
        .told
        .iter()
        .map(|(tag, count)| (tag.clone(), Value::from(*count)))
        .collect::<serde_json::Map<String, Value>>();
    let now = unix_now();
    let body = serde_json::json!({
        "at": now,
        "told": told,
        "blocked": state.blocked,
        "stale": state.told_stale,
        "fresh": state.fresh,
        // Stamped on every save. It can never OVER-state the answer's age, and under-states it only
        // by the gap between measuring and writing — the safe direction is to re-measure sooner.
        "fresh_at": state.fresh.map(|_| now),
    });
    let Ok(bytes) = serde_json::to_vec(&body) else {
        return false;
    };
    if std::fs::write(&path, bytes).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    prune(parent, STATE_KEEP);
    true
}

/// Claim this session's single strict block. `true` means THIS process won it.
///
/// 🔴 **`blocked` IN A JSON FILE IS A READ-MODIFY-WRITE AND HOOKS RUN CONCURRENTLY.** Two parallel
/// `Read` calls can both load `blocked: false`, both decide `Block`, and both deny — one refusal
/// becoming N. Found by a rival reviewer on the PR. `create_new` is `O_EXCL`: exactly one process
/// can create the file, so exactly one can be the one that blocks, and every loser degrades to a
/// nudge. The JSON flag stays as the CHEAP pre-filter that keeps later calls off the filesystem
/// entirely; this is the authority.
///
/// Every failure mode returns `false` — no home, no session, unwritable directory, already claimed
/// — because the fail-safe direction for a refusal is not to refuse.
pub fn claim_block(dir: Option<&Path>, session: &str) -> bool {
    let stem = session_stem(session);
    if stem.is_empty() {
        return false;
    }
    let Some(base) = state_dir(dir) else {
        return false;
    };
    if std::fs::create_dir_all(&base).is_err() {
        return false;
    }
    let path = base.join(format!("{STATE_FILE_PREFIX}{stem}{BLOCK_FILE_SUFFIX}"));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .is_ok()
}

/// Keep the newest `keep` state files. Never fails loudly; a directory we cannot read is a
/// directory we do not prune.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .filter(|(_, path)| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(STATE_FILE_PREFIX)
                        && (name.ends_with(".json") || name.ends_with(BLOCK_FILE_SUFFIX))
                })
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    let stale = files.len().saturating_sub(keep);
    for (_, path) in files.into_iter().take(stale) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The human's terminal line. Deliberately GENERIC — it never carries the caller's own pattern,
/// because `systemMessage` is printed to the terminal and written to the on-disk transcript, and
/// a shell search is exactly the shape that sometimes carries a credential
/// (`grep FIRECRAWL_API_KEY .env`). The model-facing half quotes the needle; the human-facing half
/// does not need to, since the human just watched themselves ask for it.
pub fn system_line(kind: SearchKind) -> String {
    let what = match kind {
        SearchKind::ReadFile => "a raw file read",
        SearchKind::Content => "a content search",
        SearchKind::Names => "a filename search",
    };
    format!("🔎 Estelle: {what} — pointed it at the indexed graph instead of the filesystem.")
}

/// The model-facing redirect. Names the specific tool for THIS shape, and quotes the caller's own
/// query so the sentence is about the search that just happened rather than a slogan.
pub fn redirect(kind: SearchKind, needle: &str, repo: &str) -> String {
    let needle = truncate_bytes(needle.trim(), MAX_NEEDLE_BYTES);
    let quoted = if needle.is_empty() {
        String::new()
    } else {
        format!(" for `{needle}`")
    };
    let route = match kind {
        SearchKind::ReadFile => concat!(
            "`find_definition` / `locate` answer \"where is this\" from the graph without opening ",
            "the file, `find_usages` and `blast_radius` answer \"who calls this / what breaks if I ",
            "change it\", and `verify` answers \"does this symbol exist\" with no model call"
        ),
        SearchKind::Content => concat!(
            "`estelle_grep` runs the same search over the INDEXED repository in one call, and ",
            "`find_usages` / `find_references` / `blast_radius` answer the question a grep is ",
            "usually standing in for"
        ),
        SearchKind::Names => concat!(
            "`locate` resolves paths and file names from the graph, and `find_definition` goes ",
            "straight to the symbol you are hunting the file for"
        ),
    };
    format!(
        "Estelle is indexed for {repo} and its graph is current. Before reading this repo file by file{quoted}: {route}. These answer from the real indexed repository and refuse what they cannot verify."
    )
}

/// What a STALE index says. It redirects NOWHERE — naming the tool here is exactly the mistake
/// this branch exists to avoid — and it names the one command that would fix it.
pub fn stale_line(repo: &str) -> String {
    format!(
        "Estelle's index for {repo} is behind this working tree, so its graph tools would answer \
         STALE rather than help. Not redirecting this search. Run `estelle sweep` to bring the \
         index current; until then the filesystem is the only source that is up to date."
    )
}

#[cfg(test)]
#[path = "hook_pull_tests.rs"]
mod tests;
