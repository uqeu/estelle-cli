---
name: verify-gate
description: The last gate — build, lint, coverage, security, before shipping. Use when the task involves: verify, gate, release, ci, ready, done.
---

# verify-gate

Estelle's "is this actually done?" gate. Nothing merges unverified.

## Checklist (all must pass)
1. **Build** green (compiles / imports).
2. **Lint + format** clean (ruff / prettier).
3. **Static/type** checks pass.
4. **Coverage** ≥ target (Estelle: 100% on new pure cores) — the test summary line is the source of truth.
5. **Grounding gate**: no invented APIs.
6. **Security** (`threat-scan`): no CRITICAL/HIGH CVE, secret, or SAST finding.
7. **Diff review** (`diff-review`): Standards + Spec axes, no blockers.

## Output
A pass/fail verdict with the one line that failed. Serves every PR; the swarm runs this before opening a PR.
