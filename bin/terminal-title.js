"use strict";
// The terminal tab says what is running in it.
//
// ADOPTED FROM CODEX: the interaction and one lesson, not their code (Rust, Apache-2.0 — idea-level only,
// no vendoring). `codex-rs/tui/src/chatwidget.rs:724` keeps "the last terminal title emitted, to avoid
// writing duplicate OSC updates" — without that cache a redraw writes an escape sequence every frame,
// which is invisible until something is logging the raw stream and then is very visible.
//
// Kimi renames the tab to "Python" by accident of its runtime. Ours says Estelle on purpose.
//
// OSC 0 sets both the icon name and the window title: ESC ] 0 ; <text> BEL. Restoring on exit matters more
// than setting it — a CLI that leaves the customer's tab renamed after it quits has taken something that
// was not its to keep. There is no portable "read the old title" (the DECSLPP query needs a response the
// terminal may never send), so restore means "hand the tab back to the shell", which is an EMPTY title:
// every shell that sets a title rewrites it on the next prompt.

const DEFAULT_TITLE = "Estelle";

let last = null;                 // module-scope cache — Codex's lesson, and the reason to have a module

/** Whether this stream is a terminal that can be given a title. A pipe or a file must never see an OSC. */
function canSetTitle(stream) {
  return Boolean(stream && stream.isTTY && typeof stream.write === "function" && !process.env.NO_COLOR_TITLE);
}

/**
 * Set the terminal title. A no-op on a non-TTY, and a no-op when the title is ALREADY what we want —
 * that second half is the whole point of the cache.
 *
 * Returns true when an escape sequence was actually written, so a caller (and a test) can tell "set" from
 * "already set" without reading the raw stream.
 */
function setTitle(title, stream) {
  const out = stream || process.stdout;
  const text = String(title == null ? DEFAULT_TITLE : title);
  if (!canSetTitle(out)) return false;
  if (last === text) return false;                   // do not re-emit — see the Codex note above
  out.write(`\x1b]0;${text}\x07`);
  last = text;
  return true;
}

/** Hand the tab back to the shell. Idempotent, and safe to call from an exit handler. */
function clearTitle(stream) {
  const out = stream || process.stdout;
  if (!canSetTitle(out)) return false;
  if (last === "") return false;
  out.write("\x1b]0;\x07");
  last = "";
  return true;
}

/**
 * Claim the tab for this session and register the restore. Returns the restore function so a caller can
 * run it directly; it is also wired to `exit` so an ordinary quit, a thrown error and Ctrl-C all put the
 * title back. `exit` is the one event that fires for all three.
 */
function claimTitle(title, opts) {
  const { stream = process.stdout, proc = process } = opts || {};
  setTitle(title == null ? DEFAULT_TITLE : title, stream);
  const restore = () => clearTitle(stream);
  proc.once("exit", restore);
  return () => { proc.removeListener("exit", restore); restore(); };
}

/** Test seam only: forget the cached title. Never called by the CLI. */
function _resetTitleCache() { last = null; }

module.exports = { DEFAULT_TITLE, canSetTitle, setTitle, clearTitle, claimTitle, _resetTitleCache };
