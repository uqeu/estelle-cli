"use strict";
// BRACKETED PASTE — a Module 2 concern, and one of the two surfaces nobody had ever checked.
//
// 🔴 MEASURED, NOT ASSUMED, 2026-08-02: `grep -rn 2004 cli/bin/` returned **nothing**. Bracketed paste
// mode has never been enabled. Without it a pasted block arrives as individual keystrokes, **every
// newline submits, and pasting a 20-line snippet fires 20 turns** — twenty model calls, twenty answers to
// fragments, and a session the customer cannot recover. Every reference CLI handles this. We had not
// tested it because it is a TERMINAL-LEVEL behaviour and no test we own can see one.
//
// ⛔ AND THE FINDING UNDER THE FINDING, which is worse and was invisible until this was measured:
// `input-ui.js` `collapsePaste` — the feature that turns a big paste into `[Pasted ~20 lines #1]` — is
// **unreachable from a real paste.** It takes text that has ALREADY been assembled into one string, and
// without bracketed paste readline submits at the first `\n`, so it never sees more than one line. A
// tested, documented, exercised function that the customer's actual keystrokes can never reach. That is
// the 165-unwired defect class, inside our own input layer.
//
// 🔴 THE OFF SWITCH IS NOT OPTIONAL. `\x1b[?2004h` changes the customer's TERMINAL, not our process. If
// we exit without `\x1b[?2004l` their shell keeps wrapping every paste in `200~`/`201~` markers that
// nothing strips — **we would leave their terminal broken after quitting**, which is the same class as
// 0.1.3 writing to a settings file it did not fully own. It is disabled on every exit path alt-screen
// already covers: clean leave, crash, SIGINT, SIGTERM.
//
// PURE PARSING, THIN BINDER — the split every module here uses, and E-027's warning about the seam is why
// `attach` is exercised by name rather than only its halves.

//: Declared, never retyped at a call site (the `palette.js` / `altscreen.js` rule): a test asserting the
//: CONTRACT must compare against the name, or it is asserting a string it copied from the code it checks.
const CODES = {
  on: "\x1b[?2004h",
  off: "\x1b[?2004l",
  start: "\x1b[200~",
  end: "\x1b[201~",
};

/**
 * Split a raw stdin chunk into ordinary keystrokes and completed pastes.
 *
 * Returns `{events, rest}`. `events` is `[{kind: "keys"|"paste", text}]` in arrival order; `rest` is the
 * unconsumed tail — **an unterminated paste stays in `rest`**, because a paste arrives across several
 * reads and treating a partial one as complete would submit half a snippet. Callers accumulate `rest` and
 * feed it back in, which is why this is a pure function over a buffer rather than a stream handler.
 */
/** How many trailing characters of `buf` are a proper prefix of the start marker — i.e. how much might
 * still turn into `\x1b[200~` once the next read arrives. 0 for ordinary text, which is the common case
 * and the one that must not be delayed. */
function prefixLen(buf) {
  const max = Math.min(buf.length, CODES.start.length - 1);
  for (let n = max; n > 0; n -= 1) {
    if (CODES.start.startsWith(buf.slice(buf.length - n))) return n;
  }
  return 0;
}

function parse(buffer) {
  let buf = String(buffer || "");
  const events = [];
  for (;;) {
    const start = buf.indexOf(CODES.start);
    if (start === -1) {
      // No paste beginning in what we hold. Hold back ONLY a tail that is a genuine PREFIX of the start
      // marker, so a marker split across two reads is not typed into the composer as garbage.
      //
      // Holding back a fixed-length tail instead would be correct and FEEL BROKEN: every ordinary
      // keystroke shorter than the marker would sit in the buffer until the next one arrived, so the
      // last character you typed would lag one keypress behind your fingers. A parser that is right and
      // laggy is still a defect on a surface whose entire job is typing.
      const keep = prefixLen(buf);
      const head = buf.slice(0, buf.length - keep);
      if (head) events.push({ kind: "keys", text: head });
      return { events, rest: buf.slice(buf.length - keep) };
    }
    if (start > 0) events.push({ kind: "keys", text: buf.slice(0, start) });
    const after = start + CODES.start.length;
    const end = buf.indexOf(CODES.end, after);
    if (end === -1) return { events, rest: buf.slice(start) };   // incomplete — hold the WHOLE paste
    events.push({ kind: "paste", text: buf.slice(after, end) });
    buf = buf.slice(end + CODES.end.length);
  }
}

/**
 * The text a pasted block should place in the composer.
 *
 * Newlines become spaces rather than submitting, and runs collapse. That is a real decision and it is the
 * conservative one: a terminal composer is a single line, and the alternative — a multi-line editor —
 * is a bigger change than this defect warrants. The FULL text still reaches Estelle because
 * `input-ui.collapsePaste` stores the original against its `[Pasted …]` token and `expandPastes` puts it
 * back before sending, which is exactly the mechanism that had no way to fire until now.
 *
 * `\r` is dropped outright: a CRLF paste would otherwise leave a stray carriage return that redraws the
 * line over itself, which looks like the double-print defect and is not one.
 */
function composerText(pasted) {
  return String(pasted || "").replace(/\r/g, "").replace(/\n+/g, " ").replace(/\s{2,}/g, " ").trim();
}

/** True when a pasted block is big enough that showing it raw would bury the prompt. Mirrors
 * `input-ui.js`'s own thresholds by asking IT, so the two cannot drift into different ideas of "big". */
function isLarge(pasted, collapse) {
  const { visible } = collapse(String(pasted || ""), []);
  return visible !== String(pasted || "");
}

/**
 * Turn bracketed paste on, feed completed pastes into the line editor, and turn it OFF on release.
 *
 * `deps.write` emits to the terminal; `deps.insert(text)` puts text in the composer WITHOUT submitting
 * (readline's `rl.write` does exactly that); `deps.onPaste` is told the original text so the caller can
 * collapse and store it. Returns the release function — and calling it MUST be the only way the mode is
 * left on, so it is registered with the same teardown alt-screen already owns.
 */
function attach(stdin, deps) {
  const d = deps || {};
  const write = d.write || ((s) => process.stdout.write(s));
  if (!stdin || !stdin.isTTY) return () => {};    // a pipe cannot paste; enabling the mode would be noise
  write(CODES.on);
  let held = "";
  const onData = (chunk) => {
    const { events, rest } = parse(held + String(chunk));
    held = rest;
    for (const e of events) {
      if (e.kind !== "paste") continue;
      const text = composerText(e.text);
      if (d.onPaste) d.onPaste(e.text, text);
      if (d.insert && text) d.insert(text);
    }
  };
  stdin.on("data", onData);
  let released = false;
  return () => {
    if (released) return;
    released = true;
    stdin.removeListener("data", onData);
    write(CODES.off);                             // NEVER leave the customer's terminal in this mode
  };
}

module.exports = { CODES, parse, composerText, isLarge, attach };
