#!/usr/bin/env python3
"""🔴 AN ABORTED TEST RUN MUST NOT BE READABLE AS A PASSING ONE.

`cargo test -p codex-core --lib` printed ten `... ok` lines and then died:

    test agent::control::tests::resume_agent_errors_when_manager_dropped ... ok

    thread '...interrupted_v2_agent_is_lost_after_residency_eviction' has overflowed its stack
    fatal runtime error: stack overflow, aborting

There is **no `test result:` line at all** in that output. So every habit we have
for reading a Rust suite fails on it at once: there is no `N failed` to grep, the
`failures:` block never prints, and anything that pipes the run into `tee`, `head`
or a log loses cargo's exit code to the pipeline. `FAILED=0` on a run that aborted
reads exactly like a pass — and a reader skimming ten green lines and a truncated
tail has no reason to look twice.

This wrapper refuses that reading. It asks the harness how many tests it *declares*
before running anything, then requires the run to account for every one of them.

    scripts/run-cargo-test-guarded.py -p codex-core --lib

Four independent clauses, each of which fails the run on its own:

  1. SIGNAL   — the harness died on a signal, or said so in its output.
  2. SUMMARY  — the harness declared tests and printed no `test result:` line.
  3. COUNT    — reported tests != declared tests. This is the clause that catches
                an abort that somehow still printed a summary, and it is a shape
                assertion on parsed values rather than a non-emptiness check.
  4. RED      — an ordinary test failure. Reported separately, because "four tests
                failed" and "the process died at test eleven of 2,147" are
                different facts and only one of them invalidates the other 2,136.

Exit 0 = every clause holds. Exit 1 = a named clause failed, with both numbers.

Prove it can fire, against the real harness rather than a fake one:

    CODEX_SUITE_ABORT_INJECTOR=1 scripts/run-cargo-test-guarded.py -p codex-core --lib
"""

from __future__ import annotations

import re
import subprocess
import sys
from dataclasses import dataclass
from dataclasses import field

# A libtest `--list --format=terse` line looks like `path::to::test_name: test`.
LISTED_TEST_RE = re.compile(r"^\S.*: (?:test|bench)$", re.MULTILINE)

# `test result: ok. 2143 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; ...`
SUMMARY_RE = re.compile(
    r"^test result: (?P<verdict>\w+)\. "
    r"(?P<passed>\d+) passed; "
    r"(?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored; "
    r"(?P<measured>\d+) measured; "
    r"(?P<filtered>\d+) filtered out",
    re.MULTILINE,
)

# Phrases that mean the process died rather than finished. `cargo` reports the
# signal on the child's behalf and keeps its own exit code at 101, so the
# returncode alone is not enough to tell an abort from four red tests.
DEATH_PHRASES = (
    "has overflowed its stack",
    "fatal runtime error",
    "SIGABRT",
    "SIGSEGV",
    "SIGBUS",
    "SIGILL",
    "signal: ",
)


@dataclass
class Verdict:
    declared: int
    reported: int
    passed: int = 0
    failed: int = 0
    ignored: int = 0
    measured: int = 0
    filtered: int = 0
    summaries: int = 0
    problems: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.problems


def _run(argv: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        argv,
        capture_output=True,
        text=True,
        check=False,
    )


def count_declared_tests(cargo: str, cargo_args: list[str]) -> tuple[int, str]:
    """Ask the harness how many tests exist, before running any of them."""
    listing = _run([cargo, "test", *cargo_args, "--", "--list", "--format=terse"])
    combined = listing.stdout + listing.stderr
    if listing.returncode != 0:
        raise SystemExit(
            f"GUARD ERROR: could not enumerate tests (`{cargo} test ... --list` exited "
            f"{listing.returncode}).\n{combined}"
        )
    return len(LISTED_TEST_RE.findall(listing.stdout)), combined


def judge(declared: int, returncode: int, output: str) -> Verdict:
    summaries = list(SUMMARY_RE.finditer(output))
    verdict = Verdict(declared=declared, reported=0, summaries=len(summaries))
    for match in summaries:
        verdict.passed += int(match.group("passed"))
        verdict.failed += int(match.group("failed"))
        verdict.ignored += int(match.group("ignored"))
        verdict.measured += int(match.group("measured"))
        verdict.filtered += int(match.group("filtered"))
    verdict.reported = (
        verdict.passed
        + verdict.failed
        + verdict.ignored
        + verdict.measured
        + verdict.filtered
    )

    # 1. SIGNAL.
    died = [phrase for phrase in DEATH_PHRASES if phrase in output]
    if returncode < 0:
        died.append(f"wrapper saw signal {-returncode}")
    if died:
        verdict.problems.append(
            "SIGNAL: the harness process died rather than finishing "
            f"({', '.join(sorted(set(died)))}). Everything it had not reached is "
            "UNMEASURED — the green lines above it are not a partial pass."
        )

    # 2. SUMMARY.
    if declared > 0 and not summaries:
        verdict.problems.append(
            f"SUMMARY: the harness declared {declared} tests and printed no "
            "`test result:` line. Nothing here is evidence of anything."
        )

    # 3. COUNT.
    if summaries and verdict.reported != declared:
        verdict.problems.append(
            f"COUNT: the harness declared {declared} tests and accounted for "
            f"{verdict.reported} "
            f"(passed {verdict.passed}, failed {verdict.failed}, "
            f"ignored {verdict.ignored}, measured {verdict.measured}, "
            f"filtered {verdict.filtered}). "
            f"{declared - verdict.reported} test(s) never reported an outcome."
        )

    # 4. RED.
    if verdict.failed:
        verdict.problems.append(
            f"RED: {verdict.failed} test(s) failed. This is an ordinary red run, "
            "distinct from an aborted one."
        )

    return verdict


def main(argv: list[str]) -> int:
    cargo_args = argv[1:]
    if not cargo_args:
        print(__doc__)
        return 2
    cargo = "cargo"

    declared, _ = count_declared_tests(cargo, cargo_args)
    run = _run([cargo, "test", *cargo_args])
    output = run.stdout + run.stderr
    sys.stdout.write(run.stdout)
    sys.stderr.write(run.stderr)

    verdict = judge(declared, run.returncode, output)

    print()
    print(f"guard: declared {verdict.declared} | accounted {verdict.reported} | "
          f"passed {verdict.passed} | failed {verdict.failed} | "
          f"ignored {verdict.ignored} | filtered {verdict.filtered} | "
          f"cargo exit {run.returncode}")
    if verdict.ok:
        print("guard: PASS — every declared test reported an outcome.")
        return 0
    for problem in verdict.problems:
        print(f"guard: FAIL — {problem}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
