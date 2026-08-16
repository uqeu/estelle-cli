#!/usr/bin/env python3
"""Drive customer-visible surfaces through the installed public Estelle binary."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

sys.dont_write_bytecode = True
from terminal_screen import rendered_screen


READ_SURFACES = [
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

FAILURE_MARKERS = (
    "Estelle rejected the stored credential.",
    "Estelle returned HTTP ",
    "The Estelle request exceeded ",
    "The Estelle request could not reach a response.",
    "The request was cancelled.",
    "The Estelle request failed:",
)
GROUNDING_QUESTION = "Which file defines an application entry point in this repository?"
DIFF_SURFACES = ("/review", "/scan")
SMALL_SWEEP_PATH = "rag_tutorials/multimodal_agentic_rag/frontend"


def installed_version_receipt(expected_tag: str) -> dict[str, object]:
    assert expected_tag.startswith("v")
    assert len(expected_tag) > 1
    resolved = shutil.which("estelle")
    if resolved is None:
        return {
            "sent": "estelle --version",
            "came_back": "bare estelle did not resolve",
            "pass": False,
            "resolved_binary": "",
        }
    result = subprocess.run(
        ["estelle", "--version"],
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )
    output = result.stdout.strip()
    return {
        "sent": "estelle --version",
        "came_back": output or result.stderr.strip(),
        "pass": result.returncode == 0 and output == f"estelle {expected_tag[1:]}",
        "resolved_binary": str(resolved),
    }


def repository_size_receipt(
    root: Path, minimum_per_language: int = 100
) -> dict[str, object]:
    assert minimum_per_language > 0
    assert root.is_dir()
    python_files = 0
    typescript_files = 0
    for path in root.rglob("*"):
        if not path.is_file() or ".git" in path.parts:
            continue
        python_files += path.suffix == ".py"
        typescript_files += path.suffix in (".ts", ".tsx")
    output = f"{python_files:,} Python + {typescript_files:,} TypeScript files"
    return {
        "sent": "measure cloned public repository",
        "came_back": output,
        "pass": python_files >= minimum_per_language
        and typescript_files >= minimum_per_language,
    }


def erasure_gate_receipt() -> dict[str, object]:
    arguments = ["estelle", "memory", "forget", "receipt-sentinel"]
    result = subprocess.run(
        arguments,
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )
    output = "\n".join(
        part.strip() for part in (result.stdout, result.stderr) if part.strip()
    )
    required = ("EVERY namespace", "--yes", "Nothing was sent")
    return {
        "sent": " ".join(arguments),
        "came_back": output,
        "pass": result.returncode == 0 and all(marker in output for marker in required),
    }


def command_receipt(
    arguments: list[str], required: tuple[str, ...], timeout: float
) -> dict[str, object]:
    sent = " ".join(arguments)
    try:
        result = subprocess.run(
            arguments,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return {"sent": sent, "came_back": f"timed out after {timeout}s", "pass": False}
    output = "\n".join(
        part.strip() for part in (result.stdout, result.stderr) if part.strip()
    )
    return {
        "sent": sent,
        "came_back": output,
        "pass": result.returncode == 0 and all(marker in output for marker in required),
    }


def head_surface_receipts(timeout: float = 600) -> list[dict[str, object]]:
    return [
        command_receipt(
            ["estelle", "sweep", "--path", SMALL_SWEEP_PATH], ("Repo swept",), timeout
        ),
        command_receipt(["estelle", "sweep"], ("Repo swept",), timeout),
        command_receipt(["estelle", "reindex"], ("Memory current",), timeout),
    ]


def first_run_picker_receipt(timeout: float = 10) -> dict[str, object]:
    environment = os.environ.copy()
    environment.pop("ESTELLE_API_KEY", None)
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe("estelle", ["estelle"], environment)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 120, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    try:
        visible = _read_until(
            fd, observed, ("CONNECT ESTELLE",), time.monotonic() + timeout
        )
        required = ("CONNECT ESTELLE", "1 Estelle account", "2 Claude subscription")
        return {
            "sent": "estelle (without a credential)",
            "came_back": visible.strip(),
            "pass": all(marker in visible for marker in required),
        }
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except (ChildProcessError, OSError, ProcessLookupError):
            pass


def _read_until(
    fd: int, observed: bytearray, markers: tuple[str, ...], deadline: float
) -> str:
    visible = rendered_screen(observed, rows=50, columns=160)
    while time.monotonic() < deadline and not any(marker in visible for marker in markers):
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            observed.extend(chunk)
            visible = rendered_screen(observed, rows=50, columns=160)
    return visible


def tui_turn_receipt(
    turn: str,
    repo: str,
    timeout: float = 30,
) -> dict[str, object]:
    assert turn.strip() == turn and turn
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp("estelle", ["estelle", "--repo", repo])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    visible = ""
    try:
        ready_deadline = time.monotonic() + timeout
        visible = _read_until(fd, observed, ("Ask Estelle",), ready_deadline)
        if "Ask Estelle" in visible:
            time.sleep(0.25)
            submitted = f"{turn} " if turn.startswith("/") else turn
            os.write(fd, submitted.encode())
            time.sleep(0.1)
            os.write(fd, b"\r")
            visible = _read_until(fd, observed, (f"you  {turn}",), ready_deadline)
            visible = _read_until(fd, observed, ("› Ask Estelle",), ready_deadline)
        passed = (
            f"you  {turn}" in visible
            and "› Ask Estelle" in visible
            and not any(marker in visible for marker in FAILURE_MARKERS)
        )
        return {"sent": turn, "came_back": visible.strip(), "pass": passed}
    finally:
        try:
            os.write(fd, b"\x03")
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except (ChildProcessError, OSError, ProcessLookupError):
            pass


def tui_surface_receipt(
    command: str, repo: str, timeout: float = 30
) -> dict[str, object]:
    assert command.startswith("/")
    return tui_turn_receipt(command, repo, timeout)


def http_contract_receipt(path: Path) -> tuple[dict[str, object], list[object]]:
    try:
        records = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    except (OSError, json.JSONDecodeError) as error:
        return (
            {
                "sent": "inspect sanitized HTTP trace",
                "came_back": f"trace unreadable: {type(error).__name__}",
                "pass": False,
            },
            [],
        )
    requests = [record.get("request", {}) for record in records]
    answer = next(
        (
            request
            for request in requests
            if request.get("path") == "/deep-search"
            and request.get("body", {}).get("question") == GROUNDING_QUESTION
            and isinstance(request.get("body", {}).get("working_memory"), dict)
        ),
        None,
    )
    deep = any(
        request.get("path") == "/gate" and request.get("body", {}).get("deep") is True
        for request in requests
    )
    whole_lockfile = any(
        request.get("path") == "/scan"
        and any(
            file.get("path", "").endswith("package-lock.json")
            and len(file.get("content", "")) > 1_000
            for file in request.get("body", {}).get("files", [])
            if isinstance(file, dict)
        )
        for request in requests
    )
    separated = answer is not None and not any(
        key in answer.get("body", {}) for key in ("instruction", "prompt")
    )
    head_routes = {
        request.get("path")
        for request in requests
        if request.get("path") in ("/sync", "/ingest/start", "/reindex")
        and len(request.get("body", {}).get("head", "")) == 40
        and all(
            character in "0123456789abcdef"
            for character in request.get("body", {}).get("head", "")
        )
    }
    three_heads = head_routes == {"/sync", "/ingest/start", "/reindex"}
    proof = (
        f"grounded question data-only={separated}; deep review={deep}; "
        f"whole lockfile={whole_lockfile}; three head markers={three_heads}"
    )
    return (
        {
            "sent": "inspect sanitized HTTP trace",
            "came_back": proof,
            "pass": separated and deep and whole_lockfile and three_heads,
        },
        records,
    )


def run_receipts(
    expected_version: str, repo: str, timeout: float
) -> dict[str, object]:
    receipts = [
        installed_version_receipt(expected_version),
        repository_size_receipt(Path.cwd()),
        erasure_gate_receipt(),
        first_run_picker_receipt(),
    ]
    receipts.extend(
        tui_surface_receipt(surface, repo, timeout) for surface in READ_SURFACES
    )
    receipts.append(tui_turn_receipt(GROUNDING_QUESTION, repo, timeout))
    receipts.extend(
        tui_surface_receipt(surface, repo, timeout) for surface in DIFF_SURFACES
    )
    receipts.extend(head_surface_receipts(max(timeout, 600)))
    http_records = None
    if raw_path := os.environ.get("ESTELLE_RECEIPT_PATH"):
        http_receipt, http_records = http_contract_receipt(Path(raw_path))
        receipts.append(http_receipt)
    passed = sum(receipt["pass"] is True for receipt in receipts)
    report = {
        "expected_version": expected_version,
        "repo": repo,
        "receipts": receipts,
        "summary": {"passed": passed, "failed": len(receipts) - passed},
    }
    if http_records is not None:
        report["http_contracts"] = http_records
    return report


def main(argv: list[str]) -> int:
    if argv == ["--list"]:
        print(json.dumps(READ_SURFACES))
        return 0
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args(argv)
    report = run_receipts(args.expected_version, args.repo, args.timeout)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    summary = report["summary"]
    print(f"public receipts: {summary['passed']} passed, {summary['failed']} failed")
    return 0 if summary["failed"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
