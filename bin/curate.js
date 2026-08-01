"use strict";
// THE CURATED TURN — the half of the thesis that only works where Estelle assembles the prompt itself.
//
// The thesis: the LLM is an ENGINE, Estelle owns the MEMORY, and every turn gets exactly the working set it
// needs — so a 5,000-turn session is really 5,000 one-turn sessions with perfect context, and the model's
// window never fills, never rots, and never self-conditions on its own past errors.
//
// The eviction half of that needs a prompt Estelle CONTROLS. Inside Claude Code it cannot: no hook can remove
// content from context (PreCompact may only BLOCK or INJECT), and nothing lets an MCP server trigger a clear
// or choose what compaction keeps. Injection is the only lever there. In THIS CLI there is no such limit —
// the transcript is a local array and the prompt is assembled here, so eviction is real.
//
// WHAT IS PORTED, AND WHY ONLY THIS. serve/guardian.py's levers were measured on their own instrument
// (estelle.evals.lever_ablation, docs/anti-degradation.md). The verdicts, applied honestly:
//
//   * curate_errors  EARNS ITS PLACE — 50% -> 100% on a task needing the record of what was already tried,
//     at 3.4x fewer tokens than keeping the verbose trace. Its measured value is that the LESSON survives
//     the truncation that deletes the turn. PORTED, and it is the load-bearing lever here.
//   * rot_budget     no measured accuracy effect; a smaller prompt. A COST lever. Ported AS a cost lever and
//     described as one — never as an accuracy claim.
//   * _reminder      recitation earned NOTHING on that bench. NOT ported as its own lever. Lessons still land
//     at the high-attention tail because that is the position the measured arm used, not because tail
//     placement is itself known to help.
//   * repetition_ratio  BLIND past 12 turns (it reads only the last 8). NOT ported — a signal that reads 0.00
//     on a long loop is worse than no signal, because it would be trusted.
//
// Pure and dependency-free: every function takes data and returns new data, so the whole loop is unit-tested
// without a terminal, a socket or a model. Nothing here mutates its input.

// ~4 chars/token, the same conservative estimate as estelle.serve.metering.estimate_tokens, so a budget set
// here means the same thing a budget set server-side does.
const CHARS_PER_TOKEN = 4;

/** A conservative token estimate for `text` (~4 chars/token, rounded up). */
function estimateTokens(text) {
  return Math.ceil(String(text || "").length / CHARS_PER_TOKEN);
}

/** `text` sliced to at most `maxTokens` by that estimate ("" at or below zero). */
function truncateToTokens(text, maxTokens) {
  const n = Math.max(0, Math.floor(maxTokens));
  const body = String(text || "");
  return n <= 0 ? "" : body.slice(0, n * CHARS_PER_TOKEN);
}

/** The text of a turn, whether it is a `{role, content}` record or a bare string. */
function content(turn) {
  if (turn && typeof turn === "object") return String(turn.content || "");
  return String(turn || "");
}

function role(turn) {
  return turn && typeof turn === "object" ? String(turn.role || "user") : "user";
}

// A failed-attempt turn — what the Illusion-of-Diminishing-Returns paper says poisons a trace by
// self-conditioning. VETOED by resolution vocabulary: a turn that narrates a failure it then FIXED is a
// success story, and evicting it would lose real work and plant a false "avoid" lesson. Kept byte-faithful to
// guardian.py's `_ERROR` / `_RESOLVED` so the two implementations classify the same turn the same way.
const ERROR_RE = new RegExp(
  "\\b(?:[a-z]*errors?|traceback|exception|failed|failure|assertion|does ?n['o]t exist|not found|" +
  "undefined|refused|denied|cannot|could ?n['o]t|invalid|timeout|timed out)\\b", "i");
const RESOLVED_RE = new RegExp(
  "\\b(?:fixed|resolved|solved|succeed(?:ed|s)?|tests? (?:now )?pass(?:es|ing)?|" +
  "now (?:works|working|passes|passing)|works now|all green)\\b", "i");

/** True when a turn reads as an UNRESOLVED failed attempt — a curate-and-evict candidate.
 *
 * A USER turn is never one: it carries intent, and intent is the one thing a session may not lose. Same
 * heuristic limits guardian.py documents apply — a status line containing "errors" distils to a junk lesson,
 * and a genuine failure whose prose says "fixed" is vetoed. Erring toward KEEPING is the safe direction. */
function looksLikeError(turn) {
  if (role(turn) === "user") return false;
  const body = content(turn);
  return ERROR_RE.test(body) && !RESOLVED_RE.test(body);
}

/** Lowercased, whitespace-collapsed head of a text — the dedup key for lessons. */
function normalize(text) {
  return String(text || "").toLowerCase().split(/\s+/).filter(Boolean).join(" ").slice(0, 200);
}

/** A bounded "tried X -> failed: Y" lesson distilled from a failed-attempt turn.
 *
 * The first substantive line is what was tried; the first line naming a failure is what went wrong. This is
 * the ONLY trace of an old failed attempt that survives truncation, so it carries the whole value of the
 * lever — the verbose turn it came from is dropped. */
function lessonFrom(turn) {
  const lines = content(turn).split("\n").map((l) => l.trim()).filter(Boolean);
  const tried = lines.find((l) => l.length >= 8) || lines[0] || "";
  const failed = lines.find((l) => ERROR_RE.test(l)) || "";
  const triedShort = tried.slice(0, 90);
  if (failed && normalize(failed) !== normalize(tried)) {
    return `tried: ${triedShort} -> failed: ${failed.slice(0, 110)}`;
  }
  return `avoid: ${triedShort || failed.slice(0, 110)}`;
}

/** Order-preserving, case-insensitive dedup of lesson strings. */
function dedupeLessons(lessons) {
  const seen = new Set();
  const out = [];
  for (const lesson of lessons || []) {
    const text = String(lesson || "").trim();
    if (!text) continue;
    const key = normalize(text);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(text);
  }
  return out;
}

/**
 * Distil OLD failed-attempt turns into bounded lessons and EVICT the verbose turns.
 *
 * Returns `{kept, lessons}` — the trace minus the old failure turns, plus the deduped lessons the caller
 * re-injects as one compact note. The last `keepRecent` turns are kept verbatim whatever they contain: a turn
 * still being worked on is part of the live working set and must never be evicted out from under it.
 *
 * Pure: a new array every time, the input untouched.
 */
function curateErrors(turns, keepRecent = 4) {
  const list = Array.isArray(turns) ? turns : [];
  const cutoff = list.length - Math.max(0, keepRecent);
  const kept = [];
  const lessons = [];
  for (let i = 0; i < list.length; i++) {
    if (i < cutoff && looksLikeError(list[i])) {
      lessons.push(lessonFrom(list[i]));
      continue;   // evict the verbose failure turn; the lesson survives, the self-conditioning mass does not
    }
    kept.push(list[i]);
  }
  return { kept, lessons: dedupeLessons(lessons) };
}

// ── the transcript Estelle owns ─────────────────────────────────────────────────
//
// A ring, not a log. The full session still goes to /checkpoint (durable, recallable); what lives HERE is
// only what a turn might need. When a turn falls off the end its LESSON is distilled first and carried
// forever, which is the property that makes the 5,000-turn claim true rather than rhetorical: the verbose
// turn is gone, what it taught is not.
const MAX_LIVE_TURNS = 400;

/** A fresh, empty transcript state. */
function emptyTranscript() {
  return { turns: [], lessons: [] };
}

/**
 * `state` with `turn` appended — a NEW state, never a mutation.
 *
 * Past `MAX_LIVE_TURNS` the oldest turns fall off the live ring, but not before their lessons are distilled
 * and merged into the carried set. Nothing a failed attempt taught is ever lost to the ring.
 */
function record(state, turn) {
  const prior = (state && Array.isArray(state.turns) ? state.turns : []);
  const carried = (state && Array.isArray(state.lessons) ? state.lessons : []);
  const text = content(turn);
  if (!text.trim()) return { turns: prior, lessons: carried };     // a blank turn is not a turn
  const turns = [...prior, { role: role(turn), content: text }];
  if (turns.length <= MAX_LIVE_TURNS) return { turns, lessons: carried };
  const overflow = turns.slice(0, turns.length - MAX_LIVE_TURNS);
  const { lessons } = curateErrors(overflow, 0);                   // keepRecent 0: the whole overflow is old
  return { turns: turns.slice(-MAX_LIVE_TURNS), lessons: dedupeLessons([...carried, ...lessons]) };
}

// ── the working set for ONE turn ────────────────────────────────────────────────

// What the CLI is willing to spend on carried context, and how many turns stay verbatim. The budget is a COST
// lever (guardian.py's rot_budget measured no accuracy effect) — it is here to keep a long session's prompt
// bounded, and it is not claimed to make answers better.
const TURN_BUDGET_TOKENS = 6000;
const RECENT_TURNS = 6;
const MAX_LESSONS = 12;

/** Append `text` if it fits the remaining budget, truncating to the remainder when it partly fits. */
function pack(turn, used, budget) {
  const text = content(turn).trim();
  if (!text) return { packed: null, used };
  const remaining = budget - used;
  if (remaining <= 0) return { packed: null, used };
  const cost = estimateTokens(text);
  if (cost <= remaining) return { packed: { role: role(turn), content: text }, used: used + cost };
  const cut = truncateToTokens(text, remaining);
  return { packed: { role: role(turn), content: cut }, used: used + estimateTokens(cut) };
}

/**
 * The context ONE turn carries: the recent turns verbatim, plus every lesson from the failed attempts that
 * are no longer verbatim.
 *
 * Returns `{verbatim, lessons, tokens, evicted, dropped}`:
 *   verbatim — the turns the model sees in full, oldest first, bounded by `budgetTokens`
 *   lessons  — carried lessons plus the ones distilled from this window's older failures
 *   tokens   — what the whole carried block costs
 *   evicted  — how many failed-attempt turns were distilled away
 *   dropped  — how many older turns simply did not make the verbatim window
 *
 * HONEST NOTE on eviction, the same one guardian.py makes about itself: because `keepRecent` is pinned at the
 * verbatim window, curation can never change WHICH turns survive — last-N truncation would have dropped those
 * same old turns anyway. What curation uniquely adds is that the LESSON survives that truncation at all. That
 * is the whole measured value, and it is not small: it is the difference between the model re-trying a fix it
 * already watched fail and knowing not to.
 */
function workingSet(state, options) {
  const opts = options || {};
  const budget = Math.max(0, opts.budgetTokens == null ? TURN_BUDGET_TOKENS : opts.budgetTokens);
  const recent = Math.max(0, opts.recentTurns == null ? RECENT_TURNS : opts.recentTurns);
  const turns = (state && Array.isArray(state.turns) ? state.turns : []);
  const carried = (state && Array.isArray(state.lessons) ? state.lessons : []);

  const { lessons: fresh } = curateErrors(turns, recent);
  const lessons = dedupeLessons([...carried, ...fresh]).slice(-MAX_LESSONS);

  // The lessons block is reserved FIRST. It is the measured lever, and a budget that spends itself on verbose
  // recent turns and then has no room for the lesson would drop the one thing worth carrying.
  const lessonText = renderLessons(lessons);
  const lessonCost = lessonText ? estimateTokens(lessonText) : 0;
  const verbatimBudget = Math.max(0, budget - lessonCost);

  // Newest-first so the freshest turns win the budget; emitted oldest-first so the conversation reads forward.
  const picked = [];
  let used = 0;
  const window = recent > 0 ? turns.slice(-recent) : [];
  for (let i = window.length - 1; i >= 0; i--) {
    const { packed, used: next } = pack(window[i], used, verbatimBudget);
    if (packed === null) break;
    picked.push(packed);
    used = next;
  }
  const verbatim = picked.reverse();
  return {
    verbatim, lessons, tokens: used + lessonCost,
    evicted: fresh.length,
    dropped: Math.max(0, turns.length - verbatim.length),
  };
}

/** The lessons note, or "" when there is nothing to say. Advisory wording: the failure detection is a
 * heuristic, so it reports what APPEARED to fail rather than banning the path outright. */
function renderLessons(lessons) {
  const list = (lessons || []).filter(Boolean);
  if (!list.length) return "";
  return "LESSONS (earlier attempts in this session that appeared to fail — do not repeat them unchanged):\n"
    + list.map((l) => `- ${l}`).join("\n");
}

const CONTEXT_HEADER = "SESSION SO FAR (curated by Estelle — older failed attempts are distilled below):";

/**
 * The working set as the text that travels with the question.
 *
 * `/deep-search` takes a question, not a message array, so the carried context is rendered into it. Order is
 * deliberate: the recent turns first, then the lessons, then the question — lessons sit at the high-attention
 * tail because that is the position the measured ablation arm used. Tail placement is NOT independently
 * measured to help; the LESSONS are what earned the result.
 */
function renderTurn(set, question) {
  const parts = [];
  const turns = (set && set.verbatim) || [];
  if (turns.length) {
    parts.push(CONTEXT_HEADER + "\n"
      + turns.map((t) => `${role(t) === "assistant" ? "Estelle" : "You"}: ${content(t)}`).join("\n\n"));
  }
  const lessons = renderLessons((set && set.lessons) || []);
  if (lessons) parts.push(lessons);
  const q = String(question || "").trim();
  parts.push(turns.length || lessons ? `CURRENT QUESTION: ${q}` : q);
  return parts.join("\n\n");
}

/**
 * What one reply contributes to the transcript.
 *
 * A reply is not only its prose: a blocked gate and an ungrounded symbol list are the two most useful things
 * a later turn can know, and they are exactly the "failed attempt" a naive transcript would carry verbatim
 * forever. The wording is chosen so the classifier CAN see them — the failure detection is keyword-based (a
 * limit guardian.py documents rather than hides), so text Estelle writes itself should say plainly that
 * something failed rather than leaving the lever to guess.
 */
function replyText(res) {
  if (!res) return "";
  if (res.error) return `failed: ${String((res.error && res.error.message) || res.error)}`;
  const parts = [String(res.answer || "").trim()];
  const graded = res.merge !== undefined || res.verdict;
  if (graded && !(res.merge === true || res.verdict === "clean")) {
    parts.push(`this change failed the merge gate: ${res.verdict || "not clean"}`);
  }
  const ungrounded = (res.ungrounded || []).slice(0, 6);
  if (ungrounded.length) parts.push(`these symbols were not found in this repo: ${ungrounded.join(", ")}`);
  return parts.filter(Boolean).join("\n");
}

/** The receipt for `/context`: exactly what the next turn will carry, and what it cost. */
function contextReport(state, options) {
  const set = workingSet(state, options);
  const total = (state && Array.isArray(state.turns) ? state.turns.length : 0);
  return {
    ...set,
    total,
    lines: [
      ["turns", `${set.verbatim.length} verbatim of ${total} recorded`],
      ["lessons", set.lessons.length ? `${set.lessons.length} carried` : "none"],
      ["evicted", `${set.evicted} failed-attempt turn${set.evicted === 1 ? "" : "s"} distilled`],
      ["cost", `~${set.tokens} tokens carried into the next turn`],
    ],
  };
}

module.exports = {
  estimateTokens, truncateToTokens, content, role,
  looksLikeError, lessonFrom, curateErrors, dedupeLessons,
  emptyTranscript, record, workingSet, renderLessons, renderTurn, contextReport, replyText,
  MAX_LIVE_TURNS, TURN_BUDGET_TOKENS, RECENT_TURNS, MAX_LESSONS, CONTEXT_HEADER,
};
