# Rival-CLI port receipt — permission and reliability mechanisms

**Date:** 2026-09-01 · **Branch:** `coach/rival-port-20260901` · **Base:** `f6fd5cc1bde9cc9fadee0608770dd64ec7ab0a15`
(`origin/coach/r11-cli-integration`, read back with `git ls-remote` before starting).
**Source teardown:** `docs/cli-design/RIVAL-CLI-TEARDOWN-2026-09-01.md` (in the PARENT repo, not this one).
**Scope:** three shipped changes in `core/` and `execpolicy/`; one receipt-only patch for a lane-owned file.

---

## 0. THE MAP — what our inherited Codex approval tree already does

This section decided the rest of the work. **Six of the twenty-two take-list items are already
implemented here**, and two of those are implemented *better* than the rival they were taken from.
Every row below is a file I opened in this worktree.

### 0.1 The brief's framing needs one correction

The brief said `protocol/src/permissions.rs` (3,415 lines) is part of "Codex's entire approval tree."
**It is not an approval-decision module at all.** It is the filesystem/network *sandbox policy*
module: `FileSystemSandboxPolicy`, `FileSystemAccessMode`, `NetworkSandboxPolicy`, `ReadDenyMatcher`,
`can_write_path_with_cwd`. There is no `PermissionResult`, no rule evaluator and no ask/allow/deny
vocabulary in it (`protocol/src/permissions.rs:78-1311`, every `pub fn` enumerated). The actual
approval decision path is elsewhere, and it is this:

| stage | owner | `file:line` |
|---|---|---|
| the human's / reviewer's ANSWER | `ReviewDecision` (7 variants) | `protocol/src/protocol.rs:4113-4145` |
| the stored POLICY | `Decision { Allow, Prompt, Forbidden }` | `execpolicy/src/decision.rs:9-15` |
| policy → requirement | `ExecPolicyManager::create_exec_approval_requirement_for_command` | `core/src/exec_policy.rs:368-500` |
| the requirement | `ExecApprovalRequirement { Skip, NeedsApproval, Forbidden }` | `core/src/tools/sandboxing.rs:229-248` |
| enforcement / short-circuit | `ToolOrchestrator` | `core/src/tools/orchestrator.rs:197-199` |
| session "always" cache | `ApprovalStore` + `with_cached_approval` | `core/src/tools/sandboxing.rs:41-190` |
| patch safety | `assess_patch_safety` → `SafetyCheck { AutoApprove, AskUser, Reject }` | `core/src/safety.rs:20-110` |
| await the human | `Session::request_command_approval` / `request_patch_approval` | `core/src/session/mod.rs:2296-2419` |
| UI-agnostic preset table | `builtin_approval_presets()` | `utils/approval-presets/src/lib.rs` (77 lines) |

`shell-command/src/command_safety/` (2,901 lines) is a **classifier**, not a policy: `is_known_safe_command`
and `dangerous_command_match` feed `render_decision_for_unmatched_command`
(`core/src/exec_policy.rs:794-870`) as the heuristics fallback. `tui/src/permission_compat.rs` is 95
lines of compat shim. The brief's "grep the capability, not the location" holds: the capability is
`exec_policy` + `ReviewDecision`, not `permissions.rs`.

### 0.2 Take-list items ALREADY IMPLEMENTED here — skipped, with the citation

| # | Item | Status here | Evidence |
|---|---|---|---|
| **3** | `PermissionResult` with `Timeout` DISTINCT from deny | ✅ **ALREADY DONE, and richer than jcode's** | `ReviewDecision::TimedOut` is its own variant (`protocol/src/protocol.rs:4139-4140`) alongside `Denied{rejection}`, `Abort`, `ApprovedForSession`, `ApprovedExecpolicyAmendment`, `NetworkPolicyAmendment` — **7 variants vs jcode's 4**. It is handled distinctly at 6 independent consumers: `core/src/guardian/review.rs:481`, `core/src/mcp_tool_call.rs:1525`, `core/src/tools/approvals.rs:182`, `core/src/tools/network_approval.rs:1004`, `core/src/tools/runtimes/shell/unix_escalation.rs:558`, `core/src/codex_delegate.rs:812`. `to_opaque_string()` emits `"timed_out"`, not `"denied"` (`protocol/src/protocol.rs:4177`). **A timeout does NOT collapse into deny.** |
| **6** | Most-restrictive-wins across a multi-resource/multi-command request | ✅ ALREADY DONE | `Evaluation::from_matches` takes `matched_rules.iter().map(RuleMatch::decision).max()` (`execpolicy/src/policy.rs:366`) over a `Decision` declared `Allow < Prompt < Forbidden`. **Was untested — now pinned, see item B.** |
| **2** | A saved "always" cannot override a configured deny | ✅ ALREADY DONE, two independent mechanisms | (a) `ExecApprovalRequirement::Forbidden` returns `ToolError::Rejected` at `core/src/tools/orchestrator.rs:197-199`, which is **before** `start_approval_async` / `with_cached_approval` is ever reached — so no "always" button is even offered for a denied command. (b) `append_amendment_and_update` writes an `allow` prefix rule (`core/src/exec_policy.rs:503-548`) but a matching `Forbidden` still wins via the `max` above. **Was untested — now pinned, see item B.** |
| **11** | The blanket "yes" flag must not answer the dangerous class | ✅ ALREADY DONE | `render_decision_for_unmatched_command` returns `Decision::Forbidden` (not Allow) for a `dangerous_command_match` under `AskForApproval::Never` (`core/src/exec_policy.rs:839-847`). This is aider's `explicit_yes_required` inversion, in policy rather than in the prompt loop. |
| **15** | One UI-agnostic preset table pairing approval policy with permission profile | ⚠️ **PRESENT AND WIRED — but only ONE consumer** | The crate exists (`utils/approval-presets/`) and is a real dependency of the TUI (`tui/Cargo.toml:62`), used at `tui/src/chatwidget.rs:478-479`, `tui/src/app.rs:161`, `tui/src/app/config_persistence.rs:10`, `tui/src/chatwidget/windows_sandbox_prompts.rs:466`. **Settles teardown §12 item 2.** But `grep -rln codex-utils-approval-presets --include=Cargo.toml` returns exactly three files: the workspace root, `tui/`, and the crate itself. `mcp-server/`, `app-server/`, `estelle-mcp/`, `exec/`, `cli/` and `core/` do not consume it. Codex's own module doc says *"Keep this UI-agnostic so it can be reused by both TUI and MCP server"* — we have the one owner and **one** reader, so the anti-drift property is half-realised. Not fixed here; named. |
| **17** | Disable the composer only for an unanswered modal, never for "busy" | ⚠️ **MOSTLY DONE — one clause missing.** See §D. | Three disable sites, all "an unanswered modal owns the keyboard", none of them busy: `tui/src/bottom_pane/mod.rs:1476` ("Answer the questions to continue."), `:1546` ("Respond to the tool suggestion to continue."), `:1563` ("Respond to the MCP server request to continue."); re-enabled at `:1611`. **Busy appears in none of them.** opencode's memo is `permissions().length > 0 \|\| questions().length > 0`; we implement the *questions* half and not the *permissions* half. |

**What this means for the estimate:** items 2, 3, 6, 11 needed **zero construction** — only tests. Item 1
needed a real fix. That is why this lane shipped in hours rather than days.

### 0.3 What is genuinely NOT here (measured, not assumed)

- **No `decided_via` / provenance field on any approval decision** anywhere in the tree before this
  change. `ReviewDecision` records *what* was decided, never *who or what* decided it.
  → **Fixed, item C.**
- **No approval-latency instrument** (kimi-cli #10). `codex.approval.requested` is a counter with no
  duration and no surface dimension. Not fixed; out of scope.
- **No out-of-band permissions queue screen** (jcode #13). Nothing resembling it; the approval
  surface is an in-process overlay only.
- **No `foreground_turn` vs `background_agent` distinction on an approval request** (kimi-cli #14).
- **No conformance-suite pattern** (pi #8) — no `{group, name, run()}` array registered against both
  a fake and a real backend anywhere in the workspace.
- **No dead-session sweep for orphaned approval requests** (jcode #4). A dropped oneshot becomes
  `ReviewDecision::Abort` (`core/src/session/mod.rs:2377, 2419`), which is at least distinct from
  `Denied` — but nothing expires a request whose owner died, because requests are in-process only.

---

## A. Take-list #1 — a corrupt trust file silently allowed what it denied

**Source:** goose `crates/goose/src/config/permission.rs:49-59` (Apache-2.0) — *idea only, no code copied*.
Shaped by goose's monotone inspector ratchet (`crates/goose/src/tool_inspection.rs:252-256`, take-list #5).

### The defect

`load_exec_policy_with_warning` (`core/src/exec_policy.rs:684-698`) catches `ExecPolicyError::ParsePolicy`
and substitutes a fallback policy — the enterprise-requirements overlay, or `Policy::empty()`. The
parser fails per-FILE, so **every rule in that file is dropped, including every `deny` the operator
wrote.** `ExecPolicyManager::load` then swallowed the warning into a `tracing::warn!` and started.

Because an unmatched command falls through to `render_decision_for_unmatched_command`, which returns
`Decision::Allow` under `AskForApproval::Never` for anything not on the built-in dangerous list
(`core/src/exec_policy.rs:849-854`), **the parse failure ended up more permissive than the file it
could not read.**

**The guard exists and runs on the wrong path.** `codex exec` refuses to start
(`exec/src/lib.rs:466-476`, `eprintln!` + `std::process::exit(1)`). The interactive path does not:
`app-server/src/lib.rs:619-621` pushes a non-fatal `config_warnings` entry, and
`app-server/src/in_process.rs:356-363` — the path the TUI actually takes via
`estelle_tui::run_main` → in-process app-server — downgrades it to the notification
*"Error parsing rules; custom rules not applied."* and continues. `grep -rn check_execpolicy_for_warnings tui/src/`
returns **zero hits**. That is CLAUDE.md's *"a guard that runs in one session and not another has a
coverage hole where it matters."*

### Red first — the quoted failure

```
thread 'exec_policy::tests::corrupt_rules_file_must_not_make_a_denied_command_allowed'
panicked at core/src/exec_policy_tests.rs:2482:5:
a rules file that FAILED TO PARSE must not leave the runtime more permissive than the file
it could not read; got Skip { bypass_sandbox: false, proposed_execpolicy_amendment:
Some(ExecPolicyAmendment { command: ["curl", "https://example.com"] }) }
```

`Skip` means *no approval required*. The fixture is a single `user.rules` containing
`prefix_rule(pattern=["curl"], decision="forbidden")` followed by a syntax error — so the operator's
deny is real, and it is dropped. It also proposed an amendment to make the allow permanent.

### What changed

| `file:line` | change |
|---|---|
| `core/src/exec_policy.rs:292-306` | **new** `ratchet_decision_for_degraded_policy(decision, approval_policy) -> Decision`. Monotone by construction: `decision.max(floor)`, with `floor = Forbidden` under `Never` (nobody to prompt) and `Prompt` otherwise. |
| `core/src/exec_policy.rs:314` | **new** field `ExecPolicyManager::degraded: bool`. |
| `core/src/exec_policy.rs:338-344` | **new** `ExecPolicyManager::new_degraded`. |
| `core/src/exec_policy.rs:349-361` | `load` now branches on the warning: a `ParsePolicy` warning constructs a **degraded** manager and logs *"running with a degraded exec policy: no command will be auto-approved"*. |
| `core/src/exec_policy.rs:390-412` | the `exec_policy_fallback` closure ratchets its result when degraded. |
| `core/src/exec_policy_tests.rs:2437-2550` | three tests (below). |

Deliberately **not** done: making the TUI abort. Aborting the interactive binary on a malformed
user-authored rules file is a worse product than degrading to "ask about everything", and the
ratchet is strictly fail-closed either way.

### Five states

- **built** — yes, `core/src/exec_policy.rs:292-306, 314, 338-344, 349-361, 390-412`.
- **wired** — yes. `ExecPolicyManager::load` is called from `core/src/session/mod.rs:592` and
  `core/src/session/tests/guardian_tests.rs:700`; the ratcheted closure is inside
  `create_exec_approval_requirement_for_command`, which every shell / unified-exec / apply-patch
  approval goes through via `ToolOrchestrator`. A real caller reaches it from the running binary.
- **tested** — yes, red-first, 3 tests: the defect
  (`corrupt_rules_file_must_not_make_a_denied_command_allowed`), the **exemption shape**
  (`healthy_policy_is_not_ratcheted` — an intact policy must NOT be ratcheted, or the fix would
  silently turn every `Never` session into a prompting session), and the monotonicity property over
  the full 3 × 3 cross-product (`degraded_ratchet_never_grants`).
- **SHIPPED-IN-PREVIEW** — **NO.** Committed on `coach/rival-port-20260901` only. No binary was
  built or published. Nothing merged to `coach/r11-cli-integration` or `main`.
- **PROBED** — **NO.** Not exercised through a running binary; no `cargo build --release`, no
  hand-run of the TUI with a broken `.rules` file. The evidence is the test, not a probe.

### Limits

1. **This ratchets the HEURISTIC fallback only.** Admin-authored rules from the requirements overlay
   that still match are honoured unchanged. That is deliberate and already pinned by the
   pre-existing `malformed_custom_rules_preserve_requirements_exec_policy`. An enterprise `allow`
   therefore still allows on a degraded policy — correct, but worth saying out loud.
2. **The degraded flag is set at load and never re-evaluated.** If a config reload path exists that
   does not go through `ExecPolicyManager::load`, it will not set the flag. I did not enumerate
   reload paths; `append_amendment_and_update` mutates the policy in place and does not clear or set
   `degraded`, which is the safe direction (a degraded manager stays degraded).
3. **`app-server`'s and `in_process`'s config-warning text is unchanged.** It still says *"custom
   rules not applied"*, which is now an understatement — the runtime is also refusing to
   auto-approve. Correcting that string is a one-line follow-up in `app-server/`.
4. **Windows paths untested here.** `render_decision_for_unmatched_command` has `cfg!(windows)` arms;
   my tests ran on macOS only.

---

## B. Take-list #2 and #6 — "deny beats allow" was correct by accident and untested

**Source:** opencode `packages/core/src/permission.ts:147-160` (MIT, Copyright (c) 2025 opencode) —
*idea only, no code copied.*

### The finding

The guarantee **already holds** (see §0.2), and it rode entirely on the *declaration order* of
`Decision` (`execpolicy/src/decision.rs:9-15`) plus a bare `.max()` at `execpolicy/src/policy.rs:366`.
Nothing in the tree asserted either. Reordering two enum variants — a change no reviewer would flag —
would silently turn every configured `deny` into an `allow`, with no test going red. That is the
naked-invariant shape.

**This is green-first, not red-first, and I am not going to pretend otherwise:** the behaviour is
correct today, so a test of the behaviour passes immediately. Its value is regression-pinning. To
satisfy "prove the instrument can fail" I ran a mutant instead.

### The mutant proof

Swapped `Allow` and `Forbidden` in `Decision`'s declaration order. Result:

```
failures:
    a_saved_always_allow_cannot_override_a_configured_deny
    decision_ordering_is_least_to_most_restrictive
    one_denied_command_in_a_batch_denies_the_batch

test result: FAILED. 1 passed; 3 failed; 0 ignored; 0 measured
```

with the diff on the batch case reading `< Forbidden / > Allow`. The **one that stayed green is the
negative control** — `a_saved_always_allow_still_works_when_nothing_denies_it` — which is exactly
right: the rule is *"deny wins"*, not *"the policy always refuses"*. Without that control the suite
would pass on a policy that denies everything.

Mutant reverted with `git show HEAD:execpolicy/src/decision.rs > execpolicy/src/decision.rs` (never
`git checkout --`), and `git diff` on that file is empty. **`execpolicy/src/decision.rs` is
unmodified in this branch.**

### What changed

`execpolicy/tests/saved_always_cannot_override_deny.rs` — **new file, tests only, no production
change.** Four tests:

1. `decision_ordering_is_least_to_most_restrictive` — pins `Allow < Prompt < Forbidden` and that
   `max` means *more restrictive*.
2. `a_saved_always_allow_cannot_override_a_configured_deny` — the adversarial case the brief asked
   for: save an "always allow" for `curl`, configure an explicit `forbidden` for `curl`, assert
   `Forbidden` — **in both insertion orders**, so the result cannot depend on which rule was added
   first.
3. `a_saved_always_allow_still_works_when_nothing_denies_it` — the exemption shape / negative control.
4. `one_denied_command_in_a_batch_denies_the_batch` — opencode's most-restrictive-resource rule over
   a multi-command evaluation (`ls` allowed + `curl` denied ⇒ denied).

**This also pins item A**, whose `decision.max(floor)` depends on the same ordering to mean
"more restrictive". The two changes are coupled and the coupling is now tested.

### Five states

- **built** — n/a (test-only).
- **wired** — the *invariant* is wired: `Evaluation::from_matches` is on every policy evaluation
  path. The *test* runs in `cargo test -p codex-execpolicy`.
- **tested** — yes, 4 tests, mutant-killed 3/4 with the control correctly surviving.
- **SHIPPED-IN-PREVIEW** — **NO.**
- **PROBED** — **NO.**

### Limits

1. **Green-first.** Stated above; the mutant is the substitute for a red, not an equal.
2. **The mutant I ran is one mutant.** It proves the tests see a *variant reorder*. It does not prove
   they would see, say, `from_matches` being changed to `.min()` — though
   `decision_ordering_is_least_to_most_restrictive` would not catch that one and
   `a_saved_always_allow_cannot_override_a_configured_deny` would. I did not run that second mutant.
3. **This is the `execpolicy` layer only.** opencode's stronger property — *evaluate the configured
   ruleset ALONE first, and only merge the user's saved answers if it did not deny* — is a different
   mechanism from ours. Ours reaches the same outcome by ordering `Decision`, which is arguably
   better (no two-pass evaluation) but means **the two rule sources are not separable**: we cannot
   answer *"would this have been allowed by configuration alone?"* the way opencode can. Nothing
   here fixes that, and it is the honest gap behind the ✅ in §0.2.
4. **`Decision::parse` and the Starlark parser disagree on the deny keyword** —
   `execpolicy/src/decision.rs:23` accepts `"forbidden"`, `execpolicy/src/parser.rs:255` accepts
   `"deny"`. Both work in their own call path; noted, not touched.

---

## C. Take-list #3 (the audit half) and #10 — a cached "always" was answered with no audit event

**Source:** jcode `crates/jcode-base/src/safety.rs:76-84` (MIT, Copyright (c) 2025 Jeremy Huang),
where `decided_via` is a **non-optional** field; and kimi-cli
`src/kimi_cli/approval_runtime/models.py:37` (Apache-2.0), `approved_via_session_cache: bool`.
*Shapes taken, no code copied.*

### The defect

`with_cached_approval` (`core/src/tools/sandboxing.rs`, pre-change) emitted the
`codex.approval.requested` counter **only after `fetch().await`** — i.e. only on the branch that
actually asked someone. The early return on a session-cache hit emitted nothing at all. Two
consequences:

- the counter **under-reported** approvals by however many were served from cache, and
- **no field distinguished** "a human was asked" from "a prior 'always' answered."

An audit that cannot tell those apart cannot see rubber-stamping — which is the failure mode that
makes an approval surface theatre, and the one no other metric can see.

### Red first

`resolve_cached_approval` and `ApprovalDecidedVia` did not exist; there was no value in the system
that answered *"who decided?"*. The failing run:

```
error[E0425]: cannot find function `resolve_cached_approval` in this scope
error[E0433]: cannot find type `ApprovalDecidedVia` in this scope
  --> core/src/tools/sandboxing_tests.rs:314, 318, 323, 327, 351, 356, 358 …
```

This is a **compile red, which is the weak kind**, and I am saying so rather than dressing it up. The
behavioural claim the tests then make — that a cache hit and a prompt are *distinguishable* — is real
and was unrepresentable before the change.

### What changed

| `file:line` | change |
|---|---|
| `core/src/tools/sandboxing.rs:66-94` | **new** `ApprovalDecidedVia { Prompt, SessionCache }` with `as_str()` → `"prompt"` / `"session_cache"`. |
| `core/src/tools/sandboxing.rs:96-102` | **new** `ApprovalOutcome { decision, decided_via }` — the two travel together so no caller can record one without the other. |
| `core/src/tools/sandboxing.rs:113-157` | **new** `resolve_cached_approval(&Mutex<ApprovalStore>, keys, fetch) -> ApprovalOutcome`. Takes the store directly, not `SessionServices`, so it is unit-testable with `Mutex::new(ApprovalStore::default())`. |
| `core/src/tools/sandboxing.rs:159-190` | `with_cached_approval` is now a thin wrapper that emits the counter **once, from one exit point**, carrying `("decided_via", …)`. The "forgot to count this branch" defect is now structurally impossible rather than remembered (one owner per derived fact). |
| `core/src/tools/sandboxing_tests.rs:292-389` | four tests. |

### Five states

- **built** — yes, `core/src/tools/sandboxing.rs:66-190`.
- **wired** — yes. `with_cached_approval` is called from all three real approval sites:
  `core/src/tools/runtimes/shell.rs:156`, `core/src/tools/runtimes/unified_exec.rs:201`,
  `core/src/tools/runtimes/apply_patch.rs:162`. `resolve_cached_approval` has exactly one production
  caller (`with_cached_approval`) plus the tests — which is the intent, not a dark module.
- **tested** — yes, 4 tests through the **real** `ApprovalStore` (no test double):
  `cache_hit_and_prompt_are_distinguishable_in_the_audit` (and the second call's `fetch` closure
  `panic!`s, so a re-prompt fails the test rather than passing quietly);
  `non_session_decisions_are_never_served_from_the_cache` — the **exemption shape**, over
  `Approved` / `Denied` / `TimedOut` / `Abort`, asserting none of them is promoted into a stored
  always; `empty_keys_are_reported_as_a_prompt_not_a_cache_hit`;
  `decided_via_wire_values_are_stable`.
- **SHIPPED-IN-PREVIEW** — **NO.**
- **PROBED** — **NO.**

### Limits — read this one

1. 🔴 **The tests do NOT assert the emitted metric.** They assert the `decided_via` *value*.
   `SessionTelemetry::counter` no-ops when no `MetricsClient` is configured
   (`otel/src/events/session_telemetry.rs:164-168`) and this repo has no capturing sink I could
   attach without standing up an OTLP exporter. So: **the provenance is a tested value; the tag
   arriving at a backend is unverified.** A hostile reader should say "you tested the input to the
   counter, not the counter" — correct.
2. **`decided_via` is telemetry, not a ledger.** jcode's `Decision` is a persisted record with
   `request_id` and `decided_at`; ours is a metric dimension on a counter. There is still **no
   durable approval-decision ledger** in this tree (hermes-agent's admitted gap, take-list #19,
   which depends on one). Not built.
3. **`ApprovalDecidedVia::Prompt` conflates a human with the guardian auto-reviewer.** Both arrive
   through `fetch()`. Splitting them needs a third variant and a signal from
   `ApprovalReviewer::{Guardian, …}` at `core/src/tools/orchestrator.rs:182-216`; not done.
4. **No latency.** kimi-cli's `duration_ms` + `approval_surface` (take-list #10's actual headline) is
   NOT implemented. Only the provenance half is. The rubber-stamping detector this enables is
   partial: it can tell you *how many* approvals nobody was asked about, not *how fast* the ones
   people did see were answered.
5. **Only the exec/patch approval path.** MCP tool approvals keep their own cache
   (`core/src/mcp_tool_call.rs:1941-1947`) and do **not** go through `with_cached_approval`, so they
   emit no `decided_via`. Same defect, second location, not fixed.

---

## D. Take-list #17 — the composer rule: PATCH ONLY, lane-owned file, NOT APPLIED

`tui/src/bottom_pane/` is owned by another lane. **I have not edited it.** Below is the exact patch
and the exact finding, for that lane to apply.

### The finding

Our rule is opencode's, minus one clause. Three call sites disable the composer, and every one of
them is "an unanswered modal owns the keyboard" — **"busy" appears in none of them**, which is the
whole point of opencode `packages/tui/src/routes/session/index.tsx:242`:

- `tui/src/bottom_pane/mod.rs:1476` — user-input questions
- `tui/src/bottom_pane/mod.rs:1546` — tool-suggestion modal
- `tui/src/bottom_pane/mod.rs:1563` — MCP server elicitation
- re-enabled at `tui/src/bottom_pane/mod.rs:1611` (`on_active_view_complete`)

opencode's memo is `permissions().length > 0 || questions().length > 0`. **We implement the questions
half and not the permissions half.** Both sites that push the `ApprovalOverlay` call
`pause_status_timer_for_modal()` and `push_view(...)` and *not* `set_composer_input_enabled(false, …)`:

- `tui/src/bottom_pane/mod.rs:1441-1450` (immediate path, inside `push_approval_request`)
- `tui/src/bottom_pane/mod.rs:601-612` (delayed path, inside `maybe_show_delayed_approval_requests_at`)

This is the "partial guard reporting complete" species: the rule is present in the file three times,
and the clause nobody wrote is the permission one.

### ⚠️ The limit on this finding — stated before the patch, not after

**This is a placeholder/render inconsistency, NOT a keystroke leak.** `BottomPane::handle_key_event`
routes every key to the top view whenever `view_stack` is non-empty
(`tui/src/bottom_pane/mod.rs:616-618`), so an approval overlay already owns the keyboard. The
observable defect is that the composer keeps showing its normal placeholder ("Ask Codex to do
anything") while an approval is unanswered, where the other three modals say what is owed. I did not
find a path where a keystroke reaches the composer during an approval, and I did not exhaustively
prove none exists.

### The exact patch

```diff
--- a/tui/src/bottom_pane/mod.rs
+++ b/tui/src/bottom_pane/mod.rs
@@ (inside maybe_show_delayed_approval_requests_at, ~:610)
         while let Some(delayed) = self.delayed_approval_requests.pop_back() {
             modal.enqueue_request(delayed.request);
         }
         self.pause_status_timer_for_modal();
+        // An UNANSWERED PERMISSION owns the keyboard, same as an unanswered
+        // question: opencode disables the composer for exactly these two and
+        // never for "busy" (packages/tui/src/routes/session/index.tsx:242).
+        self.set_composer_input_enabled(
+            /*enabled*/ false,
+            Some("Respond to the approval to continue.".to_string()),
+        );
         self.push_view(Box::new(modal));
     }
@@ (inside push_approval_request, ~:1448)
             self.pause_status_timer_for_modal();
+            self.set_composer_input_enabled(
+                /*enabled*/ false,
+                Some("Respond to the approval to continue.".to_string()),
+            );
             self.push_view(Box::new(modal));
         }
     }
```

`on_active_view_complete` (`:1609-1612`) already re-enables unconditionally, so no matching change is
needed on the way out.

**The test that lane should write** (and the reason to write it as a *negative* too): assert the
composer is disabled while an approval overlay is active, **and assert it is NOT disabled merely
because a turn is running** — a positive-only test here would pass on a build that disables the
composer for busy, which is the thing we most want to never do.

### Five states

built **NO** · wired **NO** · tested **NO** · SHIPPED-IN-PREVIEW **NO** · PROBED **NO**.
This item is a patch in a document. Nothing was changed.

---

## E. Licences, NOTICE files, and attribution — settles teardown §12 item 4

I copied **no source** from any vendored repo. Every line in this branch is written here. The
citations in the code comments are provenance-of-idea, which copyright does not reach — but the
teardown flagged two `NOTICE` files as unread and CANNOT DETERMINE, so I read them.

| repo | licence | NOTICE | requirement, and our state |
|---|---|---|---|
| **codex** | Apache-2.0 | **read**: *"OpenAI Codex / Copyright 2025 OpenAI"* + Ratatui MIT (Florian Dehau 2016-2022, The Ratatui Developers 2023-2025) | **Already satisfied.** `cli-rs/NOTICE` carries all of it verbatim plus a fork-boundary note. |
| **kimi-cli** | Apache-2.0 | **read**: *"Kimi Code CLI / Copyright 2025 Moonshot AI"*, and it declares it reuses Apache-2.0 code from OpenAI Codex (`src/kimi_cli/skills/skill-creator/SKILL.md`) | **Already satisfied** — `cli-rs/NOTICE` lists *"Kimi Code CLI … Copyright 2025 Moonshot AI, licensed under the Apache License, Version 2.0."* |
| **goose** | Apache-2.0 | **no NOTICE file exists** (`ls vendor-reference/goose/NOTICE` → No such file). Apache-2.0 §4(d) therefore imposes no NOTICE obligation. | `cli-rs/NOTICE` lists goose (Copyright 2024 Block, Inc.) anyway. |
| **opencode** | MIT | n/a | Copyright line quoted in the test-file header and the commit message. Not in `NOTICE` because no code was taken; add it the day any is. |
| **jcode** | MIT | n/a | Copyright line quoted in the code comment and commit message; `cli-rs/NOTICE` already lists jcode. |

⚠️ **One live claim in our own NOTICE, checked:** it says *"The planned Estelle CLI grafts also
reference the following projects. **No source from these projects is included in P0**"* for goose,
jcode and Kimi. **That statement is still true after this branch** — I took shapes and ideas, not
lines. If a future lane ports jcode's `record_permission_via_file` or goose's `tool_inspection.rs`
verbatim, that sentence becomes false the same day and must change with the code.

⛔ **`pi` / `oh-my-pi`:** I read nothing from `packages/ai/src/{auth,api,registry/oauth,providers,usage}`
in either repo. No fingerprint, header-builder, usage or auth code was opened, cited or ported.

---

## F. Test counts — measured at my own base, not inherited

The brief quoted "~3,288 with 6 pre-existing failures (4 `plan_mode`, 2 insta snapshots)".
**That number does not reproduce at this base.** Measured here:

| suite | BASE `f6fd5cc1b` | AFTER (this branch) | delta |
|---|---|---|---|
| `cargo test -p codex-core --lib` | **2147 run, 2142 passed, 5 failed** | **2154 run, 2149 passed, 5 failed** | **+7 tests, +7 passed, +0 failures** |
| `cargo test -p codex-execpolicy` (4 targets) | 7 + 0 + 27 = 34 passed, 0 failed | 7 + 0 + 27 + 4 = 38 passed, 0 failed | +4, all green |

**Both runs require `RUST_MIN_STACK=33554432`** — see the stack-overflow finding below.

### The 5 pre-existing `codex-core` failures — named, and proven not mine

Byte-identical set before and after:

```
config::schema::tests::config_schema_matches_fixture
config::tests::to_mcp_config_preserves_apps_feature_from_config
guardian::tests::guardian_ephemeral_retry_preserves_parallel_trunk_and_fork_history
session::tests::fork_startup_context_then_first_turn_diff_snapshot
session::turn::tests::post_sampling_token_estimate_is_disabled_by_always_on_sinks
```

Established empirically, not by reasoning: I reverted all four of my changed `core/` files to the base
blob (`git show f6fd5cc1b:<path> >`), re-ran the full suite, and got the same five names. Then
restored from `git show HEAD:<path> >` and confirmed `git status --short` clean.

### 🔴 A second finding, not in the brief: two `agent::control` tests abort the whole suite by default

```
thread 'agent::control::residency::tests::interrupted_v2_agent_is_lost_after_residency_eviction'
  has overflowed its stack
fatal runtime error: stack overflow, aborting        (SIGABRT)

thread 'agent::control::tests::resume_agent_releases_slot_after_resume_failure'
  has overflowed its stack
fatal runtime error: stack overflow, aborting        (SIGABRT)
```

Both reproduce **in isolation** on the default macOS libtest thread stack, and both reproduce with my
changes fully reverted to `f6fd5cc1b` — **pre-existing, not mine.** They are not "5 failures", they
are a **process abort**: `cargo test -p codex-core --lib` with no env override never reaches a
`test result:` line at all, so **the suite has no default-configuration test count on this platform.**
`RUST_MIN_STACK=33554432` clears both.

⚠️ This matters more than a flaky test: a run that SIGABRTs at test #11 of 2154 prints ten `ok` lines
and then dies. Anyone eyeballing the head of that output sees green. Whichever lane owns
`core/src/agent/control/` should either raise the stack in the test harness or shrink the future —
and CI should be checked for whether it is silently in this state.

---

## G. Files changed

```
core/src/exec_policy.rs                                  +71   -5
core/src/exec_policy_tests.rs                           +116    0
core/src/tools/sandboxing.rs                             +92  -19
core/src/tools/sandboxing_tests.rs                      +100    0
execpolicy/tests/saved_always_cannot_override_deny.rs   +123    0   (new)
docs/cli-design/RIVAL-PORT-RECEIPT-2026-09-01.md         new       (this file)
```

**Not touched:** anything under `tui/src/` (§D is a patch, not an edit); `tui/src/live_renderer.rs`,
`tui/src/main.rs`, `tui/src/session_view.rs`, `tui/src/bottom_pane/*` — all lane-owned.
`execpolicy/src/decision.rs` was mutated for the mutant proof and restored; `git diff` on it is empty.
Zero box-drawing or corner glyphs added; no bordered panel rendered; `tui/src/box_glyphs.rs` untouched
and un-exempted.

`cargo fmt` applied to both crates. `cargo clippy -p codex-core --lib --tests` → **exit 0, zero
warnings**. `cargo clippy -p codex-execpolicy --all-targets` → the 3 pre-existing lib-level
`does not refer to a reachable type` warnings and **no new ones**.

---

## H. What a human must do to see each change

Nothing here has been built into a binary or run outside `cargo test`. In order:

```bash
cd /path/to/cli-rs && git fetch origin && git checkout coach/rival-port-20260901
```

**A — the corrupt trust file no longer allows what it denied.** Two ways.

*Fast (the test):*
```bash
cargo test -p codex-core --lib exec_policy::tests::corrupt_rules_file_must_not
cargo test -p codex-core --lib exec_policy::tests::healthy_policy_is_not_ratcheted
```
To watch it fail without the fix, delete the `if degraded { … }` arm at `core/src/exec_policy.rs:403-411`
and re-run: the first test returns `Skip { … }` again.

*Real (in the running binary — the probe I did NOT do):*
```bash
mkdir -p ~/.codex/rules
printf 'prefix_rule(pattern=["curl"], decision="forbidden")\nprefix_rule(\n' > ~/.codex/rules/user.rules
cargo build --release -p codex-cli          # ONE build; ~80GB debug dir if you use --debug
./target/release/<binary>                    # then ask it to run `curl https://example.com`
```
Expected after: it asks (or refuses), and the log line reads *"running with a degraded exec policy:
no command will be auto-approved"*. Expected before: it just runs. **Remember to delete
`~/.codex/rules/user.rules` afterwards** — it is a deliberately broken trust file.

**B — deny beats allow, and the guard can fail.**
```bash
cargo test -p codex-execpolicy --test saved_always_cannot_override_deny     # 4 pass
```
To reproduce the mutant kill: swap the `Allow` and `Forbidden` lines in `execpolicy/src/decision.rs:9-15`,
re-run (3 fail, the control passes), then
`git show HEAD:execpolicy/src/decision.rs > execpolicy/src/decision.rs`. **Not `git checkout --`.**

**C — a cached "always" is now distinguishable in the audit.**
```bash
cargo test -p codex-core --lib tools::sandboxing::tests
```
To see the tag on the wire you need a metrics backend this repo does not ship a capture for
(limit C.1) — the alternative is to read `core/src/tools/sandboxing.rs:179-187` and confirm the
single emit site carries `("decided_via", …)`.

**D — the composer rule.** Nothing to see. Hand §D to the lane that owns `tui/src/bottom_pane/`.

**Full suite, if you want the numbers yourself:**
```bash
RUST_MIN_STACK=33554432 cargo test -p codex-core --lib   # 2154 run, 2149 pass, the 5 named in §F
```
Without `RUST_MIN_STACK` it SIGABRTs at test ~11 and prints no result line. That is §F's second
finding, and it is not caused by this branch.

**Housekeeping:** this lane's `target/` directory was removed on completion.
