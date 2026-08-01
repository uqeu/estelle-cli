"use strict";
// PREVENTION INSTEAD OF EVICTION — the half of the thesis that DOES work inside Claude Code.
//
// No hook can remove content from a Claude Code context. PreCompact may BLOCK or INJECT, never curate; there
// is no supported way for a hook or an MCP server to trigger a clear or to choose what compaction keeps. So
// eviction is off the table there — but PostToolUse can REPLACE a tool's result before it ever reaches the
// model (`hookSpecificOutput.updatedToolOutput`, verified against the current Claude Code hooks reference).
// A verbose result that never enters the window costs nothing to hold and nothing to compact later. That is
// the same win as eviction, taken one moment earlier.
//
// THE RULE THIS FILE IS BUILT AROUND: destroying information the model needed is far worse than verbosity.
// A dropped stack trace costs a whole debugging loop; a kept one costs a few hundred tokens. So:
//
//   * It only ever drops lines it can NAME as noise. There is no "keep the first N and last M" truncation
//     anywhere in here — a line survives unless a specific pattern says what kind of noise it is.
//   * Failure vocabulary OVERRIDES every noise rule. A line that mentions an error, a failure or a traceback
//     is never dropped, whatever else it looks like.
//   * It refuses outright on tools whose output IS the answer (Read, Grep, Glob): the model asked for that
//     text, and a distiller has no way to know which line it was after.
//   * Every refusal path returns null, which the caller renders as "pass the output through untouched". The
//     failure mode of this file is verbosity, never silence.
//   * The full text is spilled to disk and NAMED in the replacement, so nothing is lost and the model can
//     read it back in full whenever the distillation was not enough.

const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

// Below this, verbosity is not a problem worth taking any risk to solve.
const MIN_CHARS = 2000;
// A distillation that does not clearly pay for itself is pure downside — pass the original through instead.
const MIN_SAVING = 0.25;
// The first this-many copies of an identical consecutive line are kept; the rest of the run is dropped. Three
// is enough to make a repeat visible as a repeat, which is all the information a run of identical lines has.
const REPEAT_RUN = 3;

// Tools whose output IS the thing the model asked for. Never touched — see the header.
const NEVER_DISTIL = new Set(["Read", "Grep", "Glob", "NotebookRead", "WebFetch", "WebSearch"]);

// Failure vocabulary. A line matching this is SIGNAL and survives every noise rule below. Deliberately wide:
// a false positive here costs a few tokens, a false negative costs the model the reason something broke.
const SIGNAL = new RegExp(
  "\\b(?:error|errors|traceback|exception|failed|failure|fail|fatal|panic|assert|assertion|" +
  "warning|warn|denied|refused|not found|no such|undefined|invalid|timeout|timed out|" +
  "cannot|could ?n['o]t|does ?n['o]t|missing|unexpected|expected)\\b|" +
  "^\\s*(?:E\\s|>\\s|\\s+at\\s|File \"|\\s*\\^+\\s*$)", "i");

// Noise: lines a build or a test run emits to say "still going" or "this one was fine". Each pattern names a
// specific, recognisable shape. Nothing generic, nothing length-based.
const NOISE = [
  // pytest / unittest — passing results and the progress ruler
  [/^\s*[.sxX]+\s*(\[\s*\d+%\]\s*)?$/, "pytest progress"],
  [/^\S+\s+PASSED(\s|$)/, "pytest pass"],
  [/^\s*\S+::\S+\s+PASSED/, "pytest pass"],
  // node:test / TAP — an ok line is a passing assertion; the failing ones say "not ok" and are SIGNAL
  [/^\s*ok\s+\d+\s/, "tap pass"],
  [/^\s*#\s*(pass|duration_ms|todo|skip|subtests)\b/, "tap summary noise"],
  // jest / vitest / mocha
  [/^\s*(?:[✓√]|PASS)\s/, "jest pass"],
  // go test
  [/^--- PASS:/, "go pass"],
  [/^\s*=== RUN\s/, "go run marker"],
  [/^ok\s+\S+\s+[\d.]+m?s$/, "go package ok"],
  // cargo / rust
  [/^test\s+.+\s\.\.\.\sok$/, "cargo pass"],
  // package managers and downloaders
  [/^\s*Requirement already satisfied:/, "pip already satisfied"],
  [/^\s*(?:Downloading|Collecting|Using cached|Installing collected packages:)\s/, "pip progress"],
  [/^\s*\[\d+\/\d+\]\s/, "step progress"],
  [/^\s*(?:npm|yarn|pnpm)\s+(?:http|timing|info)\s/, "npm chatter"],
  [/^\s*\d+(?:\.\d+)?%\s*(?:\||\[|complete)/, "percent progress"],
  // NOTE what is deliberately NOT here: blank lines. They carry no information, but they carry STRUCTURE —
  // stripping them merges a traceback into a wall and makes the surviving signal harder to read, which is a
  // real cost for a token saving of about one per line. Long RUNS of them are handled by the repeat rule.
];

/** The noise kind for a line, or "" when the line is signal (or unrecognised, which counts as signal). */
function noiseKind(line) {
  const text = String(line == null ? "" : line);
  if (SIGNAL.test(text)) return "";                 // failure vocabulary always wins
  for (const [pattern, kind] of NOISE) if (pattern.test(text)) return kind;
  return "";
}

/** The marker that stands in for the tail of a collapsed run. The COUNT is the information a run of identical
 * lines carries — "retrying" three times and "retrying" nine hundred times are different facts — so it is
 * stated rather than quietly dropped. */
function repeatMarker(n) {
  return `    ... (previous line repeated ${n} more time${n === 1 ? "" : "s"})`;
}

/**
 * Drop the named-noise lines from `text` and cut long runs of identical lines short.
 *
 * Returns `{text, dropped, collapsed}`. Every surviving line is byte-identical to its original except the
 * `repeatMarker` lines, which are the only text this function writes — it filters, it never rewrites,
 * summarises or truncates. A line it cannot name as noise always survives.
 */
function filterNoise(text) {
  const lines = String(text || "").split("\n");
  const out = [];
  let dropped = 0, collapsed = 0, run = 0, pending = 0, last = null;
  const flush = () => { if (pending > 0) { out.push(repeatMarker(pending)); pending = 0; } };
  for (const line of lines) {
    if (noiseKind(line)) { flush(); dropped++; run = 0; last = null; continue; }
    if (line === last) {
      run++;
      if (run >= REPEAT_RUN) { collapsed++; pending++; continue; }   // first REPEAT_RUN copies stay verbatim
    } else {
      flush();
      run = 0;
    }
    last = line;
    out.push(line);
  }
  flush();
  return { text: out.join("\n"), dropped, collapsed };
}

/** The text a tool response carries, whatever shape the host used for it. "" when there is none to read. */
function responseText(response) {
  if (response == null) return "";
  if (typeof response === "string") return response;
  if (typeof response !== "object") return "";
  const parts = [];
  for (const key of ["stdout", "stderr", "output", "text", "content"]) {
    const value = response[key];
    if (typeof value === "string" && value) parts.push(value);
  }
  return parts.join("\n");
}

/**
 * The distilled replacement for a tool result, or `null` to pass the original through untouched.
 *
 * `null` is the answer whenever anything is uncertain: an unlisted tool shape, a short output, an output whose
 * noise lines do not add up to a real saving, or a tool whose text IS the answer. Returns
 * `{text, original, kept, dropped, collapsed, saving}` when it does distil.
 */
function distil(payload, options) {
  const opts = options || {};
  const tool = String((payload && payload.tool_name) || "");
  if (NEVER_DISTIL.has(tool)) return null;                       // the output IS what was asked for
  const body = responseText(payload && payload.tool_response);
  const minChars = opts.minChars == null ? MIN_CHARS : opts.minChars;
  if (body.length < minChars) return null;                       // short output is not a problem to solve
  const { text, dropped, collapsed } = filterNoise(body);
  if (!dropped && !collapsed) return null;                       // nothing nameable to drop; hands off
  const saving = 1 - (text.length / body.length);
  if (saving < (opts.minSaving == null ? MIN_SAVING : opts.minSaving)) return null;
  return { text, original: body, kept: text.length, dropped, collapsed, saving };
}

/** The line that tells the model what was removed and where the untouched original lives. */
function receipt(result, spillPath) {
  const pct = Math.round((result.saving || 0) * 100);
  const bits = [`${result.dropped} noise line${result.dropped === 1 ? "" : "s"} removed`];
  if (result.collapsed) bits.push(`${result.collapsed} repeated line${result.collapsed === 1 ? "" : "s"} collapsed`);
  const where = spillPath ? ` Full untouched output: ${spillPath}` : "";
  return `[Estelle curated this tool output: ${bits.join(", ")}, ${pct}% smaller. `
    + `Nothing matching an error, failure, warning or traceback was removed.${where}]`;
}

// ── the spill: nothing is lost ──────────────────────────────────────────────────
// The distilled form is what enters the window; the ORIGINAL goes to disk and is named in the replacement, so
// a model that needs the part we dropped can read it back in full. Bounded so a long session cannot fill a
// home directory. Best-effort by construction: a spill that fails degrades to "no path in the receipt", never
// to a lost tool result -- the caller only ever replaces output it has already produced.

// Resolved on CALL, not at module load — the same rule auth.js:20-22 already states for the same reason.
// This was the ONLY eager module-scope os.homedir() in cli/bin, and it is the one file that produced ZERO
// TAP output in CI while every other file passed: `node --test` names a file that never emitted a single
// test, which is an IMPORT-time stall, not a slow test. os.homedir() reads $HOME and falls back to a
// passwd/NSS lookup when it is unset, which can block in a container in a way it never does on a laptop —
// so an eager call runs that lookup before a single line of the module's own logic.
//
// Honest about the evidence: this was NOT reproduced locally (import is 7ms with HOME unset on macOS). It
// is fixed because it breaks a rule this codebase wrote down for itself, and the next CI run is the probe.
const spillDir = () => path.join(os.homedir(), ".estelle", "tool-output");
const SPILL_KEEP = 200;

/** Write `body` to the spill directory and return its path, or "" when it could not be written. */
function spill(body, dir) {
  const target = dir || spillDir();
  try {
    fs.mkdirSync(target, { recursive: true, mode: 0o700 });
    const name = `${Date.now()}-${crypto.createHash("sha256").update(body).digest("hex").slice(0, 12)}.log`;
    const file = path.join(target, name);
    fs.writeFileSync(file, body, { mode: 0o600 });
    pruneSpill(target);
    return file;
  } catch {
    return "";
  }
}

/** Keep the newest `SPILL_KEEP` spill files; drop the rest. Never throws. */
function pruneSpill(dir, keep) {
  try {
    const limit = keep == null ? SPILL_KEEP : keep;
    const files = fs.readdirSync(dir).filter((f) => f.endsWith(".log")).sort();
    for (const stale of files.slice(0, Math.max(0, files.length - limit))) {
      try { fs.unlinkSync(path.join(dir, stale)); } catch { /* already gone */ }
    }
  } catch { /* unreadable spill dir is not an error worth surfacing */ }
}

/** The PostToolUse envelope that replaces a tool result, in the shape Claude Code expects. */
function replacement(text) {
  return {
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      updatedToolOutput: { type: "text", text: String(text || "") },
    },
  };
}

module.exports = {
  noiseKind, filterNoise, repeatMarker, responseText, distil, receipt, replacement,
  spill, pruneSpill, spillDir, SPILL_KEEP, MIN_CHARS, MIN_SAVING, REPEAT_RUN, NEVER_DISTIL,
};
