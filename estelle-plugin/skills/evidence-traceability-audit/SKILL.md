---
name: evidence-traceability-audit
description: Re-open every citation and check it says what was claimed. Use when the task involves: evidence traceability, traceable to evidence, fabricated citation, unreproducible number, untraceable claim, citation audit.
---

# evidence-traceability-audit

Estelle's evidence-traceability skill, from Google Cloud AI Research's **ScientistOne** (arXiv:2605.26340,
project page scientist-one.github.io) and its Chain-of-Evidence design axiom *"every
claim traceable to evidence."* The mechanism inverts that axiom into an adversarial check: for every claim AND
every citation an answer already carries, RE-RESOLVE it against live recall — a `[source]` header that points at
nothing is a FABRICATED CITATION, and a number no cited source or green run reproduces is an UNREPRODUCIBLE
NUMBER. Any claim that can't be traced back fails the deterministic merge gate. It runs on the real primitives:
`retrieve_cited` (`serve/memory_pipeline.py`) returns `(text, source_file)` pairs; `cite_block` heads each with
its `[source]`; hybrid RRF (`rrf_fuse` / `fuse_ranked_texts`, `serve/hybrid.py`) does the recall; `code_search`
(`serve/memory_facade.py`) re-resolves a citation to an exact file:line with no embedding; `GroundingReport.is_grounded`
(`agent/grounding.py`) checks uncited claims against the real symbol graph; `run_and_repair_suite` /
`parse_suite_output` (`agent/verify_suite.py`) reproduce a claimed number from the repo's own suite; and
`_gate_verdict` (`serve/api.py`, POST /gate) returns `merge: false`. Distinct from `atomic-fact-audit`, which
LABELS each claim Supported/Unsupported/Contradicted and returns a FActScore precision number, and from
`provenance-trail`, which ATTACHES a receipts chain to an answer it already trusts: this skill is the trace-back
that HUNTS fabricated citations and unreproducible numbers and HARD-FAILS the gate on any untraceable claim.

## When to run
Before shipping any answer, report, or PR description that CITES sources or quotes numbers — a benchmark result,
a "per orders.py …" attribution, a metric in a design note. It earns its keep exactly when the draft was written
with confident-looking citations, because a made-up `[source]` reads identically to a real one until you try to
resolve it. Run it on load-bearing, contestable output; skip it on trivia.

## Procedure
1. **Extract every claim and its attached citation.** Split the answer into claims, and for each pull the
   citation it purports to rest on — the `[source]` header `cite_block` emits or an inline "per `<file>`". A claim
   WITH a citation is audited for FABRICATION; a claim WITHOUT one is audited for groundlessness. Pairing each
   claim to its alleged source is what makes the trace-back checkable.
2. **Re-resolve each cited source independently.** For every citation, run FRESH recall keyed on the claim —
   `retrieve_cited` (`serve/memory_pipeline.py`, hybrid dense+sparse RRF via `rrf_fuse` / `fuse_ranked_texts`,
   `serve/hybrid.py`) — and confirm the `(text, source_file)` pair actually exists and actually says what the
   claim attributes to it. A citation whose `source_file` no live recall returns is a FABRICATED CITATION —
   ScientistOne's axiom failing.
3. **Pin the citation to an exact repo location.** Confirm the cited file with deterministic `code_search`
   (`Memory.code_search`, exact file:line, no embedding) so the trace-back is reproducible rather than a fuzzy
   semantic near-match. A `[orders.py]` header that `code_search` cannot locate is untraceable, full stop.
4. **Reproduce every quantitative claim.** A number ("recall@4 90.7% against a 2.4% control", "42 tests pass") is traceable only if it
   can be re-derived: either the cited source literally states it, or it comes from a green run —
   `run_and_repair_suite` / `parse_suite_output` (`agent/verify_suite.py`), green ONLY when the pytest summary
   line truly says so. A number backed by neither is an UNREPRODUCIBLE NUMBER — flag it.
5. 🔴 **A RETRACTION IS NOT DONE UNTIL EVERY COPY IS GONE — grep for the DIGITS, not the sentence.** This
   playbook itself cited a headline retrieval figure that had been retracted as unreproducible a day
   earlier. The retraction was applied where someone was looking (`docs/scorecard.md`, `README.md`) and
   missed the document those numbers were *sourced from* — so **21 files in shipped source went on
   asserting it, 19 of them playbooks like this one.** When you retract a number, search the whole tree for
   the literal digits and fix the SOURCE document first, or the leaves will quietly re-seed it.
5. **Trace decision claims to temporal memory.** A claim resting on a team decision ("we standardized on X") must
   trace to a recorded `Fact` — `facts` (`serve/memory_facade.py`: `{key}`→`history`, `{at}`→as-of) — with its
   validity window. A decision claim with no `Fact` behind it, or one citing a superseded value, is untraceable.
6. **Check every uncited claim against the real symbol graph.** For a claim carrying no citation, run the
   grounding gate — `check_grounding` / `GroundingReport.is_grounded` (`agent/grounding.py`) — so an assertion
   referencing a repo symbol still has to name one the repo actually defines. Ungrounded here counts as untraceable.
7. **Fail the gate on any untraceable claim.** Emit each fabricated citation, unreproducible number, and
   groundless claim as an error-severity finding so `_gate_verdict` (`serve/api.py`, POST /gate) returns
   `merge: false` — the same hard block a leaked secret or a hallucinated API gets, never an advisory a human
   waves through. Traceable-but-weak items surface as warnings.
8. **Bank every fabrication caught.** Record the fabricated citations and unreproducible numbers as lessons —
   `learn_from_grounding` / `ExperienceStore.record` (`agent/reflect.py` + `agent/experience.py`) keyed on the
   prompt — so a recurring made-up source is recalled and pre-empted next time instead of re-audited from scratch.

## Output
A traceability audit: each claim tagged TRACEABLE (with the re-resolved `[source]` + file:line, or the
green-run / `Fact` that reproduces its number) or UNTRACEABLE (fabricated citation / unreproducible number /
groundless), plus a merge-gate block (`merge: false`) whenever any claim is untraceable. Where `atomic-fact-audit`
returns a precision score and `provenance-trail` attaches a receipts chain to a trusted answer, this NAMES the
fabrications and hard-fails on them.
