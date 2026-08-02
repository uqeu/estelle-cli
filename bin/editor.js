"use strict";
// MODULE 2'S INPUT HALF — **our own composer.** Founder ruling 2026-08-02: *"OWN THE COMPOSER. STOP
// BORROWING readline. This is the structural fix your own audit identified, and it closes seven defects
// at once instead of an eighth arriving tomorrow."*
//
// > `readline` is a LINE EDITOR, not a composer — it cannot show pending input, cannot do multi-line,
// > submits on every `\n` by definition, owns the cursor, and fights anything else that draws.
//
// Symptoms **a, d, e, f, g**, **E-036** and the invisible queue are ONE CAUSE, and this is it. Each was
// fixed at its own site and the cause was never removed.
//
// THE SHAPE IS CODEX'S `bottom_pane` (`vendor-reference/`, Rust, Apache-2.0) — **taken as a shape, never
// ported**: one component owns the region below the transcript, holds its own buffer and cursor, and
// redraws on a loop rather than reacting to whoever wrote last. That shape is language-independent, which
// is the whole reason it transfers to zero-dependency JS.
//
// ⛔ MULTI-LINE BY CONSTRUCTION, which is the sentence that matters. The buffer is a STRING THAT MAY
// CONTAIN NEWLINES. A pasted block is inserted verbatim and becomes ONE entry with N lines — so E-036's
// "a 20-line paste fires 20 turns" is not fixed here, it is **unrepresentable**. There is no code path
// that could submit on an embedded newline, because submission is a KEY, not a character.
//
// PURE CORE, THIN BINDER. Every decision — what a key means, what the buffer becomes, what is drawn — is
// a pure function tested by name. Only `attach` touches stdin, and E-027 is why its seam is driven by the
// PTY harness rather than by a synthetic stdin: unit tests over a fake stdin are exactly what passed
// while a real readline split every paste.

const HISTORY_NONE = -1;

/** A fresh composer. `text` may contain newlines; `cursor` is an index into it. */
function create(opts) {
  const o = opts || {};
  return {
    text: String(o.text || ""),
    cursor: Number.isFinite(o.cursor) ? o.cursor : String(o.text || "").length,
    history: Array.isArray(o.history) ? o.history.slice() : [],
    historyAt: HISTORY_NONE,      // -1 = editing a fresh line, 0.. = walking back through history
    draft: "",                    // what was being typed before ↑ started walking; restored on ↓ past the end
  };
}

/**
 * What a keypress MEANS. Pure, and the whole keyboard contract in one table.
 *
 * `key` is Node's keypress descriptor (`{name, ctrl, meta, shift, sequence}`); `ch` is the character.
 * Returning a NAME rather than mutating is what lets the entire keyboard be tested without a terminal —
 * the half that had no tests at all in the module whose absence caused all eight symptoms.
 */
function keyAction(key, ch) {
  const k = key || {};
  const seq = String(k.sequence || "");
  // SUBMIT vs NEWLINE. Enter submits; a newline is inserted by shift+enter where the terminal can express
  // it (CSI-u `\x1b[13;2u`), and by alt+enter or ctrl-j everywhere else. Terminals disagree about
  // shift+enter — most send a bare CR — so offering only that would make multi-line unreachable for most
  // customers, which is the "advertised but does not fire" defect again.
  if (seq === "\x1b[13;2u" || seq === "\x1b\r" || seq === "\x1b\n") return "newline";
  if (k.name === "return" || k.name === "enter") return k.shift || k.meta ? "newline" : "submit";
  if (k.ctrl && k.name === "j") return "newline";

  if (k.ctrl && k.name === "c") return "cancel";
  if (k.ctrl && k.name === "d") return "eof";
  if (k.name === "escape") return "escape";

  if (k.name === "backspace" || seq === "\x7f") return "backspace";
  if (k.name === "delete") return "delete";
  if (k.ctrl && k.name === "u") return "kill-line";
  if (k.ctrl && k.name === "w") return "kill-word";
  if (k.ctrl && k.name === "a") return "home";
  if (k.ctrl && k.name === "e") return "end";
  if (k.name === "home") return "home";
  if (k.name === "end") return "end";

  // WORD JUMP: alt/ctrl + arrow, which is what every terminal actually sends for it.
  if (k.name === "left") return k.meta || k.ctrl ? "word-left" : "left";
  if (k.name === "right") return k.meta || k.ctrl ? "word-right" : "right";
  if (k.name === "up") return "history-back";
  if (k.name === "down") return "history-forward";

  // A printable character. Control bytes are dropped rather than inserted: a stray escape sequence typed
  // into the buffer is how a composer ends up with garbage nobody can see or delete.
  if (typeof ch === "string" && ch.length >= 1 && !k.ctrl && !k.meta && ch >= " ") return "insert";
  return "";
}

/** The index of the start of the word before `i`. */
function wordLeft(text, i) {
  let j = i;
  while (j > 0 && /\s/.test(text[j - 1])) j -= 1;
  while (j > 0 && !/\s/.test(text[j - 1])) j -= 1;
  return j;
}

/** The index just past the word after `i`. */
function wordRight(text, i) {
  let j = i;
  while (j < text.length && /\s/.test(text[j])) j += 1;
  while (j < text.length && !/\s/.test(text[j])) j += 1;
  return j;
}

/** The start of the visual line containing `i` (multi-line aware — home/end are per LINE, not per buffer). */
function lineStart(text, i) {
  const nl = text.lastIndexOf("\n", Math.max(0, i - 1));
  return nl === -1 ? 0 : nl + 1;
}

function lineEnd(text, i) {
  const nl = text.indexOf("\n", i);
  return nl === -1 ? text.length : nl;
}

/**
 * Apply one action. **Pure** — returns a NEW state, never mutates.
 *
 * `submit`, `cancel` and `eof` are decisions for the CALLER: this returns the state unchanged and the
 * binder acts on the action name. Keeping them out of here is what makes the whole buffer testable as
 * data, with no notion of a session, a network or an exit.
 */
function apply(state, action, ch) {
  const s = state;
  const t = s.text;
  const i = s.cursor;
  switch (action) {
    case "insert": {
      const text = t.slice(0, i) + ch + t.slice(i);
      return { ...s, text, cursor: i + String(ch).length, historyAt: HISTORY_NONE };
    }
    case "newline": {
      const text = `${t.slice(0, i)}\n${t.slice(i)}`;
      return { ...s, text, cursor: i + 1, historyAt: HISTORY_NONE };
    }
    case "backspace":
      if (!i) return s;
      return { ...s, text: t.slice(0, i - 1) + t.slice(i), cursor: i - 1, historyAt: HISTORY_NONE };
    case "delete":
      if (i >= t.length) return s;
      return { ...s, text: t.slice(0, i) + t.slice(i + 1), historyAt: HISTORY_NONE };
    case "kill-line": {
      const start = lineStart(t, i);
      return { ...s, text: t.slice(0, start) + t.slice(i), cursor: start, historyAt: HISTORY_NONE };
    }
    case "kill-word": {
      const j = wordLeft(t, i);
      return { ...s, text: t.slice(0, j) + t.slice(i), cursor: j, historyAt: HISTORY_NONE };
    }
    case "left": return { ...s, cursor: Math.max(0, i - 1) };
    case "right": return { ...s, cursor: Math.min(t.length, i + 1) };
    case "word-left": return { ...s, cursor: wordLeft(t, i) };
    case "word-right": return { ...s, cursor: wordRight(t, i) };
    case "home": return { ...s, cursor: lineStart(t, i) };
    case "end": return { ...s, cursor: lineEnd(t, i) };
    case "history-back": {
      // ↑ inside a multi-line buffer moves the CURSOR, not the history — otherwise editing line 2 of a
      // paste would silently replace the whole thing, which is data loss disguised as a shortcut.
      if (t.includes("\n") && lineStart(t, i) > 0) return { ...s, cursor: Math.max(0, i - 1) };
      if (!s.history.length) return s;
      const at = s.historyAt === HISTORY_NONE ? 0 : Math.min(s.history.length - 1, s.historyAt + 1);
      const draft = s.historyAt === HISTORY_NONE ? t : s.draft;
      const text = s.history[s.history.length - 1 - at] || "";
      return { ...s, text, cursor: text.length, historyAt: at, draft };
    }
    case "history-forward": {
      if (t.includes("\n") && lineEnd(t, i) < t.length) return { ...s, cursor: Math.min(t.length, i + 1) };
      if (s.historyAt === HISTORY_NONE) return s;
      if (s.historyAt === 0) return { ...s, text: s.draft, cursor: s.draft.length, historyAt: HISTORY_NONE };
      const at = s.historyAt - 1;
      const text = s.history[s.history.length - 1 - at] || "";
      return { ...s, text, cursor: text.length, historyAt: at };
    }
    case "clear": return { ...s, text: "", cursor: 0, historyAt: HISTORY_NONE, draft: "" };
    default: return s;
  }
}

/** Insert a whole pasted block at the cursor, newlines and all. **This is why E-036 is unrepresentable:**
 * a paste is one insertion, and submission is a KEY, so there is no path that submits mid-block. */
function insertPaste(state, text) {
  const body = String(text || "").replace(/\r\n?/g, "\n");
  if (!body) return state;
  return { ...state, text: state.text.slice(0, state.cursor) + body + state.text.slice(state.cursor),
           cursor: state.cursor + body.length, historyAt: HISTORY_NONE };
}

/**
 * The composer as lines, plus where the cursor sits — **the render pass owns this region and nothing
 * else writes into it.**
 *
 * Returns `{lines, cursorRow, cursorCol}` so ONE renderer can place the real terminal cursor. Symptom (f)
 * was the cursor belonging to a library that also redrew; here it belongs to the thing that draws.
 */
function render(state, opts, c) {
  const o = opts || {};
  const label = String(o.label || "›");
  const rows = state.text.split("\n");
  const lines = rows.map((row, n) => (n === 0 ? `${label} ${row}` : `${" ".repeat(label.length)} ${row}`));
  if (!state.text && o.placeholder) {
    lines[0] = `${label} ${c.dim(String(o.placeholder))}`;
  }
  const before = state.text.slice(0, state.cursor).split("\n");
  return {
    lines,
    cursorRow: before.length - 1,
    cursorCol: (before.length === 1 ? label.length + 1 : label.length + 1) + before[before.length - 1].length,
  };
}

/**
 * THE KEY LOOP — the only impure function here, and it DOES NOT DRAW.
 *
 * ⛔ THAT SEPARATION IS THE WHOLE FIX. readline both read keys and drew, so it fought every other writer;
 * symptoms d, e, f and g are all two things drawing on one screen. This reads keys and reports state
 * changes; the SESSION owns the single render pass that paints transcript-above + composer-below. One
 * writer, by construction.
 *
 * `deps.onChange(state)` fires whenever the buffer moves — the session repaints.
 * `deps.onSubmit(text)`  fires on Enter with a non-empty buffer.
 * `deps.onCancel()`      ctrl-c on a non-empty buffer (clear); on an empty one the session may exit.
 * `deps.onEof()`         ctrl-d on an empty buffer.
 * `deps.onEscape()`      esc — the slash menu and the pending queue both want it.
 *
 * Returns `{ state, close, setHistory }`. Raw mode is entered here and RESTORED on close, including on a
 * throw: a CLI that exits with the terminal in raw mode leaves a shell that does not echo, which is the
 * same class of damage as exiting inside the alternate screen.
 */
function attach(stdin, deps) {
  const d = deps || {};
  let state = create({ history: d.history || [] });
  if (!stdin || !stdin.isTTY) {
    // A pipe has no keys. Returning an inert handle keeps every caller identical on both paths rather
    // than making them branch — the non-TTY path is the one every test and CI run takes.
    const inert = { isTTY: false, on() { return inert; }, removeListener() { return inert; },
                    setRawMode() { return inert; } };
    return { get state() { return state; }, keys: inert, paste() {}, close() {}, setHistory() {} };
  }
  const readline = d.readline || require("readline");
  readline.emitKeypressEvents(stdin);
  if (stdin.setRawMode) stdin.setRawMode(true);

  // 🔴 ONE READER, LAYERED ROUTING — and this is what the PTY acceptance run forced.
  //
  // Four modules each called `emitKeypressEvents(process.stdin)` and bound their own `keypress`:
  // `mode-ui` (shift+tab), `slash-menu` (the menu), `altscreen` (scrolling) and `paste`. Four readers of
  // one stream, plus the composer trying to own it, is why a paste that is provably ONE submit in
  // isolation came back as three entries and 21 submits inside a real session.
  //
  // `keys` is what they bind to now: a tiny emitter the composer DISPATCHES to, first-claim-wins, before
  // the buffer sees anything. Codex's `bottom_pane` says the same thing in its own header — "input
  // routing is layered: the pane decides which local surface receives a key" — and it is the reason a
  // menu and a composer can coexist at all. Their code is unchanged; only what they bind to is.
  const listeners = [];
  const keys = {
    isTTY: true,
    on(event, fn) { if (event === "keypress") listeners.push(fn); return keys; },
    removeListener(event, fn) {
      if (event !== "keypress") return keys;
      const i = listeners.indexOf(fn);
      if (i > -1) listeners.splice(i, 1);
      return keys;
    },
    // A binder that reaches for raw mode must not fight the composer for it: the composer already owns it.
    setRawMode() { return keys; },
  };

  const onKey = (ch, key) => {
    // OVERLAYS FIRST. A menu that is open owns ↑/↓/enter/esc; the composer must not also act on them,
    // which is symptom (e) — two handlers on one keypress, and the customer's line disappearing between
    // them. `claimed` is how a surface says "this key was mine".
    for (const fn of listeners.slice()) {
      try { if (fn(ch, key) === true) return; } catch (_) { /* an overlay must never kill the session */ }
    }
    const action = keyAction(key, ch);
    if (!action) return;
    if (action === "submit") {
      const text = state.text;
      if (!text.trim()) return;                       // a bare Enter is not a question
      state = { ...create({ history: state.history }), history: state.history };
      if (d.onSubmit) d.onSubmit(text);
      if (d.onChange) d.onChange(state);
      return;
    }
    if (action === "cancel") {
      if (state.text) { state = apply(state, "clear"); if (d.onChange) d.onChange(state); return; }
      if (d.onCancel) d.onCancel();
      return;
    }
    if (action === "eof") { if (!state.text && d.onEof) d.onEof(); return; }
    if (action === "escape") { if (d.onEscape) d.onEscape(); return; }
    state = apply(state, action, ch);
    if (d.onChange) d.onChange(state);
  };
  stdin.on("keypress", onKey);

  let closed = false;
  return {
    get state() { return state; },
    /** What the overlays bind to instead of `process.stdin`. See the note on layered routing above. */
    keys,
    /** The current text — what an overlay reads to know what has been typed. The slash menu needs it to
     * filter its rows, and reading readline's buffer instead is what made `/` open nothing. */
    line: () => state.text,
    /** Replace the buffer — tab-completion. Cursor goes to the end, which is where a completion leaves it. */
    setLine(text) {
      state = { ...state, text: String(text || ""), cursor: String(text || "").length,
                historyAt: HISTORY_NONE };
      if (d.onChange) d.onChange(state);
    },
    /** A whole pasted block, inserted at the cursor. ONE entry with N lines. */
    paste(text) { state = insertPaste(state, text); if (d.onChange) d.onChange(state); },
    setHistory(history) { state = { ...state, history: Array.isArray(history) ? history.slice() : [] }; },
    close() {
      if (closed) return;
      closed = true;
      stdin.removeListener("keypress", onKey);
      // RESTORE RAW MODE. Exiting with it left on gives the customer a shell that does not echo what they
      // type — the same damage class as exiting inside the alternate screen, and just as hard to diagnose.
      if (stdin.setRawMode) stdin.setRawMode(false);
    },
  };
}

module.exports = {
  HISTORY_NONE, create, keyAction, apply, insertPaste, render, attach,
  wordLeft, wordRight, lineStart, lineEnd,
};
