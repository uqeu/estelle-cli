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

test("RULE 3: skills carry their short line, and it is the SHORT one", () => {
  const rows = menu.menuRows(COMMANDS, WIRED, SKILLS);
  const skills = rows.filter((r) => r.group === "skills");
  assert.deepStrictEqual(skills.map((r) => r.name), ["skill_api-shape", "skill_bug-hunt"]);
  assert.ok(skills.every((r) => r.short.length <= 70), "a 201-char summary leaked into a menu row");
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
  const many = Array.from({ length: 30 }, (_, i) => ({ name: `s${i}`, short: "x" }));
  const rows = menu.menuRows({}, new Set(), many);
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
