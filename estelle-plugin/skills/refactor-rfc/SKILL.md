---
name: refactor-rfc
description: Name the smell, the target shape, and the order to get there. Use when the task involves: refactor, rfc, debt, cleanup, restructure, smell.
---

# refactor-rfc

Estelle's debt-paydown planner. Big-bang rewrites are how refactors die; this files a plan of tiny, individually-safe commits that keep the build green the whole way.

## Procedure
1. **Name the smell** — the concrete pain (duplication, tangled module, untestable seam) and the cost of leaving it.
2. **Draw the target shape** — what the code looks like after, grounded against the current repo structure so the endpoint is real.
3. **Sequence tiny commits** — an ordered list where each step is small, reversible, and green on its own; no step leaves the tree broken.
4. **Guard with tests first** — ensure characterization tests cover current behavior before moving code, so refactors stay behavior-preserving.
5. **Order for safety** — front-load the low-risk mechanical steps; isolate the one genuinely risky change and flag it.
6. **File the RFC** — publish to the tracker as an RFC; execute each commit through `verify-gate`.

## Output
An RFC with the smell, the target shape, and an ordered checklist of small safe commits. Serves debt paydown incrementally, without big-bang risk.
