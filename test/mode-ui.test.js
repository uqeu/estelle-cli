"use strict";
// The mode SWITCH — the visible half of the ceiling. Everything asserted here is pure except the key
// binding, which is exercised against a fake stream so the test needs no terminal.
const test = require("node:test");
const assert = require("node:assert");
const { EventEmitter } = require("node:events");
const ui = require("../bin/mode-ui.js");
const local = require("../bin/session-commands.js");

const C = new Proxy({}, { get: () => (t) => String(t) });   // colours off, so assertions read plainly

// ── the key ─────────────────────────────────────────────────────────────────────

test("shift+tab is the cycle key — plain tab and ctrl+tab are not", () => {
  // Node's keypress decoder turns the terminal's back-tab sequence (ESC [ Z) into {name:tab,shift:true};
  // a PLAIN tab is a character readline inserts into the line, so binding it would eat the user's input.
  assert.equal(ui.isCycleKey({ name: "tab", shift: true }), true);
  assert.equal(ui.isCycleKey({ name: "tab", shift: false }), false);
  assert.equal(ui.isCycleKey({ name: "tab", shift: true, ctrl: true }), false);
  assert.equal(ui.isCycleKey({ name: "tab", shift: true, meta: true }), false);
  assert.equal(ui.isCycleKey({ name: "z", shift: true }), false);
  assert.equal(ui.isCycleKey(null), false);
  assert.equal(ui.isCycleKey(undefined), false);
});

// ── what the cycle may reach ────────────────────────────────────────────────────

test("the cycle only walks rungs the account can actually reach", () => {
  // A switch that lets you select something that does nothing is the switch-wired-to-nothing this work
  // exists to remove. When the server dial is known, the reachable set STOPS there.
  assert.deepEqual(ui.cycleModes("branch"), ["read_only", "propose", "branch"]);
  assert.deepEqual(ui.cycleModes("read_only"), ["read_only"]);
  assert.deepEqual(ui.cycleModes("execute"), ["read_only", "propose", "branch", "execute"]);
});

test("an UNKNOWN dial offers every rung — we cannot prove one is unreachable", () => {
  // Fail-closed applies to what we DO, not to what we let a user select: the effective mode is still
  // min(local, server) at the moment of action, and the indicator says the dial is unverified.
  assert.deepEqual(ui.cycleModes(""), ["read_only", "propose", "branch", "execute"]);
  assert.deepEqual(ui.cycleModes(null), ["read_only", "propose", "branch", "execute"]);
  assert.deepEqual(ui.cycleModes("nonsense"), ["read_only", "propose", "branch", "execute"]);
});

test("cycling wraps within the reachable set and never lands above the ceiling", () => {
  assert.equal(ui.nextMode("read_only", "branch"), "propose");
  assert.equal(ui.nextMode("propose", "branch"), "branch");
  assert.equal(ui.nextMode("branch", "branch"), "read_only");        // wraps, never to execute
  assert.equal(ui.nextMode("read_only", "read_only"), "read_only");  // a one-rung account cannot move
});

test("a local mode ABOVE the ceiling is pulled back into the set, not carried around it", () => {
  // Someone types `/mode execute` on a propose account, then presses shift+tab. The next rung must be a
  // real one, not execute-plus-one.
  assert.equal(ui.nextMode("execute", "propose"), "read_only");
  assert.equal(ui.nextMode("garbage", "propose"), "read_only");
});

// ── what the eye sees ───────────────────────────────────────────────────────────

test("the prompt carries the EFFECTIVE mode — the only thing true of the next command", () => {
  // The names here are the DISPLAY names: `execute` prints as `auto`, `read_only` as `read`. The founder
  // designed these modes and could not tell what `read_only` meant while looking at it.
  assert.equal(ui.promptLabel("propose", "execute"), "propose");
  assert.equal(ui.promptLabel("execute", "propose"), "auto→propose");      // clamped, and it shows
  assert.equal(ui.promptLabel("propose", ""), "propose?");                 // dial unverified
});

test("the display name is DISPLAY ONLY — the value underneath is still the server's rung", () => {
  // The whole risk of renaming a privilege ladder for humans is that the rename becomes the value. It
  // must not: parseMode still resolves every word to the rung, and the ladder test would fail otherwise.
  assert.equal(local.modeName("read_only"), "read");
  assert.equal(local.modeName("execute"), "auto");
  assert.equal(local.parseMode("read"), "read_only");
  assert.equal(local.parseMode("auto"), "execute");
  assert.equal(local.parseMode("read_only"), "read_only");   // the rung's own name still works
  assert.equal(local.modeName("nonsense"), "nonsense");      // unknown is shown, never hidden
});

test("the banner names the mode, what it permits, and the cycle key", () => {
  const line = ui.modeBanner("propose", "execute", C);
  assert.match(line, /propose/);
  assert.match(line, /shift\+tab/);
  assert.match(line, /reviewable PR/);            // MODE_WHAT, not a bare word
});

test("the banner says outright when the account clamps the mode", () => {
  const line = ui.modeBanner("execute", "propose", C);
  assert.match(line, /propose/);
  assert.match(line, /clamped|cannot raise|account/i);
});

test("the banner flags an unverified dial rather than implying the mode is granted", () => {
  const line = ui.modeBanner("branch", "", C);
  assert.match(line, /unverified|unknown/i);
});

// ── binding, and the non-TTY case ───────────────────────────────────────────────

test("on a NON-TTY the binder is inert — it never touches the stream", () => {
  // Piped stdin and CI: raw-mode key handling would corrupt the output stream and there is no human to
  // press anything. Returning a no-op unbind keeps the caller's teardown honest.
  const stdin = new EventEmitter();
  stdin.isTTY = false;
  let emitted = 0;
  const bind = ui.keyBinder(stdin, { write: () => {} });
  const unbind = bind(async () => { emitted += 1; return { banner: "", prompt: "" }; });
  assert.equal(typeof unbind, "function");
  assert.equal(stdin.listenerCount("keypress"), 0, "a non-TTY must not be listened to");
  unbind();
});

test("on a TTY shift+tab runs the cycle, prints the banner, and redraws the prompt", async () => {
  const stdin = new EventEmitter();
  stdin.isTTY = true;
  const written = [];
  const rl = { setPrompt: (p) => written.push(`PROMPT:${p}`), prompt: () => written.push("REDRAW") };
  const bind = ui.keyBinder(stdin, { write: (s) => written.push(s), rl,
                                     readline: { emitKeypressEvents: () => {} } });
  let calls = 0;
  const unbind = bind(async () => { calls += 1; return { banner: "  mode → branch", prompt: "branch › " }; });

  stdin.emit("keypress", "", { name: "tab", shift: true });
  await new Promise((r) => setImmediate(r));
  assert.equal(calls, 1);
  assert.ok(written.some((w) => w.includes("mode → branch")), "the banner must be shown");
  assert.ok(written.includes("PROMPT:branch › "), "the prompt must carry the new mode");
  assert.ok(written.includes("REDRAW"), "the prompt line must be redrawn, not left half-erased");

  stdin.emit("keypress", "x", { name: "x" });
  await new Promise((r) => setImmediate(r));
  assert.equal(calls, 1, "an ordinary key must not cycle the mode");

  unbind();
  stdin.emit("keypress", "", { name: "tab", shift: true });
  await new Promise((r) => setImmediate(r));
  assert.equal(calls, 1, "unbind must actually detach");
});

test("a cycle that throws never takes the session down with it", async () => {
  const stdin = new EventEmitter();
  stdin.isTTY = true;
  const written = [];
  const bind = ui.keyBinder(stdin, { write: (s) => written.push(s), rl: null,
                                     readline: { emitKeypressEvents: () => {} } });
  const unbind = bind(async () => { throw new Error("scope fetch died"); });
  stdin.emit("keypress", "", { name: "tab", shift: true });
  await new Promise((r) => setImmediate(r));
  await new Promise((r) => setImmediate(r));
  assert.ok(written.join("").length >= 0);            // reaching here at all is the assertion
  unbind();
});

// ── THE FOOTER THAT PRINTED FOUR TIMES ────────────────────────────────────────────────────────────────
// Observed literally on the shipped CLI: four shift+tabs left four identical footers stacked up the
// screen. Each press wiped only the half-drawn PROMPT line and then APPENDED a fresh banner. The
// interaction was ported from Codex (mode-ui.js:6-14 says so); the rendering was not — Codex owns a
// bottom pane and repaints it, we printed and moved on. One footer, redrawn in place.
//
// This asserts the ESCAPE BYTES, because that is the whole defect. A test that only checked "the banner
// text is present" passes on the broken version — it was present four times.
function cycleHarness() {
  const stdin = new EventEmitter();
  stdin.isTTY = true;
  const written = [];
  const lineHandlers = [];
  const rl = { setPrompt: () => {}, prompt: () => {},
               on: (ev, fn) => { if (ev === "line") lineHandlers.push(fn); },
               removeListener: () => {} };
  const bind = ui.keyBinder(stdin, { write: (s) => written.push(s), rl,
                                     readline: { emitKeypressEvents: () => {} } });
  const unbind = bind(async () => ({ banner: "  read · nothing is written", prompt: "read › " }));
  const press = async () => {
    stdin.emit("keypress", "", { name: "tab", shift: true });
    await new Promise((r) => setImmediate(r));
  };
  const submitLine = () => lineHandlers.forEach((fn) => fn("hello"));
  return { press, submitLine, unbind, screen: () => written.join(""),
           banners: () => written.filter((w) => w.includes("nothing is written")).length,
           erases: () => written.filter((w) => w.includes("\x1b[1A")).length };
}

test("cycling four times leaves ONE footer on screen, not four", async () => {
  const h = cycleHarness();
  await h.press();
  assert.equal(h.erases(), 0, "the FIRST footer has nothing above it to erase");
  await h.press(); await h.press(); await h.press();
  assert.equal(h.banners(), 4, "each press must still draw a footer");
  assert.equal(h.erases(), 3, "presses 2-4 must each erase the footer they replace");
  h.unbind();
});

test("after a submitted line the footer is APPENDED, never erased over the transcript", async () => {
  // The safety half, and it matters more than the fix: the previous banner is only the line directly
  // above while nothing has been emitted since. Erasing the wrong line is a worse bug than the one being
  // fixed, so a submitted line resets the claim.
  const h = cycleHarness();
  await h.press();
  h.submitLine();
  await h.press();
  assert.equal(h.erases(), 0, "it erased a line of the customer's transcript");
  assert.equal(h.banners(), 2);
  h.unbind();
});
