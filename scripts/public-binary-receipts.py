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
import tempfile
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
CONVERSATIONAL_QUESTION = "hi"
DIFF_SURFACES = ("/review", "/scan")
SMALL_SWEEP_PATH = "rag_tutorials/multimodal_agentic_rag/frontend"
SKILL_TURNS = (
    "/skill:grill-me State one risk in changing a CLI contract.",
    "/skill:grill-me Challenge that answer.",
)
DROPPED_COMMANDS = (
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
    arguments: list[str],
    required: tuple[str, ...],
    timeout: float,
    input_text: str | None = None,
) -> dict[str, object]:
    sent = " ".join(arguments)
    try:
        result = subprocess.run(
            arguments,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout,
            input=input_text,
        )
    except subprocess.TimeoutExpired:
        return {"sent": sent, "came_back": f"timed out after {timeout}s", "pass": False}
    output = "\n".join(
        part.strip() for part in (result.stdout, result.stderr) if part.strip()
    )
    receipt = {
        "sent": sent,
        "came_back": output,
        "pass": result.returncode == 0 and all(marker in output for marker in required),
    }
    if input_text is not None:
        receipt["stdin"] = input_text
    return receipt


def head_surface_receipts(timeout: float = 600) -> list[dict[str, object]]:
    return [
        command_receipt(
            ["estelle", "sweep", "--path", SMALL_SWEEP_PATH], ("Repo swept",), timeout
        ),
        command_receipt(["estelle", "sweep"], ("Repo swept",), timeout),
        command_receipt(["estelle", "reindex"], ("Memory current",), timeout),
    ]


def _hook_specs(root: Path, transcript: Path) -> list[tuple[str, str, dict]]:
    common = {"cwd": str(root), "session_id": "public-receipt-session"}
    checkpoints = {
        **common,
        "transcript_path": str(transcript),
    }
    return [
        ("PreToolUse/ground", "ground", {**common, "tool_name": "Write", "tool_input": {"file_path": "receipt_probe.py", "content": "def receipt_probe():\n    return 1\n"}}),
        ("PreToolUse/guard", "guard", {**common, "tool_name": "Bash", "tool_input": {"command": "chmod -R 777 /"}}),
        ("PostToolUse/shift", "shift", {**common, "tool_name": "Read", "tool_input": {"file_path": "README.md"}}),
        ("PostToolUse/sync", "sync", {**common, "tool_name": "Write", "tool_input": {"file_path": "README.md"}}),
        ("PostToolUse/distil", "distil", {**common, "tool_name": "Bash", "tool_response": {"stdout": "tests/a.py::test_one PASSED\ntests/a.py::test_two PASSED\n"}}),
        ("Stop/checkpoint", "checkpoint", {**checkpoints, "hook_event_name": "Stop"}),
        ("PreCompact/checkpoint", "checkpoint", {**checkpoints, "hook_event_name": "PreCompact"}),
        ("SessionEnd/checkpoint", "checkpoint", {**checkpoints, "hook_event_name": "SessionEnd"}),
        ("SessionStart/welcome", "welcome", {**common, "hook_event_name": "SessionStart"}),
        ("UserPromptSubmit/context", "context", {**common, "prompt": "Where is the application entry point?", "hook_event_name": "UserPromptSubmit"}),
    ]


def hook_event_receipts(root: Path, timeout: float = 30) -> list[dict[str, object]]:
    transcript = root / ".estelle-public-receipt-transcript.jsonl"
    records = [
        {"type": "user", "cwd": str(root), "gitBranch": "main", "version": "receipt", "message": {"role": "user", "content": "inspect the application entry point"}},
        {"type": "assistant", "message": {"role": "assistant", "model": "receipt", "content": [{"type": "text", "text": "inspection complete"}]}},
    ]
    transcript.write_text(
        "".join(json.dumps(record) + "\n" for record in records), encoding="utf-8"
    )
    receipts = [
        command_receipt(
            ["estelle", "install-hooks"], ("full session lifecycle",), timeout
        )
    ]
    receipts[0]["event"] = "install/current-table"
    for event, mode, payload in _hook_specs(root, transcript):
        receipt = command_receipt(
            ["estelle", "hook", mode], (), timeout, json.dumps(payload)
        )
        receipt["event"] = event
        receipts.append(receipt)
    return receipts


def _terminate_pty(pid: int, fd: int) -> None:
    try:
        os.write(fd, b"\x03")
    except OSError:
        pass
    try:
        os.kill(pid, signal.SIGKILL)
    except (OSError, ProcessLookupError):
        return
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            waited, _ = os.waitpid(pid, os.WNOHANG)
        except (ChildProcessError, OSError):
            return
        if waited == pid:
            return
        time.sleep(0.02)


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
        deadline = time.monotonic() + timeout
        visible = _read_until(fd, observed, ("CONNECT ESTELLE",), deadline)
        if "CONNECT ESTELLE" in visible:
            os.write(fd, b"1")
            visible = _read_until(fd, observed, ("Estelle key:",), deadline)
        required = (
            "CONNECT ESTELLE",
            "1 Estelle account",
            "2 Claude subscription",
            "Estelle key:",
        )
        return {
            "sent": "estelle (without a credential)",
            "came_back": visible.strip(),
            "pass": all(marker in visible for marker in required),
        }
    finally:
        _terminate_pty(pid, fd)


def _write_rejected_fixture(home: Path) -> Path:
    estelle_home = home / ".estelle"
    estelle_home.mkdir(mode=0o700)
    auth_path = estelle_home / "auth.json"
    auth_fd = os.open(auth_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        os.write(auth_fd, b'{"key":"public-receipt-intentionally-invalid"}\n')
    finally:
        os.close(auth_fd)
    return auth_path


def credential_retention_receipt(
    repo: str,
    timeout: float = 30,
) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="estelle-rejected-fixture-") as raw_home:
        home = Path(raw_home)
        auth_path = _write_rejected_fixture(home)
        child_env = os.environ.copy()
        child_env.pop("ESTELLE_API_KEY", None)
        child_env["HOME"] = str(home)
        pid, fd = pty.fork()
        if pid == 0:
            os.execvpe("estelle", ["estelle", "--repo", repo], child_env)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
        os.kill(pid, signal.SIGWINCH)
        observed = bytearray()
        visible = ""
        retained = False
        removed = False
        try:
            visible = _read_until(
                fd, observed, ("NOT removed",), time.monotonic() + timeout
            )
            retained = auth_path.is_file() and "NOT removed" in visible
            if retained:
                os.write(fd, b"/me ")
                time.sleep(0.5)
                os.write(fd, b"\r")
                visible = _read_until(
                    fd, observed, ("you  /me",), time.monotonic() + 1
                )
                if "you  /me" not in visible:
                    os.write(fd, b"\r")
                visible = _read_until(
                    fd,
                    observed,
                    ("different routes, so it was removed",),
                    time.monotonic() + timeout,
                )
                removed = not auth_path.exists()
            named_routes = "a background poll" in visible and "me" in visible
            return {
                "sent": "production rejection on a background poll, then /me",
                "came_back": visible.strip(),
                "fixture": "non-secret intentionally rejected sentinel",
                "after_one_route": "retained" if retained else "missing",
                "after_two_routes": "removed" if removed else "retained",
                "pass": retained
                and removed
                and named_routes
                and "different routes, so it was removed" in visible,
            }
        finally:
            _terminate_pty(pid, fd)


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
        _terminate_pty(pid, fd)


def tui_skill_thread_receipt(
    repo: str,
    timeout: float = 30,
) -> dict[str, object]:
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp("estelle", ["estelle", "--repo", repo])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    screens = []
    passed = True
    try:
        visible = _read_until(
            fd, observed, ("Ask Estelle",), time.monotonic() + timeout
        )
        passed = "Ask Estelle" in visible
        for turn in SKILL_TURNS:
            if not passed:
                break
            deadline = time.monotonic() + timeout
            os.write(fd, f"{turn} ".encode())
            time.sleep(0.1)
            os.write(fd, b"\r")
            visible = _read_until(fd, observed, (f"you  {turn}",), deadline)
            visible = _read_until(fd, observed, ("› Ask Estelle",), deadline)
            screens.append(visible.strip())
            passed = (
                f"you  {turn}" in visible
                and "› Ask Estelle" in visible
                and not any(marker in visible for marker in FAILURE_MARKERS)
            )
        return {
            "sent": list(SKILL_TURNS),
            "came_back": screens,
            "processes_started": 1,
            "pass": passed and len(screens) == len(SKILL_TURNS),
        }
    finally:
        _terminate_pty(pid, fd)


def _trace_line_count(path: Path | None) -> int | None:
    if path is None or not path.exists():
        return 0 if path is not None else None
    return sum(1 for line in path.read_text(encoding="utf-8").splitlines() if line)


def dropped_command_receipt(
    repo: str,
    http_trace: Path | None,
    timeout: float = 30,
    settle_seconds: float = 2,
) -> dict[str, object]:
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp("estelle", ["estelle", "--repo", repo])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    outputs = []
    passed = True
    try:
        visible = _read_until(
            fd, observed, ("Ask Estelle",), time.monotonic() + timeout
        )
        passed = "Ask Estelle" in visible
        time.sleep(settle_seconds)
        _read_until(fd, observed, ("receipt-settle-marker",), time.monotonic() + 0.1)
        before = _trace_line_count(http_trace)
        conversation_deadline = time.monotonic() + timeout
        production_tui = http_trace is not None and settle_seconds > 0
        enter = b"\r" if production_tui else b"\n"
        for name in DROPPED_COMMANDS:
            expected = (
                f"Unknown command /{name}; nothing ran and nothing was sent. Use /help."
            )
            os.write(fd, f"/{name} ".encode())
            time.sleep(0.1)
            os.write(fd, enter)
            visible = _read_until(
                fd,
                observed,
                (expected,),
                min(conversation_deadline, time.monotonic() + 1),
            )
            if expected not in visible:
                if production_tui:
                    os.write(fd, enter)
                visible = _read_until(
                    fd, observed, (expected,), conversation_deadline
                )
            visible = _read_until(
                fd, observed, ("› Ask Estelle",), conversation_deadline
            )
            found = expected in visible
            outputs.append(expected if found else visible.strip())
            passed = passed and found
            if not found:
                break
        time.sleep(settle_seconds)
        after = _trace_line_count(http_trace)
        wire_unchanged = before is None or before == after
        return {
            "sent": [f"/{name}" for name in DROPPED_COMMANDS],
            "came_back": outputs,
            "processes_started": 1,
            "http_lines": {"before": before, "after": after},
            "pass": passed
            and len(outputs) == len(DROPPED_COMMANDS)
            and wire_unchanged,
        }
    finally:
        _terminate_pty(pid, fd)


def tui_surface_receipt(
    command: str, repo: str, timeout: float = 30
) -> dict[str, object]:
    assert command.startswith("/")
    return tui_turn_receipt(command, repo, timeout)


def _answer_contract(requests: list[dict]) -> bool:
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
    return answer is not None and not any(
        key in answer.get("body", {}) for key in ("instruction", "prompt")
    )


def _conversational_contract(requests: list[dict]) -> bool:
    matches = [
        request
        for request in requests
        if request.get("path") == "/deep-search"
        and request.get("body", {}).get("question") == CONVERSATIONAL_QUESTION
    ]
    return len(matches) == 1 and "working_memory" not in matches[0].get("body", {})


def _whole_lockfile_contract(requests: list[dict]) -> bool:
    return any(
        request.get("path") == "/scan"
        and any(
            file.get("path", "").endswith("package-lock.json")
            and len(file.get("content", "")) > 1_000
            for file in request.get("body", {}).get("files", [])
            if isinstance(file, dict)
        )
        for request in requests
    )


def _head_contract(requests: list[dict]) -> bool:
    routes = {
        request.get("path")
        for request in requests
        if request.get("path") in ("/sync", "/ingest/start", "/reindex")
        and len(request.get("body", {}).get("head", "")) == 40
        and all(
            character in "0123456789abcdef"
            for character in request.get("body", {}).get("head", "")
        )
    }
    return routes == {"/sync", "/ingest/start", "/reindex"}


def _hook_network_contract(requests: list[dict]) -> bool:
    checkpoint_events = {
        request.get("body", {}).get("client", {}).get("event")
        for request in requests
        if request.get("path") == "/checkpoint"
    }
    return (
        any(request.get("path") == "/verify" for request in requests)
        and any(
            request.get("path") == "/reindex"
            and "head" not in request.get("body", {})
            for request in requests
        )
        and checkpoint_events == {"Stop", "PreCompact", "SessionEnd"}
        and any(
            request.get("path") == "/search"
            and request.get("body", {}).get("query")
            == "Where is the application entry point?"
            for request in requests
        )
    )


def _skill_thread_contract(records: list[dict]) -> bool:
    successful = [
        record
        for record in records
        if record.get("request", {}).get("path") == "/skill/run"
        and 200 <= record.get("response", {}).get("status", 0) < 300
    ]
    if len(successful) != 2:
        return False
    first, second = successful
    first_body = first.get("request", {}).get("body", {})
    second_body = second.get("request", {}).get("body", {})
    first_reply = first.get("response", {}).get("body", {}).get("reply")
    expected_messages = [
        {"role": "user", "content": "State one risk in changing a CLI contract."},
        {"role": "assistant", "content": first_reply},
        {"role": "user", "content": "Challenge that answer."},
    ]
    return (
        first_body.get("skill") == "grill-me"
        and first_body.get("task") == expected_messages[0]["content"]
        and "messages" not in first_body
        and isinstance(first_reply, str)
        and bool(first_reply.strip())
        and second_body.get("skill") == "grill-me"
        and second_body.get("task") == expected_messages[2]["content"]
        and second_body.get("messages") == expected_messages
    )


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
    requests = [
        record.get("request", {})
        for record in records
        if 200 <= record.get("response", {}).get("status", 0) < 300
    ]
    separated = _answer_contract(requests)
    conversational = _conversational_contract(requests)
    deep = any(
        request.get("path") == "/gate" and request.get("body", {}).get("deep") is True
        for request in requests
    )
    whole_lockfile = _whole_lockfile_contract(requests)
    three_heads = _head_contract(requests)
    hook_network = _hook_network_contract(requests)
    skill_thread = _skill_thread_contract(records)
    proof = (
        f"grounded question data-only={separated}; deep review={deep}; "
        f"whole lockfile={whole_lockfile}; three head markers={three_heads}; "
        f"hook network rows={hook_network}; skill thread={skill_thread}; "
        f"conversational upload absent={conversational}"
    )
    return (
        {
            "sent": "inspect sanitized HTTP trace",
            "came_back": proof,
            "pass": separated
            and conversational
            and deep
            and whole_lockfile
            and three_heads
            and hook_network
            and skill_thread,
        },
        records,
    )


def run_receipts(
    expected_version: str, repo: str, timeout: float
) -> dict[str, object]:
    raw_path = os.environ.get("ESTELLE_RECEIPT_PATH")
    http_trace = Path(raw_path) if raw_path else None
    receipts = [
        installed_version_receipt(expected_version),
        repository_size_receipt(Path.cwd()),
        erasure_gate_receipt(),
        first_run_picker_receipt(),
        credential_retention_receipt(repo, timeout),
    ]
    receipts.extend(
        tui_surface_receipt(surface, repo, timeout) for surface in READ_SURFACES
    )
    receipts.append(tui_turn_receipt(GROUNDING_QUESTION, repo, timeout))
    receipts.append(tui_turn_receipt(CONVERSATIONAL_QUESTION, repo, timeout))
    receipts.append(tui_skill_thread_receipt(repo, timeout))
    receipts.append(
        dropped_command_receipt(
            repo,
            http_trace,
            max(timeout, 10),
            settle_seconds=2 if http_trace is not None else 0,
        )
    )
    receipts.extend(
        tui_surface_receipt(surface, repo, timeout) for surface in DIFF_SURFACES
    )
    receipts.extend(head_surface_receipts(max(timeout, 600)))
    receipts.extend(hook_event_receipts(Path.cwd(), max(timeout, 30)))
    http_records = None
    if http_trace is not None:
        http_receipt, http_records = http_contract_receipt(http_trace)
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
