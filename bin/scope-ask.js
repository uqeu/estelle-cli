"use strict";
// THE FIRST CALLER OF MODULE 2 — and the one the founder photographed.
//
// 🔴 THE DEFECT, in his words: *"The `escalate` prompt is a PRINTED LIST YOU CANNOT SELECT FROM. It asked
// 'Which repo should I check this against?' and listed `- isoproof-bravo / - uqeu/estelle` as plain text.
// No arrows, no numbers, no highlight, no way to answer. **A question with no input mechanism.**"* And
// then: *"The same escalate block PRINTED TWICE, identically, in a row."*
//
// Both halves have the same cause and it is not the escalate skill. **The server was already doing this
// correctly.** `/skill/run`, `/deep-search`, `/verify` and the merge gate all return the SAME fail-closed
// envelope — `{scope_ask: true, candidates: [...], question, reason}` — and all four accept a `repo` in
// the request that answers it (`skill_run.py:275-285`, `skill_run.py:297`). A complete, well-composed
// question with its own answer set, and **the CLI had nothing that could ask anything except free text.**
// So it printed the question as prose, the customer typed something, the request went back WITHOUT a
// repo, and the identical block came back — which is symptom (c), not a second bug.
//
// ⛔ WHY THIS IS ONE MODULE AND NOT FOUR CALL SITES. Four surfaces return this envelope. Four hand-rolled
// prompts would be four chances to disagree about what "cancel" means, whether the choice is recorded,
// and whether the retry carries the scope — which is exactly the shape of every defect in §A2. One
// module, four callers.
//
// FAIL-CLOSED IS PRESERVED, and that is the point of the whole exchange: a scope question exists because
// grounding against the wrong repo *"would produce a confident, plausible, entirely-wrong answer"*.
// Cancelling therefore leaves the request UNANSWERED — it never falls back to a default repo, never picks
// the first candidate, and never retries unscoped. An ambiguous typed answer resolves to null (`ask.js`
// `resolveTyped`) for the same reason.

const ask = require("./ask.js");
const transcript = require("./transcript.js");

/** The candidates a scope-ask envelope offers, normalised. Empty when this is not a scope ask. */
function candidatesOf(res) {
  const r = res || {};
  if (!r.scope_ask) return [];
  const raw = Array.isArray(r.candidates) ? r.candidates : [];
  return raw.map((x) => String(x)).filter(Boolean);
}

/** The question to put to the customer. The server writes a good one; we use ITS words rather than
 * inventing ours, because it knows why it could not resolve the scope and we do not.
 *
 * E-030's defect was discarding exactly this — "a complete, well-composed question the server had already
 * written" — and rendering a heading with an empty list instead. */
function questionOf(res) {
  const r = res || {};
  return String(r.question || r.reply || "Which repo should I check this against?").split("\n")[0];
}

/** The reason line, or "". Kept separate from the question so the surface can dim it. */
function reasonOf(res) {
  const r = res || {};
  return String(r.unverified_reason || r.reason || "");
}

/** Is this envelope a scope ask we can actually resolve?
 *
 * `scope_ask` with NO candidates is a real state and must NOT open an empty picker: it means the account
 * has nothing swept that could answer, and the honest response is to say so, not to ask a question with
 * no answers. A picker with zero rows is symptom (b) wearing the fix's clothes. */
function isResolvable(res) {
  return candidatesOf(res).length > 0;
}

/**
 * Ask which repo, record the answer, and hand back the scope to retry with.
 *
 * Returns `{repo, transcript}` — `repo` is `""` when the customer cancelled or answered ambiguously, and
 * the caller must then leave the original refusal standing rather than retrying unscoped.
 */
async function resolve(res, io, record) {
  const options = candidatesOf(res);
  const spec = {
    kind: "choice",
    question: questionOf(res),
    detail: reasonOf(res),
    label: "repo",
    options,
  };
  const { result, transcript: t } = await ask.askAndRecord(spec, io, record);
  return { repo: result.ok ? String(result.value) : "", transcript: t };
}

/** The line shown when a scope ask cannot be resolved — cancelled, ambiguous, or no candidates.
 *
 * It names the flag that answers it without a prompt, because a customer in a pipe or a CI job cannot
 * answer a picker and must still have a way through. */
function unresolvedLines(res, c) {
  const lines = [];
  const why = reasonOf(res);
  if (!isResolvable(res)) {
    lines.push(`  ${c.amber("!")} ${c.bold("Nothing swept can answer this.")}`);
    if (why) lines.push(`  ${c.dim(why)}`);
    lines.push(`  ${c.dim("Run ")}${c.teal("estelle sweep")}${c.dim(" in the repo you mean, then try again.")}`);
    return lines;
  }
  lines.push(`  ${c.dim("Nothing was checked — a scope question is not a verdict on your code.")}`);
  lines.push(`  ${c.dim("Answer it inline any time: ")}${c.teal("/verify <file> --repo owner/name")}`);
  return lines;
}

/** The transcript entry for a scope ask that went unanswered, so the record still says what happened. */
function unresolvedEntry(res) {
  return transcript.choice(questionOf(res), "unanswered — nothing was checked");
}

module.exports = { candidatesOf, questionOf, reasonOf, isResolvable, resolve, unresolvedLines, unresolvedEntry };
