# Shown in a film, not built yet

**Why this file exists.** The founder: *"these demo videos give us foresight into: we made this in a
demo but we haven't been able to actually do this, so we should code this up later."* Every
capability a film depicts that is not wired today gets a line here, with the film and the beat.
Two jobs: **his voiceover stays accurate**, and **we get the backlog for free**.

⚠️ **Read the confidence column.** `probed` means I ran it or read the code path and can cite a
line. `repo doc` means I am repeating a claim from this repo's own documentation and have not
re-measured it. **A shipped/not-shipped judgement I did not verify is marked as such rather than
stated flatly** — the whole point of the file is that the founder can trust it in a room.

---

## Film 1 · `saltbox/inkwell` · the wall and the local fleet

| beat | depicted | status | confidence |
|---|---|---|---|
| 2 | **Ten subagents running, then all ten stopped by a usage cap** | The orchestra fleet view is a **server** surface. `commands.rs:518` says it plainly: *"one server task… production does not emit it yet."* No client-side ten-worker view exists. | probed |
| 2 | **The plan and in-flight diffs survive the provider dying** | Session state and memory are built. **Whether a half-finished multi-step run resumes exactly at step 5 after a provider swap is NOT something I verified.** | unverified |
| 3 | **Projected API cost for the remaining steps** | Per-role receipts are real. **A projection of the remaining work at published rates is not a surface we ship.** | probed |
| 5 | **Per-role model pinning (plan / implement / review)** | Per-role AUTO/PINNED is described as built and deployed in the repo's own film notes. ⚠️ The `Ctrl+M` models screen is **not reachable** — that chord is carriage return, and the design-book lane moved the hint to `ctrl+g context`. | repo doc + probed |
| 5b | **Ten local models resuming the same ten jobs from shared memory** | ⛔ **Not built.** The local FLEET MEASUREMENT is real (`estelle_machine::machine/named_model/fit`, on the recording machine). Ten workers coordinating through shared memory is not. | probed |
| 7, 8b | **Live doc research feeding the local model — context7, Stripe, GitHub, Vercel** | The research ladder exists; **live documentation retrieval through it is in progress, not shipped.** MCP servers are real and `context7` is a real one we ship against. | repo doc |
| 8a | **A worker stopped by TEAM MEMORY, with the commit** | ⛔ **Not built.** The repo's own film notes call contradiction detection over the memory graph *"the single most valuable unbuilt thing"*. This is that. | repo doc |
| 8c | **The gate refusing a local model's diff, then a repaired second attempt** | ✅ Built. `gate_refusal::lines` is the live modal's own renderer, called here at the real pane width. | probed |
| 8d | **Sweep bringing the graph current mid-task** | ✅ Built. ⚠️ The repo carries a known defect: a missing `repo` field 503s any sweep over 500KB. | repo doc |
| 9 | **Cross-family review objecting to the local model's code** | ✅ Built — `review` runs a rival model family. |
| 11 | **The credential fence, before the network** | ✅ Built and **measured**: `estelle_client::find_secret_shape` returns `Some(("an sk- API key", 1))` for the film's fixture string. | probed |
| 12b | **The idle fleet drafting ten queued jobs across ten suites** | ⛔ **Not built as one surface.** The individual suites exist in various states; an idle fleet picking up a queue and drafting propose-only work does not. | probed |
| 13 | **`/spend` with the same task priced on the API** | Per-role receipts are real and committed. **The two-column "what you paid / what the API costs" view is not a shipped screen.** The arithmetic is on published rates. | repo doc |

## Film 2 · `cartwheel/storefront` · the repo moved without him

| beat | depicted | status | confidence |
|---|---|---|---|
| 1 | **A four-day rollup: who merged, who decided, who opened what** | `list_sessions` and team activity are built; **the narrative rollup is not.** | repo doc |
| 2 | **A team decision volunteered BEFORE he writes the code, with its ADR line** | ⛔ **Not built.** Contradiction detection over the memory graph. The repo's own film notes call it *"the single most valuable unbuilt thing"*. | repo doc |
| 3 | Recording his own counter-decision, linked to the one it departs from | ⛔ **Not built.** Memory holds decisions; a decision that cites the decision it revises is not a shape we write. | probed |
| 3b | **A teammate's proposal parked in Slack, and a second teammate's agent answer about it** | ⛔ **Not built.** Requires Slack ingest joined to session memory. | probed |
| 4 | **A teammate inside the same file right now** | ⛔ **Not built.** Concurrent-work detection across teammates. | repo doc |
| 5 | 🔴 **What a teammate ASKED THEIR AGENT and what it ANSWERED** | ⛔ **NOT BUILT, AND IT IS THE MOST DIFFERENTIATED FRAME IN THE THREE FILMS.** Sessions are stored per account; a cross-teammate read of question-and-answer pairs is not a surface, a query, or an endpoint today. **This is the one to build.** | probed |
| 5b | A second agent-conversation frame, used to confirm rather than contradict | ⛔ **Not built.** Same mechanism as beat 5. | probed |
| 6 | **The memory catch: code that compiles, that the gate passes, that a team decision forbids** | ⛔ **Not built** — the catch itself. ✅ The gate half is real. | repo doc |
| 8 | Slack + Linear + GitHub in one turn, replying **inside a teammate's own thread** | ✅ The Slack → PR loop is live and proven. ⛔ **Threading a reply into a specific teammate's earlier thread, and the Linear write, are not.** | repo doc |

## Film 3 · `uqeu/estelle` · production, and the gate refusing

| beat | depicted | status | confidence |
|---|---|---|---|
| 1 | **"While you were gone" rollup** | `list_sessions` and team activity are built; **the narrative rollup is not.** | repo doc |
| 1 | **A decision extracted from a teammate's revert** | ⛔ **Not built.** | repo doc |
| 4 | The stale-index refusal, with both SHAs | ✅ Built — the server produces the verdict and the design-book lane wired the CLI to read it. | probed |
| 8 | 41,934 labelled snippets, 100.0% / 0.0%, offline | ✅ Measured 2026-09-02. 🔴 **The limit is on screen, not in the voiceover**: it measures invented repository APIs, in Python, and 12 of 23 languages block. | repo doc |

---

## Cross-cutting, and these are the ones most likely to be asked about

| depicted | status |
|---|---|
| **The production rail ticking** — latency drifting, counters climbing, agents changing state | ⛔ **Not built into the films.** `dress()` sets one static JSON per film and no beat touches the rail again. The rail is a real surface reading real endpoints; the films feed it fixtures. |
| **Estelle interrupting mid-typing** | ⛔ **Not built.** The player has no cue that fires during a typing phase. |
| **Closing the production rail by asking in plain English** | ⛔ **Not possible today, and this is a product gap, not a film gap.** The design gives production a permanent home: `live_renderer.rs:2515-2523` computes `prod_as_rail` from the pane width alone, and `prod_panel_visible` is only consulted on the narrow path. **There is no flag that closes the rail on a wide terminal.** |

## The honesty line every film keeps

⛔ **No comparative quality number appears in any film.** Not "as good as frontier", not a
percentage, not a model-versus-model claim. That experiment is designed and has not run. `/spend` is
allowed because it is arithmetic on published rates. **The quality argument lives in the founder's
voiceover, where he owns it.**
