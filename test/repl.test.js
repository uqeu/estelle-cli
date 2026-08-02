"use strict";
// The session's pure half — parsing, formatting, and the reply receipt. The loop around these is thin
// I/O, so testing here covers the parts that can actually be wrong.
const test = require("node:test");
const assert = require("node:assert");
const r = require("../bin/repl.js");

const C = new Proxy({}, { get: () => (t) => String(t) });   // colours off, so assertions read plainly

test("a key must look like a key", () => {
  assert.equal(r.looksLikeKey("estelle_live_9f2b7c1d4e6a8b0c2d4e"), true);
  assert.equal(r.looksLikeKey("khai@fatelabs.ca"), false);   // pasted the wrong thing
  assert.equal(r.looksLikeKey("short"), false);
  assert.equal(r.looksLikeKey("has spaces in it right here"), false);
});

test("a stored key is masked, never shown whole", () => {
  const masked = r.maskKey("estelle_live_9f2b7c1d4e6a8b0c2d4e3f9");
  assert.ok(masked.includes("…") && !masked.includes("9f2b7c1d4e6a8b0c"));
  assert.equal(r.maskKey("tiny"), "••••");
});

test("durations read like a person wrote them", () => {
  assert.equal(r.humanDuration(6 * 3600 + 37 * 60), "6h 37m");
  assert.equal(r.humanDuration(3600), "1h");
  assert.equal(r.humanDuration(725), "12m");
  assert.equal(r.humanDuration(0), "—");
  assert.equal(r.humanDuration(5), "1m");                    // never "0m"
});

test("relative time beats a timestamp when re-entering work", () => {
  const now = Date.parse("2026-07-24T12:00:00Z");
  assert.equal(r.relativeTime("2026-07-24T11:59:30Z", now), "just now");
  assert.equal(r.relativeTime("2026-07-24T11:30:00Z", now), "30m ago");
  assert.equal(r.relativeTime("2026-07-22T12:00:00Z", now), "2d ago");
  assert.equal(r.relativeTime("not a date", now), "");
});

test("slash commands are commands; everything else is a question", () => {
  assert.deepEqual(r.parseInput("/orchestra fix the flaky tests"),
    { kind: "command", name: "orchestra", arg: "fix the flaky tests" });
  assert.deepEqual(r.parseInput("  /HELP  "), { kind: "command", name: "help", arg: "" });
  assert.deepEqual(r.parseInput("how does auth work?"), { kind: "ask", text: "how does auth work?" });
  assert.deepEqual(r.parseInput("   "), { kind: "ask", text: "" });
});

test("swarm is not a command — orchestra is", () => {
  assert.ok(r.COMMANDS.orchestra);
  assert.equal(r.COMMANDS.swarm, undefined);
});

test("status says what Estelle knows, and admits when it knows nothing", () => {
  const known = r.statusLines({ email: "khai@fatelabs.ca", plan: "Team", files: 857, memories: 16991,
                                repo: "acme/payments", account_id: "acct_1", providers: 4 });
  // the account id is the handle a support conversation needs, and it is designed to be safe to share
  assert.deepEqual(known[0], ["account", "khai@fatelabs.ca · Team · acct_1"]);
  // TWO DIFFERENT NUMBERS. Reporting `memories` under the label "files" told a customer with 857 indexed
  // files that it had 16,991 of them.
  assert.deepEqual(known[1], ["repo", "acme/payments · 857 files · 16991 memories"]);
  // ROUTING, never `model` — a model line would advertise the one thing the product says it does not do
  const routing = known.find(([label]) => label === "routing");
  assert.ok(routing && /4 providers/.test(routing[1]) && /\/routing/.test(routing[1]));
  assert.ok(!known.some(([label]) => label === "model"), "a `model` line is off-thesis (brief §1.1a)");
  const empty = r.statusLines({ email: "k@x.io" });
  assert.match(empty[1][1], /nothing indexed/);
});

test("a returning user is greeted with the handoff, a new one with nothing", () => {
  const now = Date.parse("2026-07-24T12:00:00Z");
  const g = r.welcomeBack({ at: "2026-07-22T12:00:00Z", seconds: 23820, task: "the token-refresh race" }, now);
  assert.match(g, /Last session · 2d ago · 6h 37m/);
  assert.match(g, /You were on: the token-refresh race/);
  assert.equal(r.welcomeBack(null, now), "");
});

test("the receipt always shows — grounded, refused, or degraded", () => {
  const ok = r.renderAnswer({ answer: "Short-lived JWTs.", grounded: true,
    sources: [{ file: "auth.py", line: 42 }, { file: "rotate.py" }] }, C);
  assert.match(ok, /Short-lived JWTs\./);
  assert.match(ok, /✓ grounded/);
  assert.match(ok, /auth\.py:42 · rotate\.py/);

  const bad = r.renderAnswer({ answer: "it calls frob()", grounded: false, ungrounded: ["frob"] }, C);
  assert.match(bad, /✗ not in this repo: frob/);

  const deg = r.renderAnswer({ answer: "quoted from memory", degraded: true }, C);
  assert.match(deg, /degraded/);
});

test("a scope ask is the whole reply — no receipt bolted on", () => {
  const out = r.renderAnswer({ answer: "Which repo should I answer about?", scope_ask: true }, C);
  assert.match(out, /Which repo/);
  assert.ok(!out.includes("grounded"));
});

test("every input lands on an endpoint Estelle already serves", () => {
  // the session must not mint a parallel /cli/* API — each branch reuses a real route
  assert.deepEqual(r.routeInput({ kind: "ask", text: "how does auth work?" }),
    { path: "/deep-search", body: { question: "how does auth work?" } });
  assert.deepEqual(r.routeInput({ kind: "command", name: "orchestra", arg: "fix the flaky tests" }),
    { path: "/orchestra", body: { task: "fix the flaky tests" } });
  assert.deepEqual(r.routeInput({ kind: "command", name: "gate", arg: "" }), { path: "/gate", body: {} });
  assert.equal(r.routeInput({ kind: "command", name: "memory", arg: "" }).path, "/deep-search");
});

test("/gate and /scan carry the DIFF the caller computed, not an empty body", () => {
  // The bug: both routed with `{}`, so the server answered "the merge gate needs a 'diff'" every single
  // time while /help listed them as working commands. The session must supply the diff it computed.
  const ctx = { diff: "--- a/x.py\n+++ b/x.py\n@@\n+new\n" };
  assert.deepEqual(r.routeInput({ kind: "command", name: "gate", arg: "" }, ctx),
    { path: "/gate", body: { diff: ctx.diff } });
  assert.deepEqual(r.routeInput({ kind: "command", name: "scan", arg: "" }, ctx),
    { path: "/scan", body: { diff: ctx.diff } });
});

test("the session's local commands are listed in /help — a hidden command is not a feature", () => {
  for (const name of ["mode", "status", "shell"]) {
    assert.ok(r.COMMANDS[name], `/help must document ${name}`);
  }
  assert.match(r.COMMANDS.shell, /!/, "the shell entry must show the ! form");
});

test("a rejected key is detected, and a cold start still opens", async () => {
  const rejected = await r.sessionStatus({ key: "bad", get: async () => ({ error: { code: 404 } }) });
  assert.equal(rejected.rejected, true);

  // every call failing must still yield a usable status rather than throwing the user out
  const cold = await r.sessionStatus({ key: "k", get: async () => { throw new Error("offline"); } });
  assert.equal(cold.files, 0);
  assert.equal(cold.rejected, undefined);
});

test("@file references travel with the question", () => {
  const files = { "auth.py": "def login(): pass", "rotate.py": "def rotate(): pass" };
  const { attached, missing } = r.expandFileRefs(
    "why is @auth.py slow and does @rotate.py help?", (p) => files[p] ?? null);
  assert.deepEqual(attached.map((a) => a.path), ["auth.py", "rotate.py"]);
  assert.equal(attached[0].content, "def login(): pass");
  assert.deepEqual(missing, []);
});

test("a missing @file is reported, never silently dropped", () => {
  // answering "about @ghost.py" without having read it is exactly the failure this product prevents
  const { attached, missing } = r.expandFileRefs("what about @ghost.py?", () => null);
  assert.deepEqual(attached, []);
  assert.deepEqual(missing, ["ghost.py"]);
});

test("a diff reads as a diff", () => {
  const out = r.renderDiff("--- a/x.py\n+++ b/x.py\n@@ -1 +1 @@\n-old\n+new\n context", C);
  assert.match(out, /@@ -1 \+1 @@/);
  assert.match(out, /\+new/);
  assert.match(out, /-old/);
});

test("the gate verdict is one scannable line, honest either way", () => {
  const clean = r.renderGate({ merge: true, referenced: 14, grounded_count: 14 }, C);
  assert.match(clean, /✓ gate clean/);
  assert.match(clean, /14\/14 symbols/);

  const blocked = r.renderGate({ merge: false, verdict: "blocked", ungrounded: ["frob"], secrets: ["AKIA…"] }, C);
  assert.match(blocked, /✗ gate blocked/);
  assert.match(blocked, /1 invented/);
  assert.match(blocked, /1 secrets/);
});

test("every new command routes to a real endpoint", () => {
  const expected = {
    work: "/work", gate: "/gate", scan: "/scan", improve: "/improve",
    verify: "/verify", init: "/wiki", sessions: "/sessions", resume: "/session",
  };
  for (const [name, path] of Object.entries(expected)) {
    assert.equal(r.routeInput({ kind: "command", name, arg: "x" }).path, path, `${name} -> ${path}`);
  }
  // the GET-shaped ones must say so, or the session would POST to a read endpoint
  assert.equal(r.routeInput({ kind: "command", name: "init", arg: "" }).method, "GET");
  assert.equal(r.routeInput({ kind: "command", name: "resume", arg: "s1" }).query.id, "s1");
});

test("EVERYTHING Estelle exposes over MCP is reachable from the session", () => {
  // the whole point: code-graph nav, verify, the diary, and ~190 skill playbooks are all MCP tools.
  // An unknown slash command must fall through to tools/call rather than being rejected — otherwise the
  // CLI and the MCP door drift apart the moment a skill is added.
  const nav = r.routeInput({ kind: "command", name: "find_definition", arg: "handle_work" });
  assert.equal(nav.path, "/mcp");
  assert.equal(nav.body.method, "tools/call");
  assert.deepEqual(nav.body.params, { name: "find_definition", arguments: { args: "handle_work" } });

  const skill = r.routeInput({ kind: "command", name: "skill_verify-gate", arg: "" });
  assert.equal(skill.body.params.name, "skill_verify-gate");

  assert.equal(r.routeInput({ kind: "command", name: "tools", arg: "" }).body.method, "tools/list");
});

test("an MCP reply is unwrapped to its text, and its errors stay errors", () => {
  assert.equal(r.mcpText({ result: { content: [{ text: "src/estelle/serve/api_dev.py:289" }] } }).answer,
    "src/estelle/serve/api_dev.py:289");
  assert.match(r.mcpText({ result: { content: [{ text: "boom" }], isError: true } }).error.message, /boom/);
  assert.match(r.mcpText({ error: { message: "unknown tool" } }).error.message, /unknown tool/);
  assert.match(r.mcpText({ result: { tools: [{ name: "verify" }, { name: "locate" }] } }).answer, /locate\s+verify/);
  assert.match(r.mcpText(null).error.message, /no reply/);
});

// ── input polish (patterns learned from Codex + OpenCode) ──────────────────────

test("a big paste collapses to a token but is sent whole", () => {
  const stack = Array.from({ length: 47 }, (_, i) => `  at frame ${i}`).join("\n");
  const { visible, marks } = r.collapsePaste(stack, []);
  assert.match(visible, /\[Pasted ~47 lines #1\]/);

  // the question stays readable, and Estelle still receives every line
  const typed = `why does this happen? ${visible}`;
  assert.equal(r.expandPastes(typed, marks), `why does this happen? ${stack}`);
});

test("a short paste is left alone", () => {
  const { visible, marks } = r.collapsePaste("just a line", []);
  assert.equal(visible, "just a line");
  assert.deepEqual(marks, []);
});

test("several pastes each keep their own identity", () => {
  const a = r.collapsePaste("x\n".repeat(5), []);
  const b = r.collapsePaste("y\n".repeat(9), a.marks);
  assert.match(a.visible, /#1\]/);
  assert.match(b.visible, /#2\]/);
  assert.equal(r.expandPastes(`${a.visible} ${b.visible}`, b.marks), `${"x\n".repeat(5)} ${"y\n".repeat(9)}`);
});

test("frecency: recent-and-frequent beats merely frequent", () => {
  const now = Date.parse("2026-07-24T12:00:00Z");
  const today = { hits: 2, at: now - 3600_000 };            // twice, an hour ago
  const stale = { hits: 30, at: now - 400 * 86400_000 };    // thirty times, a year+ ago
  assert.ok(r.frecencyScore(1, today, now) > r.frecencyScore(1, stale, now));
  assert.equal(r.frecencyScore(5, null, now), 5);           // never seen → untouched score
});

test("history survives a torn file instead of losing everything", () => {
  const raw = [
    JSON.stringify({ text: "how does auth work?" }),
    "{not valid json",                                        // a kill -9 mid-append
    JSON.stringify({ text: "fix the race" }),
    JSON.stringify({ text: "fix the race" }),                 // consecutive dupe
  ].join("\n");
  assert.deepEqual(r.parseHistory(raw), ["how does auth work?", "fix the race"]);
  assert.deepEqual(r.parseHistory(""), []);
});

test("history is capped so the file can't grow forever", () => {
  const raw = Array.from({ length: 200 }, (_, i) => JSON.stringify({ text: `q${i}` })).join("\n");
  const parsed = r.parseHistory(raw);
  assert.equal(parsed.length, r.HISTORY_MAX);
  assert.equal(parsed[parsed.length - 1], "q199");            // newest kept
});

test("a blank or repeated entry adds no history line", () => {
  assert.equal(r.historyLine("   ", ""), "");
  assert.equal(r.historyLine("same", "same"), "");
  assert.match(r.historyLine("new thought", "same"), /"text":"new thought"/);
});

test("ctrl-c clears a half-typed thought, and only exits when empty", () => {
  assert.equal(r.interruptAction("how does au"), "clear");   // reflexive ctrl-c must not quit
  assert.equal(r.interruptAction(""), "exit");
  assert.equal(r.interruptAction("   "), "exit");
});

test("the spinner waits before showing and holds before hiding", () => {
  assert.equal(r.spinnerPlan(200, null), "wait");            // fast work never flashes a spinner
  assert.equal(r.spinnerPlan(600, null), "show");
  assert.equal(r.spinnerPlan(1200, 600), "hold");            // shown 600ms ago — too soon to hide
  assert.equal(r.spinnerPlan(4000, 600), "may-hide");
});

test("an unknown-tool reply is the signal to try a skill", () => {
  assert.equal(r.unknownTool({ error: { message: "Unknown tool: 'deepen-architecture'" } }), true);
  assert.equal(r.unknownTool({ error: { message: "rate limit exceeded" } }), false);
  assert.equal(r.unknownTool({ answer: "ok" }), false);
  assert.equal(r.unknownTool(null), false);
});

// ── skill sessions via /skill/run (playbook stays server-side) ─────────────────

test("a skill exit line ends the session; ordinary text does not", () => {
  for (const l of ["/done", "/exit", "/stop", "  /back ", "/QUIT"]) assert.equal(r.isSkillExit(l), true, l);
  for (const l of ["yes", "/help", "tell me more", ""]) assert.equal(r.isSkillExit(l), false, l);
});

test("runSkill: a 404 means 'not a skill' so the caller falls through to normal routing", async () => {
  const deps = {
    key: "k", c: new Proxy({}, { get: () => (t) => String(t) }), out: () => {}, prompt: async () => "/done",
    post: async () => ({ error: { code: 404, message: "unknown skill" } }),
  };
  assert.equal(await r.runSkill("find_definition", "x", deps), "not-skill");
});

test("runSkill: a 402 tells the user it needs a model — and NEVER shows a playbook", async () => {
  const said = [];
  const deps = {
    key: "k", c: new Proxy({}, { get: () => (t) => String(t) }), out: (l) => said.push(l), prompt: async () => "/done",
    post: async () => ({ error: { code: 402, message: "set a provider API key (BYOK) to run a skill" } }),
  };
  assert.equal(await r.runSkill("deepen-architecture", "", deps), "needs-model");
  assert.match(said.join("\n"), /needs a model/);
  assert.ok(!said.join("\n").includes("playbook"));
});

test("runSkill: a ONE-SHOT skill prints its result and returns — no questions", async () => {
  const said = [];
  const deps = {
    key: "k", c: new Proxy({}, { get: () => (t) => String(t) }), out: (l) => said.push(l),
    prompt: async () => { throw new Error("one-shot must not prompt"); },   // it must NOT ask anything
    post: async () => ({ skill: "deepen-architecture", mode: "one-shot", reply: "Top shallow module: X", done: true }),
  };
  assert.equal(await r.runSkill("deepen-architecture", "audit", deps), "ran");
  assert.match(said.join("\n"), /Top shallow module: X/);
});

test("runSkill: an INTERACTIVE skill drives the back-and-forth; the CLIENT sends only the name + messages", async () => {
  // the whole point: the client never fetches/holds the playbook — it POSTs {skill, messages} each turn
  const said = [], sentBodies = [], answers = ["a coding CLI", "/done"];
  const deps = {
    key: "k", c: new Proxy({}, { get: () => (t) => String(t) }), out: (l) => said.push(l),
    prompt: async () => answers.shift(),
    post: async (path, body) => { sentBodies.push(body); return { skill: "interrogate", mode: "interactive", reply: "Next?", done: false }; },
  };
  assert.equal(await r.runSkill("interrogate", "", deps), "ran");
  assert.ok(sentBodies.every((b) => b.skill === "interrogate" && !("playbook" in b) && !("system" in b)));
  assert.match(said.join("\n"), /interactive/);                         // the mode banner showed
  assert.deepEqual(sentBodies[1].messages.map((m) => m.content), ["Begin.", "Next?", "a coding CLI"]);
});

test("an error is an error, never a blank success", () => {
  assert.match(r.renderAnswer({ error: { message: "rate limit exceeded" } }, C), /✗ rate limit exceeded/);
  assert.match(r.renderAnswer(null, C), /✗ no reply/);
});

// ── the local commands: answered in the terminal, with no request at all ────────

/** handleLocal's ctx, with everything recorded so the assertions read as "what did the user see?". */
function localCtx(over) {
  const said = [];
  return {
    said,
    ctx: {
      out: (l) => said.push(l), c: C, key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
      state: { mode: "propose", serverMode: null },
      serverMode: async () => "propose",
      deps: { api: "https://api.fatelabs.ca", repo: "uqeu/estelle" },
      ...over,
    },
  };
}

test("/help documents the shell escape in the form you actually type", () => {
  const { said, ctx } = localCtx();
  return r.handleLocal({ kind: "command", name: "help", arg: "" }, ctx).then((done) => {
    assert.equal(done, "handled");
    assert.match(said.join("\n"), /!<cmd>/);
    assert.match(said.join("\n"), /\/mode/);
    assert.match(said.join("\n"), /\/status/);
    assert.ok(!said.join("\n").includes("/shell"), "there is no /shell command — the ! form is the real one");
  });
});

test("/mode with no argument reports; a bad name changes nothing", async () => {
  const { said, ctx } = localCtx();
  await r.handleLocal({ kind: "command", name: "mode", arg: "" }, ctx);
  assert.match(said.join("\n"), /effective\s+propose/);

  await r.handleLocal({ kind: "command", name: "mode", arg: "yolo" }, ctx);
  assert.match(said.join("\n"), /no mode called "yolo"/);
  assert.equal(ctx.state.mode, "propose", "an unrecognised name must never move the ceiling");
});

test("/mode plan LOWERS the ceiling — the one direction the CLI is allowed to move it", async () => {
  const { said, ctx } = localCtx();
  await r.handleLocal({ kind: "command", name: "mode", arg: "plan" }, ctx);
  assert.equal(ctx.state.mode, "read_only");
  assert.match(said.join("\n"), /effective\s+read_only/);
});

test("/mode auto on a propose account is CLAMPED and says so — the CLI cannot grant autonomy", async () => {
  const { said, ctx } = localCtx();
  await r.handleLocal({ kind: "command", name: "mode", arg: "auto" }, ctx);
  const out = said.join("\n");
  assert.match(out, /local\s+execute/);
  assert.match(out, /effective\s+propose/);
  assert.match(out, /cannot raise/i);
});

test("/status answers what this session is pointed at, and masks the key", async () => {
  const { said, ctx } = localCtx();
  await r.handleLocal({ kind: "command", name: "status", arg: "" }, ctx);
  const out = said.join("\n");
  assert.match(out, /api\.fatelabs\.ca/);
  assert.match(out, /uqeu\/estelle/);
  assert.ok(!out.includes("9f2b7c1d4e6a8b0c"), "a credential is never echoed whole");
});

test("/sweep stops advertising a command that never worked — it names the one that does", async () => {
  // It was listed in /help but had no route, so it fell through to an MCP tool that has never existed.
  const { said, ctx } = localCtx();
  assert.equal(await r.handleLocal({ kind: "command", name: "sweep", arg: "" }, ctx), "handled");
  assert.match(said.join("\n"), /estelle sweep/);
});

test("every OTHER slash command still falls through to the router", async () => {
  // If a local dispatcher swallowed unknown names, the CLI and the MCP door would drift apart the moment
  // a skill or a nav tool was added.
  const { ctx } = localCtx();
  assert.equal(await r.handleLocal({ kind: "command", name: "find_definition", arg: "x" }, ctx), "");
  assert.equal(await r.handleLocal({ kind: "command", name: "exit", arg: "" }, ctx), "exit");
});

// ── the session loop: the gate is run on a real diff, or not at all ─────────────

/** A session driven by a scripted list of typed lines. Returns everything printed and every call made. */
async function session(lines, over) {
  const said = [], posts = [];
  const queue = [...lines];
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async (path, body) => { posts.push({ path, body }); return { answer: "ok" }; },
    prompt: async () => (queue.length ? queue.shift() : null),
    out: (l) => said.push(l), c: C, cwd: "estelle", now: () => Date.now(),
    ...over,
  });
  return { said: said.join("\n"), posts };
}

test("/gate posts the DIFF — the bug was an empty body that errored every single time", async () => {
  const diff = "--- a/x.py\n+++ b/x.py\n@@\n+new\n";
  const { posts } = await session(["/gate"], { diff: () => diff });
  const gate = posts.find((p) => p.path === "/gate");
  assert.ok(gate, "/gate must actually be called");
  assert.equal(gate.body.diff, diff);
});

test("/gate <ref> passes the base ref through to the diff, not to the server", async () => {
  const seen = [];
  await session(["/gate main"], { diff: (base) => { seen.push(base); return "d"; } });
  assert.deepEqual(seen, ["main"]);
});

test("a git failure BLOCKS and sends nothing — 'git broke' is not a clean verdict", async () => {
  const { said, posts } = await session(["/gate"], { diff: () => null });
  assert.equal(posts.filter((p) => p.path === "/gate").length, 0);
  assert.match(said, /BLOCKED \(fail-closed\)/);
});

test("nothing staged is said plainly, and still sends nothing", async () => {
  const { said, posts } = await session(["/scan"], { diff: () => "" });
  assert.equal(posts.filter((p) => p.path === "/scan").length, 0);
  assert.match(said, /Nothing staged/);
});

test("!<command> runs in the shell and comes back to the prompt", async () => {
  const { said, posts } = await session(["!echo estelle-shell-ok", "how does auth work?"]);
  assert.match(said, /estelle-shell-ok/);
  assert.deepEqual(posts.map((p) => p.path), ["/deep-search"], "a shell line is never sent to Estelle");
});

// ── the mode switch, and the apply path it exists to feed ───────────────────────
// A mode indicator without a local write path is a switch wired to nothing: /work used to render a diff
// and drop it. These pin both halves — what the eye sees, and what actually lands on disk.

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

/** A throwaway git repo with one committed file, so an apply can be asserted against real bytes. */
function tinyRepo(body) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-repl-"));
  const git = (...a) => execFileSync("git", a, { cwd: root, encoding: "utf8",
    env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" } });
  fs.writeFileSync(path.join(root, "a.txt"), body);
  git("init", "-q"); git("config", "user.email", "t@t.t"); git("config", "user.name", "t");
  git("add", "-A"); git("commit", "-qm", "base");
  return root;
}

const WORK_DIFF = ["--- a/a.txt", "+++ b/a.txt", "@@ -1,3 +1,3 @@", " one", "-two", "+TWO", " three", ""].join("\n");

/** Like `session`, but records the PROMPT STRING each turn — that is where the mode is rendered. */
async function promptSession(lines, over) {
  const said = [], prompts = [];
  const queue = [...lines];
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async () => ({ answer: "ok" }),
    prompt: async (q) => { prompts.push(String(q)); return queue.length ? queue.shift() : null; },
    out: (l) => said.push(l), c: C, cwd: "estelle", now: () => Date.now(),
    ...over,
  });
  return { said: said.join("\n"), prompts };
}

test("the prompt carries the mode on every line — the ceiling is no longer invisible", async () => {
  const { prompts } = await promptSession(["hello"]);
  assert.ok(prompts.length, "the session must have prompted at all");
  assert.ok(prompts.every((p) => p.includes("propose")), `mode missing from prompt: ${prompts[0]}`);
});

test("the prompt shows the CLAMP once the account's dial is known and lower", async () => {
  // /mode forces the lazy dial fetch; after it, a local mode above the dial must read as clamped.
  const { prompts } = await promptSession(["/mode execute", "hi"],
    { get: async (p) => (p === "/autonomy/scope" ? { global: "propose" } : {}) });
  // `execute` prints as `auto` now — display only; parseMode still resolves it to the rung.
  assert.ok(prompts.some((p) => p.includes("auto→propose")), prompts.join(" | "));
});

test("shift+tab is BOUND on entry and UNBOUND on the way out", async () => {
  let bound = 0, unbound = 0, cycle = null;
  await promptSession(["/exit"], {
    bindKeys: (fn) => { bound += 1; cycle = fn; return () => { unbound += 1; }; },
  });
  assert.equal(bound, 1, "the key must be bound exactly once");
  assert.equal(unbound, 1, "leaving without unbinding leaks a listener on stdin");
  assert.equal(typeof cycle, "function");
});

test("the cycle moves the mode and hands back a banner and a fresh prompt", async () => {
  let cycle = null;
  await promptSession(["/exit"], { bindKeys: (fn) => { cycle = fn; return () => {}; },
                                   get: async (p) => (p === "/autonomy/scope" ? { global: "branch" } : {}) });
  const first = await cycle();
  assert.match(first.banner, /shift\+tab/);
  assert.match(first.prompt, /read|propose|branch|auto/);   // the DISPLAY names
  const second = await cycle();
  assert.notEqual(first.prompt, second.prompt, "cycling twice must land on a different rung");
});

test("/work now APPLIES its diff instead of drawing it and dropping it", async () => {
  const root = tinyRepo("one\ntwo\nthree\n");
  const { said } = await promptSession(["/work fix it", "y"], {
    root,
    post: async (p) => (p === "/work" ? { answer: "done", diff: WORK_DIFF } : { answer: "ok" }),
    get: async (p) => (p === "/autonomy/scope" ? { global: "propose" } : {}),
  });
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "one\nTWO\nthree\n",
               `the diff never reached disk. session said:\n${said}`);
});

test("a /work diff is drawn ONCE — the apply flow owns the receipt", async () => {
  const root = tinyRepo("one\ntwo\nthree\n");
  const { said } = await promptSession(["/work fix it", "n"], {
    root,
    post: async (p) => (p === "/work" ? { answer: "done", diff: WORK_DIFF } : { answer: "ok" }),
    get: async (p) => (p === "/autonomy/scope" ? { global: "propose" } : {}),
  });
  assert.equal(said.split("+TWO").length - 1, 1, `the diff was rendered twice:\n${said}`);
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "one\ntwo\nthree\n", "'n' must write nothing");
});

test("/undo puts an applied /work change back", async () => {
  const root = tinyRepo("one\ntwo\nthree\n");
  const { said } = await promptSession(["/work fix it", "y", "/undo"], {
    root,
    post: async (p) => (p === "/work" ? { answer: "done", diff: WORK_DIFF } : { answer: "ok" }),
    get: async (p) => (p === "/autonomy/scope" ? { global: "propose" } : {}),
  });
  assert.equal(fs.readFileSync(path.join(root, "a.txt"), "utf8"), "one\ntwo\nthree\n", said);
});

test("/apply with nothing to apply says so rather than pretending", async () => {
  const { said } = await promptSession(["/apply"], { root: tinyRepo("one\ntwo\nthree\n") });
  assert.match(said, /no diff yet/i);
});

test("/undo with nothing to undo says so rather than reporting success", async () => {
  const { said } = await promptSession(["/undo"], { root: tinyRepo("one\ntwo\nthree\n") });
  assert.match(said, /nothing to undo/i);
});

// ── the thinking indicator (spinnerPlan, finally wired) ────────────────────────

test("without a terminal the spinner is inert — piped output must stay exactly what it was", async () => {
  let wrote = 0;
  const v = await r.withSpinner("thinking", async () => "answer", { write: null });
  assert.equal(v, "answer");
  assert.equal(wrote, 0);
});

test("a fast call never draws a frame; a slow one draws and then clears the line", async () => {
  const frames = [];
  const fast = await r.withSpinner("thinking", async () => "quick", { write: (s) => frames.push(s) });
  assert.equal(fast, "quick");
  assert.deepEqual(frames, [], "a 0ms call must not flash a spinner");

  const slow = await r.withSpinner("thinking", () => new Promise((res) => setTimeout(() => res("slow"), 800)),
                                   { write: (s) => frames.push(s) });
  assert.equal(slow, "slow");
  assert.ok(frames.length > 1, "a slow call must show it is alive");
  assert.match(frames[frames.length - 1], /\x1b\[2K$/, "the spinner must erase itself, not leave a frame behind");
});

test("a call that throws still tears the spinner down", async () => {
  const frames = [];
  await assert.rejects(
    () => r.withSpinner("thinking", () => new Promise((_, rej) => setTimeout(() => rej(new Error("boom")), 700)),
                        { write: (s) => frames.push(s) }),
    /boom/);
  assert.match(frames[frames.length - 1], /\x1b\[2K$/, "an error must not leave the line wedged");
});

// ── the curated turn, driven through the real session loop ─────────────────────────────────────────────
// The unit tests for the curation itself live in curate.test.js. What is asserted here is that the SESSION
// actually uses it: that a second question carries the first, that a failed turn is carried as a lesson and
// not as wreckage, and that the prompt Estelle assembles cannot grow without bound.

test("a session carries its own history — the second question knows about the first", async () => {
  const replies = [{ answer: "The sweep walks the tree serially." }, { answer: "Batch them." }];
  const asked = [];
  const queue = ["why is the sweep slow?", "so how do I fix it?"];
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async (_p, body) => { asked.push(body.question || ""); return replies[asked.length - 1] || {}; },
    prompt: async () => (queue.length ? queue.shift() : null),
    out: () => {}, c: C, cwd: "estelle", now: () => Date.now(),
  });
  assert.equal(asked[0], "why is the sweep slow?", "turn one is just the question — no ceremony");
  const second = asked[1];
  assert.match(second, /SESSION SO FAR/);
  assert.match(second, /why is the sweep slow\?/, "the earlier question travels");
  assert.match(second, /walks the tree serially/, "and so does the earlier answer");
  assert.match(second, /CURRENT QUESTION: so how do I fix it\?/);
});

test("a failed turn is carried as a LESSON, not as wreckage", async () => {
  // A long, verbose failure, then five more turns so it falls out of the verbatim window.
  const wreck = "Tried the batch upload.\nThe run failed: 413 Payload Too Large\n"
    + Array.from({ length: 40 }, (_, i) => `  File "sweep.py", line ${i}, in _upload_batch`).join("\n");
  const replies = [{ answer: wreck }, { answer: "a" }, { answer: "b" }, { answer: "c" },
                   { answer: "d" }, { answer: "e" }, { answer: "f" }];
  let n = 0;
  const seen = [];
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async (path, body) => { seen.push(body.question || ""); return replies[n++] || { answer: "" }; },
    prompt: async () => (n < 7 ? `question ${n}` : null),
    out: () => {}, c: C, cwd: "estelle", now: () => Date.now(),
  });
  const last = seen[seen.length - 1];
  assert.ok(!last.includes("_upload_batch"), "the 40 stack frames must not still be travelling");
  assert.match(last, /LESSONS/);
  assert.match(last, /413 Payload Too Large/, "but what the failure TAUGHT must be");
});

test("an ERROR reply is recorded too — the wreckage a naive transcript would carry forever", async () => {
  const seen = [];
  let n = 0;
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async (_p, body) => { seen.push(body.question || ""); n++; return n === 1 ? { error: { message: "402 no provider key" } } : { answer: "ok" }; },
    prompt: async () => (n < 2 ? "ask" : null),
    out: () => {}, c: C, cwd: "estelle", now: () => Date.now(),
  });
  assert.match(seen[1], /402 no provider key/, "the next turn knows the last one failed");
});

test("/context prints the receipt for what the next turn will carry", async () => {
  const { said } = await session(["why is the sweep slow?", "/context"]);
  assert.match(said, /turns.*verbatim of 2 recorded/);
  assert.match(said, /cost.*~\d+ tokens carried/);
});

test("a slash command that hits the server is NOT wrapped in the session context", async () => {
  // /work, /gate and the rest take structured bodies; only a typed question carries the working set.
  const { posts } = await session(["why is the sweep slow?", "/improve"]);
  const improve = posts.find((p) => p.path === "/improve");
  assert.ok(improve && !JSON.stringify(improve.body).includes("SESSION SO FAR"));
});

test("`--version` prints the version and exits 0, instead of opening a session", async () => {
  // FOUND BY RUNNING THE PUBLISHED 0.1.9 THROUGH npx, not by reading the source. `cmd.startsWith("--")`
  // fell through to the interactive session, so the most standard flag in any CLI prompted for a key and
  // exited 1 on a non-TTY — which is what a script, a CI job, or the first line of a bug report does.
  // Every existing test drove a SUBCOMMAND, so nothing exercised the bare-flag path. Register #75.
  const { execFile } = require("node:child_process");
  const cliPath = require("node:path").join(__dirname, "..", "bin", "estelle.js");
  for (const flag of ["--version", "-v"]) {
    const r = await new Promise((res) => {
      execFile(process.execPath, [cliPath, flag], { encoding: "utf8" },
               (err, stdout) => res({ code: err ? (err.code === undefined ? 1 : err.code) : 0, stdout }));
    });
    assert.strictEqual(r.code, 0, `${flag} exited ${r.code}`);
    assert.match(r.stdout.trim(), /^\d+\.\d+\.\d+$/, `${flag} printed ${JSON.stringify(r.stdout)}`);
  }
});
