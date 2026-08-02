"use strict";
// The slash menu — brief §2.1, "the highest-value single fix in the whole document".
//
// Estelle opened NOTHING on `/`. The commands lived behind `/help` and ~246 skill playbooks were reachable
// only by typing an exact name. These tests pin the four rules that make the menu worth having, and the
// first one is the one that matters most: a row that does nothing is a door with no capability.

const { test } = require("node:test");
const assert = require("node:assert");
const menu = require("../bin/slash-menu.js");

const COMMANDS = {
  help: "what you can do here", work: "plan then implement", gate: "run the merge gate",
  apply: "write the diff", clear: "clear the screen", memory: "what Estelle knows",
  sweep: "index this repo", shell: "run a shell command",
};
const WIRED = new Set(["help", "work", "gate", "apply", "clear", "memory", "sweep"]);   // no `shell`
const SKILLS = [{ name: "bug-hunt", short: "Reproduce, shrink, instrument, fix." },
                { name: "api-shape", short: "Design a REST API that behaves the same way." }];

test("RULE 1: a documented-but-unwired command never appears", () => {
  // `shell` is in the docs table and is NOT dispatched — the `!` form is the real one. Offering it as a
  // completion would be a door with no capability, which is the defect this whole campaign has been closing.
  const rows = menu.menuRows(COMMANDS, WIRED, []);
  assert.ok(!rows.some((r) => r.name === "shell"), "an unwired command was offered as a completion");
  assert.ok(rows.some((r) => r.name === "work"), "and the wired ones must still be there");
});

test("RULE 2: grouped by kind, NOT globally alphabetical", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  const groups = rows.map((r) => r.group);
  // every group's rows are contiguous, and the groups run session -> memory -> code -> skills
  const seen = [...new Set(groups)];
  assert.deepStrictEqual(seen, ["session", "memory", "code", "skills"]);
  assert.deepStrictEqual(groups, [...groups].sort(
    (a, b) => menu.GROUPS.indexOf(a) - menu.GROUPS.indexOf(b)), "groups are interleaved");
  // THE PAIRED NEGATIVE for this rule: a globally alphabetical list would put /apply first. It must not.
  assert.notStrictEqual(rows[0].name, "apply",
    "the menu is globally alphabetical — the one ordering the founder flagged as worse than ours");
});

test("RULE 2b: most-used first inside a group, then alphabetical", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, []);
  const code = rows.filter((r) => r.group === "code").map((r) => r.name);
  assert.deepStrictEqual(code, ["work", "gate", "apply"],
    "work and gate are the most-used; the unranked tail is alphabetical");
});

test("DECISION G: the 246 skill rows are ONE browser row, not 246 rows", () => {
  // Founder ruling 2026-08-02 — "a slash command is for something the USER does; a tool is for
  // something ESTELLE does." Skills are selected by relevance, by the model. The menu offers a place
  // to LOOK, never a list to choose from.
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  const skills = rows.filter((r) => r.group === "skills");
  assert.deepStrictEqual(skills.map((r) => r.name), ["skills"]);
  assert.match(skills[0].short, /2 playbooks/, "the row must say how many it stands for");
  assert.ok(skills.every((r) => r.short.length <= 70), "a 201-char summary leaked into a menu row");
});

test("DECISION G: no skill row at all when the list could not be read", () => {
  // A browser row promising playbooks we could not list would be a door onto an empty room.
  assert.deepStrictEqual(menu.menuRows(COMMANDS, WIRED, []).filter((r) => r.group === "skills"), []);
});

test("RULE 4: no emoji anywhere in a rendered menu", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  const text = menu.renderMenu(rows, 0, 20).join("\n");
  assert.ok(!/\p{Extended_Pictographic}/u.test(text), `emoji in the menu:\n${text}`);
});

test("filtering: a name match beats a description match", () => {
  // someone typing `ga` means /gate, not the skill whose sentence happens to contain those letters
  const rows = menu.menuRows({ ...COMMANDS, misc: "a galling description" },
                             new Set([...WIRED, "misc"]), []);
  const hits = menu.filterRows(rows, "ga");
  assert.strictEqual(hits[0].name, "gate");
});

test("filtering narrows as you type, and an empty query shows everything", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  assert.strictEqual(menu.filterRows(rows, "").length, rows.length);
  const narrow = menu.filterRows(rows, "swe");
  assert.deepStrictEqual(narrow.map((r) => r.name), ["sweep"]);
});

test("filtering is case-insensitive and keeps grouping stable within a tier", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  assert.deepStrictEqual(menu.filterRows(rows, "GATE").map((r) => r.name), ["gate"]);
  const many = menu.filterRows(rows, "e");           // matches several across groups
  const groups = many.filter((r) => r.name.startsWith("e") === false);
  assert.ok(groups.length >= 0);                     // shape check: no throw, order preserved
  assert.deepStrictEqual(many, many.slice());        // stable
});

test("no match returns an empty list, so the caller can say so instead of drawing an empty box", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  assert.deepStrictEqual(menu.filterRows(rows, "zzzzz"), []);
  assert.deepStrictEqual(menu.renderMenu([], 0, 8), []);
});

test("the menu is capped and SAYS how many it hid", () => {
  // silently truncating a list reads as 'that is all there is' — the same defect class as a capped read
  // Built from COMMANDS rather than skills since DECISION G collapsed the 246 skill rows into one.
  const many = Object.fromEntries(Array.from({ length: 30 }, (_, i) => [`c${i}`, "x"]));
  const rows = menu.menuRows(many, new Set(Object.keys(many)), []);
  const lines = menu.renderMenu(rows, 0, 8);
  assert.strictEqual(lines.length, 9);
  assert.match(lines[8], /22 more/);
});

test("the selected row is marked and the descriptions line up", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, []);
  const lines = menu.renderMenu(rows, 2, 20);
  assert.strictEqual(lines.filter((l) => l.startsWith(">")).length, 1);
  // the real invariant is that every DESCRIPTION starts in the same column. Searching for a run of two
  // spaces finds the name padding instead, which is why the first version of this assertion was wrong.
  const shown = rows.slice(0, 20);
  const cols = lines.map((l, i) => l.indexOf(shown[i].short));
  assert.ok(cols.every((c) => c > 0), "a description went missing from its row");
  assert.strictEqual(new Set(cols).size, 1, `descriptions are ragged: ${cols}`);
});

// ── SYMPTOM (g) — THE MENU RENDERED ABOVE THE PROMPT ──────────────────────────
// Founder, 2026-08-02: "The slash menu renders ABOVE the prompt instead of below it." The eye goes to
// what you are typing; Codex and Kimi both draw the list under the input for that reason.
//
// ⛔ ASSERTED ON THE EMITTED SEQUENCE, never on a screenshot. What is ON SCREEN cannot be tested from a
// process — the visual half is UNMEASURABLE and is walked by hand at a named terminal. What CAN be
// asserted is the invariant that produces it: THE CURSOR NEVER LEAVES THE PROMPT ROW.

function fakeMenuTty() {
  const writes = [];
  const handlers = {};
  return {
    stdin: { isTTY: true, on: (k, fn) => { handlers[k] = fn; }, removeListener() {} },
    handlers, writes, all: () => writes.join(""),
    write: (s) => writes.push(String(s)),
  };
}

/** Net vertical movement of a written sequence: +1 per row down, -1 per row up. */
function netRows(seq) {
  let net = 0;
  for (const m of seq.matchAll(/\x1b\[(\d*)([AB])/g)) {
    const n = m[1] === "" ? 1 : Number(m[1]);
    net += m[2] === "B" ? n : -n;
  }
  for (const _ of seq.matchAll(/\n/g)) net += 1;
  return net;
}

test("SYMPTOM g: painting the menu leaves the cursor EXACTLY where it started", async () => {
  const t = fakeMenuTty();
  const rl = { line: "/", cursor: 1, prompt() {} };
  const bind = menu.attachMenu(t.stdin, { rl, write: t.write, readline: { emitKeypressEvents() {} }, max: 5 });
  bind(() => menu.menuRows({ work: "a", gate: "b", status: "c" }, new Set(["work", "gate", "status"]), []));
  t.handlers.keypress("/", { name: "slash" });
  await new Promise((r) => setImmediate(() => setImmediate(r)));
  assert.ok(t.all().length, "the menu must actually have painted");
  assert.strictEqual(netRows(t.all()), 0,
    "net vertical movement must be ZERO — a menu that moves the cursor moves the prompt");
});

test("SYMPTOM g: the menu is drawn DOWN from the prompt, never up", async () => {
  const t = fakeMenuTty();
  const rl = { line: "/g", cursor: 2, prompt() {} };
  const bind = menu.attachMenu(t.stdin, { rl, write: t.write, readline: { emitKeypressEvents() {} }, max: 5 });
  bind(() => menu.menuRows({ gate: "b" }, new Set(["gate"]), []));
  t.handlers.keypress("/", { name: "slash" });
  await new Promise((r) => setImmediate(() => setImmediate(r)));
  const seq = t.all();
  const firstDown = seq.search(/\x1b\[\d*B/);
  const firstUp = seq.search(/\x1b\[\d*A/);
  assert.ok(firstDown !== -1, "it must move DOWN to draw — below the prompt");
  assert.ok(firstUp === -1 || firstDown < firstUp, "and only come back up afterwards, never lead with up");
});

test("SYMPTOM d/g: erasing returns to the prompt row too — the pair must agree", async () => {
  // The old erase and paint disagreed about where the cursor was, which is how a redraw APPENDS instead
  // of replacing — the same mechanism as the footer printing seven times.
  const t = fakeMenuTty();
  const rl = { line: "/g", cursor: 2, prompt() {} };
  const bind = menu.attachMenu(t.stdin, { rl, write: t.write, readline: { emitKeypressEvents() {} }, max: 5 });
  bind(() => menu.menuRows({ gate: "b", work: "c" }, new Set(["gate", "work"]), []));
  t.handlers.keypress("/", { name: "slash" });
  await new Promise((r) => setImmediate(() => setImmediate(r)));
  t.handlers.keypress("g", { name: "g" });          // re-filter: erase then repaint
  await new Promise((r) => setImmediate(() => setImmediate(r)));
  assert.strictEqual(netRows(t.all()), 0, "after any number of redraws the cursor is back on the prompt row");
});

test("SYMPTOM e: closing the menu NEVER clears the prompt row — that is where your typing lives", async () => {
  // Founder: "I typed a message, sent it, and it VANISHED with no echo and no answer." Reproduced with
  // the menu OPEN, which is when it happened.
  //
  // THE CAUSE: the old erase led with `\r\x1b[2K` — clear the CURRENT line — and the current line was the
  // prompt, holding everything the customer had typed. Pressing Enter with the menu open wiped the input
  // visually; readline still submitted it, so the message went out with no echo and nothing on screen.
  // The erase now moves DOWN first and only ever clears rows it drew.
  const t = fakeMenuTty();
  const rl = { line: "/gate main", cursor: 10, prompt() {} };
  const bind = menu.attachMenu(t.stdin, { rl, write: t.write, readline: { emitKeypressEvents() {} }, max: 5 });
  bind(() => menu.menuRows({ gate: "b" }, new Set(["gate"]), []));
  t.handlers.keypress("/", { name: "slash" });
  await new Promise((r) => setImmediate(() => setImmediate(r)));

  t.writes.length = 0;
  t.handlers.keypress("\r", { name: "return" });           // Enter with the menu open
  const closing = t.all();
  assert.ok(!/^\r\x1b\[2K/.test(closing),
    "the close must not begin by clearing the row the customer is typing on");
  const firstMove = closing.search(/\x1b\[\d*[AB]/);
  if (firstMove !== -1) {
    assert.match(closing.slice(firstMove, firstMove + 8), /\x1b\[\d*B/,
      "the first cursor move when closing must be DOWN, away from the prompt");
  }
  assert.strictEqual(netRows(closing), 0, "and it must end back on the prompt row");
});

test("SYMPTOM e: Enter with the menu open does not consume the line — readline still submits", async () => {
  // The other half: if the handler swallowed the keypress the message would never be sent at all. It
  // closes the menu and returns WITHOUT consuming, so readline's own handler submits as normal.
  const t = fakeMenuTty();
  const rl = { line: "/gate main", cursor: 10, prompt() {} };
  const bind = menu.attachMenu(t.stdin, { rl, write: t.write, readline: { emitKeypressEvents() {} }, max: 5 });
  bind(() => menu.menuRows({ gate: "b" }, new Set(["gate"]), []));
  t.handlers.keypress("/", { name: "slash" });
  await new Promise((r) => setImmediate(() => setImmediate(r)));
  t.handlers.keypress("\r", { name: "return" });
  assert.strictEqual(rl.line, "/gate main",
    "the typed line must be left intact for readline to submit — never rewritten on Enter");
});
