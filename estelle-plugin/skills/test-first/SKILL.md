---
name: test-first
description: One test, one thin slice, then the next. Use when the task involves: tdd, test, test-first, red green, coverage, unit test.
---

# test-first

Estelle's TDD skill. Quality by construction, not inspection.

## Procedure
1. **Slice** the work into the thinnest vertical tracer-bullet (one behavior end to end).
2. **Red** — write the test for that slice first; run it; watch it fail for the right reason.
3. **Green** — the minimal implementation to pass. Test behavior through the *public* interface, never internals.
4. **Refactor** — improve with the test as a safety net; deepen the module (see `deepen-architecture`).
5. Repeat slice by slice; keep coverage ≥ 80% (Estelle aims 100% on new pure cores).

## Output
Working code with a test per behavior, coverage proven by the pytest summary line. Serves every feature/bugfix.
