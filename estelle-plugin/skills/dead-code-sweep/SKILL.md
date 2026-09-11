---
name: dead-code-sweep
description: Find what nothing uses, and remove it safely. Use when the task involves: dead code, cleanup, unused, duplicate, refactor, remove.
---

# dead-code-sweep

Estelle's cleanup skill. Less code is less to break.

## Procedure
1. Detect unreachable/unused code (knip/depcheck/ts-prune for JS, vulture/coverage for Python) + duplicates.
2. Confirm each candidate is truly dead — grep for dynamic/reflective uses before deleting.
3. Remove in small, independently-revertable commits; run the suite after each.
4. Fold duplicates into one home (the shared-primitive pattern) rather than deleting blindly.

## Output
A smaller, greener codebase + the list of what was removed and why. Serves maintenance + onboarding clarity.
