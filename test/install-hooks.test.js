"use strict";
// F0 — `install-hooks` destroyed a customer's Claude Code settings.
//
// THE DEFECT, in the published package, on the install path. cmdInstallHooks read
// ~/.claude/settings.json inside a bare try/catch and swallowed the failure:
//
//     let existing = {};
//     try { existing = JSON.parse(fs.readFileSync(file, "utf8")); } catch { /* first time */ }
//
// A MISSING file and an UNPARSEABLE one took the same branch. One trailing comma — the single most
// common hand-edit mistake in a JSON settings file — meant `existing` stayed `{}`, the merge produced an
// Estelle-hooks-only object, and the customer's permissions, model and env were overwritten. No backup.
//
// `init` twelve hundred lines up had always done it correctly (writeClient copies to .bak before writing),
// so the repo contained a correct implementation of the same operation the whole time.
//
// These tests drive the REAL command through a temp HOME rather than unit-testing a helper, because the
// defect lived in the wiring — a helper test would have passed throughout.

const { test } = require("node:test");
const assert = require("node:assert");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const CLI = path.join(__dirname, "..", "bin", "estelle.js");

function withHome(settings) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-f0-"));
  fs.mkdirSync(path.join(home, ".claude"), { recursive: true });
  const file = path.join(home, ".claude", "settings.json");
  if (settings !== null) fs.writeFileSync(file, settings);
  return { home, file };
}

function run(home) {
  try {
    execFileSync(process.execPath, [CLI, "install-hooks"],
                 { env: { ...process.env, HOME: home }, encoding: "utf8", stdio: "pipe" });
    return 0;
  } catch (e) {
    return e.status === undefined ? 1 : e.status;
  }
}

test("an unparseable settings file is left EXACTLY as it was", () => {
  const original = '{\n  "permissions": {"allow": ["Bash"]},\n  "model": "opus",\n}\n';  // trailing comma
  const { home, file } = withHome(original);
  const code = run(home);
  assert.notStrictEqual(code, 0, "must exit non-zero rather than pretend it worked");
  assert.strictEqual(fs.readFileSync(file, "utf8"), original, "the customer's file was modified");
  assert.ok(!fs.existsSync(file + ".bak"), "nothing was written, so nothing should have been backed up");
});

test("settings that are valid JSON but not an object are refused too", () => {
  // `[]` and `"text"` parse cleanly and would merge into nonsense — JSON.parse succeeding is not enough.
  for (const body of ["[]", '"just a string"', "null"]) {
    const { home, file } = withHome(body);
    assert.notStrictEqual(run(home), 0, `${body} should be refused`);
    assert.strictEqual(fs.readFileSync(file, "utf8"), body, `${body} was modified`);
  }
});

// THE PAIRED POSITIVE. Without this the guard could pass by refusing everything, which would be the same
// defect wearing the opposite sign — a customer who can never install is no better off than one whose
// settings were eaten.
test("valid settings still install, keep their own keys, and are backed up", () => {
  const { home, file } = withHome('{\n  "permissions": {"allow": ["Bash"]},\n  "model": "opus"\n}\n');
  assert.strictEqual(run(home), 0, "a valid file must still install");
  const after = JSON.parse(fs.readFileSync(file, "utf8"));
  assert.deepStrictEqual(after.permissions, { allow: ["Bash"] }, "permissions were lost");
  assert.strictEqual(after.model, "opus", "model was lost");
  assert.ok(after.hooks, "hooks were not installed");
  assert.ok(fs.existsSync(file + ".bak"), "an existing file must be backed up before it is rewritten");
});

test("a first-time install with no settings file at all still works", () => {
  const { home, file } = withHome(null);
  assert.strictEqual(run(home), 0, "absent settings is the normal first-run case, not an error");
  assert.ok(JSON.parse(fs.readFileSync(file, "utf8")).hooks, "hooks were not installed");
  assert.ok(!fs.existsSync(file + ".bak"), "there was nothing to back up");
});
