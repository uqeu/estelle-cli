"use strict";
// THE ALT-SCREEN BOUNDARY — tests for bin/altscreen.js. Release 0.2.0, and it ships ALONE.
//
// ⛔ THE ONE LINE FOR WHOEVER TAKES 0.2.0, from RESUME-next:
//
//   > ASSERT ON DECLARED CODES AND GLYPHS, NEVER ON RENDERED OUTPUT. With colour disabled every painter
//   > returns the bare string, so an output comparison reports every role identical — the test goes green
//   > while a human sees two identical reds. The same trap waits in alt-screen: comparing what was WRITTEN
//   > proves nothing about what is ON SCREEN.
//
// So the split is deliberate and total. `screen.js` owns WHICH ROWS ARE VISIBLE and is tested as DATA
// (screen.test.js). This file owns WHEN THE ESCAPE CODES ARE EMITTED, and is tested against the DECLARED
// constants — never by matching a byte stream against a hand-typed escape sequence, which would pass on a
// typo in both places at once.
//
// 🔴 THE FAILURE THIS FILE EXISTS TO PREVENT: a CLI that exits without leaving the alternate screen has
// EATEN THE USER'S SHELL. Their scrollback is gone, their prompt is gone, and the fix is `reset(1)` — which
// they have to know. It is the worst outcome available in this release, and it is reachable from a crash,
// a Ctrl-C, a SIGTERM from a parent process, or an unhandled rejection three layers down.

const test = require("node:test");
const assert = require("node:assert");
const alt = require("../bin/altscreen.js");

/** A fake tty that records what was written, so emission can be asserted without a terminal. */
function fakeTty(isTTY = true) {
  const writes = [];
  return { isTTY, columns: 80, rows: 24, write: (s) => { writes.push(String(s)); return true; }, writes,
           all: () => writes.join("") };
}

// ── the codes themselves ────────────────────────────────────────────────────────

test("the codes are DECLARED, so a test can name them instead of retyping them", () => {
  // Retyping "\x1b[?1049h" in the test is how a typo passes: the same wrong string in both places compares
  // equal. Naming the constant means the test asserts the CONTRACT, not a transcription.
  assert.equal(typeof alt.CODES.enter, "string");
  assert.equal(typeof alt.CODES.leave, "string");
  assert.notEqual(alt.CODES.enter, alt.CODES.leave, "enter and leave must differ");
  // The one property that is genuinely about the VALUE: 1049 is the private mode that saves the cursor and
  // switches buffer in one sequence. 47 and 1047 do not restore the cursor, and 1048 only saves it.
  assert.match(alt.CODES.enter, /1049h$/);
  assert.match(alt.CODES.leave, /1049l$/);
});

// ── when it may be used at all ──────────────────────────────────────────────────

test("🔴 a NON-TTY never gets alt-screen — piped output must stay clean", () => {
  // `estelle | tee log`, CI, a test harness: emitting escape codes into a pipe corrupts the output for
  // every downstream reader, and there is no human watching a frame anyway.
  assert.equal(alt.shouldUse({}, fakeTty(false)), false);
  assert.equal(alt.shouldUse({}, fakeTty(true)), true);
});

test("ESTELLE_ALT_SCREEN=0 turns it off, and any other value leaves it on", () => {
  assert.equal(alt.shouldUse({ ESTELLE_ALT_SCREEN: "0" }, fakeTty(true)), false);
  assert.equal(alt.shouldUse({ ESTELLE_ALT_SCREEN: "1" }, fakeTty(true)), true);
  assert.equal(alt.shouldUse({}, fakeTty(true)), true);
});

test("TERM=dumb is honoured — it means the terminal cannot do this", () => {
  assert.equal(alt.shouldUse({ TERM: "dumb" }, fakeTty(true)), false);
});

// ── entering and leaving ────────────────────────────────────────────────────────

test("enter writes the declared code once, and a second enter writes nothing", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  const after = tty.all();
  assert.ok(after.includes(alt.CODES.enter), "the declared enter code must be emitted");
  s.enter();
  assert.equal(tty.all(), after, "entering twice must be a no-op — nested enters leak a buffer");
});

test("leave writes the declared code, and leaving twice writes nothing", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  const beforeLeave = tty.writes.length;
  s.leave();
  assert.ok(tty.all().includes(alt.CODES.leave));
  assert.ok(tty.writes.length > beforeLeave);
  const after = tty.all();
  s.leave();
  assert.equal(tty.all(), after, "leaving twice must be a no-op");
});

test("🔴 leave WITHOUT enter writes nothing — it must not emit a stray reset", () => {
  // If alt-screen was never entered (non-TTY, or the flag off), the exit handler still runs. Emitting a
  // leave there prints garbage into a pipe, which is the exact thing shouldUse exists to prevent.
  const tty = fakeTty();
  const s = alt.create(tty);
  s.leave();
  assert.equal(tty.all(), "", "a leave with no matching enter must be silent");
});

test("the cursor is hidden on enter and SHOWN AGAIN on leave", () => {
  // A hidden cursor left behind is the same class of damage as a borrowed screen: the user's shell still
  // works, but they cannot see where they are typing.
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  assert.ok(tty.all().includes(alt.CODES.hideCursor));
  s.leave();
  assert.ok(tty.all().includes(alt.CODES.showCursor), "the cursor must be restored");
});

test("leave emits the codes in the order that leaves a working terminal", () => {
  // Show the cursor and leave the buffer — if the cursor is shown AFTER the switch it is restored on the
  // wrong screen, which is the bug that makes this look fixed while the prompt stays invisible.
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  tty.writes.length = 0;
  s.leave();
  const out = tty.all();
  assert.ok(out.indexOf(alt.CODES.showCursor) < out.indexOf(alt.CODES.leave),
    "show the cursor BEFORE switching back, or it is restored on the alternate buffer");
});

// ── the restore guarantee ───────────────────────────────────────────────────────

test("🔴 install() restores on EVERY exit path, not just the happy one", () => {
  // A CLI that exits inside the alternate screen has eaten the user's shell — scrollback gone, prompt
  // gone, and the fix is `reset(1)`, which they have to know. Reachable from a crash, a Ctrl-C, a SIGTERM
  // from a parent, or an unhandled rejection three layers down.
  const tty = fakeTty();
  const s = alt.create(tty);
  const hooks = {};
  const proc = {
    on: (ev, fn) => { (hooks[ev] = hooks[ev] || []).push(fn); },
    removeListener: (ev, fn) => { hooks[ev] = (hooks[ev] || []).filter((f) => f !== fn); },
    exit: () => {},
  };
  s.enter();
  const uninstall = s.install(proc);
  // Every signal and error path a terminal app can die on must be registered.
  for (const ev of ["exit", "SIGINT", "SIGTERM", "SIGHUP", "uncaughtException", "unhandledRejection"]) {
    assert.ok((hooks[ev] || []).length, `nothing restores the terminal on ${ev}`);
  }
  uninstall();
});

test("the exit handler actually LEAVES — registering is not restoring", () => {
  // Asserting that a handler was registered proves a listener exists, not that it does anything. This
  // fires it and checks the declared code came out.
  const tty = fakeTty();
  const s = alt.create(tty);
  const hooks = {};
  const proc = { on: (ev, fn) => { (hooks[ev] = hooks[ev] || []).push(fn); },
                 removeListener: () => {}, exit: () => {} };
  s.enter();
  s.install(proc);
  tty.writes.length = 0;
  for (const fn of hooks.exit) fn(0);
  assert.ok(tty.all().includes(alt.CODES.leave), "the exit handler did not leave the alternate screen");
});

test("a SIGINT handler restores AND re-raises — swallowing Ctrl-C is its own bug", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  const hooks = {}; let exited = null;
  const proc = { on: (ev, fn) => { (hooks[ev] = hooks[ev] || []).push(fn); },
                 removeListener: () => {}, exit: (c) => { exited = c; } };
  s.enter();
  s.install(proc);
  for (const fn of hooks.SIGINT) fn();
  assert.ok(tty.all().includes(alt.CODES.leave), "SIGINT must leave the alternate screen");
  assert.equal(exited, 130, "Ctrl-C must still terminate, with the conventional 128+SIGINT status");
});

test("uninstall removes the handlers it added — a long session must not leak listeners", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  const hooks = {};
  const proc = {
    on: (ev, fn) => { (hooks[ev] = hooks[ev] || []).push(fn); },
    removeListener: (ev, fn) => { hooks[ev] = (hooks[ev] || []).filter((f) => f !== fn); },
    exit: () => {},
  };
  s.enter();
  s.install(proc)();
  for (const ev of Object.keys(hooks)) assert.equal(hooks[ev].length, 0, `${ev} listener was left behind`);
});

// ── painting ────────────────────────────────────────────────────────────────────

test("paint writes exactly the rows it was given, positioned from the top", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  tty.writes.length = 0;
  s.paint(["one", "two"]);
  const out = tty.all();
  assert.ok(out.includes("one") && out.includes("two"));
  assert.ok(out.includes(alt.CODES.home), "a frame must start from a known cursor position");
});

test("paint CLEARS each row it writes — a shorter frame must not leave the old one behind", () => {
  // Without a per-row clear, a long line followed by a short one leaves the long line's tail on screen,
  // which is exactly the corruption §2.3 reported.
  const tty = fakeTty();
  const s = alt.create(tty);
  s.enter();
  tty.writes.length = 0;
  s.paint(["short"]);
  assert.ok(tty.all().includes(alt.CODES.clearLine), "each painted row must clear to end-of-line");
});

test("paint is inert when alt-screen was never entered", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  s.paint(["nothing should reach the stream"]);
  assert.equal(tty.all(), "", "painting outside the alternate screen would overwrite the user's terminal");
});

// ── the viewport size ───────────────────────────────────────────────────────────

test("size reports the terminal's rows and columns, with a sane floor", () => {
  const tty = fakeTty();
  const s = alt.create(tty);
  assert.deepEqual(s.size(), { rows: 24, columns: 80 });
  const tiny = fakeTty(); tiny.rows = 0; tiny.columns = 0;
  const s2 = alt.create(tiny);
  const sz = s2.size();
  assert.ok(sz.rows >= 1 && sz.columns >= 20, "a zero-size terminal must not produce a zero-row viewport");
});

// ── THE SEAM — repl.js × altscreen.js × screen.js ──────────────────────────────
//
// 🔴 E-027, applied to the release it was learned in. The 0.1.10 crash survived 348 green tests because
// slash-menu.js was tested as pure functions, the keypress handler was tested separately, and THE JOIN
// BETWEEN THEM WAS NEVER CALLED. screen.js has 19 tests and altscreen.js has 17 above; neither proves the
// session actually connects them. These do.

const repl = require("../bin/repl.js");
const screenModel = require("../bin/screen.js");

/** A recording alt-screen impl, so the seam can be driven with no terminal. */
function recorder(rows = 24, columns = 80) {
  const painted = [];
  let entered = false, left = false, installed = false, uninstalled = false;
  return {
    painted,
    get entered() { return entered; }, get left() { return left; },
    get installed() { return installed; }, get uninstalled() { return uninstalled; },
    enter() { entered = true; return true; },
    leave() { left = true; return true; },
    size: () => ({ rows, columns }),
    paint(list) { painted.push(list.slice()); return true; },
    install() { installed = true; return () => { uninstalled = true; }; },
  };
}

async function driveSession(over) {
  const queue = ["/exit"];
  return repl.runSession({
    key: "estelle_live_9f2b7c1d4e6a8b0c2d4e3f9",
    get: async () => ({}), post: async () => ({ answer: "ok" }),
    prompt: async () => (queue.length ? queue.shift() : null),
    out: () => {}, c: new Proxy({}, { get: () => (s) => String(s) }),
    cwd: "estelle", now: () => Date.now(),
    ...over,
  });
}

test("🔴 THE SEAM: with alt-screen on, the session ENTERS, PAINTS, and LEAVES", async () => {
  const rec = recorder();
  await driveSession({ altScreen: true, altScreenImpl: rec });
  assert.ok(rec.entered, "the session never entered the alternate screen");
  assert.ok(rec.painted.length > 0, "the session entered and then painted NOTHING");
  assert.ok(rec.installed, "the restore handlers were never installed");
  assert.ok(rec.left, "🔴 THE SESSION EXITED WITHOUT LEAVING — this eats the user's shell");
  assert.ok(rec.uninstalled, "the exit handlers were leaked");
});

test("🔴 the header actually REACHES the frame — painting is not the same as painting the content", async () => {
  // `painted.length > 0` above would pass on a session that painted empty frames forever. This asserts the
  // transcript is what is on screen: the account line the header prints must appear in a painted row.
  const rec = recorder();
  await driveSession({
    altScreen: true, altScreenImpl: rec,
    get: async (p) => (p === "/account" ? { email: "seam@example.com", plan: "ultra" } : {}),
  });
  const everything = rec.painted.flat().join("\n");
  assert.match(everything, /seam@example\.com/, "the header never reached the alternate screen");
});

test("with alt-screen OFF the session writes through the caller's out, and never touches the screen", async () => {
  // The inert path must be byte-identical to what shipped before 0.2.0 — this is the escape hatch working.
  const rec = recorder();
  const lines = [];
  await driveSession({ altScreen: false, altScreenImpl: rec, out: (l) => lines.push(l) });
  assert.equal(rec.entered, false, "alt-screen was disabled and the session entered it anyway");
  assert.equal(rec.painted.length, 0);
  assert.ok(lines.length > 0, "with alt-screen off the caller's `out` must still receive the session");
});

test("🔴 the viewport RESERVES rows for the prompt — a frame that fills the screen hides the input line", async () => {
  const rec = recorder(10, 80);
  await driveSession({ altScreen: true, altScreenImpl: rec,
                       get: async () => ({ memory: { memories: 5 } }) });
  for (const frame of rec.painted) {
    assert.ok(frame.length <= 8, `a frame used ${frame.length} of 10 rows — readline has nowhere to draw`);
  }
});

test("scrolling moves the viewport and does not lose the bottom", () => {
  // The behaviour §2.3 exists to fix, asserted through the model the session actually drives.
  let v = screenModel.create({ width: 80 });
  for (let i = 1; i <= 30; i += 1) v = screenModel.append(v, `line ${i}`, 80);
  assert.equal(screenModel.visible(v, 5).atBottom, true);
  const up = screenModel.scroll(v, -4, 5);
  assert.equal(screenModel.visible(up, 5).atBottom, false);
  assert.equal(screenModel.visible(screenModel.toBottom(up), 5).lines.at(-1), "line 30");
});
