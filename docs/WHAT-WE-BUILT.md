# WHAT WE BUILT — Estelle CLI

This is the separate CLI repository's inventory of PORTED mechanisms, OURS, and established SYSTEM DESIGN.
Every row names a source, licence, implementation location, and measured status. A removed mechanism loses
its row. A status changes only from a probe, never from recollection.

**Status:** ✅ built + wired + public-artifact/prod-proven · 🟡 built, but no public customer artifact proves
the path · ⬜ decided, not built. **TODAY — 2026-08-15:** clean run `31913589144` passed the standalone
suite and four native builds, published non-draft release `v0.2.4` with nine assets, verified all four public
archive checksums plus arm64 member/version/provenance, and published npm `0.2.4`. Release-hosted,
raw-`main` latest, and npm postinstall customer paths each installed native `estelle 0.2.4`; interactive
product paths remain 🟡 until their own probes exist.

## PORTED — mechanism, source, and licence

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Codex terminal application core: Ratatui event loop, composer, history, approvals, model execution, and local tools | OpenAI Codex commit `582569998181aad08a88bacc151a94b2048a5d1f` · Apache-2.0; exact tree and import delta in `fork-manifest.yaml` | `tui/src/lib.rs`, `tui/src/app.rs`, `core/`, `tools/` | 🟡 |
| Filled user-message block and direct number-key picker selection | OpenAI Codex at the pinned import · Apache-2.0 | `tui/src/main.rs:3313`, `tui/src/main.rs:5318` | 🟡 |
| Persistent right-side terminal surface pattern, with Estelle-owned grounding data rather than a copied agent brain | jcode `crates/jcode-tui/src/tui/ui_input.rs:2558`, `jcode-tui-render/src/chrome.rs:31`, `jcode-tui/src/tui/ui.rs:2727` · MIT | `tui/src/main.rs:1581`, `tui/src/main.rs:4214` | 🟡 |
| Three-type `Api | OAuth | WellKnown` auth record and account-id fallback chain | opencode `packages/opencode/src/auth/index.ts` and provider `openai.ts` · MIT | `estelle-client/src/auth_record.rs:1`, `login/src/token_data.rs:91` | 🟡 |
| ChatGPT device-code login and refresh behavior, kept in a store separate from Estelle credentials | OpenAI Codex device auth · Apache-2.0; opencode refresh failure behavior · MIT | `tui/src/login.rs:161`, `login/src/auth/manager.rs:185` | 🟡 |

## OURS — the product-specific half

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Typed Estelle transport with one endpoint inventory, boundary validation, secret-safe rendering, and fail-loud envelope collisions | Fate Labs · Apache-2.0 repository | `estelle-client/src/endpoint.rs`, `estelle-client/src/lib.rs`, `estelle-client/src/tests.rs` | 🟡 |
| One Rust hook owner that generates Claude Code and Codex host tables and pins five Python/Rust decisions | Fate Labs · Apache-2.0 repository | `tui/src/top_level.rs`, `tui/src/hook_distil.rs`, `tui/src/session_gap.rs` | 🟡 |
| Standalone Python/Rust release oracles: 19 returning-brief decisions plus exact guard, distil, repository, sync-refusal, grounding-verdict, and verify-request fixtures; live Python comparison when the server source exists | Fate Labs · Apache-2.0 repository; fixtures originate in parent `tests/test_hook_contract.py` | `tui/src/session_gap.rs:708`, `tui/src/top_level.rs:3196` | ✅ |
| Finite fork/egress audit: pinned upstream/import trees, reviewed risky blobs, named sinks, and primitive census | Fate Labs · Apache-2.0 repository | `fork-manifest.yaml`, `docs/egress-sinks.toml`, `scripts/check-fork-audit.py` | ✅ |
| Native retirement of the abandoned JavaScript package through an exact-version, checksum-first npm launcher | Fate Labs · Apache-2.0 repository | `npm-shim/install.js`, `npm-shim/bin/estelle.js` | ✅ |

## SYSTEM DESIGN — established mechanisms applied here

| mechanism | source / licence | implementation | status |
|---|---|---|---|
| Reproducible tag-triggered release: exact version gate, target-native builds, normalized archives, checksum manifest, and no manual bypass | Reproducible Builds principles + GitHub Actions · workflow/configuration licences apply; repository code Apache-2.0 | `.github/workflows/release.yml`, `scripts/package-release.sh` | ✅ |
| Build provenance tied to downloadable bytes through OIDC rather than a long-lived signing secret | SLSA provenance model + `actions/attest-build-provenance` · Apache-2.0 | `.github/workflows/release.yml:130`, `.github/workflows/release.yml:175` | ✅ |
| Verify before install, one-member archive, bounded resources, and atomic destination replacement | Standard supply-chain and filesystem transaction pattern · repository code Apache-2.0 | `install.sh`, `npm-shim/install.js` | ✅ |
| Separate credential domains for Estelle API access and a user's model-plan OAuth, with auth rejection treated as evidence rather than deletion permission | Least privilege + typed sum types · repository code Apache-2.0 | `estelle-client/src/auth.rs`, `estelle-client/src/auth_record.rs`, `tui/src/login.rs` | 🟡 |

## Decided, not built

| mechanism | source / licence | intended owner | status |
|---|---|---|---|
| Any-mode question picker using the inherited renderer | OpenAI Codex request-user-input renderer · Apache-2.0 | `tui/src/bottom_pane/request_user_input/` | ⬜ |
| PLAN / ACCEPT-EDITS / AUTO with ACCEPT-EDITS as the everyday default and persistent visible mode | Fate Labs product decision · repository licence when implemented | TUI collaboration-mode state | ⬜ |
| Mermaid diagrams rendered in the terminal | jcode renderer precedent · MIT | TUI markdown/render pipeline | ⬜ |
