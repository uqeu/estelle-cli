---
name: mutation-test
description: Break the code on purpose and see if your tests notice. Use when the task involves: mutation testing, kill mutants, test effectiveness, mutmut, pitest, unasserted.
---

# mutation-test

A state-of-the-art engineering playbook, drawn from Google 'State of Mutation Testing at Google' / PIT, that an agent grounding in a real repo can execute. Every referenced symbol is checked by Estelle's grounding gate like any other code.

## When to run
Before trusting a green suite / high coverage as real safety — especially on critical logic where a passing-but-vacuous test would hide a bug.

## Procedure
1. Run a mutation tool (mutmut/cosmic-ray for Python) on the target module: it flips operators, boundaries, and returns.
2. A SURVIVING mutant = a fault the suite didn't catch = a missing assertion. Rank survivors by how central the mutated line is.
3. Write the assertion that kills each high-value survivor; ignore equivalent mutants.
4. Report the mutation score alongside coverage — coverage says 'executed', mutation says 'checked'.

## Grounding note
Apply this to the caller's ACTUAL code (the code graph + real symbols), not a generic template — the value is a concrete, verifiable change on their repo, not advice.
