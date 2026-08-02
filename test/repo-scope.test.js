"use strict";
// #23 IN THE CUSTOMER PATH — the header and the answer must not disagree about scope.
//
// THE LIVE REPRODUCTION, on 0.1.10, on the founder's own account (`acct_1m3ad3xf2w`), four lines apart on
// ONE screen:
//
//   the HEADER said:  repo  estelle · 16991 memories
//   the ANSWER said:  "no repository has been filed under a name yet; there are only unfiled indexed
//                      items in the account inventory"       ...and cited omega_11 / omega_9 / omega_4
//
// Measured on prod while diagnosing it: `GET /repos` -> ["isoproof-bravo"] and `GET /overview` ->
// {memories: 16991, repo_files: 0}. So the ANSWER was telling the truth and the HEADER was the liar — it
// asserted a repo name that came from `path.basename(process.cwd())` and had never touched the server,
// then pinned an ACCOUNT-WIDE memory count to it.
//
// 🔴 THE TEST THAT MUST EXIST FOREVER, in the founder's words: sweep a repo, ask a question through the
// SAME door a customer uses, and assert the answer cites files FROM THAT REPO. Today 16,991 memories exist
// and the answer path sees none of them — SO A TEST THAT ONLY CHECKED "memories exist" WOULD PASS. That is
// defect class 3 again: unknown rendered as OK. These tests check the JOIN, never one half.

const test = require("node:test");
const assert = require("node:assert");
const repl = require("../bin/repl.js");
const repoName = require("../bin/repo-name.js");

// ── the resolver: one definition, four consumers ────────────────────────────────

test("🔴 the REPL resolves the repo through the SAME function as the sweep and the hooks", () => {
  // `repoNameFor` is the one definition (repo-name.js:47), created by E-015 for the sweep and both hooks.
  // The REPL header was a FOURTH consumer that did not use it — it used basename(cwd), which is why the
  // founder's screen said `estelle` while every writing path said `uqeu/estelle`. A name that disagrees
  // with the one memory was written under is the same defect as no name at all.
  assert.equal(typeof repl.resolveRepo, "function", "the REPL must expose its scope resolution");
  assert.equal(repl.resolveRepo("/tmp/does-not-exist-xyz"), repoName.repoNameFor("/tmp/does-not-exist-xyz"),
    "the REPL must not have its own idea of which repo it is in");
});

// ── the header: it may not claim what the server does not have ─────────────────

test("🔴 an UNFILED repo is labelled unindexed — the count is never pinned to a name the server lacks", () => {
  const line = repl.repoStatusLine({ repo: "uqeu/estelle", filed: ["isoproof-bravo"],
                                     files: 0, memories: 16991 });
  assert.equal(line.indexed, false);
  assert.match(line.value, /not indexed/i, "it must SAY the repo is not indexed");
  assert.doesNotMatch(line.value, /16991|16,991/,
    "an account-wide memory count must NOT be rendered as if it belonged to this repo");
  // Stronger than the original form of this test: the repo line carries NO count at all now, filed or not,
  // because both numbers are account-wide. Two true numbers under a false heading is still a false line.
  assert.doesNotMatch(repl.repoStatusLine({ repo: "u/e", filed: ["u/e"], files: 9, memories: 9 }).value,
    /\d/, "no count belongs on the repo line in either state");
  assert.match(repl.memoryStatusLine({ files: 0, memories: 16991 }), /this account, all repos/,
    "the counts must be labelled with the scope they actually have");
});

test("a FILED repo shows its name and its own counts", () => {
  const line = repl.repoStatusLine({ repo: "uqeu/estelle", filed: ["uqeu/estelle"],
                                     files: 2566, memories: 16991 });
  assert.equal(line.indexed, true);
  assert.match(line.value, /uqeu\/estelle/);
  assert.match(line.value, /indexed/);
});

test("filed-list membership tolerates the bare-name form the server also stores", () => {
  // `/repos` returned a BARE name (`isoproof-bravo`) beside owner/name entries, so membership cannot be a
  // naive `includes` or a correctly-swept repo reads as unindexed and nags the customer to re-sweep.
  const line = repl.repoStatusLine({ repo: "uqeu/estelle", filed: ["estelle"], files: 10, memories: 5 });
  assert.equal(line.indexed, true, "owner/name must match a bare filed name for the same repo");
});

test("🔴 UNKNOWN IS NOT OK: when the filed list could not be read, it says so and claims nothing", () => {
  // A failed /repos fetch must never render as "not indexed" — that is a claim about the server made from
  // a failure to ask, which is exactly the #76 defect one surface over.
  const line = repl.repoStatusLine({ repo: "uqeu/estelle", filed: null, files: 0, memories: 16991 });
  assert.equal(line.indexed, null, "unknown must be its own value, distinct from false");
  assert.match(line.value, /unknown/i);
  assert.doesNotMatch(line.value, /not indexed/i, "a failure to ASK is not evidence that it is unfiled");
});

test("no repo resolvable at all still renders something honest", () => {
  const line = repl.repoStatusLine({ repo: "", filed: [], files: 0, memories: 0 });
  assert.equal(typeof line.value, "string");
  assert.ok(line.value.length > 0);
});

// ── the ask: it must carry the same scope the header showed ────────────────────

test("🔴 THE JOIN: the ask sends the SAME repo the header names", () => {
  // The live defect: repl.js:553 sent {question} with NO repo, so the server fell back to the BASE
  // namespace and answered from unfiled stress junk while the header displayed a repo name. Two paths
  // resolving scope differently, with nothing responsible for making them agree.
  const route = repl.routeInput({ kind: "ask", text: "hi" }, {}, { repo: "uqeu/estelle" });
  assert.equal(route.path, "/deep-search");
  assert.equal(route.body.repo, "uqeu/estelle",
    "the answer must be scoped to the repo the header just claimed");
  assert.equal(route.body.question, "hi");
});

test("with no resolvable repo the ask omits it rather than sending a guess", () => {
  const route = repl.routeInput({ kind: "ask", text: "hi" }, {}, { repo: "" });
  assert.ok(!route.body.repo, "an empty repo must not be sent as a scope signal");
});

test("the /memory command is scoped the same way — it asks the same question of the same door", () => {
  const route = repl.routeInput({ kind: "command", name: "memory", arg: "" }, {}, { repo: "uqeu/estelle" });
  assert.equal(route.body.repo, "uqeu/estelle");
});

// ── the SEAM that 348 green tests never touched ─────────────────────────────────
//
// 🔴 PUBLISHED 0.1.10 CRASHED ON `/`. A customer pressing the first key of the release's headline feature
// got an uncaught TypeError and a full Node stack:
//
//     TypeError: Cannot read properties of undefined (reading 'has')
//         at bin/repl.js:340 -> Array.filter -> menuFor -> ReadStream.onKey (bin/slash-menu.js:162)
//
// ONE WORD. `local.HELP_ONLY` — but `local` is session-commands.js and HELP_ONLY is a constant in repl.js
// ITSELF, used correctly as a bare identifier fifty lines away. It reads as a copy-paste from neighbouring
// code that legitimately says `local.`.
//
// WHY IT SURVIVED 348 GREEN TESTS, which matters more than the fix: slash-menu.js was built as PURE
// FUNCTIONS (ordering, filtering, rendering) with keypress handling left in the REPL "because it needs
// readline". Both halves were tested. THE SEAM BETWEEN THEM — `menuFor` — WAS NEVER CALLED. The tests
// stopped exactly where the defect lives.
//
// That is our own standing rule proving itself on us: NO CAPABILITY IS DONE UNTIL A JOURNEY WALKS TO IT
// THROUGH A CUSTOMER DOOR. So this test does not check the menu's CONTENT — slash-menu.test.js already
// does that, and it passed throughout. It presses the key.

test("🔴 opening the slash menu does not throw — the seam, not either half", async () => {
  const repl2 = require("../bin/repl.js");
  let menuFor = null;
  await repl2.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}),
    post: async () => ({ answer: "ok" }),
    prompt: async () => null,                       // ctrl-d immediately; we only need the binding
    out: () => {}, c: new Proxy({}, { get: () => (s) => String(s) }),
    cwd: "estelle", now: () => Date.now(),
    // This is exactly how the real CLI hands the menu builder to the keypress handler
    // (`slash-menu.js:162` calls it on `/`). Capturing it here lets the test BE the keypress.
    bindMenu: (fn) => { menuFor = fn; return () => {}; },
  });
  assert.equal(typeof menuFor, "function", "the session must bind a menu builder");
  const rows = menuFor();                            // <-- the line that threw for every customer
  assert.ok(Array.isArray(rows) && rows.length > 0, "pressing / must produce rows, not an exception");
});

test("every `local.X` in repl.js exists on session-commands.js", () => {
  // The generalised form of the same defect, as a structural check rather than a spot fix: the crash was a
  // property read on the wrong module, and only a sweep can promise there is not a second one waiting on a
  // rarer path. Cheap, and it cannot go stale the way a hand-listed set would.
  const fs = require("node:fs"), path = require("node:path");
  const local = require("../bin/session-commands.js");
  const raw = fs.readFileSync(path.join(__dirname, "../bin/repl.js"), "utf8");
  // COMMENT LINES ARE DROPPED, line by line — and getting here took two wrong versions, both instructive.
  //   v1 scanned raw source and failed on the explanatory comment written directly ABOVE the fix. A
  //      checker that cannot survive its own fix being documented fails forever, and the tempting "fix"
  //      is to delete the explanation.
  //   v2 stripped block comments with /\*[\s\S]*?\*\//g first. That regex ate the CODE too — verified by
  //      counting: with the bug re-introduced the raw source held 2 occurrences and the stripped source
  //      held ZERO. So the checker went green while the defect was present, i.e. it had become the exact
  //      thing this campaign keeps catching: an instrument that cannot fail.
  // Line-based cannot over-reach: a line is dropped only if it BEGINS as a comment.
  const src = raw.split("\n").filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l)).join("\n");
  const used = [...new Set([...src.matchAll(/\blocal\.([A-Za-z_$][\w$]*)/g)].map((m) => m[1]))];
  // PROVE THE SCANNER CAN SEE ANYTHING AT ALL before believing an empty `missing` list — the paired
  // positive, without which "nothing missing" is indistinguishable from "nothing scanned".
  assert.ok(used.length >= 5, `the scanner found only ${used.length} local.X references — it is not reading the file`);
  const missing = used.filter((n) => !(n in local));
  assert.deepEqual(missing, [], `repl.js reads ${missing.join(", ")} off the wrong module`);
});

// ── typing the README's own command inside the session ──────────────────────────

test("🔴 `estelle sweep` inside the REPL suggests /sweep instead of spending a model call", () => {
  // Observed on 0.1.10: typing `estelle sweep` — THE COMMAND IN OUR OWN README — matched neither a slash
  // command nor `!<cmd>`, so it went to the model as a question and came back "I can't tell what 'estelle
  // sweep' refers to from the provided repository context." Technically correct and terrible: the customer
  // paid a model call to be told their own product's documented command is not a repo symbol.
  const hint = repl.commandHint("estelle sweep");
  assert.ok(hint, "the CLI must recognise its own command name");
  assert.match(hint.suggest, /^\/sweep$/);
  assert.equal(hint.kind, "slash");
});

test("the npx form and a leading `git`/`npm` are recognised too", () => {
  assert.match(repl.commandHint("npx @fatelabs/estelle gate").suggest, /^\/gate$/);
  const shell = repl.commandHint("git status");
  assert.equal(shell.kind, "shell", "a real shell command should point at the `!` form");
  assert.match(shell.suggest, /^!git status$/);
});

test("it stays silent on anything that is genuinely a question", () => {
  // The guard must not eat prose. "how does estelle sweep work" is a QUESTION about the command, and
  // answering it with a did-you-mean would be worse than the defect being fixed.
  assert.equal(repl.commandHint("how does estelle sweep work"), null);
  assert.equal(repl.commandHint("why is auth slow"), null);
  assert.equal(repl.commandHint("estelle blorp"), null, "an unknown subcommand is not a suggestion");
  assert.equal(repl.commandHint(""), null);
});
