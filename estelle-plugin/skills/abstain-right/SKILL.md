---
name: abstain-right
description: Say "I don't know" instead of guessing, and ask for what's missing. Use when the task involves: abstain, refuse to guess, guess, fabricate, fabricated answer, no citation.
---

# abstain-right

Every frontier model is trained to always produce an answer — its own creator denies it the option to say
"I can't tell." Estelle gives that option back, and makes it enforceable: the clarify stage
(`needs_clarification`, the `config.clarify` step in `best_answer`) and the deterministic grounding gate
(`GroundingReport.is_grounded`, served as POST /verify → `{grounded, ungrounded, arity_errors}`) decide
answer-or-abstain from ground truth, not from the model's felt confidence. This is Estelle's founding
thesis — it exists to stop the confident hallucination that would otherwise pass review.

## When to run
Before writing any answer that asserts a fact about the repo, an API, or a team decision — i.e. any claim
that could be wrong. Run it hardest when recall came back thin, when the request names a file or function
that could be one of several, or when you feel the pull to fill a gap from training memory. If the claim
is fully grounded, this costs one cheap check and you answer as normal.

## Procedure
1. **Recall before you reason.** Pull the repo/team memory with `retrieve_cited` (hybrid RRF, recall@4
   90.7%, control 2.4%) — these are the sources you would cite. If recall returns nothing on the load-bearing claim,
   treat that as an early abstain signal, not an invitation to improvise.
2. **Run the clarify check first.** Before spending tokens on an answer, `needs_clarification` grounds the
   request in that recalled context and asks: is the target file/function ambiguous, a behavior
   unspecified, or are there multiple plausible readings? If yes, return ONE specific question and stop —
   asking beats guessing.
3. **Draft only what the context supports.** Write against the recalled sources, using only symbols and
   signatures that appear there. Do not reach past the context to complete a pattern from memory.
4. **Submit the draft to the grounding gate.** `ground_report_and_repair` → `GroundingReport` checks every
   repo symbol you referenced against the real symbol graph (`ungrounded`, `arity_errors`, `third_party`).
   `is_grounded` is the deterministic verdict — POST /verify returns the same `{grounded, ungrounded, …}`.
5. **Read the verdict, not your gut.** If `is_grounded` is False the gate names the invented symbols and
   wrong-arity calls. Your subjective confidence is irrelevant here; the compiler-accurate check is truth.
6. **Choose the honest exit.** If the gap is ambiguity → ask the clarifying question from step 2. If the
   gap is a missing fact or a symbol the repo simply doesn't define (and can't be repaired from context)
   → abstain: say "I don't know" or "I need X first", naming exactly what is missing to answer correctly.
7. **Never fabricate to satisfy the gate.** Inventing a plausible-looking symbol to make the answer read
   complete is the exact failure Estelle exists to block. The right to abstain is also the requirement:
   an ungrounded claim must become a question or an admission, never a guess dressed as an answer.
8. **Bank the catch.** When the gate rejected an invented symbol, `learn_from_grounding` records it as a
   lesson keyed on this prompt, so the next similar ask recalls it and you don't reinvent it.

## Output
Either a grounded answer with its cited sources attached, OR an explicit abstention ("I don't know — I
need X to answer this") or a single specific clarifying question. Never a confident guess: every claim
that ships has passed the gate, and every claim that couldn't has been surfaced as a gap instead of hidden.
