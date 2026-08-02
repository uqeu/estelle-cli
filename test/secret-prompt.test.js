"use strict";
// THE KEY ECHO, observed live on 2026-08-01 on the first command every customer runs.
//
// `estelle init` built its prompt with `readline.createInterface({input: process.stdin, output:
// process.stdout})`. An attached output in terminal mode ECHOES what is typed, so the full
// `estelle_live_…` key was printed into the terminal, into scrollback, and into a screenshot. `repl.js`
// claimed in its own comment that the key is "never echoed back" while nothing enforced it.
//
// These tests assert the OUTPUT — what a human would see on screen — rather than which branch ran. The
// echo was invisible to every existing test precisely because no test read stdout during a key prompt.

const { test } = require("node:test");
const assert = require("node:assert");
const { PassThrough } = require("node:stream");
const { askSecret } = require("../bin/secret-prompt.js");

/** A stand-in for a terminal: raw-mode capable, isTTY true, and it records everything written. */
function fakeTty() {
  const input = new PassThrough();
  input.isTTY = true;
  input.isRaw = false;
  input.setRawMode = function (on) { this.isRaw = on; return this; };
  const written = [];
  const output = { write: (s) => { written.push(String(s)); return true; } };
  return { input, output, screen: () => written.join("") };
}

test("a typed key is NEVER written to the screen", async () => {
  const { input, output, screen } = fakeTty();
  const secret = "estelle_live_sk_do_not_echo_me";
  const answer = askSecret("key: ", { input, output });
  input.write(secret + "\n");
  assert.strictEqual(await answer, secret, "the value must still reach the caller");
  assert.ok(!screen().includes(secret), `the key appeared on screen:\n${screen()}`);
  assert.ok(screen().startsWith("key: "), "the prompt itself must still be shown");
});

test("a PASTE arrives as ONE chunk and survives whole", async () => {
  // This is the normal case, not an edge case: nobody types a 60-character key. A per-keystroke handler
  // that ignored chunking would drop all but the first character and look fine in a manual test.
  const { input, output } = fakeTty();
  const secret = "estelle_live_" + "a".repeat(48);
  const answer = askSecret("key: ", { input, output });
  input.write(secret + "\r");
  assert.strictEqual(await answer, secret);
});

test("the customer still sees that their paste landed", async () => {
  // A first-run prompt with NO visible response reads as a hung terminal, which costs more than it buys.
  const { input, output, screen } = fakeTty();
  const answer = askSecret("key: ", { input, output });
  input.write("abcde\n");
  await answer;
  assert.ok(screen().includes("*****"), `no masked feedback was shown:\n${screen()}`);
});

test("backspace erases a character instead of committing it", async () => {
  const { input, output } = fakeTty();
  const answer = askSecret("key: ", { input, output });
  input.write("abcX\n");
  assert.strictEqual(await answer, "abc");
});

test("ctrl-c abandons the prompt with an empty answer rather than hanging", async () => {
  const { input, output } = fakeTty();
  const answer = askSecret("key: ", { input, output });
  input.write("");
  assert.strictEqual(await answer, "");
});

test("raw mode is always restored, so the shell is not left broken", async () => {
  const { input, output } = fakeTty();
  const answer = askSecret("key: ", { input, output });
  input.write("abc\n");
  await answer;
  assert.strictEqual(input.isRaw, false, "raw mode was left on — the customer's terminal stops echoing");
});

test("a PIPE is read as a plain line — there is no terminal echo to suppress", async () => {
  // CI and every scripted test take this path; forcing raw mode on a pipe throws.
  const input = new PassThrough();
  input.isTTY = false;
  const written = [];
  const output = { write: (s) => { written.push(String(s)); return true; } };
  const answer = askSecret("key: ", { input, output });
  input.write("estelle_from_a_pipe_000000\n");
  input.end();
  assert.strictEqual(await answer, "estelle_from_a_pipe_000000");
  assert.ok(!written.join("").includes("estelle_from_a_pipe_000000"));
});
