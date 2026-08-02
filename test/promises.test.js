"use strict";
// #84 AND #86 — WHAT THE CLI PROMISES A CUSTOMER MUST BE SOMETHING IT DOES.
//
// Two defects, one shape. Both are the standing rule's INVERSE — not "a capability with no door", but
// "a door with no capability", which reads to a customer as the product being broken.
//
//   #84  the header printed "/routing to see picks" ON EVERY LAUNCH and `routing` was NOT A COMMAND.
//        Typing it fell through to an MCP tools/call and came back "unknown tool". The first thing a
//        customer reads advertised something that errors.
//   #86  `estelle sweep --dry-run` SILENTLY IGNORED THE FLAG and uploaded the whole repository. `init`
//        honours it (estelle.js:246) and `reindex` honours it (:636) — so the flag reads as a safety net
//        on the ONE command where it does nothing.
//
// 🔴 THE TEST THAT GENERALISES IS THE COPY SCAN. Fixing `/routing` fixes one line; asserting that EVERY
// slash-token in the copy a customer actually reads resolves to a real command is what stops the next one.
// It reads the RENDERED OUTPUT rather than the source, per the founder's rule: a correct refusal followed
// by a success narrative still fails, and reading the code would never have caught it.

const test = require("node:test");
const assert = require("node:assert");
const repl = require("../bin/repl.js");

const C = new Proxy({}, { get: () => (s) => String(s) });

/** Every `/token` a customer could plausibly TYPE, pulled out of rendered copy. */
function slashTokens(text) {
  // `/word` not preceded by a path character, so `~/.estelle/auth.json` and `fatelabs.ca/dashboard` do not
  // register as commands. The point is what looks TYPEABLE on screen.
  return [...new Set([...String(text).matchAll(/(^|[\s(])\/([a-z][a-z0-9-]{1,20})\b/g)].map((m) => m[2]))];
}

test("🔴 #84 every slash command the HEADER advertises is a real command", () => {
  // The exact line that shipped: `routing   auto · 1 provider · /routing to see picks`.
  const rendered = repl.statusLines({
    email: "k@x.io", plan: "ultra", repo: "u/e", filed: ["u/e"], files: 10, memories: 20, providers: 1,
  }).map(([label, value]) => `${label} ${value}`).join("\n");
  const promised = slashTokens(rendered);
  assert.ok(promised.length, "the header must advertise at least one command, or this test is vacuous");
  const missing = promised.filter((n) => !(n in repl.COMMANDS));
  assert.deepEqual(missing, [], `the header promises ${missing.join(", ")} — not a command`);
});

test("#84 the same holds with NO provider configured — the other branch of the same line", () => {
  const rendered = repl.statusLines({ email: "k@x.io", providers: 0 })
    .map(([l, v]) => `${l} ${v}`).join("\n");
  const missing = slashTokens(rendered).filter((n) => !(n in repl.COMMANDS));
  assert.deepEqual(missing, [], `the header promises ${missing.join(", ")} — not a command`);
});

test("#84 /help and the slash menu derive from the SAME registry, so they cannot disagree", () => {
  // They already did share `COMMANDS` — this pins it, because the disagreement the founder saw was
  // between the CLI and the BRIEF, and the fix for that must not be to fork the CLI's own two views.
  const slashMenu = require("../bin/slash-menu.js");
  const wired = new Set(Object.keys(repl.COMMANDS).filter((n) => !repl.HELP_ONLY.has(n)));
  const menu = new Set(slashMenu.menuRows(repl.COMMANDS, wired, []).map((r) => r.name));
  for (const name of wired) assert.ok(menu.has(name), `${name} is wired but never appears in the menu`);
  for (const name of menu) assert.ok(name in repl.COMMANDS, `the menu shows ${name}, which is not declared`);
});

test("#84 /routing is wired and asks the server which model it would pick", () => {
  const route = repl.routeInput({ kind: "command", name: "routing", arg: "" }, {}, { repo: "" });
  assert.equal(route.path, "/route", "it must hit the real endpoint, not fall through to an MCP tool");
  assert.ok(!route.mcp, "falling through to tools/call is what produced 'unknown tool'");
});

test("🔴 #84 the routing receipt RENDERS — a wired command that draws nothing is still a door onto silence", () => {
  // The first fix wired `/routing` and the test asserted the ROUTE. Run live, it printed BLANK LINES:
  // renderAnswer knew nothing about POST /route's shape. Asserting the routing is asserting the source;
  // this asserts THE OUTPUT THE CUSTOMER SEES, which is the only thing that caught it.
  const real = { routed: true, provider: "openrouter", model: "openai/gpt-5.5", effort: "",
                 tier: "balanced", reason: "chat: default \u2192 balanced" };
  const out = repl.renderAnswer(real, C);
  assert.match(out, /openai\/gpt-5\.5/, "the customer must see WHICH model");
  assert.match(out, /openrouter/);
  assert.match(out, /balanced/);
  assert.ok(out.trim().length > 10, "a blank render is what shipped the first time");

  // `routed: false` is an ANSWER, not an absence — the provider has no tiers, so the default is used.
  const untiered = repl.renderAnswer({ routed: false, provider: "custom", model: "" }, C);
  assert.ok(untiered.trim().length > 10, "routed:false must still say something");
  assert.match(untiered, /default/i);
});

test("#84 `/route` is accepted as the same command — both spellings appear in our own copy", () => {
  const route = repl.routeInput({ kind: "command", name: "route", arg: "" }, {}, { repo: "" });
  assert.equal(route.path, "/route");
});

// ── #86: a safety flag that does nothing is worse than no flag ──────────────────

test("🔴 #86 sweep HONOURS --dry-run — the plan is printed and nothing is sent", async () => {
  // The defect: `cmdSweep` had no --dry-run handling at all, while `init` (estelle.js:246) and `reindex`
  // (:636) both honour it — and reindex's own comment says the stop "has to be deliberate" because
  // "uploading against an explicit don't is not a smaller mistake than a typo". So the ONE command that
  // uploads an entire repository was the one where the brake did nothing.
  //
  // Found by the walkthrough on its first live run, which started a real sweep of a 6,457-file repo
  // believing it was a dry run.
  const { execFile } = require("node:child_process");
  const path = require("node:path"), fs = require("node:fs"), os = require("node:os");
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-dry-"));
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-src-"));
  fs.writeFileSync(path.join(dir, "a.py"), "def hello():\n    return 1\n");

  const r = await new Promise((resolve) => {
    execFile(process.execPath, [path.join(__dirname, "../bin/estelle.js"), "sweep", "--dry-run", "--path", dir],
      { env: { ...process.env, HOME: home, ESTELLE_API_KEY: "estelle_live_dry_run_probe_000000",
               // Point every network path at a closed port. IF THE DRY RUN LEAKS A REQUEST, the command
               // hangs or errors on the connection — so this is a POSITIVE detector for a send, not just
               // an absence check. A dry run that touches the network cannot pass this quietly.
               ESTELLE_API_URL: "http://127.0.0.1:1", ESTELLE_MCP_URL: "http://127.0.0.1:1" },
        timeout: 30000, encoding: "utf8" },
      (err, stdout, stderr) => resolve({ code: err && typeof err.code === "number" ? err.code : (err ? 1 : 0),
                                         out: `${stdout || ""}${stderr || ""}` }));
  });
  fs.rmSync(home, { recursive: true, force: true });
  fs.rmSync(dir, { recursive: true, force: true });

  // IDENTITY FIRST: it must actually have RUN and found the file, or "nothing was sent" is vacuous — a
  // command that died at startup also sends nothing.
  assert.match(r.out, /Found/, `sweep did not run: ${r.out.slice(0, 200)}`);
  assert.match(r.out, /dry-run/i, "it must say the dry run stopped it");
  assert.doesNotMatch(r.out, /ECONNREFUSED|Updating your Estelle memory|Uploading/i,
    "a dry run must not touch the network");
  assert.equal(r.code, 0, "a dry run that stops cleanly is a success, not a failure");
});

test("#86 WITHOUT the flag it still tries to upload — the guard must not have disabled sweep", async () => {
  // The paired positive. Without this, "nothing was sent" could mean the guard broke sweep entirely, and
  // the test above would pass on a command that no longer works at all.
  const { execFile } = require("node:child_process");
  const path = require("node:path"), fs = require("node:fs"), os = require("node:os");
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-wet-"));
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-src2-"));
  fs.writeFileSync(path.join(dir, "a.py"), "def hello():\n    return 1\n");
  const r = await new Promise((resolve) => {
    execFile(process.execPath, [path.join(__dirname, "../bin/estelle.js"), "sweep", "--path", dir],
      { env: { ...process.env, HOME: home, ESTELLE_API_KEY: "estelle_live_dry_run_probe_000000",
               ESTELLE_API_URL: "http://127.0.0.1:1", ESTELLE_MCP_URL: "http://127.0.0.1:1" },
        timeout: 30000, encoding: "utf8" },
      (err, stdout, stderr) => resolve({ out: `${stdout || ""}${stderr || ""}` }));
  });
  fs.rmSync(home, { recursive: true, force: true });
  fs.rmSync(dir, { recursive: true, force: true });
  assert.doesNotMatch(r.out, /dry-run/i, "no flag was passed, so nothing should mention a dry run");
  assert.match(r.out, /ECONNREFUSED|could not|failed|unreachable|error/i,
    "without --dry-run sweep must actually attempt the network");
});

// ── #88: `verify` rendered a REFUSAL as a FINDING ───────────────────────────────
//
// 🔴 E-015'S DEFECT IN A THIRD CONSUMER. `/verify` returns `grounded: false` for BOTH "I found invented
// APIs" and "I could not check at all" — E-015 fixed that join in the Python hook AND the Node hook, and
// `cmdVerify` was never touched. Two of three consumers fixed is how a defect survives its own errata.
//
// Measured on prod for a file with NOTHING invented in it:
//     {grounded: false, scope_ask: true, ungrounded: [],
//      unverified_reason: "multi-repo account, no scope signal — ask which repo",
//      question: "Which repo should I check this against? …", candidates: [...]}
//
// The CLI printed "Ungrounded references (not defined in your repo):" and then NOTHING — a heading for an
// empty list — and threw away a complete question the server had already written. The customer is told
// their file references undefined APIs when the truth is "I need to know which repo".

test("🔴 #88 a scope_ask renders as a QUESTION, never as a finding", () => {
  const out = repl.verifyLines({
    grounded: false, scope_ask: true, ungrounded: [],
    unverified_reason: "multi-repo account, no scope signal — ask which repo",
    question: "Which repo should I check this against?", candidates: ["a/b", "c/d"],
  }, C).join("\n");
  assert.doesNotMatch(out, /Ungrounded references/i,
    "a refusal must never be rendered as a finding about the customer's file");
  assert.match(out, /which repo/i, "it must ask the question the server already wrote");
  assert.match(out, /a\/b/, "and list the candidates so the answer is actionable");
  assert.match(out, /--repo/, "and name the flag that resolves it");
});

test("#88 an ungrounded list with ENTRIES still renders as a finding — the guard must not swallow real ones", () => {
  const out = repl.verifyLines({ grounded: false, ungrounded: ["some_invented_fn"] }, C).join("\n");
  assert.match(out, /Not defined in your repo/i);
  assert.match(out, /some_invented_fn/);
});

test("🔴 #88 EVERY finding bucket renders — only `ungrounded` ever did", () => {
  // Measured on prod: `requests.fetch_all_pages` comes back with `third_party` POPULATED and `ungrounded`
  // EMPTY. The old code read one field of eight, so the case `verify` is most often pointed at rendered as
  // the heading "Ungrounded references:" and an empty list. Reading one field and treating its emptiness
  // as "nothing found" is the same shape as the defect above it.
  const real = { grounded: false, ungrounded: [], third_party: ["requests.fetch_all_pages"],
                 hallucination_spans: [{ symbol: "requests.fetch_all_pages", kind: "third_party" }] };
  const out = repl.verifyLines(real, C).join("\n");
  assert.match(out, /fetch_all_pages/, "a third-party fabrication must be shown");
  assert.doesNotMatch(out, /could not verify/i, "a real finding is not an inability to check");

  for (const bucket of ["arity_errors", "type_errors", "missing_required", "incomplete_work", "style_violations"]) {
    const o = repl.verifyLines({ grounded: false, [bucket]: ["marker_" + bucket] }, C).join("\n");
    assert.match(o, new RegExp("marker_" + bucket), `${bucket} is never rendered`);
    assert.doesNotMatch(o, /could not verify/i, `${bucket} renders as an inability to check`);
  }
});

test("#88 grounded renders as grounded", () => {
  const out = repl.verifyLines({ grounded: true, ungrounded: [] }, C).join("\n");
  assert.match(out, /Grounded/);
  assert.doesNotMatch(out, /Ungrounded references/i);
});

test("🔴 #88 grounded:false with an EMPTY list and no scope_ask says COULD NOT VERIFY, not 'ungrounded'", () => {
  // The third state, and the one that has no name in the current code. "I could not check" is not
  // "your file is wrong" — that is defect class 3 pointed at the customer.
  const out = repl.verifyLines({ grounded: false, ungrounded: [], reason: "repo not swept" }, C).join("\n");
  assert.doesNotMatch(out, /Ungrounded references/i);
  assert.match(out, /could not verify/i);
  assert.match(out, /repo not swept/, "the server's reason must reach the customer");
});

// ── #89, the display half: /routing reported the DEFAULT, not what your message got ────

test("🔴 #89 /routing with no argument routes YOUR LAST MESSAGE, not a generic default", () => {
  // The founder typed "hi", asked /routing, and read `chat: default → balanced`. The PRODUCT defect was
  // real (fixed in routing.py — a greeting now takes the cheap tier), but the DISPLAY was lying alongside
  // it: with no argument the command sent `{task_kind:"chat"}`, which asks "what does a generic chat turn
  // route to" — a different question from "what did MY message route to", answered in the same words.
  const route = repl.routeInput({ kind: "command", name: "routing", arg: "" }, {},
                                { repo: "", lastAsk: "hi" });
  assert.equal(route.path, "/route");
  assert.equal(route.body.prompt, "hi", "it must route the message the customer actually sent");
  assert.ok(!("task_kind" in route.body), "task_kind asks the generic question, not theirs");
});

test("#89 an explicit argument still wins over the last message", () => {
  const route = repl.routeInput({ kind: "command", name: "routing", arg: "refactor the auth module" }, {},
                                { repo: "", lastAsk: "hi" });
  assert.equal(route.body.prompt, "refactor the auth module");
});

test("🔴 #89 with NOTHING to route it says so, rather than showing a default that reads as an answer", () => {
  const route = repl.routeInput({ kind: "command", name: "routing", arg: "" }, {}, { repo: "", lastAsk: "" });
  assert.equal(route.defaultOnly, true,
    "the caller must be able to tell this is the generic default and say so to the customer");
  assert.equal(route.body.task_kind, "chat");
});
