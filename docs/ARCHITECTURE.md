# Estelle CLI architecture

**Owner:** CLI lane. **Repository boundary:** this document owns the architecture of the separate
`uqeu/estelle-cli` repository. The parent repository may index or summarize it, but cannot be the only
copy: a behaviour change and its architecture record must be reviewable in one CLI commit.

**TODAY — source + public probes, 2026-08-21:** candidate-first release run `32500974705` built all four
native targets before creating `v0.2.28`. Remote ref read-back peeled the tag to exact candidate
`7cd03c283f4fc3507b233ec8bb04263de47d700e`; GitHub returned nine non-draft/non-prerelease assets; public
installation printed `estelle 0.2.28`; and independent npm view/pack returned 0.2.28 with all four customer
files byte-identical to `npm-shim/`. npm accepted publication at 17:22:24Z but exposed it at 17:27:36Z, so
the run's one-minute registry read-back failed honestly; the source verifier now allows five minutes. The
separate public-install and full public-binary receipt jobs were still running at this read-back and are not
claimed here. Earlier v0.2.9–v0.2.11 session, file-shift, and provider-boundary receipts remain historical
evidence rather than being silently attributed to 0.2.28.

## System boundary

```mermaid
flowchart LR
    Human[Developer] --> TUI[estelle connect · terminal client]
    TUI <--> Session[estelle serve · session owner]
    Editor[Claude Code / Codex hooks] --> CLI[estelle binary]
    ACP[ACP-capable editor] <--> CLI
    Session --> API[api.fatelabs.ca]
    CLI --> Model[User-selected model provider]
    CLI --> API
    API --> Memory[Estelle memory / graph / gates]
    Release[GitHub Release] --> Installer[checksummed installer]
    Installer --> CLI
    NPM[@fatelabs/estelle shim] --> Installer
```

The binary is the only Estelle door that can spend a customer's local ChatGPT-plan OAuth credential.
Estelle's server receives grounding and product requests; the selected model provider performs inference.
The CLI must not silently introduce a third destination.

### Plan-local model routing

The ACP ChatGPT-plan path reads the provider's authenticated `GET /models` result once per session and
sends only those visible model slugs plus the current user message to Estelle `POST /route`. Estelle returns
the model, tier, effort, and reason; the CLI verifies the model is in the authenticated list, applies the
returned effort, and spends the plan credential only against `chatgpt.com/backend-api/codex`. The route
request contains the Estelle key, repository, model names, and user message; it never contains the plan
access or refresh token. The final local receipt names the selected model, tier, and bounded/redacted reason.

If the provider model census fails, routing fails, Estelle declines, or Estelle names a model outside the
list, the CLI retains the provider's existing default and says why in the receipt. A bundled catalog is a
runtime fallback, not proof of plan entitlement, so its models are never advertised as `available`.

**Current limit:** this bridge is built and locally tested only for the ACP ChatGPT-plan runtime. Claude
subscription, Kimi, local OpenAI-compatible runtimes, and the custom TUI conversation path do not consume
this route yet. The `v0.2.28` candidate carries the bridge, but only remote publication read-back can make
that a public-artifact fact; this machine has no ChatGPT-plan credential, so no real-plan receipt is claimed.

## Entrypoints and owners

| entrypoint | implementation owner | contract |
|---|---|---|
| Interactive terminal | `tui/src/main.rs`, `tui/src/lib.rs` | Ratatui work surface, local approvals, grounding views, and server-backed Estelle commands |
| Session owner | `tui/src/session_server.rs` | Long-lived questions, remote commands, sweeps, typed results, progress, cancellation, reconnect replay, and bounded same-repository file-shift notices over an owner-only local socket |
| Headless commands | `tui/src/top_level.rs` | Login, sweep, hooks, MCP, ACP, settings, and explicit one-shot operations |
| Typed Estelle transport | `estelle-client/` | Endpoint inventory, request/response types, auth store, cancellation, bounded timeouts, and redaction |
| ChatGPT plan login | `login/`, `estelle-client/src/auth_record.rs` | Device flow and refresh rotation; ChatGPT credentials do not enter the Estelle credential store |
| Credential onboarding | `tui/src/provider_catalog.rs`, `tui/src/provider_store.rs`, `tui/src/main.rs`, `tui/src/login.rs`, `tui/src/claude_import.rs`, `tui/src/copilot_login.rs`, `tui/src/local_provider.rs`, `tui/src/provider_keys.rs`, `tui/src/doctor.rs` | One provider-data catalogue drives first-run/shell/slash routing, masked input, shared private local-store transactions, presence-only diagnostics, and separate logout radii |
| ACP adapter | `estelle-acp/` | Editor session protocol backed by the user's selected model credentials |
| MCP adapter | `estelle-mcp/` | MCP-facing Estelle catalogue; client-provided MCP servers are deliberately rejected |
| Always-on hooks | `tui/src/top_level.rs`, generated host configuration | One Rust owner generates Claude Code and Codex hook tables; PostToolUse read/edit activity feeds file-shift tracking while Python/Rust decisions remain contract-pinned |
| Public distribution | `.github/workflows/release.yml`, `install.sh`, `npm-shim/` | Explicit SemVer candidate to four native archives, then an exact-SHA immutable tag, checksums, provenance, GitHub Release, npm retirement shim, and independent registry artifact read-back |

## Server-owned sessions

`estelle serve` is the only process in the split that resolves the Estelle credential. It owns named
sessions per canonical working tree and repository, keeps each transcript plus active cancellation token
in memory, and performs questions, remote slash commands, and sweeps. `estelle connect` opens the Ratatui
client without touching the credential store. Closing that terminal drops only its socket: the server task
continues, records its typed result, and replays completed and still-active work to the next client.

The local rendezvous defaults to `~/.estelle/session.sock`. Its directory is forced to mode 0700 and the
socket and startup lock to mode 0600 on Unix. A held process lock prevents a competing server from reaping
or replacing a live socket; a stale socket can be removed only while the next server owns that lock.
Frames are newline-delimited typed JSON over the repository's cross-platform UDS layer. Request IDs use a
random client seed and are rejected if repeated within a session.

The design follows the server/client boundary documented by jcode (MIT): the server, not a terminal,
owns work and broadcasts lifecycle events to any attached client. Estelle does not port jcode's provider
brain. Its session owner calls the existing typed Estelle API paths and preserves the server's grounding,
gate, retrieval, and product ownership.

`estelle connect --session NAME` creates or attaches a validated named session. The server returns the
same-repository session catalogue with every snapshot; the TUI renders it as a tab row, marks active work,
switches the watched transcript with Alt+Left/Right (with Ctrl+Tab accepted where terminals report it), and
closes only the local tab with Ctrl+W. Switching creates
a new server session when the name is not present and never cancels the session being left. Affinity/
Orchestra worker registration and durable restart recovery remain ordered work. Explicit `!` local shell
commands and patch application remain terminal-owned because they mutate the attached working tree; they
are never presented as detachable server work. Shell commands have a 30-second deadline and a shared
64-KiB stdout/stderr capture ceiling, render as a distinct command transcript entry, and are killed and
reaped on timeout. The deadline is visible before and during execution and can be set from 1–1,800 seconds
with `ESTELLE_SHELL_TIMEOUT_SECONDS`; an absent, zero, malformed, or out-of-range value retains the measured
30-second default. Those bounds prove containment, not command safety: the human typed the command and it
does not pass through Estelle autonomy.

### Adopted terminal surfaces and live work

The Estelle binary uses the retained upstream `bottom_pane` composer as its sole input surface, feeding it
Estelle's command catalogue instead of rebuilding chrome in `main.rs`. `tui/src/transcript.rs` adapts
application semantics onto the retained history-cell renderer: file paths, commands, symbols, and links
receive a theme-safe semantic blue, and shell output is a visually distinct, collapsed `▸` row expanded by
clicking that exact row. Adjacent equal-style markdown spans are coalesced after wrapping so adoption does
not fragment plain-text selection/search. The server `/sessions` result opens the retained resume picker and submits
only the selected server-returned session id; an empty result has no selectable action. The discarded pets,
OSS-selection, model-migration, and debug-config product surfaces are absent, and the library target is
`estelle_tui`. Upstream protocol literals that identify the Codex wire format remain unchanged because a
product rename is not authorization to fork an external protocol.

There is no context-window percentage bar. `/compact` sends the masked caller-owned journal to Guardian's
`POST /govern` compact mode and treats HTTP 200 as transport evidence only. Blocked and unchanged receipts
must return an identical `governed` projection and retain their generation; compacted receipts advance by
exactly one and replace the local journal. Missing, malformed, or contradictory projections fail closed.

`/work` is a durable operation: a 202 receipt names a caller-bound `job_<24 lowercase hex>` locator, then
both standalone and attached-session clients poll `GET /jobs/{id}` until its remote terminal state. Each
whole progress snapshot contains a strictly increasing revision plus the measured phase tally and the
server-owned `work.label`. The TUI renders that label verbatim; it owns no phase-to-copy dictionary. An
older server that omits the additive label retains the raw `last measured <phase>` display. The TUI
accepts only non-regressing snapshots in the server-owned order `scope → recall → conventions → prompt →
implement → gate`, prints measured elapsed seconds, and names how long no newer phase has arrived. It never
derives a percentage or ETA from those boundaries. A malformed locator is refused before transport; an
unknown phase, unknown tally key, repeated revision, or backwards phase leaves the last valid display
unchanged. Completion and cancellation remove the live row rather than leaving a stale spinner behind.
The terminal result carries a typed server-owned completion receipt: elapsed seconds, RFC 3339 finish
time, vendor-list spend with its upper/lower-bound labels, and the number of gate findings that refused
the proposal. The TUI ends the transcript with that receipt. An unpriced model says `spend unknown`, and
an older server with no receipt gets no client-timed substitute; the terminal never owns a second clock.

`tui/design-book-prototype.html` is the disposable design book for the 15 named customer surfaces. It owns
three selectable variants per surface, uses the live cream/ink/red palette, includes explicit failure
states, and uses no fabricated metrics. It is review input, not a runtime dependency; adoption proceeds one
replacement at a time and the executable tests remain the wiring evidence.

The installed Claude Code and Codex `PostToolUse` table reports `Read`, `Write`, and `Edit` activity through
the same socket. Read sets are kept by the server, capped at 4,096 repository-relative paths per session.
When another named session under the exact same repository and root reports a change to one of those paths,
the server stores and broadcasts a notice to the reader; each reader retains at most 64 notices. A detached
reader receives the pending notice in its next snapshot, while an attached TUI renders `FILE SHIFT` and
acknowledges it. The headless hook returns the same warning as model context and acknowledges it after
delivery. Absolute paths are stripped only when they are under the session root; traversal, outside-root,
and empty paths are rejected. Tool contents and credential values are never included—the summary is only
the completed tool action.

This mechanism follows the reader-after-edit behavior documented at `jcode.sh/swarm` and the bounded
server-owned activity architecture in jcode's MIT-licensed `jcode-swarm-core` and `FileTouchService`.
Estelle does not claim an exact port: the current vendored jcode `latest_peer_touches` predicate selects
modifying peers, while the requested Estelle invariant explicitly selects prior readers. The red test pins
that stronger behavior and the implementation is original Apache-2.0 Estelle code. It does not yet include
jcode's direct-message, broadcast, heartbeat, roster, or Orchestra worker-registration surfaces.

## Release pipeline

`workflow_dispatch` supplies the intended SemVer, never an already-created tag. For a new release the
workflow requires the candidate SHA to be current `main`; for a resumed release it requires an existing tag
to peel to that exact SHA, even if `main` has since advanced. The candidate version must exactly equal both
the Cargo workspace version and npm shim version. Validation then runs four independent gates before any
platform build:

1. The shell installer must install all four declared target shapes; print resolved-path, final-version,
   and exact zsh/bash PATH guidance without silent profile mutation; resolve a bare command from a clean
   shell after that guidance is applied; and refuse malformed repository, malformed version, checksum,
   archive-member, and wrong-PATH mutants.
2. Release archives must reproduce byte-for-byte with normalized metadata.
3. `fork-manifest.yaml` must prove the pinned upstream tree, the imported tree, audited ancestry, every
   high-risk blob since that audit, and the finite egress census.
4. Formatting, warning-denied clippy, and locked client/TUI tests must pass in the standalone public repo.

Only then do target-native runners build macOS arm64, macOS x64, Linux x64, and Linux arm64 under a finite
120-minute budget derived from the measured Intel tail. Each runner checks the object-file architecture and
executes `estelle --version`. After all four succeed, the release job creates the annotated exact-SHA tag,
packages exactly one binary per archive, writes `SHA256SUMS`, attests every downloadable artifact with
GitHub OIDC provenance, and creates or verifies the versioned GitHub Release. An exact tag/release can resume
idempotently after a downstream outage. There is no unsigned fallback.

The install script downloads the checksum manifest before the selected archive, validates an exact manifest
entry, hashes the archive, rejects any member set other than one regular `estelle` file, and atomically
installs it. It then resolves the destination, runs the installed binary for the final version line, and
checks the destination against `PATH`. If absent, it prints an exact export for `.zshrc` or `.bashrc`, offers
an interactive append through the controlling terminal, never edits without a yes response, and says that a
new shell is required. The public command is:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

The legacy npm package is not allowed to keep executing abandoned JavaScript. Each published version is a
small compatibility launcher: its postinstall downloads the same exact-version native archive, accepts only HTTPS
GitHub/release-asset redirects, bounds manifest/archive/redirect resources, verifies the checksum and member
set, and exposes only the verified binary. Its workflow publication is provenance-signed and runs only after
the GitHub Release job. Publication success is read back from the registry independently: `npm view` must
return the exact version, then `npm pack` must yield customer files byte-identical to the source shim. The
workflow's own exit code is not publication evidence.

## Trust and egress boundaries

`fork-manifest.yaml` records the upstream Codex import and hashes every reviewed high-risk delta after the
audit checkpoint. `docs/egress-sinks.toml` is the finite sink register. The release gate currently expects
17 released and 5 latent entries and fails if a source symbol disappears or a primitive census changes.
This is a source census, not a process-tree network proof; runtime canaries remain explicitly open.

The released product may send customer data only to Estelle or a provider the customer selected. Local shell
execution is an explicit user-controlled capability, not an allowlist for hidden product egress. The TUI's
inherited OpenAI announcement fetch, npm update check, feedback upload, sharing transport, telemetry setup,
and remote catalogue initialization have been removed from the released Estelle entrypoint. The installer
itself contacts GitHub solely to acquire public release bytes and sends no repository contents.

The downloadable binary is a readable customer artifact, so the server/CLI ownership line is also enforced
on the artifact rather than trusted as a source-layout convention. `scripts/check-ip-boundary.py` reads one
regular binary under a named 512 MiB ceiling and rejects the server-owned Python symbol prefixes
`estelle.serve` and `estelle.agent`, plus Rust module symbols for a `ranker`, `scorer`, `judge`, or `chunker`
even when no Python package marker is present. It matches implementation-shaped, length-prefixed module
segments rather than prose or dependency function suffixes. Every target-native release build crosses this
gate before packaging; `scripts/test-ip-boundary.py` plants one mutant for each forbidden implementation
category, proves each is rejected, and proves an incidental dependency suffix remains legal. All four target-native
`v0.2.11` builds crossed the guard, and the separately downloaded, shell-installed, and npm-installed arm64
bytes passed the original prefix gate again. The expanded gate applies to the next release. This proves the
named package and implementation boundaries, not the absence of every conceivable proprietary byte pattern.

The release workflow's public receipt job starts only after GitHub has published the release. On a fresh
runner it fetches `install.sh` from that release's public URL, requires bare `estelle` to resolve to the
new native binary, checks the exact SemVer, and clones `Shubhamsaboo/awesome-llm-apps` rather than using a
fixture repository. The receipt records the clone's measured Python and TypeScript file counts and fails
unless both are at least 100. `scripts/public-binary-receipts.py` then opens the installed TUI in a real
pseudo-terminal once per audited read surface: the 22 route-coverage additions plus the pre-existing
`/init` and `/sessions` paths. Every receipt records what was typed and the rendered server result. An
unreached surface, auth rejection, HTTP error, timeout, network failure, or client decode failure fails the
whole job; no surface can be recorded as skipped. A non-conversational repository question also crosses
the same installed TUI seam and records its grounded production answer. The exact JSON is attached to the
public release and linked from its release notes. This 24-read slice is wired for the next release; it is
not a shipped claim until that public job passes.

The same report removes the receipt credential from one child process and requires the first-run
`CONNECT ESTELLE` picker to render numbered Estelle-account and Claude-subscription choices. The receipt
then presses `1` and passes only when that digit directly activates the `Estelle key:` prompt, proving the
advertised no-arrows/no-Enter path rather than treating visible badges as wiring. It also runs
`estelle memory forget receipt-sentinel` without `--yes` and passes only when the installed binary names
the account-wide erasure radius, gives the explicit confirmation remedy, and says that nothing was sent.
In a separate isolated child home, a non-secret intentionally invalid sentinel crosses the production
API: repeated background-route rejections must leave its local fixture present and say `NOT removed`;
only a subsequent `/me` rejection may remove it, after the installed TUI names both distinct routes.
The recorder structurally omits authorization headers, and the report contains only retained/removed state.
The cloned repository carries one harmless whitespace change to its smallest tracked `package-lock.json`;
the installed TUI must drive both `/review` and `/scan` successfully against that measured diff. This
keeps deep review and the whole-lockfile attachment path in the public receipt rather than accepting their
wiremock proofs as production evidence.

The receipt job explicitly sets `ESTELLE_RECEIPT_PATH`, which activates a fail-closed recorder at the
shared `estelle-client` HTTP seam. Each JSONL row contains method, route, query/body, status, and decoded
response. Headers are structurally absent, sensitive key/token fields and secret-shaped strings are
redacted before the append, and recorder I/O failure fails the customer request. The outer harness embeds
those sanitized rows in the release receipt and requires the grounded question to remain byte-identical
with working memory as a separate data object, `/review` to send `deep: true`, and `/scan` to carry the
full changed lockfile rather than only its diff hunk. A separate `hi` turn must still cross `/deep-search`
with the question byte-identical while omitting `working_memory` entirely, proving the conversational
bandwidth gate from the installed client rather than inferring it from a production answer.

Interactive-skill receipts keep two `/skill:grill-me` turns inside one installed TUI process. The screen
receipt records both returned frames and the single spawned process; the HTTP proof requires the first
request to omit invented history and the second request's `messages` to equal the first user turn, the
exact assistant reply returned by production, and the second user turn in order.

The dropped-command receipt drives all 26 Codex-only or colliding names through one installed TUI process.
Every name must return the exact local `nothing ran and nothing was sent` refusal, including `/vim` rather
than its fuzzy `/vis` neighbor, and the sanitized HTTP trace line count must remain unchanged across the
whole sequence. One conversation-wide deadline and bounded PTY cleanup keep that negative proof fail-closed.

Graph-currency receipts exercise all three transport branches from the same installed binary and clone:
a bounded subdirectory sweep takes the synchronous `/sync` path, the full real-size repository sweep takes
`/ingest/start`, and `estelle reindex` sends the measured lockfile change. The HTTP proof passes only when
each request carries a 40-character hexadecimal `head`; a successful screen message without those three
wire facts is a failure.

Hook receipts first run `estelle install-hooks`, then drive all 10 rows in the current canonical table with
safe synthetic host envelopes: two PreToolUse rows, three PostToolUse rows (including file shift), the
Stop/PreCompact/SessionEnd checkpoints, SessionStart, and UserPromptSubmit. Each row records its stdin and
output even when the host contract is intentionally silent. The HTTP proof additionally requires the
ground `/verify`, sync `/reindex`, all three distinct checkpoint event values, and context `/search` calls
to return successfully; local guard, distil, shift, and welcome behavior cannot be substituted for those
server-bound facts.

Secrets are refused or redacted before Estelle request construction. Stored ChatGPT OAuth material and
Estelle API credentials have separate typed records and stores. A single remote auth rejection is evidence,
not permission to destroy a stored credential.

## Credential onboarding and provider boundary

Credential absence is resolved before the first interactive frame. With no Estelle credential, the TUI
first asks the identity question: one `Connect Estelle` row obtains the Estelle key. After success, a
separate `Choose how model tokens are paid` surface offers Claude subscription, provider API key, local
model, or GitHub Copilot. `/login` reopens that two-step flow. Cancel is a decision and closes the surface;
rejection may reopen the relevant identity step. Repair advice has one context owner: TUI failures name
`/doctor` and shell failures name `estelle doctor`. The explanatory billing sentence renders above each
picker rather than beside a choice.

The stores and runtimes are deliberately distinct:

- Estelle credentials authorize grounding/product requests and provider-key storage on `POST /key`.
- ChatGPT device-flow acquisition is not exposed. The inherited flow presented Codex's first-party OAuth
  client id rather than an Estelle-owned id. Legacy credentials under `~/.estelle/chatgpt` remain detectable
  and removable so an upgrade does not strand a secret, but no login surface creates one.
- Claude subscription acquisition starts at the authenticated server-owned `POST /oauth/start` door. The
  CLI accepts only an HTTPS authorization URL, opens it in the user's browser, and polls `/account` until
  `uses_plan: true` reads back; a browser launch alone is never success. The retired Claude Code import is
  no longer callable. Its old Estelle-owned snapshot remains detectable and removable so upgrades do not
  strand a local secret.
- `tui/src/provider_catalog.rs` is the single owner of canonical ids, globally unique aliases, acquisition
  kinds, picker surfaces, server identities, endpoint defaults, and base-URL requirements. Shell and slash
  commands and both provider sub-pickers resolve through that same table.
- `claude` means server-held Claude subscription OAuth and `anthropic`/`anthropic-api` mean an Anthropic API
  key. `openai-api` means an OpenAI API key; `chatgpt` is deliberately unresolved. This keeps plan OAuth
  distinct from API-key wire identity before a credential is requested.
- Gemini, Azure, Bedrock, OpenRouter, DeepSeek, Fireworks, and MiniMax use the one masked provider-key path.
  Azure prompts for its non-secret API base first. Unknown providers and unsafe public HTTP bases fail
  before a secret prompt or network request.
- Copilot uses an explicit GitHub device flow with bounded requests/polling and a mode-0600 Estelle-owned
  token snapshot. Presence does not prove Copilot entitlement or model-runtime acceptance.
- LM Studio and Ollama supply their local defaults; a custom OpenAI-compatible provider prompts for its API
  base. HTTP is accepted only for loopback, private, link-local, CGNAT, or `.local` hosts. Those hosts may
  omit a key; a remote HTTPS endpoint may not. Endpoint/key metadata stays client-side in a mode-0600 file.
  Login immediately calls the bounded `/models` binding probe. A data-array response is bound; 401/403 is
  refused; other non-2xx, malformed 2xx, and unreachable endpoints remain distinct failures because their
  remedies differ.
- `/whoami` renders only credential presence and server-returned provider names. `/logout` removes local
  Estelle/plan/Copilot/endpoint stores but never silently deletes server-owned provider keys.

The local/custom endpoint is now stored, read back, and probed during login and doctor; that proves endpoint
binding only, not model spending by the answer runtime. Claude OAuth proves the server account binding, and
Copilot proves GitHub device authorization, but neither proof alone makes a model request. The custom Estelle
conversation still uses the server answer path. The missing runtime contracts and acceptance proof are
recorded in `docs/SERVER-CONTRACTS-NEEDED.md`; import, authorization, and endpoint census are not styled as
completed inference.

The update notifier never executes an installer and never prints executable `curl | sh` advice. A behind or
explicitly unanswerable check links to the human-inspectable latest-release page. The public installer still
exists as a separately documented distribution surface; linking to it is not evidence that an update ran.

## Cross-repository contracts

The Rust hook implementation must agree with the parent Python implementation on guard, distil, returning
brief, repository naming, sync refusal, grounding verdict, and verify-request behavior. A GitHub release
checkout intentionally lacks that separate repository, so each contract carries explicit Python-produced
outputs as a non-optional oracle. The returning brief records 19 SHOW/SILENT decisions; the remaining tests
record every named fixture's typed result, exact text, request field set, or refusal. In the source-of-truth
checkout the live Python implementation must also reproduce those recorded outputs. A missing parent checkout
can no longer turn a release test into an environment failure or a vacuous pass.

The server owns memory, graph, gate, entitlement, and account facts. The CLI renders typed responses and
must preserve absent/unknown states; it does not infer a clean verdict from a missing field, elapsed time, or
an HTTP completion. The CLI owns local filesystem inventory and is therefore still the only secret filter on
uncommitted sweep contents. Its Git inventory uses Git's exclude-standard result even for explicitly named
inputs, so ignored secret files and ignored allowlisted source trees cannot bypass the fence; the paired
positive keeps an ordinary tracked source file in the sweep. This closes the `.gitignore` defect tracked as
#66 while retaining the architectural fact that uncommitted bytes are filtered client-side.

`/work` progress is one revisioned snapshot with two additive projections. `work` carries the six measured
phase spans and their server-authored human label; `plan` carries bounded architect steps with stable ids,
status, and an evidence string that may honestly be blank. The server publishes the plan immediately after
architecture and again after the gate.
The TUI renders that same wire state as Screen 13 with an evidence column; `— unevidenced` is visible, and a
deployment step remains `▲ protected` because `/work` proposes code and never deploys it. All status glyphs
are one terminal column. Legacy phase-only snapshots remain valid. `research` is reserved but unemitted;
the CLI neither invents that phase nor the copy `Researching the web`.

`/gate` is still a synchronous server request and exposes no server-owned progress stream. The interactive TUI
therefore reports only the two phases it can observe without guessing: `reading local diff`, then `waiting for
server verdict`, with live elapsed time. Those events cross both the standalone TUI channel and the
`serve`/`connect` session protocol. A wait beyond 30 seconds retains the observed phase while adding `still
waiting for Estelle`; it never invents which grounding or security sub-check the server is running. The headless
one-shot `estelle gate` command still waits synchronously without live status updates.

## Current limits

- A 2026-08-27 fresh-profile probe installed the public `v0.2.28` binary to an empty temporary directory in
  6.07 seconds and read back `estelle 0.2.28`. Fresh account creation then stopped honestly at the public
  boundary: `POST /signup` returned 403 in 0.364 seconds with `sign up at fatelabs.ca and verify your email
  before creating an API key`; a second probe returned the same 403 in 0.448 seconds. No founder credential was
  substituted, so auth, repo connection, first sweep, first gate, and first coding-turn timings remain
  unmeasured until a verified fresh mailbox/account is supplied. The website-owned verification flow is not a
  CLI workaround target.

- The immutable `v0.2.12` release is public at candidate commit `6fa3bf744f1d08a3cb1f2ecf3e115a2e40cfae78`.
  All four native archives, the checksum-first public installer, and npm package `@fatelabs/estelle@0.2.12`
  passed remote read-back. Fresh temporary-home installs through both the release installer and npm resolved
  bare `estelle` to `estelle 0.2.12`; the release-installed 41,510,536-byte macOS arm64 binary also passed the
  IP-boundary scan. This is SHIPPED evidence, not a production-surface receipt.
- The `v0.2.12` public-binary receipt job failed closed before its first request because the repository does
  not have `ESTELLE_RECEIPT_API_KEY` configured. No receipt asset was attached. The 24 reads, production
  answer/review/scan, graph-head paths, hook network paths, and interactive skill thread therefore remain
  not PROBED through the installed public binary. The missing external credential is not converted into a
  skip, mock, or green board row.
- The local Sigstore verifier could not initialize during the preceding `v0.2.11` probe, so external
  attestation verification is still absent even though the release workflow's signing steps passed.
- The founder's clean-machine default-shell restart and shadowed-npm-machine repair remain human/device-bound;
  the clean public install and first-run picker are independently reproduced. ChatGPT acquisition is removed,
  not awaiting a walkthrough.
- Claude OAuth is built and locally wire-tested, but staging currently returns the generic route 404 for
  `POST /oauth/start`; a staging browser callback, restart-surviving account binding, and Claude inference
  are therefore not PROBED. Copilot and local-model acquisition likewise do not prove answer-runtime spend.
- Live `/work` phase and plan rendering is built and locally tested through the durable event stream. It is
  not DEPLOYED or PROBED from a published CLI against production; the UI never invents ETA or percentage.
- Client-provided MCP servers remain deliberately rejected.
- The public server/client split passed terminal detach/reconnect and detached failure replay on
  `awesome-llm-apps`; a production-authenticated successful answer still requires a customer credential.
- Session state is in-memory across terminal detach, not durable across a server process or machine restart;
  Affinity/Orchestra worker registration is not yet wired to create the named sessions automatically.
- File-shift read sets and pending notices share that in-memory lifetime. Public URL-installed `v0.2.10`
  passed a synthetic host `PostToolUse` Read/Edit sequence around an actual disposable-file edit in a fresh
  `awesome-llm-apps` clone; an editor-generated event and durable restart remain unproved.
- Runtime process-tree egress proof, production settings/autonomy writes, and the complete ACP lifecycle
  remain open measurements.
- The binary IP-boundary invariant is SHIPPED and PROBED for its two named package prefixes; unnamed or
  differently encoded proprietary logic remains outside that deliberately narrow claim.
- Any-mode question picking, terminal Mermaid rendering, and the remaining ACP session surfaces are ordered
  product work; this document does not style their presence as shipped.
