//! PUSH PROTECTION — refuse a `git push`, or an agent's file write, that would carry a
//! LIVE-SHAPED credential, before the bytes leave the machine.
//!
//! Two doors, one detector:
//!   * **`git push`** rides the existing `PreToolUse Bash` row (`guard`). The outgoing set is every
//!     commit reachable from what is being pushed and NOT on any remote (`--not --remotes`), and each
//!     commit's patch is read separately — so a key added in one commit and deleted in the next is
//!     still caught, because both commits travel and the first one carries the value.
//!   * **a file write** rides the existing `PreToolUse Write|Edit` row (`ground`), before the network
//!     call, so it refuses on every language the grounding gate abstains on.
//!
//! The detector is [`estelle_client::find_secret_shape`] and nothing else — the same function that
//! refuses a credential pasted into a prompt, with its false-positive discipline intact: `\bsk-` plus
//! a digit (the 0.4.2 `ask-flow-…` fix), the engine's entropy gates, and the upstream allowlists of
//! published examples (`AKIAIOSFODNN7EXAMPLE`). A second detector here would be a second owner of
//! "what is a credential" (Power of Ten #9), and the two would disagree.
//!
//! A refusal names `file:line` and the shape — NEVER the value. A bypass needs a reason, and the
//! reason is RECORDED (`~/.estelle/push-protection.jsonl`) before the action is allowed; a bypass
//! that cannot be recorded is refused, because an unrecorded bypass is the silent kind.
//!
//! LIMITS, out loud:
//!   * **"Live-shaped" is a SHAPE, not a validation.** Nothing here calls a provider to ask whether
//!     the key works; that is the server's `secret_validate` path. A revoked key in the right shape
//!     is refused; a live key in a shape the catalogue does not know is not.
//!   * The push door reads the COMMAND TEXT. An alias, a wrapper script, a `$(…)` or a push made
//!     outside the agent's Bash tool (a human's terminal) is not seen. This is an agent guard, not a
//!     git `pre-push` hook.
//!   * "Outgoing" is `--not --remotes`: a commit already on ANY remote-tracking ref counts as pushed,
//!     even if the push targets a different remote.
//!   * A capped read (too many commits, too many bytes) is reported as NOT SCANNED, never as clean,
//!     and it does not block — a guard that refuses every large push is turned off within a day.

use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use serde_json::Value;
use serde_json::json;

/// The line marker that downgrades ONE line from a refusal to a recorded bypass. The text after it
/// is the reason, and it must not be empty.
pub(crate) const ALLOW_MARKER: &str = "estelle:allow-secret";
/// The environment-assignment prefix that bypasses a whole push: `ESTELLE_ALLOW_SECRET="why" git push`.
/// Read from the COMMAND TEXT the hook was shown, never from the hook's own environment.
pub(crate) const ALLOW_PUSH_VAR: &str = "ESTELLE_ALLOW_SECRET";
/// The kill switch, read from the hook's environment: `ESTELLE_PUSH_PROTECTION=off`.
const KILL_SWITCH_VAR: &str = "ESTELLE_PUSH_PROTECTION";

/// Commits read per push. Above this the push is NOT SCANNED (said so), never certified clean.
const MAX_OUTGOING_COMMITS: usize = 500;
/// Patch bytes read per push.
const MAX_PATCH_BYTES: u64 = 32 * 1024 * 1024;
/// Bytes read of an edited file, only to turn an `Edit`'s relative line into a file line.
const MAX_EDITED_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Findings named in one refusal; the rest are counted.
const MAX_NAMED_FINDINGS: usize = 8;
/// Shell segments examined per command.
const MAX_SEGMENTS: usize = 32;

/// One credential-shaped added line. `kind` is the detector's shape name; the value is never held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub path: String,
    /// 1-based; a FILE line unless `place` says what else it counts.
    pub line: usize,
    /// "" for a file line; otherwise how `line` is to be read ("of the edit", "commit 1a2b3c4").
    pub place: String,
    pub kind: &'static str,
    /// `Some(reason)` when the line carries [`ALLOW_MARKER`]; `Some("")` when it carries it bare.
    pub allow: Option<String>,
}

impl Finding {
    fn label(&self) -> String {
        let at = format!("{}:{}", self.path, self.line);
        if self.place.is_empty() {
            format!("{at} ({})", self.kind)
        } else {
            format!("{at} ({}, {})", self.kind, self.place)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Not a push / nothing credential-shaped / switched off.
    Clear,
    /// Refuse the tool call with this reason.
    Block(String),
    /// Allowed, and the bypass was recorded — a warning for the human.
    Bypassed(String),
    /// The guard could not read everything it needed: a warning, never a pass.
    NotScanned(String),
}

pub(crate) fn switched_off() -> bool {
    std::env::var(KILL_SWITCH_VAR).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false"
        )
    })
}

pub(crate) fn default_log_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".estelle").join("push-protection.jsonl"))
}

/// The detector over one line, honouring the allow marker. Pure.
fn scan_line(text: &str) -> Option<(&'static str, Option<String>)> {
    let (kind, _) = estelle_client::find_secret_shape(text)?;
    let allow = text.find(ALLOW_MARKER).map(|at| {
        let rest = &text[at + ALLOW_MARKER.len()..];
        rest.trim_start_matches([':', ' ', '\t'])
            .trim_end_matches(['*', '/', '>', '-', ' ', '\t', '"', '\''])
            .trim()
            .to_string()
    });
    Some((kind, allow))
}

// ─── the push door ───────────────────────────────────────────────────────────────────────────

/// What one `git push` in a command line would send. Pure output of [`parse_push`].
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PushCommand {
    /// Every `-C <dir>`, in order; resolved against the hook's cwd.
    pub dirs: Vec<String>,
    /// Revisions whose unpushed commits travel. Empty = nothing content-bearing (`--delete`).
    pub revs: Vec<String>,
    /// The `ESTELLE_ALLOW_SECRET=` value on that segment, if any.
    pub bypass: Option<String>,
}

/// Split a command line at unquoted `;`, `&`, `|` and newlines. Bounded by [`MAX_SEGMENTS`].
fn segments(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in command.chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quote != Some('\'') {
            escaped = true;
        } else if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if matches!(ch, ';' | '&' | '|' | '\n') {
            out.push(std::mem::take(&mut current));
            if out.len() >= MAX_SEGMENTS {
                return out;
            }
            continue;
        }
        current.push(ch);
    }
    out.push(current);
    out
}

/// Every `git push` in `command`. `Err` when a segment LOOKS like a push but cannot be parsed —
/// the caller reports that as not scanned rather than guessing.
pub(crate) fn parse_pushes(command: &str) -> Result<Vec<PushCommand>, String> {
    let mut pushes = Vec::new();
    for segment in segments(command) {
        let looks_like_push = segment.contains("git") && segment.contains("push");
        match shlex::split(&segment) {
            Some(words) => pushes.extend(parse_push(&words)),
            None if looks_like_push => {
                return Err("a `git push` whose quoting could not be parsed".to_string());
            }
            None => {}
        }
    }
    Ok(pushes)
}

/// One shell segment's words → the push it performs, or `None` when it is not a `git push`.
pub(crate) fn parse_push(words: &[String]) -> Option<PushCommand> {
    let mut push = PushCommand::default();
    let mut index = 0;
    // Leading `env` and NAME=value assignments.
    while let Some(word) = words.get(index) {
        if word == "env" {
            index += 1;
            continue;
        }
        let Some((name, value)) = word.split_once('=') else {
            break;
        };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            break;
        }
        if name == ALLOW_PUSH_VAR {
            push.bypass = Some(value.trim().to_string());
        }
        index += 1;
    }
    if words.get(index).map(String::as_str) != Some("git") {
        return None;
    }
    index += 1;
    // git's global options, up to the subcommand.
    while let Some(word) = words.get(index) {
        match word.as_str() {
            "-C" => {
                push.dirs.push(words.get(index + 1)?.clone());
                index += 2;
            }
            "-c" | "--git-dir" | "--work-tree" | "--namespace" => index += 2,
            option if option.starts_with('-') => index += 1,
            _ => break,
        }
    }
    if words.get(index).map(String::as_str) != Some("push") {
        return None;
    }
    push.revs = push_revs(&words[index + 1..]);
    Some(push)
}

/// The revisions a `git push <args>` sends. `HEAD` when no refspec names one.
fn push_revs(args: &[String]) -> Vec<String> {
    const VALUED: &[&str] = &["-o", "--push-option", "--repo", "--receive-pack", "--exec"];
    let mut positionals = Vec::new();
    let mut every_branch = false;
    let mut tags = false;
    let mut delete = false;
    let mut index = 0;
    while let Some(word) = args.get(index) {
        index += 1;
        match word.as_str() {
            "--all" | "--branches" | "--mirror" => every_branch = true,
            "--tags" => tags = true,
            "-d" | "--delete" => delete = true,
            option if VALUED.contains(&option) => index += 1,
            option if option.starts_with('-') => {}
            _ => positionals.push(word.clone()),
        }
    }
    if delete {
        return Vec::new();
    }
    let mut revs: Vec<String> = positionals
        .iter()
        .skip(1) // the remote
        .filter_map(|refspec| {
            let source = refspec.trim_start_matches('+');
            let source = source.split_once(':').map_or(source, |(src, _)| src);
            (!source.is_empty() && !source.starts_with('-')).then(|| source.to_string())
        })
        .collect();
    if every_branch {
        revs.push("--branches".to_string());
    }
    if tags {
        revs.push("--tags".to_string());
    }
    if revs.is_empty() && !every_branch && !tags {
        revs.push("HEAD".to_string());
    }
    revs
}

/// One added line of an outgoing patch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AddedLine {
    pub commit: String,
    pub path: String,
    pub line: usize,
    pub text: String,
}

/// The commit separator [`outgoing_patch`] asks git to print. Content lines always start with
/// `+`, `-`, ` ` or `\` under `-p`, so this cannot collide with a line of a file.
const COMMIT_MARK: &str = "estelle-commit ";

/// `git log -p -U0` output → every ADDED line with its file line. Pure.
pub(crate) fn parse_patch(patch: &str) -> Vec<AddedLine> {
    let mut out = Vec::new();
    let mut commit = String::new();
    let mut path = String::new();
    let mut next_line = 0usize;
    for raw in patch.lines() {
        if let Some(sha) = raw.strip_prefix(COMMIT_MARK) {
            commit = sha.chars().take(12).collect();
            path.clear();
        } else if let Some(target) = raw.strip_prefix("+++ ") {
            path = target.strip_prefix("b/").unwrap_or(target).to_string();
        } else if raw.starts_with("@@ ") {
            next_line = hunk_start(raw).unwrap_or(0);
        } else if let Some(text) = raw.strip_prefix('+') {
            if !path.is_empty() && path != "/dev/null" && next_line > 0 {
                out.push(AddedLine {
                    commit: commit.clone(),
                    path: path.clone(),
                    line: next_line,
                    text: text.to_string(),
                });
            }
            next_line += 1;
        } else if raw.starts_with(' ') {
            next_line += 1;
        }
    }
    out
}

/// `@@ -a,b +c,d @@` → c.
fn hunk_start(header: &str) -> Option<usize> {
    let plus = header.split_whitespace().nth(2)?.strip_prefix('+')?;
    plus.split(',').next()?.parse().ok()
}

/// Added lines → findings. Pure.
pub(crate) fn findings_in(lines: &[AddedLine]) -> Vec<Finding> {
    lines
        .iter()
        .filter_map(|added| {
            let (kind, allow) = scan_line(&added.text)?;
            Some(Finding {
                path: added.path.clone(),
                line: added.line,
                place: format!("commit {}", &added.commit[..added.commit.len().min(7)]),
                kind,
                allow,
            })
        })
        .collect()
}

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotePath=false", "--no-pager"])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// How many outgoing commits `revs` carries, or why it could not be counted.
fn outgoing_count(dir: &Path, revs: &[String]) -> Result<usize, String> {
    let output = git(dir)
        .args(["rev-list", "--count"])
        .args(revs)
        .args(["--not", "--remotes"])
        .output()
        .map_err(|_| "git could not be run".to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git could not list the outgoing commits of {}",
            revs.join(" ")
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "git's commit count was not a number".to_string())
}

/// The outgoing commits' patches, bounded by [`MAX_PATCH_BYTES`].
fn outgoing_patch(dir: &Path, revs: &[String]) -> Result<String, String> {
    let mut child = git(dir)
        .args([
            "log",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            "-p",
            "-U0",
        ])
        .arg(format!("--format={COMMIT_MARK}%H"))
        .args(revs)
        .args(["--not", "--remotes"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|_| "git could not be run".to_string())?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .map(|stdout| stdout.take(MAX_PATCH_BYTES + 1).read_to_end(&mut bytes));
    if bytes.len() as u64 > MAX_PATCH_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "the outgoing patch is larger than {} MiB",
            MAX_PATCH_BYTES / 1024 / 1024
        ));
    }
    let status = child.wait().map_err(|_| "git did not exit".to_string())?;
    if !matches!(read, Some(Ok(_))) || !status.success() {
        return Err("git could not produce the whole outgoing patch".to_string());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The push door. `cwd` is the directory the command runs in; `log` is where a bypass is recorded.
pub(crate) fn check_push(command: &str, cwd: &Path, log: Option<&Path>) -> Verdict {
    if !command.contains("push") {
        return Verdict::Clear;
    }
    let pushes = match parse_pushes(command) {
        Ok(pushes) => pushes,
        Err(why) => return not_scanned(&why),
    };
    let mut findings = Vec::new();
    let mut bypass = None;
    for push in pushes.iter().filter(|push| !push.revs.is_empty()) {
        let dir = push
            .dirs
            .iter()
            .fold(cwd.to_path_buf(), |dir, step| dir.join(step));
        match outgoing_count(&dir, &push.revs) {
            Err(why) => return not_scanned(&why),
            Ok(count) if count > MAX_OUTGOING_COMMITS => {
                return not_scanned(&format!(
                    "{count} outgoing commits is more than the {MAX_OUTGOING_COMMITS} it reads"
                ));
            }
            Ok(_) => {}
        }
        let patch = match outgoing_patch(&dir, &push.revs) {
            Ok(patch) => patch,
            Err(why) => return not_scanned(&why),
        };
        findings.extend(findings_in(&parse_patch(&patch)));
        bypass = bypass.or(push.bypass.clone());
    }
    decide("push", &findings, bypass.as_deref(), log)
}

fn not_scanned(why: &str) -> Verdict {
    Verdict::NotScanned(format!(
        "Estelle push protection did NOT scan this push ({why}). It is not blocked, and it is not certified free of credentials."
    ))
}

// ─── the write door ──────────────────────────────────────────────────────────────────────────

/// A write tool call's `tool_input` → findings, with FILE lines wherever they are knowable.
/// `file_text` is the current file, used only to place an `Edit`'s `new_string`.
pub(crate) fn write_findings(path: &str, input: &Value, file_text: Option<&str>) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Some(content) = ["content", "contents", "text"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
    {
        findings.extend(text_findings(path, content, 0, ""));
    }
    let mut edits: Vec<&Value> = vec![input];
    if let Some(list) = input.get("edits").and_then(Value::as_array) {
        edits.extend(list);
    }
    for edit in edits {
        let Some(new) = edit.get("new_string").and_then(Value::as_str) else {
            continue;
        };
        let old = edit
            .get("old_string")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let base = file_text
            .filter(|_| !old.is_empty())
            .and_then(|text| text.find(old).map(|at| text[..at].matches('\n').count()));
        let place = if base.is_some() {
            ""
        } else {
            "line of the edit's new text"
        };
        findings.extend(text_findings(path, new, base.unwrap_or(0), place));
    }
    if let Some(patch) = ["input", "patch"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
    {
        findings.extend(patch_body_findings(patch));
    }
    findings
}

fn text_findings(path: &str, text: &str, base: usize, place: &str) -> Vec<Finding> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (kind, allow) = scan_line(line)?;
            Some(Finding {
                path: path.to_string(),
                line: base + index + 1,
                place: place.to_string(),
                kind,
                allow,
            })
        })
        .collect()
}

/// An apply-patch body: only its `+` lines are new, and only the patch's own position is known.
fn patch_body_findings(patch: &str) -> Vec<Finding> {
    let mut path = String::from("the patch");
    let mut findings = Vec::new();
    for (index, line) in patch.lines().enumerate() {
        let header = ["*** Update File:", "*** Add File:"]
            .iter()
            .find_map(|marker| line.strip_prefix(marker));
        if let Some(named) = header {
            path = named.trim().to_string();
            continue;
        }
        let Some(added) = line.strip_prefix('+') else {
            continue;
        };
        if let Some((kind, allow)) = scan_line(added) {
            findings.push(Finding {
                path: path.clone(),
                line: index + 1,
                place: "line of the patch".to_string(),
                kind,
                allow,
            });
        }
    }
    findings
}

/// Read the file being edited, bounded; `None` when it is absent, unreadable or too large.
pub(crate) fn read_bounded(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(MAX_EDITED_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= MAX_EDITED_FILE_BYTES).then_some(text)
}

/// The write door.
pub(crate) fn check_write(path: &str, input: &Value, cwd: &Path, log: Option<&Path>) -> Verdict {
    let resolved = cwd.join(path);
    let file_text = if input.get("new_string").is_some() || input.get("edits").is_some() {
        read_bounded(&resolved)
    } else {
        None
    };
    let findings = write_findings(path, input, file_text.as_deref());
    decide("write", &findings, None, log)
}

// ─── the decision, shared by both doors ──────────────────────────────────────────────────────

/// Findings + an optional whole-action bypass → the verdict. Records every bypass before allowing.
pub(crate) fn decide(
    surface: &str,
    findings: &[Finding],
    bypass: Option<&str>,
    log: Option<&Path>,
) -> Verdict {
    if findings.is_empty() {
        return Verdict::Clear;
    }
    let whole = bypass.map(str::trim).filter(|reason| !reason.is_empty());
    let unexcused: Vec<&Finding> = findings
        .iter()
        .filter(|finding| whole.is_none() && finding.allow.as_deref().is_none_or(str::is_empty))
        .collect();
    debug_assert!(unexcused.len() <= findings.len());
    if !unexcused.is_empty() {
        return Verdict::Block(block_reason(surface, &unexcused, bypass.is_some()));
    }
    let reasons: Vec<String> = match whole {
        Some(reason) => vec![reason.to_string()],
        None => findings.iter().filter_map(|f| f.allow.clone()).collect(),
    };
    assert!(
        !reasons.is_empty(),
        "an allowed finding must carry a reason"
    );
    match record_bypass(surface, findings, &reasons, log) {
        Ok(()) => Verdict::Bypassed(format!(
            "Estelle push protection: {} credential-shaped line(s) allowed through on a recorded reason ({}): {}.",
            findings.len(),
            reasons.join("; "),
            named(findings.iter().collect::<Vec<_>>().as_slice()),
        )),
        Err(why) => Verdict::Block(format!(
            "Estelle push protection refused this {surface}: a bypass was given, but it could not be recorded ({why}), and an unrecorded bypass is not allowed. {}",
            named(findings.iter().collect::<Vec<_>>().as_slice()),
        )),
    }
}

fn named(findings: &[&Finding]) -> String {
    let mut labels: Vec<String> = findings
        .iter()
        .take(MAX_NAMED_FINDINGS)
        .map(|finding| finding.label())
        .collect();
    if findings.len() > MAX_NAMED_FINDINGS {
        labels.push(format!("and {} more", findings.len() - MAX_NAMED_FINDINGS));
    }
    labels.join(", ")
}

fn block_reason(surface: &str, findings: &[&Finding], empty_bypass: bool) -> String {
    let what = if surface == "push" {
        "the outgoing commits add"
    } else {
        "the new text contains"
    };
    let remedy = if surface == "push" {
        "Remove the credential AND rewrite the commit that added it (a later commit deleting it does not take it out of history), and load it from the environment instead"
    } else {
        "Read the credential from the environment or a secret manager instead of writing it into a file"
    };
    let bypass = if surface == "push" {
        format!(
            "If it is deliberate (a revoked key, a test fixture), bypass with a recorded reason: prefix the command with {ALLOW_PUSH_VAR}=\"<why>\", or put `{ALLOW_MARKER} <why>` on that line"
        )
    } else {
        format!(
            "If it is deliberate (a test fixture), put `{ALLOW_MARKER} <why>` on that line; the reason is recorded"
        )
    };
    let empty = if empty_bypass {
        " A bypass was given with no reason, which does not count."
    } else {
        ""
    };
    format!(
        "Estelle push protection blocked this {surface}: {what} {} live-shaped credential(s) — {}. The value is not shown.{empty} {remedy}. {bypass}.",
        findings.len(),
        named(findings),
    )
}

/// Append one bypass to the local audit log. File, line, shape and reason — never the value.
fn record_bypass(
    surface: &str,
    findings: &[Finding],
    reasons: &[String],
    log: Option<&Path>,
) -> Result<(), String> {
    let path = log.ok_or("no home directory to record it in")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "the log directory could not be created")?;
    }
    let entry = json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "surface": surface,
        "reasons": reasons,
        "findings": findings.iter().map(|finding| json!({
            "path": finding.path,
            "line": finding.line,
            "place": finding.place,
            "kind": finding.kind,
        })).collect::<Vec<_>>(),
    });
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "the log could not be opened")?;
    writeln!(file, "{entry}").map_err(|_| "the log could not be written")?;
    Ok(())
}

#[cfg(test)]
#[path = "push_protect_tests.rs"]
mod tests;
