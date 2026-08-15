# Orchestra live-view data contract

This contract is now section 2 of `docs/SERVER-CONTRACTS-NEEDED.md`, where it is ranked beside every
other server-owned gap blocking the CLI. Keep this file only as a compatibility pointer for existing
references. The normative endpoint, payload, lifecycle, absence, evidence, and rendering requirements live
in that document.

<!--

## The missing wire

The current `POST /orchestra` and `POST /orchestra/run` handlers return only after execution. Their
completed envelopes contain routing and roll-up data, but no job identity, revision, per-agent lifecycle,
current action, or incremental read. Those envelopes cannot drive a live view.

The server must make execution asynchronous:

1. `POST /orchestra/run` accepts one `task` or an explicit `tasks` array and returns `202` after the
   server has planned and durably created the fleet.
2. `GET /orchestra/status?fleet_id=<id>&after_revision=<n>&wait_s=20&repo=<owner/name>` is a scoped
   long-poll. It returns when the revision advances, the fleet becomes terminal, or 20 seconds pass.
3. Every response carries the full latest `fleet` snapshot. The client replaces by revision; it never
   reconstructs missed transitions.

## Normative snapshot

```json
{
  "fleet": {
    "id": "fleet-41",
    "batch": "Retry missing 5 assignments",
    "models": ["K3", "gpt-5.5"],
    "state": "running",
    "attempt": "retry",
    "revision": 8,
    "observed_at": 1785203400.0,
    "stale_after_s": 60,
    "completed": 1,
    "total": 2,
    "narrator": {
      "text": "a007 lost 4 assignments, a034 lost 1 (driver timeouts). Retrying those two slices.",
      "evidence": "observed"
    },
    "agents": [
      {
        "index": 1,
        "status": "running",
        "attempt": "retry",
        "state_observed_at": 1785203400.0,
        "current_action": "Checking kill switch invariants",
        "progress": {"completed": 2, "total": 5},
        "assignments": {"attempted": 4, "completed": null, "lost": 4},
        "failure_cause": {"text": "driver timeout", "evidence": "measured"}
      },
      {
        "index": 2,
        "status": "unknown",
        "attempt": "retry",
        "state_observed_at": 1785203380.0,
        "unknown_reason": "worker has not reported state",
        "assignments": {"attempted": null, "completed": null, "lost": null}
      }
    ]
  },
  "todo": {
    "observed_at": 1785203400.0,
    "stale_after_s": 60,
    "items": [
      {
        "title": "Isolation width 10",
        "status": "done",
        "result": "owner 10/10, cross-tenant 0/10",
        "evidence": "measured"
      },
      {"title": "Mutation lane", "status": "in_progress", "result": null, "evidence": "observed"}
    ]
  }
}
```

## Fleet invariants

- `id` is stable for the run; `revision` is strictly increasing.
- `total` is the number of admitted slots after the parallel decider, not the requested ceiling. An
  unknown total is `null`, never zero. The renderer may show received rows but labels the total `?`.
- `agents` contains exactly one row for every admitted index, including agents still `queued`.
- `models` contains the participant model names reported by the server. The client trims blank entries and
  removes exact duplicates while preserving first-reported order. Legacy producers may send the single
  `model` string as a backward-compatible fallback; the client never splits or expands it. If neither field
  names a model, the header says `models unknown` rather than inferring from routing policy or agent prose.
- The empirical Kimi lifecycle is `created`, `starting`, `running`, `awaiting_approval`, then
  `completed`, `failed`, `killed`, or `lost`. Estelle additionally admits `queued`, `timed_out`,
  `blocked`, `needs_input`, `cancelled`, and explicit `unknown`. Legacy `done` is accepted only as a wire
  alias for `completed`; new producers MUST emit `completed`.
- `unknown` is a real state, not an omitted value. It requires a non-empty `unknown_reason`. A worker
  whose state cannot be observed MUST report `unknown`; missing state may never default to `running`.
- `completed`, `failed`, `timed_out`, `killed`, and `lost` are distinct terminal outcomes with distinct
  glyphs and colours. A stopped process is not a successful process. `completed` requires a measured
  successful exit; a timeout may never render a checkmark.
- `attempt` is `first`, `retry`, or `unknown`. Retry batches retain the identities, lost assignment
  counts, and failure causes that triggered recovery. They are never silently restarted.
- `assignments.attempted`, `.completed`, and `.lost` are nullable counts. `null` means the server does
  not know. Missing knowledge may never default to zero.
- `failure_cause` and `narrator` carry `evidence`: `measured`, `observed`, `derived`, `inferred`, or
  `unknown`. Derived, inferred, and unknown statements are visibly marked in every UI surface.
- Every agent carries `state_observed_at` as Unix epoch seconds. The snapshot carries `observed_at` and
  `stale_after_s`; the client labels a row `STALE` once its observation exceeds that bound. Missing or
  non-finite observation times violate the response contract rather than appearing current.
- `current_action` is server-observed bounded text, never chain-of-thought. It is required while
  `running`, optional otherwise, and contains no credential-shaped value. The client collapses whitespace,
  removes markup, skips pure headings, and truncates at a display-width-aware Unicode boundary.
- Agent `progress` is emitted only when the worker owns a real denominator. Absence means unknown, never
  zero. The fleet footer uses a separate animated glyph for client liveness; animation is not progress.
- Fleet `completed` and `total` are nullable measured counts. The aggregate bar is green to the completed
  fraction and blue beyond it; glyphs duplicate the colour meaning for terminals without truecolour.
- A task refused by the parallel decider is not an agent row; it remains in `decision.refused`.
- The status read is account- and repo-scoped. Unknown or mismatched IDs return 404 without revealing
  whether another tenant owns them.
- `ETag` equals the fleet revision. `If-None-Match` may return `304`; `429` and `5xx` carry `Retry-After`.

## Todo invariants

- `todo` is session state, independent of `fleet`; it can render when no Orchestra run exists.
- Item status is `pending`, `in_progress`, `done`, or explicit `unknown`.
- A done item retains its `result`; the TUI crosses out the full line instead of replacing it with a
  checkmark that discards the finding.
- The collapsed view shows five items and reports both hidden and hidden-done counts. `Ctrl+T` expands or
  collapses it; `/todo` opens or closes the surface. Both bindings appear in `/help`.
- `observed_at` and `stale_after_s` apply to the todo snapshot. An old ledger is labelled stale rather
  than implied current.

## Client rendering

The TUI renders five columns at every supported width, one display-width-truncated line per agent, one
fleet progress line, and no scrolling inside the grid. The header says `Estelle Orchestra`, names only the
server-reported model roster, and the terminal fleet footer says `Completed`. A narrator line precedes recovery grids. The
transcript remains the exhaustive record. A reply without `fleet` continues to render the completed-run
report and does not open an invented live view. A reply without `todo` does not invent a task list.
-->
