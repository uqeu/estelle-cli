//! The Bash-output distiller, ported from cli/bin/distill.js — PREVENTION INSTEAD OF EVICTION.
//! PostToolUse can REPLACE a tool's result before it ever reaches the model; a verbose result
//! that never enters the window costs nothing to hold and nothing to compact later.
//!
//! THE RULE THIS FILE IS BUILT AROUND: destroying information the model needed is far worse
//! than verbosity. So:
//!
//!   * It only ever drops lines it can NAME as noise — no "keep the first N and last M"
//!     truncation anywhere; a line survives unless a specific pattern says what noise it is.
//!   * Failure vocabulary OVERRIDES every noise rule.
//!   * It refuses outright on tools whose output IS the answer (Read, Grep, Glob).
//!   * Every refusal returns `None`, which the caller renders as "pass the output through
//!     untouched". The failure mode of this file is verbosity, never silence.
//!   * The full text is spilled to disk and NAMED in the replacement, so nothing is lost.

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use regex_lite::Regex;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

/// Below this, verbosity is not a problem worth taking any risk to solve.
pub const MIN_CHARS: usize = 2_000;
/// A distillation that does not clearly pay for itself is pure downside.
pub const MIN_SAVING: f64 = 0.25;
/// The first this-many copies of an identical consecutive line are kept; the rest of the run is
/// dropped. Three is enough to make a repeat visible as a repeat.
pub const REPEAT_RUN: usize = 3;
/// Keep the newest this-many spill files; drop the rest.
pub const SPILL_KEEP: usize = 200;

/// Tools whose output IS the thing the model asked for. Never touched.
const NEVER_DISTIL: &[&str] = &[
    "Read",
    "Grep",
    "Glob",
    "NotebookRead",
    "WebFetch",
    "WebSearch",
];

fn compile(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid distil pattern {pattern:?}: {error}"),
    }
}

/// Failure vocabulary. A line matching this is SIGNAL and survives every noise rule below.
/// Deliberately wide: a false positive here costs a few tokens, a false negative costs the
/// model the reason something broke.
static SIGNAL: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?i)\b(?:error|errors|traceback|exception|failed|failure|fail|fatal|panic|assert|assertion|",
        r"warning|warn|denied|refused|not found|no such|undefined|invalid|timeout|timed out|",
        r"cannot|could ?n['o]t|does ?n['o]t|missing|unexpected|expected)\b|",
        r#"^\s*(?:E\s|>\s|\s+at\s|File "|\s*\^+\s*$)"#
    ))
});

/// Noise: lines a build or a test run emits to say "still going" or "this one was fine".
/// Each pattern names a specific, recognisable shape. Nothing generic, nothing length-based.
/// Blank lines are deliberately NOT here: they carry STRUCTURE — stripping them merges a
/// traceback into a wall. Long runs of them are handled by the repeat rule.
static NOISE: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        // pytest / unittest — passing results and the progress ruler
        (r"^\s*[.sxX]+\s*(\[\s*\d+%\]\s*)?$", "pytest progress"),
        (r"^\S+\s+PASSED(\s|$)", "pytest pass"),
        (r"^\s*\S+::\S+\s+PASSED", "pytest pass"),
        // node:test / TAP — an ok line is a passing assertion; "not ok" is SIGNAL
        (r"^\s*ok\s+\d+\s", "tap pass"),
        (
            r"^\s*#\s*(pass|duration_ms|todo|skip|subtests)\b",
            "tap summary noise",
        ),
        // jest / vitest / mocha
        (r"^\s*(?:[✓√]|PASS)\s", "jest pass"),
        // go test
        (r"^--- PASS:", "go pass"),
        (r"^\s*=== RUN\s", "go run marker"),
        (r"^ok\s+\S+\s+[\d.]+m?s$", "go package ok"),
        // cargo / rust
        (r"^test\s+.+\s\.\.\.\sok$", "cargo pass"),
        // package managers and downloaders
        (
            r"^\s*Requirement already satisfied:",
            "pip already satisfied",
        ),
        (
            r"^\s*(?:Downloading|Collecting|Using cached|Installing collected packages:)\s",
            "pip progress",
        ),
        (r"^\s*\[\d+/\d+\]\s", "step progress"),
        (
            r"^\s*(?:npm|yarn|pnpm)\s+(?:http|timing|info)\s",
            "npm chatter",
        ),
        (
            r"^\s*\d+(?:\.\d+)?%\s*(?:\||\[|complete)",
            "percent progress",
        ),
    ]
    .iter()
    .map(|(pattern, kind)| (compile(pattern), *kind))
    .collect()
});

/// The noise kind for a line, or `None` when the line is signal (or unrecognised, which counts
/// as signal).
pub fn noise_kind(line: &str) -> Option<&'static str> {
    if SIGNAL.is_match(line) {
        return None; // failure vocabulary always wins
    }
    NOISE
        .iter()
        .find(|(pattern, _)| pattern.is_match(line))
        .map(|(_, kind)| *kind)
}

/// The marker that stands in for the tail of a collapsed run. The COUNT is the information a
/// run of identical lines carries, so it is stated rather than quietly dropped.
fn repeat_marker(n: usize) -> String {
    format!(
        "    ... (previous line repeated {n} more time{})",
        if n == 1 { "" } else { "s" }
    )
}

/// The outcome of [`filter_noise`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Filtered {
    pub text: String,
    pub dropped: usize,
    pub collapsed: usize,
}

/// Drop the named-noise lines from `text` and cut long runs of identical lines short. Every
/// surviving line is byte-identical to its original except the `repeat_marker` lines — this
/// filters, it never rewrites, summarises or truncates.
pub fn filter_noise(text: &str) -> Filtered {
    let mut out: Vec<String> = Vec::new();
    let mut dropped = 0;
    let mut collapsed = 0;
    let mut run = 0;
    let mut pending = 0;
    let mut last: Option<&str> = None;
    for line in text.split('\n') {
        if noise_kind(line).is_some() {
            if pending > 0 {
                out.push(repeat_marker(pending));
                pending = 0;
            }
            dropped += 1;
            run = 0;
            last = None;
            continue;
        }
        if Some(line) == last {
            run += 1;
            if run >= REPEAT_RUN {
                collapsed += 1;
                pending += 1;
                continue; // the first REPEAT_RUN copies stay verbatim
            }
        } else {
            if pending > 0 {
                out.push(repeat_marker(pending));
                pending = 0;
            }
            run = 0;
        }
        last = Some(line);
        out.push(line.to_string());
    }
    if pending > 0 {
        out.push(repeat_marker(pending));
    }
    Filtered {
        text: out.join("\n"),
        dropped,
        collapsed,
    }
}

/// The text a tool response carries, whatever shape the host used for it. "" when there is none.
pub fn response_text(response: &Value) -> String {
    match response {
        Value::String(text) => text.clone(),
        Value::Object(fields) => ["stdout", "stderr", "output", "text", "content"]
            .iter()
            .filter_map(|key| fields.get(*key))
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A distillation that paid for itself.
#[derive(Clone, Debug)]
pub struct Distilled {
    pub text: String,
    pub original: String,
    pub dropped: usize,
    pub collapsed: usize,
    pub saving: f64,
}

/// The distilled replacement for a tool result, or `None` to pass the original through
/// untouched. `None` is the answer whenever anything is uncertain: an unlisted tool shape, a
/// short output, an output whose noise lines do not add up to a real saving, or a tool whose
/// text IS the answer.
pub fn distil(tool_name: &str, response: &Value) -> Option<Distilled> {
    if NEVER_DISTIL.contains(&tool_name) {
        return None; // the output IS what was asked for
    }
    let body = response_text(response);
    if body.len() < MIN_CHARS {
        return None; // short output is not a problem to solve
    }
    let filtered = filter_noise(&body);
    if filtered.dropped == 0 && filtered.collapsed == 0 {
        return None; // nothing nameable to drop; hands off
    }
    let saving = 1.0 - (filtered.text.len() as f64 / body.len() as f64);
    if saving < MIN_SAVING {
        return None;
    }
    Some(Distilled {
        text: filtered.text,
        original: body,
        dropped: filtered.dropped,
        collapsed: filtered.collapsed,
        saving,
    })
}

/// The line that tells the model what was removed and where the untouched original lives.
pub fn receipt(result: &Distilled, spill_path: Option<&str>) -> String {
    let pct = (result.saving * 100.0).round() as u64;
    let mut bits = vec![format!(
        "{} noise line{} removed",
        result.dropped,
        if result.dropped == 1 { "" } else { "s" }
    )];
    if result.collapsed > 0 {
        bits.push(format!(
            "{} repeated line{} collapsed",
            result.collapsed,
            if result.collapsed == 1 { "" } else { "s" }
        ));
    }
    let where_ = spill_path
        .map(|path| format!(" Full untouched output: {path}"))
        .unwrap_or_default();
    format!(
        "[Estelle curated this tool output: {}, {pct}% smaller. Nothing matching an error, failure, warning or traceback was removed.{where_}]",
        bits.join(", ")
    )
}

/// The PostToolUse envelope that replaces a tool result, in the shape Claude Code expects.
pub fn replacement(text: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "updatedToolOutput": {"type": "text", "text": text},
        },
    })
    .to_string()
}

/// The spill directory — resolved on CALL, not at module load (the auth.js rule: a home lookup
/// can block in a container in a way it never does on a laptop).
fn spill_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("tool-output"))
}

/// Write `body` to the spill directory and return its path, or `None` when it could not be
/// written. Best-effort by construction: a failed spill degrades to "no path in the receipt",
/// never to a lost tool result.
pub fn spill(body: &str, dir: Option<&Path>) -> Option<String> {
    let target = dir.map(PathBuf::from).or_else(spill_dir)?;
    fs::create_dir_all(&target).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).ok()?;
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let digest = format!("{:x}", Sha256::digest(body.as_bytes()));
    let file = target.join(format!("{millis}-{}.log", &digest[..12]));
    fs::write(&file, body).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).ok()?;
    }
    prune_spill(&target, SPILL_KEEP);
    Some(file.to_string_lossy().into_owned())
}

/// Keep the newest `keep` spill files; drop the rest. Never fails loudly.
fn prune_spill(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut logs = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| name.ends_with(".log"))
        .collect::<Vec<_>>();
    logs.sort();
    let stale = logs.len().saturating_sub(keep);
    for name in logs.into_iter().take(stale) {
        let _ = fs::remove_file(dir.join(name));
    }
}
