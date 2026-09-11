---
name: bug-hunt
description: Reproduce, shrink, instrument, fix — then lock it with a test. Use when the task involves: bug, debug, diagnose, crash, incident, repro.
---

# bug-hunt

Estelle's debugging discipline. No guess-and-check; a loop that ends with a test that would have caught it.

## Procedure
1. **Reproduce** deterministically — the smallest input/state that triggers it.
2. **Minimize** — strip everything that isn't required to reproduce.
3. **Hypothesize** the root cause (not the symptom); rank hypotheses by likelihood.
4. **Instrument** — add the logging/asserts that confirm or kill the top hypothesis before changing anything.
5. **Fix the root cause**, grounded against the repo (no invented APIs).
6. **Regress** — write the failing test first, watch it go green, keep it. Run `verify-gate`.

## Output
The root cause, the minimal fix, and a regression test that locks it. Serves incidents + flaky failures.
