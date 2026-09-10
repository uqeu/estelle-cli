//! Giving a SUBAGENT the same grounding a top-level turn gets — without paying for it N times.
//!
//! 🔴 **WHAT A SUBAGENT GOT BEFORE THIS FILE: NOTHING, AND `rc == 0` SAID SO WITH ONE BYTE.**
//! `UserPromptSubmit` is the only door that injects repository context, and it fires on **the
//! human's prompt**. A subagent is spawned by the parent's `Task` tool and never submits one, so
//! that door structurally cannot reach it. `SubagentStart` is a first-class Claude Code event
//! (host changelog 2.0.43) and the shipped runner answered it by falling through
//! [`crate::top_level`]'s prompt precheck — empty `prompt` → `ContextPrecheck::Silent` → an empty
//! `Vec` → **exit 0 with a 1-byte stdout**. Measured against the published `@fatelabs/estelle@0`
//! on 2026-09-10: `SubagentStart` → 1 byte, `UserPromptSubmit` → 11202 bytes, same binary, same
//! invocation. A clean exit over empty output is the silent-failure family this repo keeps paying
//! for; the assertion that catches it is `out_bytes > 0`, never `rc == 0`.
//!
//! 🔴 **AND THE OBVIOUS FIX IS THE ONE THAT BREAKS THE PRODUCT.** Making `SubagentStart` issue its
//! own `POST /search` would put every subagent on the account's FULL concurrency limit — 12
//! concurrent subagents on ULTRA take all 10 slots for ~6 s and starve the human's own `/gate`,
//! `/work` and MCP calls to a bare `429` with no queue and no retry. **Parity in capability must
//! not mean parity in cost.** So the context is INHERITED, not re-earned: the parent's
//! `UserPromptSubmit` already paid for a scoped recall, this module stores it under the parent's
//! session id, and a subagent reads it off disk. **N subagents cost zero additional requests and
//! zero concurrency slots**, whatever N is.
//!
//! 🔴 **THE CERTIFICATE IS THE PART THAT MAKES THIS HONEST.** An inherited context that silently
//! degraded to nothing would be strictly worse than no feature: the subagent would believe it was
//! grounded. [`inherited_context`] therefore renders exactly one of three DISTINGUISHABLE states
//! and never silence — GROUNDED (with the parent's id and the age in seconds), STALE (the same
//! block past [`FRESH_FOR_S`], labelled old rather than withheld), and NO GROUNDING (said out
//! loud, naming the Estelle tools the subagent can still call for itself over MCP).
//!
//! ⚠️ **THE LIMIT, SAID OUT LOUD RATHER THAN HIDDEN.** The Python twin
//! (`uqeu/estelle:scripts/hooks/subagent_parity.py`) caches on FIVE context paths because
//! `prompt_context` has five durable returns. This runner has exactly ONE durable context source —
//! the `/search` recall — so one write site is total here rather than nearly-total. If a second
//! durable source is ever added to the context hook, it must call [`store_grounding`] too, and the
//! partial-guard shape this repo has paid for before is back.
//!
//! ⚠️ **THE WIRE FORMAT IS THE HOST'S AND IT IS EVENT-SENSITIVE.** Claude Code ignores
//! `additionalContext` whose `hookEventName` does not match the event that fired, so the envelope
//! this module feeds must carry `SubagentStart` and nothing else. That is asserted, not assumed.

use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Where an inherited context lives: under `~/.estelle` beside `last-session.json`, with the same
/// private modes that file already takes. This is repository content, and a world-readable copy of
/// it on a shared machine is a disclosure we would have created ourselves.
const CACHE_DIR_NAME: &str = "subagent-context";

/// Directory 0700, file 0600 — matched to the sibling state this runner already writes.
#[cfg(unix)]
const DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// Past this age the context is served LABELLED STALE rather than withheld. Withholding trades a
/// known-old answer for no answer; an agent told the age can judge it, one told nothing cannot.
/// The number is a session-turn scale, not a guess at freshness — the parent refreshes this on
/// every prompt, so a subagent spawned in the normal way reads something seconds old.
const FRESH_FOR_S: f64 = 30.0 * 60.0;

/// Bound on the stored block (Power of Ten #3, bound the resource before you take it). A recall is
/// a few KB; this is a ceiling an adversarial or runaway response cannot cross, and it is applied
/// on the WRITE so a corrupt file can never be large enough to matter on the read.
const MAX_CONTEXT_CHARS: usize = 24_000;

/// Bound on the cache directory. Nothing else cleans these up, so the writer prunes.
const MAX_CACHED_SESSIONS: usize = 64;

/// The host event this module answers.
pub(crate) const SUBAGENT_START_EVENT: &str = "SubagentStart";

/// What a subagent is told when the parent left nothing. It names the doors that still work: MCP
/// servers ARE inherited by a subagent (measured on the founder's machine, `find_definition` from
/// inside one reached the server and answered), so "no cache" is a degraded state with a stated
/// remedy, never a dead end.
const NO_GROUNDING_NOTICE: &str = concat!(
    "ESTELLE — NO INHERITED CONTEXT FOR THIS SUBAGENT.\n",
    "The parent session cached no repository grounding, so nothing about this repo was injected here. ",
    "This is an absence of evidence, never a statement that the repo is clean or that a symbol is ",
    "missing. Estelle's tools are reachable from this subagent over MCP: call `estelle_resume` to see ",
    "what this team already decided, `verify` / `find_definition` / `locate` before asserting any ",
    "symbol exists, and `gate` before proposing a change. Until you do, mark repo claims unverified."
);

/// A session id becomes a FILENAME, so it is VALIDATED as one and never sanitised into one. Hook
/// payloads are hostile host data, and rewriting `../x` into something plausible is worse than
/// refusing it: the repair invents a name that addresses a file the caller never asked for.
///
/// The rule, spelled out because it is a security boundary rather than a formatting preference:
/// first byte alphanumeric, then up to 127 more of `[A-Za-z0-9_.-]`, and nothing else — no `/`, no
/// `\`, no `:`, no NUL, and no bare `..` (which cannot match, since `.` may not lead).
fn safe_session_id(session_id: &str) -> Option<&str> {
    let candidate = session_id.trim();
    let mut bytes = candidate.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    let rest = candidate.len() - 1;
    if rest > 127 {
        return None;
    }
    bytes
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        .then_some(candidate)
}

/// `~/.estelle/subagent-context`, or `None` when this process has no home directory.
///
/// ⚠️ The same `dirs::home_dir()` the sibling state uses (`session_gap::state_path`), so the two
/// halves of `~/.estelle` cannot end up in different homes — one owner for "where is home".
fn cache_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join(CACHE_DIR_NAME))
}

fn cache_path(session_id: &str) -> Option<PathBuf> {
    let name = safe_session_id(session_id)?;
    Some(cache_dir()?.join(format!("{name}.json")))
}

fn now_epoch_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or_default()
}

/// The first `max` CHARACTERS of `text` — never the first `max` bytes.
///
/// ⚠️ `&text[..MAX]` panics on a multi-byte boundary, and a recall is prose that routinely carries
/// one. A truncation that can abort the hook is a truncation that deletes the whole envelope.
fn take_chars(text: &str, max: usize) -> String {
    text.char_indices()
        .nth(max)
        .map_or(text, |(index, _)| &text[..index])
        .to_string()
}

/// Record the grounding the PARENT just paid for, so its subagents can inherit it.
///
/// Returns whether it stored — a CHECKED return (Power of Ten #7), because a silent write failure
/// here degrades every later subagent to NO GROUNDING and the caller is the only party that can
/// say so.
///
/// ⚠️ **THE PROMPT IS NOT STORED. Only the rendered context block is.** A prompt is the field most
/// likely to hold a pasted credential — it is why the context hook refuses one outright — and a
/// cache is a file at rest: the safe design is for the secret-bearing field never to arrive here.
///
/// ⚠️ **THE REDACTION PASS IS DEFENCE IN DEPTH AND IS EXPECTED TO BE A NO-OP.** What is stored is
/// the server's own recall, already scrubbed at storage. Scrubbing again on the way to a NEW file
/// at rest costs microseconds and means this cache does not inherit its safety from an argument
/// about a different module.
pub(crate) fn store_grounding(session_id: &str, context: &str, repo: &str) -> bool {
    let Some(name) = safe_session_id(session_id) else {
        return false;
    };
    let redacted = estelle_client::redact_secrets_engine(context.trim());
    let body = redacted.trim();
    if body.is_empty() {
        return false;
    }
    let Some(target) = cache_path(session_id) else {
        return false;
    };
    let Some(directory) = cache_dir() else {
        return false;
    };
    let payload = json!({
        "session_id": name,
        "repo": repo,
        "at": now_epoch_s(),
        "context": take_chars(body, MAX_CONTEXT_CHARS),
    });
    if fs::create_dir_all(&directory).is_err() {
        return false;
    }
    restrict(&directory, dir_mode());
    if fs::write(&target, payload.to_string()).is_err() {
        return false;
    }
    restrict(&target, file_mode());
    prune(&target);
    true
}

#[cfg(unix)]
fn dir_mode() -> u32 {
    DIR_MODE
}
#[cfg(unix)]
fn file_mode() -> u32 {
    FILE_MODE
}
#[cfg(not(unix))]
fn dir_mode() -> u32 {
    0
}
#[cfg(not(unix))]
fn file_mode() -> u32 {
    0
}

/// Best effort, never fatal: a cache whose mode could not be tightened is still a cache, and the
/// alternative — refusing to store — costs every subagent its grounding to fix a permission bit.
#[cfg(unix)]
fn restrict(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path, _mode: u32) {}

/// Hold the cache to [`MAX_CACHED_SESSIONS`] files, newest first. Best effort, never raises.
fn prune(keep: &std::path::Path) {
    let Some(directory) = cache_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(&directory) else {
        return;
    };
    let mut found: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, stale) in found.into_iter().skip(MAX_CACHED_SESSIONS) {
        if stale != keep {
            let _ = fs::remove_file(stale);
        }
    }
}

/// What the parent stored, plus how old it is.
#[derive(Debug, PartialEq)]
pub(crate) struct Stored {
    pub(crate) context: String,
    pub(crate) repo: String,
    /// Seconds since the parent wrote it, or `None` when the file carried no usable timestamp.
    /// `None` is NOT zero: "age unknown" and "written this instant" are opposite facts, and a
    /// reader that cannot tell them apart cannot judge the block it was handed.
    pub(crate) age_s: Option<f64>,
}

/// The parent's stored grounding, or `None` when there is none.
///
/// `None` means exactly one thing — nothing readable is cached — and the caller renders that as a
/// STATED absence rather than as an empty context. A malformed or truncated file is `None` for the
/// same reason: half a cached answer is not a smaller answer, it is an unknown one.
pub(crate) fn load_grounding(session_id: &str) -> Option<Stored> {
    let raw = fs::read_to_string(cache_path(session_id)?).ok()?;
    let stored = serde_json::from_str::<Value>(&raw).ok()?;
    let body = stored.get("context").and_then(Value::as_str)?.trim();
    if body.is_empty() {
        return None;
    }
    let age_s = stored
        .get("at")
        .and_then(Value::as_f64)
        .map(|at| (now_epoch_s() - at).max(0.0));
    Some(Stored {
        context: take_chars(body, MAX_CONTEXT_CHARS),
        repo: stored
            .get("repo")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        age_s,
    })
}

/// The certificate line: WHERE this came from, HOW OLD it is, and WHETHER it is current.
fn header(parent: &str, age_s: Option<f64>, repo: &str, stale: bool) -> String {
    let when = age_s.map_or_else(
        || "age unknown".to_string(),
        |age| format!("{}s old", age as i64),
    );
    let scope = if repo.is_empty() {
        String::new()
    } else {
        format!(", repo {repo}")
    };
    let state = if stale {
        "STALE INHERITED CONTEXT"
    } else {
        "INHERITED CONTEXT"
    };
    format!(
        "ESTELLE — {state} (from parent session {parent}, {when}{scope}).\n\
         This is the grounding the parent turn already paid for; it was NOT re-read for this \
         subagent, so it does not reflect edits made since. Estelle's tools are reachable here \
         over MCP — re-check any symbol you are about to assert with `verify` or `find_definition`."
    )
}

/// The text a `SubagentStart` carries into the subagent, or `None` when the payload is not a
/// subagent start this runner can answer.
///
/// 🔴 IT NEVER RETURNS SILENCE ON A SESSION IT RECOGNISES. A cache miss renders
/// [`NO_GROUNDING_NOTICE`], because the difference between *"here is the repo"* and *"I have
/// nothing about the repo"* is the entire safety property: a subagent that cannot tell those apart
/// will assert from recall, which is the failure this product exists to prevent.
///
/// ⚠️ NO NETWORK CALL HAPPENS ON THIS PATH, and that is the design rather than an optimisation.
pub(crate) fn inherited_context(session_id: &str) -> Option<String> {
    let parent = safe_session_id(session_id)?;
    let Some(stored) = load_grounding(parent) else {
        return Some(NO_GROUNDING_NOTICE.to_string());
    };
    let stale = stored.age_s.is_some_and(|age| age > FRESH_FOR_S);
    Some(format!(
        "{}\n\n{}",
        header(parent, stored.age_s, &stored.repo, stale),
        stored.context
    ))
}

/// A private `HOME` for the duration of one test, restored on drop.
///
/// WARNING: `HOME` is process-global and the test harness is multi-threaded, so every holder
/// carries `#[serial_test::serial(estelle_home)]`. The lock has to be on the TEST rather than in
/// here, because a lock taken inside cannot exclude a test in another module that also reads
/// `HOME`. It is an RAII guard rather than a closure so an async test can hold it across `await`
/// without blocking the runtime it is running on.
#[cfg(test)]
pub(crate) struct TempHome {
    _dir: tempfile::TempDir,
    previous: Option<std::ffi::OsString>,
}

#[cfg(test)]
impl TempHome {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp home");
        let previous = std::env::var_os("HOME");
        // SAFETY: the holder is serialised on the `estelle_home` key, so no other test thread is
        // reading or writing the environment for the lifetime of this guard.
        unsafe { std::env::set_var("HOME", dir.path()) };
        Self {
            _dir: dir,
            previous,
        }
    }
}

#[cfg(test)]
impl Drop for TempHome {
    fn drop(&mut self) {
        // SAFETY: same serialisation as `new`.
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `body` under a private `HOME`. See [`TempHome`] for the serialisation rule.
    fn with_home<T>(body: impl FnOnce() -> T) -> T {
        let _home = TempHome::new();
        body()
    }

    const PARENT: &str = "3caf5c52-cadf-4a07-8a82-f6cd7cb34330";
    const RECALL: &str =
        "FROM THIS REPO: run_checkpoint lives in scripts/hooks/estelle_session_hooks.py";

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_stored_block_comes_back_with_a_certificate_naming_the_parent() {
        with_home(|| {
            assert!(store_grounding(PARENT, RECALL, "uqeu/estelle"));
            let context = inherited_context(PARENT).expect("an envelope");
            assert!(context.contains(RECALL), "the recall itself must travel");
            assert!(
                context.contains(PARENT),
                "the certificate must name the parent"
            );
            assert!(
                context.contains("uqeu/estelle"),
                "and the repo it was scoped to"
            );
            assert!(
                context.contains("INHERITED CONTEXT") && !context.contains("STALE"),
                "a fresh block must not be labelled stale: {context}"
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_miss_is_said_out_loud_and_is_never_silence() {
        with_home(|| {
            let context = inherited_context(PARENT).expect("a miss still answers");
            assert!(
                context.contains("NO INHERITED CONTEXT"),
                "a cache miss must state itself: {context}"
            );
            assert!(
                context.contains("estelle_resume"),
                "and name the doors that still work: {context}"
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_traversal_session_id_is_refused_and_never_repaired() {
        for hostile in ["../escape", "a/b", "", "   ", ".hidden", "a\0b"] {
            assert_eq!(safe_session_id(hostile), None, "accepted {hostile:?}");
            with_home(|| {
                assert!(!store_grounding(hostile, RECALL, ""), "stored {hostile:?}");
                assert_eq!(inherited_context(hostile), None, "answered {hostile:?}");
            });
        }
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn an_empty_block_never_evicts_the_good_one() {
        with_home(|| {
            assert!(store_grounding(PARENT, RECALL, ""));
            assert!(!store_grounding(PARENT, "   ", ""));
            assert!(
                inherited_context(PARENT)
                    .expect("still there")
                    .contains(RECALL),
                "an empty write must not have overwritten the block the parent paid for"
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn the_stored_block_is_bounded_on_the_write() {
        with_home(|| {
            assert!(store_grounding(
                PARENT,
                &"x".repeat(MAX_CONTEXT_CHARS * 3),
                ""
            ));
            let stored = load_grounding(PARENT).expect("stored");
            assert_eq!(stored.context.chars().count(), MAX_CONTEXT_CHARS);
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_multibyte_block_truncates_without_panicking() {
        with_home(|| {
            // Three-byte characters: a byte slice at MAX_CONTEXT_CHARS lands mid-character.
            assert!(store_grounding(
                PARENT,
                &"あ".repeat(MAX_CONTEXT_CHARS + 10),
                ""
            ));
            assert_eq!(
                load_grounding(PARENT)
                    .expect("stored")
                    .context
                    .chars()
                    .count(),
                MAX_CONTEXT_CHARS
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_stale_block_is_labelled_rather_than_withheld() {
        with_home(|| {
            assert!(store_grounding(PARENT, RECALL, ""));
            // Rewrite the timestamp to one older than the freshness window.
            let path = cache_path(PARENT).expect("path");
            let mut stored: Value =
                serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
            stored["at"] = json!(now_epoch_s() - FRESH_FOR_S - 60.0);
            fs::write(&path, stored.to_string()).expect("write");
            let context = inherited_context(PARENT).expect("an envelope");
            assert!(context.contains("STALE INHERITED CONTEXT"), "{context}");
            assert!(
                context.contains(RECALL),
                "stale means labelled, never withheld"
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_corrupt_file_reads_as_a_miss_rather_than_as_half_an_answer() {
        with_home(|| {
            assert!(store_grounding(PARENT, RECALL, ""));
            let path = cache_path(PARENT).expect("path");
            fs::write(&path, "{\"context\": \"trunc").expect("write");
            assert_eq!(load_grounding(PARENT), None);
            assert!(
                inherited_context(PARENT)
                    .expect("still answers")
                    .contains("NO INHERITED CONTEXT"),
                "a corrupt file must degrade to the STATED absence, not to silence"
            );
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn the_cache_is_bounded_and_the_newest_write_survives_the_prune() {
        with_home(|| {
            for index in 0..(MAX_CACHED_SESSIONS + 8) {
                assert!(store_grounding(&format!("s{index:04}"), RECALL, ""));
            }
            let directory = cache_dir().expect("cache dir");
            let count = fs::read_dir(&directory).expect("read dir").count();
            assert!(
                count <= MAX_CACHED_SESSIONS,
                "the cache grew to {count} files past the {MAX_CACHED_SESSIONS} bound"
            );
            let newest = format!("s{:04}", MAX_CACHED_SESSIONS + 7);
            assert!(
                load_grounding(&newest).is_some(),
                "the prune deleted the file it had just written"
            );
        });
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial(estelle_home)]
    fn the_cache_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        with_home(|| {
            assert!(store_grounding(PARENT, RECALL, ""));
            let file = fs::metadata(cache_path(PARENT).expect("path")).expect("stat");
            assert_eq!(file.permissions().mode() & 0o777, FILE_MODE);
            let dir = fs::metadata(cache_dir().expect("dir")).expect("stat");
            assert_eq!(dir.permissions().mode() & 0o777, DIR_MODE);
        });
    }

    #[test]
    #[serial_test::serial(estelle_home)]
    fn a_credential_in_the_block_does_not_reach_the_file_at_rest() {
        with_home(|| {
            let secret = format!("estelle_live_{}", "a".repeat(40));
            assert!(store_grounding(PARENT, &format!("key: {secret}"), ""));
            let raw = fs::read_to_string(cache_path(PARENT).expect("path")).expect("read");
            assert!(
                !raw.contains(&secret),
                "the credential survived into the cache file"
            );
        });
    }
}
