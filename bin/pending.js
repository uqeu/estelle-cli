"use strict";
// THE QUEUE — Codex's `bottom_pane/pending_input_preview`, which we did not have at all.
//
// 🔴 THE DEFECT, founder 2026-08-02: *"it is loading all of the things I said, like it is in a backlog —
// we have made Estelle go into a backlog on our CLI."* And then the correction that names it exactly:
// *"in Claude and Codex and Kimi it just puts it in a queue. Estelle doesn't have that queue. It needs to
// actually have a queue."*
//
// **We did not have a queue. We had a blind FIFO drain.** `estelle.js` pushes every line readline emits
// into an array and the loop shifts one whenever it is free — so lines typed while a turn is running are
// invisible, uncancellable, and each one fires a full turn. #101 measured a turn at **9–20 seconds**, so
// three impatient keystrokes is a minute of answers to things the customer stopped caring about after the
// first. The mechanism is correct and the ABSENCE OF A VIEW is the defect.
//
// A queue is three things, and we had only the first:
//   1. it HOLDS what you typed while busy                    ← we had this
//   2. it SHOWS you what is waiting                          ← missing
//   3. it lets you CHANGE YOUR MIND                          ← missing
//
// ⛔ AND IT MUST NEVER DROP SILENTLY. Discarding queued input to "fix" the backlog would be worse than
// the backlog: a customer who typed something and never saw it happen has been lied to twice. Everything
// here is visible-and-cancellable, never quiet.
//
// Pure. `estelle.js` owns the array and readline; this owns what it MEANS and how it reads.

/** How many lines are waiting behind the one being answered. */
function depth(queued) {
  return Array.isArray(queued) ? queued.length : 0;
}

/**
 * The line shown while a turn runs and input is waiting behind it — or "" when nothing is.
 *
 * Shows the FIRST pending line, because that is the one that will run next and the one a customer needs
 * to recognise in order to decide whether to keep it. A bare count ("3 queued") tells them a number and
 * not a decision.
 */
function preview(queued, c, max) {
  const n = depth(queued);
  if (!n) return "";
  const width = Math.max(20, max || 48);
  const head = String(queued[0] || "").replace(/\s+/g, " ").trim();
  const shown = head.length > width ? `${head.slice(0, width - 1)}…` : head;
  const more = n > 1 ? c.dim(` +${n - 1} more`) : "";
  return `  ${c.dim("queued")} ${c.bold(shown)}${more}${c.dim("  · esc to clear")}`;
}

/** What ESC means while a turn is running.
 *
 * `clear` only when something is actually queued — otherwise ESC belongs to whatever else wants it (the
 * slash menu dismisses on ESC), and stealing a key that has another meaning is its own defect. */
function escapeAction(queued, busy) {
  return busy && depth(queued) > 0 ? "clear" : "";
}

/** The confirmation after clearing. Names the COUNT, because the customer is entitled to know exactly
 * how much of their typing was discarded — by them, deliberately, which is the only way it may happen. */
function clearedLine(n, c) {
  const k = Math.max(0, Number(n) || 0);
  if (!k) return "";
  return `  ${c.dim(`cleared ${k} queued line${k === 1 ? "" : "s"} — nothing was sent`)}`;
}

/**
 * 🔴 THE GUARD THAT MATTERS MOST: a line that has been waiting a long time is probably stale.
 *
 * Nine to twenty seconds per turn (#101) means a customer who types three things is answered minutes
 * later. `staleAfterMs` is not a timeout — it never drops anything — it decides whether to ASK before
 * running a line the customer may have typed in a different frame of mind. Returns the lines to confirm.
 */
function stale(queued, enqueuedAt, now, staleAfterMs) {
  const ms = Number(staleAfterMs) || 45000;
  const at = Array.isArray(enqueuedAt) ? enqueuedAt : [];
  return (Array.isArray(queued) ? queued : [])
    .map((line, i) => ({ line, waited: Number(now) - Number(at[i] || now) }))
    .filter((x) => x.waited >= ms);
}

module.exports = { depth, preview, escapeAction, clearedLine, stale };
