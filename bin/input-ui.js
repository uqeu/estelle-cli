"use strict";
// INPUT POLISH — the interaction details that separate a REPL you tolerate from one you live in.
//
// Split out of repl.js, which had grown past the point where it read as one thing. Everything here is a PURE
// function over text and numbers (the terminal I/O around them is thin), so the whole file is testable with
// no terminal, and repl.js goes back to being a router. Re-exported from repl.js so nothing that already
// imports these has to change.

// ── input polish, learned from the mature agent CLIs ────────────────────────────
// These are the interaction details that separate a REPL you tolerate from one you live in. Each is a
// pure function so it can be tested without a terminal.

const PASTE_MIN_LINES = 3, PASTE_MIN_CHARS = 150, HISTORY_MAX = 50;

/**
 * Collapse a big paste to a token the user can see past. A stack trace pasted into a prompt otherwise
 * buries the question you were asking; `[Pasted ~47 lines]` keeps the line readable while the real text
 * still reaches Estelle on submit. Returns the visible text plus the side table that restores it.
 */
function collapsePaste(text, store) {
  const body = String(text || "");
  const lines = body.split("\n").length;
  if (lines < PASTE_MIN_LINES && body.length < PASTE_MIN_CHARS) return { visible: body, marks: store || [] };
  const marks = (store || []).slice();
  const token = `[Pasted ~${lines} line${lines === 1 ? "" : "s"} #${marks.length + 1}]`;
  marks.push({ token, text: body });
  return { visible: token, marks };
}

/** Put every collapsed paste back before the text is sent — what Estelle sees is what you pasted. */
function expandPastes(visible, marks) {
  let out = String(visible || "");
  for (const m of marks || []) out = out.split(m.token).join(m.text);
  return out;
}

/**
 * Frecency: recent AND frequent beats merely frequent. `score * (1 + hits / (1 + ageDays))`, so the file
 * you touched twice this morning outranks one you opened thirty times last year.
 */
function frecencyScore(base, entry, now) {
  if (!entry || !entry.hits) return base;
  const ageDays = Math.max(0, ((now || Date.now()) - (entry.at || 0)) / 86400000);
  return base * (1 + entry.hits / (1 + ageDays));
}

/**
 * Prompt history, JSONL, newest last. SELF-HEALING: a corrupt line is dropped rather than throwing, so a
 * half-written file (a kill -9 mid-append) never costs you the whole history. Consecutive duplicates are
 * collapsed — pressing up should walk distinct thoughts, not repeats.
 */
function parseHistory(raw) {
  const out = [];
  for (const line of String(raw || "").split("\n")) {
    if (!line.trim()) continue;
    try {
      const e = JSON.parse(line);
      const text = typeof e === "string" ? e : e && e.text;
      if (typeof text === "string" && text.trim() && out[out.length - 1] !== text) out.push(text);
    } catch { /* a torn line is skipped, never fatal */ }
  }
  return out.slice(-HISTORY_MAX);
}

/** One history line to append, or "" when this entry adds nothing (blank, or the same as last time). */
function historyLine(text, previous) {
  const t = String(text || "").trim();
  if (!t || t === previous) return "";
  return JSON.stringify({ text: t, at: Date.now() }) + "\n";
}

/**
 * Ctrl+C is reflexive — people hit it to abandon a half-typed thought, not to quit. So it CLEARS a
 * non-empty line and only exits on an empty one. Returns what the session should do.
 */
function interruptAction(currentText) {
  return String(currentText || "").trim() ? "clear" : "exit";
}

/**
 * Show a spinner only if the work outlives `delayMs`, and once shown hold it for `minMs`. Without the
 * delay every 200ms call flashes a spinner; without the hold it vanishes before the eye resolves it.
 */
function spinnerPlan(elapsedMs, shownAtMs, delayMs = 500, minMs = 3000) {
  if (shownAtMs == null) return elapsedMs >= delayMs ? "show" : "wait";
  return elapsedMs - shownAtMs >= minMs ? "may-hide" : "hold";
}

const SPINNER_FRAMES = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
// The hold is deliberately SHORT here — long enough that a spinner cannot strobe, far too short to be felt
// as latency. spinnerPlan's 3s default is right for a full-screen composer that has nothing else to draw;
// holding a FINISHED answer back for three seconds to satisfy an animation is the wrong trade in a REPL.
const SPINNER_HOLD_MS = 250;

/**
 * Run `work` behind a thinking indicator. A /work or a Deep Search takes many seconds, and until now the
 * session sat on a dead prompt with no sign it was alive.
 *
 * Inert without a `write` — piping the session (CI, `printf … | estelle`) must never get animation frames
 * spliced into its output, and there is nobody watching one anyway.
 */
async function withSpinner(label, work, deps) {
  const write = deps && deps.write;
  if (!write) return work();
  const clock = (deps && deps.now) || Date.now;
  const started = clock();
  let shownAt = null, frame = 0;
  const draw = () => {
    const elapsed = clock() - started;
    if (spinnerPlan(elapsed, shownAt, 500, SPINNER_HOLD_MS) === "show") shownAt = elapsed;
    if (shownAt == null) return;
    write(`\r\x1b[2K  ${SPINNER_FRAMES[frame++ % SPINNER_FRAMES.length]} ${label}`);
  };
  const timer = setInterval(draw, 90);
  if (timer.unref) timer.unref();                     // never hold the process open on the animation
  try {
    return await work();
  } finally {
    clearInterval(timer);
    if (shownAt != null) {
      const elapsed = clock() - started;
      // Honour the (short) hold, so a call that lands just past the delay does not strobe.
      if (spinnerPlan(elapsed, shownAt, 500, SPINNER_HOLD_MS) === "hold") {
        await new Promise((r) => setTimeout(r, SPINNER_HOLD_MS - (elapsed - shownAt)));
      }
      write("\r\x1b[2K");
    }
  }
}

module.exports = {
  PASTE_MIN_LINES, PASTE_MIN_CHARS, HISTORY_MAX, SPINNER_FRAMES, SPINNER_HOLD_MS,
  collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine, interruptAction,
  spinnerPlan, withSpinner,
};
