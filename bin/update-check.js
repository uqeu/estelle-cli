"use strict";
// THE UPDATE PROMPT — brief §2.6, and it is NOT cosmetic.
//
// Customers sat on `0.1.3` — the build whose `install-hooks` DESTROYED their Claude Code settings — across
// THREE failed release attempts. The fix was written, merged and tagged, and none of that reaches a machine
// that never learns a new version exists. **An update prompt is the mechanism that would have moved them.**
// It is the difference between shipping a fix and a customer receiving one.
//
// FOUR CONSTRAINTS, and every one of them is "do not make the CLI worse to save the CLI":
//   * NEVER BLOCKS. The check is fire-and-forget with a short timeout; the session starts regardless.
//   * SILENT WHEN OFFLINE. A plane, a locked-down network or a registry outage produces NOTHING — not a
//     warning, not a stack trace. A tool that nags about its own connectivity is worse than one that is
//     quiet.
//   * CACHED. One registry call a day, not one per invocation — the hooks path runs this binary on every
//     edit, and PLAN-06 already counted npx resolution as a real recurring cost.
//   * ZERO DEPENDENCIES. The comparison is ~15 lines; a semver package is not worth the supply-chain
//     surface that ADR 0015 calls our best property.

const fs = require("fs");
const os = require("os");
const path = require("path");

const REGISTRY = "https://registry.npmjs.org/@fatelabs/estelle/latest";
// ONE HOUR, NOT ONE DAY — register #92. We shipped FIVE releases in THIRTEEN HOURS, and a 24h cache
// cannot describe that cadence: a customer who checked at 09:00 was told nothing until 09:00 the next
// day, four releases later. The interval is a guess about how fast we ship, so it lives beside the reason
// and is overridable (`ESTELLE_UPDATE_TTL_MS`) rather than being a constant nobody can revisit.
// It is the SECOND half of the fix — `shouldCheck`'s staleness condition is the half that matters.
const CACHE_TTL_MS = Number(process.env.ESTELLE_UPDATE_TTL_MS) || 60 * 60 * 1000;
const TIMEOUT_MS = 2000;                        // a slow registry must never be felt
// The release notes for whatever we are recommending. We have GitHub releases and pointed at NOTHING —
// Kimi's upgrade UX links its changelog, and "what changed?" is the first question anyone asks before
// running a command that modifies their machine.
const CHANGELOG = "https://github.com/uqeu/estelle-cli/releases";

const cacheFile = () => path.join(os.homedir(), ".estelle", "update-check.json");

/** `[major, minor, patch]` from a version string; non-numeric parts read as 0 rather than NaN. */
function parts(version) {
  return String(version || "").trim().replace(/^v/, "").split("-")[0].split(".")
    .slice(0, 3).map((n) => (Number.isFinite(Number(n)) ? Number(n) : 0));
}

/**
 * Whether `latest` is strictly newer than `current`. Pure, and deliberately tiny.
 *
 * A PRE-RELEASE IS NEVER NEWER: `0.2.0-beta.1` must not nag someone on a stable `0.1.9`, because the
 * upgrade it recommends would be the less-tested build. Equal-or-older returns false, so a machine running
 * ahead of the registry (a local build, a dev checkout) is never told to downgrade.
 */
function isNewer(current, latest) {
  const l = String(latest || "");
  if (!l || l.includes("-")) return false;                 // no version, or a pre-release
  const [a, b, c] = parts(current);
  const [x, y, z] = parts(l);
  if (x !== a) return x > a;
  if (y !== b) return y > b;
  return z > c;
}

/**
 * Whether to ask the registry again. Missing/corrupt cache → yes, check.
 *
 * 🔴 #92 — THE CACHE STORED A NEGATIVE AND HAD NO WAY TO KNOW IT WAS STALE. `~/.estelle/update-check.json`
 * held `{"latest":"0.1.8"}`, written when 0.1.8 really was newest. We then shipped FIVE releases in
 * THIRTEEN HOURS. A machine running 0.1.10 read that cache, compared 0.1.10 against a "latest" of 0.1.8,
 * concluded it was AHEAD, and said nothing — for a full day, working exactly as designed. **The design was
 * wrong for our cadence.**
 *
 * THE STALENESS CONDITION IS THE FIX, AND IT COSTS NOTHING: if the version we are RUNNING is newer than
 * the version the cache calls "latest", the cache is **provably** wrong — no clock, no TTL, no guess about
 * how fast we ship. You cannot be running a release the registry has never heard of unless the cached
 * answer predates it. Re-check immediately.
 *
 * ⛔ THE GENERAL SHAPE, worth more than this instance (defect class 3, inside a cache): **a cache that
 * stores "there is nothing" needs a STALENESS CONDITION, not just a TTL.** A TTL bounds how long a
 * positive is trusted; nothing bounds how long an ABSENCE is trusted, and an absence is what silences a
 * mechanism. The register names two more to audit for this shape — the Firecrawl vendor cache and the
 * research caches — and both also cache absences.
 */
function shouldCheck(cachedAt, now, ttl, current, cachedLatest) {
  if (isNewer(cachedLatest, current)) return true;     // running AHEAD of "latest" → the cache is stale
  const at = Number(cachedAt);
  if (!Number.isFinite(at) || at <= 0) return true;
  return (Number(now) - at) >= (Number(ttl) || CACHE_TTL_MS);
}

/**
 * The line a human sees, or `""` when there is nothing to say.
 *
 * Names the exact command, because "an update is available" that leaves someone guessing the incantation
 * is half a message. No emoji (house rule), and no interactive prompt: this is a session starting, not a
 * dialog to dismiss.
 */
function updateNotice(current, latest) {
  if (!isNewer(current, latest)) return "";
  // THE EXACT COMMAND, and WHAT IT COSTS YOU. Kimi's upgrade UX is better than ours and the REASONS matter
  // more than the layout: it shows the command before running it (we are asking to modify their machine),
  // and it says config and sessions carry over — the sentence that actually makes someone press enter.
  // The changelog link closes the other half: we have GitHub releases and pointed at nothing.
  return `update available — ${current} → ${latest}   npm i -g @fatelabs/estelle`
    + `\n  your key, config and sessions carry over · what changed: ${CHANGELOG}`;
}

/** Read the cached `{checkedAt, latest}`; `{}` on anything unreadable, corrupt, or absent. Never throws:
 * a broken cache file must degrade to "check again", never to a crash on session start. */
function readCache(file) {
  try {
    const raw = JSON.parse(fs.readFileSync(file || cacheFile(), "utf8"));
    return raw && typeof raw === "object" ? raw : {};
  } catch {
    return {};
  }
}

/** Persist the check. Best-effort — an unwritable HOME must not break a session over a version number. */
function writeCache(latest, now, file) {
  try {
    const target = file || cacheFile();
    fs.mkdirSync(path.dirname(target), { recursive: true, mode: 0o700 });
    fs.writeFileSync(target, JSON.stringify({ checkedAt: Number(now) || Date.now(), latest }) + "\n");
  } catch { /* a version cache is never worth an error */ }
}

/**
 * The whole check: cached read, conditional fetch, notice. Resolves to the line to print or `""`.
 *
 * Everything I/O is injected so the tests never touch the network or the real HOME — the same shape the
 * rest of this CLI uses, and the reason this file can be tested at all.
 */
async function checkForUpdate(current, opts) {
  const o = opts || {};
  const now = o.now || Date.now();
  const file = o.file || cacheFile();
  const cache = readCache(file);
  if (!shouldCheck(cache.checkedAt, now, o.ttl, current, cache.latest)) {
    return updateNotice(current, cache.latest);
  }
  let latest = "";
  try {
    const fetchFn = o.fetch || fetch;
    const res = await fetchFn(o.registry || REGISTRY,
                              { signal: AbortSignal.timeout(o.timeout || TIMEOUT_MS) });
    if (res && res.ok) latest = String(((await res.json()) || {}).version || "");
  } catch {
    return "";                                  // offline, slow, blocked, or a bad payload: say NOTHING
  }
  if (latest) writeCache(latest, now, file);
  return updateNotice(current, latest);
}

module.exports = { isNewer, shouldCheck, updateNotice, checkForUpdate, readCache, writeCache,
                   cacheFile, CACHE_TTL_MS, TIMEOUT_MS, REGISTRY, CHANGELOG };
