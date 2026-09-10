#!/usr/bin/env python3
"""RUST GUARD MUTANTS for the SubagentStart/SubagentStop door.

`scripts/guard_mutants.py` in uqeu/estelle is a PYTEST runner rooted at that repo; the code under
test here is Rust in uqeu/estelle-cli, so it cannot drive it. This applies the same six rules
rather than borrowing its green:

  1. baseline green, else the whole run is void
  2. the pattern is PRESENT and UNIQUE (0 matches mutates nothing; 2 mutates something unnamed)
  3. the state is ACHIEVED - new text present AND old text gone
  4. a kill is a NAMED FAILING TEST, never a non-zero exit
  5. the EXPECTED test is the one that died (a mutant that kills something else proves the suite
     is sensitive, not that the guard exists)
  6. every target is restored in a `finally`, so a crash cannot leave the tree mutated

Run it: `python3 scripts/subagent-door-mutants.py`. Exit 0 only when every mutant is KILLED by
the test it NAMES. Last measured 2026-09-10 on v0.3.4: 7 killed / 0 survived / 0 void / 0 wrong.

WARNING: this MUTATES tracked source and restores it. Do not run it with uncommitted work in
`tui/src/top_level.rs` or `tui/src/subagent_context.rs` that you have not saved elsewhere.
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass

#: The repository root, resolved from THIS file. A hard-coded path is a guard that runs on one
#: machine, which is the same defect as no guard at all.
ROOT = pathlib.Path(__file__).resolve().parents[1]
TOP = "tui/src/top_level.rs"
SUB = "tui/src/subagent_context.rs"


@dataclass(frozen=True)
class Mutant:
    name: str
    path: str
    old: str
    new: str
    expect_dead: tuple[str, ...]
    expect_alive: tuple[str, ...] = ()


MUTANTS: list[Mutant] = [
    Mutant(
        "delete-the-subagent-start-branch",
        TOP,
        'if payload.hook_event_name.trim() == crate::subagent_context::SUBAGENT_START_EVENT {',
        'if false {',
        ("top_level::tests::subagent_start_is_answered_through_the_shipped_dispatch",
         "top_level::tests::a_subagent_inherits_the_recall_the_parent_paid_for"),
    ),
    Mutant(
        "never-store-what-the-parent-paid-for",
        TOP,
        "let _stored =\n            crate::subagent_context::store_grounding(parent_session, recall, repo.as_str());",
        "let _stored = recall.is_empty() && parent_session.is_empty();",
        ("top_level::tests::a_subagent_inherits_the_recall_the_parent_paid_for",),
        # The miss path must still answer: this mutant removes inheritance, not the door.
        ("top_level::tests::subagent_start_is_answered_through_the_shipped_dispatch",),
    ),
    Mutant(
        "a-miss-becomes-silence",
        SUB,
        "        return Some(NO_GROUNDING_NOTICE.to_string());",
        "        return None;",
        ("subagent_context::tests::a_miss_is_said_out_loud_and_is_never_silence",
         "top_level::tests::subagent_start_is_answered_through_the_shipped_dispatch"),
    ),
    Mutant(
        "envelope-names-the-wrong-event",
        TOP,
        "                        crate::subagent_context::SUBAGENT_START_EVENT,\n                    )]",
        '                        "UserPromptSubmit",\n                    )]',
        ("top_level::tests::subagent_start_is_answered_through_the_shipped_dispatch",),
    ),
    Mutant(
        "session-id-is-repaired-instead-of-refused",
        SUB,
        "fn safe_session_id(session_id: &str) -> Option<&str> {\n    let candidate = session_id.trim();",
        "fn safe_session_id(session_id: &str) -> Option<&str> {\n    return Some(session_id);\n    #[allow(unreachable_code)]\n    let candidate = session_id.trim();",
        ("subagent_context::tests::a_traversal_session_id_is_refused_and_never_repaired",),
    ),
    Mutant(
        "the-cache-at-rest-is-not-redacted",
        SUB,
        "    let redacted = estelle_client::redact_secrets_engine(context.trim());",
        "    let redacted = context.trim().to_string();",
        ("subagent_context::tests::a_credential_in_the_block_does_not_reach_the_file_at_rest",),
    ),
    Mutant(
        "the-subagent-start-row-stops-shipping",
        TOP,
        'event: "SubagentStart",\n        matcher: None,\n        mode: "context",\n        timeout: CONTEXT_HOOK_HOST_BUDGET_S,\n        claude_async: false,\n        plugin: true,',
        'event: "SubagentStart",\n        matcher: None,\n        mode: "context",\n        timeout: CONTEXT_HOOK_HOST_BUDGET_S,\n        claude_async: false,\n        plugin: false,',
        ("top_level::tests::the_plugin_manifest_is_generated_from_the_one_hook_table",),
    ),
]


def run_tests() -> tuple[int, set[str]]:
    """Return (exit code, the set of NAMED failing tests). A compile error yields an empty set,
    which can never be read as a kill - rule 4."""
    done = subprocess.run(
        ["cargo", "test", "-p", "estelle-tui", "--bin", "estelle", "--", "--color=never"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=1800,
        check=False,
        env={**__import__("os").environ, "RUST_MIN_STACK": "8388608", "CARGO_TERM_COLOR": "never"},
    )
    failed = set(re.findall(r"^test (\S+) \.\.\. FAILED$", done.stdout, re.M))
    compiled = "error[E" not in done.stderr and "could not compile" not in done.stderr
    if not compiled:
        return (done.returncode, set())
    return (done.returncode, failed)


def main() -> int:
    code, failed = run_tests()
    if code != 0 or failed:
        print(f"VOID: baseline is not green (exit {code}, failing {sorted(failed)})")
        return 1
    print("baseline: green")

    killed, survived, void, wrong = [], [], [], []
    for mutant in MUTANTS:
        path = ROOT / mutant.path
        original = path.read_text()
        try:
            hits = original.count(mutant.old)
            if hits != 1:
                void.append((mutant.name, f"pattern matched {hits} times, needs exactly 1"))
                print(f"VOID  {mutant.name}: pattern matched {hits} times")
                continue
            mutated = original.replace(mutant.old, mutant.new, 1)
            if mutant.new not in mutated or mutant.old in mutated:
                void.append((mutant.name, "state not achieved"))
                print(f"VOID  {mutant.name}: state not achieved")
                continue
            path.write_text(mutated)
            _, dead = run_tests()
            missing = [t for t in mutant.expect_dead if t not in dead]
            resurrected = [t for t in mutant.expect_alive if t in dead]
            if not dead:
                survived.append(mutant.name)
                print(f"SURVIVED {mutant.name}: no test failed")
            elif missing:
                wrong.append((mutant.name, sorted(dead)))
                print(f"WRONG TEST {mutant.name}: expected {missing} dead, got {sorted(dead)}")
            elif resurrected:
                wrong.append((mutant.name, sorted(dead)))
                print(f"WRONG TEST {mutant.name}: {resurrected} died and must not have")

            else:
                killed.append((mutant.name, sorted(dead)))
                print(f"KILLED {mutant.name}: {len(dead)} red, incl. {mutant.expect_dead[0]}")
        finally:
            path.write_text(original)
            assert path.read_text() == original, "restore failed"

    print()
    print(f"{len(killed)} killed / {len(survived)} survived / {len(void)} void / {len(wrong)} wrong test")
    for name, dead in killed:
        print(f"  killed  {name}: {len(dead)} failing -> {dead[:4]}")
    for name in survived:
        print(f"  SURVIVED {name}")
    for name, detail in void:
        print(f"  void    {name}: {detail}")
    for name, dead in wrong:
        print(f"  WRONG   {name}: {dead}")
    return 0 if not (survived or void or wrong) else 1


if __name__ == "__main__":
    sys.exit(main())
