# P6 showpiece acceptance ledger

P6 changes only the customer-reachable Ratatui client. No `web/` file was edited and no sandbox stream
was invented. A subsequent founder follow-up added the production pane, live-fleet renderer and grounding
context surface; each remains a view over emitted or measured data:

| surface | data owner | rendered truth |
|---|---|---|
| cited evidence | `Source` values on the Estelle HTTP answer | `file` and optional real `line`; an absent line is omitted, never zero-filled |
| sweep gauge | the shared local collector plus `/sweep/estimate`, `/sync`, and background-ingest progress | collected files/bytes, capacity check, transport progress, completion |
| blast-radius chart | the exact patch sent to `/gate` plus Git `--numstat -z` for the same comparison | changed lines per file; NUL-delimited paths preserve spaces, tabs, and newlines |
| refusal red | the server's typed gate verdict or non-empty blockers | red modal only for an actual refusal; a passing response never wakes it |
| production health | repo-scoped `/monitor/issues` and `/monitor/overview` | real issue counts, ranges, gate absence and repair state; missing request denominators are disclosed |
| Estelle Orchestra | optional typed `fleet` snapshots | fixed five-column server state and server-reported model roster; unknown and stale are explicit, and missing state never becomes running |
| Todo ledger | optional typed `todo` snapshots | five visible rows, retained completed results, explicit provenance and staleness; `/todo` and `Ctrl+T` are the only bindings |
| grounding context | answer citations plus the exact Working-memory attachment list | Repo graph and private uncommitted files remain separate |

The original calm-state `Canvas` was rejected after the founder's first run: it promoted bare Braille
dither into the content. The repaired ground uses the website's exact `BG`, `GHOST`, `INK`, `RED` values
and Bayer ordering to quantize a composed sun/ridgeline/ghost-bloom scene. It renders before useful empty
state actions, so content owns every occupied cell. A separate caret wake uses cream at the editing core
and red only in its trail. The whole layer disappears when transcript, sweep, palette, or gate data needs
the content area and never enters the composer rows.

## P5-class citation defect

The HTTP type already parsed `sources`; the TUI dropped them in the answer-success branch at
`tui/src/main.rs:987`. The repaired seam now copies them into both current-pane state and the durable
`TranscriptEntry::Answer` at `tui/src/main.rs:989`. The renderer consumes them at
`tui/src/main.rs:1652`, with the wide split pane beginning at `tui/src/main.rs:2073`.

That was customer-visible grounding loss: an HTTP answer carrying `api/charge.ts:52` reached the screen
without that citation. The regression test crosses mock HTTP, event handling, app state, and the TestBackend
frame; it failed on the old renderer and passes only when `api/charge.ts:52` is visible without moving the
composer.

## Red-before-green evidence

- Swarm outcomes: `completed` first failed at serde because the old wire admitted only `done`; the guard
  passed after explicit completed/failed/timed-out/killed/lost outcomes replaced the generic success bucket.
- Swarm cell text: the reference `**Report:**` heading occupied the only useful line; the guard passed after
  markup removal, whitespace collapse, meaningful-line selection and Unicode display-width truncation.
- Todo: its first guard failed because no typed snapshot or renderer existed. The collapse guard now retains
  full completed results and counts hidden/done rows; separate red tests established the `Ctrl+T` binding,
  inferred marker and stale heading.
- Aggregate: the completion-boundary test failed before a styled progress line existed, then passed with a
  green measured segment, blue remainder and independent moon-phase liveness glyph.

- Citation seam: the real HTTP response carried `api/charge.ts:52`; the rendered frame did not. It passed
  after sources became part of the transcript contract and wide/narrow renderers both consumed them.
- Sweep: `/sweep` initially printed instructions and never started ingest. The first test failed because no
  active request existed. It passed after the TUI called the same collector/estimate/sync engine as the
  top-level command.
- Sweep progress: the sending state was deliberately changed from 35% to 100%; the wire test failed with
  `[10, 20, 100, 100]` instead of `[10, 20, 35, 100]`, then passed after restoration.
- Gate: a real mock `/gate` refusal initially rendered as ordinary transcript text. The integration test
  failed until `EDIT REFUSED`, the protected-repository sentence, two files, six changed lines, and the next
  action were all visible in the modal.
- Filename measurement: the NUL-delimited parser was deliberately changed to truncate at newline. Its test
  failed with `dir/name` instead of `dir/name\nwith\ttabs.rs`, then passed after byte-preserving restoration.
- Empty frame: the new first-frame guard failed because `Ask Estelle` and every real action were absent.
  The former Braille-presence test then failed when the rejected material was removed. Its replacement
  proves composed symbol art remains deterministic, sits behind useful actions, and never restores bare
  Braille.
- Caret wake: the first test incorrectly changed text without moving Codex's real caret and failed. The
  repaired test drives arrow events through the maintained composer and proves the separate wake follows
  that caret without touching transcript state.
- Snapshots: three new P6 baselines first failed as unreviewed `.snap.new` files. Review rejected the gate's
  first line chart because interpolation fabricated values between independent files. The accepted frame is
  a scatter chart with explicit per-file counts. All three final snapshots were read before acceptance and no
  `.snap.new` remains.

## Measurements and corrections

- The current renderer cadence is **100 ms**, not the brief's stated 80 ms
  (`tui/src/main.rs`, `FRAME_INTERVAL`). P6 preserves the measured implementation.
- Over 500 debug TestBackend frames at 120x32: plain averaged **503 us**; composed ground averaged
  **3,498 us**. The full frame is **3.5%** of the 100,000 us cadence, below the predeclared 10% cutoff.
  Cursor tracking therefore stays; composer geometry is unchanged.
- Inline Mermaid is technically feasible through jcode's renderer, but Estelle has **0 accepted frame-cost
  measurements** for it against the **100,000 us** cadence. It therefore has not passed the frame-budget
  guard and is not shipped. The measured non-Mermaid baseline remains **3,498 us/frame**; this decision is
  revisitable only after a representative cold/warm benchmark records p50 and p99 beside that baseline.
- `Cargo.lock`: **1,311 packages**, unchanged from P5.
- Cargo workspace: **124 packages**, unchanged from P5.
- Direct P0 deletion exceptions: **19**, unchanged. P6 added no crate.
- The spec's aesthetic instruction and the ancestor's ANSI-color lint conflict for exact `#E9E6DC`.
  Four named P6 palette constants use Ratatui's typed true-color constructor; the lint remains enabled and
  strict Clippy passes.

## Reviewed frames

The accepted snapshots contain every terminal row. The crops below preserve every occupied product row.

### Sweep

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories

sweep  sending complete source set · 35% · 184 files · 891 KB
████████████████████████████          35%

input · plan  |  model auto  |  memory ...  |  server ...

› Ask Estelle
```

### Cited answer

```text
you  Why is this charge retried?                         │ cited evidence
                                                         │ 1  api/charge.ts:52
estelle  grounded                                        │ 2  billing/retry.ts:118
The retry is bounded by the idempotency guard.           │

input · plan  |  model auto  |  memory ...  |  server ...
› Ask Estelle
```

### Gate refusal

```text
┌ gate · deterministic · no model ─────────────────────────────────────────────┐
│                                EDIT REFUSED                                  │
│            Gate protected this repository. Nothing was written.             │
│Verdict  blocked                                                              │
│ blast radius · 2 files · 6 changed lines                                     │
│⠁                                                                            │
│                                                                            ⢀│
│     5  api/charge.ts                                                         │
│     1  billing/retry.ts                                                      │
│blocked  api/charge.ts:52  invented call rotate_all_keys does not exist       │
│                       Enter or Esc closes · Ask Estelle                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### First frame

```text
Ask about uqeu/estelle

/review  Read current changes
/sweep   Index or refresh this repo
?        Show shortcuts

0   0 EOF 0 0 0 0 0 err          0x
      0       0x          err
1 0 0 0 0 EOF       NaN           0x

input · plan  |  model auto  |  memory ...  |  server ...
› Ask Estelle
   enter ask    shift+enter newline    ? shortcuts
```

Exact baselines:

- `tui/src/snapshots/estelle__tests__snapshot_p6_sweep_gauge.snap`
- `tui/src/snapshots/estelle__tests__snapshot_p6_citation_pane.snap`
- `tui/src/snapshots/estelle__tests__snapshot_p6_gate_refusal.snap`
- `tui/src/snapshots/estelle__tests__snapshot_empty_composer.snap`

## Acceptance evidence

- `cargo test -p estelle-tui --bin estelle`: **104 passed**.
- `cargo clippy -p estelle-client --all-targets -- -D warnings`: passed.
- `cargo clippy -p estelle-tui --bin estelle -- -D warnings`: passed.
- `cargo fmt -p estelle-client -p estelle-tui -- --check`: passed.
- `cargo build --release -p estelle-tui --bin estelle`: passed. The retained app-server dependency emitted
  its pre-existing `unused_mut` warning; the Estelle binary emitted no warning.
- The optimized binary launched in a PTY, drew the composed ground, resolved live header counts, accepted `/exit`,
  and restored the alternate screen.
- Package-wide `cargo test -p estelle-tui` still fails at the documented P0 exception
  `tui/tests/all.rs:8`: it imports the deleted `codex_cli` crate. The shipped binary target is green; no hollow
  compatibility crate was added to hide the retained exception.
