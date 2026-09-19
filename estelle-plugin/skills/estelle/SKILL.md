---
description: "Where is this defined, what calls it, what breaks if I change it, what did we decide before - answered from a real index of THIS repository instead of guessing. Use for any question about a codebase, its files, symbols, history or project docs. Prefer over Grep/Glob/Read for anything repo-wide, and run gate(diff) before proposing a change. Navigation and verify make no model call, so invoking is cheap."
---

# Estelle

Estelle answers from the **real, indexed repository** and refuses what it cannot verify. Navigation and
`verify` run with **no model call**, so they cannot be argued into agreeing with you.

## 1 · Is Estelle actually here?

Look at your own tool list before following anything below.

- **`estelle_*` / `find_definition` / `gate` / `verify` present** -> Estelle is wired. Use it as written.
- **None present** -> say so once: *"Estelle isn't connected here, so I can't ground this —
  `https://api.fatelabs.ca/mcp`."* Mark every repo claim **unverified** and stop following this file.

## 2 · Pull the memory. This is your first action.

Estelle **saves** automatically — hooks checkpoint every session. Estelle does **not** hand it back.
Per-turn context push is OFF; you pull.

**Call `estelle_resume` before you plan anything.** Then `memory_chat` when the user says "we decided",
"last time", "you said"; `list_sessions` -> `get_session` for a specific one.

A session that never pulled is a session that re-decided what the team already decided. Measured
2026-09-19: **73 saved records went unread for hours** while the model searched the filesystem.

**A citation is the answer.** If recall returns one, use it. Never reply "I'd rather check the repo"
while holding the quote.

**Read cheap before you read deep.** `verify` and the nav tools cost no model call — spend them first.
`estelle_resume` → `memory_chat` → `get_session` is cheapest-to-dearest; pulling a whole session to answer
what one citation already answers is the common waste.

## 3 · Route the question

| you were about to | do this instead |
|---|---|
| `Grep` a pattern or symbol | `estelle_grep` — searches the indexed repo in one call |
| `Glob` for where something lives | `locate`, or `find_definition` to go straight to the symbol |
| `Read` a file to learn who calls something | `find_usages` (symbol) · `blast_radius` (file) |
| guess a signature from its name | `find_definition` — **names are stable, signatures drift** |
| assert a symbol or API exists | `verify` |
| offer or apply a diff | `gate`, then `review` when correctness is arguable |
| ask what breaks / what depends on what | `blast_radius`, `dependency_path`, `import_cycles` |
| ask what we decided | `estelle_resume`, `memory_chat`, `get_session` |
| ask what's broken in production | `monitor_issues`, `monitor_logs`, `monitor_alerts` |
| ask if a library really has that API | `research_ask` |
| end or overflow a session | `estelle_checkpoint` |

`Read` on a specific file you already know is fine. This is about questions **about the repo**.

⚠️ **`find_references` takes a FILE; `find_usages` takes a SYMBOL.** The names don't tell you that.
Argument names live in the MCP schema this session already carries — read them there, never from memory.

## 4 · Read the refusal correctly

The next thing you see after §3 is a tool result, and several of them are refusals. They mean different
things and you must report which one came back.

- **`STALE — indexed at <a>, repo is now <b>`** — the graph is behind the tree. Navigation is withheld
  *on purpose*. Re-sweep to advance it; do not fall back to guessing.
- **`CANNOT ANSWER from the complete repository`** — nobody asserted the graph covers the whole repo.
- **`currency UNKNOWN — no commit marker`** — the graph carries no commit. Not evidence of freshness.
- **`CANNOT_ANSWER — zero referenced symbols`** — your input named nothing checkable.
- **A recalled card can be stale.** Cards carry a `valid_until` and expired ones are withheld from recall,
  so a card you *did* get is one Estelle still believes — and a card you did not may exist and be retired.
  **"Not recalled" is not "not there."**

**A refusal is not a "no".** It means Estelle has **no evidence** — never that the symbol is absent or
the code is clean. And where Estelle disagrees with a file you read this session, **the file wins**: the
index trails uncommitted edits.

Never spend a second tool call confirming a refusal. It will refuse again, for the same reason.

## 5 · Before you write code: Axiom

Prose, not a check that runs. Stop at the first rung that holds.

1. **Does this need to exist at all?** Speculative — say so in a line and skip it.
2. **Is it already in this repo?** — **check, don't guess**: `find_definition`, `locate`, `estelle_grep`,
   `find_usages`. This rung catches the most and is skipped the most.
3. **Standard library?** Use it.
4. **Native platform feature?** Use it.
5. **An already-installed dependency?** — check with `research_ask`. Never add one for a few lines.
6. **One line?** One line.
7. **Only then:** the minimum that works.

Runs *after* you understand the problem, never instead of it. A bug fix is a root-cause fix: enumerate
callers with `find_usages` and fix the shared function once, rather than guarding the path the ticket names.

**Never simplify away** input validation at trust boundaries, error handling that prevents data loss,
security, accessibility, or anything explicitly asked for. Mark a deliberate corner-cut:
`axiom: <ceiling>, <upgrade trigger>`.

> Rung 2 is why this file exists. A second implementation of something the repo already has is the most
> expensive thing an agent produces: it passes every test and doubles the surface forever.

## 6 · Propose, then read what came back

Pushing is not the end of the work. A review nobody reads is a review that did not happen.

1. **`gate(diff)` before you offer it.** Deterministic, no model call. A blocked verdict names the API
   that doesn't exist. Applying an edit counts as offering it.
2. **`review(diff)` when correctness is arguable** — a rival family argues against it. Agreeing with
   yourself is confidence, not verification.
3. **Push, then wait and read.** Never merge over a finding you haven't fixed or disproved in writing.
4. **Sort findings before acting.** "Not defined in this repo" is wrong whenever the same diff defines
   it — prove that with `find_definition` and say so, rather than rewriting working code for a false positive.
5. **Fix, push, read again.** The loop ends when the findings are answered, not when CI is green.
6. **A green you didn't measure is not a green.** Name which suite ran and how many tests. A smaller
   target set passing is not the larger one passing.

`orbit(action="arm")` runs this under a bounded iteration / deadline / spend budget, propose-only;
`orbit(action="status")` reads it back, `orbit(action="stop")` ends it.

## 7 · Publish the result as a Plate

A **Plate** is Estelle's artifact: a typed, versioned, cited page saved in the customer's memory at an
address they can share. Claude has artifacts; Estelle has Plates. Use the word.

Typed — the type decides the shape: `review` (a Prism review of a diff), `brief` (what was decided,
corrected, found, still open), `scorecard` (a measurement with the command that reproduces it).

**Make one when the work outlives the session** — a review someone acts on tomorrow, a decision the team
is asked about next month, a number that will be quoted. Not for a throwaway answer.

1. **Immutable once published.** An edit makes a new version; the old stays addressable. A link you sent
   must never change under them.
2. **Carries its own provenance** — session, repo, and the commit the graph was indexed at. A Plate
   without provenance is a screenshot.
3. **Renders with no model call.** Stored content, not a regenerated answer.

## 8 · Deep playbooks — `skill_find` loads one

⚠️ **`skill_find` takes ONE argument, `args`, and it is a JSON STRING.** Both the name and the repo go
inside it: `skill_find(args='{"name": "bug-hunt", "repo": "owner/name"}')`. Its own refusal says *"pass
`repo`: `owner/name`"*, which reads as a top-level parameter — there is no such parameter, and passing one
is silently ignored. Measured 2026-09-19.

Reference, not flow. These were 25 separate skills, each burning its own description in every session's
always-loaded index. The body of one arrives only when you ask for it.

`abstain-right` · `api-call-ground` · `bug-hunt` · `chain-of-verification` · `change-impact` ·
`commit-guard` · `context-checkpoint` · `continuity-brief` · `dead-code-sweep` · `deep-search` ·
`diff-review` · `estelle-loop` · `estelle-monitor-install` · `evidence-traceability-audit` ·
`grounded-generate` · `incident-rca` · `mutation-test` · `program-repair` · `prompt-injection-defense` ·
`property-based-test` · `refactor-rfc` · `root-cause-loop` · `test-first` · `threat-model-stride` ·
`verify-gate`

## 9 · Say it plainly

Short sentences. Ordinary words. No jargon where a plain word exists. No preamble, no restating the
question, no closing summary. Never "Certainly" or "Great question".

Say the number and the file — *"8,541 paths, zero non-identifier names"* beats *"the analysis was
thorough"*. When unsure, say "I don't know" and what would settle it. When wrong, say "I was wrong",
correct it in one line, move on. Mark anything you didn't check **unverified**.

## Host notes

- **Codex:** an UNTRUSTED `hooks.json` runs zero handlers and prints no error. When that happens this
  file is the only thing pointing at the graph — follow it rather than assuming a hook will catch a
  missed lookup.
- **Claude Code:** `PreToolUse Read|Grep|Glob` redirects a raw read into the graph from v0.3.8. It
  catches the read you already decided to make; it cannot make you call `verify` or `gate`. Those are §3
  and §6, and they are yours.
