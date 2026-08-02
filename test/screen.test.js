"use strict";
// THE SCROLLBACK MODEL — tests for bin/screen.js.
//
// ⛔ EVERY ASSERTION HERE IS ABOUT THE MODEL, NEVER ABOUT RENDERED BYTES. That is the whole discipline of
// this file and it is inherited from palette.js, which learned it the hard way: with colour disabled every
// painter returns the bare string, so an OUTPUT comparison reports every role identical and the test goes
// green while a human sees two identical reds.
//
// The alt-screen trap is the same shape one layer up — **comparing what was WRITTEN proves nothing about
// what is ON SCREEN.** So the model answers "which lines are visible, in which order, at what offset" as
// DATA, and the tests read that data. The terminal boundary (which escape codes, written when) is asserted
// separately in altscreen.test.js against DECLARED constants.

const test = require("node:test");
const assert = require("node:assert");
const s = require("../bin/screen.js");

const ESC = "\x1b[38;5;210m", OFF = "\x1b[0m";

test("displayWidth ignores SGR escapes — a coloured word is as wide as the word", () => {
  assert.equal(s.displayWidth("hello"), 5);
  assert.equal(s.displayWidth(`${ESC}hello${OFF}`), 5);
  assert.equal(s.displayWidth(""), 0);
  // PAIRED POSITIVE: the escape really is in the string, so "width 5" is not measuring an empty input.
  assert.ok(`${ESC}hello${OFF}`.length > 5, "fixture must actually contain escapes");
});

test("displayWidth counts a wide CJK char as two columns", () => {
  assert.equal(s.displayWidth("中文"), 4);
  assert.equal(s.displayWidth("a中"), 3);
});

test("wrap: a short line is returned unchanged, as one row", () => {
  assert.deepEqual(s.wrap("abc", 10), ["abc"]);
});

test("wrap: an empty line is ONE empty row, never zero rows", () => {
  // A blank line is a blank line — dropping it would silently close up the spacing the REPL prints.
  assert.deepEqual(s.wrap("", 10), [""]);
});

test("wrap: a long line breaks at the width, losing nothing", () => {
  const rows = s.wrap("abcdefghij", 4);
  assert.deepEqual(rows, ["abcd", "efgh", "ij"]);
  assert.equal(rows.join(""), "abcdefghij", "no character may be dropped by wrapping");
});

test("wrap: it prefers a word boundary when one is available", () => {
  assert.deepEqual(s.wrap("alpha beta", 7), ["alpha", "beta"]);
});

test("wrap: a break inside a coloured run REOPENS the colour on the next row", () => {
  const rows = s.wrap(`${ESC}abcdefgh${OFF}`, 4);
  assert.equal(rows.length, 2);
  // Assert on the DECLARED code appearing in the continuation row — not on how it renders.
  assert.ok(rows[1].startsWith(ESC), "the continuation row must reopen the active SGR");
  assert.equal(s.displayWidth(rows[0]), 4);
  assert.equal(s.displayWidth(rows[1]), 4);
  // And the escape is never split across rows, which would emit garbage to the terminal.
  for (const r of rows) assert.ok(!/\x1b\[[0-9;]*$/.test(r), "an escape sequence was cut in half");
});

test("append: lines land in order and the view stays pinned to the bottom", () => {
  let v = s.create({ max: 100 });
  v = s.append(v, "one", 20);
  v = s.append(v, "two", 20);
  const view = s.visible(v, 10);
  assert.deepEqual(view.lines, ["one", "two"]);
  assert.equal(view.atBottom, true);
  assert.equal(view.hiddenBelow, 0);
});

test("visible: shows the LAST `height` rows when pinned to the bottom", () => {
  let v = s.create({ max: 100 });
  for (const n of ["1", "2", "3", "4", "5"]) v = s.append(v, n, 20);
  assert.deepEqual(s.visible(v, 3).lines, ["3", "4", "5"]);
});

test("🔴 THE DEFECT §2.3 EXISTS TO FIX: appending while scrolled up must NOT move the reader", () => {
  let v = s.create({ max: 100 });
  for (const n of ["1", "2", "3", "4", "5"]) v = s.append(v, n, 20);
  v = s.scroll(v, -2, 3);                       // scroll UP two rows
  const before = s.visible(v, 3).lines;
  assert.deepEqual(before, ["1", "2", "3"]);

  v = s.append(v, "6", 20);                     // output arrives while the reader is up here
  const after = s.visible(v, 3);
  assert.deepEqual(after.lines, before, "the visible window must not shift under the reader");
  assert.equal(after.atBottom, false);
  assert.equal(after.hiddenBelow, 3, "and it must say how much is waiting below");
});

test("scroll: clamps at the top and cannot go below the bottom", () => {
  let v = s.create({ max: 100 });
  for (const n of ["1", "2", "3"]) v = s.append(v, n, 20);
  v = s.scroll(v, -999, 2);
  assert.deepEqual(s.visible(v, 2).lines, ["1", "2"], "cannot scroll past the first line");
  v = s.scroll(v, 999, 2);
  assert.deepEqual(s.visible(v, 2).lines, ["2", "3"], "cannot scroll past the last line");
  assert.equal(s.visible(v, 2).atBottom, true);
});

test("toBottom returns to live, whatever the offset was", () => {
  let v = s.create({ max: 100 });
  for (const n of ["1", "2", "3", "4"]) v = s.append(v, n, 20);
  v = s.scroll(v, -3, 2);
  assert.equal(s.visible(v, 2).atBottom, false);
  v = s.toBottom(v);
  assert.equal(s.visible(v, 2).atBottom, true);
  assert.deepEqual(s.visible(v, 2).lines, ["3", "4"]);
});

test("the buffer is BOUNDED — oldest rows are evicted and the eviction is counted, not hidden", () => {
  let v = s.create({ max: 3 });
  for (const n of ["1", "2", "3", "4", "5"]) v = s.append(v, n, 20);
  assert.equal(v.lines.length, 3);
  assert.deepEqual(s.visible(v, 3).lines, ["3", "4", "5"]);
  assert.equal(v.dropped, 2, "an unbounded buffer is a memory leak; a silent one is worse");
});

test("eviction while scrolled up does not teleport the reader past the dropped rows", () => {
  let v = s.create({ max: 4 });
  for (const n of ["1", "2", "3", "4"]) v = s.append(v, n, 20);
  v = s.scroll(v, -2, 2);
  assert.deepEqual(s.visible(v, 2).lines, ["1", "2"]);
  v = s.append(v, "5", 20);                     // evicts "1"
  // "1" is genuinely gone, so the honest result is the top of what remains — never a silent jump to the end.
  const view = s.visible(v, 2);
  assert.equal(view.atBottom, false);
  assert.deepEqual(view.lines, ["2", "3"]);
});

test("append is IMMUTABLE — the input state is never mutated", () => {
  const a = s.append(s.create({ max: 10 }), "one", 20);
  const snapshot = JSON.parse(JSON.stringify(a));
  const b = s.append(a, "two", 20);
  assert.deepEqual(JSON.parse(JSON.stringify(a)), snapshot, "append mutated its argument");
  assert.notEqual(a, b);
  assert.equal(s.visible(b, 5).lines.length, 2);
});

test("a multi-line append becomes multiple rows", () => {
  let v = s.create({ max: 100 });
  v = s.append(v, "a\nb\nc", 20);
  assert.deepEqual(s.visible(v, 5).lines, ["a", "b", "c"]);
});

test("reflow: a width change re-wraps the retained text instead of leaving broken rows", () => {
  let v = s.create({ max: 100 });
  v = s.append(v, "alpha beta gamma", 80);
  assert.equal(s.visible(v, 5).lines.length, 1);
  v = s.reflow(v, 10);
  const rows = s.visible(v, 5).lines;
  assert.ok(rows.length > 1, "a narrower terminal must re-wrap, not clip");
  for (const r of rows) assert.ok(s.displayWidth(r) <= 10, `row wider than the terminal: ${JSON.stringify(r)}`);
});

test("hiddenAbove reports what is off the top, so an indicator can be honest", () => {
  let v = s.create({ max: 100 });
  for (const n of ["1", "2", "3", "4", "5"]) v = s.append(v, n, 20);
  assert.equal(s.visible(v, 2).hiddenAbove, 3);
  assert.equal(s.visible(v, 5).hiddenAbove, 0);
});

test("a viewport taller than the content shows everything and still reads atBottom", () => {
  let v = s.create({ max: 100 });
  v = s.append(v, "only", 20);
  const view = s.visible(v, 10);
  assert.deepEqual(view.lines, ["only"]);
  assert.equal(view.atBottom, true);
  assert.equal(view.hiddenAbove, 0);
  assert.equal(view.hiddenBelow, 0);
});
