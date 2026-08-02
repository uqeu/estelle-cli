"use strict";
// The composer and its status line — brief §1.3.

const { test } = require("node:test");
const assert = require("node:assert");
const c = require("../bin/composer.js");

test("the memory budget is OUR number, not a copied context window", () => {
  // Kimi shows the model's context window. Ours is the memory-token budget against the plan cap — a number
  // we already hold and have never shown, and the more useful one: a context window resets every turn,
  // a memory budget is what a customer buys.
  assert.strictEqual(c.budgetReadout(92, 250_000_000), "memory <0.1% (92/250.0M)");
  assert.strictEqual(c.budgetReadout(17_520_637, 600_000_000), "memory 2.9% (17.5M/600.0M)");
});

test("a budget with NO CAP renders nothing, never a percentage of an unknown", () => {
  // a percentage with no denominator is the kind of confident meaningless number this product refuses
  assert.strictEqual(c.budgetReadout(500, 0), "");
  assert.strictEqual(c.budgetReadout(500, undefined), "");
});

test("a small non-zero budget never rounds down to a flat 0.0%", () => {
  // "some" must not read as "none" — that is the same class as an empty result reading as a clean one
  assert.match(c.budgetReadout(1, 1_000_000), /<0\.1%/);
  assert.strictEqual(c.budgetReadout(0, 1_000_000), "memory 0.0% (0/1.0M)");
});

test("the branch carries dirty and ahead, and degrades to just the name", () => {
  assert.strictEqual(c.branchLabel({ branch: "main", dirty: true, ahead: 254 }), "main [± ↑254]");
  assert.strictEqual(c.branchLabel({ branch: "main", dirty: true }), "main [±]");
  assert.strictEqual(c.branchLabel({ branch: "main" }), "main");
  assert.strictEqual(c.branchLabel({}), "", "no branch means no segment, not an empty bracket");
});

test("empty segments are DROPPED, not rendered as placeholders", () => {
  // a status line full of dashes teaches nothing and costs a row
  const line = c.statusLine({ mode: "read", cwd: "~/x" });
  assert.ok(!line.includes("  -  ") && !line.includes("()"));
  assert.match(line, /^read   ~\/x   ctrl-j: newline$/);
});

test("the mode appears in the composer border AND the status line, from ONE source", () => {
  const frame = c.composer({ mode: "plan", modeWhat: "nothing is written", cwd: "~/e" }, 80);
  assert.match(frame.head, /input · plan/);          // Kimi's placement
  assert.match(frame.status, /^plan · nothing is written/);
  // one field in, two renderings out — the caller can never pass two different modes
  const both = c.composer({ mode: "auto", cwd: "~/e" }, 80);
  assert.match(both.head, /input · auto/);
  assert.match(both.status, /^auto/);
});

test("the frame is a real box at the width it was given, and never negative", () => {
  const frame = c.composer({ mode: "read" }, 80);
  assert.strictEqual(frame.head.length, 76);
  assert.strictEqual(frame.body.length, 76);
  assert.strictEqual(frame.foot.length, 76);
  // a tiny or absurd terminal must not throw or emit a negative repeat
  for (const w of [0, 1, 10, -5, NaN, undefined]) {
    assert.doesNotThrow(() => c.composer({ mode: "read" }, w), `width ${w}`);
  }
});

test("counts render identically on every machine", () => {
  // toLocaleString is locale-dependent; the CLI must print the same string everywhere
  assert.strictEqual(c.commas(16991), "16,991");
  assert.strictEqual(c.commas(0), "0");
  assert.strictEqual(c.compact(262_100), "262.1k");
  assert.strictEqual(c.compact(847), "847");
});

test("no emoji in the frame or the status line", () => {
  const frame = c.composer({ mode: "read", cwd: "~/x", git: { branch: "main", dirty: true } }, 80);
  const text = [frame.head, frame.body, frame.foot, frame.status].join("\n");
  assert.ok(!/\p{Extended_Pictographic}/u.test(text), text);
});
