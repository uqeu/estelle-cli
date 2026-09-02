# Estelle

**The trust layer under your AI coding agents.** Estelle grounds every model in your real codebase and
refuses to ship what isn't true.

It is not another agent. It runs underneath the agents you already use and gives them three things they
do not have on their own: memory of your repository, a deterministic gate that resolves every symbol a
change names against your real code, and grounded review. Model-agnostic, bring your own key.

## Install

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

That is the whole install. It puts the native `estelle` binary in `~/.local/bin`.

This npm package is a launcher for the same binary, for machines that are already npm-based:

```sh
npm install -g @fatelabs/estelle
```

Either way you get one binary. See [How the install is verified](#how-the-install-is-verified) for what
the launcher checks before it puts anything on your disk.

## Two doors, and the difference between them

Both doors reach the same hosted server at `https://api.fatelabs.ca/mcp`. They do not see the same code.

**Door 1: the plugin. Use this one.** In Claude Code:

```
/plugin marketplace add uqeu/estelle-cli
/plugin install estelle@fatelabs
```

The plugin ships the hooks and the server entry together. The hooks fire on every edit rather than when
the model decides to call a tool, and they send your working tree as you edit it, so **uncommitted work
is indexed**. The plugin carries no credential: the client negotiates OAuth in the browser. The hooks
shell out through `npx`, so Node 18 or newer has to be on the machine.

**Door 2: the remote MCP URL.** For hosts that cannot run a Claude Code plugin:

```sh
claude mcp add --transport http estelle https://api.fatelabs.ca/mcp
```

`estelle init` writes the same entry for every editor it finds, so you do not have to paste it once per
tool.

This door has no hooks. **It sees only what you have pushed, so its graph is never fresher than your last
push to GitHub.** If you are asking about code you wrote in the last ten minutes, use door 1.

## What the CLI does

**Ground a question in your actual repository.**

```sh
estelle sweep                 # index this repo into your memory
estelle ask "how does billing retry a failed charge?"
```

Answers cite the files and lines they came from. When Estelle cannot ground an answer, it says so instead
of inventing one.

**Check a change before it merges.**

```sh
estelle verify src/http/headers.rs    # resolve every symbol against the repo's real symbol graph
estelle gate                          # run that check over a local diff
```

**Run on every edit, rather than when a model chooses to call a tool.**

```sh
estelle install-hooks
```

Writes ten handlers across seven events (`PreToolUse`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`,
`Stop`, `PreCompact`, `SessionEnd`), for both Claude Code and the Codex CLI. This is door 1's mechanism,
available to editors that are not Claude Code.

**What the edit hook does, exactly.** It grounds the edit and reports one of four verdicts to you and to
the model: PASSED, FLAGGED, ABSTAINED, or UNREACHABLE. ABSTAINED says in its own words that it is not a
pass. **It does not block the edit.** The finding is advisory while the server does not attest index
freshness, and the runner says so in the text it emits. The blocking check is `estelle gate`, which
returns a merge verdict you can fail a build on.

**And the edit hook inspects Python only today.** A write to any other file type produces no output at
all, so on that path "not checked" and "clean" look the same. Use `estelle gate` on the diff if you need
a verdict for another language, and read the language scope under "What is measured" below.

**Serve any other harness.**

```sh
estelle mcp-server            # Estelle's tools over MCP, for any agent
estelle acp                   # Agent Client Protocol, over stdio
```

`estelle --help` lists all twenty-six commands. A few worth knowing early:

| | |
|---|---|
| `estelle setup` | configure, brief, sweep, then prove Estelle on a real symbol from your repo |
| `estelle init` | write the MCP entry for every editor you have installed |
| `estelle doctor` | credential and runtime readiness, reported without printing a secret value |
| `estelle recall` | search your memory and your code together |
| `estelle research` | watch a dependency for API drift, and propose a repair when it moves |
| `estelle monitor` | production health, in the terminal |
| `estelle github` | link GitHub, connect an installation, and sweep a repository |

## What is measured

Estelle's grounding gate caught **38,153 of 38,153 invented APIs**, with **0 false positives on 6,933 real
symbols**, over 45,086 labelled cases. It makes no model call: it parses the change and resolves each
symbol against your repository's indexed symbol graph, so the verdict is a property of the artifact rather
than a score that moves between runs.

Read the limit in the same breath, because it is the part that decides whether the number applies to you:

- **That measures invented repository APIs, in Python.** A case is one code reference naming a repository
  symbol or import your codebase does not define.
- **Twelve of twenty-three supported languages block** at some rung. Python is the only one with the full
  guarantee of existence, arity, type, and member calls on any receiver. The other eleven languages are
  navigated, not gated: their references are excluded from the rate rather than counted as passes.
- **It is not a correctness claim.** Code can be perfectly grounded and still wrong. The gate tells you a
  symbol exists, not that the change does what you meant.
- The eval harness is not in this public repository, so you cannot re-run it from here. What you can check
  from here is the behaviour, on your own code, with `estelle verify` and `estelle gate`.

## Bring your own key

Estelle routes to whichever model you configure: Anthropic, OpenAI, Google, DeepSeek, Moonshot, or a local
model on your own hardware. Your key, your provider, your bill. Estelle owns the routing, the grounding,
and the memory. It does not own your model.

Fixes are proposed, not merged. The default writes to a sandbox and opens a pull request a human reviews.
Auto-merge is opt-in, off by default, and gated on the fix being proved green in a sandbox first. There is
no auto-deploy step.

## Requirements

macOS or Linux, on arm64 or x86_64. Node 18 or newer for this npm install path and for the plugin's hooks.
There is no Windows build.

## How the install is verified

`npm install` runs `install.js`, which:

- downloads over HTTPS only, from `github.com` or `release-assets.githubusercontent.com` and nowhere else.
  A redirect that leaves those hosts aborts the install.
- bounds every download before taking it: 64 KiB for the manifest, 512 MiB for the archive, 5 redirects,
  and a 30 second timeout.
- requires the release's `SHA256SUMS` manifest to name your archive exactly once, with a well-formed
  64-hex digest.
- compares the archive's SHA-256 against that digest, and installs nothing on a mismatch.
- requires the archive to contain exactly one member named `estelle`, and rejects any archive carrying
  unexpected members.
- refuses a binary that is not a regular file, or that is a symlink.
- stages into a temporary directory and moves the verified binary into place atomically.

The shell installer performs the same checksum and single-member checks. Both are mutation-tested in CI: a
corrupted archive and an archive carrying an extra member must both be refused.

Every release is built by GitHub Actions, which publishes a signed SLSA build provenance attestation for
each artifact. Neither installer checks that attestation for you, so verify it yourself if you want the
stronger guarantee:

```sh
gh attestation verify estelle-<target>.tar.gz --repo uqeu/estelle-cli
```

This package is published with npm provenance. The binaries themselves are not code-signed or notarized.

---

[fatelabs.ca](https://www.fatelabs.ca) · [docs](https://www.fatelabs.ca/docs) ·
[install guide](https://www.fatelabs.ca/docs/install) · Apache-2.0
