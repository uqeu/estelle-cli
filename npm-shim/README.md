# Estelle

**The trust layer under your AI coding agents.**

Estelle grounds every model in your real codebase and refuses to ship what isn't true. It is not
another agent — it is the substrate under the agents you already use: memory, a deterministic
grounding gate, grounded review, and a propose → verify → repair loop. Model-agnostic, bring your
own key.

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

That is the install. One command, no account needed to start.

---

## The one number

Of code references that name a repository symbol your codebase does not define, Estelle's
deterministic gate catches **100%**, with **zero false positives** across 6,395 real repository
symbols and 1,323 third-party library symbols. An LLM asked to judge its own output catches about
half of them.

It is deterministic — no model, no network — so it is a property of the artifact, not a score that
drifts between runs. **Reproduce it yourself, offline, with no API key:**

```sh
python scripts/eval_hallucination.py src all      # 41,604 labelled cases
python scripts/eval_hallucination_libs.py         # requests · httpx · urllib3 · click
```

⚠️ This measures **invented repository APIs**, not correctness in general. Code can be perfectly
grounded and still wrong. We claim the thing we measure.

---

## What it does

**Ground a question in your actual repository.**

```sh
estelle sweep                 # index this repo into your memory
estelle ask "how does billing retry a failed charge?"
```

Answers cite files and lines from your code. When Estelle cannot ground an answer, it says so
instead of inventing one.

**Refuse an invented API before it reaches a file.**

```sh
estelle verify src/http/headers.rs    # flags symbols your repo does not define
estelle gate                          # run the merge gate on a local diff
```

**Run always-on, so the agent does not get to choose.**

```sh
estelle install-hooks
```

Nine hooks fire on write, edit, shell, prompt, and session end. An agent that decides whether to use
its trust layer is not protected by one.

**Serve other harnesses.**

```sh
estelle mcp-server            # Estelle's tools over MCP, for any agent
estelle acp                   # Agent Client Protocol, over stdio
```

---

## Everything else

Twenty-seven commands. `estelle --help` lists them; the ones worth knowing first:

| | |
|---|---|
| `estelle setup` | configure, brief, sweep, and prove Estelle on a real symbol from your repo |
| `estelle doctor` | diagnose credentials and runtime readiness — and it never prints a secret value |
| `estelle recall` | search your memory and your code together |
| `estelle monitor` | production health, in the terminal |
| `estelle research` | vendor drift, and repairs grounded in it |
| `estelle screens` | thirteen designed terminal surfaces, each stamped as fixture data |

---

## Bring your own key

Estelle routes to whichever model you configure — Anthropic, OpenAI, Google, DeepSeek, Moonshot, or a
local model on your own hardware. Your key, your provider, your bill. Estelle owns the routing, the
grounding, and the memory; it does not own your model.

---

## Requirements

macOS or Linux. The install script downloads the matching native binary from
[`uqeu/estelle-cli`](https://github.com/uqeu/estelle-cli), verifies it against the release's SHA-256
manifest, rejects unexpected archive members, and only then installs it. Every release is built and
signed by GitHub Actions with published provenance.

`npm i @fatelabs/estelle` installs the same native binary through a small launcher, for environments
that are already npm-based. The previous JavaScript CLI is retired.

---

[fatelabs.ca](https://fatelabs.ca) · [docs](https://fatelabs.ca/docs) · Apache-2.0
