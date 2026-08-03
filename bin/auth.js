"use strict";
// ONE definition of "where the stored key lives", for every command that needs one.
//
// It used to be two, and they disagreed. The REPL saved a pasted key to ~/.estelle/auth.json (repl.js) and
// then printed the next step — "Run `estelle sweep`" — but `cmdSweep` resolved its key from
// `flag("--key", process.env.ESTELLE_API_KEY)` and never looked at the file. So the FIRST-RUN flow the
// product prints for itself failed on the literal next command: key saved, key ignored, "Need --key".
// The same read was missing from `needKey()`, which is the door for ask/recall/verify/gate/github and —
// via ctx — monitor/research/memory.
//
// Splitting it into a leaf module is what makes "the writer and every reader agree" a fact about the code
// rather than a convention someone has to remember at each new call site. Zero dependencies beyond Node
// built-ins, so the entry point can require it at the top without paying for repl.js's dependency tree
// (curate/apply/mode-ui) just to answer "what key am I running as".

const fs = require("fs");
const os = require("os");
const path = require("path");

// Resolved on CALL, not at module load: `os.homedir()` reads $HOME, and a process that changes it — or a
// test that drives the real command through a temp HOME — must see the directory it actually set.
const authDir = () => path.join(os.homedir(), ".estelle");
const authFile = () => path.join(authDir(), "auth.json");

/**
 * The key stored on this machine, or "" when there is none.
 *
 * A missing file, an unreadable one, malformed JSON and a file with no usable `key` all mean the same
 * thing to a caller — "no stored credential" — so they collapse to "". They must never throw: a corrupt
 * auth.json turning `estelle gate` into a stack trace would be a fail-OPEN in CI.
 */
function readAuth() {
  try {
    const raw = JSON.parse(fs.readFileSync(authFile(), "utf8"));
    return raw && typeof raw.key === "string" && raw.key.trim() ? raw.key.trim() : "";
  } catch {
    return "";
  }
}

/** Persist the pasted key. 0700/0600 — a bearer credential for the whole account is never world-readable. */
function writeAuth(key) {
  fs.mkdirSync(authDir(), { recursive: true, mode: 0o700 });
  fs.writeFileSync(authFile(), JSON.stringify({ key }, null, 2) + "\n", { mode: 0o600 });
}

/**
 * THE resolution order, in one place: `$ESTELLE_API_KEY` first, then the stored file. "" when unset.
 *
 * The environment wins over the file on purpose — an explicit `ESTELLE_API_KEY=…` in a CI job or a
 * one-off shell is a deliberate override of whatever this machine happens to have saved, and silently
 * preferring the file would run the job as the wrong account. (`--key` outranks both; that is the
 * caller's `flag()`, since only the entry point owns argv.)
 */
function storedKey(env) {
  const e = env || process.env;
  return (e.ESTELLE_API_KEY || "").trim() || readAuth();
}

/** Remove the stored key. Idempotent — a missing file is the desired end state, not an error.
 *
 * 🔴 ONE DEFINITION, because "a rejected key must not stay on disk" was true on ONE of the two doors.
 * `init` gets it right by never writing an unverified key (`estelle.js:336` — `if (!v.rejected)
 * persistKey(key)`), and `verify-cli-publish.sh` step 5 asserts exactly that and passes. The SESSION door
 * writes FIRST and validates after (`repl.js:575`), so a rejected key stayed in `~/.estelle/auth.json` and
 * the only recovery offered was "delete the file yourself" — dotfile surgery, on the door a first-time
 * customer meets before any other. Two doors, one guard; this is the guard the second one lacked.
 *
 * ⛔ ONLY an EXPLICIT rejection (401/403/404) may call this. A timeout, a firewall or an outage is a
 * failure to ASK, and discarding a valid key on one is the defect `init`'s comment at :332 was written
 * to prevent — it would make a brief blip cost the customer their credential. */
function clearAuth() {
  try { fs.rmSync(authFile(), { force: true }); } catch { /* already gone is success */ }
}

module.exports = { authDir, authFile, readAuth, writeAuth, clearAuth, storedKey };
