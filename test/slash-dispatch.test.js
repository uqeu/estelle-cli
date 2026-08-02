"use strict";
// REGISTER #93 — a slash command must never fall through to a model call, and must never render nothing.
//
// TWO HALVES, and neither one is a test of the feature on its own (E-027):
//
//   (a) EVERY advertised command dispatches. Enumerated from the code at run time, never sampled — a
//       hand-written list would go stale the moment a command is added and then pass green while the new
//       one is untested, which is the defect class this whole campaign is closing.
//   (b) FALL-THROUGH IS IMPOSSIBLE. An unmatched `/` is refused LOCALLY with **zero network calls**, and
//       the paired positive is asserted beside it: a name the registry DOES know still goes out, or the
//       guard would pass by refusing everything.
//
// Plus the render half, which is what a customer actually experienced: six routed commands returned
// shapes `renderAnswer` could not draw and printed a BLANK LINE after a real round-trip. Measured on prod
// 2026-08-02 against api.fatelabs.ca; the fixtures below are those measured shapes, not invented ones.

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");

const r = require(path.join(__dirname, "..", "bin", "repl.js"));
const slashMenu = require(path.join(__dirname, "..", "bin", "slash-menu.js"));
const known = require(path.join(__dirname, "..", "bin", "known-commands.js"));
const replies = require(path.join(__dirname, "..", "bin", "replies.js"));

const C = new Proxy({}, { get: () => (s) => String(s === undefined ? "" : s) });

// ── the harness ────────────────────────────────────────────────────────────────
// Every call the session makes is recorded with its path, so "no network call" is an assertion about
// OBSERVED traffic rather than about what the code looks like it does.

// `serverGet`/`serverPost` model the REAL server's answers, and the recording wrapper is applied on top of
// whatever a test supplies. An override that replaced `post` wholesale would silently stop recording, and
// then "it sent nothing" would pass for a session that sent plenty — a vacuous pass inside the very
// assertion this file exists to make.
const SKILLS = [{ name: "bug-hunt", short: "x" }];
const TOOLS = [{ name: "find_definition" }, { name: "blast_radius" }];

async function serverGet(p) {
  if (p === "/skills") return { skills: SKILLS };
  if (p === "/autonomy/scope") return { global: "read_only" };
  return {};
}
async function serverPost(p, body) {
  if (p === "/mcp" && body && body.method === "tools/list") return { result: { tools: TOOLS } };
  // A real `/skill/run` 404s for a name that is not a skill — that 404 is the whole reason the old code
  // spent a round-trip per unknown command, so a stub that answered every name would hide the defect.
  if (p === "/skill/run") {
    const name = String((body && body.skill) || "");
    if (!SKILLS.some((s) => s.name === name)) return { error: { message: `unknown skill: ${name}`, code: 404 } };
    return { reply: "ran it", done: true };
  }
  return { answer: "ok" };
}

async function session(lines, over) {
  const said = [], posts = [], gets = [];
  const queue = [...lines];
  const o = over || {};
  const get = o.get || serverGet;
  const post = o.post || serverPost;
  const { get: _g, post: _p, ...rest } = o;
  await r.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async (p, k) => { gets.push(p); return get(p, k); },
    post: async (p, body, k) => { posts.push({ path: p, body }); return post(p, body, k); },
    prompt: async () => (queue.length ? queue.shift() : null),
    out: (l) => said.push(l === undefined ? "" : String(l)), c: C, cwd: "estelle", now: () => Date.now(),
    ...rest,
  });
  return { said: said.join("\n"), posts, gets };
}

/** The calls a session makes on ITS OWN at startup, with no line typed. The baseline every
 * "no network call" assertion is measured against — otherwise the guard would be credited for silence
 * the session was always going to produce. */
async function baseline() {
  const { posts, gets } = await session([]);
  return { posts: posts.length, gets: gets.length };
}

// ── (a) EVERY ADVERTISED COMMAND DISPATCHES ────────────────────────────────────

test("#93a every /help entry dispatches — none falls through to a raw MCP tools/call", async () => {
  const names = Object.keys(r.COMMANDS);
  assert.ok(names.length >= 20, `derived only ${names.length} commands — the derivation is broken`);
  const fellThrough = [];
  for (const name of names) {
    // LOCAL first: handleLocal answers a command with no request at all.
    const state = { mode: "read_only", serverMode: "read_only", transcript: { turns: [], lessons: [] } };
    const local = await r.handleLocal({ kind: "command", name, arg: "" }, {
      out: () => {}, c: C, state, serverMode: async () => "read_only", key: "k",
      deps: { api: "a", repo: "o/n", cwdPath: "/tmp", root: "/tmp", prompt: async () => "" },
    });
    if (local === "handled" || local === "exit") continue;
    const route = r.routeInput({ kind: "command", name, arg: "x" }, {}, { repo: "o/n" });
    const raw = route.mcp && route.body && route.body.method === "tools/call"
      && route.body.params && route.body.params.name === name;
    if (raw) fellThrough.push(name);
  }
  assert.deepEqual(fellThrough, [],
    `these /help entries reach nothing but a raw MCP tools/call: ${fellThrough.join(", ")}`);
});

test("#93a every menu row is a command, and every command is a menu row — both directions", () => {
  const declared = new Set(Object.keys(r.COMMANDS));
  const wired = new Set([...declared].filter((n) => !r.HELP_ONLY.has(n)));
  const rows = slashMenu.menuRows(r.COMMANDS, wired, []).filter((x) => x.group !== "skills");
  const rowNames = new Set(rows.map((x) => x.name));
  assert.deepEqual([...rowNames].filter((n) => !declared.has(n)), [],
    "a menu row promises what nothing implements");
  // The inverse, which is the half that was broken: `tools` was routed and absent from COMMANDS, so a
  // real capability had no door anywhere a customer could see it.
  assert.deepEqual([...declared].filter((n) => !rowNames.has(n) && !r.HELP_ONLY.has(n)), [],
    "a wired command never appears in the menu");
});

test("#93a every alias resolves onto a real command — an alias to nothing is a door to nothing", () => {
  for (const [alias, target] of Object.entries(r.COMMAND_ALIASES)) {
    assert.ok(Object.prototype.hasOwnProperty.call(r.COMMANDS, target),
      `${alias} aliases ${target}, which is not a command`);
  }
});

test("#93a /shell is answered LOCALLY with the ! form — it used to cost two round-trips", async () => {
  const before = await baseline();
  const { said, posts, gets } = await session(["/shell"]);
  assert.match(said, /!git status/, "it must show the form that actually works");
  assert.equal(posts.length, before.posts, "/shell must send nothing");
  assert.equal(gets.length, before.gets, "/shell must send nothing");
});

test("#93a /orchestra and /work refuse an EMPTY task locally rather than posting one", async () => {
  const before = await baseline();
  // `/work` is refused for its CEILING first when the dial is read_only, which is correct and is a
  // different refusal — so the arg guard is measured on an account that may actually write.
  const permissive = { get: async (p) => (p === "/autonomy/scope" ? { global: "execute" } : serverGet(p)) };
  for (const [name, over] of [["orchestra", {}], ["work", permissive]]) {
    const { said, posts } = await session([`/${name}`], over);
    assert.match(said, new RegExp(`/${name} needs a task`), `${name} must say what it needs`);
    assert.equal(posts.length, before.posts, `/${name} with no task must send nothing`);
  }
  // …and under a read_only ceiling /work still refuses locally, for the ceiling, sending nothing.
  const ro = await session(["/work do a thing"]);
  assert.match(ro.said, /the write path is off|autonomy dial/);
  assert.equal(ro.posts.filter((p) => p.path === "/work").length, 0);
});

// ── (b) FALL-THROUGH IS IMPOSSIBLE ─────────────────────────────────────────────

test("#93b an UNKNOWN slash command sends NOTHING to the model — the whole point", async () => {
  const before = await baseline();
  const { said, posts } = await session(["/sessionz"]);
  // ZERO model-bearing calls. /skill/run and a tools/call are what it used to cost, in that order.
  assert.equal(posts.filter((p) => p.path === "/skill/run").length, 0, "it must not try it as a skill");
  assert.equal(posts.filter((p) => p.path === "/deep-search").length, 0, "it must never reach the model");
  const calls = posts.filter((p) => !(p.path === "/mcp" && p.body && p.body.method === "tools/list"));
  assert.equal(calls.length, before.posts, "the only permitted call is the ONE cached registry read");
  assert.match(said, /unknown command/, "it must say so");
  assert.match(said, /nothing was sent/, "and say that nothing was spent");
});

test("#93b the refusal offers a did-you-mean when there is a real candidate, and none when there isn't", async () => {
  const near = await session(["/sessionz"]);
  assert.match(near.said, /did you mean/);
  assert.match(near.said, /\/sessions/);
  const far = await session(["/xyzzy"]);
  assert.match(far.said, /unknown command/);
  assert.ok(!/did you mean/.test(far.said), "a wrong did-you-mean is worse than none");
});

test("#93b THE PAIRED POSITIVE — a name the registry knows still goes out", async () => {
  // Without this the guard could pass by refusing everything, which is the vacuity trap.
  const { posts } = await session(["/find_definition resolve_grounding_scope"]);
  const call = posts.find((p) => p.path === "/mcp" && p.body && p.body.method === "tools/call");
  assert.ok(call, "a real MCP tool must still reach the server");
  assert.equal(call.body.params.name, "find_definition");
  // …and it no longer pays for a skill probe first, because we now KNOW it is a tool.
  assert.equal(posts.filter((p) => p.path === "/skill/run").length, 0,
    "a known tool must not be tried as a skill — that was the second wasted round-trip");
});

test("#93b a KNOWN SKILL still runs server-side", async () => {
  const { posts } = await session(["/skill_bug-hunt look at auth"]);
  assert.ok(posts.find((p) => p.path === "/skill/run"), "a listed skill must still reach /skill/run");
});

test("#93b the registry cache is read ONCE per session, not once per command", async () => {
  const { posts } = await session(["/nope1", "/nope2", "/nope3"]);
  const lists = posts.filter((p) => p.path === "/mcp" && p.body && p.body.method === "tools/list");
  assert.equal(lists.length, 1, `the tool list was fetched ${lists.length} times`);
});

test("#93b AN UNREADABLE REGISTRY MUST NOT REFUSE — a failure to ask is not evidence", async () => {
  // #76's defect one surface over: if OUR fetch fails we have no basis to tell a customer their working
  // command does not exist. It must fall through, and say why.
  const { said, posts } = await session(["/whatever"], {
    get: async () => { throw new Error("offline"); },
    post: async (p, body) => {
      if (p === "/mcp" && body && body.method === "tools/list") throw new Error("offline");
      return serverPost(p, body);
    },
  });
  assert.ok(!/unknown command/.test(said), "it must NOT refuse on a registry it could not read");
  assert.match(said, /could not be read/, "and it must say the registry was unreadable");
  assert.ok(posts.find((p) => p.path === "/skill/run" || p.path === "/mcp"), "it must still try");
});

test("#93b a skill_ name with an unreadable SKILL list is unverified, with a readable one it refuses", () => {
  const commands = new Set(Object.keys(r.COMMANDS));
  assert.equal(known.classify("skill_nope", known.registry({ commands, skills: null, tools: new Set() })).verdict,
    "unverified");
  assert.equal(known.classify("skill_nope", known.registry({ commands, skills: new Set(["bug-hunt"]), tools: null })).verdict,
    "unknown", "a skill_ name can only ever be a skill — an unreadable TOOL list is irrelevant to it");
});

test("#93b an EMPTY registry is a real answer and DOES refuse — null and [] are different facts", () => {
  const commands = new Set(Object.keys(r.COMMANDS));
  assert.equal(known.classify("nope", known.registry({ commands, skills: new Set(), tools: new Set() })).verdict,
    "unknown");
  assert.equal(known.classify("nope", known.registry({ commands, skills: new Set(), tools: null })).verdict,
    "unverified");
});

// ── the render half — what the customer SEES ───────────────────────────────────

// The shapes below were captured from api.fatelabs.ca on 2026-08-02 with a real key. Each one used to
// render the empty string, after a real round-trip and a spinner.
const MEASURED = {
  sessions: { sessions: [{ id: "sess:38173971bdd8", title: "what do you know about this repo?",
                           started_at: "2026-08-02T08:11:30.706398+00:00", run_count: 5 }], count: 23 },
  resume: { id: "sess:38173971bdd8", title: "what do you know about this repo?",
            started_at: "2026-08-02T08:11:30.706398+00:00", members: ["a@b.c"], repos: [],
            run_count: 5, skill_count: 0, meaning: "", edited: false, runs: [], artifacts: [] },
  init: { wiki: "# Repository map\n\n1 files · 2 symbols.\n", repo: "journeyrepo", scope: "repo:journeyrepo" },
  scan: { findings: [{ path: "x.py", line: 2, severity: "error",
                       body: "Possible hardcoded secret (GitHub token) — move it to a secret store." }], count: 1 },
  improve: { proposals: [{ category: "security", file: "cli/bin/session-commands.js", line: 216,
                           title: "[dynamic-exec] eval/exec executes arbitrary code",
                           severity: "high", suggested_action: "Remediate before it ships.",
                           verdict: "confirmed" }] },
  verify: { grounded: false, verified: false, complete: false, scope_ask: true,
            unverified_reason: "multi-repo account, no scope signal — ask which repo",
            question: "Which repo should I check this against?", candidates: ["a", "b"],
            ungrounded: [], arity_errors: [], type_errors: [], third_party: [] },
  orchestra: { level: "read_only", count: 1, runs: [{ task: "name the modules", model: "m", tier: "cheap" }] },
};

test("#93 render — NO routed command renders a blank screen (the founder's 'it disappeared')", () => {
  const blank = [];
  for (const [command, res] of Object.entries(MEASURED)) {
    const out = r.renderAnswer(res, C, { command, now: Date.parse("2026-08-02T10:00:00Z") });
    if (!String(out || "").trim()) blank.push(command);
  }
  assert.deepEqual(blank, [], `these render NOTHING after a real round-trip: ${blank.join(", ")}`);
});

test("#93 render — each shape renders its OWN facts, not a generic placeholder", () => {
  const at = { now: Date.parse("2026-08-02T10:00:00Z") };
  assert.match(r.renderAnswer(MEASURED.sessions, C, { command: "sessions", ...at }), /sess:38173971bdd8/);
  assert.match(r.renderAnswer(MEASURED.sessions, C, { command: "sessions", ...at }), /of 23 sessions/);
  assert.match(r.renderAnswer(MEASURED.init, C, { command: "init", ...at }), /Repository map/);
  assert.match(r.renderAnswer(MEASURED.scan, C, { command: "scan", ...at }), /x\.py:2/);
  assert.match(r.renderAnswer(MEASURED.scan, C, { command: "scan", ...at }), /hardcoded secret/);
  assert.match(r.renderAnswer(MEASURED.improve, C, { command: "improve", ...at }), /dynamic-exec/);
  assert.match(r.renderAnswer(MEASURED.resume, C, { command: "resume", ...at }), /5 runs/);
  assert.match(r.renderAnswer(MEASURED.orchestra, C, { command: "orchestra", ...at }), /name the modules/);
});

test("#93 render — /verify uses the SHARED renderer, so the session and the command agree (E-030)", () => {
  const out = r.renderAnswer(MEASURED.verify, C, { command: "verify" });
  assert.match(out, /Which repo should I check this against/, "a scope ask is a QUESTION, not a verdict");
  assert.match(out, /Nothing was verified/, "and it must say nothing was verified");
  // The bucket the command exists for, and which the session never read.
  const tp = r.renderAnswer({ grounded: false, ungrounded: [], third_party: ["requests.fetch_all_pages"] },
                            C, { command: "verify" });
  assert.match(tp, /requests\.fetch_all_pages/, "a third-party fabrication must be visible");
});

test("#93 render — an UNKNOWN reply shape names its fields; blank output is not reachable", () => {
  const out = r.renderAnswer({ quokkas: 3, wombats: ["a"] }, C, { command: "somethingnew" });
  assert.ok(String(out).trim(), "the floor must never return an empty string");
  assert.match(out, /no renderer/);
  assert.match(out, /quokkas/, "it must name what the server actually sent");
});

test("#93 render — an empty body says so rather than printing nothing", () => {
  assert.match(r.renderAnswer({}, C, { command: "sessions" }), /No sessions yet/);
  assert.match(r.renderAnswer({}, C, { command: "zzz" }), /empty body/);
});

test("#93 render — a reply that DOES have an answer is untouched (no regression on the common path)", () => {
  const out = r.renderAnswer({ answer: "auth lives in auth.py", grounded: true,
                              sources: [{ file: "auth.py", line: 12 }] }, C, { command: "" });
  assert.match(out, /auth lives in auth\.py/);
  assert.match(out, /grounded/);
});

// ── the pure classifier, on its own ────────────────────────────────────────────

test("#93 didYouMean never invents a suggestion for a word that resembles nothing", () => {
  const reg = known.registry({ commands: new Set(["sessions", "status", "sweep"]),
                               skills: new Set(), tools: new Set() });
  assert.deepEqual(known.didYouMean("sesions", reg), ["sessions"]);
  assert.deepEqual(known.didYouMean("qqqqqqqq", reg), []);
});

test("#93 knownNames spans all three registries and prefixes skills the way the menu does", () => {
  const reg = known.registry({ commands: new Set(["help"]), skills: new Set(["bug-hunt"]),
                               tools: new Set(["blast_radius"]) });
  assert.deepEqual(known.knownNames(reg), ["blast_radius", "help", "skill_bug-hunt"]);
});

test("#93 replies.describe is the floor and always produces at least one line", () => {
  assert.ok(replies.describe({ a: 1 }, C).length);
  assert.ok(replies.describe({}, C).length);
  assert.ok(replies.describe(null, C).length);
});
