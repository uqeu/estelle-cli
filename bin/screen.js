"use strict";
// THE SCROLLBACK MODEL — brief §2.2/§2.3 (the release nicknamed "§9").
//
// WHY THIS FILE EXISTS. §2.2: the screen is borrowed, not owned — Estelle prints on top of the user's
// scrollback, so it reads as command output rather than an application. The fix is the alternate screen
// buffer, and it comes with one consequence that has to be designed for rather than discovered:
//
//   ⛔ IN ALT-SCREEN THERE IS NO TERMINAL SCROLLBACK. THE APPLICATION MUST IMPLEMENT ITS OWN.
//
// That is this file. It is PURE — no terminal, no I/O, no escape codes written anywhere. It answers one
// question as DATA: *given everything printed so far, a viewport height and a scroll position, which rows
// are on screen?* `altscreen.js` owns the boundary; `repl.js` owns the loop; neither owns this.
//
// 🔴 THE TEST DISCIPLINE THIS SPLIT EXISTS TO ENABLE, inherited from palette.js and stated in RESUME-next
// as the one line for whoever takes 0.2.0:
//
//   > ASSERT ON DECLARED CODES AND GLYPHS, NEVER ON RENDERED OUTPUT. Comparing what was WRITTEN proves
//   > nothing about what is ON SCREEN.
//
// A test that diffs a byte stream can pass while the reader sees garbage. A test that reads `visible()`
// is asking the question the human is asking. So the model is the thing under test, and the byte stream is
// a thin function of it.
//
// §2.3 — the observed defect — is a direct consequence of not having this: "scrolling up in the REPL
// corrupts the view", because the footer was redrawn with cursor movement relative to the CURSOR while the
// viewport was scrolled. Once the app owns the viewport, "where is the reader looking" is state we hold
// rather than something we infer from a terminal that will not tell us.

// ── width, which is not string length ───────────────────────────────────────────
// Our own output is full of SGR escapes (palette.js) and customer content can carry anything. Measuring
// with `.length` would wrap a coloured 40-column line at 20 and corrupt every frame it appears in.

// CSI (colour, cursor) and OSC (titles) — matched so they can be measured as zero and never split.
const ANSI_RE = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g;

/** The text with every escape sequence removed — what a human actually sees. */
function stripAnsi(text) {
  return String(text == null ? "" : text).replace(ANSI_RE, "");
}

// Ranges that occupy two terminal columns. Not exhaustive — Unicode's full width table is large and
// changes — but it covers CJK, Hangul, fullwidth forms and emoji, which is where a wrong answer is
// VISIBLE as a broken frame rather than an off-by-one nobody notices.
const WIDE = [
  [0x1100, 0x115f], [0x2e80, 0x303e], [0x3041, 0x33ff], [0x3400, 0x4dbf], [0x4e00, 0x9fff],
  [0xa000, 0xa4cf], [0xac00, 0xd7a3], [0xf900, 0xfaff], [0xfe30, 0xfe6f], [0xff00, 0xff60],
  [0xffe0, 0xffe6], [0x1f300, 0x1f64f], [0x1f900, 0x1f9ff], [0x20000, 0x3fffd],
];

function charWidth(cp) {
  if (cp >= 0x0300 && cp <= 0x036f) return 0;          // combining marks sit on the previous cell
  if (cp === 0x200d || cp === 0xfe0f) return 0;        // ZWJ and the variation selector add no column
  for (const [lo, hi] of WIDE) if (cp >= lo && cp <= hi) return 2;
  return 1;
}

/** How many terminal columns `text` occupies, ignoring escapes. */
function displayWidth(text) {
  let w = 0;
  for (const ch of stripAnsi(text)) w += charWidth(ch.codePointAt(0));
  return w;
}

// ── wrapping ────────────────────────────────────────────────────────────────────

const SGR_RE = /^\x1b\[([0-9;]*)m/;

/**
 * Wrap one logical line to `width` columns.
 *
 * Three properties, each of which is a defect if it does not hold:
 *   * **No character is dropped** — wrapping is a view, never an edit.
 *   * **An escape sequence is never cut in half.** Half an escape is emitted to the terminal as garbage
 *     and can leave every subsequent line mis-coloured.
 *   * **A colour open at the break is REOPENED on the next row.** A run of red text that wraps must not
 *     turn white halfway down, and the row must not leak its colour into whatever the frame draws next.
 *
 * An empty line wraps to ONE empty row, never zero — the REPL prints blank lines deliberately for spacing,
 * and dropping them would silently close the layout up.
 */
function wrap(text, width) {
  const w = Math.max(1, Math.floor(Number(width) || 1));
  const src = String(text == null ? "" : text);
  if (src === "") return [""];

  const rows = [];
  let buf = "", col = 0, active = "";          // `active` = the SGR params currently in force
  let lastBreak = -1, breakCol = 0;            // where a space was seen, for word-preferred wrapping

  const flush = (upto) => {
    let row = buf;
    if (upto != null) row = upto;
    if (active) row += "\x1b[0m";              // close, so a row never leaks colour into the frame
    rows.push(row);
    buf = active ? `\x1b[${active}m` : "";     // and REOPEN on the continuation row
    col = 0; lastBreak = -1; breakCol = 0;
  };

  let i = 0;
  while (i < src.length) {
    const rest = src.slice(i);
    const sgr = SGR_RE.exec(rest);
    if (sgr) {                                  // an escape costs no columns and is copied verbatim
      active = sgr[1] === "" || sgr[1] === "0" ? "" : sgr[1];
      buf += sgr[0];
      i += sgr[0].length;
      continue;
    }
    const other = rest.match(ANSI_RE);
    if (other && rest.startsWith(other[0])) { buf += other[0]; i += other[0].length; continue; }

    const ch = String.fromCodePoint(rest.codePointAt(0));
    const cw = charWidth(ch.codePointAt(0));
    if (col + cw > w) {
      // Prefer the last space, so words survive — but only when it leaves something on the row.
      if (lastBreak >= 0 && breakCol > 0) {
        const head = buf.slice(0, lastBreak);
        const tail = buf.slice(lastBreak).replace(/^ +/, "");
        flush(head);
        buf += tail;
        col = displayWidth(tail);
      } else {
        flush();
      }
    }
    if (ch === " ") { lastBreak = buf.length; breakCol = col; }
    buf += ch;
    col += cw;
    i += ch.length;
  }
  if (buf !== "" || rows.length === 0) rows.push(active ? buf + "\x1b[0m" : buf);
  return rows;
}

// ── the viewport ────────────────────────────────────────────────────────────────
//
// `offset` is THE NUMBER OF ROWS HIDDEN BELOW THE VIEWPORT. Zero means live/pinned to the bottom. It is
// counted from the bottom rather than the top on purpose: new output arrives at the bottom, and an offset
// counted from the top would have to be adjusted on every single append just to stand still.

const DEFAULT_MAX = 5000;

/** A new, empty scrollback. `max` bounds RETAINED ROWS — an unbounded buffer in a long session is a leak. */
function create(opts) {
  const o = opts || {};
  return {
    entries: [],                               // logical lines, kept so a resize can re-wrap (see reflow)
    lines: [],                                 // rendered rows — what `visible` slices
    width: Math.max(1, Math.floor(Number(o.width) || 80)),
    max: Math.max(1, Math.floor(Number(o.max) || DEFAULT_MAX)),
    offset: 0,
    dropped: 0,                                // rows evicted, COUNTED — a silent drop is worse than none
  };
}

/** Re-render `entries` to rows and re-apply the cap. Returns {lines, entries, dropped}. */
function _render(entries, width, max) {
  let lines = [];
  const kept = entries.slice();
  for (const e of kept) lines = lines.concat(wrap(e, width));
  let dropped = 0;
  // Evict whole entries from the front until the rendered rows fit. Whole entries, so `reflow` after an
  // eviction cannot resurrect half of one.
  while (lines.length > max && kept.length > 1) {
    const first = wrap(kept.shift(), width);
    dropped += first.length;
    lines = lines.slice(first.length);
  }
  return { lines, entries: kept, dropped };
}

/**
 * Append text (which may contain newlines) and return a NEW state.
 *
 * 🔴 THE ONE BEHAVIOUR THIS WHOLE FILE IS FOR: **if the reader has scrolled up, the visible window must
 * not move.** Output arriving while someone is reading history is the single most infuriating thing a
 * terminal app does, and it is what §2.3 observed as "scrolling corrupts the view". So the offset grows by
 * exactly the number of rows added — the reader stays on the same text — and `visible()` reports how much
 * is waiting below so the frame can say so instead of hiding it.
 */
function append(state, text, width) {
  const w = Math.max(1, Math.floor(Number(width) || state.width || 80));
  const added = String(text == null ? "" : text).split("\n");
  const entries = state.entries.concat(added);
  const addedRows = added.reduce((n, e) => n + wrap(e, w).length, 0);
  const r = _render(entries, w, state.max);
  // Anchored while scrolled; still pinned to the bottom when live. Then the eviction is subtracted,
  // because rows dropped off the TOP move the reader's text closer to the bottom by exactly that much —
  // without this the window slides and the reader loses their place at the moment the buffer fills.
  const offset = state.offset > 0
    ? Math.max(0, Math.min(state.offset + addedRows - r.dropped, Math.max(0, r.lines.length - 1)))
    : 0;
  return { ...state, width: w, entries: r.entries, lines: r.lines, offset,
           dropped: state.dropped + r.dropped };
}

/** Re-wrap everything for a new terminal width. A resize must re-flow, never leave rows clipped. */
function reflow(state, width) {
  const w = Math.max(1, Math.floor(Number(width) || state.width || 80));
  if (w === state.width) return state;
  const r = _render(state.entries, w, state.max);
  // The offset is in ROWS, and the row count just changed — so scale it rather than keeping a number that
  // now means something else. Clamped by `visible`/`scroll` regardless.
  const scale = state.lines.length > 0 ? r.lines.length / state.lines.length : 1;
  const offset = state.offset > 0 ? Math.round(state.offset * scale) : 0;
  return { ...state, width: w, entries: r.entries, lines: r.lines, offset,
           dropped: state.dropped + r.dropped };
}

/** Move the viewport. Negative `delta` scrolls UP (toward older output). Clamped at both ends. */
function scroll(state, delta, height) {
  const h = Math.max(1, Math.floor(Number(height) || 1));
  const cap = Math.max(0, state.lines.length - h);
  const next = Math.max(0, Math.min(cap, state.offset - Math.floor(Number(delta) || 0)));
  return { ...state, offset: next };
}

/** Snap back to live. */
function toBottom(state) {
  return state.offset === 0 ? state : { ...state, offset: 0 };
}

/**
 * What is on screen, as data.
 *
 * `hiddenAbove` / `hiddenBelow` exist so the frame can be HONEST about what it is not showing. A scroll
 * indicator that cannot be computed is a scroll indicator that gets faked.
 */
function visible(state, height) {
  const h = Math.max(1, Math.floor(Number(height) || 1));
  const len = state.lines.length;
  const offset = Math.max(0, Math.min(state.offset, Math.max(0, len - h)));
  const end = len - offset;
  const start = Math.max(0, end - h);
  return {
    lines: state.lines.slice(start, end),
    atBottom: offset === 0,
    hiddenAbove: start,
    hiddenBelow: len - end,
  };
}

module.exports = {
  ANSI_RE, DEFAULT_MAX,
  stripAnsi, charWidth, displayWidth, wrap,
  create, append, reflow, scroll, toBottom, visible,
};
