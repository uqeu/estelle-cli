"use strict";
// The tab title, and the one lesson taken from Codex with it: cache the last title emitted, or a redraw
// writes an escape sequence every frame.

const { test, beforeEach } = require("node:test");
const assert = require("node:assert");
const { EventEmitter } = require("node:events");
const t = require("../bin/terminal-title.js");

function tty() {
  const written = [];
  return { isTTY: true, write: (s) => { written.push(String(s)); return true; }, seen: () => written.join("") };
}

beforeEach(() => t._resetTitleCache());

test("a TTY gets the OSC 0 sequence naming the app", () => {
  const out = tty();
  assert.strictEqual(t.setTitle("Estelle", out), true);
  assert.strictEqual(out.seen(), "\x1b]0;Estelle\x07");
});

test("a DUPLICATE title writes nothing at all", () => {
  // Codex keeps "the last terminal title emitted, to avoid writing duplicate OSC updates"
  // (codex-rs/tui/src/chatwidget.rs:724). Without the cache every redraw emits an escape sequence.
  const out = tty();
  t.setTitle("Estelle", out);
  const before = out.seen();
  assert.strictEqual(t.setTitle("Estelle", out), false, "it re-emitted an identical title");
  assert.strictEqual(out.seen(), before, "bytes were written for a title that had not changed");
});

test("a genuinely different title IS emitted — the cache must not wedge it", () => {
  // the paired positive: a cache that suppresses everything is worse than no cache.
  const out = tty();
  t.setTitle("Estelle", out);
  assert.strictEqual(t.setTitle("Estelle · sweeping", out), true);
  assert.ok(out.seen().endsWith("\x1b]0;Estelle · sweeping\x07"));
});

test("a PIPE never sees an escape sequence", () => {
  // a redirected stdout is a file or another program's stdin; an OSC there is corruption, not decoration.
  const written = [];
  const pipe = { isTTY: false, write: (s) => { written.push(s); return true; } };
  assert.strictEqual(t.setTitle("Estelle", pipe), false);
  assert.strictEqual(written.length, 0);
});

test("the tab is handed back on exit, however the session ends", () => {
  // `exit` is the one event that fires for an ordinary quit, a thrown error and Ctrl-C alike.
  const out = tty();
  const proc = new EventEmitter();
  proc.once = EventEmitter.prototype.once.bind(proc);
  const release = t.claimTitle("Estelle", { stream: out, proc });
  assert.ok(out.seen().includes("\x1b]0;Estelle\x07"), "the title was never claimed");
  proc.emit("exit", 0);
  assert.ok(out.seen().endsWith("\x1b]0;\x07"), "the customer's tab was left renamed after we quit");
  release();
});

test("releasing early restores the tab and detaches the exit handler", () => {
  const out = tty();
  const proc = new EventEmitter();
  const release = t.claimTitle("Estelle", { stream: out, proc });
  release();
  assert.ok(out.seen().endsWith("\x1b]0;\x07"));
  const after = out.seen();
  proc.emit("exit", 0);
  assert.strictEqual(out.seen(), after, "the exit handler fired after release");
});
