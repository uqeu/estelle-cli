"use strict";
// The update prompt — brief §2.6, and the reason it exists is not cosmetic.
//
// Customers sat on 0.1.3, the build whose `install-hooks` DESTROYED their Claude Code settings, across
// three failed release attempts. The fix existed the whole time. A machine that never learns a new version
// exists never receives it.
//
// So these tests weight toward the failure modes that would make us turn the feature off again: nagging,
// blocking, crashing, or shouting on a plane.

const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const u = require("../bin/update-check.js");

const tmpFile = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), "estelle-upd-")), "check.json");
const ok = (version) => async () => ({ ok: true, json: async () => ({ version }) });

test("a newer version is newer, and an older or equal one is not", () => {
  assert.strictEqual(u.isNewer("0.1.8", "0.1.9"), true);
  assert.strictEqual(u.isNewer("0.1.9", "0.2.0"), true);
  assert.strictEqual(u.isNewer("0.9.9", "1.0.0"), true);
  assert.strictEqual(u.isNewer("0.1.9", "0.1.9"), false);
  assert.strictEqual(u.isNewer("0.1.9", "0.1.8"), false, "it must never suggest a downgrade");
  assert.strictEqual(u.isNewer("0.1.10", "0.1.9"), false, "10 > 9 numerically, not as a string");
});

test("a PRE-RELEASE is never offered", () => {
  // recommending 0.2.0-beta.1 to someone on a stable 0.1.9 upgrades them to the LESS tested build
  assert.strictEqual(u.isNewer("0.1.9", "0.2.0-beta.1"), false);
  assert.strictEqual(u.updateNotice("0.1.9", "0.2.0-rc.2"), "");
});

test("the notice names the exact command", () => {
  // "an update is available" that leaves someone guessing the incantation is half a message
  const line = u.updateNotice("0.1.8", "0.1.9");
  assert.match(line, /0\.1\.8/);
  assert.match(line, /0\.1\.9/);
  assert.match(line, /npm i -g @fatelabs\/estelle/);
  assert.ok(!/\p{Extended_Pictographic}/u.test(line), "no emoji — house rule");
});

test("nothing to say produces an EMPTY string, not a cheerful 'you are up to date'", () => {
  assert.strictEqual(u.updateNotice("0.1.9", "0.1.9"), "");
});

test("OFFLINE IS SILENT — no warning, no throw", async () => {
  // a plane, a locked-down network, a registry outage. A tool that nags about its own connectivity is
  // worse than one that is quiet.
  const boom = async () => { throw new Error("getaddrinfo ENOTFOUND registry.npmjs.org"); };
  const line = await u.checkForUpdate("0.1.8", { fetch: boom, file: tmpFile() });
  assert.strictEqual(line, "");
});

test("a non-200 or a junk payload is also silent", async () => {
  const file = tmpFile();
  assert.strictEqual(await u.checkForUpdate("0.1.8", { fetch: async () => ({ ok: false }), file }), "");
  assert.strictEqual(await u.checkForUpdate("0.1.8",
    { fetch: async () => ({ ok: true, json: async () => "not an object" }), file }), "");
});

test("the registry is asked ONCE PER INTERVAL, not once per invocation", async () => {
  // the hooks path runs this binary on every edit; a call per run is a real recurring cost.
  // Written against u.CACHE_TTL_MS rather than a hard-coded day: #92 shortened the interval to an hour,
  // and a test that hard-codes the constant it is guarding goes red on the fix instead of on a defect.
  const file = tmpFile();
  let calls = 0;
  const counting = async (...a) => { calls += 1; return ok("0.1.9")(...a); };
  const now = 1_700_000_000_000;
  assert.match(await u.checkForUpdate("0.1.8", { fetch: counting, file, now }), /0\.1\.9/);
  assert.strictEqual(calls, 1);
  // ...inside the interval, still cached, and STILL reports the pending update from cache
  const later = await u.checkForUpdate("0.1.8",
    { fetch: counting, file, now: now + Math.floor(u.CACHE_TTL_MS / 2) });
  assert.strictEqual(calls, 1, "it asked the registry again inside the TTL");
  assert.match(later, /0\.1\.9/, "a cached answer must still notify — the cache is for the CALL, not the news");
  // ...past the interval it asks again
  await u.checkForUpdate("0.1.8", { fetch: counting, file, now: now + u.CACHE_TTL_MS + 1 });
  assert.strictEqual(calls, 2);
});

// ── #92 — THE CACHE STORED A NEGATIVE WITH NO WAY TO KNOW IT WAS STALE ─────────
// ~/.estelle/update-check.json held {"latest":"0.1.8"}, written when that was true. Five releases in
// thirteen hours later, a machine running 0.1.10 read it, concluded it was AHEAD of "latest", and went
// silent for a full day — working exactly as designed, on a design that was wrong for our cadence.

test("#92 running AHEAD of the cached 'latest' re-checks IMMEDIATELY — the cache is provably stale", async () => {
  const file = tmpFile();
  // The exact cache from the founder's machine, and a fresh timestamp so the TTL alone would say "wait".
  const now = 1_700_000_000_000;
  u.writeCache("0.1.8", now, file);
  let calls = 0;
  const counting = async (...a) => { calls += 1; return ok("0.1.12")(...a); };
  const notice = await u.checkForUpdate("0.1.10", { fetch: counting, file, now: now + 1000 });
  assert.strictEqual(calls, 1, "a version newer than the cached latest MUST force a re-check");
  assert.match(notice, /0\.1\.12/, "and the customer must finally be told");
});

test("#92 the staleness condition needs no clock at all", () => {
  // It is not a shorter TTL wearing a disguise: you cannot be RUNNING a release the registry has never
  // heard of unless the cached answer predates it. No time arithmetic is involved.
  assert.strictEqual(u.shouldCheck(Date.now(), Date.now(), 1e12, "0.1.10", "0.1.8"), true);
  assert.strictEqual(u.shouldCheck(Date.now(), Date.now(), 1e12, "0.1.8", "0.1.10"), false,
    "…and a normally-behind machine inside the TTL still uses the cache");
});

test("#92 THE PAIRED NEGATIVE — an equal version does NOT force a re-check", () => {
  // Without this the guard could pass by re-checking on every invocation, which is the cost the cache
  // exists to avoid — a fix that turns the cache off is not a fix.
  assert.strictEqual(u.shouldCheck(Date.now(), Date.now(), 1e12, "0.1.9", "0.1.9"), false);
});

test("#92 the notice links the changelog and says what survives an upgrade", () => {
  // "Show the command" was already right. What was missing is the two things that make someone RUN it:
  // what changed, and whether it costs them their setup.
  const n = u.updateNotice("0.1.9", "0.2.0");
  assert.match(n, /npm i -g @fatelabs\/estelle/, "the exact command");
  assert.match(n, /carry over/, "and that their key, config and sessions survive");
  assert.match(n, new RegExp(u.CHANGELOG.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")), "and what changed");
});

test("a corrupt cache file degrades to 'check again', never to a crash", async () => {
  const file = tmpFile();
  fs.writeFileSync(file, "{ not json");
  assert.deepStrictEqual(u.readCache(file), {});
  assert.strictEqual(u.shouldCheck(undefined, Date.now()), true);
  assert.match(await u.checkForUpdate("0.1.8", { fetch: ok("0.1.9"), file }), /0\.1\.9/);
});

test("an unwritable cache location does not break the session", () => {
  // /dev/null/x is ENOTDIR on macOS AND Linux — deliberately not a platform branch, per E-010
  assert.doesNotThrow(() => u.writeCache("0.1.9", Date.now(), "/dev/null/estelle/check.json"));
});

test("a slow registry is abandoned — a timeout signal is always passed, and an abort is silent", async () => {
  // The constraint is that the session starts regardless. Asserting the ABORT SIGNAL is passed is the
  // observable half; asserting a hang would leave a pending promise and prove nothing about the timeout.
  let sawSignal = false;
  await u.checkForUpdate("0.1.8", {
    file: tmpFile(), timeout: 40,
    fetch: async (_url, init) => {
      sawSignal = Boolean(init && init.signal && typeof init.signal.aborted === "boolean");
      return { ok: true, json: async () => ({ version: "0.1.8" }) };
    },
  });
  assert.ok(sawSignal, "no abort signal was passed — a slow registry would block the session");
  // and when that signal fires, fetch rejects and we say nothing at all
  const aborted = async () => { const e = new Error("The operation was aborted"); e.name = "TimeoutError"; throw e; };
  assert.strictEqual(await u.checkForUpdate("0.1.8", { fetch: aborted, file: tmpFile() }), "");
});
