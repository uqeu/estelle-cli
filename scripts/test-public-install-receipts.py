#!/usr/bin/env python3
"""Pure contract tests for the public INSTALL receipt parser."""

import importlib.util
import json
import tempfile
import unittest
from subprocess import TimeoutExpired
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("public-install-receipts.py")
SPEC = importlib.util.spec_from_file_location("public_install_receipts", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PublicInstallReceiptTests(unittest.TestCase):
    def test_mixed_doctor_accepts_truthful_ready_rows_with_both_language_counts(self) -> None:
        output = "\n".join([
            "Repository ingest preflight  local file boundary only; server index/runtime not proven",
            "Repository TypeScript ingest preflight  ready · 15/15 files cross the local ingest boundary",
            "Repository Go ingest preflight  ready · 34/34 files cross the local ingest boundary",
        ])
        self.assertTrue(MODULE.mixed_doctor_positive(output))
        self.assertFalse(MODULE.mixed_doctor_positive(output.replace("Repository Go", "Repository Rust")))

    def test_mixed_scope_does_not_repeat_terminal_brief_or_setup_receipts(self) -> None:
        self.assertEqual(
            MODULE.receipt_plan("mixed"),
            {"brief": False, "setup": False, "mixed": True},
        )
        self.assertEqual(
            MODULE.receipt_plan("all"),
            {"brief": True, "setup": True, "mixed": True},
        )

    def test_inter_surface_pause_exceeds_the_server_dependency_cooldown(self) -> None:
        self.assertGreater(MODULE.INTER_SURFACE_COOLDOWN_S, 30)

    def test_timed_out_command_becomes_a_failed_receipt_instead_of_erasing_it(self) -> None:
        with mock.patch.object(
            MODULE.subprocess,
            "run",
            side_effect=TimeoutExpired(["estelle", "setup"], 1200, "partial", "late"),
        ):
            result = MODULE.run(Path("estelle"), ["setup"], Path("."))
        self.assertEqual(result["returncode"], 124)
        self.assertEqual(result["stdout"], "partial")
        self.assertIn("timed out", result["stderr"])

    def test_trace_requires_every_named_remote_readback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "trace.jsonl"
            records = [
                {"request": {"path": "/mcp"}, "response": {"status": 200}},
                {"request": {"path": "/ingest/start"}, "response": {"status": 200}},
                {
                    "request": {"path": "/ingest/progress"},
                    "response": {"status": 200, "body": {"state": "done", "percent": 100}},
                },
                {"request": {"path": "/deep-search"}, "response": {"status": 200}},
            ]
            trace.write_text("".join(json.dumps(row) + "\n" for row in records))
            required = {"/mcp", "/ingest/start", "/ingest/progress", "/deep-search"}
            passed, _ = MODULE.trace_summary(trace, required)
            self.assertTrue(passed)
            records.pop()
            trace.write_text("".join(json.dumps(row) + "\n" for row in records))
            passed, _ = MODULE.trace_summary(trace, required)
            self.assertFalse(passed)

    def test_trace_refuses_a_nonterminal_ingest_progress_readback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "trace.jsonl"
            records = [
                {"request": {"path": "/ingest/start"}, "response": {"status": 200}},
                {
                    "request": {"path": "/ingest/progress"},
                    "response": {
                        "status": 200,
                        "body": {"state": "ingesting", "percent": 42},
                    },
                },
            ]
            trace.write_text("".join(json.dumps(row) + "\n" for row in records))
            passed, summary = MODULE.trace_summary(
                trace, {"/ingest/start", "/ingest/progress"}
            )
            self.assertFalse(passed)
            self.assertFalse(summary["terminal_ingest"])
            records[-1]["response"]["body"] = {"state": "done", "percent": 100}
            trace.write_text("".join(json.dumps(row) + "\n" for row in records))
            passed, summary = MODULE.trace_summary(
                trace, {"/ingest/start", "/ingest/progress"}
            )
            self.assertTrue(passed)
            self.assertTrue(summary["terminal_ingest"])

    def test_trace_keeps_only_the_safe_error_reason_not_secret_findings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "trace.jsonl"
            trace.write_text(json.dumps({
                "request": {"path": "/ingest/start"},
                "response": {
                    "status": 422,
                    "body": {
                        "error": {
                            "code": 422,
                            "type": "unsafe_ingest",
                            "message": "ingest refused: findings present; no files were stored",
                        },
                        "secret_findings": [
                            {"path": "must-not-escape.ts", "line": 7, "shape": "hidden"}
                        ],
                    },
                },
            }) + "\n")
            passed, summary = MODULE.trace_summary(trace, {"/ingest/start"})
            self.assertFalse(passed)
            self.assertEqual(summary["errors"], {
                "/ingest/start": [{
                    "status": 422,
                    "code": 422,
                    "type": "unsafe_ingest",
                    "message": "ingest refused: findings present; no files were stored",
                }]
            })
            self.assertNotIn("must-not-escape", json.dumps(summary))

    def test_mixed_sweep_accepts_sync_or_terminal_background_not_started_only(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "trace.jsonl"
            trace.write_text(json.dumps({
                "request": {"path": "/sync"}, "response": {"status": 200}
            }) + "\n")
            passed, summary = MODULE.sweep_trace_summary(trace)
            self.assertTrue(passed)
            self.assertEqual(summary["sweep_transport"], "sync")

            rows = [
                {"request": {"path": "/ingest/start"}, "response": {"status": 200}},
                {"request": {"path": "/ingest/progress"},
                 "response": {"status": 200, "body": {"state": "ingesting"}}},
            ]
            trace.write_text("".join(json.dumps(row) + "\n" for row in rows))
            passed, summary = MODULE.sweep_trace_summary(trace)
            self.assertFalse(passed)
            self.assertEqual(summary["sweep_transport"], "background-incomplete")
            rows[-1]["response"]["body"]["state"] = "done"
            trace.write_text("".join(json.dumps(row) + "\n" for row in rows))
            passed, summary = MODULE.sweep_trace_summary(trace)
            self.assertTrue(passed)
            self.assertEqual(summary["sweep_transport"], "background-terminal")

    def test_setup_accepts_sync_or_terminal_background_sweep_but_not_started_only(self) -> None:
        common = [
            {"request": {"path": "/mcp"}, "response": {"status": 200}},
            {"request": {"path": "/v1/chat/completions"}, "response": {"status": 200}},
        ]
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "trace.jsonl"
            sync = common + [
                {"request": {"path": "/sync"}, "response": {"status": 200}}
            ]
            trace.write_text("".join(json.dumps(row) + "\n" for row in sync))
            passed, summary = MODULE.setup_trace_summary(trace)
            self.assertTrue(passed)
            self.assertEqual(summary["sweep_transport"], "sync")

            started = common + [
                {"request": {"path": "/ingest/start"}, "response": {"status": 200}},
                {
                    "request": {"path": "/ingest/progress"},
                    "response": {
                        "status": 200,
                        "body": {"state": "ingesting", "percent": 50},
                    },
                },
            ]
            trace.write_text("".join(json.dumps(row) + "\n" for row in started))
            passed, summary = MODULE.setup_trace_summary(trace)
            self.assertFalse(passed)
            self.assertEqual(summary["sweep_transport"], "background-incomplete")

            started[-1]["response"]["body"] = {"state": "done", "percent": 100}
            trace.write_text("".join(json.dumps(row) + "\n" for row in started))
            passed, summary = MODULE.setup_trace_summary(trace)
            self.assertTrue(passed)
            self.assertEqual(summary["sweep_transport"], "background-terminal")

    def test_health_requires_identity_and_exact_surface(self) -> None:
        healthy = {
            "build": "abc123",
            "build_verified": True,
            "surface": MODULE.EXPECTED_SURFACE,
        }
        self.assertTrue(MODULE.verified_health(healthy))
        self.assertFalse(MODULE.verified_health({**healthy, "surface": {"tools_base": 16}}))
        self.assertFalse(MODULE.verified_health({**healthy, "build_verified": False}))

    def test_each_surface_discards_a_cross_build_window(self) -> None:
        healthy = {
            "build": "before-sha",
            "build_verified": True,
            "surface": MODULE.EXPECTED_SURFACE,
        }
        stable = MODULE.build_window(healthy, dict(healthy))
        self.assertTrue(stable["stable_and_verified"])
        self.assertFalse(stable["discarded"])
        crossed = MODULE.build_window(healthy, {**healthy, "build": "after-sha"})
        self.assertFalse(crossed["stable_and_verified"])
        self.assertTrue(crossed["discarded"])
        self.assertEqual(crossed["before"], "before-sha")
        self.assertEqual(crossed["after"], "after-sha")

    def test_failed_command_reason_is_bounded_and_keeps_the_actionable_tail(self) -> None:
        result = {"stdout": "prefix\n" + "x" * 2_000, "stderr": "capacity refused"}
        reason = MODULE.command_reason(result)
        self.assertLessEqual(len(reason), 1_000)
        self.assertIn("capacity refused", reason)
        self.assertNotIn("prefix", reason)


if __name__ == "__main__":
    unittest.main()
