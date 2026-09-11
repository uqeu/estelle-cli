---
name: program-repair
description: Localise, write several patches, and keep the one that holds. Use when the task involves: automated program repair, fault localization, majority vote patch, repair, localize, patch generation.
---

# program-repair

Estelle's automated program-repair pipeline. This is *not* the interactive, human-driven debugging of `bug-hunt` or
`root-cause-loop` — here a red test suite drives a machine loop: localize the fault with the whole-repo `CodeGraph`, fan N
candidate patches out across isolated worktrees (`run_swarm`), gate each one against compile + the real pytest summary
(`run_and_repair_suite` / `parse_suite_output`) + the security scan (`scan_diff`) + the grounding gate, and select the winner
by majority vote (`majority` / `select_best`). Grounded in Agentless's 3-phase localize→repair→validate (2407.01489),
AutoCodeRover's spectrum+AST localization (2404.05427), and Meta's SapFix.

## When to run
When the acceptance signal is *executable* — a red suite with a known failing test, an incoming CVE patch, or a mechanical
fix you'd rather not babysit. The loop only works when "did it work?" can be answered by running tests, so it can select a
patch without a human in the loop. If there is no failing test to localize from and no green condition to validate against,
use `root-cause-loop` first to establish one.

## Procedure
1. **Localize by spectrum first.** Start from the failing test node ids in `parse_suite_output(...).failures` (`agent/verify_suite.py`)
   and map each failing test to the files it exercises — the code the red tests actually touch is the first suspect set.
2. **Narrow by AST and the code graph.** Resolve the symbols named in the traceback with the `CodeGraph`
   (`conveyor/code_graph.py`): `definition_sites` / `files_defining` to find where they live, `references` / `blast_radius` for
   what a change would ripple to, and `central_files` / `betweenness_centrality` to rank the suspects. The union, ranked, is the
   repair target.
3. **Generate N candidate patches in parallel.** Fan the localized task out with `run_swarm` (`serve/swarm.py`) — one *isolated
   worktree per candidate* at PROPOSE+ autonomy — routing the work through `route(kind="repair")` → the FRONTIER tier. Diversify
   candidates (temperature) so the vote has real alternatives, not one answer echoed N times.
4. **Apply each candidate to its own sandbox.** Turn every model reply into bytes on disk with `extract_code_block`
   (`agent/patching.py`), each in its own worktree, so the repo's real tests can run against it without candidates colliding.
5. **Gate on compile + the failing test.** Run `run_and_repair_suite` / `parse_suite_output`: a run is GREEN *only* when the
   pytest summary line says passed and reports no failed/error — a passing coverage %, a clean import, or a partial run never
   count. Discard any candidate whose targeted test still fails.
6. **Gate on regression + safety.** Re-run the *whole* suite (no new red allowed), then run `scan_diff` (secret + SAST + OSV
   dependency-CVE) and the grounding gate (no invented APIs). A candidate that fixes the target test but breaks another test or
   introduces a vulnerability is rejected — this is exactly the composition `_gate_verdict` (`serve/api.py`) uses for merge.
7. **Select by majority vote.** Among the surviving patches, `majority` / `self_consistency` (`agent/candidates.py`) picks the
   convergent diff (independent candidates arriving at the same fix are more trustworthy than any single one); break ties toward
   the grounded candidate with `select_best` — fewest ungrounded APIs wins.
8. **Record the run and gate for merge.** Persist the repair (run history + `remember_fact`) and the caught mistake
   (`ExperienceStore.record`) so recall improves on this codebase, then hand the winning patch to `verify-gate` before the PR.

## Output
A single validated patch — localized, compiled, target-test green, regression-clean, security-scanned, and majority-selected —
alongside the rejected candidates and the exact gate each failed. Fully automated; it escalates to `bug-hunt` only when *no*
candidate survives the gates.
