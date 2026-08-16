# Estelle CLI architecture

**Owner:** CLI lane. **Repository boundary:** this document owns the architecture of the separate
`uqeu/estelle-cli` repository. The parent repository may index or summarize it, but cannot be the only
copy: a behaviour change and its architecture record must be reviewable in one CLI commit.

**TODAY — source + public probes, 2026-08-16:** `v0.2.6` release run `31927645013` completed successfully.
The 06:00Z read-back found nine public assets, verified all four archive checksums and one-member sets, ran
and installed the arm64 artifact as `estelle 0.2.6`, reached `CONNECT ESTELLE` from a clean-home public
install, and installed the public npm shim to the same native version. npm returned SHA-512 integrity and
SLSA v1 provenance metadata. Local GitHub attestation verification still could not initialize a Sigstore
verifier, so independent provenance read-back remains unproved outside the successful workflow. The release
adds a fail-closed installer shadow check, a real-PTY first-run gate, and credential onboarding before the
conversation surface. Local tests prove the five-row `/login` picker, masked provider-key wire request,
read-only Claude Code snapshot, `/logout`, `/whoami`, `/doctor`, and explicit provider-route refusals. They
do not prove Claude, ChatGPT, or a local model can serve the custom Estelle conversation path; those runtime
bindings remain absent and are rendered as such.

## System boundary

```mermaid
flowchart LR
    Human[Developer] --> CLI[estelle binary]
    Editor[Claude Code / Codex hooks] --> CLI
    ACP[ACP-capable editor] <--> CLI
    CLI --> Model[User-selected model provider]
    CLI --> API[api.fatelabs.ca]
    API --> Memory[Estelle memory / graph / gates]
    Release[GitHub Release] --> Installer[checksummed installer]
    Installer --> CLI
    NPM[@fatelabs/estelle shim] --> Installer
```

The binary is the only Estelle door that can spend a customer's local ChatGPT-plan OAuth credential.
Estelle's server receives grounding and product requests; the selected model provider performs inference.
The CLI must not silently introduce a third destination.

## Entrypoints and owners

| entrypoint | implementation owner | contract |
|---|---|---|
| Interactive terminal | `tui/src/main.rs`, `tui/src/lib.rs` | Ratatui work surface, local approvals, grounding views, and server-backed Estelle commands |
| Headless commands | `tui/src/top_level.rs` | Login, sweep, hooks, MCP, ACP, settings, and explicit one-shot operations |
| Typed Estelle transport | `estelle-client/` | Endpoint inventory, request/response types, auth store, cancellation, bounded timeouts, and redaction |
| ChatGPT plan login | `login/`, `estelle-client/src/auth_record.rs` | Device flow and refresh rotation; ChatGPT credentials do not enter the Estelle credential store |
| Credential onboarding | `tui/src/main.rs`, `tui/src/login.rs`, `tui/src/claude_import.rs`, `tui/src/provider_keys.rs`, `tui/src/doctor.rs` | First-run picker, masked input, provider routing, presence-only diagnostics, and separate logout radii |
| ACP adapter | `estelle-acp/` | Editor session protocol backed by the user's selected model credentials |
| MCP adapter | `estelle-mcp/` | MCP-facing Estelle catalogue; client-provided MCP servers are deliberately rejected |
| Always-on hooks | `tui/src/top_level.rs`, generated host configuration | One Rust owner generates Claude Code and Codex hook tables; Python/Rust decisions are contract-pinned |
| Public distribution | `.github/workflows/release.yml`, `install.sh`, `npm-shim/` | Exact SemVer tag to four native archives, checksums, provenance, GitHub Release, and npm retirement shim |

## Release pipeline

The tag is the release input. The workflow rejects a tag unless it exactly equals both the Cargo workspace
version and the npm shim version. Validation then runs four independent gates before any platform build:

1. The shell installer must install all four declared target shapes; print resolved-path, final-version,
   and exact zsh/bash PATH guidance without silent profile mutation; resolve a bare command from a clean
   shell after that guidance is applied; and refuse malformed repository, malformed version, checksum,
   archive-member, and wrong-PATH mutants.
2. Release archives must reproduce byte-for-byte with normalized metadata.
3. `fork-manifest.yaml` must prove the pinned upstream tree, the imported tree, audited ancestry, every
   high-risk blob since that audit, and the finite egress census.
4. Formatting, warning-denied clippy, and locked client/TUI tests must pass in the standalone public repo.

Only then do target-native runners build macOS arm64, macOS x64, Linux x64, and Linux arm64. Each runner
checks the object-file architecture and executes `estelle --version`. The release job packages exactly one
binary per archive, writes `SHA256SUMS`, attests every downloadable artifact with GitHub OIDC provenance,
and creates a versioned GitHub Release. There is no manual release path and no unsigned fallback.

The install script downloads the checksum manifest before the selected archive, validates an exact manifest
entry, hashes the archive, rejects any member set other than one regular `estelle` file, and atomically
installs it. It then resolves the destination, runs the installed binary for the final version line, and
checks the destination against `PATH`. If absent, it prints an exact export for `.zshrc` or `.bashrc`, offers
an interactive append through the controlling terminal, never edits without a yes response, and says that a
new shell is required. The public command is:

```sh
curl -fsSL https://raw.githubusercontent.com/uqeu/estelle-cli/main/install.sh | sh
```

The legacy npm package is not allowed to keep executing abandoned JavaScript. Each published version is a
small compatibility launcher: its postinstall downloads the same exact-version native archive, accepts only HTTPS
GitHub/release-asset redirects, bounds manifest/archive/redirect resources, verifies the checksum and member
set, and exposes only the verified binary. Its workflow publication is provenance-signed and runs only after
the GitHub Release job.

## Trust and egress boundaries

`fork-manifest.yaml` records the upstream Codex import and hashes every reviewed high-risk delta after the
audit checkpoint. `docs/egress-sinks.toml` is the finite sink register. The release gate currently expects
14 released and 5 latent entries and fails if a source symbol disappears or a primitive census changes.
This is a source census, not a process-tree network proof; runtime canaries remain explicitly open.

The released product may send customer data only to Estelle or a provider the customer selected. Local shell
execution is an explicit user-controlled capability, not an allowlist for hidden product egress. The TUI's
inherited OpenAI announcement fetch, npm update check, feedback upload, sharing transport, telemetry setup,
and remote catalogue initialization have been removed from the released Estelle entrypoint. The installer
itself contacts GitHub solely to acquire public release bytes and sends no repository contents.

Secrets are refused or redacted before Estelle request construction. Stored ChatGPT OAuth material and
Estelle API credentials have separate typed records and stores. A single remote auth rejection is evidence,
not permission to destroy a stored credential.

## Credential onboarding and provider boundary

Credential absence is resolved before the first interactive frame. With no Estelle credential, the TUI
opens the existing bottom-pane picker instead of accepting questions that must fail. `/login` reopens the
same surface. The five top-level choices name what they buy: Estelle grounding, Claude subscription import,
ChatGPT plan, provider API key, or local model. A failed inline flow returns to that picker and points to
`/doctor`; shell failures point to `estelle doctor`.

The stores and runtimes are deliberately distinct:

- Estelle credentials authorize grounding/product requests and provider-key storage on `POST /key`.
- ChatGPT device-flow credentials live under `~/.estelle/chatgpt`, never in Estelle's API-key record.
- Claude import occurs only after the user selects it. It reads `CLAUDE_CODE_OAUTH_TOKEN` or the macOS
  `Claude Code-credentials` Keychain item, copies a refreshable snapshot to a mode-0600 Estelle file, and
  never moves, deletes, or modifies Claude Code's source credential.
- API-key routes are allowlisted before prompting. `openai` means the ChatGPT plan; `openai-api` is the
  explicit OpenAI API-key spelling. Unknown, Copilot, Azure, Bedrock, and unconfigured local routes fail
  before a secret prompt or network request.
- `/whoami` renders only credential presence and server-returned provider names. `/logout` removes local
  Estelle/plan stores but never silently deletes server-owned provider keys.

This is onboarding and storage, not a completed provider runtime. Claude subscription tokens require the
Anthropic OAuth wire/tool schema, while the custom Estelle conversation currently uses the server answer
path. LM Studio, Ollama, and no-key localhost endpoints likewise have no binding into that path. The missing
contract and acceptance proof are recorded in `docs/SERVER-CONTRACTS-NEEDED.md`; neither import nor storage
is styled as “connected.”

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
an HTTP completion. The CLI owns local filesystem inventory and is therefore still the only secret filter
on uncommitted sweep contents. That client-side-only boundary is design limit #66 and remains open.

## Current limits

- The immutable `v0.2.6` release is public and its archive/install/first-run/npm bytes passed read-back. The local
  Sigstore verifier could not initialize, so external attestation verification is still absent from that
  probe even though the release workflow's signing step passed.
- The founder's clean-machine default-shell restart, shadowed-npm-machine repair, and ChatGPT device-flow
  walkthrough remain human/device-bound; the clean public install and first-run picker are independently
  reproduced.
- Claude/ChatGPT/local-model credential acquisition is not yet bound to the custom TUI answer runtime.
- Client-provided MCP servers remain deliberately rejected.
- Runtime process-tree egress proof, production settings/autonomy writes, and the complete ACP lifecycle
  remain open measurements.
- Any-mode question picking, terminal Mermaid rendering, and the remaining ACP session surfaces are ordered
  product work; this document does not style their presence as shipped.
