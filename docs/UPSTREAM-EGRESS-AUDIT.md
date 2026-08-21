# Upstream fork and egress audit

Status: source census updated on 2026-08-20; runtime process-tree canary and call-site exhaustion remain
open. This document is an inventory, not a claim that the binary cannot reach anything else.

## Provenance boundary

Estelle imported the `codex-rs/` subtree from OpenAI Codex commit
`582569998181aad08a88bacc151a94b2048a5d1f` into root commit
`50edb00a709fd678875b9c65ef81f844ebf737cd`. The upstream object was fetched directly from
`https://github.com/openai/codex.git` and verified locally on 2026-08-15:

- upstream commit tree: `d766eaa8b35c98c879da6c99ef8aac81cfed5752`;
- upstream `codex-rs` subtree: `e299e81a692a6dcc2d14898b0fef30445b5a00b6`;
- Estelle import tree: `c0ce9c7d21ae63ecf8cd6675983b84221e2fcc2e`.

The import is not an identical subtree snapshot. Its first commit already differs by 124 added, 110 deleted,
and 13 modified paths. That is intentional amputation plus Estelle product work, but it means provenance is
the pinned upstream object plus an auditable delta—not shared Git ancestry and not a README assertion.
`fork-manifest.yaml` is the machine-readable authority.

## Two denominators

The finite source census is in `docs/egress-sinks.toml`:

- 17 sinks reachable from the custom Estelle entrypoint;
- 5 latent sinks retained in the inherited workspace but not initialized by that entrypoint.

Released reachability includes Estelle API startup/search/hooks and the ACP plan-route advisory, ChatGPT login and ACP, the loopback-only
token endpoint test override, the GitHub browser handoff, citation clicks, explicit shell escape, and explicit
MCP client/server commands. Latent capability includes the inherited OpenAI update checker/executor,
analytics, configurable OTLP, and plugin marketplace/remote-bundle machinery.

Sites is absent from the local source and existing Estelle artifact. That proves local absence only. Remote
plugin machinery remains in the workspace, so the stronger claim that a separately built inherited Codex
entrypoint can never receive a server-catalogued Sites bundle is not proven.

## Controls landed

- ChatGPT refresh/revoke endpoint overrides accept only exact loopback hosts; arbitrary remote hosts cannot
  receive a token through those environment variables.
- `estelle github link` accepts only `https://github.com/login/oauth/authorize`, no userinfo, fragment, or
  non-default port, and requires one non-empty client id, one state, and the exact requested loopback
  redirect before opening a browser.
- The release workflow fetches the pinned OpenAI object, verifies the manifest and census, and publishes the
  manifest plus its SHA-256 digest as attested release assets.

## Deliberately open proof

The static primitive census catches deletion or count drift in currently named browser, token-override, and
marketplace-Git seams. It is not yet a complete Rust call-graph census: a new HTTP builder, socket, browser
wrapper, or dynamically assembled subprocess can evade it.

Closure therefore still requires a fresh release artifact plus a deny-by-default process-tree network test
that exercises startup, dirty working memory, all hooks, ACP, MCP client/server, GitHub linking, citation
opening, shell escape, and the latent upstream update/plugin entrypoint. Plant code and secret canaries, and
assert exact destinations and payload classes. Until that exists, the whole-fork egress capability remains
open.
