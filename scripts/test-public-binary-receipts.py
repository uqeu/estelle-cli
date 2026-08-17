#!/usr/bin/env python3
"""Behavioral tests for the installed-public-binary receipt harness."""

from __future__ import annotations

import json
import importlib.util
import os
import subprocess
import sys
import tempfile
from pathlib import Path


sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
HARNESS = ROOT / "scripts" / "public-binary-receipts.py"

EXPECTED_READ_SURFACES = [
    "/init",
    "/graph",
    "/graph nodes",
    "/me",
    "/keys",
    "/team",
    "/team board",
    "/cards",
    "/entities",
    "/usage",
    "/activity",
    "/runs",
    "/outcomes",
    "/memories",
    "/analytics",
    "/audit",
    "/requests",
    "/presence",
    "/leaderboard",
    "/marketplace",
    "/automations",
    "/suites",
    "/billing",
    "/sessions",
]
EXPECTED_DROPPED_COMMANDS = [
    "pet",
    "vim",
    "theme",
    "statusline",
    "title",
    "raw",
    "copy",
    "mention",
    "ide",
    "apps",
    "plugins",
    "experimental",
    "app",
    "import",
    "logout",
    "rollout",
    "debug-config",
    "test-approval",
    "debug-m-drop",
    "debug-m-update",
    "setup-default-sandbox",
    "sandbox-add-read-dir",
    "hooks",
    "personality",
    "agent",
    "subagents",
]


def load_harness():
    spec = importlib.util.spec_from_file_location("public_binary_receipts", HARNESS)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_inventory() -> None:
    result = subprocess.run(
        [sys.executable, str(HARNESS), "--list"],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    surfaces = json.loads(result.stdout)
    assert surfaces == EXPECTED_READ_SURFACES, surfaces
    assert len(set(surfaces)) == 24


def test_installed_version() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-receipt-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n[ \"$1\" = --version ] && printf 'estelle 9.9.9\\n'\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().installed_version_receipt("v9.9.9")
        finally:
            os.environ["PATH"] = original_path
        assert receipt["sent"] == "estelle --version"
        assert receipt["came_back"] == "estelle 9.9.9"
        assert receipt["pass"] is True
        assert Path(receipt["resolved_binary"]).samefile(fake_estelle)


def test_tui_surface() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-tui-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "printf '\\033[2J\\033[1;1HAsk Estelle\\n'\n"
            "IFS= read -r command\n"
            "printf '\\033[2J\\033[1;1Hyou  %s\\nSERVER RECEIPT OK\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_surface_receipt(
                "/graph", "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["sent"] == "/graph"
        assert "you  /graph" in receipt["came_back"]
        assert "SERVER RECEIPT OK" in receipt["came_back"]
        assert receipt["pass"] is True


def test_question_turn_uses_the_same_public_tui_seam() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-question-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\nIFS= read -r turn\n"
            "printf 'you  %s\\nGROUNDED ANSWER\\n› Ask Estelle\\n' \"$turn\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_turn_receipt(
                "Which file defines the application entry point?",
                "uqeu/estelle",
                timeout=3,
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True
        assert "GROUNDED ANSWER" in receipt["came_back"]


def test_tui_turn_waits_past_the_composer_paste_suppression_window() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-paste-window-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys, time, tty\n"
            "print('Ask Estelle', flush=True)\n"
            "tty.setraw(0)\n"
            "started = None\n"
            "data = bytearray()\n"
            "while True:\n"
            "    byte = os.read(0, 1)\n"
            "    if started is None:\n"
            "        started = time.monotonic()\n"
            "    if byte in (b'\\r', b'\\n'):\n"
            "        break\n"
            "    data.extend(byte)\n"
            "elapsed = time.monotonic() - started\n"
            "turn = data.decode()\n"
            "if elapsed < 0.18:\n"
            "    print('Estelle returned HTTP 409: Enter suppressed', flush=True)\n"
            "else:\n"
            "    print(f'you  {turn}\\nTIMING RECEIPT\\n› Ask Estelle', flush=True)\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_turn_receipt(
                "timing sentinel", "uqeu/estelle", timeout=2
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True
        assert "TIMING RECEIPT" in receipt["came_back"]


def test_grounded_question_requires_both_request_and_response_evidence() -> None:
    harness = load_harness()
    record = {
        "request": {
            "path": "/deep-search",
            "body": {
                "question": harness.GROUNDING_QUESTION,
                "working_memory": {"files": [{"path": "main.py", "content": "run()"}]},
            },
        },
        "response": {
            "status": 200,
            "body": {
                "answer": "main.py defines the application entry point.",
                "grounded": True,
                "sources": [{"file": "main.py", "line": 1}],
            },
        },
    }
    assert harness._surface_http_contract(harness.GROUNDING_QUESTION, record) is True
    no_working_memory = json.loads(json.dumps(record))
    del no_working_memory["request"]["body"]["working_memory"]
    assert (
        harness._surface_http_contract(harness.GROUNDING_QUESTION, no_working_memory)
        is False
    )
    ungrounded = json.loads(json.dumps(record))
    ungrounded["response"]["body"]["grounded"] = False
    assert harness._surface_http_contract(harness.GROUNDING_QUESTION, ungrounded) is False


def test_skill_thread_receipt_keeps_both_turns_in_one_tui_process() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-skill-thread-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\n"
            "IFS= read -r first\n"
            "printf 'you  %s\\nFIRST SKILL REPLY pid=%s\\n› Ask Estelle\\n' \"$first\" \"$$\"\n"
            "IFS= read -r second\n"
            "printf 'you  %s\\nSECOND SKILL REPLY pid=%s\\n› Ask Estelle\\n' \"$second\" \"$$\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_skill_thread_receipt(
                "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True
        assert receipt["processes_started"] == 1
        screens = receipt["came_back"]
        assert "FIRST SKILL REPLY" in screens[0]
        assert "SECOND SKILL REPLY" in screens[1]
        first_pid = screens[0].split("pid=", 1)[1].split()[0]
        second_pid = screens[1].split("pid=", 1)[1].split()[0]
        assert first_pid == second_pid


def test_tui_surface_fails_closed() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-tui-fail-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "printf 'Ask Estelle\\n'\n"
            "IFS= read -r command\n"
            "printf 'you  %s\\nEstelle returned HTTP 404: absent\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_surface_receipt(
                "/graph", "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is False
        assert "HTTP 404" in receipt["came_back"]


def test_init_receipt_rejects_an_echo_without_its_named_http_route() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-init-route-receipt-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\nIFS= read -r command\n"
            "printf 'you  %s\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        trace = root / "http.jsonl"
        trace.write_text("", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        original_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        os.environ["ESTELLE_RECEIPT_PATH"] = str(trace)
        try:
            receipt = load_harness().tui_surface_receipt(
                "/init", "uqeu/estelle", timeout=1
            )
        finally:
            os.environ["PATH"] = original_path
            if original_trace is None:
                os.environ.pop("ESTELLE_RECEIPT_PATH", None)
            else:
                os.environ["ESTELLE_RECEIPT_PATH"] = original_trace
        assert receipt["pass"] is False
        assert receipt["http_route"] == {
            "path": "/wiki",
            "status": "not observed",
            "contract": False,
        }


def test_init_receipt_accepts_a_nonempty_wiki_from_its_named_http_route() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-init-route-receipt-ok-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\nIFS= read -r command\n"
            "printf '%s\\n' '{\"request\":{\"path\":\"/wiki\"},\"response\":{\"status\":200,\"body\":{\"repo\":\"uqeu/estelle\",\"wiki\":\"Architecture\"}}}' >> \"$ESTELLE_RECEIPT_PATH\"\n"
            "printf 'you  %s\\nArchitecture\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        trace = root / "http.jsonl"
        trace.write_text("", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        original_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        os.environ["ESTELLE_RECEIPT_PATH"] = str(trace)
        try:
            receipt = load_harness().tui_surface_receipt(
                "/init", "uqeu/estelle", timeout=1
            )
        finally:
            os.environ["PATH"] = original_path
            if original_trace is None:
                os.environ.pop("ESTELLE_RECEIPT_PATH", None)
            else:
                os.environ["ESTELLE_RECEIPT_PATH"] = original_trace
        assert receipt["pass"] is True
        assert receipt["http_route"] == {
            "path": "/wiki",
            "status": 200,
            "contract": True,
        }


def test_outcomes_receipt_rejects_an_echo_without_its_named_http_route() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-outcomes-route-receipt-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\nIFS= read -r command\n"
            "printf 'you  %s\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        trace = root / "http.jsonl"
        trace.write_text("", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        original_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        os.environ["ESTELLE_RECEIPT_PATH"] = str(trace)
        try:
            receipt = load_harness().tui_surface_receipt(
                "/outcomes", "uqeu/estelle", timeout=1
            )
        finally:
            os.environ["PATH"] = original_path
            if original_trace is None:
                os.environ.pop("ESTELLE_RECEIPT_PATH", None)
            else:
                os.environ["ESTELLE_RECEIPT_PATH"] = original_trace
        assert receipt["pass"] is False
        assert receipt["http_route"] == {
            "path": "/outcomes",
            "status": "not observed",
            "contract": False,
        }


def _fake_outcomes_receipt(body: dict) -> dict:
    with tempfile.TemporaryDirectory(prefix="estelle-outcomes-contract-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        record = json.dumps(
            {
                "request": {"path": "/outcomes"},
                "response": {"status": 200, "body": body},
            },
            separators=(",", ":"),
        )
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\nIFS= read -r command\n"
            f"printf '%s\\n' '{record}' >> \"$ESTELLE_RECEIPT_PATH\"\n"
            "printf 'you  %s\\nOUTCOMES RECEIPT\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        trace = root / "http.jsonl"
        trace.write_text("", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        original_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        os.environ["ESTELLE_RECEIPT_PATH"] = str(trace)
        try:
            return load_harness().tui_surface_receipt(
                "/outcomes", "uqeu/estelle", timeout=1
            )
        finally:
            os.environ["PATH"] = original_path
            if original_trace is None:
                os.environ.pop("ESTELLE_RECEIPT_PATH", None)
            else:
                os.environ["ESTELLE_RECEIPT_PATH"] = original_trace


def test_outcomes_receipt_accepts_an_honest_empty_account_contract() -> None:
    receipt = _fake_outcomes_receipt(
        {
            "total": 0,
            "accepted": 0,
            "reverted": 0,
            "rejected": 0,
            "accept_rate": 0.0,
            "revert_rate": 0.0,
        }
    )
    assert receipt["pass"] is True
    assert receipt["http_route"] == {
        "path": "/outcomes",
        "status": 200,
        "contract": True,
    }


def test_outcomes_receipt_rejects_a_fieldless_200() -> None:
    receipt = _fake_outcomes_receipt({})
    assert receipt["pass"] is False
    assert receipt["http_route"] == {
        "path": "/outcomes",
        "status": 200,
        "contract": False,
    }


def test_failed_receipt_diagnostics_name_the_surface_without_dumping_its_body() -> None:
    harness = load_harness()
    report = {
        "receipts": [
            {
                "sent": "/graph",
                "came_back": "Estelle returned HTTP 404: private response body",
                "pass": False,
            },
            {
                "sent": "estelle hook welcome --event SessionStart",
                "event": "SessionStart/welcome",
                "came_back": "exited with code 1 and no stdout/stderr",
                "exit_code": 1,
                "pass": False,
            },
            {"sent": "/me", "came_back": "working", "pass": True},
        ]
    }

    diagnostics = harness.failed_receipt_diagnostics(report)

    assert diagnostics == [
        "/graph: Estelle returned HTTP",
        "SessionStart/welcome: exited with code 1",
    ]
    assert "private response body" not in "\n".join(diagnostics)


def test_complete_harness_writes_every_receipt() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-full-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = --version ]; then printf 'estelle 9.9.9\\n'; exit 0; fi\n"
            "if [ \"$1\" = memory ]; then\n"
            "  printf 'EVERY namespace; re-run with --yes. Nothing was sent.\\n'; exit 0\n"
            "fi\n"
            "if [ \"$1\" = sweep ]; then printf 'Repo swept.\\n'; exit 0; fi\n"
            "if [ \"$1\" = reindex ]; then printf 'Memory current.\\n'; exit 0; fi\n"
            "if [ \"$1\" = install-hooks ]; then printf 'full session lifecycle\\n'; exit 0; fi\n"
            "if [ \"$1\" = hook ]; then printf '{\"mode\":\"%s\"}\\n' \"$2\"; exit 0; fi\n"
            "if [ -f \"$HOME/.estelle/auth.json\" ]; then\n"
            "  printf 'Ask Estelle\\nrejected on a background poll. It was NOT removed\\n› Ask Estelle\\n'\n"
            "  IFS= read -r command; rm \"$HOME/.estelle/auth.json\"\n"
            "  printf 'you  %s\\nrejected on a background poll, me — different routes, so it was removed.\\n› Ask Estelle\\n' \"$command\"; exit 0\n"
            "fi\n"
            "if [ -z \"${ESTELLE_API_KEY:-}\" ]; then\n"
            "  printf 'CONNECT ESTELLE\\n1 Estelle account\\n2 Claude subscription\\n'\n"
            "  stty raw -echo; dd bs=1 count=1 >/dev/null 2>&1\n"
            "  printf 'Estelle key: '; exit 0\n"
            "fi\n"
            "printf 'Ask Estelle\\n'\n"
            "while IFS= read -r command; do\n"
            "  name=${command#/}; name=${name%% *}\n"
            "  case ' pet vim theme statusline title raw copy mention ide apps plugins experimental app import logout rollout debug-config test-approval debug-m-drop debug-m-update setup-default-sandbox sandbox-add-read-dir hooks personality agent subagents ' in\n"
            "    *\" $name \"*) printf '\\033[2J\\033[1;1HUnknown command /%s; nothing ran and nothing was sent. Use /help.\\n› Ask Estelle\\n' \"$name\" ;;\n"
            "    *) printf 'you  %s\\nSERVER RECEIPT OK\\n› Ask Estelle\\n' \"$command\" ;;\n"
            "  esac\n"
            "done\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        for index in range(100):
            (root / f"module-{index}.py").write_text("pass\n", encoding="utf-8")
            (root / f"module-{index}.ts").write_text("export {};\n", encoding="utf-8")
        output = root / "receipts.json"
        environment = os.environ.copy()
        environment["PATH"] = f"{fake_bin}{os.pathsep}{environment.get('PATH', '')}"
        environment["ESTELLE_API_KEY"] = "test-receipt-key"
        result = subprocess.run(
            [
                sys.executable,
                str(HARNESS),
                "--expected-version",
                "v9.9.9",
                "--repo",
                "uqeu/estelle",
                "--output",
                str(output),
                "--timeout",
                "2",
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
            cwd=root,
            timeout=40,
        )
        assert result.returncode == 0, (
            result.stdout,
            result.stderr,
            output.read_text(encoding="utf-8") if output.exists() else "no report",
        )
        report = json.loads(output.read_text(encoding="utf-8"))
        assert report["summary"] == {"passed": 49, "failed": 0}
        assert report["receipts"][4]["after_one_route"] == "retained"
        assert [row["sent"] for row in report["receipts"][5:29]] == EXPECTED_READ_SURFACES
        assert report["receipts"][29]["sent"].startswith("Which file defines")
        assert report["receipts"][30]["sent"] == "hi"
        assert report["receipts"][31]["processes_started"] == 1
        assert len(report["receipts"][32]["sent"]) == 26
        assert [row["sent"] for row in report["receipts"][33:35]] == ["/review", "/scan"]
        assert report["receipts"][37]["sent"] == "estelle reindex"
        assert report["receipts"][-1]["event"] == "UserPromptSubmit/context"
        assert all(row["pass"] for row in report["receipts"])


def test_repository_size_receipt() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-repo-") as raw_dir:
        root = Path(raw_dir)
        for name in ["one.py", "two.py", "one.ts", "two.ts"]:
            (root / name).write_text("sentinel\n", encoding="utf-8")
        receipt = load_harness().repository_size_receipt(root, minimum_per_language=2)
        assert receipt["pass"] is True
        assert receipt["sent"] == "measure cloned public repository"
        assert "2 Python + 2 TypeScript files" in receipt["came_back"]


def test_erasure_gate_receipt() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-erasure-receipt-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'EVERY namespace owned by this account\\n'\n"
            "printf 'Re-run with --yes to confirm. Nothing was sent.\\n'\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().erasure_gate_receipt()
        finally:
            os.environ["PATH"] = original_path
        assert receipt["sent"] == "estelle memory forget receipt-sentinel"
        assert receipt["pass"] is True
        assert "Nothing was sent" in receipt["came_back"]


def test_first_run_picker_receipt() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-picker-receipt-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'CONNECT ESTELLE\\n'\n"
            "printf '1 Estelle account\\n2 Claude subscription\\n'\n"
            "stty raw -echo\n"
            "dd bs=1 count=1 >/dev/null 2>&1\n"
            "printf 'Estelle key: '\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().first_run_picker_receipt(timeout=3)
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True
        assert "1 Estelle account" in receipt["came_back"]
        assert "2 Claude subscription" in receipt["came_back"]
        assert "Estelle key:" in receipt["came_back"]


def test_credential_retention_receipt_requires_two_named_routes() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-credential-retention-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "auth_path=\"$HOME/.estelle/auth.json\"\n"
            "printf 'Ask Estelle\\n'\n"
            "printf 'rejected on a background poll. It was NOT removed\\n› Ask Estelle\\n'\n"
            "IFS= read -r command\n"
            "rm \"$auth_path\"\n"
            "printf 'you  %s\\nrejected on a background poll, me — different routes, so it was removed.\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().credential_retention_receipt(
                "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True
        assert receipt["after_one_route"] == "retained"
        assert receipt["after_two_routes"] == "removed"
        assert "a background poll" in receipt["came_back"]
        assert "me" in receipt["came_back"]
        assert "public-receipt-intentionally-invalid" not in json.dumps(receipt)


def test_dropped_commands_stay_local_in_one_installed_tui() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-dropped-commands-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\nprintf 'Ask Estelle\\n'\n"
            "while IFS= read -r command; do\n"
            "  name=${command#/}; name=${name%% *}\n"
            "  printf '\\033[2J\\033[1;1HUnknown command /%s; nothing ran and nothing was sent. Use /help.\\n› Ask Estelle\\n' \"$name\"\n"
            "done\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        http_trace = root / "http.jsonl"
        http_trace.write_text('{"baseline":true}\n', encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().dropped_command_receipt(
                "uqeu/estelle", http_trace, timeout=10, settle_seconds=0
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is True, receipt
        assert receipt["sent"] == [f"/{name}" for name in EXPECTED_DROPPED_COMMANDS]
        assert receipt["processes_started"] == 1
        assert receipt["http_lines"] == {"before": 1, "after": 1}
        assert all("nothing ran and nothing was sent" in row for row in receipt["came_back"])


def test_dropped_command_isolation_ignores_only_named_passive_routes() -> None:
    harness = load_harness()
    passive = [
        {"request": {"path": "/overview"}},
        {"request": {"path": "/monitor/overview"}},
    ]
    assert harness._unexpected_non_passive_paths(passive) == []
    active_mutant = passive + [{"request": {"path": "/deep-search"}}]
    assert harness._unexpected_non_passive_paths(active_mutant) == ["/deep-search"]


def test_http_contract_receipt_proves_hidden_fields() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-http-receipt-") as raw_dir:
        path = Path(raw_dir) / "http.jsonl"
        records = [
            {
                "request": {
                    "method": "POST",
                    "path": "/deep-search",
                    "body": {
                        "question": "Which file defines an application entry point in this repository?",
                        "working_memory": {"files": [{"path": "app.py", "content": "sentinel"}]},
                    },
                },
                "response": {"status": 200, "body": {"answer": "app.py"}},
            },
            {
                "request": {
                    "method": "POST",
                    "path": "/deep-search",
                    "body": {"question": "hi"},
                },
                "response": {"status": 200, "body": {"answer": "Hello."}},
            },
            {
                "request": {"method": "POST", "path": "/gate", "body": {"deep": True}},
                "response": {"status": 200, "body": {"verdict": "merge"}},
            },
            {
                "request": {
                    "method": "POST",
                    "path": "/scan",
                    "body": {
                        "files": [{"path": "package-lock.json", "content": "x" * 2_000}]
                    },
                },
                "response": {"status": 200, "body": {"findings": []}},
            },
            *[
                {
                    "request": {
                        "method": "POST",
                        "path": route,
                        "body": {"head": "a" * 40},
                    },
                    "response": {"status": 200, "body": {"ok": True}},
                }
                for route in ("/sync", "/ingest/start", "/reindex")
            ],
            {
                "request": {"method": "POST", "path": "/verify", "body": {"answer": "def receipt_probe(): pass"}},
                "response": {"status": 200, "body": {"grounded": True}},
            },
            {
                "request": {"method": "POST", "path": "/reindex", "body": {"files": [{"path": "README.md"}]}},
                "response": {"status": 200, "body": {"ok": True}},
            },
            *[
                {
                    "request": {"method": "POST", "path": "/checkpoint", "body": {"client": {"event": event}}},
                    "response": {"status": 200, "body": {"ok": True}},
                }
                for event in ("Stop", "PreCompact", "SessionEnd")
            ],
            {
                "request": {"method": "POST", "path": "/search", "body": {"query": "Where is the application entry point?"}},
                "response": {"status": 200, "body": {"recall": "app.py"}},
            },
            {
                "request": {
                    "method": "POST",
                    "path": "/skill/run",
                    "body": {
                        "skill": "grill-me",
                        "task": "State one risk in changing a CLI contract.",
                    },
                },
                "response": {
                    "status": 200,
                    "body": {"reply": "The client and server can disagree."},
                },
            },
            {
                "request": {
                    "method": "POST",
                    "path": "/skill/run",
                    "body": {
                        "skill": "grill-me",
                        "task": "Challenge that answer.",
                        "messages": [
                            {
                                "role": "user",
                                "content": "State one risk in changing a CLI contract.",
                            },
                            {
                                "role": "assistant",
                                "content": "The client and server can disagree.",
                            },
                            {"role": "user", "content": "Challenge that answer."},
                        ],
                    },
                },
                "response": {
                    "status": 200,
                    "body": {"reply": "Versioning can contain that risk."},
                },
            },
        ]
        path.write_text(
            "".join(json.dumps(record) + "\n" for record in records), encoding="utf-8"
        )

        receipt, observed = load_harness().http_contract_receipt(path)

        assert receipt["pass"] is True
        assert "deep review" in receipt["came_back"]
        assert "whole lockfile" in receipt["came_back"]
        assert "three head markers=True" in receipt["came_back"]
        assert "hook network rows=True" in receipt["came_back"]
        assert "skill thread=True" in receipt["came_back"]
        assert "conversational upload absent=True" in receipt["came_back"]
        assert observed == records

        missing_head = json.loads(json.dumps(records))
        headed_reindex = next(
            record
            for record in missing_head
            if record["request"].get("path") == "/reindex"
            and "head" in record["request"].get("body", {})
        )
        del headed_reindex["request"]["body"]["head"]
        path.write_text(
            "".join(json.dumps(record) + "\n" for record in missing_head),
            encoding="utf-8",
        )
        failed, _ = load_harness().http_contract_receipt(path)
        assert failed["pass"] is False
        assert "three head markers=False" in failed["came_back"]


def test_head_surface_commands_run_through_bare_binary() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-head-receipt-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = sweep ]; then printf 'Repo swept.\\n'; exit 0; fi\n"
            "if [ \"$1\" = reindex ]; then printf 'Nothing changed. Estelle memory is already current.\\n'; exit 0; fi\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipts = load_harness().head_surface_receipts(timeout=3)
            fake_estelle.write_text(
                "#!/bin/sh\nprintf 'request completed\\n'\n", encoding="utf-8"
            )
            false_green = load_harness().reindex_receipt(timeout=3)
        finally:
            os.environ["PATH"] = original_path
        assert len(receipts) == 3
        assert all(receipt["pass"] for receipt in receipts)
        assert receipts[-1]["sent"] == "estelle reindex"
        assert "already current" in receipts[-1]["came_back"]
        assert false_green["pass"] is False


def test_hook_receipts_drive_every_current_table_row() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-hook-receipt-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = install-hooks ]; then printf 'full session lifecycle\\n'; exit 0; fi\n"
            "if [ \"$1\" = hook ]; then printf '{\"mode\":\"%s\"}\\n' \"$2\"; exit 0; fi\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        (root / "README.md").write_text("receipt fixture\n", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipts = load_harness().hook_event_receipts(root, timeout=3)
        finally:
            os.environ["PATH"] = original_path
        assert len(receipts) == 11
        assert all(receipt["pass"] for receipt in receipts)
        assert [receipt["event"] for receipt in receipts[1:]] == [
            "PreToolUse/ground",
            "PreToolUse/guard",
            "PostToolUse/shift",
            "PostToolUse/sync",
            "PostToolUse/distil",
            "Stop/checkpoint",
            "PreCompact/checkpoint",
            "SessionEnd/checkpoint",
            "SessionStart/welcome",
            "UserPromptSubmit/context",
        ]


def test_hook_receipts_fail_closed_on_one_silent_nonzero() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-hook-receipt-fail-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = install-hooks ]; then printf 'full session lifecycle\\n'; exit 0; fi\n"
            "if [ \"$1\" = hook ] && [ \"$2\" = welcome ]; then exit 1; fi\n"
            "if [ \"$1\" = hook ]; then exit 0; fi\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        (root / "README.md").write_text("receipt fixture\n", encoding="utf-8")
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipts = load_harness().hook_event_receipts(root, timeout=3)
        finally:
            os.environ["PATH"] = original_path

        failed = [receipt for receipt in receipts if not receipt["pass"]]
        assert len(failed) == 1
        assert failed[0]["event"] == "SessionStart/welcome"
        assert failed[0]["exit_code"] == 1
        assert failed[0]["came_back"] == "exited with code 1 and no stdout/stderr"


def main() -> int:
    test_inventory()
    test_installed_version()
    test_tui_surface()
    test_question_turn_uses_the_same_public_tui_seam()
    test_tui_turn_waits_past_the_composer_paste_suppression_window()
    test_grounded_question_requires_both_request_and_response_evidence()
    test_skill_thread_receipt_keeps_both_turns_in_one_tui_process()
    test_tui_surface_fails_closed()
    test_init_receipt_rejects_an_echo_without_its_named_http_route()
    test_init_receipt_accepts_a_nonempty_wiki_from_its_named_http_route()
    test_outcomes_receipt_rejects_an_echo_without_its_named_http_route()
    test_outcomes_receipt_accepts_an_honest_empty_account_contract()
    test_outcomes_receipt_rejects_a_fieldless_200()
    test_failed_receipt_diagnostics_name_the_surface_without_dumping_its_body()
    test_complete_harness_writes_every_receipt()
    test_repository_size_receipt()
    test_erasure_gate_receipt()
    test_first_run_picker_receipt()
    test_credential_retention_receipt_requires_two_named_routes()
    test_dropped_commands_stay_local_in_one_installed_tui()
    test_dropped_command_isolation_ignores_only_named_passive_routes()
    test_http_contract_receipt_proves_hidden_fields()
    test_head_surface_commands_run_through_bare_binary()
    test_hook_receipts_drive_every_current_table_row()
    test_hook_receipts_fail_closed_on_one_silent_nonzero()

    print("public receipt test: all 24 audited read surfaces are mandatory")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
