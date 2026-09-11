---
name: continuity-brief
description: Start a session knowing what the last one did and decided. Use when the task involves: session start, what did i do last, wake up, continuity, resume, brief.
---

# continuity-brief

Estelle's session-start brief — continuity of self that a model, reset to amnesia every session, is denied by
its own design. Estelle keeps the memory the model can't: `save_context` stashed the last session's state under
a label, `RunHistory` recorded every action, the `TemporalLog` holds the team's live decisions, `PresenceLog`
knows who worked while you were away, and the `ExperienceStore` holds what you learned. This is the exact mirror
of `baton-pass` and `context-checkpoint`: they SAVE the handoff at session END; continuity-brief LOADS and
synthesizes it at session START. The model wakes up with a memory instead of a blank slate.

## When to run
At the very start of a session on a repo you (or a sibling agent) have worked before — before the first real
task. Run it to answer "who am I on this repo, what did I do last, what's pending, what changed while I was
away, and what have I already learned not to repeat?" — so the first action is informed, not a rediscovery.

## Procedure
1. **Load the last handoff bundle.** `list_contexts` for the saved labels, then `load_context(label)`
   (POST /context/load) to pull the exact context bundle `baton-pass` / `context-checkpoint` wrote at the last
   session's end — its done / in-progress / next / key-decisions, verbatim. If no label fits, `search_contexts`
   by the current task to find the relevant handoff by meaning.
2. **Read what you actually did last.** `RunHistory.recent(limit)` returns the newest-first agent runs — which
   branches, what was `taken` versus `refused` by the ceiling, the artifacts (diffs / PRs) produced;
   `runs_to_markdown` renders them team-readably. This is a record of your own actions, not a guess about them.
3. **Establish who you are on THIS repo.** Query the team's live decisions: `facts()` with neither `key` nor
   `at` returns `TemporalLog.current()` — every still-open decision you must not contradict. Recall the swept
   surface and conventions so you write in-house from the first line, not after a rejected round.
4. **Measure how much time passed.** Read the clock and `presence.report(now)`: `overnight()` tells you someone
   (or an agent) worked the night window while you were away, `files_in_use` flags the files other members are
   touching right now so a fresh start doesn't collide, and `handoffs` surfaces the questions left for you.
5. **Reload what you learned.** `ExperienceStore.recall(task)` pulls the Reflexion lessons from past failures on
   this repo — the mistakes not to repeat — plus the graduated `InstinctEngine` reflexes the team has already
   proven, so the session opens with hard-won context, not just facts.
6. **Synthesize ONE brief, not five dumps.** Fuse the bundle + run history + current facts + presence + lessons
   into a single short waking brief: who you are on this repo, what you did last, what's pending (the pinned
   resume point), what changed while you were away, and the decisions/lessons that constrain what you may do.
7. **Resolve staleness before acting.** A loaded fact can be superseded — if a decision looks old, check
   `facts(key=...)` for its `history()` or `facts(at=...)` for what was believed then, and treat a grounding
   lesson whose warned symbols now exist (`expire_known`) as retired, so you never act on a fact that stopped
   being true while you were gone.
8. **Land on the resume point.** End the brief on the single next action from the handoff's "next" list — the
   precise file, branch, or command to start from. Make "where do I begin?" a non-question, the exact mirror of
   what `baton-pass` pinned when it saved.

## Output
A single session-start brief synthesized from the saved context bundle, your own recent run history, the current
temporal decisions, the presence/overnight report, and recalled lessons — landing on one concrete resume action
— so the model opens the session knowing who it is on this repo, what it did last, what's pending, and what it
learned, instead of waking to cold amnesia.
