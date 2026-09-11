---
name: deep-search
description: Ask a question about your own repo and get a cited answer. Use when the task involves: deep search, deep-search, ask the codebase, natural language codebase question, question about the repo, grounded code q&a.
---

# deep-search

Estelle's Deep Search — the grounded, cited, BYOK answer to Sourcegraph's Deep Search. Sourcegraph's Deep
Search takes a natural-language question about a codebase and returns an answer with a sources list, but it
is **walled** (it runs on their model provider; BYOK is excluded) and its answer is *cited* yet never
*certified*: an answer that drifts off the retrieved sources still ships with plausible-looking citations.
Estelle packages the pieces it already has — `retrieve_cited` / `recall_cited` (`serve/memory_pipeline.py`,
`serve/memory_facade.py`, hybrid RRF), `cite_block`, and the deterministic grounding gate `check_grounding`
(`agent/grounding.py`) — into `POST /deep-search` (`handle_deep_search`, `serve/api.py`), and ADDS the two
things Sourcegraph's lacks:

* a **grounding certificate** — the finished answer is run through the deterministic gate against the repo's
  real symbol graph, so an answer that references an API the repo does not define is FLAGGED
  (`grounded=False` plus the `ungrounded` symbols), not shipped as fact; and
* **BYOK** — the answer is produced on the CALLER's own model, not a provider they are locked into.

The pure core is `deep_search_prompt` + `deep_search_result` (`serve/deep_search.py`); the model call is
injected, so the whole capability is grounded and tested without a GPU. Distinct from `/search` (raw recall +
grep, no answer) and `/verify` (grounds code the caller supplies): this ANSWERS an NL question and certifies
the answer in one call.

## When to run
When an engineer (or an agent) asks a natural-language question about a codebase — "where is auth enforced?",
"how does the retry loop work?", "what calls `charge`?" — and needs an answer they can trust enough to act on,
not just a ranked file list. Reach for it over a bare recall whenever the answer will be quoted, pasted into a
PR, or fed to another agent: the certificate is what lets the answer cross that trust boundary.

## Procedure
1. **Recall the cited evidence, question-keyed.** Retrieve the top cited chunks over the caller's swept
   namespace with `recall_cited` (`serve/memory_facade.py` → `retrieve_cited`, hybrid RRF + selective rerank),
   keyed on the QUESTION. Each chunk carries its `source_file`, so every claim can be traced to a repo
   location — provenance is the trust half of a grounded answer.
2. **Answer grounded ONLY in that evidence, on the caller's model.** Build the focused prompt with
   `deep_search_prompt` — the recalled chunks rendered through `cite_block` (each headed by its `[source]`),
   with the instruction to answer using ONLY that context, cite the file per claim, and say so plainly when
   the context does not contain the answer rather than answer from the model's own memory. Call the CALLER's
   own backend (BYOK) — their tokens, never ours.
3. **Certify with the deterministic grounding gate.** Run `check_grounding` / `memory.check` on the answer
   against the repo's real symbol graph plus exactly the recalled context the model saw. A repo-scoped symbol
   the repo never defines is a hallucinated API — the single failure that erodes trust fastest — so a drifted
   answer comes back `grounded=False` with the offending symbols in `ungrounded`. The gate costs zero model
   calls on a clean answer; it fires only when the answer went off-source.
4. **Return the certified result.** Emit `deep_search_result`: `{answer, sources:[{file, line?}], grounded,
   ungrounded, citations}` — the answer, the sources it was grounded in, the certificate, and the raw cited
   block. Ship the answer only when `grounded` is true; when it is false, surface the flag and the ungrounded
   symbols so the caller sees the answer drifted rather than acting on it.

## Output
A natural-language answer to a codebase question, each claim cited to a repo file, carrying a grounding
CERTIFICATE (`grounded` + the `ungrounded` repo symbols the answer invented) — produced on the caller's own
model. Where Sourcegraph's Deep Search is cited-but-walled and never certifies grounding, this is
cited-AND-gated and BYOK: an answer that drifts off its sources is flagged, not shipped, and it runs on the
customer's own codebase and model.
