# Kimi — the CLI lane

**You own `cli-rs/`. It is its own git repo, so nothing you do collides with any other session.**
Read this file, then `docs/HANDOFF-2026-08-06.md` §0–§5. Do not read the whole repo first.

---

## The rules, and they are not style preferences

1. **GREEN IS A CLAIM ABOUT WHAT WAS MEASURED, NEVER ABOUT WHAT WAS NOT.** When you report a pass, say
   which half it covers, and prove the test can fail.
2. **An assertion that cannot fail is itself a defect.** A harness scored a 3B model 0-invented on 8/8
   here because `ast.parse("")` succeeds — an empty answer parsed clean and scored perfect.
3. **Assert the mutation APPLIED *and* the resulting STATE.** `str.replace` is silent on a miss, and the
   language can absorb an applied edit without producing what the mutant is named for.
4. **NON-EMPTY IS NOT CORRECTLY-PARSED.** Pair every vacuity guard with a shape assertion.
5. **A SHA identifies SOURCE; a COUNT identifies what is SERVING.** Read back the artifact, never the label.
6. **A cap is not a property of the thing you measured.** If your bound cut the run short, say so.
7. **Name the line a failure came through, or say you do not know which line.**
8. **Wire it or do not land it.** A module nothing calls is not done.
9. **Suite exit code read as its OWN step.** Never `cmd | tail` — that returns `tail`'s status. Never
   chain the suite and a commit on one line.
10. **Say the limit out loud.** "Proven by unit tests, not against a real terminal" is a sentence a
    reviewer respects and cannot catch you on. Overstating is the only unrecoverable move.

**Your output will be reviewed by a competitor's agent whose job is to find what you got wrong. Be
hypercritical of everything before you emit it. If a claim is unmeasured, say so.**

**Secrets, absolute:** never read, enumerate, copy or materialise credentials from `.env`,
`stress_accounts.json`, `estelle_accounts.json`, `estelle_experience.json`. Never print a secret value —
report `file:line + type`. Never run `railway variables --kv`. **`web/` is a separate worktree; do not
touch it.**

---

## What this is

A fork of the OpenAI Codex CLI at `582569998181aad08a88bacc151a94b2048a5d1f`, re-pointed at Estelle.
~100 crates; four matter: `estelle-client` (53 typed endpoints), `tui` (package `estelle-tui`, the
Ratatui app), `estelle-acp`, `estelle-mcp`.

**Measured baseline, 2026-08-06:** `cargo test -p estelle-client` **21 passed**,
`cargo test -p estelle-tui --bin estelle` **133 passed**, **0 failed**. Re-run both before you write
anything — two minutes, and it is how you know your later red test is red because of your assertion.

⚠️ **No `target/` — your first build is cold across ~100 crates.**

---

## Order of work

### 1. D1 — and it is three defects in seven lines

`tui/src/main.rs:2441` fires **two** model round-trips whenever the repo has local changes: a
`deep_search` **and** a `chat_completion` carrying up to `MAX_CHARS = 80_000` of source. Then
`main.rs:2463-2478` glues both answers together behind a `"Working memory (…)"` header and renders that
as the assistant's turn.

So one fix closes: the **privacy leak** (retrieval plumbing shown as the answer), the **90-second "hi"**
(80 KB to a frontier model to say hello), and the **double spend**.

- Retrieval context is **model input**, never **assistant output**. `working_memory_prompt` at
  `main.rs:2481` is correct — that string goes to the model. `AnswerReply.text` is what a human reads.
- `AnswerReply.working_paths` already exists and is already populated — disclose provenance from the
  **field**, not by splicing prose into the transcript.
- The server already has a conversational fast path (`utterance.py:102 is_conversational`) behind
  `/deep-search`. The CLI routes around it by calling the raw chat endpoint. Stop doing that.
- ⚠️ `main.rs:6560` currently asserts `rendered.contains("Repo graph")` — **that test encodes the
  defect.** Invert it deliberately; do not delete it quietly.

**Red test first:** distinguishable sentinels for the two answers; assert the rendered turn contains
**neither** `"Working memory ("` **nor** any path from `working_paths`.

### 2. D2 — the welcome scene is erased the moment you type

`main.rs:4870` (`if show_ground && app.composer.is_empty()`) and `main.rs:3703` gate on an **empty
composer**. The owner is "has the first message been *submitted*", not "is the box empty."

### 3. D3 — dark theme paints ANSI black (grey) instead of inheriting

`main.rs:131-141`. `Color::Black` is ANSI 0, a painted colour; `Color::Reset` inherits the terminal.
Used at `3535`, `4597-4598`, `4635`, `4977-4980`. ⚠️ **Decide per theme** — cream is a deliberate
painted surface. Do not blanket-replace all four, and say which you changed.

### 4. D4 — turn ownership does not exist

A grep found no `YOU`/`ESTELLE` attribution in the transcript path. **This is new work, not a fix.**

### 5. D5 — the spider lily is oversized

`boot_scene.rs:341/438/475`. Cosmetic, safe to defer.

**NOT a client defect:** an ~80s response is real server latency. Render the wait honestly; `Esc`
already cancels with request-ID invalidation.

**One defect per commit. Red → fix → green → run the release binary → commit.** The previous session
announced these five dozens of times across two days and landed zero, because it re-planned instead of
writing the test. **If you catch yourself describing a fix you already described, stop and write the
assertion.**

---

## Then: the surface

**The server has 218 routes. This client declares 53 and reaches 52.** ~130 customer-reachable routes
are unwired — the whole memory graph (`/graph`, `/graph/edges`, `/graph/nodes`), the Research rebuild,
`/ideate`, `/dream`, `/impact`, `/fact`, `/memory/cards*`, `/swarm*`, `/orchestra/plan|run`, and all of
`/me/*` (billing, keys, team).

The target is **every route reachable**, not a curated subset. Founder's call: ship when it is complete
and better than the JS CLI, not at parity.

⚠️ Read `docs/SERVER-CONTRACTS-STATUS-2026-08-06.md` before building any surface that shows server
state. Some of what the CLI wants **does not exist server-side yet** (`GET /agent/health`,
`GET /orchestra/status`, `repair.patch`). Its rule is binding:

> Every optional capability has an explicit absent/unknown value and a reason. Every observation has
> `observed_at`; every live snapshot has `stale_after_s`. **No client calculates a server fact from
> elapsed time, HTTP completion, or missing data.** Unknown is `null` — never zero, never a checkmark.

**Never fake an absent capability. Absent renders as absent, with the reason.**

---

## Then: the new surfaces

- **Scoped Mermaid production panel.** Render the *scope* subgraph from the code graph; colour each node
  by its bound failures — green / amber (traffic) / red (failing). The binding already exists server-side
  (`failure --bound_to--> symbol`). ⚠️ **Scoped only.** Whole-repo auto-layout is unreadable spaghetti —
  one diagram per scope, 10–20 nodes. Parked: animation, Slack export, website export.
- **Built-in Mermaid rendering** (MIT; jcode ships it).
- **npm distribution** — per-platform binaries + a shim. **Deliberately last.**

---

## Register items that are yours

- **#66** — `.gitignore` on sweep. `top_level.rs:706-760` already layers: `--exclude-standard`,
  git-inventory membership, a source-extension allowlist, a 400 KB cap, `is_secret_shaped` content
  rejection, a 4,000-path cap. **Verify each layer with a test before closing or re-opening it.**
- **#72 is largely STALE.** `Autonomy`, `AutonomyScope`, `SettingsSuite` **are** declared in
  `endpoint.rs`. What remains: `/settings` (billing) has no client endpoint at all, and
  `/autonomy/scope` has no POST call site.

---

## Build and test

```bash
cargo test -p estelle-client
cargo test -p estelle-tui --bin estelle
cargo clippy -p estelle-client --all-targets -- -D warnings
cargo clippy -p estelle-tui --bin estelle -- -D warnings
cargo build --release -p estelle-tui --bin estelle
```

Snapshots: `cargo test -p estelle-tui --bin estelle snapshot_` then `cargo insta review`. **Read every
frame before accepting it. Reject a baseline when the fixture did not produce the intended state.**

---

## Where the rest of the picture lives

| | |
|---|---|
| `architecture/system/ARCHITECTURE.md` | the whole system in Mermaid, end to end |
| `architecture/system/BUILD-PLAN.md` | every lane, every item, every state |
| `cli-rs/docs/HANDOFF-2026-08-06.md` | the five defects in full |
| `cli-rs/docs/SERVER-CONTRACTS-STATUS-2026-08-06.md` | what the server owes, re-scored |
| `cli-rs/docs/P3-PARITY.md` | 23/23 session commands, 14/14 top-level |

**Report what you measured, not what you attempted. If something is blocked, say so and finish
everything else.**
