#!/usr/bin/env python3
"""Contract tests for the installed-binary plain-English routing measure."""

from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "plain-english-routing-receipt.py"


def load_measure():
    spec = importlib.util.spec_from_file_location("plain_english_routing_receipt", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_ten_requests_cover_every_named_suite() -> None:
    measure = load_measure()
    assert len(measure.CASES) == 10
    assert all(
        not case["prompt"].endswith((".", "?", "!")) for case in measure.CASES
    ), "the TUI normalizes terminal punctuation before rendering its echo"
    assert {case["expected_suite"] for case in measure.CASES} == {
        "research",
        "review",
        "affinity",
        "monitor",
        "guardian",
        "memory",
    }


def test_route_measure_names_failures_instead_of_crediting_deep_search() -> None:
    measure = load_measure()
    rows = measure.score_cases(
        measure.CASES,
        [
            {
                "prompt": case["prompt"],
                "paths": ["/deep-search"],
                "statuses": [200],
            }
            for case in measure.CASES
        ],
    )
    passed = [row for row in rows if row["pass"]]
    failed = [row for row in rows if not row["pass"]]
    assert len(passed) == 2
    assert {row["expected_suite"] for row in passed} == {"research"}
    assert len(failed) == 8
    assert all(row["observed_suite"] == "research" for row in failed)
    assert all(row["prompt"] for row in failed)


def test_instrument_controls_require_one_known_route_and_zero_unknown_routes() -> None:
    measure = load_measure()
    assert measure.controls_pass(
        {"paths": ["/me"], "statuses": [200]},
        {"paths": [], "statuses": []},
    ) is True
    assert measure.controls_pass(
        {"paths": [], "statuses": []},
        {"paths": [], "statuses": []},
    ) is False


def test_one_misrouted_turn_makes_the_complete_receipt_red() -> None:
    measure = load_measure()
    rows = [{"pass": True} for _ in measure.CASES]
    observations = [{"tui_completed": True} for _ in measure.CASES]
    assert measure.routing_receipt_pass(True, rows, observations, discarded=0) is True
    rows[4]["pass"] = False
    assert measure.routing_receipt_pass(True, rows, observations, discarded=0) is False
    assert measure.controls_pass(
        {"paths": ["/me"], "statuses": [200]},
        {"paths": ["/deep-search"], "statuses": [200]},
    ) is False


def main() -> int:
    test_ten_requests_cover_every_named_suite()
    test_route_measure_names_failures_instead_of_crediting_deep_search()
    test_instrument_controls_require_one_known_route_and_zero_unknown_routes()
    test_one_misrouted_turn_makes_the_complete_receipt_red()
    print("plain-English routing receipt tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
