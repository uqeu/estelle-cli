"use strict";
// THE QUEUE — Codex has `pending_input_preview`; we had a blind FIFO drain.
// Founder: "it is loading all of the things I said, like it is in a backlog… Estelle doesn't have that
// queue. It needs to actually have a queue."

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const pending = require(path.join(__dirname, "..", "bin", "pending.js"));

const C = new Proxy({}, { get: () => (s) => String(s === undefined ? "" : s) });

test("nothing queued shows NOTHING — the preview must not become chrome", () => {
  assert.strictEqual(pending.preview([], C), "");
  assert.strictEqual(pending.preview(null, C), "");
});

test("the preview names the NEXT line, not just a count", () => {
  // "3 queued" tells a customer a number. Seeing the line is what lets them decide to keep or clear it.
  const out = pending.preview(["why is the sweep slow?", "and the gate?", "third"], C, 48);
  assert.match(out, /why is the sweep slow\?/);
  assert.match(out, /\+2 more/);
  assert.match(out, /esc to clear/, "a queue you cannot change is a backlog");
});

test("a long line is truncated, never wrapped into the transcript", () => {
  const out = pending.preview(["x".repeat(300)], C, 40);
  assert.ok(out.length < 120, "the preview must stay one line");
  assert.match(out, /…/);
});

test("ESC clears ONLY when something is queued and a turn is running", () => {
  assert.strictEqual(pending.escapeAction(["a"], true), "clear");
  assert.strictEqual(pending.escapeAction([], true), "", "idle ESC belongs to the slash menu");
  assert.strictEqual(pending.escapeAction(["a"], false), "", "not busy means nothing is waiting on a turn");
});

test("clearing SAYS how much was discarded — the customer typed it", () => {
  assert.match(pending.clearedLine(3, C), /cleared 3 queued lines/);
  assert.match(pending.clearedLine(1, C), /cleared 1 queued line\b/);
  assert.strictEqual(pending.clearedLine(0, C), "", "nothing cleared says nothing");
});

test("🔴 a line that waited too long is flagged for CONFIRMATION, never dropped", () => {
  // #101 measured a turn at 9-20s. Three impatient keystrokes is a minute of answers to things the
  // customer stopped caring about after the first. Dropping them silently would be worse than the backlog.
  const now = 100000;
  const s = pending.stale(["old", "new"], [now - 60000, now - 1000], now, 45000);
  assert.deepStrictEqual(s.map((x) => x.line), ["old"]);
});

test("THE PAIRED NEGATIVE — fresh input is never flagged", () => {
  // Without this the guard could 'pass' by confirming everything, which would make the queue useless.
  const now = 100000;
  assert.deepStrictEqual(pending.stale(["a", "b"], [now, now], now, 45000), []);
});

test("stale never DROPS — it only reports", () => {
  const now = 100000;
  const queued = ["a", "b"];
  pending.stale(queued, [0, 0], now, 1);
  assert.deepStrictEqual(queued, ["a", "b"], "the queue must be untouched by a read");
});
