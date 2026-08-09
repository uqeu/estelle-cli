# P5 graft acceptance ledger

This ledger describes the customer-reachable `estelle` binary. “Repointed” includes an intentionally
visible command whose former local Codex owner is gone; its output names the real owner and says when the
server lacks the corresponding mutation. No row silently falls through to the removed local agent.

## Top-level commands (20/20)

| command | disposition | owner / reason |
|---|---|---|
| `login` | ported | encrypted Estelle credential store and remote verification |
| `init` | ported | local editor configuration plus Estelle repo state |
| `sweep` | ported | safe Git inventory to `/sync` or background ingest |
| `reindex` | ported | safe changed-file inventory to `/reindex` |
| `connect` | ported | local editor configuration |
| `remove` | ported | local editor configuration removal |
| `github` | ported | Estelle GitHub identity/setup endpoints |
| `monitor` | ported | Estelle production monitor endpoints |
| `research` | ported | Estelle vendor-drift endpoints |
| `memory` | repointed | Estelle memory/forget/retract/unlearn endpoints |
| `ask` | repointed | Estelle `/deep-search` |
| `recall` | repointed | Estelle `/search` |
| `verify` | repointed | Estelle `/verify` |
| `gate` | repointed | local measured diff to Estelle `/gate` |
| `hook` | ported | canonical Rust ground/sync hook runtime |
| `install-hooks` | ported | local Claude/Codex hook configuration |
| `uninstall-hooks` | ported | removes only Estelle-owned hook entries |
| `acp` | ported | maintained ACP stdio adapter to Estelle HTTP |
| `mcp` | ported | maintained RMCP client: external `tools/list` and `tools/call` |
| `mcp-server` | ported | maintained RMCP server over Estelle `/mcp` |

## Original Estelle session commands (23/23)

| command | disposition | owner / reason |
|---|---|---|
| `/help` | ported | local complete command ledger |
| `/init` | repointed | Estelle `/wiki` |
| `/memory` | repointed | Estelle `/deep-search`; no Codex memory filesystem |
| `/sweep` | ported | directs to the explicit top-level ingest command |
| `/sessions` | repointed | Estelle `/sessions` |
| `/resume` | repointed | Estelle `/session` |
| `/work` | repointed | Estelle `/work`, with local apply remaining explicit |
| `/orchestra` | repointed | Estelle `/orchestra`; no second client orchestrator |
| `/context` | ported | persistent Repo graph / Working-memory side panel; `Alt+M` uses the same binding |
| `/gate` | repointed | local measured diff to Estelle `/gate` |
| `/scan` | repointed | local measured diff to Estelle `/scan` |
| `/improve` | repointed | Estelle `/improve` |
| `/verify` | repointed | Estelle `/verify` |
| `/apply` | ported | explicit local application of the last server diff |
| `/undo` | ported | reverses only the last explicit `/apply` |
| `/mode` | repointed | local ceiling under Estelle `/autonomy/scope` |
| `/routing` | repointed | Estelle `/route` policy explanation |
| `/status` | ported | local/remote connection and ownership state |
| `/skills` | repointed | Estelle `/skills` |
| `/tools` | repointed | Estelle `/mcp` `tools/list` |
| `/shell` | ported | documents the explicit local `!command` surface |
| `/clear` | ported | local transcript only |
| `/exit` | ported | TUI lifecycle |

## Inherited slash commands (62/62)

| command | disposition | owner / reason |
|---|---|---|
| `/prod` | ported | off-by-default live `/monitor/issues` + `/monitor/overview` production-health pane |
| `/todo` | ported | server-emitted session task ledger; `/todo` toggles visibility and `Ctrl+T` expands/collapses without discarding completed results |
| `/model` | repointed | `/providers`; displays configured BYOK models without key material; refuses fake session pinning |
| `/plan` | ported | Kimi interaction over the server autonomy ceiling and router |
| `/memories` | repointed | Estelle `/deep-search`; Codex local memory generation is not reachable |
| `/mcp` | repointed | Estelle `/mcp` catalog |
| `/grep` | repointed | structural Estelle `/search`, exact/approximate lines disclosed |
| `/ide` | repointed | top-level editor MCP configuration |
| `/permissions` | repointed | effective local/account autonomy boundary |
| `/keymap` | repointed | maintained composer; no persisted Estelle keymap yet |
| `/vim` | repointed | maintained composer; no persisted Estelle Vim setting yet |
| `/setup-default-sandbox` | deleted | repair sandbox is server-owned |
| `/sandbox-add-read-dir` | deleted | client may not widen server or repo scope |
| `/experimental` | deleted | removed OpenAI agent feature brain |
| `/approve` | deleted | approval is Estelle server policy |
| `/import` | repointed | session server has no cross-harness import endpoint yet |
| `/hooks` | ported | canonical Rust Estelle hooks |
| `/review` | repointed | measured diff to Estelle `/gate` |
| `/rename` | repointed | session mutation has no accepted server endpoint |
| `/new` | repointed | current TUI session; no duplicate local agent session |
| `/archive` | repointed | session mutation has no accepted server endpoint |
| `/delete` | repointed | session mutation has no accepted server endpoint |
| `/fork` | repointed | session mutation has no accepted server endpoint |
| `/app` | repointed | fatelabs.ca; no implicit browser launch |
| `/compact` | repointed | server memory; no fake client summary |
| `/goal` | repointed | server sessions; goal mutation has no accepted endpoint |
| `/agent` | repointed | typed fleet view is ready; production `/orchestra` still lacks the revisioned live-state wire |
| `/side` | repointed | no server-owned ephemeral fork |
| `/btw` | repointed | no server-owned ephemeral fork |
| `/copy` | repointed | terminal-native selection |
| `/raw` | repointed | maintained terminal layer; no persisted setting |
| `/diff` | repointed | local `git diff`; nothing sent until a gate/scan/review |
| `/mention` | repointed | Working memory auto-attaches changed files; explicit mention syntax is not fabricated |
| `/usage` | repointed | account API has no accepted analytics endpoint |
| `/debug-config` | deleted | removed OpenAI local runtime |
| `/title` | repointed | maintained terminal layer; no persisted setting |
| `/statusline` | repointed | maintained terminal layer; no persisted setting |
| `/theme` | repointed | maintained terminal layer; no persisted setting |
| `/pet` | deleted | decorative local-agent state |
| `/apps` | repointed | server integrations have no matching accepted endpoint |
| `/plugins` | repointed | server integrations have no matching accepted endpoint |
| `/logout` | repointed | Estelle credential store, never OpenAI auth |
| `/feedback` | deleted | OpenAI feedback transport removed; no replacement endpoint |
| `/rollout` | deleted | removed OpenAI local runtime |
| `/ps` | deleted | removed OpenAI local runtime |
| `/stop` | deleted | removed OpenAI local runtime |
| `/personality` | deleted | response policy is server-owned |
| `/test-approval` | deleted | approval is server-owned |
| `/subagents` | repointed | typed fleet view is ready; production `/orchestra` still lacks the revisioned live-state wire |
| `/debug-m-drop` | deleted | Codex local memory brain removed |
| `/debug-m-update` | deleted | Codex local memory brain removed |
| `/version` | ported | local Estelle binary version |
| `/editor` | repointed | maintained terminal/editor handoff; no persisted setting |
| `/changelog` | repointed | public release owns metadata; TUI does not fetch it |
| `/add-dir` | deleted | one explicit repository root; no silent scope widening |
| `/export` | repointed | server sessions have no accepted export endpoint |
| `/task` | repointed | typed fleet view is ready; production `/orchestra` still lacks the revisioned live-state wire |
| `/web` | repointed | fatelabs.ca; no implicit browser launch |
| `/vis` | deleted | Kimi local trace visualizer has no Estelle owner |
| `/upgrade` | repointed | public npm release owns upgrades |
| `/yolo` | deleted | contradicts server autonomy ceiling |
| `/afk` | deleted | contradicts server autonomy ceiling |

Dynamic `/skill:<name> <task>` is ported to Estelle `/skill/run`; `/odel` is asserted to resolve to
`/model`. Shift-Tab toggles plan/edit locally without posting a policy mutation.

## Ownership corrections found during P5

- ACP is an interoperability transport, not subscription authentication. Goose's maintained ACP
  implementation still selects a provider and reads that provider's API key; Estelle therefore keeps
  ACP and credential resolution as two explicit doors. See `docs/adr/0017-acp-is-interop-not-subscription-auth.md`.
- `/providers`, not `/account`, is the BYOK pool read surface.
- The accepted server has no session-scoped model override. `/model <name>` therefore refuses to mutate
  the account-wide provider selection and leaves auto routing active.
- Working memory is the one local capability without a server owner. It is session-private and collects
  only changed, staged and untracked source files from Git; ignored files, symlink escapes, unsupported
  extensions and credential-shaped content remain excluded. Since D1 (2026-08-07, see
  `docs/2026-08-07-D1-answer-pipeline.md`) it rides the single `/deep-search` call as model input and is
  disclosed from the typed `working_paths` field — never spliced into the transcript answer.
- `codex-memories-extension` is installed only by the preserved app-server binary
  (`app-server/src/extensions.rs`), not by the Estelle TUI binary. The customer-reachable memory commands
  are repointed to Estelle. The inherited local backend remains a P0 compile exception, not a shipped
  Estelle memory owner.

## 2026-08-07 — the DROP batch: 22 Codex-only names removed outright

Per the founder's slash-command audit, these graft stubs are gone entirely — not stubbed with an
explanation, REMOVED: `pet vim theme statusline title raw copy mention ide apps plugins
experimental app import logout rollout debug-config test-approval debug-m-drop debug-m-update
setup-default-sandbox sandbox-add-read-dir`. Their names never resolve: a `DROPPED_COMMANDS`
guard runs before the one-edit typo matcher, which otherwise guessed wrong neighbors (`/vim` →
`/vis`). Unknown commands send zero requests. Two stub arms that were restored after the batch
because their names stay: `/feedback` (stub text updated to describe the removed transport) and
`/personality` (COLLIDES list, pending a founder decision). Also fixed: `/init`'s inherited
description said "instructions for Codex" — wrong product name in a user-facing string.

One find worth recording: the `/usage` graft stub ("not in the accepted contract") was SHADOWING
the real `GET /usage` wire — `handle_local_command` consults graft dispositions before remote
routing. The stub is deleted; `/usage` reaches the server now, and a test pins that no wired read
may ever fall back to a graft stub again.

## Measurements

- `Cargo.lock`: **1,311 packages**, up 4 from P4's measured 1,307. ACP introduced published protocol
  crates; RMCP was already in the preserved Codex graph.
- Cargo workspace: **124 packages**, up 2 for `estelle-acp` and `estelle-mcp`.
- Direct TUI deletion exceptions from P0: **19**, unchanged. P5 did not make the six direct TUI
  dependencies or their retained transport closure deletable.
- Reachable slash denominator: **85** static commands (23 Estelle + 62 inherited), plus the dynamic
  `/skill:<name>` namespace.

## Red-before-green evidence

Each new guard was deliberately made wrong before its passing result was accepted:

- ACP advertised `load_session`; the capability-contract test failed, then passed after the unsupported
  capability was removed.
- MCP omitted the scoped `repo`; the forwarding-contract test failed, then passed after absent repo
  injection was restored. An explicit caller repo is still never overwritten.
- Working memory used a naive source inventory; its test exposed an unchanged tracked file, then passed
  with changed, staged and untracked Git inventories only.
- Shift-Tab was matched as the wrong key event; the plan-toggle test failed, then passed on
  `KeyCode::BackTab`.
- The model renderer emitted `configured_keys`; the credential-shape test failed, then passed with an
  allowlisted provider/model projection.
- The elapsed state claimed the server cache was cold without evidence; its wording test failed, then
  passed with the measured statement that prelude/cache *may* be cold.
- All five Ratatui `.snap.new` frames were read before acceptance. The slash-menu fixture and 24-row
  viewport defect were repaired before the reviewed baselines were accepted; no `.snap.new` remains.

## Acceptance evidence

- `cargo test -p estelle-client`: **17 passed**, 1 credentialed production test intentionally ignored.
- `cargo test -p estelle-acp`: **2 passed**.
- `cargo test -p estelle-mcp`: **2 passed**.
- `cargo test -p estelle-tui --bin estelle`: **94 passed**, including all existing snapshots.
- Strict Clippy with `-D warnings` passes for all four Estelle crates, and scoped `cargo fmt --check`
  passes.
- The optimized ACP adapter completed real protocol-v1 `initialize` and `session/new` exchanges. Its
  wire response identified `estelle 0.2.4`, returned a repository session ID, and advertised every
  unsupported prompt, load and MCP capability as false.
- The Estelle MCP client completed a real stdio handshake with Codex's maintained
  `test_stdio_server`, discovered **11 tools**, called `echo`, and received structured content
  `ECHOING: p5`.
- `cargo build --release -p estelle-tui --bin estelle` passes. The optimized binary launched in a PTY,
  rendered `/help`, and exited through `/exit` with status 0.
- `cargo-watch 8.5.3` is installed; `docs/DEVELOPMENT.md` records the one-command rebuild/relaunch loop
  and the deliberate `cargo insta review` workflow.
