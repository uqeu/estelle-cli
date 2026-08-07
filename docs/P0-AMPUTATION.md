# P0 amputation record

## Ancestor

- Project: OpenAI Codex
- Commit: `582569998181aad08a88bacc151a94b2048a5d1f`
- Commit date: 2026-08-01
- License: Apache-2.0

## P0 release target

`cargo build --release` builds the `estelle` binary from the preserved Codex `tui` package. The P0
binary owns the terminal through one Ratatui render loop, accepts composer input, performs no submit
action, and has no server transport.

The original Codex multitool package remains in `cli/` for later surface inventory work but is excluded
from the P0 workspace because it is not the Estelle entry point.

## Deleted in P0

These crates were on the spec's DELETE list and are outside the TUI's normal runtime dependency closure:

| crate path | reason |
|---|---|
| `app-server-daemon` | daemon executable, not used by the in-process TUI client |
| `app-server-test-client` | app-server test executable |
| `bwrap` | standalone sandbox helper |
| `cloud-tasks` | Codex cloud task UI/commands |
| `cloud-tasks-client` | Codex cloud task transport |
| `cloud-tasks-mock-client` | Codex cloud task test transport |
| `code-mode-host` | experimental code-mode host executable |
| `code-mode-runtime` | experimental host runtime |
| `core-api` | separate API wrapper around the Codex core |
| `responses-api-proxy` | OpenAI Responses API proxy executable |
| `thread-manager-sample` | sample executable |
| `v8-poc` | experimental V8 proof of concept |

## Spec corrections: requested deletions retained in P0

The spec classifies the entries below as DELETE, but this Codex snapshot's TUI normal dependency closure
contains them. Deleting them before a replacement transport exists makes P0 unbuildable; they must move
to the P1 transplant, not be hidden behind a successful P0 claim.

| requested deletion | current consumer or dependency path |
|---|---|
| `core`, `core-plugins` | `app-server-client -> app-server -> core`; `tui` directly imports `core-plugins` |
| `app-server`, `app-server-client`, `app-server-protocol`, `app-server-protocol-noop-macros`, `app-server-transport` | `tui` directly imports the client and protocol; the client embeds `app-server` |
| `cloud-config` | direct `tui` dependency |
| `chatgpt`, `backend-client`, `codex-backend-openapi-models` | retained app-server/core transport closure |
| `codex-api` | retained `otel`, model, exec-server, and transport closure |
| `aws-auth` | `tui -> model-provider -> aws-auth` |
| `network-proxy`, `execpolicy`, `linux-sandbox`, `windows-sandbox-rs`, `sandboxing`, `process-hardening` | retained protocol, config, exec-server, arg0, and TUI types |
| `ollama`, `lmstudio` | `tui -> utils/oss` |
| `analytics`, `feedback` | analytics is in the embedded core; feedback is a direct TUI dependency |
| `code-mode`, `code-mode-protocol` | retained core/tools/rollout types |
| `agent-graph-store`, `agent-identity` | retained core, login, and model-provider closure |
| `external-agent-migration` | retained app-server closure |
| `collaboration-mode-templates` | `tui -> models-manager` |

`jcode` is MIT-licensed in the vendored repository, not Apache-2.0 as the port spec states. `NOTICE`
records its actual license. No jcode source is copied in P0.
