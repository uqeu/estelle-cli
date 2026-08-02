"use strict";
// WHAT THE CUSTOMER ACTUALLY SEES — the render half of register #93.
//
// 🔴 THE DEFECT, measured on prod against `api.fatelabs.ca` on 2026-08-02, not inferred from the source.
// The session's renderer drew a reply from `answer` (plus a diff, a gate verdict, a PR url and a source
// list). SIX routed commands return none of those fields:
//
//     /init      GET  /wiki      -> {wiki, repo, scope}                        rendered NOTHING
//     /sessions  GET  /sessions  -> {sessions, count}                          rendered NOTHING
//     /resume    GET  /session   -> {id, title, runs, artifacts, …}            rendered NOTHING
//     /scan      POST /scan      -> {findings, count}                          rendered NOTHING
//     /improve   POST /improve   -> {proposals}                                rendered NOTHING
//     /verify    POST /verify    -> {grounded, scope_ask, …8 buckets}          rendered NOTHING
//
// So the founder typed `/sessions`, the spinner said "running /sessions", a real round-trip was spent, and
// the screen printed a blank line. "It said running /sessions and it disappeared" — that is this, and it
// is why the command felt like it fell through to a model: an answer you cannot see is indistinguishable
// from an answer that never came.
//
// It is #84 REPEATED. `/routing` was wired and "opened onto BLANK LINES" because `POST /route`'s shape had
// no renderer; that was fixed for `/route`'s shape ALONE. Per E-030 — when a fix touches a seam, enumerate
// every consumer of it — the other six were never looked at. This module is that enumeration.
//
// ⛔ THE STRUCTURAL RULE, which is worth more than the six renderers below:
//
//        A REPLY THAT RENDERS TO ZERO LINES IS A DEFECT, AND THE RENDERER MUST SAY SO.
//
// `describe` is the floor. When no renderer claims a reply, it prints the fields the server actually sent
// rather than printing nothing — so the next unrendered shape shows up as a visible, reportable "this
// build does not know how to draw this" instead of as silence. Blank output stops being possible, which
// is the only version of this fix that survives the next endpoint we add.
//
// Pure functions, `c` injected, no I/O — the seam into repl.js is exercised by name (E-027).

/** ISO → "2h ago" / "3d ago". Empty for anything unparseable, never "Invalid Date". */
function ago(iso, now) {
  const then = Date.parse(iso || "");
  if (Number.isNaN(then)) return "";
  const diff = Math.max(0, ((now || Date.now()) - then) / 1000);
  if (diff < 90) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  return `${Math.round(diff / 86400)}d ago`;
}

/** Indent a block of server prose to the session's two-space gutter. */
function block(text, c) {
  return String(text || "").replace(/\s+$/, "").split("\n").map((l) => `  ${c.dim(l)}`);
}

/** `GET /sessions` -> `{sessions: [{id, title, started_at, run_count, …}], count}`. */
function sessionsLines(res, c, now) {
  const rows = Array.isArray(res.sessions) ? res.sessions : [];
  if (!rows.length) {
    return [`  ${c.dim("No sessions yet — this one is the first.")}`];
  }
  const lines = [`  ${c.bold(`${rows.length} of ${res.count ?? rows.length} sessions`)}${c.dim("  · /resume <id> to pick one up")}`];
  for (const s of rows.slice(0, 10)) {
    const when = ago(s.started_at || s.at || s.ended_at, now);
    const title = String(s.title || "").trim() || c.dim("(untitled)");
    const runs = s.run_count ? c.dim(` · ${s.run_count} run${s.run_count === 1 ? "" : "s"}`) : "";
    lines.push(`  ${c.teal(String(s.id || "?"))}  ${title}`);
    lines.push(`  ${" ".repeat(String(s.id || "?").length)}  ${c.dim(when)}${runs}`);
  }
  if (rows.length > 10) lines.push(`  ${c.dim(`… ${rows.length - 10} more`)}`);
  return lines;
}

/** `GET /session?id=` -> one session's brief. */
function sessionLines(res, c, now) {
  const lines = [`  ${c.bold(String(res.title || "(untitled session)"))}  ${c.dim(String(res.id || ""))}`];
  const when = ago(res.started_at, now);
  const facts = [
    when ? `started ${when}` : "",
    res.run_count ? `${res.run_count} run${res.run_count === 1 ? "" : "s"}` : "",
    res.skill_count ? `${res.skill_count} skill${res.skill_count === 1 ? "" : "s"}` : "",
    (res.repos || []).length ? `repos: ${res.repos.join(", ")}` : "",
  ].filter(Boolean);
  if (facts.length) lines.push(`  ${c.dim(facts.join(" · "))}`);
  if (String(res.meaning || "").trim()) lines.push(...block(res.meaning, c));
  for (const r of (res.runs || []).slice(0, 8)) {
    lines.push(`  ${c.dim("·")} ${String((r && (r.title || r.task || r.kind)) || "run")}`);
  }
  return lines;
}

/** `GET /wiki` -> `{wiki, repo, scope}`. `wiki` is markdown the server already composed. */
function wikiLines(res, c) {
  const text = String(res.wiki || "").trim();
  if (!text) {
    return [`  ${c.amber("!")} ${c.dim(`no repo brief yet for ${res.repo || "this repo"} — run `)}${c.teal("estelle sweep")}${c.dim(" first.")}`];
  }
  return [`  ${c.bold(String(res.repo || "repo"))}${c.dim(res.scope ? `  · ${res.scope}` : "")}`, "", ...block(text, c)];
}

/** `POST /scan` -> `{findings: [{path, line, severity, body}], count}`. */
function scanLines(res, c) {
  const rows = Array.isArray(res.findings) ? res.findings : [];
  if (!rows.length) return [`  ${c.green("✓ scan clean")}${c.dim("  no secrets, no SAST hits, no known CVEs in this diff.")}`];
  const lines = [`  ${c.red(`✗ ${res.count ?? rows.length} finding${(res.count ?? rows.length) === 1 ? "" : "s"}`)}`];
  for (const f of rows.slice(0, 20)) {
    const where = `${f.path || "?"}${f.line ? `:${f.line}` : ""}`;
    const sev = String(f.severity || "").toLowerCase();
    const mark = sev === "error" || sev === "high" || sev === "critical" ? c.red("✗") : c.amber("!");
    lines.push(`  ${mark} ${c.teal(where)}  ${String(f.body || f.title || "").trim()}`);
  }
  if (rows.length > 20) lines.push(`  ${c.dim(`… ${rows.length - 20} more`)}`);
  return lines;
}

/** `POST /improve` -> `{proposals: [{title, file, line, severity, why, suggested_action, verdict}]}`. */
function improveLines(res, c) {
  const rows = Array.isArray(res.proposals) ? res.proposals : [];
  if (!rows.length) return [`  ${c.dim("No ranked improvements came back for this repo.")}`];
  const lines = [`  ${c.bold(`${rows.length} ranked improvement${rows.length === 1 ? "" : "s"}`)}`];
  for (const p of rows.slice(0, 10)) {
    const where = p.file ? `${p.file}${p.line ? `:${p.line}` : ""}` : "";
    const sev = String(p.severity || "").toLowerCase();
    const mark = sev === "high" || sev === "critical" ? c.red("✗") : c.amber("!");
    lines.push(`  ${mark} ${String(p.title || "").trim()}`);
    if (where) lines.push(`    ${c.teal(where)}${c.dim(p.category ? `  · ${p.category}` : "")}`);
    if (p.suggested_action) lines.push(`    ${c.dim(String(p.suggested_action))}`);
  }
  if (rows.length > 10) lines.push(`  ${c.dim(`… ${rows.length - 10} more`)}`);
  return lines;
}

/** `POST /orchestra` -> `{level, count, runs: [AgentRun]}`. */
function orchestraLines(res, c) {
  const rows = Array.isArray(res.runs) ? res.runs : [];
  const lines = [`  ${c.bold(`${res.count ?? rows.length} agent${(res.count ?? rows.length) === 1 ? "" : "s"}`)}${c.dim(res.level ? `  · at ${res.level}` : "")}`];
  for (const r of rows.slice(0, 12)) {
    const head = String((r && (r.task || r.subtask || r.title)) || "task").trim();
    lines.push(`  ${c.teal("·")} ${head}`);
    const bits = [r && r.model, r && r.tier, r && r.effort].filter(Boolean).join(" · ");
    if (bits) lines.push(`    ${c.dim(bits)}`);
    if (r && r.grounded === false) lines.push(`    ${c.amber("· not grounded")}${c.dim(r.reason ? `  ${r.reason}` : "")}`);
  }
  return lines;
}

/**
 * THE FLOOR — what happens when nothing above claimed the reply.
 *
 * It prints the field NAMES the server sent, and says plainly that this build has no renderer for them.
 * That is not a nicety: the six blank commands above all shipped because "no renderer" and "nothing to
 * say" produced byte-identical output, so nobody could tell them apart on screen. Naming the fields makes
 * the next one a bug report instead of a silence — defect class 3 (unknown rendered as OK), closed at the
 * one place every reply passes through.
 */
function describe(res, c) {
  const keys = Object.keys(res || {}).filter((k) => k !== "usage" && k !== "meta");
  if (!keys.length) return [`  ${c.amber("!")} ${c.dim("the server replied with an empty body — nothing to show.")}`];
  return [
    `  ${c.amber("!")} ${c.dim("this build has no renderer for that reply — please report it.")}`,
    `  ${c.dim("fields returned: ")}${c.teal(keys.slice(0, 12).join(", "))}${c.dim(keys.length > 12 ? ", …" : "")}`,
  ];
}

/** True when the reply carries one of the shapes `renderAnswer` already draws itself. */
function hasOwnRenderer(res) {
  const r = res || {};
  return Boolean(String(r.answer || "").trim()) || Boolean(r.diff)
    || r.merge !== undefined || r.verdict !== undefined || r.gate !== undefined
    || (r.provider !== undefined && r.routed !== undefined);
}

/** The renderer for a routed command, or null when `renderAnswer`'s own branches cover it.
 *
 * Keyed by COMMAND rather than by a guessed shape: two endpoints may both return `{count}` and mean
 * entirely different things, and sniffing for a field is how a session list would render as a scan. */
const BY_COMMAND = {
  sessions: sessionsLines,
  resume: sessionLines,
  init: wikiLines,
  scan: scanLines,
  improve: improveLines,
  orchestra: orchestraLines,
};

function linesFor(name, res, c, now) {
  const fn = BY_COMMAND[String(name || "")];
  if (!fn) return null;
  return fn(res || {}, c, now);
}

module.exports = {
  ago, block, sessionsLines, sessionLines, wikiLines, scanLines, improveLines, orchestraLines,
  describe, hasOwnRenderer, linesFor, BY_COMMAND,
};
