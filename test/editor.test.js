"use strict";
// MODULE 2'S INPUT HALF — our own composer. "OWN THE COMPOSER. STOP BORROWING readline."
//
// The point of these is the LAST one: multi-line is a property of the buffer, so E-036 stops being a bug
// that was fixed and becomes a state that cannot be represented.

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const ed = require(path.join(__dirname, "..", "bin", "editor.js"));

const C = new Proxy({}, { get: () => (s) => String(s === undefined ? "" : s) });
const typed = (text) => ed.create({ text, cursor: text.length });

test("🔴 A PASTE IS ONE ENTRY WITH N LINES — E-036 becomes unrepresentable", () => {
  // readline submitted on every embedded \n BY DEFINITION. Here submission is a KEY, and a newline is
  // just a character in the buffer — so there is no code path that could submit mid-block.
  const body = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
  const s = ed.insertPaste(ed.create({}), body);
  assert.strictEqual(s.text, body, "all 20 lines, verbatim, in ONE buffer");
  assert.strictEqual(s.text.split("\n").length, 20);
  assert.strictEqual(ed.keyAction({ name: "return" }), "submit", "and only ENTER submits");
});

test("enter submits; shift+enter, alt+enter and ctrl-j insert a newline", () => {
  assert.strictEqual(ed.keyAction({ name: "return" }), "submit");
  assert.strictEqual(ed.keyAction({ name: "return", shift: true }), "newline");
  assert.strictEqual(ed.keyAction({ sequence: "\x1b[13;2u" }), "newline", "CSI-u shift+enter");
  assert.strictEqual(ed.keyAction({ sequence: "\x1b\r" }), "newline", "alt+enter");
  assert.strictEqual(ed.keyAction({ ctrl: true, name: "j" }), "newline");
});

test("a newline is inserted AT THE CURSOR, not appended", () => {
  const s = ed.apply(ed.create({ text: "ab", cursor: 1 }), "newline");
  assert.strictEqual(s.text, "a\nb");
  assert.strictEqual(s.cursor, 2);
});

test("printable characters insert; control bytes are DROPPED, never typed into the buffer", () => {
  assert.strictEqual(ed.keyAction({}, "x"), "insert");
  assert.strictEqual(ed.keyAction({ ctrl: true, name: "x" }, "\x18"), "", "a stray control byte is not text");
  assert.strictEqual(ed.keyAction({}, "\x1b"), "", "nor an escape");
});

test("editing: backspace, delete, kill-line, kill-word", () => {
  assert.strictEqual(ed.apply(typed("abc"), "backspace").text, "ab");
  assert.strictEqual(ed.apply(ed.create({ text: "abc", cursor: 0 }), "delete").text, "bc");
  assert.strictEqual(ed.apply(typed("hello world"), "kill-word").text, "hello ");
  assert.strictEqual(ed.apply(typed("hello"), "kill-line").text, "");
});

test("movement: left/right, word-jump, and home/end are PER LINE", () => {
  assert.strictEqual(ed.apply(typed("abc"), "left").cursor, 2);
  assert.strictEqual(ed.apply(typed("hello world"), "word-left").cursor, 6);
  assert.strictEqual(ed.apply(ed.create({ text: "hello world", cursor: 0 }), "word-right").cursor, 5);
  // multi-line: home goes to the start of THIS line, not the buffer
  const two = ed.create({ text: "first\nsecond", cursor: 9 });
  assert.strictEqual(ed.apply(two, "home").cursor, 6);
  assert.strictEqual(ed.apply(two, "end").cursor, 12);
});

test("history: ↑ walks back, ↓ returns, and the DRAFT survives the round trip", () => {
  let s = ed.create({ text: "half-typed", history: ["first", "second"] });
  s = ed.apply(s, "history-back");
  assert.strictEqual(s.text, "second", "↑ takes the most recent");
  s = ed.apply(s, "history-back");
  assert.strictEqual(s.text, "first");
  s = ed.apply(s, "history-forward");
  assert.strictEqual(s.text, "second");
  s = ed.apply(s, "history-forward");
  assert.strictEqual(s.text, "half-typed", "past the end restores what you were typing");
});

test("🔴 ↑ INSIDE A MULTI-LINE BUFFER MOVES THE CURSOR, not the history", () => {
  // Otherwise editing line 2 of a 20-line paste silently replaces the whole thing — data loss disguised
  // as a shortcut, and the customer would have no way to get it back.
  const s = ed.create({ text: "one\ntwo", cursor: 6, history: ["old"] });
  const after = ed.apply(s, "history-back");
  assert.strictEqual(after.text, "one\ntwo", "the buffer must be untouched");
  assert.strictEqual(after.cursor, 5);
});

test("history with an EMPTY list does nothing rather than blanking the line", () => {
  const s = ed.create({ text: "keep me", history: [] });
  assert.strictEqual(ed.apply(s, "history-back").text, "keep me");
});

test("render: multi-line indents under the label, and reports where the cursor is", () => {
  const r = ed.render(ed.create({ text: "one\ntwo", cursor: 5 }), { label: "›" }, C);
  assert.deepStrictEqual(r.lines, ["› one", "  two"]);
  assert.strictEqual(r.cursorRow, 1, "row 1 of the composer");
  assert.strictEqual(r.cursorCol, 3, "one char into 'two'");
});

test("render: a placeholder shows only when the buffer is empty", () => {
  const empty = ed.render(ed.create({}), { label: "›", placeholder: "ask about your code" }, C);
  assert.match(empty.lines[0], /ask about your code/);
  const full = ed.render(typed("hi"), { label: "›", placeholder: "ask about your code" }, C);
  assert.ok(!/ask about your code/.test(full.lines[0]));
});

test("apply NEVER mutates — the render and the buffer must not disagree mid-keystroke", () => {
  const before = typed("abc");
  const snapshot = JSON.stringify(before);
  ed.apply(before, "backspace");
  assert.strictEqual(JSON.stringify(before), snapshot);
});

test("ctrl-c, ctrl-d and escape are decisions for the CALLER, not buffer edits", () => {
  assert.strictEqual(ed.keyAction({ ctrl: true, name: "c" }), "cancel");
  assert.strictEqual(ed.keyAction({ ctrl: true, name: "d" }), "eof");
  assert.strictEqual(ed.keyAction({ name: "escape" }), "escape");
  const s = typed("abc");
  assert.strictEqual(ed.apply(s, "cancel").text, "abc", "the buffer is unchanged by a decision");
});

// ── THE KEY LOOP — attach() ────────────────────────────────────────────────────
// It reads keys and DOES NOT DRAW. That separation is the whole fix: readline both read and drew, so it
// fought every other writer, and symptoms d/e/f/g are all two things drawing on one screen.

function fakeTty() {
  const handlers = {};
  let raw = null;
  return {
    stdin: { isTTY: true, on: (k, fn) => { handlers[k] = fn; }, removeListener: () => {},
             setRawMode: (m) => { raw = m; } },
    handlers, get raw() { return raw; },
    readline: { emitKeypressEvents() {} },
  };
}

test("attach: typing changes the buffer and reports it — but writes NOTHING", () => {
  const t = fakeTty();
  const changes = [];
  const h = ed.attach(t.stdin, { readline: t.readline, onChange: (s) => changes.push(s.text) });
  t.handlers.keypress("h", { name: "h" });
  t.handlers.keypress("i", { name: "i" });
  assert.strictEqual(h.state.text, "hi");
  assert.deepStrictEqual(changes, ["h", "hi"]);
  h.close();
});

test("attach: ENTER submits once and clears; a bare Enter submits NOTHING", () => {
  const t = fakeTty();
  const submitted = [];
  const h = ed.attach(t.stdin, { readline: t.readline, onSubmit: (x) => submitted.push(x) });
  t.handlers.keypress("\r", { name: "return" });                 // empty — must not submit
  assert.deepStrictEqual(submitted, []);
  t.handlers.keypress("a", { name: "a" });
  t.handlers.keypress("\r", { name: "return" });
  assert.deepStrictEqual(submitted, ["a"]);
  assert.strictEqual(h.state.text, "", "the buffer clears after a submit");
  h.close();
});

test("🔴 attach: a 20-line PASTE submits ONCE, not twenty times", () => {
  // The acceptance criterion in one test. readline submitted per embedded newline BY DEFINITION.
  const t = fakeTty();
  const submitted = [];
  const h = ed.attach(t.stdin, { readline: t.readline, onSubmit: (x) => submitted.push(x) });
  h.paste(Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n"));
  assert.strictEqual(submitted.length, 0, "a paste must not submit anything on its own");
  t.handlers.keypress("\r", { name: "return" });
  assert.strictEqual(submitted.length, 1, "ONE submit");
  assert.strictEqual(submitted[0].split("\n").length, 20, "carrying all 20 lines");
  h.close();
});

test("attach: ctrl-c CLEARS a non-empty line and only EXITS on an empty one", () => {
  const t = fakeTty();
  let cancelled = 0;
  const h = ed.attach(t.stdin, { readline: t.readline, onCancel: () => { cancelled += 1; } });
  t.handlers.keypress("x", { name: "x" });
  t.handlers.keypress("\x03", { ctrl: true, name: "c" });
  assert.strictEqual(h.state.text, "", "it wipes the line");
  assert.strictEqual(cancelled, 0, "…and does NOT quit — people press it to abandon a thought");
  t.handlers.keypress("\x03", { ctrl: true, name: "c" });
  assert.strictEqual(cancelled, 1, "on an empty line it is a quit");
  h.close();
});

test("🔴 attach: RAW MODE IS RESTORED on close", () => {
  // Exiting with raw mode left on gives the customer a shell that does not echo what they type — the
  // same damage class as exiting inside the alternate screen, and just as hard to diagnose.
  const t = fakeTty();
  const h = ed.attach(t.stdin, { readline: t.readline });
  assert.strictEqual(t.raw, true, "it must enter raw mode to read keys");
  h.close();
  assert.strictEqual(t.raw, false, "and leave it as it found it");
});

test("attach: a NON-TTY returns an inert handle rather than making callers branch", () => {
  const h = ed.attach({ isTTY: false }, {});
  assert.strictEqual(h.state.text, "");
  assert.doesNotThrow(() => h.close());
});

// ── LAYERED KEY ROUTING — one reader, overlays first ──────────────────────────
// Four modules each bound their own keypress on process.stdin. Four readers of one stream, plus the
// composer, is why a paste that is ONE submit in isolation came back as three entries under a real pty.

test("🔴 an overlay sees keys FIRST and can claim them", () => {
  const t = fakeTty();
  const h = ed.attach(t.stdin, { readline: t.readline });
  const seen = [];
  h.keys.on("keypress", (ch, key) => { seen.push(key.name); return key.name === "up"; });
  t.handlers.keypress("\x1b[A", { name: "up" });      // claimed by the overlay
  t.handlers.keypress("x", { name: "x" });            // not claimed — reaches the buffer
  assert.deepStrictEqual(seen, ["up", "x"]);
  assert.strictEqual(h.state.text, "x", "a claimed key must NOT also edit the buffer");
  h.close();
});

test("an overlay that THROWS must not kill the session", () => {
  const t = fakeTty();
  const h = ed.attach(t.stdin, { readline: t.readline });
  h.keys.on("keypress", () => { throw new Error("overlay blew up"); });
  assert.doesNotThrow(() => t.handlers.keypress("a", { name: "a" }));
  assert.strictEqual(h.state.text, "a", "and the composer keeps working");
  h.close();
});

test("removeListener detaches an overlay", () => {
  const t = fakeTty();
  const h = ed.attach(t.stdin, { readline: t.readline });
  const fn = () => true;
  h.keys.on("keypress", fn);
  t.handlers.keypress("a", { name: "a" });
  assert.strictEqual(h.state.text, "", "claimed");
  h.keys.removeListener("keypress", fn);
  t.handlers.keypress("b", { name: "b" });
  assert.strictEqual(h.state.text, "b", "released");
  h.close();
});

test("the non-TTY handle has the SAME shape — callers never branch", () => {
  const h = ed.attach({ isTTY: false }, {});
  assert.strictEqual(typeof h.keys.on, "function");
  assert.doesNotThrow(() => { h.keys.on("keypress", () => {}); h.paste("x"); h.close(); });
});
