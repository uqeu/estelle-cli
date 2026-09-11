---
name: change-impact
description: Change this and what else has to change? Nothing missed. Use when the task involves: change impact, blast radius, cross-cutting change, what breaks if I change, ripple effect, impact analysis.
---

# change-impact

Estelle's cross-cutting-change skill, and a direct answer to Sourcegraph's flagship demo (a batch change that
touched "31 files across 7 layers a naive agent missed"). Finding those files is the *easy* half — a structural
graph does it deterministically. This playbook runs that half over the CUSTOMER's whole-repo `RepoSurface` /
`CodeGraph` (a Sweep of *their* code, not Estelle's package), then adds the half a find-the-files tool
structurally cannot: **verification**. It doesn't just list the sites — it certifies that the proposed edit
references only symbols that actually exist and type-checks, using the same deterministic grounding gate that
earns "zero hallucinated APIs on your own codebase". Sourcegraph finds; Estelle finds **and** verifies.

The whole capability is pure and reproducible — same graph → same map — and is exposed directly as
`change_impact(graph, target, *, changed_symbols=()) -> ImpactReport` in `conveyor/change_impact.py`, composed
with the grounding gate by the `POST /impact` endpoint.

## When to run
Before any change that could ripple beyond the file you're editing: renaming or retyping a model field,
changing a function signature, deleting a symbol, a framework/library bump, or a batch codemod across the repo.
Run it *first* to size the blast radius and to catch the layer you'd otherwise forget (the migration, the
tests, the serializer), and run `/impact` verification on the drafted edit *before* it lands so a hallucinated API
never reaches review.

## Procedure
1. **Build the code graph of the customer's repo.** Sweep their repository into a `CodeGraph` / `RepoSurface`
   (`build_repo_surface` over the Sweep's `(path, content)` files) so the fan-out in step 3 rides *their* real
   import edges and symbol definitions — resolved Python dotted imports and JS/TS relative imports — not
   assumed ones. This is the ground truth a chunk-RAG index cannot provide.
2. **Name the change precisely.** The `target` is the changed symbol or file; `changed_symbols` narrows the
   fan-out to the specific fields/methods that moved. For "I changed field `total` on class `Order`", pass
   `change_impact(graph, "Order", changed_symbols=("total",))` — the class gives the origin file for the
   import closure, the field drives the reference fan-out.
3. **Fan out to EVERY affected site (nothing missed).** Union three deterministic graph queries, deduped by
   file: the **definition sites** (`definition_sites` — the change's own home, `file:line`); every **direct
   reference** (`references` — repo-wide find-usages, instant where grep is noisy); and the **transitive
   importer closure** (`blast_radius` — every file that transitively imports an origin file, the reverse-import
   closure that answers "what could break?"). Each site carries WHY it's in the set (defines / references /
   transitively imports).
4. **Group the sites into stack layers.** Cluster the affected files by `subsystems` plus a deterministic
   path/name heuristic into the layers a cross-cutting change commonly spans — model, data-access/store,
   api/dto, auth/middleware, routes/frontend, migration, audit/logging, tests — so the map reads top-down
   through the stack instead of as a flat file list. Rank the load-bearing layers with `betweenness_centrality`
   / `central_files` when you need to know which hop is riskiest.
5. **Flag the layers this change FORGOT.** When the change touches a core-data layer (model or store) but
   leaves a companion layer untouched — no migration, no updated tests — surface it in `commonly_missed`. This
   is the "did you forget the migration?" signal: the exact class of miss a naive per-file search silently
   ships, and the reason a graph beats grep here.
6. **VERIFY the proposed edit — the beat.** Send the drafted cross-cutting change through `/impact`, which
   reuses the deterministic grounding + TYPE gate (`check_grounding`): it flags any symbol the edit references
   that the repo doesn't define (`ungrounded` — a hallucinated API), any call with the wrong arity
   (`arity_errors`), and any attribute/argument that contradicts the repo's own types (`type_errors`). A
   find-the-files tool would list your invented `Order.recompute_totals()` right next to the real methods; the
   gate refuses it. Certification, not a suggestion.
7. **Route the model by task.** Producing the human-readable change plan from the report is a `classify` /
   `chat` job, so `model_router.route("classify", tokens=…)` returns the CHEAP tier; drafting the actual
   cross-cutting edit is an `architect` task, so `route("architect", needs_reasoning=True)` returns FRONTIER.
   `Route.reason` keeps the choice auditable. The verify step costs zero model calls when the edit is clean.
8. **Emit the map and gate the edit into the loop.** Return the `ImpactReport` — target, total sites, the
   ordered per-layer breakdown with `file:line` sites and the `why`, and `commonly_missed` — then require the
   proposed change to clear the grounding gate (and the merge gate + test suite) before it lands. File wide-radius
   changes as `run_autonomy` swarm tasks, ceiling-gated, one worktree per affected subsystem.

## Output
A verified change-impact map of the customer's repo: from one changed symbol or file, every affected site
(deduped, with `file:line` and why it's affected), grouped into the stack layers the change spans and ordered
top-down, plus a `commonly_missed` flag naming the companion layers a change of this shape usually touches but
this one didn't. And — the differentiator — a grounding verdict on the proposed edit itself, certifying it
references only real symbols and type-checks against the repo's own surface. Sourcegraph tells you which files
changed; Estelle tells you that *and* whether your change is real.
