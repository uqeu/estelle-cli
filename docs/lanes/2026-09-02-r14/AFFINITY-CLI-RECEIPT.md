# AFFINITY-CLI receipt

_Lane report · 2026-09-02 · Estelle · round r14_

## Outcome

The CLI now has two full-screen, boxless surfaces:

- `Ctrl+M` opens a per-role model dial for plan, implement, and review. Each role can remain AUTO or
  select a configured provider/model pin. The screen reads the server's effective `routing_table`, sends
  one complete three-role proposal, never sends `_routing_table`, and renders the server's returned bundle
  after save.
- `Ctrl+S` and `Ctrl+Shift+S` open spend. Completed Work receipts show served model, input tokens, output
  tokens, and vendor-list cost by role. Completed Orchestra receipts show priced aggregate models plus
  worker model calls; worker-call cost remains `not measured` unless the server attaches it directly.
  Vendor-list estimates and Estelle token invoices are separate owners and separate rows.

Unknown cost is never painted as zero. A received zero is `$0.000000`; absent or unsupported cost is
`not measured`. The screen makes no savings claim. Session totals include exact completed receipts only
and say when the session is incomplete. Plan remaining comes from the account budget and period spend.
Memory capacity comes from a live `/sweep/estimate` request over the same bounded, git-visible inventory
used by sweep, reduced to path and byte count before the request.

## Founder notes

Addressed, in the founder's words:

- “Affinity chooses the models for them, but if they want to hard-code — select a model to run a task —
  they can.” The CLI now exposes AUTO and PINNED per role through `Ctrl+M`.
- “I'm just very sad that I don't see the costing thing.” The CLI now exposes spend through `Ctrl+S` and
  `Ctrl+Shift+S`.
- “Orchestra should be showing the costs of each model.” Completed Orchestra receipts now show the model
  totals actually priced by the server and refuse unjoined per-worker costs.

The remaining design-book notes were not changed because this lane owns only the model picker and costing
panel. Login layout, light-theme luminance, waiting copy, skill offers, slash-command audit, gate loops,
Mermaid rendering, and tool-call styling remain with their named lanes; changing them here would mix
unreviewed concerns into the separate CLI repository.

## Captures

- Models: [80 columns](captures/models-80.txt), [120 columns](captures/models-120.txt),
  [80-column SVG](captures/models-80.svg), [120-column SVG](captures/models-120.svg)
- Spend: [80 columns](captures/spend-80.txt), [120 columns](captures/spend-120.txt),
  [80-column SVG](captures/spend-80.svg), [120-column SVG](captures/spend-120.svg)

Both widths are produced by the production `render_frame` path with typed test payloads. The capture guard
also asserts that the model surface contains no box-corner glyph and that its selected row has a semantic
background. These are renderer captures, not production HTTP probes.

## Tests and proved-red guards

Eight new guards cover the new contracts:

1. `absent_cost_is_not_rendered_as_zero_but_received_zero_is`
2. `capacity_preserves_unlimited_and_measured_remaining_as_distinct_states`
3. `orchestra_does_not_allocate_global_cost_to_worker_calls`
4. `work_receipt_keeps_role_models_tokens_and_two_money_owners`
5. `effective_display_table_wins_and_unconfigured_models_are_not_offered`
6. `save_proposes_one_complete_table_without_the_private_routing_key`
7. `affinity_shortcuts_open_and_close_the_full_screen_surfaces`
8. `affinity_models_and_spend_capture_at_80_and_120_columns`

Each guard was mutated independently and observed red before restoration:

| guard | mutation | observed failure |
|---|---|---|
| absent versus zero | default absent money to `$0.00` | expected `not measured`, received `$0.00` |
| capacity states | render an unlimited cap as `0 remaining` | required `unlimited` text disappeared |
| Orchestra ownership | assign exact zero to an unpriced worker call | expected `NotMeasured`, received `Exact(0.0)` |
| Work roles | label implementation usage as plan | roles became `[plan, plan, review]` |
| effective table | read private `_routing_table` | effective plan role was omitted |
| save body | emit `_routing_table` | public `routing_table` was absent from the request |
| shortcuts | disable the `m` and `s` control-letter path | the models surface did not open |
| boxless capture | add a `┌` to the models title | the no-box capture assertion failed |

After restoration, `RUST_MIN_STACK=16777216` runs collected and passed:

- controlled serial library suite: 3,218 passed, 0 failed, 1 ignored;
- controlled serial `estelle` binary suite with ambient `ESTELLE_API_KEY` removed: 369 passed, 0 failed;
- integration binaries: `all` 9 passed / 4 ignored, `hook_process` 1 passed,
  `manager_dependency_regression` 1 passed, `test_backend` 0 tests, `visual_gallery` 1 passed;
- focused affinity selection: 9 passed, 0 failed;
- `cargo clippy -p estelle-tui --all-targets -- -D warnings`: clean.

The ordinary parallel package run was not green: the library reported 3,214 passed, 4 failed, 1 ignored,
and the main binary reported 367 passed, 2 failed. The failures were shared request-count/socket timing and
ambient-login-state tests. Every affected test passed alone, and both complete controlled serial suites
passed. Therefore the green claim covers deterministic serial execution, not freedom from parallel-test
interference in the pre-existing harness.

## Limits

The live `FleetSnapshot` contract does not join worker tasks to served-model tokens or cost. This UI does
not infer that join; it waits for a completed receipt. Vendor-list values are routing disclosures, not
Estelle charges, provider invoices, comparative-quality evidence, or measured savings. A byte-derived
memory estimate is labelled estimated when the server says it is not exact.

This CLI-only lane did not census or move an architecture board. It did not deploy, tag, publish npm, or
exercise production HTTP because the lane brief reserves integration and production actions for COACH.

```estelle-receipt
lane: cli
round: 14
date: 2026-09-02
branch: codex/affinity-cli-20260902
head: c4af69ae0b128de09d36eca639f477796a733003
head_verified: git ls-remote

board_before: green=0 yellow=0 red=0 population=0
board_after:  green=0 yellow=0 red=0 population=0

closed_by_probe: 0
closed_by_wire: 0
closed_by_build: 0
retired: 0

states: built=yes wired=yes tested=yes deployed=no probed=no

limit: CLI-only lane censused no architecture board; live FleetSnapshot has no worker-to-cost join; controlled serial suites passed but the pre-existing parallel harness still interferes; no deployment, publication, or production HTTP probe was authorized
open: COACH owns integration; the release owner owns publication and production positive/negative probes
```
