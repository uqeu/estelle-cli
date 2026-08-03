"use strict";
// EVERY `<module>.X` A CALLER READS MUST ACTUALLY EXIST ON THAT MODULE.
//
// 🔴 THE DEFECT THIS EXISTS FOR, found 2026-08-03 while trying to run the credential scrub on a real
// machine. `estelle.js:1543` calls `repl.scrubHistory(rawHistory)`. `scrubHistory` is defined in
// `input-ui.js`, and `repl.js` re-exports most of that module — `parseHistory`, `historyLine`,
// `interruptAction` — but **not that one**. So `repl.scrubHistory` was `undefined`, and the call threw.
//
// It threw INSIDE this block (`estelle.js:1536-1549`):
//
//     try {
//       const rawHistory = fs.readFileSync(histFile, "utf8");
//       const scrubbed = repl.scrubHistory(rawHistory);   // <-- TypeError, every launch
//       if (scrubbed.removed) { fs.writeFileSync(...); out.line("removed N credentials..."); }
//       history = repl.parseHistory(...);                 // <-- never reached
//     } catch { /* first run */ }
//
// **A bare catch labelled "first run" swallowed it**, so TWO shipped features were silently dead for every
// customer who had a history file:
//   1. the CREDENTIAL SCRUB never ran — a live API key stayed at rest in `~/.estelle/history.jsonl`,
//      which is the entire reason 0.2.2 was cut as urgent;
//   2. ↑/↓ PERSISTED HISTORY never loaded — the throw aborts the block before `parseHistory`, which is
//      why history read as "wired but unproven".
//
// One missing name in an export list, and the failure presented as two unrelated features quietly not
// working. Nothing could catch it: the unit tests call `inputUi.scrubHistory` directly (it is correct),
// and the seam between the modules was never exercised.
//
// THIS IS THE SAME DEFECT AS `local.HELP_ONLY` (`repo-scope.test.js:116`) — a property read off the wrong
// module, in the same file, caught then by a structural sweep over `local.X`. **That sweep was written to
// cover exactly one pair.** A guard aimed at one seam does not cover the next one, so this generalises it:
// every module boundary the CLI actually reads across is checked here, and adding a module to the table is
// the whole cost of covering it.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const BIN = path.join(__dirname, "../bin");

// Each row: the file doing the reading, the identifier it reads through, and the module that must satisfy
// it. Deliberately a hand-written table rather than an import graph — it states what we CLAIM about each
// seam, and an unlisted seam is visibly unlisted rather than silently uncovered.
const SEAMS = [
  ["estelle.js", "repl", "../bin/repl.js"],
  ["repl.js", "local", "../bin/session-commands.js"],
  ["hook.js", "distill", "../bin/distill.js"],
  ["hook.js", "sessionGap", "../bin/session-gap.js"],
];

// THE OTHER SHAPE OF THE SAME SEAM, and the one the shipped defect actually lived in. `repl.js:198`
// destructures its re-exports from `input-ui.js`:
//
//     const { collapsePaste, expandPastes, parseHistory, historyLine, interruptAction, ... } = inputUi;
//
// 🔴 A DESTRUCTURE OF A NAME THAT DOES NOT EXIST DOES NOT THROW — it silently binds `undefined`, and the
// error surfaces later, somewhere else, as "X is not a function". `scrubHistory` was missing from this
// list AND from the export list below it; the failure appeared two files away, inside a `catch` labelled
// "first run". Property-read scanning cannot see this seam at all (there are zero `inputUi.X` reads),
// which is why it needs its own check rather than another row in the table above.
const DESTRUCTURES = [
  ["repl.js", "inputUi", "../bin/input-ui.js"],
];

/** Source with whole-line comments dropped — line by line, never a block regex.
 *
 * `repo-scope.test.js:155` records why, and it cost two wrong versions: `/\*[\s\S]*?\*\//g` ate the CODE
 * as well as the prose, and the checker went green while the defect was present. A line is dropped only
 * if it BEGINS as a comment, so a scanner cannot be blinded by its own fix being documented above it. */
function code(file) {
  return fs.readFileSync(path.join(BIN, file), "utf8")
    .split("\n").filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l)).join("\n");
}

/** The property names `file` reads off `ident`. */
function propertiesRead(file, ident) {
  const re = new RegExp(`\\b${ident}\\.([A-Za-z_$][\\w$]*)`, "g");
  return [...new Set([...code(file).matchAll(re)].map((m) => m[1]))]
    // `require("./repl.js")` matches `repl.js` and yields a phantom property named `js`. It is a filename,
    // not a member read — excluding it is not a loosening, since no module exports a member called `js`.
    .filter((name) => name !== "js");
}

for (const [file, ident, modulePath] of SEAMS) {
  test(`every \`${ident}.X\` read in ${file} exists on ${path.basename(modulePath)}`, () => {
    const target = require(modulePath);
    const used = propertiesRead(file, ident);
    // PROVE THE SCANNER SAW SOMETHING before believing an empty `missing` list. Without this, a renamed
    // identifier makes the regex match nothing and the test passes for every seam forever — the vacuous
    // green this repo has now caught seven times.
    assert.ok(used.length >= 2,
      `found only ${used.length} \`${ident}.X\` reads in ${file} — the scanner is not reading the file, ` +
      "so an empty missing-list below would mean nothing");
    const missing = used.filter((name) => !(name in target));
    assert.deepEqual(missing, [],
      `${file} reads ${missing.join(", ")} off ${path.basename(modulePath)}, which does not export ` +
      "it — the call throws at runtime, and a surrounding try/catch may hide it");
  });
}

/** The names `file` destructures out of `ident`, across a possibly multi-line pattern. */
function namesDestructured(file, ident) {
  const re = new RegExp(`\\{([^{}]*)\\}\\s*=\\s*${ident}\\s*;`, "g");
  const names = [];
  for (const m of code(file).matchAll(re)) {
    for (const part of m[1].split(",")) {
      // `a: b` binds `b` from key `a`; the KEY is what must exist on the module.
      const name = part.split(":")[0].trim();
      if (/^[A-Za-z_$][\w$]*$/.test(name)) names.push(name);
    }
  }
  return [...new Set(names)];
}

for (const [file, ident, modulePath] of DESTRUCTURES) {
  test(`every name destructured from \`${ident}\` in ${file} exists on ${path.basename(modulePath)}`, () => {
    const target = require(modulePath);
    const used = namesDestructured(file, ident);
    assert.ok(used.length >= 2,
      `found only ${used.length} names destructured from \`${ident}\` in ${file} — the pattern did not ` +
      "match, so an empty missing-list below would mean nothing");
    const missing = used.filter((name) => !(name in target));
    assert.deepEqual(missing, [],
      `${file} destructures ${missing.join(", ")} from ${path.basename(modulePath)}, which does not ` +
      "export it — each binds `undefined` silently and fails later at the call site");
  });
}

test("🔴 the scrub specifically: the exact call estelle.js makes must work", () => {
  // The regression test for the shipped defect, written as the CALL rather than as the export list —
  // an export list can be satisfied by a name that is re-exported as `undefined`.
  const repl = require("../bin/repl.js");
  assert.equal(typeof repl.scrubHistory, "function",
    "repl.scrubHistory is what estelle.js:1543 calls on every launch");

  const key = "estelle_live_" + "9f2b7c1d4e6a8b0c2d4e3f9";      // split so this file is not itself scanned as a secret
  const raw = `{"text":"hello","at":1}\n{"text":"${key}","at":2}\n{"text":"bye","at":3}\n`;
  const out = repl.scrubHistory(raw);
  assert.equal(out.removed, 1, "the credential-shaped line must be counted as removed");
  assert.ok(!out.text.includes(key), "the credential must not survive the scrub");

  // THE OTHER HALF THE THROW KILLED, and the one nobody would have connected to a missing export: the
  // block aborts before parseHistory, so ↑/↓ history silently loaded nothing for anyone with a file.
  const history = repl.parseHistory(out.text);
  assert.deepEqual(history, ["hello", "bye"],
    "history must still parse after a scrub — this is the line the TypeError skipped");
});
