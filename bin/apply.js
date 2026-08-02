"use strict";
// THE MISSING PRIMITIVE — writing a returned diff to the working tree.
//
// `/work` planned, implemented, gated and repaired a change, rendered the diff… and dropped it. Nothing in
// this CLI has ever written code to disk, which is why a Codex-style "accept edits" mode would have been a
// switch wired to nothing: there was literally nothing to accept. This file is the thing to accept.
//
// A diff comes back OVER THE WIRE, so it is attacker-reachable in exactly the way ingested content is, and
// every guard here is written for that reader rather than for a friendly one:
//
//   * CONTAINMENT — every path a patch touches must resolve inside the repo root. The same rule `reindex`
//     and the PostToolUse sync hook already enforce, borrowed from estelle.js rather than re-derived, so
//     the two cannot drift into two different ideas of "inside".
//   * NO CLOBBER — a target with uncommitted work is refused BY NAME. Overwriting someone's unsaved change
//     to save them a keystroke is the worst trade this CLI could make.
//   * THE CEILING — the decision to write is min(local, server) and nothing else. A read_only account can
//     never auto-apply, whatever the client believes (see the long note in session-commands.js).
//   * REVERSIBLE — every apply snapshots what it overwrites first. An irreversible auto-apply is not
//     something to ship.
//
// git does the actual patching. `git apply` is atomic (it verifies every hunk before writing any of them),
// and a hand-rolled hunk matcher that got fuzz-matching subtly wrong would corrupt source files silently —
// the one failure mode this product cannot have.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const local = require("./session-commands.js");

// ONE containment rule, not two. estelle.js owns it (`repoRelative`) because the sweep and reindex key the
// graph by it; a second implementation here would be a second opinion about what escapes a repo, and the
// disagreement would be silent. Required lazily — estelle.js is the entrypoint and pulling it in at module
// load would make the require graph depend on load ORDER.
const repoRelative = (root, p) => require("./estelle.js").repoRelative(root, p);

// ── reading the patch ───────────────────────────────────────────────────────────

/** One header path (`a/src/x.py`, `b/src/x.py`, `/dev/null`) as a repo path, or "" for /dev/null.
 *
 * Strips git's `a/`/`b/` prefix (the -p1 the patch is applied with) and any trailing tab-timestamp that
 * `diff -u` appends. Deliberately does NOT unquote a C-quoted path: a path git had to quote is one with a
 * newline, a quote or a non-ASCII byte in it, and guessing at the unquoting is how a containment check gets
 * fooled. Such a patch keeps its literal quoted form and is refused by the containment check below. */
function headerPath(raw) {
  const text = String(raw || "").split("\t")[0].trim();
  if (!text || text === "/dev/null") return "";
  return text.replace(/^[ab]\//, "");
}

/** Every `(old, new)` pair a unified diff declares. The parse the safety checks and the flow both read. */
function parseHeaders(diff) {
  const pairs = [];
  let old = null;
  for (const line of String(diff || "").split("\n")) {
    if (line.startsWith("--- ")) { old = line.slice(4); continue; }
    if (line.startsWith("+++ ") && old !== null) {
      pairs.push({ old: headerPath(old), new: headerPath(line.slice(4)) });
      old = null;
    }
  }
  return pairs;
}

/** The files a patch touches and what happens to each: `add` · `delete` · `modify`.
 *
 * The path that matters is the one WRITTEN — the `+++` side — except on a delete, where there isn't one. */
function patchTargets(diff) {
  return parseHeaders(diff).map(({ old, new: nu }) => (
    !old ? { path: nu, kind: "add" }
      : !nu ? { path: old, kind: "delete" }
        : { path: nu, kind: "modify" }
  ));
}

/** Why this patch must not be applied to ``root``, or "" when it may be. Fail-closed: anything that does
 * not parse into at least one in-repo target is refused, never applied as a no-op and reported as done. */
function unsafePatch(root, diff) {
  const pairs = parseHeaders(diff);
  if (!pairs.length) return "no file headers in this diff — there is nothing to apply";
  for (const pair of pairs) {
    for (const p of [pair.old, pair.new]) {
      if (p && !repoRelative(root, p)) return `refusing to write outside the repo root: ${p}`;
    }
  }
  return "";
}

// ── the clobber refusal ─────────────────────────────────────────────────────────

/** The target paths that already carry uncommitted work, read from `git status --porcelain`.
 *
 * ANY porcelain status counts — modified, staged, and untracked alike. A staged edit is uncommitted work,
 * and an untracked file that a patch would CREATE is someone's scratch file about to be overwritten. */
function conflicts(targets, porcelain) {
  const dirty = new Set();
  for (const line of String(porcelain || "").split("\n")) {
    if (line.length < 4) continue;
    const body = line.slice(3).trim();
    // A rename is reported as `old -> new`; the new name is the one on disk.
    const named = body.includes(" -> ") ? body.split(" -> ").pop() : body;
    dirty.add(named.replace(/^"|"$/g, ""));
  }
  return (targets || []).map((t) => t.path).filter((p) => dirty.has(p));
}

// ── the ceiling ─────────────────────────────────────────────────────────────────

/** May this apply happen, and does a human have to say so? `refuse` · `confirm` · `auto`.
 *
 * The rung mapping follows `serve/autonomy.py` rather than inventing a second ladder:
 *
 *   read_only   writes NOTHING — refuse, wherever the read_only came from;
 *   propose     the ADR 0012 default: a change a human reviews. So it applies, but always after a y/N;
 *   branch+     `requires_human_ack` is already true at this rung — the customer has signed for Estelle
 *               writing without per-change review — so it may apply unattended.
 *
 * The one asymmetry, and it is deliberate: an UNVERIFIED dial degrades to `confirm`, never to `auto`. For a
 * server call the server is the enforcement point, so `workRefusal` lets an unknown dial through; for a
 * write to YOUR DISK there is no server in the path at all, so this client is the enforcement point and
 * "I could not check" cannot mean "you may". The human typing y is the trusted trigger (autonomy.py
 * invariant I1); silence is not. */
function applyDecision(localMode, serverMode) {
  const known = local.modeRank(serverMode) >= 0;
  if (local.modeRank(localMode) < local.modeRank("propose")) {
    return { decision: "refuse", why: `mode is ${localMode} — nothing is written. shift+tab to raise it.` };
  }
  if (known && local.modeRank(serverMode) < local.modeRank("propose")) {
    return { decision: "refuse",
             why: `your account's autonomy dial is ${serverMode} — Estelle will not write. Raise it in the dashboard.` };
  }
  if (!known) {
    return { decision: "confirm", why: "your account's dial is unverified — confirming by hand (fail-closed)" };
  }
  const effective = local.effectiveMode(localMode, serverMode);
  if (local.modeRank(effective) >= local.modeRank("branch")) return { decision: "auto", why: effective };
  return { decision: "confirm", why: "propose — a human reviews every change (ADR 0012 default)" };
}

// ── reversibility ───────────────────────────────────────────────────────────────

/** Where undo records live for ``root``.
 *
 * Inside `.git/` on purpose: git never reports it, so a snapshot cannot show up as a dirty file, dirty the
 * next gate, or get committed by accident. A linked worktree has `.git` as a FILE, so that case falls back
 * to the user's home. */
function undoBase(root) {
  const dotgit = path.join(root, ".git");
  try {
    if (fs.statSync(dotgit).isDirectory()) return path.join(dotgit, "estelle-undo");
  } catch { /* not a git dir — fall through */ }
  return path.join(os.homedir(), ".estelle", "undo");
}

/** `20260730-143355-a1b2` — sorts chronologically as a plain string, which is what `latestUndo` relies on. */
function stamp(now) {
  const d = new Date(now || Date.now());
  const p = (n, w = 2) => String(n).padStart(w, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-`
       + `${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}-`
       + Math.random().toString(36).slice(2, 6);
}

/** Snapshot every file a patch is about to touch. A file that does NOT exist yet is recorded as absent, so
 * undoing a creation removes it — restoring "absent" is still a restore. */
function snapshot(root, targets, base, now) {
  const dir = path.join(base, stamp(now));
  fs.mkdirSync(path.join(dir, "files"), { recursive: true });
  const entries = (targets || []).map((t, i) => {
    const abs = path.join(root, t.path);
    let body = null;
    try { body = fs.readFileSync(abs); } catch { body = null; }
    const backup = body === null ? "" : `files/${i}`;
    if (backup) fs.writeFileSync(path.join(dir, backup), body, { mode: 0o600 });
    return { path: t.path, existed: body !== null, backup };
  });
  const record = { at: new Date(now || Date.now()).toISOString(), root, dir, entries };
  // 0600: an undo backup is a verbatim copy of the customer's source file.
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(record, null, 2) + "\n",
                   { mode: 0o600 });
  return record;
}

/** Put a snapshot back. Reports what it could not restore rather than claiming a clean undo. */
function restoreUndo(record) {
  const r = record || {};
  const restored = [], errors = [];
  for (const e of r.entries || []) {
    const abs = path.join(r.root, e.path);
    try {
      if (e.existed) {
        fs.mkdirSync(path.dirname(abs), { recursive: true });
        fs.writeFileSync(abs, fs.readFileSync(path.join(r.dir, e.backup)));
      } else if (fs.existsSync(abs)) {
        fs.unlinkSync(abs);
      }
      restored.push(e.path);
    } catch (err) {
      errors.push(`${e.path}: ${String((err && err.message) || err)}`);
    }
  }
  return { restored, errors };
}

/** The newest undo directory under ``base``, or "" when there is none. */
function latestUndo(base) {
  let names;
  try { names = fs.readdirSync(base); } catch { return ""; }
  const dirs = names
    .filter((n) => { try { return fs.statSync(path.join(base, n, "manifest.json")).isFile(); } catch { return false; } })
    .sort();
  return dirs.length ? path.join(base, dirs[dirs.length - 1]) : "";
}

/** Read a manifest back off disk — what `/undo` reaches for on a later run. */
function readUndo(dir) {
  try { return JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8")); } catch { return null; }
}

// ── applying ────────────────────────────────────────────────────────────────────

/** The git top-level for ``cwd``, or "".
 *
 * A diff's paths are keyed to the repo ROOT, but `git apply` resolves them against the directory it runs
 * in — so applying from a subdirectory without this either refuses or writes to the wrong place. It is
 * also the root the containment check must be measured against, for the same reason. */
function repoRoot(cwd) {
  try {
    return execFileSync("git", ["rev-parse", "--show-toplevel"],
                        { cwd: cwd || process.cwd(), encoding: "utf8" }).trim();
  } catch { return ""; }
}

/** Run one git command in ``root``, with ``input`` on stdin. Throws on a non-zero exit, by design. */
function gitRunner(root) {
  return (args, input) => execFileSync("git", args, {
    cwd: root, encoding: "utf8", input, maxBuffer: 64 * 1024 * 1024,
  });
}

/** Apply ``diff`` to the tree at ``root``. `{ok, applied, undo, error}` — never throws, never half-writes.
 *
 * `--check` first so a patch that does not fit is rejected before a single byte is snapshotted or written;
 * `git apply` then re-verifies and writes all-or-nothing. `--whitespace=nowarn` keeps a trailing-space
 * complaint from failing a change that is otherwise exactly right. */
function applyPatch(diff, deps) {
  const d = deps || {};
  const root = d.root || process.cwd();
  const git = d.exec || gitRunner(root);
  const targets = patchTargets(diff);
  const body = String(diff || "").replace(/\n?$/, "\n");        // git rejects a patch with no final newline
  try {
    git(["apply", "--check", "--whitespace=nowarn", "-"], body);
  } catch (e) {
    return { ok: false, applied: [], undo: null, error: errorText(e) };
  }
  const undo = snapshot(root, targets, d.undoRoot || undoBase(root), d.now);
  try {
    git(["apply", "--whitespace=nowarn", "-"], body);
  } catch (e) {
    return { ok: false, applied: [], undo, error: errorText(e) };
  }
  return { ok: true, applied: targets.map((t) => t.path), undo, error: "" };
}

/** git's own complaint, which is the only useful message here — never a bare "failed". */
function errorText(e) {
  const parts = [e && e.stderr, e && e.stdout, e && e.message].map((p) => String(p || "").trim());
  return parts.find(Boolean) || "git apply failed";
}

// ── the flow a human sees ───────────────────────────────────────────────────────

/** Show the diff, decide, and write. Returns an exit code — every refusal is NON-ZERO, because "I did not
 * do it" reported as success is how a caller ships a change that was never applied. */
async function runApply(diff, deps) {
  const { out, c } = deps;
  const root = deps.root || repoRoot(deps.cwd) || deps.cwd || process.cwd();
  const text = String(diff || "").trim();
  if (!text) { out(`  ${c.amber("!")} ${c.dim("no diff to apply.")}`); return 1; }

  const unsafe = unsafePatch(root, diff);
  if (unsafe) { out(`  ${c.red("✗ " + unsafe)}`); return 1; }

  const { decision, why } = applyDecision(deps.localMode, deps.serverMode);
  if (decision === "refuse") { out(`  ${c.amber("!")} ${why}`); return 1; }

  // The diff is the receipt — it is shown whether or not a question follows, so an auto-apply is still
  // something you can read afterwards rather than a change that appeared silently.
  out(require("./repl.js").renderDiff(diff, c));               // lazily required: repl.js requires THIS file

  const targets = patchTargets(diff);
  const git = deps.exec || gitRunner(root);
  let porcelain = "";
  try { porcelain = git(["status", "--porcelain"]); } catch (e) {
    out(`  ${c.red("✗ could not read git status — refusing to write blind.")} ${c.dim(errorText(e))}`);
    return 1;
  }
  const clash = conflicts(targets, porcelain);
  if (clash.length) {
    out(`  ${c.red("✗ uncommitted changes in:")} ${clash.join(", ")}`);
    out(`  ${c.dim("  commit or stash them first — Estelle will not overwrite work you have not saved.")}`);
    return 1;
  }

  if (decision === "confirm") {
    const answer = await deps.prompt(`  ${c.amber("apply this?")} ${c.dim("y/N")} `);
    if (!/^y(es)?$/i.test(String(answer || "").trim())) { out(`  ${c.dim("↩ not applied.")}`); return 1; }
  }

  const r = applyPatch(diff, { root, undoRoot: deps.undoRoot, exec: deps.exec, now: deps.now });
  if (!r.ok) { out(`  ${c.red("✗ apply failed —")} ${c.dim(r.error)}`); return 1; }
  out(`  ${c.green("✓ applied")} ${c.dim(r.applied.join(" · "))}`
    + (decision === "auto" ? c.dim(`  (auto — ${why})`) : ""));
  out(`  ${c.dim("  /undo puts it back")}`);
  return 0;
}

/** Put the most recent apply back. Reports WHY it could not rather than shrugging — an undo that quietly
 * does nothing is worse than no undo, because the user walks away believing the tree is clean. */
function undoLast(root) {
  const base = undoBase(root);
  const dir = latestUndo(base);
  if (!dir) return { ok: false, why: "nothing to undo — no apply has been made from this repo" };
  const record = readUndo(dir);
  if (!record) return { ok: false, why: `the undo record at ${dir} is unreadable` };
  const r = restoreUndo(record);
  if (r.errors.length) return { ok: false, why: r.errors.join(" · "), restored: r.restored, at: record.at };
  // Spent records are removed, so a second /undo does not silently "succeed" by re-restoring the same
  // snapshot over work the user has done since.
  try { fs.rmSync(dir, { recursive: true, force: true }); } catch { /* the restore already happened */ }
  return { ok: true, why: "", restored: r.restored, at: record.at };
}

module.exports = {
  headerPath, parseHeaders, patchTargets, unsafePatch, conflicts, applyDecision,
  undoBase, snapshot, restoreUndo, latestUndo, readUndo, applyPatch, runApply,
  repoRoot, undoLast,
};
