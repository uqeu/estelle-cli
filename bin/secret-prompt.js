"use strict";
// ONE way to read a credential from a human, for every path that accepts one.
//
// OBSERVED LIVE, 2026-08-01, on the FIRST command every customer runs. `estelle init` built its prompt with
// `readline.createInterface({ input: process.stdin, output: process.stdout })` — and an attached output in
// terminal mode ECHOES what is typed. The full `estelle_live_…` key was printed to the terminal, into
// scrollback, and into a screenshot. `repl.js` claimed in its own comment that the key is "never echoed
// back" while doing nothing to prevent it: one path claimed masking, neither did it.
//
// A pasted key arrives as ONE chunk, not as keystrokes, so a per-character handler that ignores chunking
// would drop most of it. That is the normal case here, not an edge case, and it is why this is a module
// with tests rather than three lines inlined at each prompt.
//
// Non-TTY input (a pipe, CI, every scripted test) is read as a plain line: there is no terminal echo to
// suppress, and forcing raw mode on a pipe throws. The behaviour a test exercises is therefore the same
// code path a human uses, minus the part a pipe cannot have.

const readline = require("readline");

const ENTER = /[\r\n]/;
const BACKSPACE = /[]/;
const CTRL_C = "";
const CTRL_D = "";

/**
 * Ask for a secret and return it WITHOUT echoing it. Resolves to the trimmed string ("" if nothing typed).
 *
 * `mask` is what stands in for each character on screen. "" prints nothing at all (what a password prompt
 * does); "*" gives the customer feedback that a paste landed. We use "*" — a first-run paste with no
 * visible response reads as a hung terminal, and that costs more than it buys.
 */
function askSecret(question, opts) {
  const { input = process.stdin, output = process.stdout, mask = "*" } = opts || {};
  if (!input.isTTY || typeof input.setRawMode !== "function") {
    // A pipe echoes nothing by construction. Read one line the ordinary way.
    const rl = readline.createInterface({ input, output, terminal: false });
    return new Promise((resolve) => {
      output.write(question);
      rl.question("", (answer) => { rl.close(); output.write("\n"); resolve(String(answer || "").trim()); });
    });
  }
  return new Promise((resolve) => {
    const wasRaw = input.isRaw;
    let buffer = "";
    output.write(question);
    input.setRawMode(true);
    input.resume();
    input.setEncoding("utf8");
    const finish = (value) => {
      input.removeListener("data", onData);
      input.setRawMode(Boolean(wasRaw));
      input.pause();
      output.write("\n");
      resolve(value);
    };
    const onData = (chunk) => {
      // Iterate the CHUNK, not a single keystroke: a paste is one chunk carrying the whole key.
      for (const ch of String(chunk)) {
        if (ENTER.test(ch)) return finish(buffer.trim());
        if (ch === CTRL_C || ch === CTRL_D) return finish("");
        if (BACKSPACE.test(ch)) {
          if (buffer.length) { buffer = buffer.slice(0, -1); if (mask) output.write("\b \b"); }
          continue;
        }
        if (ch < " " || ch === "") continue;             // ignore remaining control bytes
        buffer += ch;
        if (mask) output.write(mask);
      }
    };
    input.on("data", onData);
  });
}

module.exports = { askSecret };
