---
name: estelle-loop
description: Take a goal and grind until the gate says it is done. Use when the task involves: self improving loop, autonomous build loop, grind until done, propose gate test repair, self-drive, gated loop.
---

# estelle-loop

Estelle's self-driving build loop — its variation of an agent loop. A plain agent loop calls the model, applies
the change, and hopes; two failure modes follow — an ungrounded change slips through, and the loop repeats the
same mistake because it never learned. Estelle's loop closes both with primitives it already owns: every
iteration runs the deterministic merge gate (`_gate_verdict` — grounding against the real symbol graph + security
scan, every error-severity finding blocks) and the repo's REAL pytest (`run_and_repair_suite` /
`parse_suite_output`), and every iteration banks what it learned (`learn_from_grounding` → `ExperienceStore`, the
`InstinctEngine`, `SkillLearning`). So the loop is the `handle_work` unit-of-work — generate → gate → repair —
lifted into a goal-driven cycle that picks the next target, audits each run into `RunHistory`, and stops on an
autonomy ceiling (`run_autonomy`) or a metering budget (`model_pricing.usage_breakdown` over real provider
counts received through `usage_sink`).

## When to run
When a goal is bigger than one change and you want Estelle to grind it out autonomously — harden a module, raise
coverage, work down a backlog, resolve a batch of findings — rather than hand-holding each edit. Use it when the
work is open-ended enough that "pick the next best thing" is itself part of the job. Do not use it for a single
scoped edit (`self-eval` grades one output) or a fan-out audit by concern (`agent-orchestration`); this is the
sequential, self-improving driver that keeps going.

## Procedure
1. **State the goal and the stopping condition first.** Turn the ask into an explicit goal plus the two bounds
   that stop the loop: an autonomy ceiling (`run_autonomy` over `AutonomyLevel` READ_ONLY→PROPOSE→BRANCH→EXECUTE
   — the loop may only take actions the ceiling permits) and a budget guard read from real metering
   (`model_pricing.usage_breakdown` on actual served-model token counts, never an estimate). The loop ends when the
   goal is met OR a bound trips — never "when the model feels done."
2. **Pick the next highest-value target.** One target per iteration. Use recall + the code graph (`memory.locate`
   / `recall`) to choose the single change with the most leverage remaining toward the goal, and route its
   difficulty with `model_router.route` (`repair`/`agentic` → FRONTIER, small bulk → CHEAP) so flagship tokens go
   to the hard step, not the coverage.
3. **Propose the change, grounded from attempt #1.** Generate via the work prompt (`_work_prompt` in
   `handle_work`) with the RECALL side of the learning loop injected up front — prior lessons
   (`recall_lessons` / `ExperienceStore.as_prompt_block`), the grounded repo context, and the team's learned
   house style (`memory.conventions`) — and output only code. In-convention, in-context generation means the
   gate then only has to catch what slips.
4. **Gate it — no ungrounded change survives.** Run `_gate_verdict` on the change (wrapped as a diff by
   `_as_added_diff`): it returns `{merge, verdict, blockers, warnings}`, where `blockers` are error-severity
   facts from the real symbol graph (invented symbol, wrong arity) plus the security scan. This deterministic
   gate on EVERY iteration is exactly what makes this loop different from a generic agent loop.
5. **Repair against the gate, then against the real suite — and know WHICH limit stopped you.** While
   `gate["blockers"]` and under `max_rounds`,
   feed the gate's exact rejections back (`_repair_prompt`) and regenerate — repairing against ground truth, not
   a vague "don't hallucinate." Then run the repo's OWN tests through `run_and_repair_suite` /
   `parse_suite_output`: the change counts green ONLY when the pytest summary line truly says `passed` with no
   `failed`/`error` and it isn't "no tests ran" (the exact trap that once hid a regression for 8 commits).
   **Both halves feed refusals back and both NAME THEIR EXIT.** `/work`'s gate half reports
   `abandonment()` — `rounds` (used every round allowed) vs `budget` (our clock stopped it) vs `starved`
   (an earlier round ate this one's declared slice). The suite half reports `SuiteVerdict.outcome` —
   `passed_clean` · `passed_repaired` · `rounds` · `not_attempted` — bounded by `DEFAULT_SUITE_ROUNDS`.
   The ORCHESTRA code worker feeds BOTH its evaluators back (gate blockers, then repro failures) on one
   shared `max_repairs` budget, and marks a terminal admission refusal `gate-refused` rather than
   letting it read as a failed test run. 🔴 **`rounds` and `not_attempted` are deliberately different
   values**: *gave up after N attempts* and *was never given an attempt* are different facts about a red
   result, and they used to be the same bytes. Never report a loop as having failed when it was never
   allowed to run.
6. **Bank the lesson — this is why the loop improves.** On a caught hallucination, `learn_from_grounding` writes
   an `ExperienceStore` lesson naming the invented symbols (deduped, and expirable once those symbols become
   real) so the next iteration recalls it. Feed the outcome to `SkillLearning.record` so `blended` selection
   biases toward what worked here, and to the `InstinctEngine` (`observe`) — a reflex that keeps being confirmed
   graduates at `GRADUATE_CONFIDENCE` over `GRADUATE_MIN_OBSERVATIONS` into a first-class skill the loop can then
   select. Lessons compound per-namespace with no weights touched.
7. **Audit the iteration into team-shared history.** Fold each iteration's `AgentRun` into `RunHistory.record`
   (rendered by `runs_to_markdown`, capped, snapshot-persisted) so every member's console sees what ran, what
   the ceiling refused, and the artifacts. When several targets are independent and the ceiling allows writes,
   fan them out with `run_swarm` — each in its OWN isolated gated worktree — instead of serializing.
8. **Loop until the goal or a ceiling stops it, and say WHICH.** Re-evaluate the goal against the banked state; if it's unmet
   and neither the autonomy ceiling nor the budget has tripped, return to step 2 on what's left. Because each
   pass is gated (no bad change survives) and smarter (lessons + instincts + skill scores carry forward), the
   loop converges on the goal instead of drifting — and halts honestly when a bound says stop.

## Output
A goal worked end-to-end by a self-driving loop: a sequence of gated, test-passed, repaired changes; a growing
per-namespace store of lessons, graduated instincts, and skill scores that made each iteration better than the
last; a team-shared `RunHistory` fleet report of every iteration and every action the ceiling refused; and an
honest terminal state — goal met, or halted by the autonomy ceiling or the metering budget — never a claim of
"done" that a change couldn't survive the gate to earn.
