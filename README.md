# Estelle CLI

**The trust layer under your AI coding agents.** Estelle grounds every model in your real codebase and
refuses to ship what isn't true.

It is not another agent. It runs underneath the agents you already use and gives them three things they do
not have on their own: memory of your repository, a deterministic gate that resolves every symbol a change
names against your real code, and grounded review. Model-agnostic, bring your own key.

This repository holds the Rust CLI and the Claude Code plugin. The hosted server is separate.

## Install

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

That is the whole install. It detects your platform, downloads from this repository's latest release,
verifies the archive against the release's `SHA256SUMS`, rejects archives with unexpected members, and
atomically installs `estelle` into `~/.local/bin`. macOS or Linux, arm64 or x86_64. There is no Windows
build.

The same binary is on npm for machines that are already npm-based:

```sh
npm install -g @fatelabs/estelle
```

## Two doors, and the difference between them

Both doors reach the same hosted server at `https://api.fatelabs.ca/mcp`. They do not see the same code.

**Door 1: the plugin. Use this one.** In Claude Code:

```
/plugin marketplace add uqeu/estelle-cli
/plugin install estelle@fatelabs
```

The plugin ships the hooks and the server entry together. The hooks fire on every edit rather than when the
model decides to call a tool, and they send your working tree as you edit it, so **uncommitted work is
indexed**. The plugin carries no credential: the client negotiates OAuth in the browser. Its hooks shell
out through `npx`, so Node 18 or newer has to be on the machine.

For editors that are not Claude Code, `estelle install-hooks` writes the same ten handlers across seven
events (`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`, `Stop`, `PreCompact`,
`SessionEnd`) for both Claude Code and the Codex CLI.

Every hook fails silently by design (`tui/src/top_level.rs:378`): no credentials, no network, a slow
server or an empty memory all produce nothing rather than an error on the hot path. **The
`UserPromptSubmit` handler is the slow one.** It asks the server for a full search
(`top_level.rs:384`) and reads only the recall text (`top_level.rs:389`), so on a long prompt it can
exceed its timeout, and when the host kills it the turn simply proceeds with no added context. Nothing
tells you that happened.

**What the edit hook does, exactly.** It grounds the edit and reports one of four verdicts to you and to
the model: PASSED, FLAGGED, ABSTAINED, or UNREACHABLE. ABSTAINED says in its own words that it is not a
pass. **It does not block the edit** (`tui/src/top_level.rs:717`). The finding is advisory while the
server does not attest index freshness, and the runner says so in the text it emits. The blocking check is
`estelle gate`, which returns a merge verdict you can fail a build on.

**And the edit hook inspects Python only today** (`tui/src/top_level.rs:719`). A write to any other file
type produces no output at all, so on that path "not checked" and "clean" look the same. Use
`estelle gate` on the diff if you need a verdict for another language.

**Door 2: the remote MCP URL.** For hosts that cannot run a Claude Code plugin:

```sh
claude mcp add --transport http estelle https://api.fatelabs.ca/mcp
```

`estelle init` writes that entry for every editor it finds, so you do not paste it once per tool.

This door has no hooks. **It sees only what you have pushed, so its graph is never fresher than your last
push to GitHub.** If you are asking about code you wrote in the last ten minutes, use door 1.

## What the CLI does

```sh
estelle sweep                          # index this repo into your memory
estelle ask "how does billing retry a failed charge?"
estelle verify src/http/headers.rs     # resolve every symbol against the repo's real symbol graph
estelle gate                           # run that check over a local diff
estelle mcp-server                     # Estelle's tools over MCP, for any agent
estelle acp                            # Agent Client Protocol, over stdio
```

Answers cite the files and lines they came from. When Estelle cannot ground an answer, it says so instead
of inventing one. `estelle --help` lists all twenty-six commands.

To bypass credential storage, set `ESTELLE_API_KEY` in the process environment. It takes precedence over
`~/.estelle/auth.json` and is never persisted by Estelle.

## What is measured

Estelle's grounding gate caught **38,153 of 38,153 invented APIs**, with **0 false positives on 6,933 real
symbols**, over 45,086 labelled cases. It makes no model call: it parses the change and resolves each
symbol against the repository's indexed symbol graph, so the verdict is a property of the artifact rather
than a score that moves between runs.

Read the limit in the same breath, because it is the part that decides whether the number applies to you:

- **That measures invented repository APIs, in Python.** A case is one code reference naming a repository
  symbol or import the codebase does not define.
- **Twelve of twenty-three supported languages block** at some rung. Python is the only one with the full
  guarantee of existence, arity, type, and member calls on any receiver. The other eleven are navigated,
  not gated: their references are excluded from the rate rather than counted as passes.
- **It is not a correctness claim.** Code can be perfectly grounded and still wrong.
- The eval harness is not in this public repository, so you cannot re-run it from here. What you can check
  from here is the behaviour, on your own code, with `estelle verify` and `estelle gate`.

Fixes are proposed, not merged. The default writes to a sandbox and opens a pull request a human reviews.
Auto-merge is opt-in, off by default, and gated on the fix being proved green in a sandbox first. There is
no auto-deploy step.

The component boundaries, trust model, release path, measurements, provenance inventory, and named limits
live in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/SCORECARD.md`](docs/SCORECARD.md), and
[`docs/WHAT-WE-BUILT.md`](docs/WHAT-WE-BUILT.md).

## How a release is verified

Public releases are cut by [`.github/workflows/release.yml`](.github/workflows/release.yml) and carry four
checksummed binaries: macOS and Linux, arm64 and x86_64. The workflow refuses a tag that disagrees with the
version in `Cargo.toml`, `npm-shim/package.json`, `estelle-plugin/.claude-plugin/plugin.json`, or
`.claude-plugin/marketplace.json`, builds from the lockfile with warnings denied, and requests a
GitHub-signed SLSA build provenance attestation for every published artifact.

Neither installer checks that attestation for you, so verify it yourself if you want the stronger
guarantee:

```sh
gh attestation verify estelle-<target>.tar.gz --repo uqeu/estelle-cli
```

`ESTELLE_RELEASE_REPOSITORY` is an explicit operator and test override that changes the installer's trust
root. Normal installs must leave it unset.

## Provenance

Forked from OpenAI Codex at commit `582569998181aad08a88bacc151a94b2048a5d1f`. The fork boundary and the
egress census are enforced by `scripts/check-fork-audit.py` and `fork-manifest.yaml` on every release.

---

[fatelabs.ca](https://www.fatelabs.ca) · [docs](https://www.fatelabs.ca/docs) ·
[install guide](https://www.fatelabs.ca/docs/install) · Apache-2.0
