"use strict";
// The session's LOCAL half — the things that happen in the terminal rather than on the server: the `!`
// shell escape, the mode ceiling, and the staged diff the gate is actually run on. Every decision here is
// pure so it can be asserted without a shell, a repo, or a network.
const test = require("node:test");
const assert = require("node:assert");
const s = require("../bin/session-commands.js");
const hook = require("../bin/hook.js");

const C = new Proxy({}, { get: () => (t) => String(t) });   // colours off, so assertions read plainly

// ── `!` shell passthrough ───────────────────────────────────────────────────────

test("a bang line is a shell command; everything else is not", () => {
  assert.deepEqual(s.parseBang("!git status"), { command: "git status" });
  assert.deepEqual(s.parseBang("  !  npm test  "), { command: "npm test" });
  assert.equal(s.parseBang("!"), null);              // a bare bang is not a command
  assert.equal(s.parseBang("!   "), null);
  assert.equal(s.parseBang("/gate"), null);
  assert.equal(s.parseBang("how does auth work?"), null);
  assert.equal(s.parseBang("wow! that worked"), null);   // a bang mid-line is prose
});

test("the shell escape reuses the hook's danger heuristic — it never re-implements it", () => {
  // A second copy of this rule would drift from the hook's within a release, and the drift would be
  // silent: the guard that fires on `estelle hook guard` would not fire here.
  assert.equal(s.dangerousCommand, hook.dangerousCommand);
});

test("a dangerous shell command asks before it runs, and a refusal runs nothing", async () => {
  const out = [];
  let spawned = null;
  const deps = {
    out: (l) => out.push(l), c: C,
    prompt: async () => "n",
    spawn: (cmd) => { spawned = cmd; return { status: 0, stdout: "", stderr: "" }; },
  };
  const code = await s.runShell("rm -rf /", deps);
  assert.equal(spawned, null, "a refused command must never reach the shell");
  assert.equal(code, 1, "a refusal is not a success");
  assert.match(out.join("\n"), /recursive force-delete/);
});

test("a dangerous shell command runs when the human confirms — advisory, not a block", async () => {
  let spawned = null;
  const code = await s.runShell("rm -rf /", {
    out: () => {}, c: C, prompt: async () => "y",
    spawn: (cmd) => { spawned = cmd; return { status: 0, stdout: "gone", stderr: "" }; },
  });
  assert.equal(spawned, "rm -rf /");
  assert.equal(code, 0);
});

test("an ordinary shell command runs with no prompt at all", async () => {
  const out = [];
  let asked = 0;
  const code = await s.runShell("git status", {
    out: (l) => out.push(l), c: C,
    prompt: async () => { asked += 1; return "y"; },
    spawn: () => ({ status: 0, stdout: "On branch main\n", stderr: "" }),
  });
  assert.equal(asked, 0, "a guard that cries wolf on `git status` gets muted within a day");
  assert.equal(code, 0);
  assert.match(out.join("\n"), /On branch main/);
});

test("a failing shell command shows its stderr and keeps its exit code", async () => {
  const out = [];
  const code = await s.runShell("false", {
    out: (l) => out.push(l), c: C, prompt: async () => "y",
    spawn: () => ({ status: 3, stdout: "", stderr: "boom\n" }),
  });
  assert.equal(code, 3, "an error must never be reported as a success");
  assert.match(out.join("\n"), /boom/);
  assert.match(out.join("\n"), /exit 3/);
});

test("a shell that cannot start is an error, never a silent nothing", async () => {
  const out = [];
  const code = await s.runShell("nope", {
    out: (l) => out.push(l), c: C, prompt: async () => "y",
    spawn: () => ({ error: new Error("spawn ENOENT"), status: null, stdout: "", stderr: "" }),
  });
  assert.notEqual(code, 0);
  assert.match(out.join("\n"), /ENOENT/);
});

// ── modes: a CEILING that reflects the server, never a grant ────────────────────

test("the mode vocabulary IS the server's autonomy ladder, with the familiar aliases", () => {
  assert.deepEqual(s.MODES, ["read_only", "propose", "branch", "execute"]);
  assert.equal(s.parseMode("plan"), "read_only");        // the founder's word for it
  assert.equal(s.parseMode("read-only"), "read_only");
  assert.equal(s.parseMode("READONLY"), "read_only");
  assert.equal(s.parseMode("propose"), "propose");
  assert.equal(s.parseMode("auto"), "execute");
  assert.equal(s.parseMode("yolo"), "");                 // unknown never resolves to a level
  assert.equal(s.parseMode(""), "");
});

test("the effective ceiling is the LOWER of the local mode and the server's dial", () => {
  assert.equal(s.effectiveMode("execute", "propose"), "propose");   // the server wins going up
  assert.equal(s.effectiveMode("read_only", "execute"), "read_only"); // the local mode wins going down
  assert.equal(s.effectiveMode("propose", "propose"), "propose");
});

test("an unknown server dial fails CLOSED, never open", () => {
  // /autonomy/scope unreachable is "cannot answer", not "you may do anything".
  assert.equal(s.effectiveMode("execute", ""), "read_only");
  assert.equal(s.effectiveMode("execute", null), "read_only");
  assert.equal(s.effectiveMode("execute", "nonsense"), "read_only");
});

test("the mode report says what the server granted, not what the user typed", () => {
  const clamped = s.modeReport("execute", "read_only", C).join("\n");
  assert.match(clamped, /local\s+execute/);
  assert.match(clamped, /server\s+read_only/);
  assert.match(clamped, /effective\s+read_only/);
  assert.match(clamped, /cannot raise/i, "the CLI must say it cannot grant autonomy");

  const honest = s.modeReport("propose", "propose", C).join("\n");
  assert.doesNotMatch(honest, /cannot raise/i);
  assert.match(honest, /effective\s+propose/);
});

test("an unreachable server is DISCLOSED in the report, never rendered as read_only-by-choice", () => {
  const report = s.modeReport("propose", "", C).join("\n");
  assert.match(report, /unknown/);
  assert.match(report, /read_only/);
});

test("the write path is refused at a KNOWN read_only, and the refusal explains itself", () => {
  assert.equal(s.workRefusal("propose", "propose"), "");
  assert.equal(s.workRefusal("execute", "execute"), "");

  const mine = s.workRefusal("read_only", "execute");   // the user lowered it themselves
  assert.match(mine, /mode is read_only/);
  assert.match(mine, /\/mode/, "a refusal a user cannot act on is a dead end");

  const theirs = s.workRefusal("propose", "read_only"); // the account says no
  assert.match(theirs, /autonomy dial is read_only/);
});

test("an UNKNOWN dial does not make the CLI block the write path — the server enforces it", () => {
  // Blocking on "I could not read /autonomy/scope" would break /work against every server that does not
  // serve it, and buy nothing: the server gates /work through autonomy.allows either way.
  assert.equal(s.workRefusal("propose", ""), "");
  assert.equal(s.workRefusal("propose", null), "");
});

// ── /status ─────────────────────────────────────────────────────────────────────

test("status shows the endpoint, the masked key, the filed repo and the ceiling", () => {
  const rows = s.statusRows({ api: "https://api.fatelabs.ca", keyMasked: "estelle…3f9",
                              repo: "uqeu/estelle", mode: "propose", serverMode: "propose" });
  const flat = new Map(rows);
  assert.equal(flat.get("endpoint"), "https://api.fatelabs.ca");
  assert.equal(flat.get("key"), "estelle…3f9");
  assert.equal(flat.get("repo"), "uqeu/estelle");
  assert.match(flat.get("mode"), /propose/);
});

test("status distinguishes 'your account said no' from 'we could not ask'", () => {
  // Reporting an unreachable server as an account setting sends the user to the dashboard to change a
  // dial that was never the problem.
  const clamped = new Map(s.statusRows({ mode: "execute", serverMode: "propose" })).get("mode");
  assert.match(clamped, /clamped by your account/);

  const unknown = new Map(s.statusRows({ mode: "propose", serverMode: "" })).get("mode");
  assert.match(unknown, /unknown/);
  assert.doesNotMatch(unknown, /clamped by your account/);
});

test("status never invents a key or a repo it does not have", () => {
  const flat = new Map(s.statusRows({ api: "https://api.fatelabs.ca", keyMasked: "", repo: "" }));
  assert.match(flat.get("key"), /not set/i);
  assert.match(flat.get("repo"), /unfiled|not/i);
});

// ── the staged diff the gate actually runs on ───────────────────────────────────

test("a git failure is null — 'git broke' is not 'nothing to gate'", () => {
  // Conflating them is a fail-OPEN: the gate would report clean over a diff it never saw.
  assert.equal(s.stagedDiff("", () => { throw new Error("not a repo"); }), null);
});

test("the staged diff is the default; a base ref diffs the branch instead", () => {
  const seen = [];
  s.stagedDiff("", (args) => { seen.push(args); return "D"; });
  assert.deepEqual(seen[0], ["diff", "--cached", "--no-color"]);
  s.stagedDiff("main", (args) => { seen.push(args); return "D"; });
  assert.deepEqual(seen[1], ["diff", "--no-color", "main...HEAD"]);
});

test("a diff-shaped body is built only when there IS a diff", () => {
  // The bug this fixes: the session posted `{}` to /gate and /scan, so both errored every single time
  // while /help advertised them as working.
  assert.deepEqual(s.diffBody("--- a\n+++ b\n"), { diff: "--- a\n+++ b\n" });
  assert.equal(s.diffBody(""), null);
  assert.equal(s.diffBody("   \n  "), null);
  assert.equal(s.diffBody(null), null);
});
