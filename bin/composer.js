"use strict";
// THE COMPOSER AND ITS STATUS LINE — brief §1.3.
//
// Kimi renders a large bordered input with a status line beneath it. Estelle should have the same for the
// same reason: **long sessions with long prompts.** A one-line `read_only ›` prompt is the wrong shape for
// a product people paste stack traces and diffs into.
//
// Everything here is PURE — the border, the status line, the byte formatting. The REPL owns the readline;
// this file owns what the frame looks like, which is the split `mode-ui.js` and `slash-menu.js` both use
// and the reason any of it can be tested.
//
// 🔴 THE MODE APPEARS TWICE — in the composer border (`─ input · plan ─`, Kimi's placement) AND in the
// status line — from ONE source of truth, redrawn in place, never appended. Two renderings of one fact is
// fine; two SOURCES for it is the two-ladders defect that `TestTheCliIsAViewOfThisLadderAndNotASecondOne`
// exists to prevent.

const BOX = { tl: "┌", tr: "┐", bl: "└", br: "┘", h: "─", v: "│" };

/** `16,991` — a count a human can read at a glance. No locale dependency: the CLI must render the same
 * string on every machine, and `toLocaleString` does not. */
function commas(n) {
  const s = String(Math.max(0, Math.floor(Number(n) || 0)));
  return s.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** `262.1k` / `1.2M` / `847` — the compact form used inside the budget readout. */
function compact(n) {
  const v = Math.max(0, Math.floor(Number(n) || 0));
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}k`;
  return String(v);
}

/**
 * `memory 0.4% (92/250M)` — OUR answer to Kimi's `context: 0.0% (0/262.1k)`.
 *
 * Theirs is the model's context window. **Ours is the memory-token budget against the plan cap, which is a
 * number we already hold and have never shown** — and it is the more useful one, because a context window
 * resets every turn while a memory budget is what a customer actually buys.
 *
 * Returns "" when there is no cap to measure against: a percentage with no denominator is the kind of
 * confident meaningless number this product exists to refuse.
 */
function budgetReadout(held, cap) {
  const c = Number(cap) || 0;
  if (c <= 0) return "";
  const h = Math.max(0, Number(held) || 0);
  const pct = (h / c) * 100;
  // one decimal, but never round a non-empty budget down to a flat 0.0% — "some" must not read as "none"
  const shown = pct > 0 && pct < 0.05 ? "<0.1" : pct.toFixed(1);
  return `memory ${shown}% (${compact(h)}/${compact(c)})`;
}

/** `fable-fix-campaign [± ↑254]` — branch plus dirty/ahead, or just the branch, or "". */
function branchLabel(git) {
  const g = git || {};
  const name = String(g.branch || "").trim();
  if (!name) return "";
  const marks = [g.dirty ? "±" : "", Number(g.ahead) > 0 ? `↑${g.ahead}` : ""].filter(Boolean).join(" ");
  return marks ? `${name} [${marks}]` : name;
}

/**
 * The status line under the composer: mode · cwd · branch · newline hint · memory budget.
 *
 * Empty segments are DROPPED rather than rendered as placeholders — a status line full of dashes teaches
 * nothing and costs a row. Joined with the same separator the header uses.
 */
function statusLine(state) {
  const s = state || {};
  const mode = s.mode ? `${s.mode}${s.modeWhat ? ` · ${s.modeWhat}` : ""}` : "";
  return [mode, s.cwd, branchLabel(s.git), s.newlineHint || "ctrl-j: newline",
          budgetReadout(s.held, s.cap)].filter(Boolean).join("   ");
}

/**
 * The composer frame. `width` is the terminal width; the label rides IN the top border, Kimi-style.
 *
 * The mode in the border and the mode in the status line come from the SAME `state.mode` — the caller
 * never passes two.
 */
function composer(state, width) {
  const s = state || {};
  const w = Math.max(20, Math.floor(Number(width) || 80) - 4);
  const label = s.mode ? ` input · ${s.mode} ` : " input ";
  const head = `${BOX.tl}${label}${BOX.h.repeat(Math.max(0, w - label.length - 2))}${BOX.tr}`;
  const body = `${BOX.v}${" ".repeat(Math.max(0, w - 2))}${BOX.v}`;
  const foot = `${BOX.bl}${BOX.h.repeat(Math.max(0, w - 2))}${BOX.br}`;
  return { head, body, foot, status: statusLine(s) };
}

/**
 * The boxed wordmark line: `┌ estelle  by Fate Labs                    v0.1.9 ┐`.
 *
 * BOXED, matching §1.1b's mockup. The brief briefly carried a "No rectangle — founder's call" bullet; that
 * was an INVENTED CONSTRAINT recorded from a garbled voice note and was never the founder's decision
 * (PLAN-ERRATA). Codex and Kimi both box their headers and he likes how they look. The bullet is deleted
 * rather than edited around, so no future reader inherits the phantom ruling.
 */
function headerBox(left, right, width) {
  const w = Math.max(24, Math.floor(Number(width) || 80) - 4);
  const inner = w - 2;
  const l = ` ${String(left || "")} `;
  const r = String(right || "") ? ` ${right} ` : "";
  const pad = Math.max(1, inner - l.length - r.length);
  return {
    head: `${BOX.tl}${BOX.h.repeat(inner)}${BOX.tr}`,
    body: `${BOX.v}${l}${" ".repeat(pad)}${r}${BOX.v}`.slice(0, w),
    foot: `${BOX.bl}${BOX.h.repeat(inner)}${BOX.br}`,
  };
}

module.exports = { BOX, commas, compact, budgetReadout, branchLabel, statusLine, composer, headerBox };
