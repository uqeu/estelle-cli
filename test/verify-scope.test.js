"use strict";
// EVERY `/verify` CALLER CARRIES THE SCOPE — the REQUEST half of the gate, which stayed broken while the
// RESPONSE half of the very same call was hardened twice.
//
// `cli/bin/hook.js:417` is the always-on PreToolUse hook every customer installs with `estelle init`. It
// posted `{answer: code}` with NO `repo`. A multi-repo account cannot have its scope resolved server-side,
// so `grounding_ask()` answers
//     {"grounded": false, "reason": "multi-repo account, no scope signal - ask which repo", …}
// which `groundVerdict` reads — correctly, by design — as an ABSTENTION. The hook then printed
// "⚠ Estelle CANNOT verify …" and let the edit through. THE GROUNDING GATE, THE PRODUCT'S HEADLINE
// FEATURE, HAD NEVER ONCE RUN FOR A MULTI-REPO CUSTOMER.
//
// 🔴 WHY THIS GETS A FILE INSTEAD OF A LINE. Both hardening passes on this call read the ANSWER: the
// fail-closed ordering in `groundVerdict` exists because the hook once tested `report === null` only, and
// the abstention branch exists because an abstention used to read as clean. Nobody read the QUESTION — and
// the question guaranteed the answer would always be "cannot verify". A guard made rigorous about how it
// interprets a verdict, fed an input that ensured there would never be one. That is E-030's most expensive
// shape: two halves of one call, fixed at different times by people looking at different things. (E-043.)
//
// The comment at `estelle.js:1062` — "repoNameFor is the ONE definition, the same value sweep and both
// hooks write under (#23)" — has asserted this invariant since #23 was fixed, and it was false in three of
// three `/verify` callers the whole time. A comment cannot fail. The bottom half of this file can.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const h = require("../bin/hook.js");
const { repoNameFor } = require("../bin/repo-name.js");

// ── the behaviour: a real multi-repo account, both directions ──────────────────

/**
 * The server as it actually behaves for a multi-repo account: WITHOUT a `repo` in the body it cannot
 * answer, and says so in the shape prod really returns. That refusal is the whole defect — a double that
 * answers regardless of scope would make every test here pass with the bug present.
 */
function multiRepoServer(verdicts) {
  const seen = [];
  const post = async (route, body) => {
    seen.push({ route, body });
    if (!body || !body.repo) {
      return { grounded: false, reason: "multi-repo account, no scope signal - ask which repo",
               candidates: ["uqeu/estelle", "uqeu/other"] };
    }
    return verdicts[body.answer] || { grounded: true };
  };
  return { post, seen };
}

// ⛔ ONE DIRECTION PROVES NOTHING, and this is the test the founder asked for by name. "the hook stays
// silent" passes if the fix made everything read as verified — strictly WORSE than the bug, because the
// gate would then certify code it never checked. "the hook warns" passes if the fix made everything flag,
// which gets the gate muted in a day. Only the pair, against a server that answers the two inputs
// differently, distinguishes a working gate from a stuck one.
test("🔴 THE PAIR: on a multi-repo account a fabricated symbol is FLAGGED and real code PASSES", async () => {
  const FAKE = "svc.ghost_api()";
  const REAL = "json.dumps(payload)";
  const srv = multiRepoServer({
    [FAKE]: { grounded: false, ungrounded: ["ghost_api"] },
    [REAL]: { grounded: true },
  });

  const flagged = [];
  await h.runHook("ground", { tool_input: { file_path: "x.py", content: FAKE } },
    { out: (o) => flagged.push(o), post: srv.post });
  assert.equal(flagged.length, 1, "a fabricated symbol produced NO warning — the gate did not run");
  assert.match(flagged[0].systemMessage, /gate flagged/);
  assert.match(flagged[0].systemMessage, /ghost_api/);
  // Name the shipped failure so a regression cannot be mistaken for a pass: "CANNOT verify" IS the defect,
  // and it is a message this hook legitimately emits in other states, so it has to be excluded explicitly.
  assert.doesNotMatch(flagged[0].systemMessage, /CANNOT verify/,
    "the gate ABSTAINED instead of judging — the unscoped-request defect is back");

  const clean = [];
  await h.runHook("ground", { tool_input: { file_path: "x.py", content: REAL } },
    { out: (o) => clean.push(o), post: srv.post });
  assert.deepEqual(clean, [], "real code did not pass — a gate that flags everything is not a gate");
});

test("🔴 the ground hook SENDS a repo, resolved by the one definition", async () => {
  const srv = multiRepoServer({});
  await h.runHook("ground", { tool_input: { file_path: "x.py", content: "code" } },
    { out: () => {}, post: srv.post });
  assert.equal(srv.seen.length, 1, "the gate did not call /verify at all");
  assert.equal(srv.seen[0].route, "/verify");
  // POSITIVE FIRST, then the equality — otherwise `"" === ""` passes and the assertion is vacuous, which is
  // the failure mode a whole errata entry exists for. Inside this repo `repoNameFor` cannot be empty.
  assert.ok(srv.seen[0].body.repo, "the request carried no repo — this is the shipped defect");
  assert.equal(srv.seen[0].body.repo, repoNameFor(process.cwd()),
    "the gate must READ the namespace the sync hook WRITES — same resolver, never a second one");
});

test("the scope refusal is still reported LOUD — the fix must not paper over the fail-closed path", async () => {
  // Scoping the request removes the *cause* of the abstention; it must not remove the *handling* of one.
  // A server that still cannot resolve scope (a repo genuinely absent from the account) has to reach the
  // customer as "CANNOT verify", not as silence. This drives that branch directly, since `repoNameFor`
  // reads `process.cwd()` and cannot be made to answer empty from inside a test run in this repo — the
  // omission branch itself is covered structurally below, and behaviourally by `scoped()`'s test.
  const said = [];
  await h.runHook("ground", { tool_input: { file_path: "x.py", content: "code" } },
    { out: (o) => said.push(o),
      post: async () => ({ grounded: false, reason: "multi-repo account, no scope signal - ask which repo" }) });
  assert.equal(said.length, 1, "an unanswerable scope question must never be silent");
  assert.match(said[0].systemMessage, /CANNOT verify/);
});

// ── the invariant, as a test rather than a comment ─────────────────────────────

const VERIFY = /["']\/verify["']/;

/**
 * The call expression a `/verify` literal sits in: forward from the literal until its enclosing bracket
 * closes. Precise where a fixed line window is not — it captures a call that wraps and stops before an
 * unrelated neighbour, so neither a false pass nor a false violation can come from formatting.
 */
function expression(src, at) {
  let depth = 0;
  for (let i = at; i < src.length && i < at + 400; i++) {
    const ch = src[i];
    if (ch === "(" || ch === "{" || ch === "[") depth++;
    else if (ch === ")" || ch === "}" || ch === "]") {
      if (depth === 0) return src.slice(at, i);
      depth--;
    }
  }
  return src.slice(at, at + 400);
}

/**
 * Every `/verify` call site in one source, as `{file, line, expr}`.
 *
 * Comment lines are dropped LINE BY LINE, never with a block-comment regex — `repo-scope.test.js:155`
 * records why: `/\*[\s\S]*?\*\//g` ate the code as well as the prose, and the checker went green with the
 * defect present. A line is dropped only if it BEGINS as a comment, so a scanner cannot be blinded by its
 * own fix being documented above it.
 *
 * Matching the quoted literal exactly is what separates a call site from a display string: `scope-ask.js`
 * prints the hint "/verify <file> --repo owner/name", which is prose about the command, not a call to it.
 */
function sitesIn(file, raw) {
  const sites = [];
  raw.split("\n").forEach((line, i) => {
    if (/^\s*(\/\/|\*|\/\*)/.test(line)) return;
    const m = line.match(VERIFY);
    if (!m) return;
    sites.push({ file, line: i + 1, expr: expression(line, m.index) });
  });
  return sites;
}

/** The violations: a call site whose expression carries neither a `repo` nor the wrapper that adds one. */
function unscoped(sites) {
  return sites.filter((s) => !/\brepo\b/i.test(s.expr) && !/\bscoped\s*\(/.test(s.expr));
}

function allSites() {
  const dir = path.join(__dirname, "../bin");
  return fs.readdirSync(dir).filter((f) => f.endsWith(".js"))
    .flatMap((f) => sitesIn(f, fs.readFileSync(path.join(dir, f), "utf8")));
}

// PROVE THE INSTRUMENT CAN FAIL BEFORE BELIEVING IT PASSES. A structural checker that cannot go red is the
// exact thing this campaign keeps catching — a ratchet that has quietly stopped meaning anything. So the
// shipped defect is re-introduced here as a string and must be caught.
test("the scanner catches the shipped defect and clears both legitimate shapes", () => {
  const bad = `const r = await post("/verify", { answer: code }).catch(() => null);`;
  const direct = `const r = await post("/verify", repo ? { answer: code, repo } : { answer: code });`;
  const viaScoped = `case "verify": return { path: "/verify", body: scoped({ answer: arg }) };`;
  const prose = `lines.push(c.teal("/verify <file> --repo owner/name"));`;

  assert.deepEqual(unscoped(sitesIn("t.js", bad)).map((s) => s.line), [1],
    "the exact shipped defect scanned CLEAN — the instrument cannot fail");
  assert.deepEqual(unscoped(sitesIn("t.js", direct)), [], "an explicit repo must satisfy the invariant");
  assert.deepEqual(unscoped(sitesIn("t.js", viaScoped)), [], "the scoped() wrapper must satisfy it too");
  assert.deepEqual(sitesIn("t.js", prose), [], "a printed hint is not a call site");
  assert.deepEqual(sitesIn("t.js", `// post("/verify", { answer: code })`), [], "a comment is not a call site");
});

test("🔴 NO `/verify` CALLER IN cli/bin OMITS THE REPO", () => {
  const sites = allSites();
  // The paired positive. E-030's rule is that a seam change must ENUMERATE its consumers, so this test
  // knows how many there are: hook.js (the always-on gate), repl.js (the session command) and estelle.js
  // (`estelle verify`). If one disappears, that is a seam change and someone re-reads this line.
  assert.ok(sites.length >= 3,
    `the scanner found ${sites.length} /verify call sites, expected at least 3 — it is not reading cli/bin`);
  assert.deepEqual(unscoped(sites).map((s) => `${s.file}:${s.line}`), [],
    "a /verify caller sends no scope — on a multi-repo account the server cannot answer it");
});

test("`scoped()`, which the invariant accepts in place of a literal repo, really adds one", () => {
  // The structural test above trusts `scoped(…)` as a proxy for carrying the scope. That trust has to be
  // earned behaviourally, or the invariant could be satisfied by a wrapper that does nothing.
  const repl = require("../bin/repl.js");
  const route = repl.routeInput({ kind: "command", name: "verify", arg: "svc.ghost()" }, {},
                                { repo: "uqeu/estelle" });
  assert.equal(route.path, "/verify");
  assert.equal(route.body.repo, "uqeu/estelle", "the session's /verify must carry the header's repo");
  assert.equal(route.body.answer, "svc.ghost()");

  const none = repl.routeInput({ kind: "command", name: "verify", arg: "code" }, {}, { repo: "" });
  assert.ok(!none.body.repo, "an unresolvable repo must be omitted, never sent as a guess");
  assert.equal(none.body.answer, "code", "omitting the scope must not drop the payload");
});
