"use strict";
// The session's LOCAL half — what happens in the terminal rather than on the server.
//
// repl.js is the conversation: every line it handles becomes an HTTP call to an endpoint Estelle already
// serves. Three things do NOT fit that shape, and they live here so repl.js stays a router:
//
//   `!<cmd>`   a shell command run on the user's own machine — no server is involved at all;
//   /mode      the autonomy CEILING, which is a SERVER fact the CLI reflects (see the long note below);
//   the diff   /gate and /scan grade a unified diff, and only the client can compute one from git.
//
// Split out rather than grown into estelle.js (already ~1050 lines, over the repo's <800 rule) and rather
// than grown into repl.js (which would then own both the network and the shell). Every decision is a pure
// function with the I/O injected, so the rules are testable without a shell, a repo or a socket.

const { execFileSync, spawnSync } = require("child_process");

// ONE danger heuristic, not two. hook.js already decides what a catastrophic command looks like (rm -rf of
// a critical path, fork bombs, curl-pipe-to-shell, force-push to main) and `estelle hook guard` fires it on
// every Bash call Claude Code makes. Re-implementing it here would drift within a release, and the drift
// would be SILENT — the same command guarded in one door and waved through in the other.
const { dangerousCommand } = require("./hook.js");

// ── `!` shell passthrough ───────────────────────────────────────────────────────

/** The command in a `!`-prefixed line, or null when the line is not one.
 *
 * Only a LEADING bang counts: "wow! that worked" is a sentence for Estelle, not a request to run `that
 * worked`. A bare `!` is nothing, not an empty command — spawning a shell on it would be a confusing no-op. */
function parseBang(line) {
  const t = String(line || "").trim();
  if (!t.startsWith("!")) return null;
  const command = t.slice(1).trim();
  return command ? { command } : null;
}

/** Run one shell command and show what it said. Returns its exit code — an error is NEVER reported as a
 * success, here or anywhere else in this CLI.
 *
 * ADVISORY, like the hook: a dangerous-looking command is explained and CONFIRMED, not blocked. A false
 * positive that hard-blocks real work is its own damage, and a guard people cannot get past is a guard they
 * route around. Silence on ordinary commands is the other half of that bargain.
 *
 * `shell: true` is the feature, not an oversight — the user typed a shell command into their own terminal
 * and expects pipes and globs to work. The string comes from that line and from nothing else; no ingested
 * content, no server reply, and no model output ever reaches it. */
async function runShell(command, deps) {
  const { out, c, prompt } = deps;
  const spawn = deps.spawn || ((cmd) => spawnSync(cmd, { shell: true, encoding: "utf8",
                                                        maxBuffer: 16 * 1024 * 1024 }));
  const reason = dangerousCommand(command);
  if (reason) {
    out(`  ${c.red("⛔")} this looks like ${reason}.`);
    const answer = await prompt(`  ${c.amber("run it anyway?")} ${c.dim("y/N")} `);
    if (!/^y(es)?$/i.test(String(answer || "").trim())) {
      out(`  ${c.dim("↩ not run.")}`);
      return 1;                                   // a refusal is a non-zero outcome, not a quiet pass
    }
  }
  const r = spawn(command);
  if (r && r.error) {
    out(`  ${c.red("✗")} ${String(r.error.message || r.error)}`);
    return 1;
  }
  const body = `${(r && r.stdout) || ""}${(r && r.stderr) || ""}`.replace(/\n$/, "");
  if (body) out(body.split("\n").map((l) => `  ${l}`).join("\n"));
  const code = r && r.status != null ? r.status : 0;
  if (code !== 0) out(`  ${c.red(`✗ exit ${code}`)}`);
  return code;
}

// ── modes ───────────────────────────────────────────────────────────────────────
//
// WHY THIS IS NOT CLAUDE CODE'S MODE SWITCH, and must not become it.
//
// In Claude Code the mode IS the permission: the client decides, locally, whether an edit needs a prompt.
// Estelle's permission does not live in the client. It lives in `serve/autonomy.py` as a per-account,
// fail-closed, audited ceiling (READ_ONLY → PROPOSE → BRANCH → EXECUTE, default READ_ONLY), and ADR 0012
// makes raising it a consented, signed, kill-switchable act. The CLI is one client among several
// (dashboard, MCP, Slack, the GitHub App); a client-side "auto-accept" toggle would be either a LIE (the
// server refuses anyway) or a genuine bypass of the one contract the product's safety claim rests on.
//
// So the mode here is a CEILING, never a grant. Two rules make that true:
//
//   1. it can only ever LOWER what happens. The effective ceiling is min(local, server). Typing
//      `/mode execute` on a read_only account does not get you execute — it gets you told;
//   2. it is displayed AGAINST the server's answer (GET /autonomy/scope). When that call fails, the
//      ceiling is UNKNOWN and we fall back to read_only — the same fail-closed rule the gate follows,
//      because "I could not check" is not "you may."
//
// The vocabulary is deliberately the SERVER's four names, so the CLI and the product contract cannot drift
// into two ladders. `plan` and `auto` are aliases onto it, so the founder's words still work.

// The upgrade step, written ONCE. The footer (mode-ui.js) and `/mode` both read it, so a customer can
// never be sent to two different places to raise the same dial.
const SETTINGS_URL = "fatelabs.ca/dashboard/settings";

const MODES = ["read_only", "propose", "branch", "execute"];

const MODE_ALIASES = {
  // `read` and `plan` are the founder's words for the same RUNG: neither writes anything, so they
  // differ in ROUTING (cheap+fast vs reasoning-heavy), never in privilege. A routing distinction that
  // cannot move a privilege is safe by construction; two ladders would not be.
  read: "read_only", plan: "read_only", read_only: "read_only", "read-only": "read_only",
  // `edit` is what `propose` DISPLAYS as, so it must parse — a customer typing the word on their
  // own screen being told it is not a mode is the two-vocabularies defect wearing a fix's clothes.
  edit: "propose",
  readonly: "read_only",
  propose: "propose", pr: "propose",
  branch: "branch",
  execute: "execute", auto: "execute",
};

/** The rung's name AS A HUMAN READS IT. The canonical VALUE stays the server's (`read_only`, `execute`);
 * this is only what gets printed.
 *
 * THE DEFECT: the founder DESIGNED these modes and could not tell what `read_only` meant while looking at
 * it. Codex prints "Plan mode" and says in plain words what it permits; we printed a snake_case enum
 * borrowed from a permissions ladder. A mode you cannot read is a switch you will not touch.
 *
 * THE NAMES, founder's call 2026-08-02: `read_only` shows as `plan`, `propose` as `edit`, and the other
 * two keep their words. Nobody outside this codebase knows what "propose" means as a MODE — Claude Code has
 * plan/edit, Codex has plan and defaults to auto, Kimi has plan. We had invented four words nobody
 * recognises.
 *
 * ⛔ THE LADDER ITSELF DOES NOT CHANGE, and the mapping stays 1:1. These are real privilege rungs living in
 * `serve/autonomy.py`; the CLI is a VIEW of that dial and never a second dial. Collapsing two rungs into one
 * display name would be the two-things-one-name defect in a new place, which is the defect this whole
 * session keeps finding. DISPLAY ONLY — `parseMode` resolves every alias back to the server rung, and
 * `TestTheCliIsAViewOfThisLadderAndNotASecondOne` fails if the two lists ever diverge. */
const MODE_NAME = {
  read_only: "plan",
  propose: "edit",
  branch: "branch",
  execute: "auto",
};

/** The human name for a rung, falling back to the rung itself so an unknown value is never hidden. */
function modeName(level) {
  return MODE_NAME[String(level || "")] || String(level || "");
}

/** What each rung actually permits, in the product's own terms — /mode is useless if it prints a word. */
const MODE_WHAT = {
  read_only: "reads, answers, verifies — nothing is written",
  propose: "writes to a sandbox and opens a PR a human merges",
  branch: "pushes a branch",
  execute: "merges when every guard passes, else degrades to a PR",
};

/** A typed mode name → the canonical level, or "" when it is not one. Never guesses: an unrecognised word
 * resolving to a level is how a typo silently grants power. */
function parseMode(text) {
  const t = String(text || "").trim().toLowerCase();
  return MODE_ALIASES[t] || "";
}

/** Position in the capability order, or -1 when the value is not a level. Mirrors `autonomy.rank`. */
function modeRank(level) {
  return MODES.indexOf(String(level || ""));
}

/** The ceiling that actually applies: the LOWER of what the user asked for and what the server grants.
 *
 * An unknown/unreachable server dial resolves to read_only, not to the local mode — the same fail-closed
 * discipline as `failClosed` in estelle.js and `allows()` server-side. A CLI that assumed "I could not
 * reach /autonomy/scope, so I'll do what you asked" would be exactly the bypass this design refuses. */
function effectiveMode(local, server) {
  const s = modeRank(server);
  if (s < 0) return "read_only";
  const l = modeRank(local);
  if (l < 0) return "read_only";
  return MODES[Math.min(l, s)];
}

/** The /mode block: what you asked for, what your account grants, and what will actually happen. The third
 * line is the only one that is true of the next command, so it is never omitted. */
function modeReport(local, server, c) {
  const known = modeRank(server) >= 0;
  const effective = effectiveMode(local, server);
  // NAMES, not enums. `/mode` is the screen a customer opens BECAUSE they are confused; printing the
  // internal ladder there is what made the founder unable to tell what his own mode meant.
  const lines = [
    `  ${c.dim("here".padEnd(10))}${modeName(local)}${c.dim("  — what this session is set to")}`,
    `  ${c.dim("account".padEnd(10))}${known ? modeName(server) : c.amber("unknown")}`
      + c.dim(known ? "  — your account's ceiling; the enforcement point"
                    : "  — could not reach Estelle; assuming read_only (fail-closed)"),
    `  ${c.dim("in force".padEnd(10))}${c.bold(modeName(effective))}${c.dim("  " + MODE_WHAT[effective])}`,
  ];
  if (known && modeRank(local) > modeRank(server)) {
    // Say it outright. A user who believes the CLI just enabled auto-merge, and finds out when it silently
    // opens a PR instead, has been misled by us — which is the failure this product exists to prevent.
    lines.push(`  ${c.amber("!")} ${c.dim(`the CLI can only LOWER the ceiling — Estelle refuses anything above ${modeName(server)}.`)}`);
  }
  // 🔴 EXPLAIN THE CLAMP AND NAME THE EXACT STEP. The founder hit a ceiling he could not move and could
  // not find the step to move it — "I could not find it, and I BUILT this." A ceiling with no named exit
  // is the same defect as the footer advertising a cycle that cannot happen: it reports a state and
  // withholds the remedy. `/mode` is where the remedy belongs, in full, every time it is clamped.
  if (known && modeRank(server) <= 0) {
    lines.push("");
    lines.push(`  ${c.dim(`Your account is at ${modeName(server)}, the lowest rung, so there is nothing to cycle to.`)}`);
    lines.push(`  ${c.dim(`To unlock ${modeName("propose")}: open ${SETTINGS_URL} → Autonomy → raise the dial.`)}`);
    lines.push(`  ${c.dim("It needs a signed, audited consent, which is why the CLI cannot do it for you.")}`);
  }
  return lines;
}

/** Why the write path is refused, or "" when the CLI has no business refusing it.
 *
 * /work is the one session command that can produce a change, so a KNOWN read_only stops it here rather
 * than spending a round-trip to be refused server-side. The asymmetry is deliberate and is the whole
 * point of the design:
 *
 *   * refuse when the LOCAL ceiling is read_only — the user asked for that;
 *   * refuse when the SERVER positively reported read_only — the account says so;
 *   * do NOT refuse when the dial is merely UNKNOWN. The enforcement point is the server, which gates
 *     /work through `autonomy.allows` regardless of what this client believes. Blocking on "I could not
 *     read the dial" would break /work against every server that does not serve /autonomy/scope, and buy
 *     no safety at all — the CLI would be guarding a door the server has already locked.
 *
 * Note this can only ever LOWER what happens. There is no branch here that lets a local mode permit
 * something the server would refuse. */
function workRefusal(local, server) {
  if (modeRank(local) >= 0 && modeRank(local) < modeRank("propose")) {
    return `mode is ${local} — the write path is off. /mode propose to allow a sandboxed diff + a PR.`;
  }
  if (modeRank(server) >= 0 && modeRank(server) < modeRank("propose")) {
    return `your account's autonomy dial is ${server} — Estelle will not write. Raise it in the dashboard.`;
  }
  return "";
}

// ── /status ─────────────────────────────────────────────────────────────────────

/** The one-screen answer to "what am I actually pointed at?" — endpoint, credential, filed repo, ceiling.
 * Every value is a fact the caller measured; nothing here is defaulted into looking configured. */
function statusRows(state) {
  const st = state || {};
  const rows = [
    ["endpoint", st.api || "(unset)"],
    ["key", st.keyMasked || "not set — paste one, or set ESTELLE_API_KEY"],
    ["repo", st.repo || "unfiled — this sweep would land in the shared namespace"],
  ];
  if (st.mode) {
    const effective = effectiveMode(st.mode, st.serverMode);
    // "clamped by your account" and "we could not ask your account" are different facts, and only one of
    // them is the customer's setting. Reporting an unreachable server as an account decision would send
    // someone to the dashboard to change a dial that was never the problem.
    const known = modeRank(st.serverMode) >= 0;
    rows.push(["mode", st.mode === effective ? st.mode
      : known ? `${st.mode} → ${effective} (clamped by your account's dial)`
              : `${st.mode} → ${effective} (server dial unknown — fail-closed)`]);
  }
  return rows;
}

// ── the diff /gate and /scan actually grade ─────────────────────────────────────

/** The unified diff to grade: staged changes, or the branch against `base`.
 *
 * null means GIT ITSELF FAILED (not a repo, a bad ref, a shallow clone) and is deliberately distinct from
 * "" (a repo with nothing staged). Collapsing the two would be a fail-OPEN — the caller would send nothing,
 * the gate would have no diff to object to, and a change nobody examined would read as clean. */
function stagedDiff(base, run) {
  const exec = run || ((args) => execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }));
  const args = base ? ["diff", "--no-color", `${base}...HEAD`] : ["diff", "--cached", "--no-color"];
  try {
    return exec(args);
  } catch (_) {
    return null;
  }
}

/** The `{diff}` body for /gate and /scan, or null when there is no diff to send.
 *
 * THE BUG THIS EXISTS TO FIX: the session routed both commands with an empty `{}` body, so the server
 * answered "the merge gate needs a 'diff'" on every single invocation while /help listed them as working
 * commands. Returning null forces the caller to say why nothing was sent instead of posting an empty
 * request and rendering the server's complaint as if it were a verdict. */
function diffBody(diff) {
  const d = String(diff || "");
  return d.trim() ? { diff: d } : null;
}

module.exports = {
  MODE_NAME, modeName, SETTINGS_URL,
  parseBang, runShell, dangerousCommand,
  MODES, MODE_WHAT, parseMode, modeRank, effectiveMode, modeReport, workRefusal,
  statusRows,
  stagedDiff, diffBody,
};
