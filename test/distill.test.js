"use strict";
// POSTTOOLUSE OUTPUT CURATION — prevention instead of eviction.
//
// The claim under test is narrow and the risk is asymmetric: a distilled output must preserve what the model
// NEEDED. So most of this file is about what the distiller REFUSES to do. A dropped stack trace costs a whole
// debugging loop; a kept one costs a few hundred tokens.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const d = require("../bin/distill.js");

/** A pytest run: 400 passing tests, one real failure with its traceback. */
function pytestRun() {
  const pass = Array.from({ length: 400 }, (_, i) => `tests/test_serve.py::test_case_${i} PASSED       [ ${i}%]`);
  return [
    "============================= test session starts ==============================",
    "collected 401 items",
    ...pass,
    "tests/test_serve.py::test_upload_batches FAILED                          [100%]",
    "=================================== FAILURES ===================================",
    "____________________________ test_upload_batches _______________________________",
    '    def test_upload_batches():',
    ">       assert resp.status == 200",
    "E       AssertionError: assert 413 == 200",
    'tests/test_serve.py:88: AssertionError',
    "=========================== 1 failed, 400 passed ===============================",
  ].join("\n");
}

test("a test run keeps every failure and drops the passing noise", () => {
  const body = pytestRun();
  const result = d.distil({ tool_name: "Bash", tool_response: { stdout: body } });
  assert.ok(result, "a 400-pass run is exactly what this exists for");

  // KEPT: everything the model needs to fix the failure
  for (const needed of ["test_upload_batches FAILED", "AssertionError: assert 413 == 200",
                        "assert resp.status == 200", "tests/test_serve.py:88",
                        "1 failed, 400 passed", "test session starts"]) {
    assert.ok(result.text.includes(needed), `must keep: ${needed}`);
  }
  // DROPPED: only the passing lines
  assert.ok(!/test_case_200 PASSED/.test(result.text), "the passing noise goes");
  assert.ok(result.dropped >= 400, `expected 400+ noise lines dropped, got ${result.dropped}`);
  assert.ok(result.saving > 0.9, `expected a big saving, got ${result.saving}`);
});

test("EVERY surviving line is byte-identical — this filters, it never rewrites", () => {
  const body = pytestRun();
  const original = new Set(body.split("\n"));
  const result = d.distil({ tool_name: "Bash", tool_response: { stdout: body } });
  for (const line of result.text.split("\n")) {
    // the repeat marker is the ONE line this function is allowed to write
    if (/^\s+\.\.\. \(previous line repeated/.test(line)) continue;
    assert.ok(original.has(line), `invented or altered a line: ${JSON.stringify(line)}`);
  }
});

test("it REFUSES the tools whose output IS the answer", () => {
  const body = "x\n".repeat(5000);
  for (const tool of ["Read", "Grep", "Glob", "NotebookRead", "WebFetch", "WebSearch"]) {
    assert.equal(d.distil({ tool_name: tool, tool_response: { stdout: body } }), null,
      `${tool}: the model asked for that text; a distiller cannot know which line it wanted`);
  }
});

test("it REFUSES short output, unrecognised output, and output it would barely shrink", () => {
  const varied = (n, prefix) => Array.from({ length: n }, (_, i) => `${prefix} ${i}`).join("\n");
  // short: verbosity is not a problem worth taking a risk to solve
  assert.equal(d.distil({ tool_name: "Bash", tool_response: { stdout: "ok 1 - fine\n".repeat(5) } }), null);
  // long but nothing NAMEABLE to drop -> hands off entirely
  assert.equal(d.distil({ tool_name: "Bash", tool_response: { stdout: varied(200, "the quick brown fox") } }), null);
  // long, one noise line, nowhere near enough saving to be worth the risk
  const mostlySignal = varied(200, "a real log line that matters") + "\nok 1 - x";
  assert.equal(d.distil({ tool_name: "Bash", tool_response: { stdout: mostlySignal } }), null);
});

test("failure vocabulary OVERRIDES every noise rule", () => {
  // each of these matches a noise pattern by shape, but says something failed
  for (const line of ["ok 12 - the retry failed and was not caught",
                      "  ✓ PASS but the fixture is missing",
                      "--- PASS: TestX (0.1s) with a warning"]) {
    assert.equal(d.noiseKind(line), "", `signal must win: ${line}`);
  }
  // and the plain shapes are still noise
  assert.ok(d.noiseKind("ok 12 - uploads a batch"));
  assert.ok(d.noiseKind("--- PASS: TestUpload (0.10s)"));
  assert.ok(d.noiseKind("tests/x.py::test_y PASSED"));
});

test("a stderr-only failure is never distilled away", () => {
  const body = "Traceback (most recent call last):\n" + '  File "x.py", line 3, in <module>\n'.repeat(300)
    + "ModuleNotFoundError: No module named 'estelle'";
  const result = d.distil({ tool_name: "Bash", tool_response: { stderr: body } });
  // the repeated frame collapses, but the error is untouched
  if (result) {
    assert.ok(result.text.includes("ModuleNotFoundError: No module named 'estelle'"));
    assert.ok(result.text.includes("Traceback (most recent call last):"));
  }
});

test("blank lines survive — structure is information too", () => {
  const { text } = d.filterNoise("FAILURES\n\n  assert 1 == 2\n\nsummary");
  assert.equal(text, "FAILURES\n\n  assert 1 == 2\n\nsummary");
});

test("a collapsed run states its COUNT — 3 retries and 900 are different facts", () => {
  const { text, collapsed } = d.filterNoise(Array(10).fill("retrying connection to db").join("\n"));
  const lines = text.split("\n");
  assert.equal(lines.length, d.REPEAT_RUN + 1, "three copies plus the marker");
  assert.equal(collapsed, 7);
  assert.match(lines[lines.length - 1], /previous line repeated 7 more times/);
  // 3 kept + 7 counted = the 10 that were there. Nothing is silently gone.
});

test("the tool response is read whatever shape the host used", () => {
  assert.equal(d.responseText("plain"), "plain");
  assert.equal(d.responseText({ stdout: "a", stderr: "b" }), "a\nb");
  assert.equal(d.responseText({ output: "c" }), "c");
  assert.equal(d.responseText(null), "");
  assert.equal(d.responseText(42), "");
});

test("the receipt names what was removed and where the untouched original is", () => {
  const line = d.receipt({ dropped: 400, collapsed: 2, saving: 0.93 }, "/tmp/x.log");
  assert.match(line, /400 noise lines removed/);
  assert.match(line, /2 repeated lines collapsed/);
  assert.match(line, /93% smaller/);
  assert.match(line, /Nothing matching an error, failure, warning or traceback was removed/);
  assert.match(line, /Full untouched output: \/tmp\/x\.log/);
  // no spill path -> the claim about where the original is simply is not made
  assert.ok(!/Full untouched output/.test(d.receipt({ dropped: 1, collapsed: 0, saving: 0.5 }, "")));
});

test("the replacement envelope is the shape Claude Code reads", () => {
  const env = d.replacement("curated");
  assert.equal(env.hookSpecificOutput.hookEventName, "PostToolUse");
  assert.deepEqual(env.hookSpecificOutput.updatedToolOutput, { type: "text", text: "curated" });
});

test("the spill writes the original and prunes itself", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-spill-"));
  try {
    const file = d.spill("the whole untouched output", dir);
    assert.ok(file, "a spill in a writable dir must succeed");
    assert.equal(fs.readFileSync(file, "utf8"), "the whole untouched output");
    assert.equal(fs.statSync(file).mode & 0o777, 0o600, "a tool result can hold anything; keep it private");
    for (let i = 0; i < 5; i++) fs.writeFileSync(path.join(dir, `0000${i}-x.log`), "old");
    d.pruneSpill(dir, 2);
    assert.equal(fs.readdirSync(dir).filter((f) => f.endsWith(".log")).length, 2);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

// UNWRITABLE PATH, PORTABLY. This test used `/proc/definitely/not/writable/estelle`, which HUNG ON LINUX
// for >=100s and held the F0 install-hooks fix off npm for three release attempts. `/proc` does not exist
// on macOS, so no local run ever took that path; CI took it every time.
//
// `/dev/null` is a CHARACTER DEVICE on both macOS and Linux, so mkdir beneath it is ENOTDIR immediately on
// either — no `process.platform` branch, and therefore no second code path that only one of us ever runs.
// A test that behaves differently per OS is how this defect stayed invisible; the fix is a path that
// behaves the SAME everywhere, not a conditional that hides the difference.
test("an unwritable spill degrades to no path, never to a lost result", () => {
  assert.equal(d.spill("body", "/dev/null/estelle"), "");
  // and pruning an unreadable dir is silent
  assert.doesNotThrow(() => d.pruneSpill("/dev/null/nope"));
});
