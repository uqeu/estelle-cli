"use strict";
// 🔴 THE TEST THAT MAKES IT PERMANENT INSTEAD OF A FIFTH ROUND.
//
// Founder ruling 2026-08-02: "Add a test that greps the source and FAILS on a direct
// process.stdout.write or console.log outside those two files, with an explicit allowlist for the few
// that are genuinely correct. That test is what makes this permanent instead of a fifth round."
//
// THE COMPLETION CRITERION:
//    NOTHING IN cli/bin WRITES TO STDOUT OR BINDS A KEY EXCEPT THROUGH MODULE 1 OR MODULE 2.
//
// ⛔ IT IS A RATCHET, NOT A CLIFF, AND THAT IS DELIBERATE. The audit found 250 items; migrating
// estelle.js's console.log surface took it to 36. Asserting ZERO today would be 36 red assertions that
// nobody can land in one commit, and a red suite stops being read within a day — which is how the
// original 250 accumulated. So the ceiling is a NUMBER THAT MAY ONLY GO DOWN. Every migration lowers it;
// a new direct write raises it and fails immediately.
//
// This is the same shape as tests/reachability_ledger.py's UNWIRED_GAP_CEILING, for the same reason.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const BIN = path.join(__dirname, "..", "bin");

// ⛔ THIS NUMBER MAY ONLY EVER DECREASE.
//
//   250  2026-08-02  the audit, before any migration
//    36  2026-08-02  estelle.js's 222 console.log sites migrated to Module 1's writer
//    24  2026-08-02  estelle.js's 13 stdout.write sites — taken as ONE PASS, not incidentally, because
//                     every one was a live instance of a defect already paid for twice
const CEILING = 24;

// The two modules allowed to write and bind. Everything else is a caller.
// Module 2 is TWO files: `ask.js` (the question surface) and `editor.js` (the composer — Module 2's
// input half, built when the founder ruled "OWN THE COMPOSER"). Both ARE the module.
const MODULES = new Set(["transcript.js", "ask.js", "editor.js"]);

// Files that own a terminal primitive BY DESIGN — the implementation Modules 1/2 delegate to. Declared
// with a reason each, so the exemption is a decision rather than an oversight.
const PRIMITIVES = {
  "altscreen.js": "owns the alternate screen and cursor codes; Module 1 paints through it",
  "screen.js": "pure viewport model — writes nothing",
  "palette.js": "returns colour strings, never writes",
  "paste.js": "owns bracketed-paste mode and the stream readline reads",
  "secret-prompt.js": "the masked reader Module 2's `secret` kind delegates to",
};

const PATTERNS = [
  ["stdout.write", /process\.stdout\.write\s*\(/],
  ["console.log", /console\.log\s*\(/],
  ["console.error", /console\.error\s*\(/],
  ["console.warn", /console\.warn\s*\(/],
  ["keypress bind", /\.on\(\s*["']keypress["']/],
  ["emitKeypressEvents", /emitKeypressEvents\s*\(/],
  ["readline listener", /\.on\(\s*["'](line|close|SIGINT|SIGTSTP|data)["']/],
  ["rl.prompt/write", /\brl\.(prompt|write|setPrompt)\s*\(/],
  ["cursor escape", /\\x1b\[[0-9;?]*[A-Za-z]/],
  ["setRawMode", /setRawMode\s*\(/],
];

function unmigrated() {
  const found = [];
  for (const file of fs.readdirSync(BIN).filter((f) => f.endsWith(".js")).sort()) {
    if (MODULES.has(file) || PRIMITIVES[file]) continue;
    const src = fs.readFileSync(path.join(BIN, file), "utf8").split("\n");
    src.forEach((line, i) => {
      // A comment explaining an escape code is documentation, not behaviour. Counting it would inflate
      // the number and make the ratchet meaningless.
      const code = line.replace(/\/\/.*$/, "").replace(/^\s*\*.*$/, "");
      if (!code.trim()) return;
      for (const [kind, re] of PATTERNS) if (re.test(code)) found.push(`${file}:${i + 1} ${kind}`);
    });
  }
  return found;
}

test("🔴 the migration ratchet — direct writes and key bindings may only DECREASE", () => {
  const items = unmigrated();
  assert.ok(items.length <= CEILING,
    `${items.length} direct writes/bindings outside Modules 1 and 2, ceiling is ${CEILING}.\n`
    + `A NEW direct write is the defect this catches — route it through transcript.js or ask.js.\n`
    + items.slice(0, 12).map((x) => `  ${x}`).join("\n"));
});

test("the ceiling is HONEST — it must not sit above the real count", () => {
  // A ceiling nobody lowers is a ceiling that stops meaning anything. If the count has dropped, this
  // fails and forces CEILING down with it — the ratchet tightens rather than drifting.
  const items = unmigrated();
  assert.strictEqual(CEILING, items.length,
    `the real count is ${items.length} but CEILING says ${CEILING} — lower it, with a dated line.`);
});

test("estelle.js no longer writes to stdout via console.log — 222 sites migrated", () => {
  const src = fs.readFileSync(path.join(BIN, "estelle.js"), "utf8");
  const direct = (src.match(/console\.log\s*\(/g) || []).length;
  assert.strictEqual(direct, 0, `${direct} console.log calls remain in the top-level command surface`);
});

test("THE PAIRED POSITIVE — Module 1's writer is byte-identical to console.log", () => {
  // The migration is only verifiable because it preserves output exactly. If `line` ever stops matching
  // console.log for a string, 222 call sites change behaviour silently.
  const T = require(path.join(BIN, "transcript.js"));
  const seen = [];
  const w = T.writer({ write: (s) => seen.push(s) });
  w.line("hello");
  w.line("");
  w.line(undefined);
  assert.deepStrictEqual(seen, ["hello\n", "\n", "\n"]);
});

test("🔴 READLINE IS NOT ON THE INTERACTIVE PATH — 'not used carefully; removed'", () => {
  // Founder ruling: a library that owns the cursor cannot coexist with a renderer that owns the screen,
  // and every attempt to make them coexist has produced a defect. The session's readline is now created
  // ONLY for a pipe; on a TTY the composer is the sole reader.
  const src = fs.readFileSync(path.join(BIN, "estelle.js"), "utf8");
  const sessionRl = src.match(/const rl = [^;]*createInterface\(\{\s*\n?\s*input: sessionInput/s)
    || src.match(/process\.stdin\.isTTY \? null : readline\.createInterface/);
  assert.ok(sessionRl, "the session's readline must be guarded by process.stdin.isTTY");
  assert.match(src, /process\.stdin\.isTTY \? null : readline\.createInterface/,
    "on a TTY there must be NO readline at all — not a paused one, not a careful one");
});

test("the composer is the SESSION'S ONLY keypress reader — overlays bind to it, not to stdin", () => {
  // Four modules each called emitKeypressEvents(process.stdin) and bound their own keypress. Four
  // readers of one stream is why a paste that is ONE submit in isolation came back as three entries
  // under a real pty.
  const src = fs.readFileSync(path.join(BIN, "estelle.js"), "utf8");
  assert.match(src, /keyBinder\(keySource/, "mode-ui must bind the composer's dispatcher");
  assert.match(src, /attachMenu\(keySource/, "the slash menu must bind the composer's dispatcher");
  assert.ok(!/keyBinder\(process\.stdin/.test(src), "nothing may bind raw stdin on the session path");
  assert.ok(!/attachMenu\(process\.stdin/.test(src), "nothing may bind raw stdin on the session path");
});
