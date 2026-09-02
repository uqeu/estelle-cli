# Estelle

**The trust layer under your AI coding agents.**

Estelle grounds every model in your real codebase and refuses to ship what isn't true. It runs
underneath the coding agents you already use and gives them four things: memory of your repository,
a deterministic grounding gate, grounded review, and a propose, verify, repair loop. Model-agnostic,
bring your own key.

## Install

```sh
npm install -g @fatelabs/estelle
```

This package is a small launcher. On install it downloads and verifies the native binary for your
platform, described under [How the install is verified](#how-the-install-is-verified) below.

To install the native binary directly, with no Node involved:

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

## What you get

**Ground a question in your actual repository.**

```sh
estelle sweep                 # index this repo into your memory
estelle ask "how does billing retry a failed charge?"
```

Answers cite the files and lines they came from. When Estelle cannot ground an answer, it says so
rather than inventing one.

**Check a change before it merges.**

```sh
estelle verify src/http/headers.rs    # resolve every symbol against the repo's real symbol graph
estelle gate                          # run that check over a local diff
```

**Run on every edit, rather than when a model chooses to call a tool.**

```sh
estelle install-hooks
```

Installs ten handlers across seven events: `PreToolUse`, `PostToolUse`, `UserPromptSubmit`,
`SessionStart`, `Stop`, `PreCompact`, and `SessionEnd`. Hooks are written for both Claude Code and
the Codex CLI.

**Serve any other harness.**

```sh
estelle mcp-server            # Estelle's tools over MCP, for any agent
estelle acp                   # Agent Client Protocol, over stdio
```

`estelle --help` lists every command. A few worth knowing early:

| | |
|---|---|
| `estelle setup` | configure, brief, sweep, and prove Estelle on a real symbol from your repo |
| `estelle doctor` | credential and runtime readiness, reported without rendering a secret value |
| `estelle recall` | search your memory and your code together |
| `estelle leaked` | scan for committed credentials |
| `estelle monitor` | production health, in the terminal |
| `estelle screens` | thirteen terminal surfaces, each stamped as bounded sample state |

## What is measured

**In our Python-scoped eval, 0 invented APIs survived the gate.**

Read that precisely, because the narrow version is the one you can check:

- **What counts as one.** A code reference that names a repository symbol or import your codebase
  does not define.
- **What the gate does.** Parses the change with tree-sitter and resolves every such symbol against
  your repository's real symbol graph. No model call and no network, so the verdict is a property of
  the artifact rather than a score that moves between runs.
- **Metric level.** Candidate-level: the rate is per code reference emitted, not per task completed
  and not per answer.
- **Dataset.** Our own repository, plus 1,323 real third-party symbols across `requests`, `httpx`,
  `urllib3` and `click`: all 1,323 caught, 0 false positives. Reproduce with
  `python scripts/eval_hallucination_libs.py`.
- **Language scope.** 23 languages are supported and 12 block at some rung, but Python is the only
  one with the full guarantee of existence, arity, type, and member calls on any receiver.
  References in the 11 navigate-only languages are not gated at all. They are excluded from the
  rate rather than counted as passes.
- **What it cannot tell you.** Whether the code is correct, or whether an API does what you expect.

It is not a claim that a model never hallucinates.

## Bring your own key

Estelle routes to whichever model you configure: Anthropic, OpenAI, Google, DeepSeek, Moonshot, or a
local model on your own hardware. Your key, your provider, your bill. Estelle owns the routing, the
grounding, and the memory. It does not own your model.

Fixes are proposed, not merged. The default path writes to a sandbox and opens a reviewable pull
request for a human to merge.

## Requirements

macOS or Linux, on arm64 or x86_64. Node 18 or newer for this install path. There is no Windows
build.

## How the install is verified

`npm install` runs `install.js`, which:

- downloads over HTTPS only, from `github.com` or `release-assets.githubusercontent.com` and nowhere
  else. A redirect that leaves those hosts aborts the install.
- bounds every download before taking it: 64 KiB for the manifest, 512 MiB for the archive, 5
  redirects, and a 30 second timeout.
- requires the release's `SHA256SUMS` manifest to name your archive exactly once, with a well-formed
  64-hex digest.
- compares the archive's SHA-256 against that digest, and installs nothing on a mismatch.
- requires the archive to contain exactly one member named `estelle`, and rejects any archive with
  unexpected members.
- refuses a binary that is not a regular file, or that is a symlink.
- stages into a temporary directory and moves the verified binary into place atomically.

The shell installer performs the same checksum and single-member checks. Both are mutation-tested in
CI: a corrupted archive and an archive carrying an extra member must both be refused.

Every release is built by GitHub Actions, which publishes a signed SLSA build provenance attestation
for each artifact. Neither installer checks that attestation for you, so verify it yourself if you
want the stronger guarantee:

```sh
gh attestation verify estelle-<target>.tar.gz --repo uqeu/estelle-cli
```

This package is published with npm provenance. The binaries themselves are not code-signed or
notarized.

---

[fatelabs.ca](https://www.fatelabs.ca) · [docs](https://www.fatelabs.ca/docs) ·
[install guide](https://www.fatelabs.ca/docs/install) · Apache-2.0
