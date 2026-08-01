"use strict";
// THE TERMINAL HALF OF ERASURE — and specifically of its PROOF.
//
// The property these tests defend is that the CLI never makes an erasure look bigger or smaller than it
// was. Three ways that goes wrong, one test each: a zero-row result rendered as a success, an empty receipt
// list rendered as blank space (indistinguishable from a failed read), and an empty `reason` sent to a
// permanent, append-only record where it reads as "a reason was given and it was nothing".
const test = require("node:test");
const assert = require("node:assert");
const mem = require("../bin/memory.js");

test("a stamp renders compactly, and a missing one renders as nothing rather than 'now'", () => {
  assert.equal(mem.when("2026-07-31T19:11:04.123456+00:00"), "2026-07-31 19:11");
  assert.equal(mem.when(""), "");
  assert.equal(mem.when("not-a-date"), "");
  assert.equal(mem.when(undefined), "");
});

test("a receipt line names the scope, the target and who asked", () => {
  const line = mem.receiptLine({
    namespace: "estelle_live_abcdef01…", scope: "source", target: "src/pay.py", rows: 3,
    reason: "withdrawn upstream", requested_by: "dev@x.io", deleted_at: "2026-07-31T19:11:00+00:00",
  });
  assert.equal(line.scope, "source");
  assert.equal(line.target, "src/pay.py");
  assert.match(line.meta, /3 chunks/);
  assert.match(line.meta, /dev@x\.io/);
  assert.match(line.meta, /withdrawn upstream/);
});

test("a whole-namespace receipt falls back to the REDACTED namespace, never a raw one", () => {
  // The server redacts because on a solo account the namespace IS the primary API key. This renderer must
  // print what it was handed and never reconstruct anything.
  const line = mem.receiptLine({ namespace: "estelle_live_abcdef01…", scope: "namespace", rows: 1 });
  assert.equal(line.target, "estelle_live_abcdef01…");
  assert.match(line.meta, /1 chunk\b/);   // singular, not "1 chunks"
});

test("an empty receipt list SAYS why it is empty", () => {
  // Printing nothing here is indistinguishable from a read that failed, which is the exact ambiguity the
  // receipt endpoint exists to remove.
  const view = mem.receiptView({ receipts: [], count: 0 });
  assert.equal(view.lines.length, 0);
  assert.equal(view.total, 0);
  assert.match(view.empty, /No erasures on record/);
  assert.match(mem.receiptView(null).empty, /No erasures on record/);
});

test("the receipt view totals the chunks actually destroyed", () => {
  const view = mem.receiptView({ receipts: [{ rows: 4, scope: "namespace" }, { rows: 7, scope: "namespace" }] });
  assert.equal(view.total, 11);
  assert.equal(view.empty, "");
  assert.equal(view.lines.length, 2);
});

test("a zero-row erasure reads as a truthful nothing, not as a success", () => {
  const line = mem.erasureLine({ retracted: "key:ghost", purged: 0, namespaces: [] }, "Retracted");
  assert.equal(line.rows, 0);
  assert.match(line.text, /Nothing live under "key:ghost"/);
  assert.doesNotMatch(line.text, /Retracted/);
});

test("a real erasure names the subject, the rows and how many namespaces it covered", () => {
  const line = mem.erasureLine(
    { retracted: "key:deploy-target", purged: 5, namespaces: [{}, {}] }, "Retracted");
  assert.equal(line.rows, 5);
  assert.match(line.text, /Retracted "key:deploy-target" — 5 chunks across 2 namespaces\./);
  const one = mem.erasureLine({ forgotten: "src/pay.py", purged: 1, namespaces: [{}] }, "Forgot");
  assert.match(one.text, /Forgot "src\/pay\.py" — 1 chunk across 1 namespace\./);
});

test("a HALF-DONE retraction never prints as a completed one", () => {
  // the server reports `partial` when only one of the two stores could be confirmed closed. Printing the
  // ✓ line there would tell a customer a claim was withdrawn while POST /facts still answers it.
  const line = mem.erasureLine(
    { retracted: "key:hair", purged: 1, namespaces: [{}], claim_closed: false, recall_cleared: true,
      partial: true, warning: "this retraction did NOT complete — the decision store (POST /facts) may still serve this claim." },
    "Retracted");
  assert.equal(line.partial, true);
  assert.match(line.text, /did NOT complete/);
  assert.match(line.text, /POST \/facts/);
  // and a fully-completed one is unchanged
  const ok = mem.erasureLine(
    { retracted: "key:hair", purged: 1, namespaces: [{}], claim_closed: true, recall_cleared: true },
    "Retracted");
  assert.equal(ok.partial, undefined);
  assert.match(ok.text, /Retracted "key:hair" — 1 chunk across 1 namespace\./);
});

test("a blank reason is omitted rather than written into a permanent record as an empty string", () => {
  assert.deepEqual(mem.retractBody("key:x", ""), { subject: "key:x" });
  assert.deepEqual(mem.retractBody("  key:x  ", "   "), { subject: "key:x" });
  assert.deepEqual(mem.retractBody("key:x", " withdrawn "), { subject: "key:x", reason: "withdrawn" });
});

test("an empty learned list says nothing is being applied, rather than printing blank", () => {
  assert.match(mem.learnedView({ instincts: [] }).empty, /has not graduated any reflexes/);
  assert.match(mem.learnedView(null).empty, /has not graduated any reflexes/);
  const view = mem.learnedView({ instincts: [{ name: "auth-first", summary: "run the security suite" }] });
  assert.equal(view.empty, "");
  assert.deepEqual(view.lines, [{ name: "auth-first", summary: "run the security suite" }]);
});

test("unlearn never sends both a skill and a reflex — the server refuses that body", () => {
  assert.deepEqual(mem.unlearnBody("test-gen", "when x", "do y"), { skill: "test-gen" });
  assert.deepEqual(mem.unlearnBody("", "when x", "do y"), { instinct: { trigger: "when x", response: "do y" } });
});

test("unlearn refuses to guess when the caller named nothing usable", () => {
  // A half-specified reflex must not be sent: an instinct is keyed by the PAIR, and sending one half would
  // either 400 or, worse, match nothing and report a confident no-op.
  assert.equal(mem.unlearnBody("", "when x", ""), null);
  assert.equal(mem.unlearnBody("", "", "do y"), null);
  assert.equal(mem.unlearnBody("", "--skill", "test-gen"), null);
  assert.equal(mem.unlearnBody("", undefined, undefined), null);
});

test("a revoke that matched nothing reads as a truthful no-op, not a failure", () => {
  const miss = mem.unlearnLine({ unlearned: "instinct", trigger: "a", response: "b", removed: false });
  assert.equal(miss.removed, false);
  assert.match(miss.text, /No such reflex/);
  const hit = mem.unlearnLine({ unlearned: "instinct", trigger: "a", response: "b", removed: true });
  assert.equal(hit.removed, true);
  assert.match(hit.text, /Revoked "a → b"/);
  assert.match(mem.unlearnLine({ unlearned: "skill", skill: "test-gen", removed: true }).text, /back to unproven/);
  assert.match(mem.unlearnLine({ unlearned: "skill", skill: "test-gen", removed: false }).text, /Nothing learned/);
});

test("the receipts path only carries a limit when one was actually given", () => {
  assert.equal(mem.receiptsPath(""), "/deletion-receipts");
  assert.equal(mem.receiptsPath("abc"), "/deletion-receipts");
  assert.equal(mem.receiptsPath(0), "/deletion-receipts");
  assert.equal(mem.receiptsPath(-5), "/deletion-receipts");
  assert.equal(mem.receiptsPath("25"), "/deletion-receipts?limit=25");
  assert.equal(mem.receiptsPath(25.7), "/deletion-receipts?limit=25");
});
