"use strict";
// SESSION-GAP AWARENESS, customer side — what Estelle says when you come back.
//
// The founder's requirement: "Estelle should know I slept eight hours. Whenever you start Estelle it gives
// you a summary of what's happened since you've been away — and especially if code you last touched has been
// touched since."
//
// This is the half that runs on the customer's own machine, and it runs there for one reason: the founder's
// first rule is that Estelle is NEVER SLOW, and this sits on the session-start path. So it makes no network
// call at all. Everything it needs is already local:
//
//   * WHEN you left           → written by the `checkpoint` hook, which already fires on every Stop.
//   * WHAT you were in        → the Write/Edit tool calls in the host's own transcript. No model involved.
//   * WHAT MOVED since        → `git log <your HEAD>..HEAD`. Git is the ground truth for "someone changed
//                               this file", it knows WHO and WHEN, and it is the same record whether the
//                               change came from a teammate, a merge, or Estelle's own repair loop.
//   * WHERE YOU ARE           → the machine's own IANA zone. Not a guess: the device saying where it is.
//
// Two refusals are the whole design:
//
//   * NOTHING IS SAID THAT ISN'T TRUE. Every line comes from a git record with an author and a timestamp.
//     A change that cannot be timed is dropped rather than narrated, and `null` (I could not look) never
//     renders as "nothing changed".
//   * SILENCE IS THE DEFAULT. A first session in this repo, a gap shorter than one working session, or any
//     git/state failure at all produces no output. A welcome message is worth zero seconds of anyone's day.
//
// Kept string-for-string identical to src/estelle/serve/session_gap.py, which is contracted in
// tests/test_hook_contract.py — the two implementations render the same brief or a test fails.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

// --- thresholds -------------------------------------------------------------------------------------
// NOT new numbers. 1800s is the session-diary's OWN boundary (session_diary.DEFAULT_WINDOW_SECONDS): the gap
// at which Estelle already stops calling two runs the same working session. Speaking sooner would have the
// welcome contradict the session list it welcomes you back to. 3600s is classify_gap's short-break ceiling —
// past it, an absence is worth acknowledging even with no news; before it, the brief must earn its place.
const MIN_GAP_SECONDS = 1800;
const NEWS_FREE_GAP_SECONDS = 3600;
const MAX_FILES = 3;
const MAX_ACTORS = 3;
// A change's description IS a commit subject, and a commit subject can be a paragraph. Measured on this repo:
// one ran 96 characters and wrapped the line twice on a normal terminal. The file and the author — what the
// line is actually for — are never truncated; the subject is, with a visible ellipsis so a cut is never
// mistaken for the whole thing.
const MAX_WHAT = 60;

const SILENT_DISABLED = "disabled";
const SILENT_FIRST_SESSION = "first-session";
const SILENT_UNKNOWN = "unknown-gap";
const SILENT_SAME_SESSION = "same-session";
const SILENT_NO_NEWS = "nothing-to-say";

const UNKNOWN_ZONE_NOTE = "time zone not set, so this is UTC and not your local time";

// Bounds on the git read. A returning customer waits for none of this: a long absence is summarised from the
// most recent commits, not from all of them, and a slow repo times out into silence.
const MAX_COMMITS = 60;
const MAX_TRACKED_FILES = 40;
const GIT_TIMEOUT_MS = 1500;

// --- pure time -------------------------------------------------------------------------------------

/** Milliseconds for an ISO timestamp, or null when it will not parse (never throws). */
function parseAt(value) {
  if (typeof value !== "string" || !value.trim()) return null;
  const ms = Date.parse(value.trim());
  return Number.isFinite(ms) ? ms : null;
}

/** Seconds from `earlier` to `later`, or null when either will not parse. */
function secondsBetween(earlier, later) {
  const a = parseAt(earlier), b = parseAt(later);
  return a === null || b === null ? null : (b - a) / 1000;
}

function plural(count, unit) {
  return count === 1 ? `${count} ${unit}` : `${count} ${unit}s`;
}

/** A span as a phrase a person would use. Coarse on purpose: "about 8 hours" is true of 7h40m and 8h20m
 * alike, and a precise "8.3 hours" is uglier and no more useful. */
function humanize(seconds) {
  const s = Math.max(0, Number(seconds) || 0);
  if (s < 60) return "under a minute";
  if (s < 3600) return plural(Math.round(s / 60), "minute");
  if (s < 172800) return "about " + plural(Math.round(s / 3600), "hour");
  return "about " + plural(Math.round(s / 86400), "day");
}

/** The machine's own IANA zone, or "" when the runtime will not say. Ground truth about where the customer
 * is — strictly better than the server's clock, strictly worse than a zone they set explicitly. */
function machineZone() {
  try { return Intl.DateTimeFormat().resolvedOptions().timeZone || ""; } catch { return ""; }
}

/** True when `name` is a zone this runtime recognizes. An unrecognized zone is UNKNOWN, never a fallback to
 * the local one — being wrong about someone's time zone is the failure this whole path avoids. */
function knownZone(name) {
  if (typeof name !== "string" || !name.trim()) return false;
  try { new Intl.DateTimeFormat("en-GB", { timeZone: name.trim() }); return true; } catch { return false; }
}

/** "07:12 Friday (America/Toronto)", or "11:12 Friday UTC (time zone not set…)" when the zone is unknown.
 * Naming the zone is what makes the line TRUE rather than merely plausible. "" for an unparseable now. */
function localTimeLine(nowIso, tzName) {
  if (parseAt(nowIso) === null) return "";
  const known = knownZone(tzName);
  const timeZone = known ? tzName.trim() : "UTC";
  const parts = new Intl.DateTimeFormat("en-GB", {
    timeZone, weekday: "long", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  }).formatToParts(new Date(parseAt(nowIso)));
  const at = (type) => (parts.find((p) => p.type === type) || {}).value || "";
  const stamp = `${at("hour")}:${at("minute")} ${at("weekday")}`;
  return known ? `${stamp} (${timeZone})` : `${stamp} UTC (${UNKNOWN_ZONE_NOTE})`;
}

// --- pure brief ------------------------------------------------------------------------------------

function clean(value) {
  return typeof value === "string" && value.trim() ? value.trim() : "";
}

/** The changes that PROVABLY happened between leaving and now, newest first.
 *
 * A change whose timestamp will not parse is dropped, not kept "just in case": the brief's entire value is
 * that every line in it is true, and "something may have happened at some point" is not worth a returning
 * customer's attention. A change stamped in the future is dropped for the same reason. */
function changesSince(changes, lastSeen, now) {
  const kept = [];
  for (const change of changes || []) {
    if (!change || typeof change !== "object") continue;
    const elapsed = secondsBetween(clean(change.at), now);
    const sinceLeft = secondsBetween(lastSeen, clean(change.at));
    if (elapsed === null || sinceLeft === null || elapsed < 0 || sinceLeft <= 0) continue;
    kept.push([elapsed, change]);
  }
  return kept.sort((a, b) => a[0] - b[0]).map((pair) => pair[1]);
}

/** The newest change to each file the customer last worked in — "code you touched has moved". A set
 * intersection, not a search. Order follows `myFiles` (most-recently-touched first), so the file they were
 * actually in when they stopped leads. */
function movedFiles(myFiles, recent) {
  const byPath = new Map();
  for (const change of recent || []) {
    const p = clean(change.path);
    if (p && !byPath.has(p)) byPath.set(p, change);
  }
  const out = [];
  const seen = new Set();
  for (const raw of myFiles || []) {
    const p = clean(raw);
    if (!p || seen.has(p) || !byPath.has(p)) continue;
    seen.add(p);
    out.push(byPath.get(p));
  }
  return out;
}

/** "by dana" — or "author not recorded". Never a guessed name. */
function byLine(change) {
  const actor = clean(change.actor);
  return actor ? `by ${actor}` : "author not recorded";
}

function movedLine(change, now) {
  const elapsed = secondsBetween(clean(change.at), now) || 0;
  const raw = clean(change.what);
  const what = raw.length > MAX_WHAT ? `${raw.slice(0, MAX_WHAT - 1)}…` : raw;
  return `- ${clean(change.path)} — ${byLine(change)}, ${humanize(elapsed)} ago${what ? ` — ${what}` : ""}`;
}

/** "by dana and sam", bounded — the fifth name in a list is read by nobody and costs the same attention as
 * the first. "" when nothing named an author. */
function actorsPhrase(changes) {
  const names = [];
  for (const change of changes) {
    const actor = clean(change.actor);
    if (actor && !names.includes(actor)) names.push(actor);
  }
  if (!names.length) return "";
  const shown = names.slice(0, MAX_ACTORS);
  const extra = names.length - MAX_ACTORS;
  const joined = shown.length === 1 ? shown[0]
    : `${shown.slice(0, -1).join(", ")} and ${shown[shown.length - 1]}`;
  return `by ${joined}` + (extra > 0 ? ` and ${extra} more` : "");
}

function restLine(rest) {
  if (!rest.length) return "";
  const who = actorsPhrase(rest);
  return `Elsewhere while you were away: ${plural(rest.length, "change")}` + (who ? `, ${who}.` : ".");
}

/** The brief a returning customer sees — or a reasoned silence.
 *
 * `changes: null` means the collector could not run, and the brief then makes NO claim about the repo;
 * `changes: []` means Estelle looked and the repo was quiet, which it may say. Collapsing the two would turn
 * a broken git read into a confident "nothing changed". */
function brief({ now, lastSeen, myFiles = [], changes = [], tzName = "", enabled = true } = {}) {
  const silent = (reason) => ({ show: false, reason, seconds: 0, lines: [], text: "", moved: [] });
  if (!enabled) return silent(SILENT_DISABLED);
  if (!clean(lastSeen)) return silent(SILENT_FIRST_SESSION);
  const seconds = secondsBetween(lastSeen, now);
  if (seconds === null) return silent(SILENT_UNKNOWN);
  if (seconds < MIN_GAP_SECONDS) return { ...silent(SILENT_SAME_SESSION), seconds };

  const looked = changes !== null && changes !== undefined;
  const recent = changesSince(changes || [], lastSeen, now);
  const moved = movedFiles(myFiles, recent);
  const rest = recent.filter((change) => !moved.includes(change));
  if (!recent.length && seconds < NEWS_FREE_GAP_SECONDS) return { ...silent(SILENT_NO_NEWS), seconds };

  const lines = [`Welcome back. You were away ${humanize(seconds)}. `
                 + `It is now ${localTimeLine(now, tzName)}.`];
  if (moved.length) {
    lines.push("Code you touched has changed since:");
    for (const change of moved.slice(0, MAX_FILES)) lines.push(movedLine(change, now));
    const extra = moved.length - MAX_FILES;
    if (extra > 0) lines.push(`(+${plural(extra, "more file")} you touched also changed)`);
  }
  const tail = restLine(rest);
  if (tail) lines.push(tail);
  // Only sayable because `changes` was a real (empty) collection. With `null` the collector never ran and
  // this reassurance would be a fabrication wearing the costume of good news.
  else if (looked && !moved.length) lines.push("Nothing you track changed while you were away.");
  return { show: true, reason: "", seconds, lines, text: lines.join("\n"), moved };
}

// --- local state ------------------------------------------------------------------------------------

/** Where the last-session record lives. One file, keyed by working directory, so a customer working in five
 * repos gets five independent gaps and never hears about the wrong one. */
function statePath() {
  return path.join(os.homedir(), ".estelle", "last-session.json");
}

function readState() {
  try { return JSON.parse(fs.readFileSync(statePath(), "utf8")) || {}; } catch { return {}; }
}

/** The record for one repo, or null when this is the first session here (→ silence, never an invented gap). */
function lastSession(cwd) {
  const entry = readState()[String(cwd || "")];
  return entry && typeof entry === "object" ? entry : null;
}

/** Record where this session ended: when, at which commit, and which files it wrote. Best-effort and
 * silent on failure — a state write that fails costs the NEXT welcome, never this session. */
function recordSession(cwd, files, now) {
  const dir = String(cwd || "");
  if (!dir) return false;
  const entry = { at: now || new Date().toISOString(), head: gitHead(dir) || "",
                  files: (files || []).slice(0, MAX_TRACKED_FILES) };
  try {
    fs.mkdirSync(path.dirname(statePath()), { recursive: true });
    // 0600, NOT THE DEFAULT. A session file is the single most likely place for a repo path, a task
    // string or a token to end up next, and "zero key-shaped tokens today" is exactly the reasoning
    // E-030 exists to defeat — the third instance always lands where nobody hardened. Every mode under
    // ~/.estelle is now a decision someone made rather than a default nobody looked at.
    fs.writeFileSync(statePath(), JSON.stringify({ ...readState(), [dir]: entry }),
                     { encoding: "utf8", mode: 0o600 });
    return true;
  } catch { return false; }
}

// --- git evidence -----------------------------------------------------------------------------------

/** One bounded git read. null on ANY failure — not a git repo, no git installed, a timeout, a detached or
 * rewritten history. Every one of those means "I cannot look", and the brief then says nothing about the
 * repo rather than something plausible about it. */
function git(args, cwd) {
  try {
    return execFileSync("git", args, { cwd, encoding: "utf8", timeout: GIT_TIMEOUT_MS,
      maxBuffer: 4 * 1024 * 1024, stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch { return null; }
}

function gitHead(cwd) {
  return git(["rev-parse", "HEAD"], cwd);
}

// Control characters as separators, because a commit subject may contain anything a human can type -- a
// pipe, a tab, a newline. \x01 starts a record and \x1f separates its fields; neither survives a keyboard.
const RECORD = "\x01";
const FIELD = "\x1f";

/** `git log <since>..HEAD` as Change records — one per (commit, file), newest first.
 *
 * The author, the commit time and the subject all come straight out of git. Nothing is inferred: this is
 * the record, reformatted. */
function parseLog(text) {
  const out = [];
  for (const block of String(text || "").split(RECORD)) {
    if (!block.trim()) continue;
    const [header, ...rest] = block.split("\n");
    const [, actor, at, subject] = header.split(FIELD);
    if (!at) continue;                                   // no timestamp → not evidence (dropped, not narrated)
    for (const file of rest) {
      if (file.trim()) out.push({ at, actor: actor || "", path: file.trim(), what: subject || "" });
    }
  }
  return out;
}

/** What changed in `cwd` since commit `sinceHead` — or null when it cannot be determined.
 *
 * null is returned when there is no recorded commit, when git will not answer, or when the recorded commit
 * is no longer an ancestor of HEAD (a rebase, a reset, a different branch). That last case is the subtle
 * one: `A..B` across a rewritten history quietly returns a WRONG set rather than an error, so it is checked
 * up front and refused rather than reported. */
function collectChanges(cwd, sinceHead) {
  if (!clean(sinceHead)) return null;
  const head = gitHead(cwd);
  if (!head) return null;
  if (head === sinceHead.trim()) return [];              // looked, and nothing has been committed since
  if (git(["merge-base", "--is-ancestor", sinceHead.trim(), "HEAD"], cwd) === null) return null;
  const log = git(["log", `${sinceHead.trim()}..HEAD`, "-n", String(MAX_COMMITS), "--name-only",
                   `--pretty=format:${RECORD}%H${FIELD}%an${FIELD}%aI${FIELD}%s`], cwd);
  return log === null ? null : parseLog(log);
}

// --- the SessionStart entry point --------------------------------------------------------------------

/** The whole returning-customer decision for one repo, from local evidence only. Never throws. */
function welcome(cwd, now, deps = {}) {
  const load = deps.lastSession || lastSession;
  const collect = deps.collectChanges || collectChanges;
  const zoneOf = deps.machineZone || machineZone;
  let previous = null;
  try { previous = load(cwd); } catch { previous = null; }
  if (!previous) return brief({ now, lastSeen: "" });     // first session in this repo → silence
  let changes = null;
  try { changes = collect(cwd, previous.head); } catch { changes = null; }
  return brief({ now, lastSeen: previous.at, myFiles: previous.files || [], changes, tzName: zoneOf() });
}

module.exports = {
  MIN_GAP_SECONDS, NEWS_FREE_GAP_SECONDS, MAX_FILES, MAX_ACTORS, MAX_WHAT, MAX_TRACKED_FILES,
  UNKNOWN_ZONE_NOTE,
  SILENT_DISABLED, SILENT_FIRST_SESSION, SILENT_UNKNOWN, SILENT_SAME_SESSION, SILENT_NO_NEWS,
  humanize, secondsBetween, machineZone, knownZone, localTimeLine,
  changesSince, movedFiles, actorsPhrase, restLine, brief,
  statePath, lastSession, recordSession, git, gitHead, parseLog, collectChanges, welcome,
};
