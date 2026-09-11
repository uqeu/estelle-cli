---
name: commit-guard
description: Format, lint, typecheck and test before a commit lands. Use when the task involves: commit, pre-commit, hook, staged, format, lint.
---

# commit-guard

Estelle's commit-time hygiene gate. Cheap, fast, and local — the tree is green before the commit lands, not after CI catches it.

## Procedure
1. **Scope to staged** — diff `--cached` to get exactly the files entering the commit; run everything against that set, not the whole repo.
2. **Format** — run the formatter on staged files and re-stage the results; a formatting-only diff never reaches review.
3. **Lint** — run the linter on the staged set; block on errors, surface warnings.
4. **Typecheck** — run the type/static checker; a type error is a hard block.
5. **Affected tests** — map changed files to their tests and run only those (fast path); on any failure, abort the commit with the failing line.
6. **Block or pass** — any red step stops the commit and reports the single reason; all green lets it through untouched.

## Output
A pass/fail verdict at commit time with the one check that failed. Serves everyday hygiene; hand off to `verify-gate` for the deeper pre-PR gate.
