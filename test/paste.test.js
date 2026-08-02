"use strict";
// BRACKETED PASTE — one of the two surfaces nobody had ever checked (founder, 2026-08-02).
// Measured before writing a line of it: `grep -rn 2004 cli/bin/` returned nothing, so a 20-line paste
// fired 20 turns. These pin the parser, the off-switch, and the finding underneath it.

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");

const paste = require(path.join(__dirname, "..", "bin", "paste.js"));
const inputUi = require(path.join(__dirname, "..", "bin", "input-ui.js"));

const S = paste.CODES.start, E = paste.CODES.end;

test("a completed paste is ONE event — not one per line", () => {
  const body = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
  const { events } = paste.parse(`${S}${body}${E}`);
  assert.strictEqual(events.length, 1);
  assert.strictEqual(events[0].kind, "paste");
  assert.strictEqual(events[0].text, body, "all 20 lines, in one event");
});

test("an INCOMPLETE paste is held, never emitted — a paste arrives across several reads", () => {
  const first = paste.parse(`${S}line one\nline two`);
  assert.deepStrictEqual(first.events, [], "half a snippet must not be submitted");
  const second = paste.parse(first.rest + `\nline three${E}`);
  assert.strictEqual(second.events.length, 1);
  assert.match(second.events[0].text, /line one\nline two\nline three/);
});

test("a START MARKER split across two reads is not typed into the composer as garbage", () => {
  const a = paste.parse("\x1b[20");
  assert.deepStrictEqual(a.events.filter((e) => e.kind === "keys"), [], "the partial marker is held back");
  const b = paste.parse(a.rest + `0~hello${E}`);
  assert.strictEqual(b.events[0].kind, "paste");
  assert.strictEqual(b.events[0].text, "hello");
});

test("ordinary keystrokes around a paste stay keystrokes, in order", () => {
  const { events } = paste.parse(`ab${S}X${E}cd`);
  assert.deepStrictEqual(events.map((e) => [e.kind, e.text]),
    [["keys", "ab"], ["paste", "X"], ["keys", "cd"]]);
});

test("newlines become spaces so a paste NEVER submits mid-block", () => {
  assert.strictEqual(paste.composerText("a\nb\nc"), "a b c");
  assert.strictEqual(paste.composerText("a\r\nb"), "a b", "a CRLF paste leaves no stray carriage return");
  assert.ok(!/\n/.test(paste.composerText("x\n".repeat(50))), "no newline may survive into the composer");
});

test("🔴 THE FINDING UNDER THE FINDING: collapsePaste was unreachable from a real paste", () => {
  // It takes text ALREADY assembled into one string. Without bracketed paste, readline submitted at the
  // first newline, so it never saw more than one line — a tested, exercised function the customer's
  // keystrokes could never reach. With the parser above it finally can.
  const body = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
  const { events } = paste.parse(`${S}${body}${E}`);
  const { visible, marks } = inputUi.collapsePaste(events[0].text, []);
  assert.match(visible, /Pasted ~20 lines/, "the collapse must finally fire");
  assert.strictEqual(inputUi.expandPastes(visible, marks), body, "and the FULL text still reaches Estelle");
});

test("🔴 THE MODE IS TURNED OFF ON RELEASE — or we leave the customer's terminal broken", () => {
  const written = [];
  const stdin = { isTTY: true, on() {}, removeListener() {} };
  const release = paste.attach(stdin, { write: (s) => written.push(s) });
  assert.ok(written.includes(paste.CODES.on), "it must turn the mode on");
  release();
  assert.ok(written.includes(paste.CODES.off), "and it must turn it OFF");
  const n = written.filter((w) => w === paste.CODES.off).length;
  release();
  assert.strictEqual(written.filter((w) => w === paste.CODES.off).length, n, "release is idempotent");
});

test("a NON-TTY never enables the mode — a pipe cannot paste and the codes would be noise", () => {
  const written = [];
  paste.attach({ isTTY: false, on() {}, removeListener() {} }, { write: (s) => written.push(s) });
  assert.deepStrictEqual(written, []);
});

test("SEAM: a pasted block reaches the composer as one insert, and is never submitted", () => {
  const handlers = {};
  const stdin = { isTTY: true, on: (k, fn) => { handlers[k] = fn; }, removeListener() {} };
  const inserted = [];
  paste.attach(stdin, { write: () => {}, insert: (t) => inserted.push(t) });
  handlers.data(`${S}alpha\nbeta${E}`);
  assert.deepStrictEqual(inserted, ["alpha beta"], "one insert, no submit");
});
