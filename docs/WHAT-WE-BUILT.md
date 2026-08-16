# WHAT WE BUILT — Estelle CLI

This is the separate CLI repository's inventory of PORTED mechanisms, OURS, and established SYSTEM DESIGN.
Every row names a source, licence, implementation location, and measured status. A removed mechanism loses
its row. A status changes only from a probe, never from recollection.

**Status:** ✅ built + wired + public-artifact/prod-proven · 🟡 built, but no public customer artifact proves
the path · ⬜ decided, not built. **TODAY — 2026-08-16:** `v0.2.7` run `31939611428` completed successfully;
the read-back verified nine public assets, four archive checksums/member sets, arm64 identity, public
installer execution, a clean-home bare command against `awesome-llm-apps`, and the public npm shim launching
the byte-identical native `0.2.7` with SLSA metadata. External GitHub attestation verification remains
unproved because the local Sigstore verifier failed to initialize. This release adds the artifact-level
server/CLI IP boundary. Provider inference bindings are not built merely because credential acquisition
exists; those rows remain separate and honest below.

## PORTED — mechanism, source, and licence

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Codex terminal application core: Ratatui event loop, composer, history, approvals, model execution, and local tools | OpenAI Codex commit `582569998181aad08a88bacc151a94b2048a5d1f` · Apache-2.0; exact tree and import delta in `fork-manifest.yaml` | `tui/src/lib.rs`, `tui/src/app.rs`, `core/`, `tools/` | 🟡 |
| Filled user-message block and direct number-key picker selection | OpenAI Codex at the pinned import · Apache-2.0 | `tui/src/main.rs:3687`, `tui/src/main.rs:5712` | 🟡 |
| Persistent right-side terminal surface pattern, with Estelle-owned grounding data rather than a copied agent brain | jcode `crates/jcode-tui/src/tui/ui_input.rs:2558`, `jcode-tui-render/src/chrome.rs:31`, `jcode-tui/src/tui/ui.rs:2727` · MIT | `tui/src/main.rs:1919`, `tui/src/main.rs:4588` | 🟡 |
| Three-type `Api | OAuth | WellKnown` auth record and account-id fallback chain | opencode `packages/opencode/src/auth/index.ts` and provider `openai.ts` · MIT | `estelle-client/src/auth_record.rs:1`, `login/src/token_data.rs:91` | 🟡 |
| ChatGPT device-code login and refresh behavior, kept in a store separate from Estelle credentials | OpenAI Codex device auth · Apache-2.0; opencode refresh failure behavior · MIT | `tui/src/login.rs:161`, `login/src/auth/manager.rs:185` | 🟡 |
| Consent-gated Claude Code credential import: environment/Keychain discovery, wrapped OAuth parsing, refresh-token requirement, and private snapshot | jcode `crates/jcode-app-core/src/external_auth.rs:66`, `jcode-base/src/auth/claude.rs:1` · MIT | `tui/src/claude_import.rs:196` | 🟡 |

## OURS — the product-specific half

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Typed Estelle transport with one endpoint inventory, boundary validation, secret-safe rendering, and fail-loud envelope collisions | Fate Labs · Apache-2.0 repository | `estelle-client/src/endpoint.rs`, `estelle-client/src/lib.rs`, `estelle-client/src/tests.rs` | 🟡 |
| One Rust hook owner that generates Claude Code and Codex host tables and pins five Python/Rust decisions | Fate Labs · Apache-2.0 repository | `tui/src/top_level.rs`, `tui/src/hook_distil.rs`, `tui/src/session_gap.rs` | 🟡 |
| Standalone Python/Rust release oracles: 19 returning-brief decisions plus exact guard, distil, repository, sync-refusal, grounding-verdict, and verify-request fixtures; live Python comparison when the server source exists | Fate Labs · Apache-2.0 repository; fixtures originate in parent `tests/test_hook_contract.py` | `tui/src/session_gap.rs:708`, `tui/src/top_level.rs:3196` | ✅ |
| Finite fork/egress audit: pinned upstream/import trees, reviewed risky blobs, named sinks, and primitive census | Fate Labs · Apache-2.0 repository | `fork-manifest.yaml`, `docs/egress-sinks.toml`, `scripts/check-fork-audit.py` | ✅ |
| Shipped-binary IP boundary rejecting server-owned `estelle.serve` and `estelle.agent` symbols on every native target | Fate Labs · Apache-2.0 repository | `scripts/check-ip-boundary.py`, `scripts/test-ip-boundary.py`, `.github/workflows/release.yml` | ✅ |
| Server-owned detachable session: `estelle serve` + credential-free `estelle connect`, owner-only UDS, question/remote-command/sweep continuation and typed reconnect replay | jcode server/client architecture (MIT), adapted to Estelle's typed API boundary; Fate Labs implementation Apache-2.0 | `tui/src/session_server.rs`, `tui/src/main.rs` | 🟡 local source proof; public `v0.2.8` pending |
| Native retirement of the abandoned JavaScript package through an exact-version, checksum-first npm launcher | Fate Labs · Apache-2.0 repository | `npm-shim/install.js`, `npm-shim/bin/estelle.js` | ✅ |
| Credential-first launch and five-way `/login` picker, plus `/logout`, presence-only `/whoami`, and context-correct `/doctor` | Fate Labs · Apache-2.0 repository | `tui/src/main.rs:706`, `tui/src/main.rs:1362`, `tui/src/main.rs:1865`, `tui/src/doctor.rs:14` | 🟡 |
| Allowlisted provider-key login with masked input, typed `/key` request, secret-free receipt, and explicit unsupported-route refusal | Fate Labs · Apache-2.0 repository | `tui/src/main.rs:427`, `tui/src/provider_keys.rs:15`, `estelle-client/src/endpoint.rs:60` | 🟡 |
| Four-run autonomy ladder with customer label `accept-edits`, non-main branch/CI/no-merge contract, and auto's reviewable-PR fallback | Fate Labs · Apache-2.0 repository | `tui/src/commands.rs:478`, `tui/src/commands.rs:492`, `tui/src/main.rs:973` | 🟡 |

## SYSTEM DESIGN — established mechanisms applied here

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Reproducible tag-triggered release: exact version gate, target-native builds, normalized archives, checksum manifest, and no manual bypass | Reproducible Builds principles + GitHub Actions · workflow/configuration licences apply; repository code Apache-2.0 | `.github/workflows/release.yml`, `scripts/package-release.sh` | ✅ |
| Build provenance tied to downloadable bytes through OIDC rather than a long-lived signing secret | SLSA provenance model + `actions/attest-build-provenance` · Apache-2.0 | `.github/workflows/release.yml:130`, `.github/workflows/release.yml:175` | ✅ |
| Verify before install, one-member archive, bounded resources, and atomic destination replacement | Standard supply-chain and filesystem transaction pattern · repository code Apache-2.0 | `install.sh`, `npm-shim/install.js` | ✅ |
| Consent-gated zsh/bash PATH setup with resolved-path output, old-command shadow refusal, final binary identity, clean-shell bare-command proof, and native first-run PTY probe | Shell startup-file conventions + fail-closed release testing · repository code Apache-2.0 | `install.sh`, `scripts/test-installer.sh`, `scripts/probe-first-run.py` | ✅ |
| Separate credential domains for Estelle API access and a user's model-plan OAuth, with auth rejection treated as evidence rather than deletion permission | Least privilege + typed sum types · repository code Apache-2.0 | `estelle-client/src/auth.rs`, `estelle-client/src/auth_record.rs`, `tui/src/login.rs` | 🟡 |

## Decided, not built

| mechanism | source / licence | intended owner | status |
|---|---|---|---|
| Any-mode question picker using the inherited renderer | OpenAI Codex request-user-input renderer · Apache-2.0 | `tui/src/bottom_pane/request_user_input/` | ⬜ |
| Claude subscription, ChatGPT plan, and local-model credentials actually serving the custom Estelle conversation runtime | jcode provider/runtime separation precedent · MIT; repository code Apache-2.0 | provider runtime bridge | ⬜ |
| Mermaid diagrams rendered in the terminal | jcode renderer precedent · MIT | TUI markdown/render pipeline | ⬜ |
