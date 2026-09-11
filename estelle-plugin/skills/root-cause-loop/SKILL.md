---
name: root-cause-loop
description: Find why it happens before changing anything. Use when the task involves: debug, root cause, systematic, heisenbug, flaky, investigate.
---

# root-cause-loop

Estelle's systematic-debugging discipline (pairs with `bug-hunt`). The fix addresses the cause, not the symptom.

## Procedure
1. State the observed behavior precisely + the expected behavior.
2. Form competing hypotheses for the *cause*; rank them.
3. Design the cheapest experiment that discriminates between the top hypotheses; run it before touching code.
4. Only once the cause is confirmed, fix it — and prove the fix with a test that fails before / passes after.
5. If the cause is systemic (recurs), fix the class, not the instance.

## Output
A confirmed root cause + a minimal, tested fix. Serves flaky/heisenbugs and recurring failures.
