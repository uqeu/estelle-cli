"use strict";
// MODULE 2 — THE INTERACTION SURFACE. Founder ruling 2026-08-02, `docs/CLI-MASTER-BRIEF.md` §A2.
//
// ONE component owning the screen region BELOW the transcript. Its whole interface is **"ask the user for
// X"** — and every existing prompt becomes a caller that renders NOTHING itself.
//
// 🔴 THE DEFECT IT REPLACES, in the founder's words: *"The `escalate` prompt is a PRINTED LIST YOU CANNOT
// SELECT FROM. It asked 'Which repo should I check this against?' and listed `- isoproof-bravo /
// - uqeu/estelle` as plain text. No arrows, no numbers, no highlight, no way to answer. **A question with
// no input mechanism.**"* That is not a missing feature in one prompt; it is the absence of this module.
// The server had already composed a perfectly good question with candidates — there was simply nothing in
// the CLI that knew how to ASK anything except free text.
//
// THE SHAPE IS CODEX'S, TAKEN AS A SHAPE AND NOT AS CODE (`vendor-reference/codex/codex-rs/tui/src/
// bottom_pane/`, Apache-2.0, Rust). Their `BottomPane` owns the composer plus a stack of transient
// `BottomPaneView`s that temporarily REPLACE it, each one reporting `is_complete` and a completion of
// `Accepted | Cancelled`. Ours is the same contract in the smallest form that fits a readline session:
// a spec goes in, a `{ok, value, cancelled}` comes out, and the SURFACE — never the caller — owns the
// keys, the highlight, the cursor and where it draws.
//
// ⛔ AND IT WRITES THE RESULT BACK INTO THE TRANSCRIPT. That is not bookkeeping, it is half the defect:
// *"a prompt that vanishes leaving no record of the choice is the same unreadability as"* the missing
// input line. A session must read as a conversation afterward — what I asked, what it answered, what it
// asked me, and **what I chose**.
//
// WHAT IS PURE AND WHAT IS NOT, because E-027 says the seam is where the bug lives: every DECISION here
// (which rows match, which is selected, what a keypress means, what gets rendered) is a pure function
// exported and tested by name. Only `ask()` touches I/O, and it is a thin loop over those functions —
// so the join is exercised by `ask()`'s own tests rather than falling between the two halves.

const secretPrompt = require("./secret-prompt.js");
const transcript = require("./transcript.js");

// The kinds of question this surface can ask. A closed set: a caller needing something else is a caller
// asking for a new INTERACTION, which is a deliberate addition here rather than a hand-rolled prompt
// somewhere else — that is the entire premise of the module.
const KINDS = ["text", "choice", "confirm", "multi", "secret"];

/** What a key means, as a pure decision. Separated so the whole keyboard contract is testable without a
 * terminal — the half that had no tests at all, in the module whose absence caused all eight symptoms. */
function keyAction(key, ch) {
  const k = key || {};
  if (k.name === "escape") return "cancel";
  if (k.ctrl && (k.name === "c" || k.name === "d")) return "cancel";
  if (k.name === "return" || k.name === "enter") return "accept";
  if (k.name === "up" || (k.ctrl && k.name === "p")) return "up";
  if (k.name === "down" || (k.ctrl && k.name === "n")) return "down";
  if (k.name === "space") return "toggle";
  if (typeof ch === "string" && /^[1-9]$/.test(ch)) return `pick:${Number(ch) - 1}`;
  return "";
}

/** Clamp a selection into range, wrapping. Wrapping is deliberate: a list you cannot leave the bottom of
 * feels broken, and every reference harness wraps. */
function moveSelection(index, delta, count) {
  if (count <= 0) return 0;
  return ((index + delta) % count + count) % count;
}

/** Normalise a caller's option into `{value, label, hint}`. A bare string is the common case and must
 * stay a bare string at the call site — a prompt that requires ceremony gets hand-rolled instead. */
function option(o) {
  if (o === null || o === undefined) return { value: "", label: "", hint: "" };
  if (typeof o === "string") return { value: o, label: o, hint: "" };
  return {
    value: o.value !== undefined ? o.value : String(o.label || ""),
    label: String(o.label !== undefined ? o.label : o.value),
    hint: String(o.hint || ""),
  };
}

/**
 * The rendered question, as lines. **BELOW the transcript and above nothing** — symptom (g) was the slash
 * menu drawing ABOVE the prompt, which is a decision this function now owns instead of each caller.
 *
 * Numbers AND arrows, both. The founder's report named both as missing; offering only arrows leaves a
 * piped or screen-reader session with no way to answer, and offering only numbers is slower for the eye.
 */
function render(spec, state, c) {
  const s = spec || {};
  const lines = [];
  if (s.question) lines.push(`  ${c.bold(String(s.question))}`);
  if (s.detail) for (const d of String(s.detail).split("\n")) lines.push(`  ${c.dim(d)}`);
  const opts = (s.options || []).map(option);
  if (s.kind === "choice" || s.kind === "multi") {
    const width = opts.reduce((w, o) => Math.max(w, o.label.length), 0);
    opts.forEach((o, i) => {
      const here = i === state.index;
      const mark = s.kind === "multi"
        ? (state.chosen && state.chosen.has(i) ? c.green("[x]") : c.dim("[ ]"))
        : (here ? c.teal("›") : " ");
      const num = c.dim(`${i + 1}.`);
      const label = here ? c.bold(o.label.padEnd(width)) : o.label.padEnd(width);
      lines.push(`  ${mark} ${num} ${label}${o.hint ? "  " + c.dim(o.hint) : ""}`);
    });
    lines.push(`  ${c.dim(s.kind === "multi"
      ? "space to toggle · ↑↓ or 1-9 to move · enter to confirm · esc to cancel"
      : "↑↓ or 1-9 to choose · enter to confirm · esc to cancel")}`);
  } else if (s.kind === "confirm") {
    lines.push(`  ${c.dim("y / N · enter for no · esc to cancel")}`);
  }
  return lines;
}

/** The transcript entry a completed question leaves behind. Exported because it is the RECORD half, and
 * a caller must never compose it — two callers composing it differently is how the escalate block came
 * to print twice in two shapes. */
function recordOf(spec, result) {
  const q = String((spec && spec.question) || "asked");
  if (!result || result.cancelled) return transcript.choice(q, "cancelled");
  if (Array.isArray(result.value)) return transcript.choice(q, result.value.join(", ") || "nothing");
  if (spec && spec.kind === "secret") return transcript.choice(q, "(saved)");
  return transcript.choice(q, String(result.value));
}

/** What `ask` returns when the caller cancelled or the stream closed. A named constructor so `cancelled`
 * is never spelled as a falsy `value` — "the user chose the empty string" and "the user cancelled" are
 * different answers, and collapsing them is how a prompt silently accepts nothing. */
const cancelled = () => ({ ok: false, cancelled: true, value: null });
const accepted = (value) => ({ ok: true, cancelled: false, value });

/**
 * ASK THE USER FOR X. The only impure function in the file.
 *
 * `io` supplies: `out(line)` (which MUST be the transcript's writer — nothing prints to stdout directly),
 * `prompt(label)` for a line read, `promptSecret` for a masked one, `c` for colour, and `keys(handler)`
 * to bind raw keypresses on a TTY.
 *
 * 🔴 THE NON-TTY PATH IS NOT A DEGRADED PATH, IT IS THE TESTED ONE. Every scripted run, CI job and
 * walkthrough drives this without a keyboard, so a selection must be answerable by TYPING a number or a
 * value. A surface that only works with arrow keys is a surface no test can walk — and "nothing walks it"
 * is the stated reason all eight symptoms shipped.
 */
async function ask(spec, io) {
  const s = { kind: "text", ...(spec || {}) };
  const { out, c } = io;
  if (!KINDS.includes(s.kind)) return cancelled();

  if (s.kind === "secret") {
    // The masked read — symptom (h), the key prompt echoing the credential in plaintext. `secret-prompt`
    // already existed and already knew how; nothing had made it the ONE way to ask for a secret.
    const reader = io.promptSecret || secretPrompt.promptSecret || io.prompt;
    const value = await reader(`  ${c.teal(s.label || "value")} ${c.dim("›")} `);
    return value === null || value === undefined ? cancelled() : accepted(String(value).trim());
  }

  if (s.kind === "text") {
    for (const l of render(s, { index: 0 }, c)) out(l);
    const value = await io.prompt(`  ${c.teal(s.label || "›")} `);
    return value === null ? cancelled() : accepted(String(value));
  }

  if (s.kind === "confirm") {
    for (const l of render(s, { index: 0 }, c)) out(l);
    const value = await io.prompt(`  ${c.teal(s.label || "y/N")} ${c.dim("›")} `);
    if (value === null) return cancelled();
    return accepted(/^y(es)?$/i.test(String(value).trim()));
  }

  // choice / multi
  const opts = (s.options || []).map(option);
  if (!opts.length) return cancelled();
  const state = { index: 0, chosen: new Set() };
  const bind = io.keys;
  if (bind) {
    // TTY: arrows and numbers, redrawn IN PLACE. `bind` returns an unbind and resolves with the action,
    // so this file never touches stdin itself — the same split `mode-ui.js` uses, for the same reason.
    const picked = await bind(state, s);
    if (picked === null) return cancelled();
    return accepted(s.kind === "multi" ? picked.map((i) => opts[i].value) : opts[picked].value);
  }
  for (const l of render(s, state, c)) out(l);
  const typed = await io.prompt(`  ${c.teal(s.label || "choose")} ${c.dim("›")} `);
  if (typed === null) return cancelled();
  const answer = resolveTyped(String(typed), opts, s.kind);
  return answer === null ? cancelled() : accepted(answer);
}

/**
 * A typed answer → the chosen value(s), or null when it matches nothing.
 *
 * Accepts a 1-based number, an exact value, or an unambiguous prefix — the three things a person
 * actually types. **Ambiguity resolves to NULL, never to a guess:** "isoproof" matching two repos must
 * re-ask, because silently picking one is how a grounding scope gets chosen for someone.
 */
function resolveTyped(typed, options, kind) {
  const opts = (options || []).map(option);
  const parts = String(typed || "").split(/[\s,]+/).map((p) => p.trim()).filter(Boolean);
  if (!parts.length) return null;
  const one = (p) => {
    if (/^[0-9]+$/.test(p)) {
      const i = Number(p) - 1;
      return i >= 0 && i < opts.length ? opts[i].value : null;
    }
    const low = p.toLowerCase();
    const exact = opts.filter((o) => String(o.value).toLowerCase() === low
                                  || o.label.toLowerCase() === low);
    if (exact.length === 1) return exact[0].value;
    const pre = opts.filter((o) => String(o.value).toLowerCase().startsWith(low)
                                || o.label.toLowerCase().startsWith(low));
    return pre.length === 1 ? pre[0].value : null;      // 2+ matches is AMBIGUOUS, and ambiguous is null
  };
  if (kind === "multi") {
    const picked = parts.map(one).filter((v) => v !== null);
    return picked.length ? picked : null;
  }
  return one(parts[0]);
}

/**
 * Ask, then RECORD — the call every caller should use.
 *
 * Returns `{result, transcript}`: the answer, and the transcript with the choice appended. Bundling them
 * is what makes "the session reads as a conversation afterward" the default rather than a thing each
 * caller has to remember, and forgetting it is precisely symptom (b)'s second half.
 */
async function askAndRecord(spec, io, t) {
  const result = await ask(spec, io);
  return { result, transcript: transcript.append(t, recordOf(spec, result)) };
}

module.exports = {
  KINDS, keyAction, moveSelection, option, render, resolveTyped, recordOf,
  cancelled, accepted, ask, askAndRecord,
};
