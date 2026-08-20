#!/usr/bin/env python3
"""Drive customer-visible surfaces through the installed public Estelle binary."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import pty
import select
import shutil
import signal
import sqlite3
import struct
import subprocess
import sys
import termios
import tempfile
import time
import urllib.request
from collections.abc import Callable
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

READ_SURFACE_HTTP_ROUTES = {
    "/init": "/wiki",
    "/graph": "/graph",
    "/graph nodes": "/graph/nodes",
    "/me": "/me",
    "/keys": "/me/keys",
    "/team": "/me/team",
    "/team board": "/team/leaderboard",
    "/cards": "/memory/cards",
    "/entities": "/entities",
    "/usage": "/usage",
    "/activity": "/activity",
    "/runs": "/runs",
    "/outcomes": "/outcomes",
    "/memories": "/memories",
    "/analytics": "/analytics",
    "/audit": "/audit",
    "/requests": "/requests",
    "/presence": "/presence",
    "/leaderboard": "/leaderboard",
    "/marketplace": "/marketplace",
    "/automations": "/automations",
    "/suites": "/suites",
    "/billing": "/settings",
    "/sessions": "/sessions",
    "Which file defines an application entry point in this repository?": "/deep-search",
}

READ_SURFACE_FIELD_TYPES = {
    "/keys": (("keys", list),),
    "/cards": (("cards", list), ("folders", dict)),
    "/entities": (("entities", list),),
    "/usage": (("series", list),),
    "/activity": (("by_endpoint", list),),
    "/runs": (("runs", list),),
    "/audit": (("entries", list),),
    "/requests": (("requests", list),),
    "/leaderboard": (("leaderboard", list),),
    "/marketplace": (("plugins", list),),
    "/automations": (("automations", list), ("active", bool)),
    "/suites": (("suites", list),),
    "/sessions": (("sessions", list),),
}

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
SESSION_RESUME_TITLE = "Receipt parser context"
SESSION_RESUME_PRIOR_QUESTION = "Keep the cobalt owl marker"
SESSION_RESUME_PRIOR_ANSWER = "The cobalt owl marker is retained"
SESSION_RESUME_QUESTION = "Which file defines an application entry point in this repository?"
DIFF_SURFACES = ("/review", "/scan")
SMALL_SWEEP_PATH = "rag_tutorials/multimodal_agentic_rag/frontend"
TUI_PASTE_SETTLE_SECONDS = 0.2
PRODUCTION_HEALTH_URL = "https://api.fatelabs.ca/health"
EXPECTED_PRODUCTION_SURFACE = {"tools_base": 16, "prompts": 246}
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

PASSIVE_TUI_ROUTES = frozenset(
    {
        "/account",
        "/overview",
        "/repos",
        "/settings/suite",
        "/autonomy/scope",
        "/issues",
        "/monitor/overview",
        "/agent/health",
        "/github/status",
        "/prs",
    }
)


def production_build_receipt(before: dict, after: dict) -> dict[str, object]:
    """Fail the aggregate receipt when production changes beneath the run."""
    before_build = before.get("build")
    after_build = after.get("build")
    stable = before_build == after_build
    health_contract = all(
        identity.get("build_verified") is True
        and identity.get("surface") == EXPECTED_PRODUCTION_SURFACE
        and isinstance(identity.get("build"), str)
        and bool(identity["build"])
        for identity in (before, after)
    )
    if not stable:
        detail = f"production build changed: {before_build} -> {after_build}"
    elif not health_contract:
        detail = "production identity failed the health contract"
    else:
        detail = f"production build stayed {before_build}"
    return {
        "sent": "pin production build for the entire receipt run",
        "came_back": detail,
        "before": before_build,
        "after": after_build,
        "pass": stable and health_contract,
    }


def _verified_production_identity(identity: dict) -> bool:
    return (
        identity.get("build_verified") is True
        and identity.get("surface") == EXPECTED_PRODUCTION_SURFACE
        and isinstance(identity.get("build"), str)
        and bool(identity["build"])
    )


def pin_surface_build(
    run: Callable[[], dict[str, object]],
    read_identity: Callable[[], dict],
) -> dict[str, object]:
    """Score one surface only when both health reads name one verified build."""
    before = read_identity()
    receipt = run()
    after = read_identity()
    before_build = before.get("build")
    after_build = after.get("build")
    crossed = before_build != after_build
    verified = _verified_production_identity(before) and _verified_production_identity(after)
    receipt["production_build"] = {
        "before": before_build,
        "after": after_build,
        "verified": verified,
    }
    receipt["discarded"] = crossed
    if crossed or not verified:
        receipt["pass"] = False
    return receipt


def receipt_summary(receipts: list[dict]) -> dict[str, int]:
    discarded = sum(receipt.get("discarded") is True for receipt in receipts)
    passed = sum(receipt.get("pass") is True for receipt in receipts)
    failed = len(receipts) - passed - discarded
    return {"passed": passed, "failed": failed, "discarded": discarded}


def pin_production_build(
    run: Callable[[], dict[str, object]],
    read_identity: Callable[[], dict],
) -> dict[str, object]:
    """Measure production identity on both sides of the complete receipt run."""
    before = read_identity()
    report = run()
    after = read_identity()
    receipts = report["receipts"]
    assert isinstance(receipts, list)
    receipts.append(production_build_receipt(before, after))
    report["summary"] = receipt_summary(receipts)
    return report


def read_production_identity(url: str) -> dict:
    """Read bounded public health metadata without credentials or response logging."""
    try:
        with urllib.request.urlopen(url, timeout=10) as response:
            payload = response.read(65_537)
        if len(payload) > 65_536:
            return {"error": "health response exceeded 65536 bytes"}
        parsed = json.loads(payload)
        return parsed if isinstance(parsed, dict) else {"error": "health response was not an object"}
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        return {"error": type(exc).__name__}


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


def write_opencode_history_fixture(
    home: Path,
    repository: Path,
    title: str,
    question: str,
    answer: str,
) -> Path:
    """Create the current OpenCode SQLite shape under a disposable receipt HOME."""
    data_home = home / ".local" / "share" / "opencode"
    data_home.mkdir(parents=True, exist_ok=True)
    database = data_home / "opencode.db"
    database.unlink(missing_ok=True)
    now_ms = time.time_ns() // 1_000_000
    connection = sqlite3.connect(database)
    try:
        connection.executescript(
            """
            CREATE TABLE session (
                id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT NOT NULL,
                time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL,
                seq INTEGER NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL
            );
            """
        )
        connection.execute(
            "INSERT INTO session (id, directory, title, time_created, time_updated) "
            "VALUES (?, ?, ?, ?, ?)",
            ("ses_estelle_receipt", str(repository.resolve()), title, now_ms, now_ms),
        )
        connection.executemany(
            "INSERT INTO session_message "
            "(id, session_id, type, seq, time_created, data) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (
                (
                    "msg_estelle_receipt_user",
                    "ses_estelle_receipt",
                    "user",
                    1,
                    now_ms,
                    json.dumps({"text": question, "files": [], "agents": []}),
                ),
                (
                    "msg_estelle_receipt_assistant",
                    "ses_estelle_receipt",
                    "assistant",
                    2,
                    now_ms + 1,
                    json.dumps(
                        {
                            "agent": "build",
                            "model": {"providerID": "receipt", "modelID": "receipt"},
                            "content": [
                                {"type": "text", "id": "txt_receipt", "text": answer}
                            ],
                        }
                    ),
                ),
            ),
        )
        connection.commit()
    finally:
        connection.close()
    return database


def write_claude_history_fixture(
    home: Path,
    repository: Path,
    title: str,
    question: str,
    answer: str,
) -> Path:
    source = home / ".claude" / "projects" / "receipt" / "session.jsonl"
    source.parent.mkdir(parents=True, exist_ok=True)
    timestamp = "2026-08-18T00:00:00Z"
    records = [
        {"type": "custom-title", "customTitle": title},
        {
            "type": "user",
            "cwd": str(repository.resolve()),
            "timestamp": timestamp,
            "message": {"content": question},
        },
        {
            "type": "assistant",
            "cwd": str(repository.resolve()),
            "timestamp": timestamp,
            "message": {"content": answer},
        },
    ]
    source.write_text(
        "".join(f"{json.dumps(record, separators=(',', ':'))}\n" for record in records),
        encoding="utf-8",
    )
    return source


def write_codex_history_fixture(
    home: Path,
    repository: Path,
    title: str,
    question: str,
    answer: str,
) -> Path:
    del title
    thread_id = "00000000-0000-4000-8000-000000000013"
    timestamp = "2026-08-18T00:00:00Z"
    source = (
        home
        / ".codex"
        / "sessions"
        / "2026"
        / "08"
        / "18"
        / f"rollout-2026-08-18T00-00-00-{thread_id}.jsonl"
    )
    source.parent.mkdir(parents=True, exist_ok=True)
    records = [
        {
            "timestamp": timestamp,
            "ordinal": 0,
            "type": "session_meta",
            "payload": {
                "session_id": thread_id,
                "id": thread_id,
                "timestamp": timestamp,
                "cwd": str(repository.resolve()),
                "originator": "public-receipt",
                "cli_version": "public-receipt",
                "source": "cli",
                "model_provider": "public-receipt",
                "history_mode": "paginated",
            },
        },
        {
            "timestamp": timestamp,
            "ordinal": 1,
            "type": "event_msg",
            "payload": {
                "type": "user_message",
                "message": question,
                "kind": "plain",
            },
        },
        {
            "timestamp": timestamp,
            "ordinal": 2,
            "type": "event_msg",
            "payload": {
                "type": "agent_message",
                "message": answer,
                "phase": None,
                "memory_citation": None,
            },
        },
    ]
    source.write_text(
        "".join(f"{json.dumps(record, separators=(',', ':'))}\n" for record in records),
        encoding="utf-8",
    )
    return source


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def session_resume_http_contract(record: dict, source_label: str = "OpenCode") -> bool:
    request = record.get("request", {})
    response = record.get("response", {})
    request_body = request.get("body", {})
    response_body = response.get("body", {})
    working_memory = request_body.get("working_memory", {})
    context = working_memory.get("session_context")
    expected_title = (
        SESSION_RESUME_PRIOR_QUESTION
        if source_label in ("Codex", "OpenCode")
        else SESSION_RESUME_TITLE
    )
    question = request_body.get("question")
    return (
        request.get("path") == "/deep-search"
        and _session_question_matches(question)
        and isinstance(context, str)
        and f"Imported {source_label} session: {expected_title}" in context
        and f"User: {SESSION_RESUME_PRIOR_QUESTION}" in context
        and f"Assistant: {SESSION_RESUME_PRIOR_ANSWER}" in context
        and response.get("status") == 200
        and isinstance(response_body.get("answer"), str)
        and bool(response_body["answer"].strip())
    )


def _session_question_matches(question: object) -> bool:
    return (
        isinstance(question, str)
        and question.rstrip("?") == SESSION_RESUME_QUESTION.rstrip("?")
    )


def _completed_session_record(records: list[dict], source_label: str) -> dict | None:
    return next(
        (
            record
            for record in records
            if session_resume_http_contract(record, source_label)
        ),
        None,
    )


def _session_trace_deadline(original_deadline: float, *, now: float | None = None) -> float:
    observed = time.monotonic() if now is None else now
    return max(original_deadline, observed + 10.0)


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
    if not output and result.returncode != 0:
        output = f"exited with code {result.returncode} and no stdout/stderr"
    receipt = {
        "sent": sent,
        "came_back": output,
        "exit_code": result.returncode,
        "pass": result.returncode == 0 and all(marker in output for marker in required),
    }
    if input_text is not None:
        receipt["stdin"] = input_text
    return receipt


def reindex_receipt(timeout: float) -> dict[str, object]:
    receipt = command_receipt(["estelle", "reindex"], (), timeout)
    output = str(receipt["came_back"])
    receipt["pass"] = receipt["exit_code"] == 0 and any(
        marker in output
        for marker in ("Memory current", "Estelle memory is already current")
    )
    return receipt


def head_surface_receipts(timeout: float = 600) -> list[dict[str, object]]:
    unsafe = command_receipt(["estelle", "sweep"], (), timeout)
    unsafe["pass"] = _unsafe_sweep_refusal_contract(unsafe)
    return [
        command_receipt(
            ["estelle", "sweep", "--path", SMALL_SWEEP_PATH], ("Repo swept",), timeout
        ),
        unsafe,
        reindex_receipt(timeout),
    ]


def _unsafe_sweep_refusal_contract(receipt: dict[str, object]) -> bool:
    output = str(receipt.get("came_back", ""))
    return (
        receipt.get("exit_code") not in (None, 0)
        and "HTTP 422 Unprocessable Entity" in output
        and "ingest refused:" in output
        and "possible hardcoded secrets" in output
        and "no files were stored" in output
        and "did not complete its requested operation" in output
    )


def _hook_specs(root: Path, transcript: Path) -> list[tuple[str, str, dict]]:
    common = {"cwd": str(root), "session_id": "public-receipt-session"}
    checkpoints = {
        **common,
        "transcript_path": str(transcript),
    }
    return [
        ("PreToolUse/ground", "ground", {**common, "hook_event_name": "PreToolUse", "tool_name": "Write", "tool_input": {"file_path": "receipt_probe.py", "content": "def receipt_probe():\n    return 1\n"}}),
        ("PreToolUse/guard", "guard", {**common, "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "chmod -R 777 /"}}),
        ("PostToolUse/shift", "shift", {**common, "hook_event_name": "PostToolUse", "tool_name": "Read", "tool_input": {"file_path": "README.md"}}),
        ("PostToolUse/sync", "sync", {**common, "hook_event_name": "PostToolUse", "tool_name": "Write", "tool_input": {"file_path": "README.md"}}),
        ("PostToolUse/distil", "distil", {**common, "hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_response": {"stdout": "tests/a.py::test_one PASSED\ntests/a.py::test_two PASSED\n"}}),
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
            ["estelle", "hook", mode, "--event", payload["hook_event_name"]],
            (),
            timeout,
            json.dumps(payload),
        )
        receipt["event"] = event
        receipts.append(receipt)
    malformed = command_receipt(
        ["estelle", "hook", "welcome", "--event", "SessionStart"],
        (),
        timeout,
        "{not json",
    )
    malformed["event"] = "SessionStart/welcome malformed-negative-control"
    malformed["pass"] = malformed["exit_code"] != 0 and all(
        marker in str(malformed["came_back"])
        for marker in (
            "event=SessionStart",
            "mode=welcome",
            "branch=input-json",
            "needed=valid JSON hook payload on stdin",
        )
    )
    receipts.append(malformed)
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


def _wait_for_process_line(
    process: subprocess.Popen[str], marker: str, deadline: float
) -> str:
    assert process.stdout is not None
    lines = []
    while time.monotonic() < deadline and process.poll() is None:
        ready, _, _ = select.select([process.stdout], [], [], 0.1)
        if not ready:
            continue
        line = process.stdout.readline()
        if not line:
            break
        lines.append(line.rstrip())
        if marker in line:
            break
    return "\n".join(lines)


def _probe_imported_source(
    source_name: str,
    source_label: str,
    source_path: Path,
    repo: str,
    repository_root: Path,
    socket: Path,
    environment: dict[str, str],
    trace_path: Path,
    timeout: float,
) -> dict[str, object]:
    source_before = _sha256_file(source_path)
    trace_offset = _trace_line_count(trace_path) or 0
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(repository_root)
        os.execvpe(
            "estelle",
            [
                "estelle",
                "--repo",
                repo,
                "connect",
                "--socket",
                str(socket),
                "--session",
                f"receipt-{source_name}",
                "--from",
                source_name,
            ],
            environment,
        )
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    visible = ""
    http_record = None
    try:
        deadline = time.monotonic() + timeout
        visible = _read_until(
            fd,
            observed,
            (SESSION_RESUME_PRIOR_ANSWER,),
            deadline,
        )
        imported_visible = (
            SESSION_RESUME_PRIOR_QUESTION in visible
            and SESSION_RESUME_PRIOR_ANSWER in visible
        )
        if imported_visible:
            os.write(fd, SESSION_RESUME_QUESTION.encode())
            time.sleep(TUI_PASTE_SETTLE_SECONDS)
            os.write(fd, b"\r")
            visible = _read_until(
                fd,
                observed,
                (f"you  {SESSION_RESUME_QUESTION}",),
                deadline,
            )
            visible = _read_until(fd, observed, ("› Ask Estelle",), deadline)
        trace_deadline = _session_trace_deadline(deadline)
        while time.monotonic() < trace_deadline and http_record is None:
            http_record = _completed_session_record(
                _read_http_records_after(trace_path, trace_offset), source_label
            )
            if http_record is None:
                time.sleep(0.05)
        source_after = _sha256_file(source_path)
        source_unchanged = source_after == source_before
        http_pass = isinstance(http_record, dict) and session_resume_http_contract(
            http_record, source_label
        )
        return {
            "source": source_label,
            "came_back": visible.strip(),
            "source_sha256_before": source_before,
            "source_sha256_after": source_after,
            "source_unchanged": source_unchanged,
            "http_route": {"path": "/deep-search", "contract": http_pass},
            "pass": imported_visible and source_unchanged and http_pass,
        }
    finally:
        _terminate_pty(pid, fd)


def session_resume_receipt(
    repo: str,
    repository_root: Path,
    timeout: float = 30,
) -> dict[str, object]:
    """Probe OpenCode history import through installed serve/connect and production HTTP."""
    raw_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
    with tempfile.TemporaryDirectory(prefix="estelle-session-resume-") as raw_home:
        home = Path(raw_home)
        trace_path = Path(raw_trace) if raw_trace else home / "session-resume-http.jsonl"
        other_repository = home / "different-repository"
        other_repository.mkdir()
        environment = os.environ.copy()
        environment["HOME"] = str(home)
        environment["ESTELLE_RECEIPT_PATH"] = str(trace_path)
        socket = home / ".estelle" / "session.sock"
        negative_database = write_opencode_history_fixture(
            home,
            other_repository,
            SESSION_RESUME_TITLE,
            SESSION_RESUME_PRIOR_QUESTION,
            SESSION_RESUME_PRIOR_ANSWER,
        )
        negative_before = _sha256_file(negative_database)
        negative = subprocess.run(
            [
                "estelle",
                "--repo",
                repo,
                "connect",
                "--socket",
                str(socket),
                "--session",
                "receipt-negative",
                "--from",
                "opencode",
            ],
            cwd=repository_root,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        negative_output = "\n".join(
            part.strip() for part in (negative.stdout, negative.stderr) if part.strip()
        )
        negative_unchanged = _sha256_file(negative_database) == negative_before
        negative_pass = (
            negative.returncode != 0
            and "no recent history matched the current repository" in negative_output
            and negative_unchanged
        )

        sources = [
            (
                "codex",
                "Codex",
                write_codex_history_fixture(
                    home,
                    repository_root,
                    SESSION_RESUME_TITLE,
                    SESSION_RESUME_PRIOR_QUESTION,
                    SESSION_RESUME_PRIOR_ANSWER,
                ),
            ),
            (
                "claude-code",
                "Claude Code",
                write_claude_history_fixture(
                    home,
                    repository_root,
                    SESSION_RESUME_TITLE,
                    SESSION_RESUME_PRIOR_QUESTION,
                    SESSION_RESUME_PRIOR_ANSWER,
                ),
            ),
            (
                "opencode",
                "OpenCode",
                write_opencode_history_fixture(
                    home,
                    repository_root,
                    SESSION_RESUME_TITLE,
                    SESSION_RESUME_PRIOR_QUESTION,
                    SESSION_RESUME_PRIOR_ANSWER,
                ),
            ),
        ]
        server: subprocess.Popen[str] | None = None
        server_output = ""
        try:
            server = subprocess.Popen(
                ["estelle", "serve", "--socket", str(socket)],
                cwd=repository_root,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            server_output = _wait_for_process_line(
                server,
                "Estelle session server listening at",
                time.monotonic() + timeout,
            )
            if "Estelle session server listening at" not in server_output:
                return {
                    "sent": "estelle connect --from codex|claude-code|opencode",
                    "came_back": server_output or "session server did not become ready",
                    "negative_control": negative_pass,
                    "pass": False,
                }
            source_receipts = [
                _probe_imported_source(
                    source_name,
                    source_label,
                    source_path,
                    repo,
                    repository_root,
                    socket,
                    environment,
                    trace_path,
                    timeout,
                )
                for source_name, source_label, source_path in sources
            ]
            return {
                "sent": "estelle connect --from codex|claude-code|opencode",
                "came_back": {
                    receipt["source"]: receipt["came_back"] for receipt in source_receipts
                },
                "sources": source_receipts,
                "negative_control": {
                    "different_repository_rejected": negative_pass,
                    "source_unchanged": negative_unchanged,
                },
                "pass": negative_pass
                and all(receipt["pass"] is True for receipt in source_receipts),
            }
        finally:
            if server is not None and server.poll() is None:
                server.terminate()
                try:
                    server.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(timeout=2)


def _read_http_records_after(path: Path, line_offset: int) -> list[dict]:
    if not path.exists():
        return []
    records = []
    for line in path.read_text(encoding="utf-8").splitlines()[line_offset:]:
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(record, dict):
            records.append(record)
    return records


def _outcomes_http_contract(body: dict) -> bool:
    counts = [body.get(key) for key in ("total", "accepted", "reverted", "rejected")]
    if not all(
        isinstance(value, int) and not isinstance(value, bool) and value >= 0
        for value in counts
    ):
        return False
    total, accepted, reverted, rejected = counts
    if total != accepted + reverted + rejected:
        return False
    rates = [body.get(key) for key in ("accept_rate", "revert_rate")]
    if not all(
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and 0 <= value <= 1
        for value in rates
    ):
        return False
    expected_accept = accepted / total if total else 0.0
    expected_revert = reverted / total if total else 0.0
    return (
        abs(rates[0] - expected_accept) <= 0.001
        and abs(rates[1] - expected_revert) <= 0.001
    )


def _grounded_question_http_contract(record: dict, response_body: dict) -> bool:
    request_body = record.get("request", {}).get("body", {})
    sources = response_body.get("sources")
    return (
        isinstance(request_body, dict)
        and request_body.get("question") == GROUNDING_QUESTION
        and isinstance(request_body.get("working_memory"), dict)
        and not any(key in request_body for key in ("instruction", "prompt"))
        and response_body.get("grounded") is True
        and isinstance(response_body.get("answer"), str)
        and bool(response_body["answer"].strip())
        and isinstance(sources, list)
        and bool(sources)
        and all(isinstance(source, dict) and source.get("file") for source in sources)
    )


def _nonnegative_int(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _typed_fields(body: dict, fields: tuple) -> bool:
    return all(key in body and isinstance(body[key], expected) for key, expected in fields)


def _named_session_breakdown(value: object) -> bool:
    return isinstance(value, list) and all(
        isinstance(row, dict)
        and isinstance(row.get("name"), str)
        and bool(row["name"].strip())
        and _nonnegative_int(row.get("sessions"))
        for row in value
    )


def _read_surface_body_contract(command: str, body: dict) -> bool:
    if command == "/init":
        return all(isinstance(body.get(key), str) and body[key].strip() for key in ("repo", "wiki"))
    if command == "/graph":
        return (
            isinstance(body.get("repo"), str)
            and bool(body["repo"].strip())
            and _nonnegative_int(body.get("files"))
            and body["files"] > 0
        )
    if command == "/graph nodes":
        return (
            _read_surface_body_contract("/graph", body)
            and _typed_fields(body, (("nodes", list), ("edges", list), ("truncated", bool)))
            and bool(body["nodes"])
        )
    if command == "/me":
        return _typed_fields(body, (("email", str), ("plan", str), ("plan_active", bool))) and bool(
            body["email"].strip() and body["plan"].strip()
        )
    if command == "/team":
        return "team" in body and (body["team"] is None or isinstance(body["team"], dict))
    if command == "/team board":
        # A caller outside a team has an honest, useful empty state. Require the explicit `team: null`
        # discriminator so `{leaderboard: []}` cannot pass merely because the account was never enrolled.
        if "team" in body and body["team"] is None:
            return body.get("leaderboard") == []
        return _typed_fields(body, (("leaderboard", list), ("window", str), ("metric", str)))
    if command == "/outcomes":
        return _outcomes_http_contract(body)
    if command == "/memories":
        return (
            isinstance(body.get("repo"), str)
            and bool(body["repo"].strip())
            and isinstance(body.get("memories"), list)
            and bool(body["memories"])
        )
    if command == "/analytics":
        counts = (body.get("runs"), body.get("sessions"), body.get("turns"))
        maps = ((key, dict) for key in ("outcomes", "events"))
        return (
            all(_nonnegative_int(value) for value in counts)
            and _named_session_breakdown(body.get("repos"))
            and _named_session_breakdown(body.get("skills"))
            and _typed_fields(body, tuple(maps))
        )
    if command == "/presence":
        return _typed_fields(
            body,
            tuple((key, list) for key in ("active", "overnight", "files_in_use", "handoffs")),
        )
    if command == "/billing":
        envelope = (("settings", dict), ("catalog", list), ("pricing", dict))
        pricing = (("total_monthly_usd", (int, float)), ("breakdown", list))
        return _typed_fields(body, envelope) and _typed_fields(
            body["pricing"], pricing
        )
    fields = READ_SURFACE_FIELD_TYPES.get(command, ())
    return command in READ_SURFACE_FIELD_TYPES and _typed_fields(body, fields)


def _surface_http_contract(command: str, record: dict) -> bool:
    expected_path = READ_SURFACE_HTTP_ROUTES.get(command)
    if expected_path is None or record.get("request", {}).get("path") != expected_path:
        return False
    response = record.get("response", {})
    status = response.get("status")
    body = response.get("body", {})
    if not isinstance(status, int) or not 200 <= status < 300 or not isinstance(body, dict):
        return False
    if command == GROUNDING_QUESTION:
        return _grounded_question_http_contract(record, body)
    return _read_surface_body_contract(command, body)


def _wait_for_surface_http_receipt(
    command: str,
    path: Path,
    line_offset: int,
    deadline: float,
) -> dict[str, object]:
    expected_path = READ_SURFACE_HTTP_ROUTES[command]
    while time.monotonic() < deadline:
        for record in _read_http_records_after(path, line_offset):
            request = record.get("request", {})
            if request.get("path") != expected_path:
                continue
            response = record.get("response", {})
            return {
                "path": expected_path,
                "status": response.get("status", "not observed"),
                "contract": _surface_http_contract(command, record),
            }
        time.sleep(0.05)
    return {"path": expected_path, "status": "not observed", "contract": False}


def _wait_for_active_http_receipt(
    path: Path,
    line_offset: int,
    deadline: float,
) -> dict[str, object]:
    while time.monotonic() < deadline:
        for record in _read_http_records_after(path, line_offset):
            request_path = record.get("request", {}).get("path")
            if request_path in PASSIVE_TUI_ROUTES:
                continue
            if isinstance(request_path, str):
                return {
                    "path": request_path,
                    "status": record.get("response", {}).get("status", "not observed"),
                }
        time.sleep(0.05)
    return {"path": "not observed", "status": "not observed"}


def tui_turn_receipt(
    turn: str,
    repo: str,
    timeout: float = 30,
    wait_for_active_http: bool = False,
) -> dict[str, object]:
    assert turn.strip() == turn and turn
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp("estelle", ["estelle", "--repo", repo])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    visible = ""
    raw_trace = os.environ.get("ESTELLE_RECEIPT_PATH")
    expected_http_path = READ_SURFACE_HTTP_ROUTES.get(turn)
    http_trace = (
        Path(raw_trace)
        if raw_trace and (expected_http_path is not None or wait_for_active_http)
        else None
    )
    trace_offset = _trace_line_count(http_trace) or 0
    try:
        ready_deadline = time.monotonic() + timeout
        visible = _read_until(fd, observed, ("Ask Estelle",), ready_deadline)
        if "Ask Estelle" in visible:
            time.sleep(0.25)
            submitted = f"{turn} " if turn.startswith("/") else turn
            os.write(fd, submitted.encode())
            # The inherited composer suppresses Enter for 120 ms after a paste burst.
            # Cross that boundary deliberately instead of depending on runner scheduling.
            time.sleep(TUI_PASTE_SETTLE_SECONDS)
            os.write(fd, b"\r")
            visible = _read_until(fd, observed, (f"you  {turn}",), ready_deadline)
            visible = _read_until(fd, observed, ("› Ask Estelle",), ready_deadline)
        http_route = None
        if http_trace is not None:
            if expected_http_path is not None:
                http_route = _wait_for_surface_http_receipt(
                    turn, http_trace, trace_offset, ready_deadline
                )
            else:
                http_route = _wait_for_active_http_receipt(
                    http_trace, trace_offset, ready_deadline
                )
            visible = _read_until(
                fd, observed, ("receipt-output-drained",), time.monotonic() + 0.25
            )
        passed = (
            f"you  {turn}" in visible
            and "› Ask Estelle" in visible
            and not any(marker in visible for marker in FAILURE_MARKERS)
            and (
                http_route is None
                or "contract" not in http_route
                or http_route["contract"] is True
            )
        )
        receipt = {"sent": turn, "came_back": visible.strip(), "pass": passed}
        if http_route is not None:
            receipt["http_route"] = http_route
        return receipt
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
            time.sleep(TUI_PASTE_SETTLE_SECONDS)
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


def _unexpected_non_passive_paths(records: list[dict]) -> list[str]:
    paths = {
        record.get("request", {}).get("path")
        for record in records
        if isinstance(record.get("request"), dict)
    }
    return sorted(
        path
        for path in paths
        if isinstance(path, str) and path not in PASSIVE_TUI_ROUTES
    )


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
            time.sleep(TUI_PASTE_SETTLE_SECONDS)
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
        new_records = (
            _read_http_records_after(http_trace, before or 0)
            if http_trace is not None
            else []
        )
        unexpected_paths = _unexpected_non_passive_paths(new_records)
        wire_isolated = not unexpected_paths
        return {
            "sent": [f"/{name}" for name in DROPPED_COMMANDS],
            "came_back": outputs,
            "processes_started": 1,
            "http_lines": {"before": before, "after": after},
            "unexpected_http_paths": unexpected_paths,
            "pass": passed
            and len(outputs) == len(DROPPED_COMMANDS)
            and wire_isolated,
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


def _head_contract(records: list[dict]) -> bool:
    routes = set()
    for record in records:
        request = record.get("request", {})
        if not isinstance(request, dict):
            continue
        path = request.get("path")
        body = request.get("body")
        head = body.get("head", "") if isinstance(body, dict) else ""
        if (
            path in ("/sync", "/ingest/start", "/reindex")
            and len(head) == 40
            and all(character in "0123456789abcdef" for character in head)
            and _terminal_head_response(record)
        ):
            routes.add(path)
    return routes == {"/sync", "/ingest/start", "/reindex"}


def _terminal_head_response(record: dict) -> bool:
    request = record.get("request", {})
    response = record.get("response", {})
    status = response.get("status")
    if isinstance(status, int) and 200 <= status < 300:
        return True
    body = response.get("body", {})
    blocked = body.get("blocked") if isinstance(body, dict) else None
    return (
        request.get("path") == "/ingest/start"
        and status == 422
        and isinstance(body, dict)
        and isinstance(blocked, int)
        and not isinstance(blocked, bool)
        and blocked > 0
        and body.get("indexed") == 0
        and body.get("chunks") == 0
    )


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
    three_heads = _head_contract(records)
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
    expected_version: str,
    repo: str,
    timeout: float,
    read_identity: Callable[[], dict] | None = None,
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

    def surface_receipt(surface: str) -> dict[str, object]:
        run = lambda: tui_surface_receipt(surface, repo, timeout)
        return pin_surface_build(run, read_identity) if read_identity is not None else run()

    resume_run = lambda: session_resume_receipt(repo, Path.cwd(), timeout)
    receipts.append(
        pin_surface_build(resume_run, read_identity)
        if read_identity is not None
        else resume_run()
    )
    receipts.extend(surface_receipt(surface) for surface in READ_SURFACES)
    grounded_run = lambda: tui_turn_receipt(GROUNDING_QUESTION, repo, timeout)
    receipts.append(
        pin_surface_build(grounded_run, read_identity)
        if read_identity is not None
        else grounded_run()
    )
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
    receipts.extend(surface_receipt(surface) for surface in DIFF_SURFACES)
    receipts.extend(head_surface_receipts(max(timeout, 600)))
    receipts.extend(hook_event_receipts(Path.cwd(), max(timeout, 30)))
    http_records = None
    if http_trace is not None:
        http_receipt, http_records = http_contract_receipt(http_trace)
        receipts.append(http_receipt)
    report = {
        "expected_version": expected_version,
        "repo": repo,
        "receipts": receipts,
        "summary": receipt_summary(receipts),
    }
    if http_records is not None:
        report["http_contracts"] = http_records
    return report


def failed_receipt_diagnostics(report: dict[str, object]) -> list[str]:
    """Name failed contracts without copying production response bodies into logs."""
    diagnostics = []
    receipts = report.get("receipts", [])
    assert isinstance(receipts, list)
    for receipt in receipts:
        assert isinstance(receipt, dict)
        if receipt.get("pass") is True:
            continue
        label = str(receipt.get("event") or receipt.get("sent") or "unnamed receipt")
        came_back = str(receipt.get("came_back", ""))
        marker = next((item for item in FAILURE_MARKERS if item in came_back), None)
        if marker is not None:
            reason = marker.rstrip()
        elif receipt.get("exit_code") not in (None, 0):
            reason = f"exited with code {receipt['exit_code']}"
        elif came_back.startswith("timed out after "):
            reason = came_back
        elif receipt.get("sent") == "inspect sanitized HTTP trace":
            reason = came_back
        else:
            reason = "expected receipt contract not observed"
        diagnostics.append(f"{label}: {reason}")
    return diagnostics


def main(argv: list[str]) -> int:
    if argv == ["--list"]:
        print(json.dumps(READ_SURFACES))
        return 0
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--health-url", default=PRODUCTION_HEALTH_URL)
    args = parser.parse_args(argv)
    read_identity = lambda: read_production_identity(args.health_url)
    report = pin_production_build(
        lambda: run_receipts(
            args.expected_version,
            args.repo,
            args.timeout,
            read_identity=read_identity,
        ),
        read_identity,
    )
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    summary = report["summary"]
    print(
        f"public receipts: {summary['passed']} passed, "
        f"{summary['failed']} failed, {summary['discarded']} discarded"
    )
    for diagnostic in failed_receipt_diagnostics(report):
        print(f"public receipt failed: {diagnostic}")
    return 0 if summary["failed"] == 0 and summary["discarded"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
