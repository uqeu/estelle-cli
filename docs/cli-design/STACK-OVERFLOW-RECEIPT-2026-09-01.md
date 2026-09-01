# STACK OVERFLOW IN `codex-core --lib` — RECEIPT, 2026-09-01

**Base:** `f6fd5cc1bde9cc9fadee0608770dd64ec7ab0a15` (`origin/coach/r11-cli-integration`, read back with
`git ls-remote`). **Branch:** `coach/stack-overflow-20260901`.
**Machine:** aarch64-apple-darwin (Darwin 25.3.0), rustc 1.95.0, cargo 1.95.0, `test` profile, `ulimit -s` 8176 KiB.

---

## 1. The headline correction

The brief said **two tests**. The measured answer is **at least 36 named tests plus at least one Tokio
worker thread**, and the reason nobody knew is the defect itself: the process aborted at test 11, so
tests 12..2147 never ran and could not report. Every count anyone has produced for this crate by reading
a `cargo test` tail is a count of *what ran before the abort*, not of what exists.

| Question | Answer |
|---|---|
| Is it recursion? | **No.** The faulting backtrace is **54 frames**, every one a distinct function. |
| Is it bounded? | **Yes** — the depth is a compile-time property of the call graph. |
| Measured peak stack | **2,106,480 bytes summed from the frame prologues**; bisect says the chain **overflows at 2,112 KiB and passes at 2,128 KiB**. |
| What it is given | libtest hands a spawned test thread exactly **2,048 KiB**. It misses by ~64–80 KiB. |
| Was 32 MiB needed? | **No.** 4 MiB clears it; 2,128 KiB clears it. `RUST_MIN_STACK=33554432` was ~16x the requirement. |

---

## 2. The two tests named, and the 34 behind them

### The one that aborts today

`core/src/agent/control/residency_tests.rs` — `interrupted_v2_agent_is_lost_after_residency_eviction`.

### The sibling, which was not healthy — it was lucky

`core/src/agent/control/residency_tests.rs` — `residency_slot_reservation_unloads_oldest_idle_v2_agent`
passes at **2,032 KiB on a 2,048 KiB stack: 16 KiB of headroom, 0.78%.** One struct field away from the
same abort. Both take the bound.

### The 34 that the abort was hiding

Sweeping with `--skip` and restarting after each abort (bounded loop, `MAX_ROUNDS`), at the default stack:

- **34 named tests in `agent::control::tests`** overflow — every `spawn_agent_*`, `resume_agent_*`,
  `list_agent_subtree_*`, `ensure_v2_agent_loaded_*` case. Full list reproducible with the sweep below.
- The sweep then **stops** on a thread named `tokio-rt-worker`, which is not a test name, cannot be
  `--skip`ped, and cannot be attributed to one test. Tokio gives a worker thread the same 2 MiB.
  **No per-test `stack_size()` can reach that thread.** That single fact is why the fix is
  workspace-level and not test-by-test.

⚠️ The 34 is a **lower bound in one direction only**: the sweep stops at the first unskippable thread, so
the remaining error points *up*. There may be more named tests over the line behind the worker-thread abort.

---

## 3. The defect at `file:line`

Faulting chain, innermost first, with the stack each frame allocates (read out of each function's
prologue in the `test`-profile binary; on aarch64 a large frame is a probe **loop**,
e.g. `sub x9, sp, #0x1b, lsl #12` = 110,592 bytes at frame #7):

| frame | bytes | function | file:line |
|------:|------:|---|---|
| #7 | 112,736 | `Session::schedule_startup_prewarm` | `core/src/session_startup_prewarm.rs:185` |
| #9 | 283,056 | `Session::new` inner async block | `core/src/session/session.rs` |
| #10 | 156,576 | `Session::new` middle async block | `core/src/session/session.rs` |
| #11 | 255,488 | `Session::new` outer async block | `core/src/session/mod.rs:719` (call site) |
| #13 | 162,448 | `Session::spawn_internal` | `core/src/session/mod.rs:515` |
| #15 | 143,776 | `Session::spawn` | `core/src/session/mod.rs:491` |
| #17 | 112,448 | `ThreadManagerState::spawn_thread_with_source` | `core/src/thread_manager.rs:1540` |
| #19 | 43,328 | `ThreadManager::start_thread_inner` | `core/src/thread_manager.rs:778` |
| #21 | 49,008 | `ThreadManager::start_thread` | `core/src/thread_manager.rs:774` |
| #22 | 545,504 | the test's own `async` body | `core/src/agent/control/residency_tests.rs` |
| #44 | 124,176 | `Runtime::block_on` frame of the test fn | same |
| | **2,106,480** | **54 frames total** | |

**Mechanism, not recursion.** Every future `.await`ed without an intervening `Box::pin` is stored inline
in its parent's state machine, so the parent's *stack frame* grows by the whole child. The frames that
already `Box::pin` (#19, #21 — `thread_manager.rs:774` and `:778`) are the two smallest on the chain at
43–49 KB, against 112–283 KB for their unboxed neighbours; that contrast is the evidence for the
mechanism, and it is also the shape of the follow-up in §8.

**So the bound is the fix, and it is honest to say so.** An unbounded recursion would swallow any bound;
this one does not — 54 frames, fixed at compile time, and 2,128 KiB is sufficient with the chain as
written. Per the brief's step 2, the bound is made explicit and named rather than left in a
`RUST_MIN_STACK` someone has to remember.

---

## 4. RED FIRST — the abort, quoted

```
$ cd /tmp/stack-overflow && CARGO_TARGET_DIR=/tmp/stack-overflow-target cargo test -p codex-core --lib
```

```
running 2147 tests
test agent::control::tests::on_event_updates_status_from_task_started ... ok
test agent::control::tests::on_event_updates_status_from_shutdown_complete ... ok
test agent::control::tests::on_event_updates_status_from_error ... ok
test agent::control::tests::on_event_updates_status_from_task_complete ... ok
test agent::control::tests::on_event_updates_status_from_turn_aborted ... ok
test agent::control::execution::tests::execution_guards_count_active_v2_subagent_turns ... ok
test agent::control::execution::tests::execution_guards_ignore_root_and_v1_turns ... ok
test agent::control::tests::get_status_returns_not_found_without_manager ... ok
test agent::control::tests::register_session_root_skips_threads_with_explicit_parent ... ok
test agent::control::tests::resume_agent_errors_when_manager_dropped ... ok

thread 'agent::control::residency::tests::interrupted_v2_agent_is_lost_after_residency_eviction' (62089601) has overflowed its stack
fatal runtime error: stack overflow, aborting
error: test failed, to rerun pass `-p codex-core --lib`

Caused by:
  process didn't exit successfully: `/tmp/stack-overflow-target/debug/deps/codex_core-939b7dad284a5c0e` (signal: 6, SIGABRT: process abort signal)
```

**Ten green lines. No `test result:` line at all.** Nothing to grep for `failed`, no `failures:` block, and
`cargo`'s exit 101 is lost the moment anyone pipes this into `tee`, `head` or a log. That is the whole
invisibility: `FAILED=0` on a run that aborted reads exactly like a pass.

⚠️ **One trap found while reproducing, worth writing down:** the first isolation attempt piped the harness
into `tail`, and the shell reported `EXIT=0` for a run that had just SIGABRTed — the pipeline's exit code
is `tail`'s. The same mistake in a script is how this ships green. Every measurement below captures the
real status, never a pipeline's.

---

## 5. Test count: before and after

| | declared | accounted for | passed | failed |
|---|---:|---:|---:|---:|
| **Before, macOS defaults** | 2,147 | **0** | — | — |
| Before, `RUST_MIN_STACK=8388608` | 2,147 | 2,147 | 2,143 | 4 |
| **After, macOS defaults, no env** | **2,149** | **2,149** | **2,145** | **4** |

*Before* is honestly **"unknown — the run accounts for nothing"**, not "2,147 pass". The ten `ok` lines
are not a partial pass; libtest prints them as they complete and the abort invalidates the schedule.

2,149 = 2,147 + the two tests this lane adds
(`session_startup_stack_bound_is_load_bearing_and_sufficient`, `suite_abort_injector_for_guard_proof`).

**The 4 failures are pre-existing and none is mine** — identical set at my own base before and after:

- `config::schema::tests::config_schema_matches_fixture` (fixture drift; says to run `just write-config-schema`)
- `config::tests::to_mcp_config_preserves_apps_feature_from_config`
- `session::tests::fork_startup_context_then_first_turn_diff_snapshot` (writes a `.snap.new`)
- `session::turn::tests::post_sampling_token_estimate_is_disabled_by_always_on_sinks`

🔴 **Another lane reported 5 pre-existing failures. I measure 4 at `f6fd5cc1b`.** I did not inherit their
number; I ran the baseline myself. A 5th appeared once and only once, under a 16 MiB bound
(`guardian::tests::guardian_ephemeral_retry_preserves_parallel_trunk_and_fork_history`, `TimedOut` vs
`Approved`) — it passes 2/2 in isolation, so it is contention-induced, and it is one candidate for the
discrepancy. I am not claiming it is *the* explanation; the other lane's fifth is unnamed here.

---

## 6. The fix, in three layers, and exactly what each covers

| Layer | File | Covers | Does **not** cover |
|---|---|---|---|
| 1. Workspace bound | `.cargo/config.toml` `[env] RUST_MIN_STACK = "8388608"` | Every crate, every test thread **and every Tokio worker thread**, for anything cargo launches | A test binary run directly; a runner that ignores cargo config |
| 2. In-code bound | `core/src/test_stack_bound.rs` + the two residency tests | Those two tests however they are launched; and it is the only knob the §7 guard can turn | The other 34 tests, and worker threads |
| 3. Abort detection | `scripts/run-cargo-test-guarded.py` | **Any** suite that dies or under-reports, whatever the cause | Nothing it is not run on (see limits) |

Layer 1 is the one that matters, and the reason it is in `.cargo/config.toml` rather than in CI is the
brief's own standard: a guard reachable only from the path you remembered is a guard on that path. This
file already owns the Windows main-thread stack bound, so the bound now has one home.

**8 MiB, not 16 MiB, and that is measured.** 16 MiB is production's number
(`TOKIO_WORKER_STACK_SIZE_BYTES`, `arg0/src/lib.rs:25`) and it is **not free**: on this machine it took the
suite from 49.6s to 105.6s and timed out a websocket test that passes in isolation. 8 MiB is what
`release.yml:88` and nine of the ten ad-hoc `TEST_STACK_SIZE_BYTES` declarations already use, leaves ~4x
headroom over the measured 2,128 KiB, and ran in 43.97s. Stack past the measurement is a cost, not a margin.

**One owner.** `TEST_STACK_SIZE_BYTES` was declared privately in **ten** places with **two different
values** (4 MiB at `core/src/guardian/tests.rs:2642`, 8 MiB at nine others) and **not at all** at the site
that aborted. `core/src/guardian/tests.rs` now takes the shared constant.

---

## 7. Can-fire proofs — every guard driven red

### 7.1 The stack bound (`session_startup_stack_bound_is_load_bearing_and_sufficient`)

It fails in **both** directions and neither half asserts on an exit status alone (a process can die by
signal for unrelated reasons, and can exit 0 having run nothing) — both halves assert on the harness's
own words.

| mutant | change | result |
|---|---|---|
| 1 | `TEST_STACK_SIZE_BYTES` → 2 MiB | **KILLED** — `does not fit in TEST_STACK_SIZE_BYTES (2097152 bytes)` |
| 2 | `LIBTEST_DEFAULT_THREAD_STACK_BYTES` → 16 MiB (simulates the chain shrinking) | **KILLED** — `the bound has stopped being load-bearing: … survived libtest's default 16777216-byte thread stack (exit Some(0)). Re-measure the chain and delete TEST_STACK_SIZE_BYTES if it is no longer needed` |
| 3 | bound removed: canary back to `#[tokio::test]` | **KILLED** — `does not fit in TEST_STACK_SIZE_BYTES (16777216 bytes)` |

3 mutants applied, 3 killed. Mutant 2 is the one that matters most: it is what stops this bound rotting
into decoration if someone later shrinks the chain.

### 7.2 The abort guard — proved on a fake harness **and** on the real one

`python3 scripts/test-run-cargo-test-guarded.py` — 18 clauses, all green. Its abort fixture is the
**literal captured bytes** from §4, not a paraphrase, because a double friendlier than production
certifies code production rejects. Cases: real SIGABRT → refused as SIGNAL; complete green → passes;
ordinary 4-failure red → refused as RED and *not* mistaken for an abort; **exit 0, zero failures, a
well-formed `test result: ok.` line, 10 of 2,147 accounted → refused** with `2137 test(s) never reported
an outcome`; silent run → refused. Then all three end-to-end through a fake `cargo` on `PATH`.

**Live proof against the real harness**, using the permanent opt-in injector
`suite_abort_injector_for_guard_proof` (inert unless `CODEX_SUITE_ABORT_INJECTOR` is set), so this claim
stays re-checkable rather than being a one-off:

```
$ CODEX_SUITE_ABORT_INJECTOR=1 python3 scripts/run-cargo-test-guarded.py -p codex-core --lib
  process didn't exit successfully: `…/codex_core-939b7dad284a5c0e` (signal: 6, SIGABRT: process abort signal)
guard: declared 2149 | accounted 0 | passed 0 | failed 0 | ignored 0 | filtered 0 | cargo exit 101
guard: FAIL — SIGNAL: the harness process died rather than finishing (SIGABRT, signal: ). Everything it
       had not reached is UNMEASURED — the green lines above it are not a partial pass.
guard: FAIL — SUMMARY: the harness declared 2149 tests and printed no `test result:` line. Nothing here
       is evidence of anything.
$ echo $?
1
```

And the guard caught a real regression during this lane: after fixing only the two residency tests, it
went red on `agent::control::tests::list_agent_subtree_thread_ids_finds_live_descendants_of_unloaded_root`
— the 35th over-budget test, which had been invisible behind the first abort for as long as it has existed.
**The guard found the thing the fix was still missing.** That is the whole argument for it.

### Green, after

```
guard: declared 2149 | accounted 2149 | passed 2145 | failed 4 | ignored 0 | filtered 0 | cargo exit 101
guard: FAIL — RED: 4 test(s) failed. This is an ordinary red run, distinct from an aborted one.
```

Exit 1 on the four pre-existing failures, which is correct: the guard's job is to make the run
*legible*, and this run is legibly red for a reason that has nothing to do with a stack.

---

## 8. The five states

| State | Verdict | Evidence |
|---|---|---|
| **built** | ✅ | `.cargo/config.toml`, `core/src/test_stack_bound.rs` (+ tests), two residency tests converted, `core/src/guardian/tests.rs` on the shared owner, `scripts/run-cargo-test-guarded.py` (+ its proof). |
| **wired** | ✅ | `#[cfg(test)] pub(crate) mod test_stack_bound;` at `core/src/lib.rs:102`; `[env]` reaches the harness — proven by the whole suite completing at defaults with `RUST_MIN_STACK` unset in the environment. |
| **tested** | ✅ | 2,149 declared / 2,149 accounted / 2,145 passed. 3 stack-bound mutants killed; 18 abort-guard clauses green; live injector proof. `cargo fmt` clean; `cargo clippy -p codex-core --lib --all-targets` clean (one `#[expect(clippy::print_stderr, reason = …)]`, reason written). |
| **SHIPPED-IN-PREVIEW** | ⛔ | Branch only. Not merged, not released, no binary built or published. |
| **PROBED** | ⛔ **N/A and deliberately so** | Test-harness and cargo-config change. No server, no endpoint, no deploy. There is nothing on a wire to read back — saying otherwise would be the fabrication this repo bans. The remote read-back that *does* apply is in §10. |

---

## 9. Limits — stated out loud

1. **One platform, one toolchain.** Every number is aarch64-apple-darwin / rustc 1.95.0 / `test` profile.
   Frame sizes are codegen-dependent; x86-64 and Linux will differ, and a compiler bump can move them.
   The load-bearing guard is what re-measures this on every run, on whatever machine runs it.
2. **The 34-test census is a floor, not a total.** The sweep halts at the first `tokio-rt-worker` overflow,
   which cannot be skipped by name. The remaining error points **up**.
3. **`[env]` in `.cargo/config.toml` binds what cargo launches.** A directly-executed test binary, or a
   runner that does not read cargo config, does not get it. I could **not** verify `cargo-nextest`
   (not installed on this machine) — and nextest is what `.config/nextest.toml` says CI uses. **Unverified:
   whether nextest honours `[env]`.** Nextest also runs one process per test, so an overflow there fails
   one test rather than the suite — which is exactly why CI never saw this and a developer did.
4. **The abort guard is not wired into CI.** It is a script with a passing self-proof; no workflow calls
   it yet. Layer 3 covers only the invocations someone points it at. Wiring it into `release.yml` is the
   obvious next step and is not done here.
5. **Two of 36+ tests carry the in-code bound.** The other 34 rely on layer 1. If someone deletes the
   `[env]` block, those 34 abort again and only the abort guard will say so.
6. **Integration tests still declare their own constant.** `core/tests/suite/rmcp_client.rs:3097` keeps a
   private 8 MiB (it is a separate crate and `test_stack_bound` is `#[cfg(test)]`, so it cannot see it).
   It happens to agree with the new owner today. Same for the eight `estelle-tui` / `codex-app-server`
   sites — untouched to avoid colliding with two live lanes.
7. **The production chain is not fixed, only measured.** Session startup costs 2.01 MiB of stack. Production
   is safe because `arg0/src/lib.rs:25` names 16 MiB for the `codex-main` thread and every Tokio worker —
   but a Tokio default worker is 2 MiB, so any future entry point that builds a runtime without
   `thread_stack_size` inherits this cliff. **The durable fix is to shrink the chain**: `Box::pin` at the
   `Session::new` and `schedule_startup_prewarm` await sites, where 695 KB and 113 KB sit unboxed, against
   43–49 KB for the two frames that already box. That is a change to hot shared production code and it is
   deliberately **not** in this lane.
8. **The `.snap.new` written by the pre-existing snapshot failure is deleted, not committed.** It is an
   artifact of a red test I did not cause.

---

## 10. Reproduce

```bash
git clone <repo> && git checkout coach/stack-overflow-20260901

# RED, on the parent commit:
git stash && env -u RUST_MIN_STACK cargo test -p codex-core --lib   # SIGABRT at test 11

# GREEN, with the fix, no environment help:
env -u RUST_MIN_STACK python3 scripts/run-cargo-test-guarded.py -p codex-core --lib

# The bound is load-bearing (both directions):
cargo test -p codex-core --lib -- --exact \
  test_stack_bound::tests::session_startup_stack_bound_is_load_bearing_and_sufficient

# The abort guard can fire, on the real harness:
CODEX_SUITE_ABORT_INJECTOR=1 python3 scripts/run-cargo-test-guarded.py -p codex-core --lib

# The abort guard can fire, on captured production bytes:
python3 scripts/test-run-cargo-test-guarded.py
```

---

## 11. The one line

🔴 **Yes — every `codex-core --lib` number reported tonight without an explicit `RUST_MIN_STACK` is
invalid, and the ones reported *with* it are incomplete.** A run at macOS defaults accounted for **zero**
of 2,147 tests, so any "N pass / 0 fail" from such a run is a count of the ten lines before the abort. The
measurement to redo is **the full `codex-core --lib` pass/fail count**, on this branch, through
`scripts/run-cargo-test-guarded.py`, quoting *declared* and *accounted for* and not just *passed*. The
correct figure at `f6fd5cc1b` + this lane is **2,149 declared / 2,149 accounted / 2,145 passed / 4 failed**,
and those 4 are pre-existing.
