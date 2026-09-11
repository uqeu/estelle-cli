---
name: threat-model-stride
description: Walk the data flow and ask how each step gets abused. Use when the task involves: threat model, stride, attack surface, trust boundary, spoofing tampering, mitigation.
---

# threat-model-stride

A state-of-the-art engineering playbook, drawn from Microsoft SDL / Shostack, that an agent grounding in a real repo can execute. Every referenced symbol is checked by Estelle's grounding gate like any other code.

## When to run
Designing or reviewing any feature that crosses a trust boundary — auth, input handling, external calls, new endpoints, file/DB access.

## Procedure
1. Draw the data-flow: external entities → processes → data stores, marking every trust boundary crossing.
2. At each boundary enumerate STRIDE threats (Spoofing→authn, Tampering→integrity, Repudiation→audit, Info-disclosure→encryption/authz, DoS→rate-limit, Elevation→least-privilege).
3. For each real threat name a concrete mitigation that must exist in the diff (a check, a gate, a limit).
4. Ground entrypoints/sinks in the real code graph so the model isn't a paper exercise.

## Grounding note
Apply this to the caller's ACTUAL code (the code graph + real symbols), not a generic template — the value is a concrete, verifiable change on their repo, not advice.
