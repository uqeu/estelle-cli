"use strict";
// EVERY FILE THE CLI CREATES UNDER ~/.estelle HAS A MODE SOMEONE CHOSE.
//
// Founder, 2026-08-02: "last-session.json IS STILL 0644. You flagged it and left it. Zero key-shaped
// tokens TODAY is exactly the reasoning E-030 exists to defeat — the third instance always lands in the
// place nobody hardened, and a session file is the single most likely place for a repo path, a task
// string or a token to end up next."

const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const BIN = path.join(__dirname, "..", "bin");

// Every writeFileSync in cli/bin that lands under ~/.estelle, with the mode it MUST carry and why.
const EXPECTED = {
  "auth.js": { mode: "0o600", why: "the API key itself" },
  "session-gap.js": { mode: "0o600", why: "a session file: repo paths, task strings, and whatever lands next" },
  "distill.js": { mode: "0o600", why: "spilled tool output — verbatim repo content" },
  "apply.js": { mode: "0o600", why: "undo backups are verbatim copies of the customer's source" },
  "update-check.js": { mode: "0o644", why: "a version string and a timestamp; the ONE argued exception" },
};

test("🔴 every writer under ~/.estelle sets an EXPLICIT mode — no defaults", () => {
  const missing = [];
  for (const [file, spec] of Object.entries(EXPECTED)) {
    const src = fs.readFileSync(path.join(BIN, file), "utf8");
    for (const m of src.matchAll(/fs\.(writeFileSync|appendFileSync)\([^;]*/g)) {
      // ⛔ SCOPED TO ~/.estelle, and the exclusion is a real distinction rather than a loosening.
      // `apply.js` also RESTORES a file into the customer's own repo on /undo. Forcing 0600 there would
      // change the permissions of THEIR file — we are putting back what we took, not deciding how they
      // should store it. A guard that hardened that write would be a defect wearing a fix's clothes.
      if (/writeFileSync\(abs\b/.test(m[0])) continue;
      if (!/mode:\s*0o[0-7]{3}/.test(m[0])) missing.push(`${file}: ${m[0].slice(0, 70)}`);
    }
    assert.match(src, new RegExp(`mode:\\s*${spec.mode}`), `${file} must write ${spec.mode} — ${spec.why}`);
  }
  assert.deepStrictEqual(missing, [], "a write with no mode is a default nobody looked at");
});

test("the ONE 0644 is argued in a comment, not inherited", () => {
  // The point is not that 0644 is wrong here — it is that the exception must be a decision on the record.
  const src = fs.readFileSync(path.join(BIN, "update-check.js"), "utf8");
  const at = src.indexOf("mode: 0o644");
  assert.ok(at > 0, "update-check must set its mode explicitly too");
  assert.match(src.slice(Math.max(0, at - 500), at), /DELIBERATELY|deliberately/,
    "the exception must carry its reason where the next reader will find it");
});
