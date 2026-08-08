# D1 landed — 2026-08-07 — the answer pipeline no longer leaks retrieval plumbing

**Status: LANDED in `cli-rs`, one commit.** This note exists because `architecture/` lives in the
parent repo and this session must not commit there — the coach carries this into the parent map.

## What D1 was

`tui/src/main.rs` `answer_question` fired **two** model round-trips whenever the repo had local
changes: a `deep_search` and a raw `chat_completion` carrying up to `MAX_CHARS = 80_000` of local
source, then spliced both answers into `AnswerReply.text` behind a `"Working memory (…)"` header.
The customer saw retrieval plumbing as the assistant's turn (privacy), paid two calls (double
spend), and a dirty-repo "hi" shipped 80 KB to a frontier model (~90 s).

## The fix, as landed

One round-trip per question, always `POST /deep-search`:

- **Non-conversational question + local context:** `working_memory_prompt` output rides the
  deep-search `question` field — the only pipe the server contract offers (`body = {question,
  repo?}`, verified at `src/estelle/serve/api_intel.py:502,551`). Working memory survives as a
  feature; provenance is disclosed from the typed `AnswerReply.working_paths` field (the
  `/context` panel), never spliced into the transcript.
- **Conversational question:** the raw question goes alone. A client-side gate
  (`is_conversational_turn`, `tui/src/main.rs`) skips the working-memory attachment so the
  server's `is_conversational` fast path (`utterance.py:102`) can fire. The gate decides
  **bandwidth, not a verdict** — both failure directions are safe (wasted upload / degraded
  answer), so it is deliberately *not* a shared copy of the server's rule. The code comment says
  so, per the founder's instruction.
- `AnswerReply.text` is now `response.rendered_answer()` and nothing else.
- The raw `chat_completion` call and the `format!` splice are deleted from `answer_question`;
  the `ChatCompletionRequest` import left `main.rs` with them.

## Evidence

- Red first: `answer_turn_shows_the_answer_only_never_the_retrieval_plumbing` and
  `conversational_question_rides_the_fast_path_with_no_working_memory_upload` failed against the
  old code with the spliced `"Working memory (…"` string verbatim in the assertion diff. The
  first asserts the reply text is the answer only, `working_paths == ["main.rs"]`, exactly one
  request received, to `/deep-search`, whose `question` carries both the raw question and the
  working-memory sentinel; the second asserts a dirty-repo "hi" sends `question == "hi"` alone
  in a single request.
- `conversational_gate_decides_bandwidth_not_a_verdict` (unit) was proven fallible by a
  deliberate mutant (`!SOCIAL_TOKENS.contains`) before its green was accepted.
- After the fix: `cargo test -p estelle-tui --bin estelle` **136 passed, 0 failed** (133 baseline
  + 3 new); `cargo test -p estelle-client` **21 passed**; clippy `-D warnings` clean on both
  crates; `cargo build --release -p estelle-tui --bin estelle` clean.

## Limits, said out loud

- Proven against a **wiremock** server at the request/response level, **not** against production
  and **not** against a live terminal. The TUI ask path has no non-interactive door.
- The test at `main.rs` (was :6560) asserting `rendered.contains("Repo graph")` was inspected,
  not inverted: it covers the `/context` grounding **panel** heading `"Repo graph · team's swept
  copy"` — the legitimate disclosure surface the fix endorses — not the transcript. No existing
  test asserted the defective transcript; the defect had zero coverage, which is how it shipped.
- **Observation, not fixed (out of D1's scope):** the headless `estelle ask` path
  (`top_level.rs:1302`) still calls the raw `v1/chat/completions` endpoint with the bare question.
  It does not splice plumbing into anything, but it routes around the deep-search fast path and
  grounding certificate exactly as the TUI did. Candidate for its own defect entry.

## Register/parent-map deltas for the coach

- D1 closed. D2–D5 remain open.
- Working-memory behavior changed: it now rides `/deep-search` and is skipped for conversational
  turns. `FOUNDER-FIRST-RUN.md` and `P5-GRAFTS.md` were updated in the same commit.
