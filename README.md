# Estelle CLI

**Repository memory and scoped checks for AI coding agents.** Estelle can ground supported
API/import claims against repository evidence. It does not certify arbitrary prose, all
symbols, test success or whole-program correctness.

The candidate 0.4.1 native runner adds explicit required/advisory/disabled policies and
bounded deterministic Stop review. A source merge or plugin refresh does not prove that
an installed runner has been upgraded. See [the policy and limits](docs/required-hook-policy.md).

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

The plugin ships generated hook registrations, skills and agents, not an MCP server
(since plugin 0.3.7). Connect Estelle's MCP endpoint separately with `estelle init` or
the remote MCP configuration below. Native hooks also need their own usable Estelle
credential; the plugin carries none. The packaged hook command resolves the npm runner,
so Node 18 or newer and a compatible native runner must be available.

`estelle install-hooks` generates Claude Code and Codex configurations from one table.
Hosts must trust and run the relevant hook events. Registration is not proof that
an event ran, that the model read its context, or that every write tool is intercepted.

**Candidate native runner 0.4.1:** explicit `advisory` is non-enforcing, `disabled`
reports skipped checks, and `required` pauses supported direct edits when a scoped
check cannot establish approval. Required installation also demands an explicit
shell choice: `pause` restricts generic shell commands; `observe` keeps them usable
but unverified. Neither is a general filesystem enforcement boundary.

The old `ESTELLE_HOOK_BLOCK=1/true/on` opt-in remains a separately reported
legacy flagged-only mode only when no modern environment or saved policy is chosen.
It can refuse fresh positive Python findings, but does not become required admission
or request Stop repairs. An explicit modern advisory choice overrides that old opt-in.

Required edits use complete candidate post-images and the existing server gate's
supported API/import/ruleset scope. Unsupported input, incomplete or stale evidence,
network/authentication failures and timeouts are visible denials, not silent passes.
Recall and background ingestion remain best-effort and do not certify the turn.

Stop review in this candidate consumes deterministic evidence only. It does not opt
into semantic BYOK/model review. Confirmed contradictions can request at most two
repairs under required policy; unknown prose is disclosed rather than declared true
or blanket-blocked. Stop cannot retract text the host already streamed. These limits,
version requirements and installation steps are detailed in
[required-hook-policy.md](docs/required-hook-policy.md).

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
estelle verify src/http/headers.rs     # inspect supported API claims against repository evidence
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
