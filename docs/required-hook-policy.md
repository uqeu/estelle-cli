# Required hook policy (candidate 0.4.1)

This is a source change, not a published release or a deployment receipt. A plugin
refresh does not establish which native executable the host actually invokes.

## Select and inspect the policy

After obtaining a built native runner at least version 0.4.1, run:

```sh
estelle --version
estelle install-hooks --policy required --shell-policy pause
estelle doctor
```

`required` pauses supported code edits when their demanded check cannot finish or
cannot establish its scope. Explicit `advisory` is strictly non-enforcing even if the old
`ESTELLE_HOOK_BLOCK` opt-in is present;
`disabled` explicitly reports that checks were not run. Installing without a policy
does not silently opt an existing installation into enforcement. The setting is
stored in the user's `.estelle/hook-policy.json`. `ESTELLE_HOOK_POLICY` is an explicit
override. A legacy explicit `ESTELLE_HOOK_BLOCK=0`, `false`, or `off` keeps its opt-out
unless the new environment override is explicitly selected. Doctor reports the
effective setting, not merely the value an installer attempted to save.

With no modern environment or saved policy, `ESTELLE_HOOK_BLOCK=1`, `true` or `on`
selects a separately labelled legacy flagged-only compatibility mode. It can refuse
fresh positive Python findings, but cannot enforce unknown-input admission and never
requests Stop repairs. It cannot be selected by `--policy` or saved as a modern policy;
choose required, advisory or disabled explicitly when changing policy settings.

Required installation also needs an explicit separate shell choice. The example
above selects `pause`, a high-restriction option that pauses tests/builds and other
shell programs without a verified candidate adapter. `--shell-policy observe`
keeps those commands usable but reports them as UNVERIFIED and does not enforce
all code-writing actions. Neither choice is silently selected. Existing settings
are retained when their flags are omitted. `ESTELLE_HOOK_SHELL_POLICY` is the
explicit shell override. Choose knowingly: this plugin has no runtime filesystem
boundary that can prove an arbitrary test, build, interpreter, alias or shell
startup script cannot write source.

Review new or modified hook trust in each host's `/hooks` UI and restart that host
session. Stop review is synchronous. An untrusted hook that the host does not run
cannot enforce anything. The installer preserves unrelated host settings and makes
backups; it does not silently approve host trust prompts.

## Code-changing admission

The required path reconstructs the complete candidate before sending any source:

* Codex `apply_patch` reads the actual `tool_input.command` field and uses the
  existing `codex-apply-patch` verifier against a bounded, read-only snapshot.
* Claude `Write` preserves full content. `Edit` applies the exact anchor replacement
  to the complete original file, including the host's explicit `replace_all` option.
* Every file is accounted for, including add, delete, rename source and rename
  destination. The adapter does not write any candidate into the working tree.
* Source bytes are retained, not trimmed or split into approximate added lines.
  Unsupported or ambiguous paths, binary input, symlinks, traversal, oversized
  input, duplicate patch targets, and credential-shaped source pause the action.

The runner calls the existing scoped `POST /gate`, not snippet `/verify`. Each
changed file with added code receives a separate actual-diff ruling. A changed,
nonempty surviving file also receives a separate complete-candidate-artifact
ruling, because removing a comment or string delimiter can activate an API call
without adding its line. That synthetic full-postimage diff is not the user's
historical patch. The reviewed `/gate` handler is store-free unless separately
requested status/reporting features are used; the hook does not request those.
Full candidate and dirty-tree post-images travel as `sources` and `working_tree`.
Per-file requests avoid treating one diff-global API ruling as evidence for another
file. Deletions, including pre-existing dirty deletions, travel as explicit
`working_tree.deleted` tombstones, never empty strings. Any request with tombstones
requires the server's `schema_version: 2` and exact deleted-path acknowledgment.
An older server that silently ignores this field cannot approve the action.
Acceptance requires the server's verified clean decision, exact repo/path coverage,
matching requested and indexed HEAD, complete overlay acknowledgment, and no
unadjudicated reference counts or dropped evidence. The exact local snapshot is
rechecked after the response. A timeout, malformed/legacy response, scope mismatch,
or concurrent local edit produces a valid PreToolUse denial and actionable reason. Short
synchronous checks do not automatically sleep/retry on `Retry-After`: HTTP 429/503
is surfaced as capacity/service failure immediately, not converted into a CLI login
problem or a timeout after the advertised wait. No raw error body, header or
credential is reflected into hook diagnostics. Missing credentials and HTTP401/403
have separate guidance. Retry explicitly once the underlying capacity recovers.

Limits are explicit: at most 32 represented files, 512 KiB per source, 2 MiB combined
source, one 20-second shared HTTP deadline across all files and both kinds of
ruling, and a cooperative 27-second required-check timer inside the host's
30-second row. The asynchronous timer cannot preempt synchronous filesystem reads
or diff generation. Those operations are byte-bounded, but a slow filesystem or
adversarial diff can still exceed the nominal wall-clock time; the host may then
kill the hook. This is not a hard process-level deadline. Current required mode needs the host cwd at the Git
repository root so repo-relative paths cannot be rebound. Inspection itself uses
sanitized, bounded Git subprocesses with hooks and fsmonitor disabled.

The demanded check is the existing gate's API/import/ruleset scope. It is NOT a proof
of whole-program correctness, test success, deletion impact, or arbitrary semantic
behavior. Whole-file deletion, no-op, and empty-candidate actions receive an explicit
API-introduction `not applicable`, never a fabricated verification. Deletion-only
edits of a surviving nonempty source still need the candidate-artifact ruling.
Deleting a definition may break callers; this policy does not claim otherwise.

The existing gate does not provide reference-level before/after provenance that
could safely distinguish every pre-existing unknown from newly activated code.
Consequently an unknown in a complete candidate can pause an unrelated safe edit.
The diagnostic identifies an artifact issue, not a proven newly introduced defect.
Relaxing this requires a tested server provenance contract, not subtracting aggregate
counts or allowing matching unknown messages.

Shell writes cannot be reconstructed by this adapter. Under explicit shell `pause`,
Bash/shell_command/exec_command programs are paused as unverified, including tests,
builds, interpreter scripts, redirections, sed and Git mutations. Only exact `pwd`,
`true`, and `false` have a no-candidate exemption, not a code verification. Under
explicit `observe`, generic shell remains available with an UNVERIFIED warning;
this does not close the shell-write bypass. Read/Grep/Glob remain available for
inspection where the host supplies them. Neither option is a shell proof engine:
aliases, startup scripts, alternate tools and runtime writes need an actual filesystem
enforcement boundary. Other host write tools without a verified adapter remain
unsupported; registration cannot intercept tools that a host does not expose.

## Final answer review and repair

This native candidate does not send `semantic_review` or `semantic_funding` and
cannot enable the server's optional customer-BYOK semantic checker. It makes no
hidden model/provider calls. Its Stop path consumes only the deterministic evidence
contract. Unsolicited model-only results are disclosed as probabilistic/unrequested,
never promoted to deterministic certification or confirmed contradiction. A future
semantic opt-in needs its own explicit setting and the server's existing customer
provider/funding controls; it is not activated by selecting required hooks.

Stop requests continuation/repair after an assistant turn. It cannot retract text
that the host has already streamed or guarantee a pre-display veto. This differs
from PreToolUse, which can prevent a supported edit before its tool executes.

A synchronous Stop hook sends the exact latest answer through the existing `/verify`
endpoint with the additive `verify_mode: "answer"` contract. Old servers that do not
return that contract yield an explicit unknown, not a pass. Repository claims also
need dated evidence and a demonstrably clean, unchanged checkout. The answer check
is limited to the evidence classes the server reports, not arbitrary prose truth.

Required policy requests repair of confirmed contradictions and rechecks the actual
latest text. It grants at most two repairs per turn. A repeated artifact does not
earn unlimited retries, and `stop_hook_active` does not skip review. Exhaustion emits
an explicit unresolved stop, never approval. Unknown or partially checked final
prose is disclosed with the host-supported `systemMessage` warning instead of being
blanket-blocked as a contradiction. Advisory policy shows findings without requesting
a repair; disabled policy reports that no answer review ran.

Current official Claude and Codex contracts support the warning field and Stop
continuation controls. Tests additionally exercise the repository's Codex parser.
This does not constitute observation of a particular customer's UI, native runner,
hook trust, or installed host version. A warning is not evidence that a model read it.

## Build, distribution, and ownership

Native runner, installation, and packaging code stay in `uqeu/estelle-cli`; server
verification stays in `uqeu/estelle`. Keep explicit cross-repository contract tests
rather than pretending plugin metadata is the native executable. Candidate 0.4.1
aligns the four version writers and adds a new bundle digest; older digests remain
immutable.

Merging native main runs CI but does not publish this runner. The manual release
workflow creates GitHub release assets and then publishes npm. Do not dispatch it
under an authorization limited to code changes in these repositories. Publication
outside the authorized repositories requires the owner to take that action. Until
a compatible runner is actually distributed and selected by a trusted hook, these
fixes are not active in an existing installation.
