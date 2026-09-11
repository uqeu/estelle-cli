---
name: diff-review
description: Review a change against both the house style and what was asked. Use when the task involves: review, pr, code review, diff, standards, spec.
---

# diff-review

Estelle's PR-review skill. Two independent axes so nothing slips through.

## Procedure
1. **Standards axis** — does the diff follow the repo's learned conventions (naming, types, docstrings, errors,
   tests, immutability)? Use the convention profile.
2. **Spec axis** — does it actually do what the originating issue/PRD asked? Check against the spec, not vibes.
3. **Gate axis** — run the deterministic grounding gate (no invented APIs), the security scan (secrets / CVE /
   SAST — see `threat-scan`), and license/provenance. Place every finding as an inline comment on its exact line.
4. Summarize: blockers (CRITICAL/HIGH) vs suggestions (MEDIUM/LOW); approve only when no blockers remain.

## Output
An inline review comment stream + a merge verdict. Serves every PR; the swarm's reviewer agents run this.
