"use strict";
// The palette — brief §1.2. Estelle is a light red, and the constraint that shapes the module is that a
// brand red and an error red must never blur into each other.

const { test } = require("node:test");
const assert = require("node:assert");
const p = require("../bin/palette.js");

test("the brand and the failure colour are DIFFERENT", () => {
  // Two reds a shade apart is exactly the failure the brief warns about. Compared on the DECLARED CODES,
  // not on rendered output: with NO_COLOR every painter returns the bare string, so an output comparison
  // would pass on a broken palette. The first version of this test did exactly that and failed in CI —
  // which is the useful kind of red.
  assert.ok(p.distinct("brand", "fail"), "brand and failure are the same colour");
  assert.ok(p.distinct("fail", "warn"), "failure and warning are the same colour");
  assert.notStrictEqual(p.CODES.brand, p.CODES.fail);
});

test("COLOUR IS NEVER THE ONLY SIGNAL — every state carries a glyph", () => {
  // a colourblind reader, NO_COLOR, a CI log and a screenshot all lose the colour and must keep the meaning
  const stripped = (line) => line.replace(/\x1b\[[0-9;]*m/g, "");
  assert.match(stripped(p.status("ok", "gate clean")), /^✓ /);
  assert.match(stripped(p.status("fail", "gate blocked")), /^✗ /);
  assert.match(stripped(p.status("warn", "unverified")), /^! /);
  // and the three remain distinguishable with every escape code removed
  const marks = ["ok", "fail", "warn"].map((k) => stripped(p.status(k, "x"))[0]);
  assert.strictEqual(new Set(marks).size, 3, "two states share a glyph — colour would be the only signal");
});

test("teal is an ALIAS onto brand, so adopting the palette actually recolours the CLI", () => {
  // `teal` was a placeholder nobody chose; pointing it at brand is what changes the colour everywhere
  assert.strictEqual(p.teal, p.brand);
  assert.strictEqual(p.red, p.fail);
});

test("no glyph is an emoji — they must be single-width and align in a column", () => {
  for (const g of Object.values(p.GLYPH)) {
    assert.ok(!/\p{Extended_Pictographic}/u.test(g), `emoji glyph: ${g}`);
  }
});

test("every exported painter is a function that returns a string, colour on or off", () => {
  // the module is imported before anything checks isTTY, so a painter must never throw or return undefined
  for (const name of ["brand", "fail", "ok", "warn", "grey", "dim", "bold"]) {
    assert.strictEqual(typeof p[name], "function", `${name} is not a painter`);
    assert.strictEqual(typeof p[name]("x"), "string", `${name} did not return a string`);
    assert.ok(p[name]("x").includes("x"), `${name} lost its text`);
  }
});

test("a non-string survives painting rather than becoming undefined", () => {
  assert.ok(p.brand(42).includes("42"));
  assert.ok(p.fail(null).includes("null"));
});
