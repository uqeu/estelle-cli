---
name: grounded-generate
description: Write several answers, keep the one that passes its own tests. Use when the task involves: best of n, dual-execution consensus, package hallucination, slopsquatting, grounded generation, execution filter.
---

# grounded-generate

Estelle's anti-hallucination generator — dead center of the thesis. Where `verify-gate` checks one *finished* output, this
samples K candidate solutions *and* K self-tests, executes them against each other, keeps only the survivors, and selects by
consensus — then proves every imported package is real. It's built from `generate_candidates` / `self_consistency`
(`agent/candidates.py`), `propose_tests` (`agent/testgen.py`), the real-suite filter `run_and_repair_suite` /
`parse_suite_output`, the reproducible patch critic (`agent/critic.py`), and the two grounding gates — repo-scoped
(`GroundingReport.is_grounded`) and library-scoped `third_party_violations` (`agent/third_party.py`). Grounded in CodeT's
dual-execution consensus (2207.10397), the AlphaCodium flow (2401.08500), and the package-hallucination / slopsquatting
work (2406.10279).

## When to run
When generating code you'll actually run — a function, a script, a fix — where a single greedy sample might invent an API or
`import` a package that doesn't exist. Reach for it especially when the answer pulls in third-party libraries: a hallucinated
package name is the exact supply-chain surface attackers pre-register (slopsquatting), so an unverified import is a security
risk, not just a bug.

## Procedure
1. **Sample K candidate solutions.** `generate_candidates(ask, prompt, K)` (`agent/candidates.py`), varying temperature for real
   diversity and routing the work through `route()` (`serve/model_router.py`). One greedy sample is a guess; K gives consensus
   something to measure.
2. **Sample K self-tests.** `propose_tests` / `build_testgen_prompt` (`agent/testgen.py`) writes tests for the ADDED code
   (`added_code`), grounded against real repo symbols so the tests themselves can't invent helpers.
3. **Execute the K×K grid.** Run each candidate solution against each self-test in a sandbox; `parse_suite_output` decides a
   pair "agrees" *only* when the pytest summary is truly green (no failed/error) — a clean import or partial run never counts.
4. **Execution-filter both sides.** Drop every candidate that passes no test and every test no candidate passes — a bad test is
   as misleading as a bad solution, so CodeT filters both. Only mutually-passing survivors move forward; if nothing survives,
   the request is unanswerable as posed, not silently answered.
5. **Select by dual-execution consensus.** The candidate that agrees with the most tests and clusters with the most peers wins:
   `majority` / `self_consistency` for convergent answers, with `select_best` breaking ties toward the grounded candidate
   (fewest ungrounded APIs). Consensus over execution, not an unverified judge's opinion.
6. **Gate the repo APIs.** Run the deterministic grounding gate (`GroundingReport.is_grounded`) on the winner — every
   repo-scoped symbol it references must actually be defined in this codebase; a hallucinated repo API is rejected outright.
7. **Verify every imported package exists (anti-slopsquatting).** Run `third_party_violations` (`agent/third_party.py`) over the
   allowlisted, actually-installed libraries: a `requests.fetch()` that isn't a real method, or an `import` of a package that
   isn't installed, is flagged. Never ship an answer that imports a nonexistent package — that is the slopsquatting attack, not
   a typo.
8. **Record the catch and gate.** Any hallucination caught → `learn_from_grounding` / `ExperienceStore.record` (`agent/reflect.py`)
   so the next similar ask recalls the lesson, then hand the survivor to `verify-gate` before it's called done.

## Output
A single code answer that survived execution against its own tests, won dual-execution consensus, references only repo APIs
that exist, and imports only packages that are actually installed — an answer that *runs*, not one that merely reads well.
Distinct from `verify-gate` (which checks one output) by sampling K, execution-filtering, and package-existence-checking before
a winner is ever chosen.
