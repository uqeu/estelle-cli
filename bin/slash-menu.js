"use strict";
// THE SLASH MENU — brief §2.1, "the highest-value single fix in the whole document".
//
// Codex and Kimi both open a filtered command list on `/`. Estelle opened NOTHING: the commands existed
// behind `/help` and were undiscoverable, and ~246 skill playbooks were reachable ONLY by typing an exact
// name. A capability with no door.
//
// Everything here is PURE — ordering, filtering, and the rendered row. The keypress handling lives in the
// REPL because it needs readline; this file is what the tests can pin, which is the same split
// `mode-ui.js` uses for the same reason.
//
// FOUR RULES, each one earned:
//
//   1. ⛔ EVERY ROW MUST BE WIRED BEFORE IT APPEARS. A menu row that does nothing is a door with no
//      capability — defect class 4 inverted, and this repo has spent a whole campaign closing its mirror.
//      `menuRows` takes the set of DISPATCHABLE names and drops anything else, so a documented-but-dead
//      command (`shell`, which is really the `!` form) can never be offered as a completion.
//   2. GROUPED BY KIND — session · memory · code · skills — most-used first WITHIN a group, then
//      alphabetical. **Deliberately NOT globally alphabetical:** that is the one thing the founder flagged
//      as worse in Kimi, because it buries `/work` under `/apply` and `/clear`.
//   3. SHORT DESCRIPTIONS ONLY. The skills' own `summary` averages 201 characters and cannot be truncated
//      into a row without landing mid-clause (measured), which is why `Skill.short` exists and why the
//      server ships `menu_line` rather than the CLI keeping its own table (RULING 1).
//   4. NO EMOJI. Standing house rule; Kimi uses them, we do not.

// Group order, and the rank of the most-used commands inside each group. Anything unranked sorts
// alphabetically after the ranked ones — so adding a command needs no decision here, it just lands in the
// tail rather than silently jumping the queue.
const GROUPS = ["session", "memory", "code", "skills"];

const GROUP_OF = {
  help: "session", status: "session", sessions: "session", resume: "session", context: "session",
  mode: "session", clear: "session", exit: "session",
  memory: "memory", sweep: "memory", init: "memory",
  work: "code", gate: "code", orchestra: "code", improve: "code", scan: "code", verify: "code",
  apply: "code", undo: "code", tools: "code",
};

// Most-used first. The list is short on purpose: it encodes what people reach for, not a full ordering.
const RANK = ["work", "gate", "memory", "sweep", "status", "improve", "orchestra", "help"];

/** Rows for the menu: `[{name, short, group}]`, grouped and ordered, WIRED ONLY.
 *
 * `commands` is `{name: shortDescription}`; `dispatchable` is the set of names the REPL will actually run;
 * `skills` is `[{name, short}]` from `GET /skills` (already carrying the server's `menu_line`).
 */
function menuRows(commands, dispatchable, skills) {
  const rows = [];
  for (const [name, short] of Object.entries(commands || {})) {
    if (dispatchable && !dispatchable.has(name)) continue;      // RULE 1 — never offer a dead door
    rows.push({ name, short: String(short || ""), group: GROUP_OF[name] || "code" });
  }
  for (const s of skills || []) {
    if (!s || !s.name) continue;
    rows.push({ name: `skill_${s.name}`, short: String(s.short || ""), group: "skills" });
  }
  const rank = (r) => {
    const i = RANK.indexOf(r.name);
    return i === -1 ? RANK.length : i;
  };
  return rows.sort((a, b) =>
    GROUPS.indexOf(a.group) - GROUPS.indexOf(b.group) ||
    rank(a) - rank(b) ||
    a.name.localeCompare(b.name));
}

/** The rows matching what has been typed after the `/`. Prefix matches lead, then substring.
 *
 * Case-insensitive, and a name match always outranks a description match — someone typing `ga` means
 * `/gate`, not the one skill whose sentence happens to contain "ga". Stable within each tier, so the
 * grouping from `menuRows` survives filtering. */
function filterRows(rows, typed) {
  const q = String(typed || "").trim().toLowerCase();
  if (!q) return rows.slice();
  const tier = (r) => {
    const name = r.name.toLowerCase();
    if (name.startsWith(q)) return 0;
    if (name.includes(q)) return 1;
    if (r.short.toLowerCase().includes(q)) return 2;
    return 3;
  };
  return rows.map((r, i) => ({ r, i, t: tier(r) }))
    .filter((x) => x.t < 3)
    .sort((a, b) => a.t - b.t || a.i - b.i)
    .map((x) => x.r);
}

/** One rendered row, padded so the descriptions line up. Never emoji, never the long summary. */
function renderRow(row, width, selected) {
  const name = `/${row.name}`.padEnd(Math.max(width + 1, 2));
  const marker = selected ? ">" : " ";
  return `${marker} ${name}  ${row.short}`;
}

/** The whole menu as lines, capped so it never takes the screen. Returns `[]` when nothing matches —
 * the caller shows a "no match" line rather than an empty box that looks like a hang. */
function renderMenu(rows, selectedIndex, max) {
  const cap = Math.max(1, max || 8);
  const shown = rows.slice(0, cap);
  const width = shown.reduce((w, r) => Math.max(w, r.name.length), 0);
  const lines = shown.map((r, i) => renderRow(r, width, i === selectedIndex));
  if (rows.length > cap) lines.push(`  … ${rows.length - cap} more — keep typing to narrow`);
  return lines;
}

/** Attach the live menu to a readline session. Returns the unbind; a NO-OP on a non-TTY.
 *
 * The same shape as `mode-ui.js`'s `keyBinder`, and for the same reason: this file owns the keyboard, the
 * caller owns the session. NON-TTY IS THE IMPORTANT CASE — piped stdin, CI and every scripted test deliver
 * no keypress events, and drawing a menu there would corrupt the output stream, so we never attach.
 *
 * `rowsFor()` is called on OPEN, not on every keystroke: the skill list comes from the server and a menu
 * that blocked a keypress on a network call would be worse than no menu at all. Cache it at session start.
 *
 * REDRAWN IN PLACE, never appended — the same bug `mode-ui.js` had, where each shift+tab stacked another
 * footer up the screen. We erase exactly the lines we drew last time and no more.
 */
function attachMenu(stdin, deps) {
  const d = deps || {};
  return function bind(rowsFor) {
    if (!stdin || !stdin.isTTY) return () => {};
    const rl = d.rl;
    const write = d.write || ((s) => process.stdout.write(s));
    const readline = d.readline || require("readline");
    readline.emitKeypressEvents(stdin);
    let drawn = 0;             // how many lines we last painted, so we erase exactly those
    let open = false;
    let selected = 0;
    let rows = [];

    const erase = () => {
      if (!drawn) return;
      write("\r\x1b[2K");
      for (let i = 0; i < drawn; i += 1) write("\x1b[1A\r\x1b[2K");
      drawn = 0;
    };
    const close = () => { erase(); open = false; selected = 0; };
    const typed = () => {
      const line = (rl && rl.line) || "";
      return line.startsWith("/") ? line.slice(1) : null;
    };
    const paint = () => {
      const q = typed();
      if (q === null) return close();
      const hits = filterRows(rows, q);
      selected = Math.max(0, Math.min(selected, hits.length - 1));
      const lines = hits.length ? renderMenu(hits, selected, d.max || 8)
                                : ["  no command matches — esc to dismiss"];
      erase();
      write("\n" + lines.join("\n"));
      drawn = lines.length;
      if (rl) rl.prompt(true);
      return undefined;
    };

    const onKey = (_ch, key) => {
      const k = key || {};
      const line = (rl && rl.line) || "";
      if (!open) {
        // opens on the `/` that starts a line — never mid-sentence, where a path like src/x is just text
        if (k.name === "escape" || !line.startsWith("/")) return;
        rows = rowsFor() || [];
        open = true;
        selected = 0;
        setImmediate(paint);
        return;
      }
      if (k.name === "escape") return close();
      if (k.name === "up") { selected = Math.max(0, selected - 1); return setImmediate(paint); }
      if (k.name === "down") { selected += 1; return setImmediate(paint); }
      if (k.name === "return" || (k.name === "tab" && !k.shift)) {
        const hits = filterRows(rows, typed() || "");
        const pick = hits[selected];
        close();
        if (pick && rl && k.name === "tab") {
          // tab COMPLETES and leaves the line to be submitted; enter lets readline submit what is there
          rl.line = `/${pick.name} `;
          rl.cursor = rl.line.length;
          rl.prompt(true);
        }
        return undefined;
      }
      return setImmediate(paint);          // any other key re-filters
    };
    stdin.on("keypress", onKey);
    return () => { close(); stdin.removeListener("keypress", onKey); };
  };
}

module.exports = { GROUPS, GROUP_OF, RANK, menuRows, filterRows, renderRow, renderMenu, attachMenu };
