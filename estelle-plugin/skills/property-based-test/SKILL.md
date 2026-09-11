---
name: property-based-test
description: State what must always be true, then hunt for a counterexample. Use when the task involves: property based, invariant, hypothesis, quickcheck, generative test, shrinking.
---

# property-based-test

A state-of-the-art engineering playbook, drawn from QuickCheck / Hypothesis, that an agent grounding in a real repo can execute. Every referenced symbol is checked by Estelle's grounding gate like any other code.

## When to run
A function has a clear invariant/round-trip/idempotence property (parsers, serializers, sort/merge, math, encoders) where example tests under-cover the input space.

## Procedure
1. Name the invariant precisely: round-trip (`decode(encode(x))==x`), idempotence, commutativity, or a postcondition that must always hold.
2. Write a `@given(strategy)` (Hypothesis) test that asserts the property over generated inputs; pick strategies that hit boundaries (empty, huge, unicode, negative, NaN).
3. Run it; when it fails, keep the SHRUNK minimal counterexample as a permanent regression example test.
4. Ground every referenced symbol in the real repo — the property test is code Estelle's gate verifies like any other.

## Grounding note
Apply this to the caller's ACTUAL code (the code graph + real symbols), not a generic template — the value is a concrete, verifiable change on their repo, not advice.
