#!/usr/bin/env python3
"""Drive a REAL estelle binary through login -> hook stream -> mcp-server against a local server.

Written for the Windows CI job, where none of these surfaces had ever run, and runnable on any host.

🔴 THE STREAM HOOK IS SILENT BY DESIGN, SO ITS EXIT CODE PROVES NOTHING. It is what makes a session
appear on the dashboard, and it exits 0 with no output whether it sent ten events or none, because a
hook that prints or fails would disturb the host. The innermost observable is the HTTP request, so
this stands up a 127.0.0.1 capture server, points the binary at it with `ESTELLE_API_URL` (plain http
is accepted only for loopback) and asserts on what ARRIVED:

  1. CONTROL — `hook stream` with a session id the hook must refuse (`safe_session` rejects spaces)
     sends ZERO requests. If it sent any, the server would be counting something other than the
     stream and arm 3 would mean nothing. Arms 1 and 3 differ in that one input.
  2. LOGIN — `estelle login` reads a key from stdin, calls the account route with it as the bearer,
     and leaves `<home>/.estelle/auth.json` behind (existence only; the file is never read).
  3. STREAM — `hook stream` with NO key in the environment POSTs `session/events` carrying this
     session's id and >= 1 event, bearing the key login stored. So this arm proves the credential
     store round-trips on this OS, not merely that an env var is forwarded.
  4. SWEEP — `estelle sweep` in a fresh one-file git repository POSTs `/sync` with that file's path
     in the body, again bearing the stored key.
  5. MCP — `estelle mcp-server` answers a stdio `initialize` with serverInfo.name == "estelle".
  6. INSTALL-HOOKS — `estelle install-hooks` writes `<home>/.claude/settings.json` whose
     UserPromptSubmit hooks include the bare `estelle hook stream --event UserPromptSubmit` command.

No real credential and no network: the key is a fixed dummy and the only listener is loopback. On
Unix HOME is a temporary directory so the real ~/.estelle is untouched; on Windows `dirs::home_dir`
reads the profile folder, not HOME, so state lands in the (ephemeral CI) runner profile and the
script REFUSES to run there if a credential already exists. Exit 0 = every arm held, 1 = one did not.
"""
from __future__ import annotations

import argparse
import datetime
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
import uuid
from pathlib import Path

DUMMY_KEY = "probe-not-a-real-key"
RUN_TIMEOUT_SECONDS = 120
MAX_BODY_BYTES = 8 * 1024 * 1024


def serve() -> tuple[http.server.ThreadingHTTPServer, list[dict]]:
    received: list[dict] = []

    class Capture(http.server.BaseHTTPRequestHandler):
        def _answer(self, status: int, body: dict | None) -> None:
            length = min(int(self.headers.get("Content-Length") or 0), MAX_BODY_BYTES)
            raw = self.rfile.read(length) if length else b""
            try:
                parsed = json.loads(raw) if raw else None
            except ValueError:
                parsed = None
            received.append({"method": self.command, "path": self.path,
                             "auth": self.headers.get("Authorization"), "body": parsed})
            payload = json.dumps(body or {}).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:  # noqa: N802 - the stdlib dispatches on this exact name
            self._answer(200, {})

        def do_GET(self) -> None:  # noqa: N802 - the stdlib dispatches on this exact name
            self._answer(200, {"account_id": "probe"})

        def log_message(self, *_args: object) -> None:
            return

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Capture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, received


class Probe:
    def __init__(self, binary: Path, repo: Path, work: Path, port: int) -> None:
        self.binary, self.repo, self.work = binary, repo, work
        self.env = {k: v for k, v in os.environ.items() if k not in {"ESTELLE_API_KEY", "ESTELLE_KEY"}}
        self.env["ESTELLE_API_URL"] = f"http://127.0.0.1:{port}"
        if os.name == "nt":
            self.home = Path(os.environ["USERPROFILE"])
        else:
            self.home = work / "home"
            self.home.mkdir()
            self.env["HOME"] = str(self.home)

    def run(self, args: list[str], stdin: str, cwd: Path | None = None) -> subprocess.CompletedProcess:
        return subprocess.run([str(self.binary), *args], input=stdin, text=True, capture_output=True,
                              cwd=cwd or self.repo, env=self.env, timeout=RUN_TIMEOUT_SECONDS)

    def sweep(self) -> subprocess.CompletedProcess:
        repo = self.work / "sweep-repo"
        repo.mkdir()
        (repo / "probe_module.py").write_text("def probe():\n    return 1\n", encoding="utf-8")
        for command in (["init", "-q"], ["remote", "add", "origin", "https://github.com/probe/probe.git"],
                        ["add", "probe_module.py"],
                        ["-c", "user.email=probe@example.invalid", "-c", "user.name=probe",
                         "commit", "-qm", "probe"]):
            subprocess.run(["git", *command], cwd=repo, check=True, capture_output=True,
                           timeout=RUN_TIMEOUT_SECONDS)
        return self.run(["sweep"], "", cwd=repo)

    def stream(self, session: str) -> subprocess.CompletedProcess:
        transcript = self.work / f"{uuid.uuid4()}.jsonl"
        now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")
        line = {"type": "user", "uuid": str(uuid.uuid4()), "timestamp": now, "sessionId": session,
                "message": {"role": "user", "content": "native surface probe"}}
        transcript.write_text(json.dumps(line) + "\n", encoding="utf-8")
        payload = {"hook_event_name": "UserPromptSubmit", "session_id": session, "cwd": str(self.repo),
                   "transcript_path": str(transcript), "prompt": "native surface probe"}
        return self.run(["hook", "stream"], json.dumps(payload))


def stderr_of(result: subprocess.CompletedProcess) -> str:
    return result.stderr.strip()[:400]


def check(probe: Probe, received: list[dict]) -> list[str]:
    failures: list[str] = []
    credential = probe.home / ".estelle" / "auth.json"
    settings = probe.home / ".claude" / "settings.json"
    for existing in (credential, settings):
        if existing.exists():
            return [f"refusing to run: {existing} already exists and this probe would overwrite it"]
    control = probe.stream("not a safe session")
    if control.returncode != 0 or received:
        failures.append(f"CONTROL: exit {control.returncode}, {len(received)} request(s); expected "
                        f"exit 0 and none. stderr={stderr_of(control)}")
    login = probe.run(["login"], DUMMY_KEY + "\n")
    asked = [r for r in received if r["auth"] == f"Bearer {DUMMY_KEY}"]
    if login.returncode != 0 or not credential.is_file() or not asked:
        failures.append(f"LOGIN: exit {login.returncode}, credential file present={credential.is_file()}, "
                        f"{len(asked)} request(s) bearing the key. stderr={stderr_of(login)}")
    before = len(received)
    session = f"probe-{uuid.uuid4()}"
    streamed = probe.stream(session)
    posts = [r for r in received[before:]
             if r["method"] == "POST" and r["path"].rstrip("/").endswith("session/events")
             and isinstance(r["body"], dict) and r["body"].get("session_id", "").endswith(session)
             and r["body"].get("events") and r["auth"] == f"Bearer {DUMMY_KEY}"]
    if streamed.returncode != 0 or not posts:
        failures.append(f"STREAM: exit {streamed.returncode}, {len(posts)} authenticated session/events "
                        f"POST(s) for {session}; saw {[r['path'] for r in received[before:]]}. "
                        f"stderr={stderr_of(streamed)}")
    before = len(received)
    swept = probe.sweep()
    syncs = [r for r in received[before:] if r["method"] == "POST" and r["path"].rstrip("/").endswith("/sync")
             and "probe_module.py" in json.dumps(r["body"]) and r["auth"] == f"Bearer {DUMMY_KEY}"]
    if swept.returncode != 0 or not syncs:
        failures.append(f"SWEEP: exit {swept.returncode}, {len(syncs)} authenticated /sync POST(s) naming "
                        f"probe_module.py; saw {[r['path'] for r in received[before:]]}. "
                        f"stderr={stderr_of(swept)}")
    initialize = {"jsonrpc": "2.0", "id": 1, "method": "initialize",
                  "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                             "clientInfo": {"name": "probe", "version": "0"}}}
    mcp = probe.run(["mcp-server"], json.dumps(initialize) + "\n")
    first = next((line for line in mcp.stdout.splitlines() if line.strip()), "")
    try:
        name = json.loads(first)["result"]["serverInfo"]["name"]
    except (ValueError, KeyError, TypeError):
        name = None
    if name != "estelle":
        failures.append(f"MCP: initialize answered {first[:200]!r}. stderr={stderr_of(mcp)}")
    installed = probe.run(["install-hooks"], "")
    try:
        groups = json.loads(settings.read_text(encoding="utf-8"))["hooks"]["UserPromptSubmit"]
        commands = [hook.get("command") for group in groups for hook in group.get("hooks", [])]
    except (OSError, ValueError, KeyError, TypeError):
        commands = []
    if installed.returncode != 0 or "estelle hook stream --event UserPromptSubmit" not in commands:
        failures.append(f"INSTALL-HOOKS: exit {installed.returncode}, UserPromptSubmit commands "
                        f"{commands}. stderr={stderr_of(installed)}")
    if not failures:
        events = sum(len(r["body"]["events"]) for r in posts)
        print(f"native surface probe ({probe.binary.name}): control sent 0 requests; login stored a "
              f"credential; stream POSTed {len(posts)}x session/events ({events} event(s)) with the "
              f"stored key; sweep POSTed /sync naming the file; mcp-server initialize answered "
              f"serverInfo.name=estelle; install-hooks wrote `estelle hook stream`")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--repo", default=Path.cwd(), type=Path)
    args = parser.parse_args()
    server, received = serve()
    try:
        with tempfile.TemporaryDirectory() as directory:
            probe = Probe(args.binary.resolve(), args.repo.resolve(), Path(directory), server.server_address[1])
            failures = check(probe, received)
    finally:
        server.shutdown()
    for failure in failures:
        print(f"native surface probe: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
