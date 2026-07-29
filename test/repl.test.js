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
  const known = r.statusLines({ email: "khai@fatelabs.ca", plan: "Team", files: 857, repo: "acme/payments" });
  assert.deepEqual(known[0], ["account", "khai@fatelabs.ca · Team"]);
  assert.deepEqual(known[1], ["memory", "857 files"]);
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
