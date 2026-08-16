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


def tui_surface_receipt(
    command: str,
    repo: str,
    timeout: float = 30,
) -> dict[str, object]:
    assert command.startswith("/")
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
            os.write(fd, f"{command} ".encode())
            time.sleep(0.1)
            os.write(fd, b"\r")
            visible = _read_until(fd, observed, (f"you  {command}",), ready_deadline)
            visible = _read_until(fd, observed, ("› Ask Estelle",), ready_deadline)
        passed = (
            f"you  {command}" in visible
            and "› Ask Estelle" in visible
            and not any(marker in visible for marker in FAILURE_MARKERS)
        )
        return {"sent": command, "came_back": visible.strip(), "pass": passed}
    finally:
        try:
            os.write(fd, b"\x03")
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except (ChildProcessError, OSError, ProcessLookupError):
            pass


def run_receipts(
    expected_version: str, repo: str, timeout: float
) -> dict[str, object]:
    receipts = [installed_version_receipt(expected_version)]
    receipts.extend(
        tui_surface_receipt(surface, repo, timeout) for surface in READ_SURFACES
    )
    passed = sum(receipt["pass"] is True for receipt in receipts)
    return {
        "expected_version": expected_version,
        "repo": repo,
        "receipts": receipts,
        "summary": {"passed": passed, "failed": len(receipts) - passed},
    }


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
