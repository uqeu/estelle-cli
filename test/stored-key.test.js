"use strict";
// The stored key the REPL saves, and the ten commands that could not read it.
//
// THE DEFECT, on the first-run path. `runSession` takes the pasted key, writes it to ~/.estelle/auth.json
// (repl.js writeAuth), and prints the next step — "This repo isn't indexed yet. Run `estelle sweep`".
// `cmdSweep` then resolved its key with:
//
//     const key = flag("--key", process.env.ESTELLE_API_KEY);
//
// and never looked at the file. So the literal next command the product printed for itself answered
// "Need --key <ESTELLE_KEY> to sweep." and exited 1. The same read was missing from `needKey()`, which is
// the door for ask / recall / verify / gate / github and — through ctx — monitor / research / memory.
//
// These tests drive the REAL commands through a temp $HOME, the way install-hooks.test.js does, because
// the defect lived in the WIRING: `repl.storedKey()` was correct the whole time and a helper-level test
// would have passed throughout. The headline test does not hand-write auth.json either — it runs the
// session and lets it save the key, so what is under test is the handoff and not a fixture.

const { test, before, after } = require("node:test");
const assert = require("node:assert");
const { execFile } = require("node:child_process");
const http = require("node:http");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const CLI = path.join(__dirname, "..", "bin", "estelle.js");
const REFUSAL = "Need an Estelle key";
// Shaped like a key (looksLikeKey wants >= 20 chars, no whitespace) but obviously not one. Nothing here
// is a real credential — these strings only ever reach the local mock below.
const SAVED_KEY = "estelle_test_saved_000000000000";
const ENV_KEY = "estelle_test_fromenv_00000000";
const FLAG_KEY = "estelle_test_fromflag_0000000";

// ── a local stand-in for api.fatelabs.ca ────────────────────────────────────────
// It records the bearer it was handed, which is what makes the precedence tests behavioural: "which key
// actually went out on the wire", not "which branch do we believe was taken".
let server, MCP_URL;
const seen = [];

before(async () => {
  server = http.createServer((req, res) => {
    req.resume();  // drain POST bodies, or the client hangs waiting for the response
    const m = /^Bearer (.+)$/.exec(req.headers.authorization || "");
    seen.push({ url: req.url, key: m ? m[1] : null });
    let body = { ok: true };
    if (req.url.startsWith("/account")) body = { email: "test@example.com", plan: { name: "Pro" } };
    else if (req.url.startsWith("/overview")) body = { memory: { repo_files: 0 } };
    else if (req.url.startsWith("/sessions")) body = { sessions: [] };
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(body));
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  MCP_URL = `http://127.0.0.1:${server.address().port}/mcp`;
});
after(() => { if (server) server.close(); });

/** A throwaway $HOME. `key` non-null means "as if a session had already saved one". */
function tmpHome(key) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-key-home-"));
  if (key) {
    fs.mkdirSync(path.join(home, ".estelle"), { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(home, ".estelle", "auth.json"),
                     JSON.stringify({ key }, null, 2) + "\n", { mode: 0o600 });
  }
  return home;
}

/** A throwaway repo with one ingestable file — enough for `sweep` to have something to do. */
function tmpRepo() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "estelle-key-repo-"));
  fs.writeFileSync(path.join(dir, "index.js"), "export const x = 1;\n");
  return dir;
}

/**
 * Run the real CLI. Never throws — the exit code and the output ARE the assertion material.
 *
 * ESTELLE_API_KEY is blanked by default so a developer's own exported key can never make a red test look
 * green; each test opts back in explicitly.
 */
function cli(args, { home, cwd, input = null, env = {} } = {}) {
  return new Promise((resolve) => {
    const child = execFile(process.execPath, [CLI, ...args], {
      cwd,
      encoding: "utf8",
      env: { ...process.env, HOME: home, NO_COLOR: "1", ESTELLE_MCP_URL: MCP_URL, ESTELLE_API_KEY: "", ...env },
    }, (err, stdout, stderr) => {
      resolve({
        code: err ? (err.code === undefined ? 1 : err.code) : 0,
        out: (stdout || "") + (stderr || ""),
      });
    });
    if (input !== null) child.stdin.write(input);
    child.stdin.end();
  });
}

// ── the reproduction, end to end ────────────────────────────────────────────────

test("the key the session saves is the key the command it prints can use", async () => {
  const home = tmpHome(null);
  const cwd = tmpRepo();

  // 1. The genuine first run: paste a key into the session. Nothing is pre-seeded on disk.
  const session = await cli([], { home, cwd, input: SAVED_KEY + "\n" });
  assert.match(session.out, /saved .*auth\.json/, "the session did not report saving the key");
  assert.ok(fs.existsSync(path.join(home, ".estelle", "auth.json")), "no auth.json was written");
  // The instruction under test. If this line ever stops being printed, the rest of this test is aimed at
  // a flow the product no longer has, and that must fail loudly rather than pass quietly.
  assert.match(session.out, /Run `estelle sweep`/, "the session no longer prints `estelle sweep` as the next step");

  // 2. The literal next command, in the same $HOME, with no --key and no ESTELLE_API_KEY.
  const sweep = await cli(["sweep"], { home, cwd });
  assert.ok(!sweep.out.includes(REFUSAL) && !sweep.out.includes("Need --key"),
            `sweep refused the key the session had just saved:\n${sweep.out}`);
  assert.strictEqual(sweep.code, 0, `sweep should succeed after the session saved a key:\n${sweep.out}`);
  assert.match(sweep.out, /Repo swept/, "the sweep did not actually complete");
});

// ── the sibling audit: every command that needs a key, both directions ───────────
// Enumerated from the dispatcher at the foot of bin/estelle.js — these are every branch that resolves a
// key before doing its work. `connect` is excluded on purpose (it PRINTS a config snippet, so resolving a
// saved credential there would splash it across the terminal) and so are the offline branches
// (remove/disconnect/off, install-hooks, uninstall-hooks, help).
const KEYED = [
  ["sweep"], ["reindex"], ["ask"], ["recall"], ["verify"], ["gate"],
  ["github"], ["monitor"], ["research"], ["memory"],
];

test("every command that needs a key reads the one the session stored", async () => {
  for (const args of KEYED) {
    const r = await cli(args, { home: tmpHome(SAVED_KEY), cwd: tmpRepo() });
    assert.ok(!r.out.includes(REFUSAL),
              `estelle ${args.join(" ")} refused a stored key:\n${r.out}`);
  }
});

// THE PAIRED NEGATIVE. Without it the fix could "pass" by accepting anything — a command that runs with no
// credential at all is a worse defect than one that refuses a good one, and for `gate` and `verify` it is
// a fail-OPEN in someone's CI.
test("with nothing stored, nothing in the environment and no flag, the same commands still refuse", async () => {
  for (const args of KEYED) {
    const r = await cli(args, { home: tmpHome(null), cwd: tmpRepo() });
    assert.ok(r.out.includes(REFUSAL),
              `estelle ${args.join(" ")} ran without any key at all:\n${r.out}`);
    assert.notStrictEqual(r.code, 0, `estelle ${args.join(" ")} exited green with no key`);
  }
});

// ── precedence, asserted on the wire ────────────────────────────────────────────

test("$ESTELLE_API_KEY outranks the stored file", async () => {
  seen.length = 0;
  const r = await cli(["sweep"], { home: tmpHome(SAVED_KEY), cwd: tmpRepo(), env: { ESTELLE_API_KEY: ENV_KEY } });
  assert.strictEqual(r.code, 0, `sweep failed:\n${r.out}`);
  const keys = new Set(seen.map((s) => s.key));
  assert.ok(keys.has(ENV_KEY), "the environment key never reached the server");
  assert.ok(!keys.has(SAVED_KEY), "the stored key was sent even though the environment overrode it");
});

test("--key outranks both the environment and the stored file", async () => {
  seen.length = 0;
  const r = await cli(["sweep", "--key", FLAG_KEY],
                      { home: tmpHome(SAVED_KEY), cwd: tmpRepo(), env: { ESTELLE_API_KEY: ENV_KEY } });
  assert.strictEqual(r.code, 0, `sweep failed:\n${r.out}`);
  const keys = new Set(seen.map((s) => s.key));
  assert.ok(keys.has(FLAG_KEY), "the --key flag never reached the server");
  assert.ok(!keys.has(ENV_KEY) && !keys.has(SAVED_KEY), "a lower-precedence key was sent anyway");
});

// ── the file itself ─────────────────────────────────────────────────────────────

test("a corrupt auth.json reads as no key, never as a crash", async () => {
  // A half-written or hand-edited file must degrade to the refusal — a stack trace out of `gate` would be
  // an unstyled non-zero that reads as a tooling failure rather than a missing credential.
  for (const body of ["{ not json", "{}", '{"key": ""}', "[]", "null"]) {
    const home = tmpHome(null);
    fs.mkdirSync(path.join(home, ".estelle"), { recursive: true });
    fs.writeFileSync(path.join(home, ".estelle", "auth.json"), body);
    const r = await cli(["gate"], { home, cwd: tmpRepo() });
    assert.ok(r.out.includes(REFUSAL), `${body} should read as "no key":\n${r.out}`);
    assert.ok(!/at .*\.js:\d+/.test(r.out), `${body} produced a stack trace:\n${r.out}`);
  }
});

// ── BOTH DOORS DISCARD A REJECTED KEY — the sixth "two doors, one guard" of the campaign ──────────────
//
// `verify-cli-publish.sh` step 5 asserts "a 401 discards the key, as it must" and PASSES. It exercises
// `init`, which gets it right by never persisting an unverified key (`estelle.js:336`). The SESSION door
// writes first and validates after (`repl.js:575`), so a rejected key stayed in ~/.estelle/auth.json and
// the recovery printed was "Delete ~/.estelle/auth.json and run estelle again" — dotfile surgery, on the
// first screen a new customer reaches, after one mistyped paste. Found by walking the path as a stranger.
//
// A guard asserted on one door is not a property of the product. Both are asserted here.

const authMod = require("../bin/auth.js");

test("🔴 auth exposes ONE way to remove a stored key", () => {
  assert.equal(typeof authMod.clearAuth, "function",
    "both doors must clear through the same definition, or they will disagree about what 'discard' means");
});

test("🔴 THE SESSION DOOR: an explicitly rejected key is REMOVED, not left for the customer to delete", async () => {
  const os = require("node:os"), fsMod = require("node:fs"), pathMod = require("node:path");
  const home = fsMod.mkdtempSync(pathMod.join(os.tmpdir(), "estelle-reject-"));
  const realHome = os.homedir;
  os.homedir = () => home;
  try {
    authMod.writeAuth("estelle_live_" + "rejectedkey000000000000");
    assert.ok(fsMod.existsSync(authMod.authFile()), "precondition: the key is on disk");

    const said = [];
    const repl = require("../bin/repl.js");
    await repl.runSession({
      key: "estelle_live_" + "rejectedkey000000000000",
      // 401 is what the server answers for a key it does not know — an EXPLICIT rejection.
      get: async () => ({ error: { code: 401 } }),
      post: async () => ({}),
      prompt: async () => null,
      out: (s) => said.push(String(s)),
      c: new Proxy({}, { get: () => (s) => String(s) }),
      cwd: "repo", now: () => Date.now(),
    });

    assert.ok(!fsMod.existsSync(authMod.authFile()),
      "the rejected key is STILL on disk — the customer is back to deleting a dotfile by hand");
    const text = said.join("\n");
    assert.match(text, /rejected/i);
    assert.doesNotMatch(text, /Delete ~\/\.estelle\/auth\.json/,
      "dotfile surgery must not be the recovery instruction any more");
  } finally {
    os.homedir = realHome;
    fsMod.rmSync(home, { recursive: true, force: true });
  }
});

test("⛔ a FAILURE TO ASK must never discard the key — only an explicit rejection may", async () => {
  // The paired negative, and the more important half. `init`'s comment at estelle.js:332 records why:
  // gating persistence on a round-trip means a firewall or a brief outage costs a customer a VALID key.
  // A fix that discarded on any error would pass the test above and be strictly worse than the bug.
  const os = require("node:os"), fsMod = require("node:fs"), pathMod = require("node:path");
  const home = fsMod.mkdtempSync(pathMod.join(os.tmpdir(), "estelle-outage-"));
  const realHome = os.homedir;
  os.homedir = () => home;
  try {
    authMod.writeAuth("estelle_live_" + "goodkey00000000000000000");
    const repl = require("../bin/repl.js");
    await repl.runSession({
      key: "estelle_live_" + "goodkey00000000000000000",
      get: async () => { throw new Error("ETIMEDOUT"); },   // the network, not the server
      post: async () => ({}),
      prompt: async () => null,
      out: () => {},
      c: new Proxy({}, { get: () => (s) => String(s) }),
      cwd: "repo", now: () => Date.now(),
    });
    assert.ok(fsMod.existsSync(authMod.authFile()),
      "an outage discarded a VALID key — a blip must never cost the customer their credential");
  } finally {
    os.homedir = realHome;
    fsMod.rmSync(home, { recursive: true, force: true });
  }
});
