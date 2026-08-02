"use strict";
// THE TWO DEEP MODULES — `docs/CLI-MASTER-BRIEF.md` §A2, founder ruling 2026-08-02.
//
// Eight symptoms, one session, ONE CAUSE: Estelle had no interaction model, so every prompt, answer and
// menu hand-rolled its own printing. These tests are written against the SYMPTOMS by letter, so a
// regression names the thing the founder actually saw rather than a function that changed.
//
// ⛔ AND THE ACCEPTANCE TEST IS HERE, not implied: "I can scroll back through a session and read it like
// a conversation — what I asked, what it answered, what it asked me, and what I chose."

const test = require("node:test");
const assert = require("node:assert");
const path = require("node:path");

const T = require(path.join(__dirname, "..", "bin", "transcript.js"));
const ask = require(path.join(__dirname, "..", "bin", "ask.js"));

const C = new Proxy({}, { get: () => (s) => String(s === undefined ? "" : s) });
const text = (t) => T.lines(t, C).join("\n");

// ── MODULE 1 — THE TRANSCRIPT ──────────────────────────────────────────────────

test("SYMPTOM a: the customer's OWN INPUT is in the transcript — the one that made it unreadable", () => {
  let t = T.create();
  t = T.append(t, T.user("why is the sweep slow?"));
  t = T.append(t, T.answer("It rebuilds the BM25 corpus every turn."));
  const out = text(t);
  assert.match(out, /why is the sweep slow\?/, "the QUESTION must be in the record, not only the answer");
  assert.match(out, /rebuilds the BM25 corpus/);
  assert.ok(out.indexOf("why is the sweep") < out.indexOf("rebuilds"), "and in the order it happened");
});

test("SYMPTOM c: a byte-identical repeat is suppressed, and a genuine repeat is NOT", () => {
  const block = T.notice("Which repo should I check this against?");
  let t = T.appendOnce(T.appendOnce(T.create(), block), block);
  assert.strictEqual(t.entries.length, 1, "the same block twice in a row is the double-print");
  // The paired positive: a customer really asking the same thing twice must appear twice, or the guard
  // is quietly eating their input — which would be symptom (a) reintroduced by the fix for (c).
  let u = T.append(T.append(T.create(), T.user("hi")), T.user("hi"));
  assert.strictEqual(u.entries.length, 2);
});

test("MODULE 1 is a VALUE that re-renders — a printed line could not survive a reflow", () => {
  let t = T.append(T.create(), T.user("a"));
  const once = text(t), twice = text(t);
  assert.strictEqual(once, twice, "rendering must be pure or a repaint would drift from the record");
});

test("MODULE 1 never mutates — a repaint must never see half an append", () => {
  const before = T.append(T.create(), T.user("a"));
  const snapshot = JSON.stringify(before);
  T.append(before, T.user("b"));
  assert.strictEqual(JSON.stringify(before), snapshot);
});

test("MODULE 1: an unknown kind becomes a notice rather than throwing mid-turn", () => {
  const e = T.normalise({ kind: "wat", text: "x" });
  assert.strictEqual(e.kind, "notice");
  assert.match(text(T.append(T.create(), e)), /x/, "losing the line is worse than a wrong heading");
});

test("MODULE 1: every kind renders through the ONE function and produces visible text", () => {
  for (const kind of T.KINDS) {
    const out = T.renderEntry({ kind, text: `body-${kind}`, meta: { name: "n", question: "q?" } }, C)
      .join("\n");
    assert.match(out, new RegExp(`body-${kind}`), `${kind} rendered nothing`);
  }
});

// ── MODULE 2 — THE INTERACTION SURFACE ─────────────────────────────────────────

test("SYMPTOM b: a choice is SELECTABLE — the escalate list had no input mechanism at all", async () => {
  const said = [];
  const spec = { kind: "choice", question: "Which repo should I check this against?",
                 options: ["isoproof-bravo", "uqeu/estelle"] };
  const r = await ask.ask(spec, { out: (l) => said.push(l), c: C, prompt: async () => "2" });
  assert.strictEqual(r.ok, true);
  assert.strictEqual(r.value, "uqeu/estelle", "typing 2 must choose the second option");
  const screen = said.join("\n");
  assert.match(screen, /1\. isoproof-bravo/, "the options must be NUMBERED, not bulleted prose");
  assert.match(screen, /enter to confirm/, "and it must say how to answer");
});

test("SYMPTOM b (other half): the CHOICE IS RECORDED, so the session reads as a conversation", async () => {
  const spec = { kind: "choice", question: "Which repo?", options: ["a/b", "c/d"] };
  const { result, transcript: t } = await ask.askAndRecord(
    spec, { out: () => {}, c: C, prompt: async () => "c/d" }, T.create());
  assert.strictEqual(result.value, "c/d");
  const out = text(t);
  assert.match(out, /Which repo\?/, "what it asked me");
  assert.match(out, /c\/d/, "and what I chose");
});

test("MODULE 2: a cancelled question is RECORDED as cancelled, never as an empty answer", async () => {
  const spec = { kind: "choice", question: "Which repo?", options: ["a/b"] };
  const { result, transcript: t } = await ask.askAndRecord(
    spec, { out: () => {}, c: C, prompt: async () => null }, T.create());
  assert.strictEqual(result.cancelled, true);
  assert.strictEqual(result.value, null, "cancelled is not the empty string");
  assert.match(text(t), /cancelled/);
});

test("MODULE 2: an AMBIGUOUS answer resolves to null, never to a guess", () => {
  const opts = ["isoproof-bravo", "isoproof-alpha"];
  assert.strictEqual(ask.resolveTyped("isoproof", opts, "choice"), null,
    "silently picking one is how a grounding scope gets chosen for someone");
  assert.strictEqual(ask.resolveTyped("isoproof-b", opts, "choice"), "isoproof-bravo");
  assert.strictEqual(ask.resolveTyped("1", opts, "choice"), "isoproof-bravo");
  assert.strictEqual(ask.resolveTyped("9", opts, "choice"), null, "out of range is not a choice");
});

test("MODULE 2: the keyboard contract is a pure decision, testable with no terminal", () => {
  assert.strictEqual(ask.keyAction({ name: "escape" }), "cancel");
  assert.strictEqual(ask.keyAction({ ctrl: true, name: "c" }), "cancel");
  assert.strictEqual(ask.keyAction({ name: "return" }), "accept");
  assert.strictEqual(ask.keyAction({ name: "up" }), "up");
  assert.strictEqual(ask.keyAction({ name: "space" }), "toggle");
  assert.strictEqual(ask.keyAction({}, "3"), "pick:2");
  assert.strictEqual(ask.keyAction({ name: "x" }, "x"), "");
});

test("MODULE 2: selection wraps in both directions", () => {
  assert.strictEqual(ask.moveSelection(0, -1, 3), 2);
  assert.strictEqual(ask.moveSelection(2, 1, 3), 0);
  assert.strictEqual(ask.moveSelection(0, 1, 0), 0, "an empty list must not divide by zero");
});

test("SYMPTOM h: a secret is read through the MASKED reader, never the echoing one", async () => {
  let maskedUsed = false, plainUsed = false;
  const r = await ask.ask({ kind: "secret", question: "Paste your key" }, {
    out: () => {}, c: C,
    promptSecret: async () => { maskedUsed = true; return "estelle_live_abcdefghijklmnop"; },
    prompt: async () => { plainUsed = true; return "leaked"; },
  });
  assert.ok(maskedUsed, "the masked reader must be the one used");
  assert.ok(!plainUsed, "the echoing reader must never see a secret");
  assert.strictEqual(r.value, "estelle_live_abcdefghijklmnop");
});

test("SYMPTOM h: the recorded entry never contains the secret itself", () => {
  const rec = ask.recordOf({ kind: "secret", question: "Paste your key" },
                           ask.accepted("estelle_live_abcdefghijklmnop"));
  const out = T.renderEntry(rec, C).join("\n");
  assert.ok(!/estelle_live/.test(out), "a transcript is scrolled back to, and screenshotted");
  assert.match(out, /saved/);
});

test("MODULE 2: multi-select returns every pick, and nothing when none match", () => {
  assert.deepStrictEqual(ask.resolveTyped("1 3", ["a", "b", "c"], "multi"), ["a", "c"]);
  assert.strictEqual(ask.resolveTyped("zzz", ["a", "b"], "multi"), null);
});

test("MODULE 2: a confirm reads y/N and defaults to NO", async () => {
  const io = (answer) => ({ out: () => {}, c: C, prompt: async () => answer });
  assert.strictEqual((await ask.ask({ kind: "confirm", question: "raise it?" }, io("y"))).value, true);
  assert.strictEqual((await ask.ask({ kind: "confirm", question: "raise it?" }, io(""))).value, false);
  assert.strictEqual((await ask.ask({ kind: "confirm", question: "raise it?" }, io("nope"))).value, false);
});

test("MODULE 2: an unknown kind cancels rather than falling through to free text", async () => {
  const r = await ask.ask({ kind: "telepathy", question: "?" }, { out: () => {}, c: C, prompt: async () => "x" });
  assert.strictEqual(r.cancelled, true);
});

// ── 🔴 THE ACCEPTANCE TEST ─────────────────────────────────────────────────────

test("ACCEPTANCE: a session scrolls back and reads like a conversation", async () => {
  // "what I asked, what it answered, what it asked me, and what I chose." All four, in order.
  let t = T.create();
  t = T.append(t, T.user("verify this file"));
  const { transcript: t2 } = await ask.askAndRecord(
    { kind: "choice", question: "Which repo should I check this against?",
      options: ["isoproof-bravo", "uqeu/estelle"] },
    { out: () => {}, c: C, prompt: async () => "2" }, t);
  t = T.append(t2, T.answer("Grounded. Every API this file references exists."));

  const out = text(t);
  const at = (re) => out.search(re);
  assert.ok(at(/verify this file/) >= 0, "what I asked");
  assert.ok(at(/Which repo should I check this against\?/) >= 0, "what it asked me");
  assert.ok(at(/uqeu\/estelle/) >= 0, "what I chose");
  assert.ok(at(/Grounded\./) >= 0, "what it answered");
  assert.ok(at(/verify this file/) < at(/Which repo/), "and in the order they happened");
  assert.ok(at(/Which repo/) < at(/Grounded\./));
});

test("ACCEPTANCE (vacuity): the test can SEE the defect it measures", () => {
  // Without this, the acceptance test above would pass on any transcript containing the strings.
  // The pre-fix behaviour is "answers only" — assert that such a transcript FAILS the first clause.
  const answersOnly = T.append(T.create(), T.answer("Grounded."));
  assert.ok(!/verify this file/.test(text(answersOnly)),
    "an answers-only transcript must not satisfy 'what I asked'");
});

// ── THE SEAM — E-027: the pure half plus the impure half is NOT a test of the feature ──────────────
// Both modules above are pure and fully tested. The defect the founder SAW lives in the join: the REPL
// read a line, sent it, printed the answer, and never echoed the question. These drive `runSession`.

const repl = require(path.join(__dirname, "..", "bin", "repl.js"));

async function drive(lines, over) {
  const said = [];
  const queue = [...lines];
  await repl.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async (p) => (p === "/skills" ? { skills: [] } : {}),
    post: async () => ({ answer: "the sweep rebuilds the BM25 corpus every turn" }),
    prompt: async () => (queue.length ? queue.shift() : null),
    out: (l) => said.push(l === undefined ? "" : String(l)),
    c: C, cwd: "estelle", now: () => Date.now(), ...over,
  });
  return said.join("\n");
}

test("SEAM: the REPL echoes the customer's question into the transcript (symptom a, end to end)", async () => {
  const out = await drive(["why is the sweep slow?"]);
  assert.match(out, /why is the sweep slow\?/,
    "the question must reach the scrollback — readline's echo lives on a row alt-screen discards");
  assert.match(out, /rebuilds the BM25 corpus/, "and the answer must still be there");
  assert.ok(out.indexOf("why is the sweep slow?") < out.indexOf("rebuilds the BM25"),
    "question before answer, or the session does not read as a conversation");
});

test("SEAM: a slash command is echoed too — it is part of what I asked", async () => {
  const out = await drive(["/status"]);
  assert.match(out, /\/status/);
});

test("SEAM (vacuity): a bare Enter is NOT echoed — the guard is not just echoing everything", async () => {
  const out = await drive(["   ", "why is the sweep slow?"]);
  const blanks = out.split("\n").filter((l) => /^\s+›\s*$/.test(l));
  assert.strictEqual(blanks.length, 0, "an empty line must not become a transcript entry");
});

test("SYMPTOM h (SEAM): the session's key prompt uses the MASKED reader, never the echoing one", async () => {
  // The old line fell back to the echoing reader whenever `promptSecret` was absent, so masking depended
  // on a caller remembering to pass it. Driven through runSession, not asserted on ask.js in isolation.
  let masked = 0, plain = 0;
  const order = [];
  const said = [];
  // A THROWAWAY $HOME, so `storedKey()` finds nothing and the first-run path is really taken. Without it
  // the real ~/.estelle/auth.json satisfies the key check and this test passes VACUOUSLY by never
  // reaching the prompt at all — the seventh vacuity catch of this campaign, and the third inside a
  // verifier.
  const fs = require("node:fs"), os = require("node:os"), path2 = require("node:path");
  const home = fs.mkdtempSync(path2.join(os.tmpdir(), "estelle-keyprompt-"));
  const realHome = process.env.HOME;
  process.env.HOME = home;
  try {
  await repl.runSession({
    key: "",                                  // no stored key -> the first-run paste path
    get: async () => ({}), post: async () => ({ answer: "ok" }),
    promptSecret: async () => { masked += 1; order.push("masked"); return "estelle_live_abcdefghijklmnopqrs"; },
    prompt: async () => { plain += 1; order.push("plain"); return null; },
    out: (l) => said.push(String(l === undefined ? "" : l)),
    c: C, cwd: "estelle", now: () => Date.now(),
    writeAuth: () => {},
  });
  } finally { process.env.HOME = realHome; fs.rmSync(home, { recursive: true, force: true }); }
  assert.strictEqual(masked, 1, "the masked reader must be the one that reads the key");
  // `plain` also counts the ordinary input loop that runs AFTER the key is saved, so the meaningful
  // assertion is ORDER: the credential read must be the masked one, and it must come first.
  assert.strictEqual(order[0], "masked", "the FIRST read — the credential — must be masked");
  assert.ok(!said.join("\n").includes("estelle_live_abcdefghijklmnopqrs"),
    "the key must never be echoed into the transcript");
});

test("SEAM: bracketed paste is released on exit — the terminal must not be left in that mode", async () => {
  let attached = 0, released = 0;
  await repl.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}), post: async () => ({ answer: "ok" }),
    prompt: async () => null,                 // ctrl-d immediately
    out: () => {}, c: C, cwd: "estelle", now: () => Date.now(),
    bindPaste: () => { attached += 1; return () => { released += 1; }; },
  });
  assert.strictEqual(attached, 1, "paste mode must be attached");
  assert.strictEqual(released, 1, "and released on the way out");
});

test("SYMPTOM d: N shift+tabs leave N mode notices, never N stacked footers", async () => {
  // It shipped "fixed" in 0.1.9 and the in-place arithmetic in mode-ui is correct. 0.2.0 made it an
  // ad-hoc stdout write racing the alt-screen repaint — two writers, one screen, no agreement about who
  // owns a row. The fix is Module 1's rule: the banner goes through the transcript, not through stdout.
  let cycle = null, screenWrite = null;
  const said = [];
  const stdoutWrites = [];
  await repl.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async (p) => (p === "/autonomy/scope" ? { global: "execute" } : {}),
    post: async () => ({ answer: "ok" }),
    prompt: async () => null,
    out: (l) => said.push(String(l === undefined ? "" : l)),
    c: C, cwd: "estelle", now: () => Date.now(),
    altScreen: true,
    altScreenImpl: {
      enter: () => true, leave: () => true, install: () => () => {},
      size: () => ({ rows: 24, columns: 80 }),
      paint: (rows) => { stdoutWrites.push(rows.join("\n")); return true; },
    },
    bindKeys: (fn, write) => { cycle = fn; screenWrite = write; return () => {}; },
  });
  assert.strictEqual(typeof cycle, "function", "the cycle handler must be bound");
  assert.strictEqual(typeof screenWrite, "function",
    "on alt-screen the binder MUST be handed a screen writer — otherwise it writes to stdout and races");

  // Seven presses, exactly as reported.
  for (let i = 0; i < 7; i += 1) {
    const r = await cycle();
    screenWrite(r.banner);
  }
  // THE FINAL FRAME, not every frame concatenated. `paint` receives the WHOLE visible transcript on each
  // repaint, so joining all of them counts every line once per subsequent repaint — which measures the
  // harness, not the screen. What a customer sees is the last frame.
  const painted = stdoutWrites[stdoutWrites.length - 1] || "";
  const footers = (painted.match(/shift\+tab to cycle/g) || []).length;
  assert.ok(footers > 0, "the mode change must be visible at all");
  // Cycling four rungs seven times repeats modes, and a byte-identical repeat is dropped — so the count
  // is bounded by the number of DISTINCT rungs, never by the number of keypresses.
  assert.ok(footers <= 4, `seven presses left ${footers} footers — it must never stack per keypress`);
});

test("SYMPTOM d (spinner): frames go to the STATUS LINE, never to stdout, when a screen is owned", async () => {
  // "The spinner prints a new line per frame — dozens of stacked 'thinking' lines. A stdout write racing
  // the repaint, in THE MOST-EXECUTED PATH IN THE CLI. Module 1's rule has no exceptions."
  //
  // The arithmetic was always right (`\r\x1b[2K` replaces in place). It stacked because screen.js
  // repaints the whole viewport while the spinner wrote straight to stdout — the `\r` returned to
  // whatever row the repaint had left the cursor on.
  const inputUi = require(path.join(__dirname, "..", "bin", "input-ui.js"));
  const writes = [], statuses = [];
  // The spinner only draws after 500ms of WORK, on a 90ms interval — so the body must actually last
  // long enough for a tick, and the injected clock must be past the delay when it fires.
  // withSpinner takes `started = clock()` FIRST, so an incrementing clock makes every later reading
  // relative to a moving origin and elapsed never passes the 500ms delay. Origin at 0, everything after
  // it past the delay.
  let calls = 0;
  const clock = () => (calls++ === 0 ? 0 : 900);
  await inputUi.withSpinner("thinking",
    () => new Promise((r) => setTimeout(r, 260)),
    { status: (t) => statuses.push(t), write: (s) => writes.push(s), now: clock });
  assert.deepStrictEqual(writes, [], "NOTHING may reach stdout when a render pass exists");
  assert.ok(statuses.length >= 1, "the label must reach the status line");
  assert.strictEqual(statuses[statuses.length - 1], null, "and be cleared when the work finishes");
});

test("SYMPTOM d (spinner): with NO render pass the in-place write is still correct", async () => {
  // THE PAIRED NEGATIVE. A plain TTY with alt-screen off owns no viewport, so writing in place is right
  // there — the fix must not silence the spinner on the path where it always worked.
  const inputUi = require(path.join(__dirname, "..", "bin", "input-ui.js"));
  const writes = [];
  let calls = 0;
  const clock = () => (calls++ === 0 ? 0 : 900);
  await inputUi.withSpinner("thinking",
    () => new Promise((r) => setTimeout(r, 260)),
    { write: (s) => writes.push(s), now: clock });
  assert.ok(writes.length >= 1, "the spinner must still draw where nothing else owns the screen");
  assert.ok(writes.every((w) => w.startsWith("\r")), "…and still replace in place, never append");
});
