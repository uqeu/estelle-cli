"use strict";
// MODULE 1 — THE TRANSCRIPT. Founder ruling 2026-08-02, `docs/CLI-MASTER-BRIEF.md` §A2.
//
// > **ESTELLE HAS NO INTERACTION MODEL.** Every prompt, every answer, every menu hand-rolls its own
// > printing. That is why they disagree with each other, double-print, cannot be selected from, and leave
// > no record of what was chosen. **Each defect is one instance of the same absence.**
//
// This is the first of the two modules that replace that absence: **an append-only log of TYPED ENTRIES
// that owns all rendering above the composer.** The rule it exists to enforce is one sentence:
//
//        ⛔ NOTHING PRINTS TO STDOUT DIRECTLY, EVER AGAIN.
//
// Four of the eight observed symptoms are fixed AT THE ROOT by that rule alone, because all four are the
// same bug — an ad-hoc write racing a redraw:
//
//   (a) 🔴 the customer's OWN INPUT was never in the transcript. Scrolling back showed Estelle's answers
//       and not the questions. `repl.js` recorded the user's line into `curate`'s MODEL-facing transcript
//       and printed only the reply, so the session was readable by the model and unreadable by the human
//       who typed it. **"The one that makes the whole CLI unreadable."**
//   (c) the escalate block printed TWICE, identically — two call sites, each printing for itself.
//   (d) the footer printed SEVEN times on shift+tab — a redraw that appended instead of replacing.
//   (e) a message typed and sent VANISHED — nothing had ever been responsible for echoing it.
//
// WHY A LOG AND NOT A PRINTER. A printer would fix (e) and leave (a): the transcript has to be a VALUE
// you can re-render, because `screen.js` repaints the whole viewport on every scroll and resize. An entry
// that was merely printed once cannot survive a reflow — which is why the scrollback and the record must
// be the same object.
//
// THE SEAM WITH THE TWO NEIGHBOURS, so nobody re-derives it:
//   * `transcript.js` (here)  — WHAT was said, as typed entries. Append-only, immutable, pure.
//   * `screen.js`             — WHICH ROWS are visible, wrapping and scrolling. Already existed.
//   * `curate.js`             — what the MODEL is told. A DIFFERENT transcript with a different job:
//                               it evicts and distils. Do not merge them; the customer's record must
//                               never lose a turn because the model's window needed the space.
//
// DELETION TEST (the founder's own criterion): remove this and every call site re-implements ordering,
// styling and redraw. It earns its keep at the first caller.

// The six kinds. A closed set on purpose: an entry that does not fit one of these is a new KIND of thing
// in the conversation, and that is a decision to make deliberately rather than by passing a stray string.
const KINDS = ["user", "answer", "tool", "choice", "notice", "error"];

/** A new, empty transcript. */
function create() {
  return { entries: [] };
}

/** `transcript` + one entry, as a NEW transcript. Never mutates — the repo's immutability rule, and here
 * it is load-bearing rather than stylistic: the renderer and the scrollback both hold references, and an
 * in-place push would let a repaint see half an append. */
function append(t, entry) {
  const e = normalise(entry);
  return { ...t, entries: [...((t && t.entries) || []), e] };
}

/** Coerce anything into a valid entry. An unknown kind becomes a `notice` rather than throwing: losing a
 * line from the customer's record is worse than showing it under a slightly wrong heading, and a throw
 * here would take down the session mid-turn. */
function normalise(entry) {
  const e = entry || {};
  const kind = KINDS.includes(e.kind) ? e.kind : "notice";
  return {
    kind,
    text: String(e.text === undefined || e.text === null ? "" : e.text),
    // `meta` carries what a KIND needs and nothing else — the choice's question, the tool's name, the
    // error's code. Deliberately free-form: constraining it would push callers back to formatting their
    // own strings, which is the thing this module exists to stop.
    meta: e.meta && typeof e.meta === "object" ? { ...e.meta } : {},
  };
}

// Named constructors, so a caller never types a kind string. `user("why is x slow?")` cannot be spelled
// wrong; `{kind: "usr"}` can, and would silently become a notice.
const user = (text) => ({ kind: "user", text });
const answer = (text) => ({ kind: "answer", text });
const tool = (name, text) => ({ kind: "tool", text, meta: { name } });
const choice = (question, picked) => ({ kind: "choice", text: picked, meta: { question } });
const notice = (text) => ({ kind: "notice", text });
const error = (text) => ({ kind: "error", text });

/**
 * ⛔ THE ONE FUNCTION. Every entry renders here and nowhere else.
 *
 * The gutter is what makes a scrolled-back session readable: the eye finds `›` and knows a question
 * starts there. Codex and Kimi both do exactly this, and it is the single cheapest thing that turns a
 * wall of output into a conversation.
 *
 *   ›  what the customer typed        — the line that was MISSING entirely (symptom a)
 *      the answer, indented
 *   ⟢  a tool result, named
 *   ✓  a choice, with the question it answered (symptom b's other half: the record of what was chosen)
 *   ·  a notice
 *   ✗  an error
 */
function renderEntry(entry, c) {
  const e = normalise(entry);
  const body = e.text.split("\n");
  switch (e.kind) {
    case "user":
      // The customer's own words, marked and BRIGHT. Dimming them would be the same mistake one shade
      // over: this is the line they scroll back to FIND.
      return body.map((l, i) => `  ${i === 0 ? c.teal("›") : " "} ${c.bold(l)}`);
    case "answer":
      return body.map((l) => `  ${l}`);
    case "tool":
      return [`  ${c.teal("⟢ " + String(e.meta.name || "tool"))}`,
              ...body.filter((l) => l.length || body.length === 1).map((l) => `  ${c.dim(l)}`)];
    case "choice":
      // WHAT WAS ASKED AND WHAT WAS CHOSEN, together and permanently. A prompt that vanishes leaving no
      // record is the same unreadability as a missing question — the founder named them as one defect.
      return [`  ${c.green("✓")} ${c.dim(String(e.meta.question || "chose"))} ${c.bold(e.text)}`];
    case "error":
      return body.map((l, i) => `  ${i === 0 ? c.red("✗") : " "} ${l}`);
    default:
      return body.map((l, i) => `  ${i === 0 ? c.dim("·") : " "} ${c.dim(l)}`);
  }
}

/** Every line of the whole transcript, in order. What `screen.js` paints and what a reflow re-derives. */
function lines(t, c) {
  const out = [];
  for (const e of (t && t.entries) || []) {
    out.push(...renderEntry(e, c));
    out.push("");            // one blank between entries, decided HERE so no caller has to remember it
  }
  return out;
}

/** The last entry of a kind, or null. Used to answer "did we already say this?" — the guard against the
 * double-print in symptom (c), available to every caller instead of re-invented per call site. */
function lastOf(t, kind) {
  const es = (t && t.entries) || [];
  for (let i = es.length - 1; i >= 0; i -= 1) if (es[i].kind === kind) return es[i];
  return null;
}

/** True when appending `entry` would repeat the entry already at the tail, byte for byte.
 *
 * Symptom (c) — "the same escalate block printed twice, identically, in a row" — is exactly this
 * predicate returning true and nobody having asked. Kept as a QUERY rather than enforced inside
 * `append`, because a customer legitimately asking the same question twice must appear twice; only the
 * callers that redraw a block should suppress. */
function repeats(t, entry) {
  const es = (t && t.entries) || [];
  const last = es[es.length - 1];
  if (!last) return false;
  const e = normalise(entry);
  return last.kind === e.kind && last.text === e.text
    && JSON.stringify(last.meta) === JSON.stringify(e.meta);
}

/** Append unless it would be a byte-identical repeat of the tail. The double-print guard as one call. */
function appendOnce(t, entry) {
  return repeats(t, entry) ? t : append(t, entry);
}

module.exports = {
  KINDS, create, append, appendOnce, repeats, normalise, renderEntry, lines, lastOf,
  user, answer, tool, choice, notice, error,
};
