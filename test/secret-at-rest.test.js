"use strict";
// 🔴 A LIVE API KEY WAS FOUND AT REST IN ~/.estelle/history.jsonl, AND ↑ RECALLED IT ONTO THE SCREEN.
//
// Founder, 2026-08-02: "85 lines, one contains estelle_live_… — the key I pasted into the first-run
// prompt got written to history like any other input. Mode 0600 so it is not exposed to other users, but
// up-arrow surfaces it on screen, in front of anyone watching or on any screen share; it is a credential
// at rest in a plaintext file that has no reason to hold one; and it is a SECOND instance of the same
// defect as the key-echo bug — the key prompt is not treated as a secret input anywhere in its lifecycle."
//
// TWO PARTS, BOTH REQUIRED, and they cover DIFFERENT mistakes:
//   1. by construction — a masked read can never degrade to a reader that records;
//   2. defence in depth — the writer refuses a credential-shaped line, because a customer pasting a key
//      at the ordinary composer is a path no prompt wiring can prevent.

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const ui = require(path.join(__dirname, "..", "bin", "input-ui.js"));
const ask = require(path.join(__dirname, "..", "bin", "ask.js"));

const C = new Proxy({}, { get: () => (s) => String(s === undefined ? "" : s) });
// Shaped like a real key and deliberately NOT one.
const FAKE = `estelle_live_${"a1b2c3d4e5f6".repeat(2)}`;

test("🔴 the history writer REFUSES a credential-shaped line", () => {
  assert.strictEqual(ui.historyLine(FAKE, ""), "", "a key must never come to rest");
  assert.strictEqual(ui.historyLine(`my key is ${FAKE} ok`, ""), "", "…even embedded in a sentence");
});

test("other vendors' credentials are refused too — ours is not the only one pasted", () => {
  for (const t of [`sk-${"x".repeat(24)}`, `ghp_${"y".repeat(28)}`, `github_pat_${"z".repeat(30)}`]) {
    assert.strictEqual(ui.historyLine(t, ""), "", t.slice(0, 12));
  }
});

test("THE PAIRED POSITIVE — ordinary input is still recorded", () => {
  // Without this the guard could 'pass' by refusing everything, which would silently kill ↑/↓ history.
  const line = ui.historyLine("why is the sweep slow?", "");
  assert.match(line, /why is the sweep slow\?/);
  assert.match(ui.historyLine("estelle sweep --dry-run", ""), /dry-run/, "a mention of estelle is not a key");
});

test("scrubHistory removes exactly the credential lines and keeps the rest", () => {
  const raw = [
    JSON.stringify({ text: "how does auth work?", at: 1 }),
    JSON.stringify({ text: FAKE, at: 2 }),
    JSON.stringify({ text: "run the gate", at: 3 }),
  ].join("\n");
  const { text, removed } = ui.scrubHistory(raw);
  assert.strictEqual(removed, 1);
  assert.ok(!text.includes(FAKE), "the credential is gone");
  assert.match(text, /how does auth work/, "and nothing else was taken");
  assert.match(text, /run the gate/);
});

test("scrubHistory on a clean file removes nothing and changes nothing", () => {
  const raw = JSON.stringify({ text: "hello", at: 1 });
  const { text, removed } = ui.scrubHistory(raw);
  assert.strictEqual(removed, 0);
  assert.strictEqual(text, raw, "a clean file must be byte-identical after a scrub");
});

test("🔴 BY CONSTRUCTION — a masked read can NEVER fall back to the recording reader", () => {
  // THE HOLE THAT CAUSED IT: `io.promptSecret || secretPrompt.promptSecret || io.prompt`. The session's
  // `prompt` records every line to history, so a masked read that quietly degraded to it wrote the key
  // straight into the file. A secret is read by a reader that masks AND does not record, or not at all.
  const src = require("node:fs").readFileSync(path.join(__dirname, "..", "bin", "ask.js"), "utf8");
  const block = src.slice(src.indexOf('if (s.kind === "secret")'), src.indexOf('if (s.kind === "text")'));
  // COMMENTS STRIPPED FIRST. The first version of this assertion tripped on its own explanation, which
  // mentions `io.prompt` in prose — a grep over source that counts documentation as behaviour reports a
  // defect in the comment describing the fix.
  const code = block.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n");
  assert.ok(!/\bio\.prompt\b/.test(code),
    "the secret path must not reach the RECORDING reader — exclusion, not filtering");
  assert.match(code, /io\.readLine/, "…it uses the non-recording reader instead");
});

test("a secret NEVER reaches the recording reader — on a pipe it uses readLine instead", async () => {
  // A pipe has no masked reader and no terminal echo to suppress, so it still needs a line read. It gets
  // one that CANNOT record. Cancelling instead would have broken first-run key entry in CI — which is
  // exactly what my first attempt did, and stored-key.test.js caught it.
  let recorded = 0, safeRead = 0;
  const r = await ask.ask({ kind: "secret", label: "key" }, {
    out: () => {}, c: C,
    prompt: async () => { recorded += 1; return FAKE; },
    readLine: async () => { safeRead += 1; return FAKE; },
  });
  assert.strictEqual(recorded, 0, "the recording reader must never be called for a secret");
  assert.strictEqual(safeRead, 1, "the non-recording reader is the one that runs");
  assert.strictEqual(r.value, FAKE, "and the key is still captured");
});

test("with NO reader at all, a secret read cancels rather than inventing one", () => {
  return ask.ask({ kind: "secret" }, { out: () => {}, c: C })
    .then((r) => assert.strictEqual(r.cancelled, true));
});

test("the transcript records a secret as (saved), never as the value", () => {
  const rec = ask.recordOf({ kind: "secret", question: "Paste your key" }, ask.accepted(FAKE));
  assert.ok(!JSON.stringify(rec).includes(FAKE), "a transcript is scrolled back to, and screenshotted");
});

// ── the THIRD surface: a credential must never be DRAWN either ─────────────────
//
// Measured on a real pty 2026-08-03 against published 0.2.2, with a BOGUS sentinel so no live key was
// involved, and with a 12-second settle after the key prompt was already on screen — so this is the
// product, not a racing test harness:
//
//     key ›
//     queued SENTINEL_LATE_PASTE_ZZZ  · esc to clear      <- drawn in full
//     **********************                              <- masking, but only after
//
// The prompt says "PASTE your Estelle key". A paste lands in exactly the window where the queue is
// capturing and the masked reader has not attached yet. `historyLine` refuses to WRITE a credential and
// `scrubHistory` removes ones already written; this third surface — DISPLAY — had no guard at all.

const pending = require("../bin/pending.js");

const plain = { dim: (s) => s, bold: (s) => s };

test("🔴 the queued preview never draws a credential", () => {
  const key = "estelle_live_" + "9f2b7c1d4e6a8b0c2d4e3f9";   // split so this file is not itself flagged
  const line = pending.preview([key], plain);
  assert.ok(!line.includes(key), "the queued indicator drew the credential in full");
  assert.match(line, /hidden/, "it must SAY why it is hidden, not silently blank");
  assert.match(line, /esc to clear/, "the customer must still be able to cancel it");
  assert.match(line, /queued/, "it must still say something is waiting");
});

test("masking the secret does not blind the ordinary preview", () => {
  // The paired positive. A `preview` that returned "••••" for everything would pass the test above and
  // destroy the feature — pending.js exists precisely so a customer can SEE what is queued.
  const line = pending.preview(["what does greet return?"], plain);
  assert.match(line, /what does greet return\?/, "ordinary queued text must still be shown verbatim");
  assert.doesNotMatch(line, /hidden/);
});

test("a credential hidden in a longer queue still hides, and keeps the count", () => {
  const key = "sk-" + "abcdefghijklmnopqrstuvwx";
  const line = pending.preview([key, "second thing"], plain);
  assert.ok(!line.includes(key));
  assert.match(line, /\+1 more/, "the rest of the queue must still be counted");
});
