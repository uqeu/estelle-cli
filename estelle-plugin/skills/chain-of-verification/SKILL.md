---
name: chain-of-verification
description: Ask the questions that would prove a draft answer wrong. Use when the task involves: chain of verification, cove, verification questions, independent recall, strike unsupported clauses, draft then verify.
---

# chain-of-verification

Estelle's Chain-of-Verification skill (Dhuliawala et al., Meta AI, arXiv:2309.11495). A model asked to re-read its
own draft tends to re-justify its own mistakes — the error is already in context, so it reads as fact. CoVe breaks
that loop by *decomposing* the draft into targeted verification questions and answering each one from FRESH,
draft-independent recall — `retrieve_cited` (`serve/memory_pipeline.py`, hybrid RRF) keyed on the QUESTION, not
the draft — then revising the answer to keep only what the independent evidence supports and proving the result
through the verify→test→repair loop (`ground_report_and_repair`, `agent/grounding.py`; `run_and_repair_suite` /
`parse_suite_output`, `agent/verify_suite.py`). Distinct from `self-eval` and `verify-gate`: those check a
finished answer against ground truth once — this is the question-generation-and-decontamination mechanism that
*repairs* the draft before either gate sees it.

## When to run
After drafting a multi-claim answer or change but before it reaches the release gate — especially when the draft
was written from the model's own reasoning and you suspect it carried a confident error through. CoVe's edge is
exactly the case where a single re-read won't help, because the model would only re-derive the same mistake from
the same contaminated context.

## Procedure
1. **Hold the draft as a baseline, not a conclusion.** Take the drafted answer as a hypothesis to be
   interrogated — the CoVe baseline. It is the thing verification will revise, never the thing it sets out to
   defend.
2. **Generate verification questions.** Ask the model (an injected `ask`, the same shape `best_of_n` uses in
   `agent/candidates.py`) to plan a short list of targeted questions — one per checkable claim in the draft
   ("does `retrieve_cited` return source pairs?", "is FRONTIER the default tier?"). Each question must isolate
   exactly one claim; a question that bundles two claims can't be cleanly answered.
3. **Answer each question with FRESH, INDEPENDENT recall.** The crux: answer every verification question by NEW
   retrieval keyed on the QUESTION, not the draft — `retrieve_cited` with the question as the query, hybrid RRF
   (`fuse_ranked_texts`, `serve/hybrid.py`), cited via `cite_block`. Because the draft never conditions the
   retrieval, the same confabulation cannot re-surface to justify itself (CoVe's factored / decontaminated
   variant). This independence is exactly what a `self-eval` re-read of the finished answer cannot give you.
4. **Compare each draft claim to its independent answer.** Supported = the fresh answer agrees; Unsupported = it
   can't confirm; Contradicted = it disagrees. The verdict comes from the decontaminated recall, not from
   re-reading the draft in place — so a claim the draft stated confidently but the fresh recall won't back is
   caught here.
5. **Revise: strike or rewrite the unsupported clauses.** Produce the final answer keeping ONLY what the
   independent verification supports — unsupported clauses struck, contradicted ones corrected to match the cited
   evidence. The revision is the CoVe output; the draft's unverified confidence does not survive into it.
6. **Prove the revised answer through the verify→test→repair loop.** Now that it's decontaminated, verify it:
   `ground_report_and_repair` checks every symbol against the real `CodeGraph`, then `run_and_repair_suite` /
   `parse_suite_output` runs the repo's OWN pytest — green ONLY when the summary line truly says so — repairing
   while red up to the round budget. Question-generation decontaminates; the loop confirms.
7. **Bank the caught contradictions.** Each struck or corrected clause is a real caught mistake:
   `learn_from_grounding` / `ExperienceStore.record` (`agent/reflect.py` + `agent/experience.py`) records it
   keyed on the prompt, so the next similar draft recalls the correction instead of re-making it.
8. **Emit the revised answer with its verification trail.** Ship the revised answer alongside the
   question → independent-answer → verdict trail, each independent answer carrying its `[source]` citation — the
   audit of what was checked, what was struck, and why.

## Output
A revised answer with every unsupported clause struck and every contradicted one corrected against fresh,
draft-independent recall — plus the verification-question → independent-answer → verdict trail that produced it
(each cited to a repo location) and the whole revision re-proven through the grounding + real-suite loop. Where
`verify-gate` is the end-of-work checklist and `self-eval` is a whole-answer pass/fail, this is the CoVe
question-generation-and-decontamination mechanism that repairs the draft before either gate sees it.
