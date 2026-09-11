---
name: api-call-ground
description: Check a function really exists before your code calls it. Use when the task involves: tool call grounding, API call grounding, hallucinated tool call, function calling, argument arity check, resolve callee signature.
---

# api-call-ground

Estelle's tool-grounding skill — it grounds each TOOL/API CALL *before it is emitted*, not the finished code. A
model connected to tools hallucinates callees and argument shapes the way it hallucinates repo symbols; Gorilla
(Patil et al., UC Berkeley, arXiv:2305.15334) fixed that by resolving every proposed call against a real API
database instead of trusting the model. Estelle already owns that ground truth: the `CodeGraph`
(`conveyor/code_graph.py`) knows `definition_sites`, `files_defining`, and `references` for repo tools and the
exact `signatures(name)` (`Signature` = name, `max_positional`, `accepted_kwargs`) of each callable; the arity
gate `arity_violations` and `GroundingReport` (`agent/grounding.py`) already flag a call with too many
positionals or an unknown keyword; and `third_party_violations` / `installed_symbol_resolver`
(`agent/third_party.py`) catch an invented library method like `requests.fetch`. This skill wires those into a
pre-emission check on the CALL itself.

## When to run
Whenever the model is about to emit a tool call, function call, or API invocation — an agent step, an MCP tool
use, a generated client call — and before that call is dispatched or committed. Especially when the callee or its
arguments came from the model rather than a typed schema you already trust.

## Procedure
1. **Resolve the callee against real definitions.** Pull the proposed call out (`extract_code_block` in
   `agent/patching.py` when it's embedded in a block), then look the callee up: `files_defining(name)` /
   `definition_sites(name)` for a repo tool, or `installed_symbol_resolver` for a third-party library. A callee
   the graph never defines is a hallucinated tool — Gorilla's exact failure mode — so flag or abstain, never emit.
2. **Fetch the real signature.** `CodeGraph.signatures(name)` returns the callable's `Signature`(s). When there's
   exactly one, the argument shape is unambiguous ground truth; when there are several (overloaded name) the gate
   stays conservative and won't false-alarm — the same no-false-alarm rule the grounding gate keeps.
3. **AST-match every argument to the parameter list.** Run `arity_violations` over the proposed call against that
   `Signature`: too many positionals → "takes at most N positional argument(s)"; an unknown keyword → "got an
   unexpected keyword argument". This is the arity half of the grounding gate applied to a call *before* it goes
   out, not after code is written.
4. **Ground third-party and library calls too.** For a call into `requests`/`json`/`os`/etc, run
   `third_party_violations` (via `installed_symbol_resolver` over the `SAFE_MODULES` allowlist) so an invented
   method is caught with the same flag-or-abstain shape — a hallucinated library API is treated exactly like a
   hallucinated repo tool, and a non-allowlisted module is skipped rather than guessed at.
5. **Assemble one reproducible verdict.** Collect the results into a `GroundingReport` — `.ungrounded` (unknown
   callees), `.arity_errors` (bad argument shapes), `.third_party` (invented library methods), and `.is_grounded`
   — the same structure `POST /verify` returns as `{grounded, ungrounded, arity_errors}`. The call plan's
   groundedness is now a first-class signal, not a vibe.
6. **Abstain or repair on an ungrounded call.** If `is_grounded` is false, do not emit. Either abstain (say the
   tool/signature doesn't exist plainly) or run one `ground_report_and_repair` round to rewrite the call using
   only real signatures — a grounded call costs zero model calls, only a flagged one pays for a repair.
7. **Suggest the real neighbor when a call is close-but-wrong.** When the callee is a near miss, `related_symbols`,
   `locate`, and `references` surface the tool the model probably meant, so the repair round has the real API in
   front of it instead of guessing again.
8. **Gate the whole plan.** Feed the call-plan findings into `_gate_verdict` (`serve/api.py`) so an ungrounded or
   wrong-arity call is an error-severity blocker and the plan doesn't merge or execute until every call resolves.
   This is what makes it distinct from `grounded-generate` (best-of-N + self-test filtering over generated CODE):
   here the unit of grounding is each individual tool/API CALL, checked before emission.

## Output
A verified call plan: every proposed API/tool call resolved to a real `definition_sites` entry (or an allowlisted
library symbol via `installed_symbol_resolver`), each argument AST-matched to the real `Signature`'s arity and
keywords through `arity_violations`, and any ungrounded or wrong-arity call captured as
`GroundingReport.ungrounded` / `.arity_errors` / `.third_party` and blocked by `_gate_verdict` — so the model
abstains or repairs instead of emitting a hallucinated call. Distinct from `grounded-generate`: this grounds the
CALL before it is emitted, not the generated code afterward.
