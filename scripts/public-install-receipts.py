#!/usr/bin/env python3
"""Probe INSTALL through an exact installed public Estelle binary and production."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

HEALTH_URL = "https://api.fatelabs.ca/health"
EXPECTED_SURFACE = {"tools_base": 16, "prompts": 246}
BEGIN = "<!-- BEGIN ESTELLE — managed block, safe to move, do not edit inside -->"
END = "<!-- END ESTELLE -->"
QUESTION = re.compile(r"Proving question: What does `([A-Za-z_][A-Za-z0-9_]*)` do")
INTER_SURFACE_COOLDOWN_S = 31


def run(binary: Path, arguments: list[str], cwd: Path, timeout: int = 1_200) -> dict:
    command = [str(binary), *arguments]
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=os.environ.copy(),
        )
    except subprocess.TimeoutExpired as error:
        def output(value: str | bytes | None) -> str:
            if isinstance(value, bytes):
                return value.decode("utf-8", errors="replace")
            return value or ""

        return {
            "arguments": arguments,
            "returncode": 124,
            "stdout": output(error.stdout),
            "stderr": f"{output(error.stderr)}\ncommand timed out after {timeout}s".strip(),
        }
    return {
        "arguments": arguments,
        "returncode": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def read_health() -> dict:
    try:
        with urllib.request.urlopen(HEALTH_URL, timeout=10) as response:
            body = response.read(65_537)
        if len(body) > 65_536:
            return {"error": "health response exceeded 65536 bytes"}
        value = json.loads(body)
        return value if isinstance(value, dict) else {"error": "health was not an object"}
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return {"error": type(error).__name__}


def verified_health(value: dict) -> bool:
    return (
        isinstance(value.get("build"), str)
        and bool(value["build"])
        and value.get("build_verified") is True
        and value.get("surface") == EXPECTED_SURFACE
    )


def build_window(before: dict, after: dict) -> dict:
    stable = (
        verified_health(before)
        and verified_health(after)
        and before.get("build") == after.get("build")
    )
    return {
        "before": before.get("build"),
        "after": after.get("build"),
        "stable_and_verified": stable,
        "discarded": not stable,
    }


def command_reason(result: dict) -> str:
    """Keep a bounded, actionable tail without serialising an entire command transcript."""
    output = "\n".join(
        value.strip()
        for value in (result.get("stdout", ""), result.get("stderr", ""))
        if isinstance(value, str) and value.strip()
    )
    return output[-1_000:]


def receipt_plan(scope: str) -> dict[str, bool]:
    return {"brief": scope == "all", "setup": scope == "all", "mixed": True}


def trace_summary(path: Path, required: set[str]) -> tuple[bool, dict]:
    records = [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    statuses: dict[str, list[int]] = {}
    errors: dict[str, list[dict]] = {}
    terminal_ingest = False
    for record in records:
        request = record.get("request", {})
        response = record.get("response", {})
        route = request.get("path")
        status = response.get("status")
        if isinstance(route, str) and isinstance(status, int):
            statuses.setdefault(route, []).append(status)
            body = response.get("body", {})
            error = body.get("error", {}) if isinstance(body, dict) else {}
            if status >= 400 and isinstance(error, dict):
                safe = {"status": status}
                for field in ("code", "type"):
                    if isinstance(error.get(field), (int, str)):
                        safe[field] = error[field]
                if error.get("type") == "unsafe_ingest" and isinstance(error.get("message"), str):
                    safe["message"] = error["message"]
                errors.setdefault(route, []).append(safe)
        if (
            route == "/ingest/progress"
            and isinstance(status, int)
            and 200 <= status < 300
            and response.get("body", {}).get("state") == "done"
        ):
            terminal_ingest = True
    passed = all(
        route in statuses and any(200 <= status < 300 for status in statuses[route])
        for route in required
    )
    if "/ingest/progress" in required:
        passed = passed and terminal_ingest
    return passed, {
        "statuses": statuses,
        "errors": errors,
        "terminal_ingest": terminal_ingest,
    }


def sweep_trace_summary(path: Path) -> tuple[bool, dict]:
    _, summary = trace_summary(path, set())
    statuses = summary["statuses"]
    sync = any(200 <= status < 300 for status in statuses.get("/sync", []))
    background = (
        any(200 <= status < 300 for status in statuses.get("/ingest/start", []))
        and summary["terminal_ingest"]
    )
    if sync:
        transport = "sync"
    elif background:
        transport = "background-terminal"
    elif "/ingest/start" in statuses:
        transport = "background-incomplete"
    else:
        transport = "absent"
    summary["sweep_transport"] = transport
    return sync or background, summary


def setup_trace_summary(path: Path) -> tuple[bool, dict]:
    common, summary = trace_summary(path, {"/mcp", "/v1/chat/completions"})
    statuses = summary["statuses"]
    sync = any(200 <= status < 300 for status in statuses.get("/sync", []))
    background = (
        any(200 <= status < 300 for status in statuses.get("/ingest/start", []))
        and summary["terminal_ingest"]
    )
    if sync:
        transport = "sync"
    elif background:
        transport = "background-terminal"
    elif "/ingest/start" in statuses:
        transport = "background-incomplete"
    else:
        transport = "absent"
    summary["sweep_transport"] = transport
    return common and (sync or background), summary


def symbol_is_tracked(repo: Path, symbol: str, globs: list[str]) -> bool:
    completed = subprocess.run(
        ["git", "grep", "-l", "-w", "--", symbol, *globs],
        cwd=repo,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return completed.returncode == 0 and bool(completed.stdout.strip())


def repository_identity(repo: Path) -> dict:
    def git(*arguments: str) -> str:
        return subprocess.run(
            ["git", *arguments],
            cwd=repo,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        ).stdout.strip()

    tracked = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=repo,
        check=True,
        capture_output=True,
        timeout=30,
    ).stdout.split(b"\0")
    return {
        "remote": git("remote", "get-url", "origin"),
        "head": git("rev-parse", "HEAD"),
        "tracked_typescript": sum(path.endswith((b".ts", b".tsx")) for path in tracked),
        "tracked_go": sum(path.endswith(b".go") for path in tracked),
    }


def brief_receipt(binary: Path, repo: Path, customer_line: str) -> dict:
    instruction = repo / "CLAUDE.md"
    if instruction.exists():
        return {"pass": False, "came_back": "fixture already had CLAUDE.md; refused overwrite"}
    original = f"# Customer rules\n\n{customer_line}\n"
    instruction.write_text(original, encoding="utf-8")
    first = run(binary, ["brief"], repo)
    after_first = instruction.read_bytes()
    first_hash = hashlib.sha256(after_first).hexdigest()
    second = run(binary, ["brief"], repo)
    after_second = instruction.read_bytes()
    malformed = repo / "BROKEN.md"
    broken_bytes = f"{BEGIN}\nunfinished\n# customer negative control\n".encode()
    malformed.write_bytes(broken_bytes)
    negative = run(binary, ["brief", "--file", "BROKEN.md"], repo)
    passed = all(
        [
            first["returncode"] == 0,
            second["returncode"] == 0,
            customer_line.encode() in after_first,
            BEGIN.encode() in after_first,
            END.encode() in after_first,
            after_first == after_second,
            "already current; nothing written" in second["stdout"],
            negative["returncode"] != 0,
            malformed.read_bytes() == broken_bytes,
            "without its closing marker" in negative["stdout"],
        ]
    )
    return {
        "sent": "brief twice, then unmatched managed-marker negative control",
        "came_back": {
            "first_hash": first_hash,
            "second_hash": hashlib.sha256(after_second).hexdigest(),
            "customer_bytes_preserved": customer_line.encode() in after_first,
            "second_run": second["stdout"].strip(),
            "negative_refused": negative["returncode"] != 0,
            "negative_bytes_preserved": malformed.read_bytes() == broken_bytes,
        },
        "pass": passed,
    }


def setup_receipt(
    binary: Path, repo: Path, language: str, trace: Path, customer_line: str
) -> dict:
    before_health = read_health()
    trace.unlink(missing_ok=True)
    instruction = repo / "CLAUDE.md"
    before = instruction.read_bytes()
    result = run(binary, ["setup"], repo)
    after = instruction.read_bytes()
    match = QUESTION.search(result["stdout"])
    symbol = match.group(1) if match else None
    globs = ["*.go"] if language == "Go" else ["*.ts", "*.tsx"]
    tracked = bool(symbol) and symbol_is_tracked(repo, symbol, globs)
    try:
        remote, statuses = setup_trace_summary(trace)
    except (OSError, json.JSONDecodeError) as error:
        remote, statuses = False, {"trace_error": [type(error).__name__]}
    production = build_window(before_health, read_health())
    return {
        "sent": f"installed public binary setup on real {language} repository",
        "came_back": {
            "returncode": result["returncode"],
            "reason": command_reason(result) if result["returncode"] != 0 else "",
            "question_symbol": symbol,
            "symbol_in_tracked_source": tracked,
            "agent_block_present": BEGIN.encode() in after and END.encode() in after,
            "customer_bytes_preserved": customer_line.encode() in after,
            "instruction_file_unchanged": before == after,
            "second_write_refused": "already current; nothing written" in result["stdout"],
            "remote_statuses": statuses,
            "production_build": production,
        },
        "discarded": production["discarded"],
        "pass": (
            result["returncode"] == 0
            and tracked
            and remote
            and before == after
            and customer_line.encode() in after
            and BEGIN.encode() in after
            and END.encode() in after
            and "already current; nothing written" in result["stdout"]
            and production["stable_and_verified"]
        ),
    }


def mixed_receipt(binary: Path, repo: Path, trace: Path) -> dict:
    before_health = read_health()
    doctor = run(binary, ["doctor"], repo, timeout=120)
    trace.unlink(missing_ok=True)
    sweep = run(binary, ["sweep"], repo)
    try:
        remote, statuses = sweep_trace_summary(trace)
    except (OSError, json.JSONDecodeError) as error:
        remote, statuses = False, {"trace_error": [type(error).__name__]}
    production = build_window(before_health, read_health())
    with tempfile.TemporaryDirectory(prefix="estelle-doctor-control-") as temporary:
        control = Path(temporary)
        subprocess.run(["git", "init", "--quiet"], cwd=control, check=True)
        (control / "worker.ts").write_text(
            "export class RetryScheduler {}\n", encoding="utf-8"
        )
        (control / "worker.go").write_text(
            "package worker\nfunc DispatchEnvelope() {}\n", encoding="utf-8"
        )
        healthy = run(binary, ["doctor"], control, timeout=120)
        (control / "worker.go").write_bytes(b"x" * 400_001)
        failed = run(binary, ["doctor"], control, timeout=120)
    positive = all(
        marker in doctor["stdout"]
        for marker in [
            "Repository TypeScript ingest preflight  PARTIAL",
            "Repository Go ingest preflight  PARTIAL",
            "server index/runtime not proven",
        ]
    )
    matched_control = all(
        [
            "Repository TypeScript ingest preflight  ready · 1/1" in healthy["stdout"],
            "Repository Go ingest preflight  ready · 1/1" in healthy["stdout"],
            "Repository TypeScript ingest preflight  ready · 1/1" in failed["stdout"],
            "Repository Go ingest preflight  FAIL · 0/1" in failed["stdout"],
        ]
    )
    return {
        "sent": "real mixed TypeScript+Go sweep plus one-side-failed doctor control",
        "came_back": {
            "doctor_rows": [
                line for line in doctor["stdout"].splitlines() if "ingest preflight" in line
            ],
            "sweep_returncode": sweep["returncode"],
            "sweep_reason": command_reason(sweep),
            "remote_statuses": statuses,
            "matched_negative_control": matched_control,
            "production_build": production,
        },
        "discarded": production["discarded"],
        "pass": doctor["returncode"] == 0
        and sweep["returncode"] == 0
        and positive
        and matched_control
        and remote
        and production["stable_and_verified"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--typescript", type=Path, required=True)
    parser.add_argument("--go", type=Path, required=True)
    parser.add_argument("--mixed", type=Path, required=True)
    parser.add_argument("--trace-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scope", choices=("all", "mixed"), default="all")
    args = parser.parse_args()
    args.trace_dir.mkdir(parents=True, exist_ok=True)
    version = run(args.binary, ["--version"], args.typescript, timeout=30)
    before = read_health()
    plan = receipt_plan(args.scope)
    receipts = []
    if plan["brief"]:
        receipts.extend([
            brief_receipt(args.binary, args.typescript, "Keep cobalt customer content."),
            brief_receipt(args.binary, args.go, "Keep amber customer content."),
        ])
    if plan["setup"]:
        setup_repositories = [
            (args.typescript, "TypeScript", "Keep cobalt customer content."),
            (args.go, "Go", "Keep amber customer content."),
        ]
        for repo, language, customer_line in setup_repositories:
            trace = args.trace_dir / f"setup-{language.lower()}.jsonl"
            os.environ["ESTELLE_RECEIPT_PATH"] = str(trace)
            receipts.append(
                setup_receipt(args.binary, repo, language, trace, customer_line)
            )
            time.sleep(INTER_SURFACE_COOLDOWN_S)
    mixed_trace = args.trace_dir / "mixed.jsonl"
    os.environ["ESTELLE_RECEIPT_PATH"] = str(mixed_trace)
    receipts.append(mixed_receipt(args.binary, args.mixed, mixed_trace))
    after = read_health()
    expected = f"estelle {args.expected_version.removeprefix('v')}"
    build_stable = (
        verified_health(before)
        and verified_health(after)
        and before.get("build") == after.get("build")
    )
    report = {
        "scope": args.scope,
        "version": version["stdout"].strip(),
        "expected_version": expected,
        "repositories": {
            "typescript": repository_identity(args.typescript),
            "go": repository_identity(args.go),
            "mixed": repository_identity(args.mixed),
        },
        "production": {
            "before": before.get("build"),
            "after": after.get("build"),
            "stable_and_verified": build_stable,
        },
        "receipts": receipts,
    }
    report["summary"] = {
        "passed": sum(receipt.get("pass") is True for receipt in receipts),
        "failed": sum(
            receipt.get("pass") is not True and receipt.get("discarded") is not True
            for receipt in receipts
        ),
        "discarded": sum(receipt.get("discarded") is True for receipt in receipts),
    }
    report["pass"] = (
        version["returncode"] == 0
        and report["version"] == expected
        and build_stable
        and all(receipt.get("pass") is True for receipt in receipts)
    )
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"pass": report["pass"], "receipts": len(receipts)}))
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
