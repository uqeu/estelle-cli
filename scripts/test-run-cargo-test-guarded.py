#!/usr/bin/env python3
"""🔴 PROVE THE ABORT GUARD CAN GO RED BEFORE TRUSTING IT GREEN.

`scripts/run-cargo-test-guarded.py` exists to make an aborted suite unreadable as a
passing one. A guard written for that job is worthless until someone has watched it
fail, so this drives it through four shapes and requires the right verdict on each.

The abort fixture below is **not invented**: it is the literal output of
`cargo test -p codex-core --lib` at f6fd5cc1b on aarch64-apple-darwin on 2026-09-01,
including cargo's own `(signal: 6, SIGABRT: process abort signal)` line. A double
that is friendlier than production certifies code production rejects, so the bytes
are copied rather than paraphrased.

The fourth case is the one worth staring at: **exit 0, zero failures, a perfectly
well-formed `test result: ok.` line — and still red**, because the harness declared
2,147 tests and accounted for 10. That is the shape an abort leaves behind once
anything downstream re-summarises it, and no `failed`-counting check can see it.

Both layers run: `judge()` on captured bytes, and the whole script end-to-end
against a fake `cargo` on PATH, so a wiring mistake between the two cannot hide.

Exit 0 = every case produced the expected verdict.
"""

from __future__ import annotations

import importlib.util
import os
import pathlib
import stat
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
GUARD = ROOT / "scripts" / "run-cargo-test-guarded.py"

spec = importlib.util.spec_from_file_location("run_cargo_test_guarded", GUARD)
assert spec and spec.loader, f"cannot load {GUARD}"
guard = importlib.util.module_from_spec(spec)
# `@dataclass` resolves annotations through `sys.modules[cls.__module__]`, so the
# module has to be registered before it is executed, not after.
sys.modules[spec.name] = guard
spec.loader.exec_module(guard)

failures: list[str] = []


def check(label: str, condition: bool, detail: str = "") -> None:
    if condition:
        print(f"  ok   {label}")
        return
    failures.append(f"{label}{(' — ' + detail) if detail else ''}")
    print(f"  FAIL {label}{(' — ' + detail) if detail else ''}")


# ── The captured production abort, byte for byte ─────────────────────────────
REAL_ABORT_OUTPUT = """\
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

thread 'agent::control::residency::tests::interrupted_v2_agent_is_lost_after_residency_eviction' \
(62089601) has overflowed its stack
fatal runtime error: stack overflow, aborting
error: test failed, to rerun pass `-p codex-core --lib`

Caused by:
  process didn't exit successfully: `/tmp/target/debug/deps/codex_core-939b7dad284a5c0e` \
(signal: 6, SIGABRT: process abort signal)
"""

REAL_GREEN_OUTPUT = """\
running 2147 tests
test agent::control::tests::on_event_updates_status_from_error ... ok

test result: ok. 2147 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.56s
"""

REAL_RED_OUTPUT = """\
running 2147 tests

failures:
    config::schema::tests::config_schema_matches_fixture

test result: FAILED. 2143 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.56s
"""

# Exit 0, a well-formed summary, zero failures — and 2,137 tests never reported.
TRUNCATED_BUT_TIDY_OUTPUT = """\
running 2147 tests

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s
"""

SILENT_OUTPUT = "running 2147 tests\n"


print("judge() against captured harness output")

v = guard.judge(declared=2147, returncode=101, output=REAL_ABORT_OUTPUT)
check("the real SIGABRT run is refused", not v.ok)
check(
    "and it is refused for dying, not merely for being red",
    any(p.startswith("SIGNAL:") for p in v.problems),
    str(v.problems),
)
check(
    "the abort is also caught by the count clause on its own",
    any(p.startswith("SUMMARY:") or p.startswith("COUNT:") for p in v.problems),
    str(v.problems),
)

v = guard.judge(declared=2147, returncode=0, output=REAL_GREEN_OUTPUT)
check("a complete green run passes", v.ok, str(v.problems))
check("and it accounts for every declared test", v.reported == 2147, str(v.reported))

v = guard.judge(declared=2147, returncode=101, output=REAL_RED_OUTPUT)
check("an ordinary red run is refused", not v.ok)
check(
    "and it is named RED, not an abort",
    [p.split(":")[0] for p in v.problems] == ["RED"],
    str(v.problems),
)

v = guard.judge(declared=2147, returncode=0, output=TRUNCATED_BUT_TIDY_OUTPUT)
check("exit 0 with zero failures is still refused when tests are missing", not v.ok)
check(
    "and the reason names the arithmetic",
    any("2137 test(s) never reported an outcome" in p for p in v.problems),
    str(v.problems),
)

v = guard.judge(declared=2147, returncode=0, output=SILENT_OUTPUT)
check("a run that printed no summary at all is refused", not v.ok)
check(
    "and the reason is the missing summary",
    any(p.startswith("SUMMARY:") for p in v.problems),
    str(v.problems),
)

v = guard.judge(declared=0, returncode=0, output="")
check("an empty target with nothing declared is not invented into a failure", v.ok, str(v.problems))


# ── End to end, through a fake `cargo` on PATH ───────────────────────────────
FAKE_CARGO = r'''#!/usr/bin/env python3
import os, sys
scenario = os.environ["FAKE_CARGO_SCENARIO"]
listing = "a::t1: test\na::t2: test\na::t3: test\n"
if "--list" in sys.argv:
    sys.stdout.write(listing)
    raise SystemExit(0)
if scenario == "green":
    sys.stdout.write("running 3 tests\ntest a::t1 ... ok\ntest a::t2 ... ok\ntest a::t3 ... ok\n\n"
                     "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; "
                     "finished in 0.01s\n")
    raise SystemExit(0)
if scenario == "abort":
    sys.stdout.write("running 3 tests\ntest a::t1 ... ok\n")
    sys.stderr.write("fatal runtime error: stack overflow, aborting\n"
                     "Caused by:\n  process didn't exit successfully: (signal: 6, SIGABRT: "
                     "process abort signal)\n")
    raise SystemExit(101)
if scenario == "short":
    sys.stdout.write("running 3 tests\ntest a::t1 ... ok\n\n"
                     "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; "
                     "finished in 0.01s\n")
    raise SystemExit(0)
raise SystemExit(3)
'''

print("end to end, through a fake cargo on PATH")
with tempfile.TemporaryDirectory() as tmp:
    fake = pathlib.Path(tmp) / "cargo"
    fake.write_text(FAKE_CARGO)
    fake.chmod(fake.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
    for scenario, expected_exit, must_say in (
        ("green", 0, "guard: PASS"),
        ("abort", 1, "SIGNAL:"),
        ("short", 1, "never reported an outcome"),
    ):
        env = dict(os.environ)
        env["PATH"] = f"{tmp}{os.pathsep}{env['PATH']}"
        env["FAKE_CARGO_SCENARIO"] = scenario
        proc = subprocess.run(
            [sys.executable, str(GUARD), "-p", "fake", "--lib"],
            capture_output=True, text=True, env=env, check=False,
        )
        blob = proc.stdout + proc.stderr
        check(
            f"scenario {scenario!r} exits {expected_exit}",
            proc.returncode == expected_exit,
            f"got {proc.returncode}: {blob[-400:]}",
        )
        check(f"scenario {scenario!r} says {must_say!r}", must_say in blob, blob[-400:])


if failures:
    print(f"\n🔴 ABORT-GUARD PROOF FAILED — {len(failures)} clause(s):", file=sys.stderr)
    for f in failures:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)

print("\n✅ the abort guard goes red on a dying suite, on a truncated-but-tidy one, "
      "and on a silent one — and green only on a run that accounted for every declared test")
