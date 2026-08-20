#!/usr/bin/env python3
"""Measure which suite ten plain-English turns reach through a public binary.

The score is deliberately about the observed HTTP route, not rendered prose.  A
server answer that merely talks about review is not evidence that Review ran.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HARNESS_PATH = ROOT / "scripts" / "public-binary-receipts.py"

CASES = (
    {"expected_suite": "research", "prompt": "Find the application entry point and cite the file."},
    {"expected_suite": "research", "prompt": "Research how authentication is implemented in this repository."},
    {"expected_suite": "review", "prompt": "Review my current changes for merge blockers."},
    {"expected_suite": "review", "prompt": "Check this diff for security and correctness regressions."},
    {"expected_suite": "affinity", "prompt": "Which model should plan this implementation task?"},
    {"expected_suite": "monitor", "prompt": "Show production errors from the last hour."},
    {"expected_suite": "monitor", "prompt": "Is production up right now?"},
    {"expected_suite": "guardian", "prompt": "Check whether this patch is grounded in the repository."},
    {"expected_suite": "memory", "prompt": "What has this repository taught us about authentication?"},
    {"expected_suite": "memory", "prompt": "Show the memories saved for this repository."},
)

SUITE_ROUTES = {
    "research": frozenset({"/deep-search"}),
    "review": frozenset({"/gate"}),
    "affinity": frozenset({"/route"}),
    "monitor": frozenset({"/monitor/issues", "/monitor/logs", "/monitor/alerts", "/monitor/uptime"}),
    "guardian": frozenset({"/verify"}),
    "memory": frozenset({"/memories", "/search", "/memory/cards"}),
}


def load_harness():
    spec = importlib.util.spec_from_file_location("public_binary_receipts", HARNESS_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def observed_suite(paths: list[str]) -> str | None:
    matches = {
        suite
        for suite, accepted in SUITE_ROUTES.items()
        if any(path in accepted for path in paths)
    }
    if len(matches) == 1:
        return next(iter(matches))
    if len(matches) > 1:
        return "multiple"
    return None


def score_cases(cases, observations):
    rows = []
    for case, observation in zip(cases, observations, strict=True):
        suite = observed_suite(observation["paths"])
        rows.append(
            {
                "prompt": case["prompt"],
                "expected_suite": case["expected_suite"],
                "observed_suite": suite,
                "observed_paths": observation["paths"],
                "statuses": observation["statuses"],
                "pass": suite == case["expected_suite"],
                "discarded": observation.get("discarded", False),
                "production_build": observation.get("production_build"),
            }
        )
    return rows


def controls_pass(positive: dict, negative: dict) -> bool:
    return (
        positive.get("paths") == ["/me"]
        and positive.get("statuses") == [200]
        and negative.get("paths") == []
        and negative.get("statuses") == []
    )


def _active_records(harness, trace: Path, offset: int) -> list[dict]:
    records = harness._read_http_records_after(trace, offset)
    return [
        record
        for record in records
        if record.get("request", {}).get("path") not in harness.PASSIVE_TUI_ROUTES
    ]


def run_turn(harness, prompt: str, repo: str, trace: Path, timeout: float) -> dict:
    offset = harness._trace_line_count(trace) or 0
    tui = harness.tui_turn_receipt(prompt, repo, timeout)
    records = _active_records(harness, trace, offset)
    paths = [record.get("request", {}).get("path") for record in records]
    statuses = [record.get("response", {}).get("status") for record in records]
    return {
        "prompt": prompt,
        "paths": [path for path in paths if isinstance(path, str)],
        "statuses": [status for status in statuses if isinstance(status, int)],
        "tui_completed": tui.get("pass") is True,
        "pass": tui.get("pass") is True,
    }


def run_measure(repo: str, trace: Path, timeout: float, health_url: str) -> dict:
    harness = load_harness()
    read_identity = lambda: harness.read_production_identity(health_url)

    def pinned(prompt: str) -> dict:
        return harness.pin_surface_build(
            lambda: run_turn(harness, prompt, repo, trace, timeout),
            read_identity,
        )

    positive = pinned("/me")
    negative = pinned("/receipt-route-that-does-not-exist")
    observations = [pinned(case["prompt"]) for case in CASES]
    rows = score_cases(CASES, observations)
    routed = sum(row["pass"] and not row["discarded"] for row in rows)
    discarded = sum(row["discarded"] for row in rows)
    instrument = controls_pass(positive, negative)
    terminal = instrument and discarded == 0 and all(
        observation.get("tui_completed") is True for observation in observations
    )
    return {
        "repo": repo,
        "instrument": {
            "positive": {"paths": positive["paths"], "statuses": positive["statuses"]},
            "negative": {"paths": negative["paths"], "statuses": negative["statuses"]},
            "pass": instrument,
        },
        "cases": rows,
        "summary": {
            "routed_correctly": routed,
            "failed": len(rows) - routed - discarded,
            "discarded": discarded,
            "total": len(rows),
        },
        "pass": terminal,
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--health-url", default="https://api.fatelabs.ca/health")
    args = parser.parse_args(argv)
    raw_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
    if not raw_trace:
        parser.error("ESTELLE_RECEIPT_PATH is required")
    report = run_measure(args.repo, Path(raw_trace), args.timeout, args.health_url)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    summary = report["summary"]
    print(
        "plain-English routing: "
        f"{summary['routed_correctly']}/{summary['total']} correct, "
        f"{summary['failed']} failed, {summary['discarded']} discarded"
    )
    for row in report["cases"]:
        if not row["pass"]:
            print(
                "routing failure: "
                f"expected {row['expected_suite']}, observed {row['observed_suite']} "
                f"via {row['observed_paths']}: {row['prompt']}"
            )
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
