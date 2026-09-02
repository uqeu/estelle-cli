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

🔴 **RENUMBERED 2026-09-02.** The film went from **19 beats to 11** — the founder's note was that he
was driving every step (*"you just have to keep talking to Estelle to keep doing stuff"*), and a beat
IS a human turn, so autonomy could only be bought by folding chains into one reply. Nothing was
deleted from the film's argument; the typed lines between the pieces were. Old beat numbers in
brackets.

| beat | depicted | status | confidence |
|---|---|---|---|
| 1 *(was 2)* | **Ten subagents running, then all ten stopped by a usage cap** | The orchestra fleet view is a **server** surface. `commands.rs:518` says it plainly: *"one server task… production does not emit it yet."* No client-side ten-worker view exists. | probed |
| 1 *(was 2)* | **The plan and in-flight diffs survive the provider dying** | Session state and memory are built. **Whether a half-finished multi-step run resumes exactly at step 5 after a provider swap is NOT something I verified.** | unverified |
| 2 *(was 3)* | **Projected API cost for the remaining steps** | Per-role receipts are real. **A projection of the remaining work at published rates is not a surface we ship.** | probed |
| 3 *(was 4+5)* | **Per-role model pinning (plan / implement / review)** | Per-role AUTO/PINNED is described as built and deployed in the repo's own film notes. ⚠️ The `Ctrl+M` models screen is **not reachable** — that chord is carriage return, and the design-book lane moved the hint to `ctrl+g context`. | repo doc + probed |
| 4 *(was 5b)* | **Ten local models resuming the same ten jobs from shared memory** | ⛔ **Not built.** The local FLEET MEASUREMENT is real (`estelle_machine::machine/named_model/fit`, on the recording machine). Ten workers coordinating through shared memory is not. ⚠️ **And the table is hand-laid rather than `orchestra_view`'s** — see the cross-cutting row below for why. | probed |
| 4, 5 *(was 7, 8b)* | **Live doc research feeding the local model — context7, Stripe, GitHub, Vercel** | The research ladder exists; **live documentation retrieval through it is in progress, not shipped.** MCP servers are real and `context7` is a real one we ship against. | repo doc |
| 5 *(was 8a)* | **A worker stopped by TEAM MEMORY, with the commit** | ⛔ **Not built.** The repo's own film notes call contradiction detection over the memory graph *"the single most valuable unbuilt thing"*. This is that. | repo doc |
| 5 *(was 8c)* | **The gate refusing a local model's diff, then a repaired second attempt** | ✅ Built. `gate_refusal::lines` is the live modal's own renderer, called here at the real pane width. | probed |
| 5 *(was 8d)* | **Sweep bringing the graph current mid-task** | ✅ Built. ⚠️ The repo carries a known defect: a missing `repo` field 503s any sweep over 500KB. | repo doc |
| 6 *(was 9)* | **Cross-family review objecting to the local model's code** | ✅ Built — `review` runs a rival model family. | repo doc |
| 7 *(was 11)* | **The credential fence, before the network** | ✅ Built and **measured**: `estelle_client::find_secret_shape` returns `Some(("an sk- API key", 1))` for the film's fixture string. | probed |
| 8 · **NEW** | **"Put ANTHROPIC_API_KEY in your Estelle `.env`. I read it when the run starts."** | ✅ Built. `arg0/src/lib.rs:298 load_dotenv()` reads a `.env` at start-up and exports it. ⚠️ **The limit, and it is why no path is on screen:** the file it reads is the one beside the CLI's own config directory, **not** the `.env` in the repo root. A user who puts the key in the repo's `.env` is not covered by this. | probed |
| 10 *(was 12b)* | **The idle fleet drafting ten queued jobs across ten suites** | ⛔ **Not built as one surface.** The individual suites exist in various states; an idle fleet picking up a queue and drafting propose-only work does not. | probed |
| 11 *(was 13)* | **`/spend` with the same task priced on the API** | Per-role receipts are real and committed. **The two-column "what you paid / what the API costs" view is not a shipped screen.** The arithmetic is on published rates. | repo doc |
| 4, 5, 6 · **NEW** | 🔴 **THE UNATTENDED CHAIN — the largest new claim in this cut.** A model hits an unknown API → it researches it → writes → the gate refuses ITS OWN code → it repairs → the reviewer objects → a planned step is pulled forward, **all with no human turn between any two of them.** | ⚠️ **The PIECES are each built or marked above. The CHAIN is not verified.** `/work` runs a plan and the gate is a plan step; whether a run continues unattended through a refusal, a repair round and a cross-family review — with no operator acknowledgement — is **not something I ran.** Treat this as the film's design intent, not as a shipped behaviour. | unverified |
| 5, 8 · **NEW** | **He types a check-in while the run keeps writing underneath his half-finished line** | ⛔ **Not built in the product.** The PLAYER does it (`Key::Interrupt`); the product's composer blocks on a turn. Same mechanism film 3 uses for the inverse. | probed |

## Film 2 · `cartwheel/storefront` · the repo moved without him

🔴 **REWRITTEN 2026-09-02 AFTER A SURFACE AUDIT, AND THE OLD TABLE WAS WRONG IN BOTH DIRECTIONS.**
The film went from **11 beats to 16**, and memory became the spine rather than one beat, on the
founder's note: *"we talk about memory in the films again, we need to show how good Estelle memory
and context is."*

⚠️ **TWO ROWS HERE PREVIOUSLY READ ⛔ NOT BUILT AND WERE FALSE NEGATIVES.** They are the reason this
section was re-measured rather than edited. A ⛔ on a thing we ship is not a harmless conservatism —
it is the founder declining, in a room, to claim something true.

* *"What a teammate asked their agent"* was filed as *"not a surface, a query, or an endpoint
  today."* **`POST /turns` is an endpoint today** (`src/estelle/serve/api_turns.py:18`), Tier 1, with
  no model on the path — and `memory_routing.namespace_for:46` resolves a team member to the **team
  id**, so `handle_turns` reads the whole team namespace with no per-member filter. A teammate's
  verbatim prompts come back to any member's key. The gap is the CLIENT, not the product.
* *"A teammate inside the same file right now"* was filed as ⛔. **`GET /presence` ships it** and the
  CLI renders it (`commands.rs:805`, `commands.rs:1799`): `active` rows carry `member`, `since` and
  `files`. The concurrency signal the row said we lacked is on the wire.

**The whole 🟡 column below is ONE missing line**: `estelle-client/src/endpoint.rs` has no `Turns`
variant, so no command can ask for a door the server already answers.

| beat | depicted | status | confidence |
|---|---|---|---|
| 1 | **Who is active, who worked overnight, what is in flight** | ✅ **Built.** `GET /presence`, team-scoped through the same namespace resolver. `commands.rs:1799` renders `active`/`overnight`/`files_in_use`/`handoffs`. | probed |
| 1 | A four-day narrative rollup naming who merged what | ⚠️ The FACTS are in memory (`commit` is a stored kind, `content_trust.py:88`). **The rollup is a grounded answer, not a dedicated surface** — it is drawn here as prose, which is what the answer path returns. | probed |
| 2 | **A team decision volunteered before he writes the code, with its ADR line** | ✅ **The surface is built** — `GET /memory/cards` returns title, folder, body and `sources`, and `commands.rs` prints a `provenance` line from them. ⛔ **The VOLUNTEERING is not.** Nothing watches what he is about to write and raises a contradicting decision unprompted; the repo's own notes call that *"the single most valuable unbuilt thing"*. **In the film he asks and it answers.** | probed |
| 3 | Estelle asks him for his reasoning instead of offering a menu | ✅ Grounded answer path. ⛔ **The `/choose` numbered picker was CUT** — no option surface exists in this client. | probed |
| 4 | **His own sentence stored verbatim, casing and typo intact** | 🟡 **Server built, no CLI door.** `turn_log.prepare_turn` strips the ends and stops: *"a store that tidies what was said cannot later prove what was said."* Read side is `POST /turns`. | probed |
| 5 | **A proposal made in Slack weeks ago, retrieved** | ✅ **Built.** `slack` is a real ingested memory kind (`content_trust.EXTERNALLY_AUTHORED_KINDS`), reachable through the grounded answer path. | probed |
| 5b | 🔴 **A listing that says HOW it knows — `grounded` may certify, `acquired` may not** | ✅ **Built, and under-used.** `GET /memories` renders `source \| trust \| N chunks \| externally authored` straight off `MemoryItem`. `content_trust.trust_of:95` returns exactly two values. ⛔ **NOT `measured\|observed\|asserted`** — that is `FleetEvidence`'s vocabulary on the orchestra surface and `MemoryItem.trust` never takes it. | probed |
| 6 | 🔴 **What a teammate TYPED at their own agent, verbatim** | 🟡 **Server built and TEAM-WIDE; no CLI door.** See the correction above. `Turn` carries `author`, `at`, `role`, `source`. **One `Turns` variant in `endpoint.rs` closes this.** | probed |
| 6 | ⛔ **What the teammate was TOLD** | ⛔ **No store holds it, and the film now REFUSES it on camera.** `prepare_turn` is called in production without a `role`, so every Tier-1 row is `role="user"`. The earlier cut drew a column headed *"what they were told"* over nothing. Estelle now says: *"I have his questions. I do not have the answers he got."* | probed |
| 7 | A second turn-log read, used to confirm rather than contradict | 🟡 Same door as beat 6. | probed |
| 7b | 🔴 **A memory RETRACTED — withdrawn, dated, and still readable** | 🟡 **Built as a TOP-LEVEL command**, `estelle memory retract <subject> --reason` (`top_level.rs:3339` → `POST /retract`); **no in-session slash form.** `Memory.retract` closes the temporal fact log AND the recall mirror and **reads both back** rather than trusting the write. ⛔ **`claim_closed` and `recall_cleared` come back on the wire and the CLI drops them** — `top_level.rs:3763` lists the scalars it prints and neither is in it. **Two words fix that.** | probed |
| 8–10 | The fleet working while he interrupts it | ✅ `POST /work`, `POST /orchestra`, and `orchestra_view` is the product's own renderer. ⛔ **The PLAYER does the mid-word interrupt (`Key::Interrupt`); the product's composer blocks on a turn.** Same limit film 1 carries. | probed |
| 11 | **The memory catch: code that compiles, that the gate passes, that a team decision forbids** | ✅ The gate half is real. ⛔ **The CATCH is not** — same unbuilt contradiction detection as beat 2. | repo doc |
| 13 | A handoff note left for teammates who are not here | 🟡 **Read half built, write half not reachable.** `POST /presence` carries a `note` (`api_console.py:111`), and `endpoint.rs` declares `Presence` as `[Get]` only. So this terminal reads a handoff note and cannot leave one. | probed |
| 13 | ⛔ **"Three members have been alerted"** | ⛔ **CUT. Nothing in the product alerts a person.** No DM, no per-member email, no push. The only human-facing push is a Slack CHANNEL post needing a connected app plus a per-channel opt-in, fired by monitor signals rather than by presence. The film now says *"Nobody was paged, and nobody will be."* | probed |
| — | ⛔ **A `● /slack` receipt posting into a teammate's thread** | ⛔ **CUT.** No `/slack` command exists in this client. The Slack bot is its own door and does not take instructions from this terminal. | probed |
| — | ⛔ **A `who \| branch \| commits` table** | ⛔ **CUT.** `presence.py:20-29` carries `member`, `started`, `last_active`, `files`, `note`. No branch. No commit count. | probed |

## Film 3 · `cartwheel/storefront` · the night checkout went down

| beat | depicted | status | confidence |
|---|---|---|---|
| 1–2 | Cited answers about the sweep budget, and real sweep statistics | ✅ Built — navigation and stats. | probed |
| 3 | 🔴 **Estelle INTERRUPTS mid-sentence, unprompted, with a reason** | ⛔ **Not built.** Nothing in the product speaks unless spoken to. This is the beat the film exists for. | probed |
| 3 | **His half-typed line is parked and given back to the character** | ⛔ **Not built.** The composer holds one draft; a draft parked across an interruption and restored later is not a shape we have. | probed |
| 3 | `142 checkouts failed since 23:04. 38 retried and failed again.` | ⛔ **Not built as a sentence.** Monitor holds the counts; translating them into what a CUSTOMER experienced is not a surface. | repo doc |
| 4–5 | Root cause to `file:line`, tied to a vendor API version | ✅ Monitor, issue detection and vendor-drift detection are built. **The consolidated view is design-book.** | repo doc |
| 6 | 🔴 **The gate REFUSES our own repair, at 23:11, before production** | ✅ **Built.** `gate_refusal::lines` is the live modal's own renderer, called here at the real pane width, and the refusal is deterministic with no model call. | probed |
| 7 | Sandboxed repro, 1,204 tests, cross-model review | ✅ Built — sandboxed repro and cross-family review. | repo doc |
| 8 | **Recovery watched on the rail after he applies the fix** | ⛔ The rail is real and reads real endpoints; **the film feeds it a scripted incident.** Post-deploy watch-and-confirm is not a surface. | probed |
| 9 | **The parked sentence returns and he finishes it** | ⛔ **Not built.** Same mechanism as beat 3. |
| 10 | `/spend` including **what the refused repair cost** | Per-role receipts are real. **Pricing a REFUSED attempt separately is not a shipped view.** | repo doc |

## Cross-cutting, and these are the ones most likely to be asked about

| depicted | status |
|---|---|
| **The production rail ticking** — latency drifting, counters climbing, agents changing state | ✅ **Built in the films** (`design_book/rail.rs`, ticked every frame). The rail is a real surface reading real endpoints; **the films feed it a scripted incident.** |
| **Estelle interrupting mid-typing** | ⛔ **Not built in the product.** The PLAYER can now do it (`Key::Interrupt`, `Key::Park`, `Key::Restore`), which is what film 3 uses; the product has no unprompted-speech path. **Film 1 uses the inverse** — HE interrupts, and the run keeps writing under his half-typed line. |
| 🟡 **Film 1 beat 4's resuming fleet is a hand-laid `Say::Table`, not `orchestra_view`** | The real renderer is right there and beat 1 already uses it twice. Two CROSS-FILM guards in `demo_session_tests.rs` block a third block: `the_stopped_fleet_reconciles_with_its_own_banner` takes the **last** fleet in film 1 and asserts the usage-cap banner against it, and `the_worker_table_never_prints_a_per_worker_model_or_price` pins **`claude-opus-4-8`** into every fleet's roster line — which a LOCAL fleet cannot honestly carry. **Widening those two is a one-line change each** (select the fleet whose `killed_at_s` is set; assert the roster is non-empty and names a model the fixture declares). That file belongs to the player, not to a film lane, so this cut kept the table and dropped the fabricated per-worker MODEL column instead. |
| **Closing the production rail by asking in plain English** | ⛔ **Not possible today, and this is a product gap, not a film gap.** The design gives production a permanent home: `live_renderer.rs:2515-2523` computes `prod_as_rail` from the pane width alone, and `prod_panel_visible` is only consulted on the narrow path. **There is no flag that closes the rail on a wide terminal.** |

## The honesty line every film keeps

⛔ **No comparative quality number appears in any film.** Not "as good as frontier", not a
percentage, not a model-versus-model claim. That experiment is designed and has not run. `/spend` is
allowed because it is arithmetic on published rates. **The quality argument lives in the founder's
voiceover, where he owns it.**
