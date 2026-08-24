# Founder first-run findings

This ledger records what the optimized Estelle binary actually exposes. `web/` was read as the visual
reference and was not edited.

## First-run defects

| Finding | Owner and result | Evidence |
|---|---|---|
| Mouse wheel recalled composer history | CLI fixed: the alternate screen enables mouse reporting and wheel events change only transcript scroll. Arrow keys remain composer history. | `tui/src/main.rs:199-220`, `tui/src/main.rs:2430-2440`; mutation guards `mouse_wheel_scrolls_the_transcript_without_recalling_composer_history` and `terminal_session_requests_mouse_events_instead_of_wheel_to_arrow_translation` |
| `hi` waited 42 seconds | Server handoff. The current server source already classifies a closed conversational vocabulary before scope, inventory, recall and gate work; 10 focused tests pass with `--no-cov`. The production measurement therefore contradicts the checked-in fast path and needs deployment/runtime verification. No client classifier was added. | `src/estelle/serve/utterance.py:102-112`, `src/estelle/serve/api_intel.py:510-545` |
| Dots-only empty state | CLI fixed. The empty frame now puts real actions in front of a faint composed scene: sun, layered ridges and ghost blooms quantized through the canonical Bayer pass. The composer caret owns a separate cream-core/red-wake layer. The field disappears once transcript, sweep, the composer command popup, or gate data needs the surface. | `tui/src/main.rs` (`scene_coverage`, `render_symbol_ground`, `render_empty_state`, `render_frame`); references read: `web/app/explore/_components/dither.ts`, `DitherField.tsx`, `BootLoader.tsx`, `LilyField.tsx`, `DitherTrail.tsx`, `SiteFoot.tsx`, `site-foot.css`, `effects.ts` |
| `/model` declined without an alternative | CLI fixed. The refusal names the account-wide dashboard and states the actual routing policy: strongest configured model plans; cheapest capable model implements. | `tui/src/commands.rs:920-929` |
| `/skills` hid a legitimate name | CLI fixed without weakening the credential detector. A validated skill-name field may contain ordinary `sk-` across a word boundary; summaries and credential-shaped names still pass through the existing masker. | `tui/src/main.rs` (`mask_skill_catalog_line`); red/green guard `skill_catalog_preserves_valid_names_while_real_credentials_stay_hidden` |

## Customer-visible copy sweep

The release path changed these inherited or misleading strings:

| Before | After |
|---|---|
| `Compose new task` | `Ask Estelle` |
| `? for shortcuts` plus Codex's `100% context left` | `enter ask`, `shift+enter newline`, `? shortcuts`; the fake local context-window meter is absent |
| Bare dither with no action | `Ask about <repo>`, `/review Read current changes`, state-aware `/sweep`, `? Show shortcuts` |
| Gate footer `Compose new task` | Gate footer `Ask Estelle` |
| `/orchestra` “fan a task across the routed fleet” | `/orchestra` “run one gated server task” |
| `/subagents` “view server orchestra agents” | Honest status: the fixed grid is implemented, but production still lacks the revisioned live-state wire |

The maintained generic `ComposerInput::plain_text()` remains available for text-only embedded fields.
The Estelle release now constructs `ComposerInput::with_commands("Ask Estelle", catalog)` on startup and
on every reset. That path uses the finished bottom-pane composer, including its command popup, filtering,
selection, validation, paste state machine, chrome, and footer. The catalog contains only Estelle-owned
commands: Codex built-ins are disabled for this embedding, and accepted commands return as submitted text
for Estelle dispatch. The former outer palette and hand-built composer border no longer run beside it.

Committed turns now pass through `history_cell` as well. User turns use its filled, width-aware cell;
assistant answers use its source-backed markdown cell so resize reflows the original markdown rather than
already-wrapped output. Estelle still supplies the grounded/degraded heading, failure copy, secret masking,
and citation lines. Those are product semantics, not duplicate wrapping or markdown machinery.

## Jcode and fleet surface inventory

| Requested graft | Implementation | Exact user door in the release binary | Binding today | Disposition |
|---|---|---|---|---|
| Side panel | The jcode surface pattern is ported as one persistent right-hand grounding view. Repo graph citations and the exact Working-memory files attached to the last question remain separate. It does not reuse jcode's local agent brain. | `/context` or `Alt+M`; both open and close the same surface, and `/help` names both. | **Yes**, in the release TUI. | Ported surface, Estelle data owners. |
| Swarm view | The TUI accepts only a server-emitted typed `fleet` snapshot and pins a five-column, fixed-height grid above transcript scrollback. Explicit `unknown`, stale observations and real denominators are visible; request timing never becomes progress. | A server response carrying `fleet` opens the view. Current production `/orchestra` does not emit that field. | Renderer: **yes**. Live production binding: **no**, pending the filed contract. | Register #41 is a server contract, not a missing client widget. |
| Todo ledger | The Kimi task-state surface is ported as a session-scoped, server-emitted ledger. Completed rows retain their measured result, inferred rows are marked, and stale snapshots say so. | `/todo` opens/closes; `Ctrl+T` expands/collapses five visible rows. | Renderer and bindings: **yes**. Production data: only when a reply emits `todo`. | No transcript-derived or timing-derived tasks. |
| Session resume from other harnesses | Repointed, not ported. `tui/src/commands.rs` routes Estelle `GET /session` and renders the server record. No local foreign-session file reader is called by this binary. | `/sessions`, then `/resume <server-session-id>`. | Server session resume: **yes**. Foreign local file resume: **no**. | Repointed to the server owner. |
| Working memory | `tui/src/top_level.rs:706` collects changed, staged and untracked Git source files. `tui/src/main.rs` `answer_question` attaches them as DATA on the single `/deep-search` call (separate `working_memory` key; the question is the user's message verbatim — D16) for non-conversational questions; conversational turns skip the upload so the server's fast path can fire. | Type any ordinary question while the repo has eligible local changes. There is no picker and no second identity. | **Yes**, automatic on the real question path. | Ported because no server owner can represent one developer's uncommitted files. |
| Cache-cold warning | `tui/src/main.rs` renders elapsed seconds, Esc cancellation and, after five seconds, the non-claim `server prelude/cache may be cold`. | Start any remote request and wait five seconds. | **Yes**, automatic. | Ported as status furniture; it never claims a measured cache miss. |
| Agent grep | `tui/src/commands.rs:542` posts `{query, code:true}` to Estelle `/search`; `tui/src/commands.rs:937` renders exact `file:line` or disclosed approximate `file:~line`. | `/grep <query>`. | **Yes**. | Repointed to the server's structural search, not a local grep brain. |

## Register #41: the live Orchestra wire is missing

The fan-out is real server work, but the server-side live window onto it does not exist. The current CLI sends exactly one
task in `tui/src/commands.rs:551`. The accepted `POST /orchestra` implementation does not return until it
has completed every run (`src/estelle/serve/api_orchestra.py:101-117`), and the legacy runner processes
its task list serially (`src/estelle/serve/orchestra.py:117-133`). There is no queued/running action,
per-slot progress, or stream in the response contract from which a truthful live grid can be rendered.

The client view now exists, but Register #41 remains open until the server exposes live, stable slot state.
`docs/SERVER-CONTRACTS-NEEDED.md` section 2 defines the required asynchronous start, revisioned status read,
explicit absent state and staleness. The implemented CLI view is:

- fixed-height grid, five columns, one row per slot;
- cell = index, bounded status glyph/bar, current action truncated to one line;
- not-started slots say `Queued`; running slots show their current action;
- one fleet progress bar, plus an `Estelle Orchestra` header containing batch name, the unique
  server-reported model roster and agent count;
- machine logs remain exhaustive elsewhere; the grid never scrolls and never fabricates activity.

Building a completed-run dashboard from the stamped response would not satisfy this register. A response
without `fleet` continues to render the old completed-run report and never opens an invented live grid.

## Follow-up rendered surfaces

```text
Estelle Orchestra · Mutation lane detection ×5 · models: K3
001 [········] Checking kill switch  002 Queued  003 ✓ Verified isolation  004 × Grounding refused  005 ? Needs repo
Working...  ████████
```

```text
grounding context
Repo graph · team's swept copy
billing.py:88
  symbol  charge_card

Working memory · private to this session
<the exact eligible files attached to the last question>
Alt+M or /context closes
```

```text
production health
prod · 1 unresolved issue
error counts · ▁█
request denominator unavailable
caught · TimeoutError in charge_card
grouped · 12 events
traced to · billing.py:82-119
gate · repair has not reached the gate
```

The production pane is off by default and opens with `/prod`. It polls repo-scoped issues every 30 seconds
and overview every 60 seconds in background tasks; failure, idle and focus-loss cadence caps at 300 seconds.
It tolerates both the current `error_rate.series[].count` payload and the newer denominator-bearing series.
It never labels counts as a rate, never zero-fills `symbol_range`, and never says `YOU ARE EDITING` because
the current TUI still has no trustworthy open-file/cursor feed. The server exposes a stamped repair verdict,
not a sandbox stream, so S3 is exactly one line: `sandbox · a clone, never production · <verdict>`.

## Red-before-green record

- The first-frame guard failed on missing `Ask Estelle` before the copy and useful empty state landed.
- The old braille guard failed after the wrong material was removed and was replaced only after every new
  frame was read.
- The caret-wake guard first failed because `set_text` does not move the real caret; it now drives actual
  right-arrow events through the composer.
- The `/model` alternative guard failed on the missing dashboard route before the refusal was rewritten.
- The skill catalogue guard failed because `change-deploy-risk-gate` was replaced wholesale; it now keeps
  that name while a real `sk-abcdefghijklmnop` remains hidden.
- The `?` guard was mutation-tested by changing the dispatcher to `!`; it failed to render `/help`, then
  passed after the real binding was restored.
