#!/usr/bin/env python3
"""Probe five installed-binary session views through the real owner socket.

The proxy records only the local, credential-free session protocol. It never logs the
``ESTELLE_API_KEY`` consumed by ``estelle serve``. A close-view pass must emit ``switch`` and no
``cancel``; the paired negative sends Ctrl+C and must emit a real ``cancel`` frame.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import pty
import re
import signal
import struct
import subprocess
import termios
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path


ANSI = re.compile(rb"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\)|[()][0-2A-Z])")
QUESTION = "Find one Python module in this repository that implements retrieval-augmented generation and cite its path."


@dataclass
class WireLog:
    entries: list[dict] = field(default_factory=list)
    next_connection: int = 1

    def append(self, connection: int, direction: str, raw: bytes) -> None:
        try:
            payload = json.loads(raw)
        except (json.JSONDecodeError, UnicodeDecodeError):
            payload = {"type": "invalid_json"}
        self.entries.append(
            {
                "at": time.monotonic(),
                "connection": connection,
                "direction": direction,
                "payload": payload,
            }
        )


class PtyClient:
    def __init__(self, binary: Path, socket_path: Path, session: str, repo: str, cwd: Path, env: dict[str, str]):
        self.output = bytearray()
        pid, descriptor = pty.fork()
        if pid == 0:
            os.chdir(cwd)
            os.execvpe(
                str(binary),
                [str(binary), "connect", "--socket", str(socket_path), "--session", session, "--repo", repo],
                env,
            )
        self.pid = pid
        self.descriptor = descriptor
        self.session = session
        self._closed = False
        import fcntl

        fcntl.ioctl(descriptor, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 0, 0))
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self) -> None:
        while True:
            try:
                chunk = os.read(self.descriptor, 65536)
            except OSError:
                return
            if not chunk:
                return
            self.output.extend(chunk)

    def send(self, value: bytes) -> None:
        os.write(self.descriptor, value)

    def visible_text(self) -> str:
        clean = ANSI.sub(b"", bytes(self.output))
        return "".join(chr(byte) if byte in (9, 10, 13) or 32 <= byte < 127 else " " for byte in clean)

    def terminate(self) -> None:
        if self._closed:
            return
        self._closed = True
        try:
            os.kill(self.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            os.waitpid(self.pid, 0)
        except ChildProcessError:
            pass
        try:
            os.close(self.descriptor)
        except OSError:
            pass


async def wait_for(description: str, predicate, timeout: float = 20.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        await asyncio.sleep(0.05)
    raise RuntimeError(f"timed out waiting for {description}")


def frames(log: WireLog, direction: str = "client") -> list[dict]:
    return [entry for entry in log.entries if entry["direction"] == direction]


def connection_for(log: WireLog, session: str) -> int | None:
    for entry in reversed(frames(log)):
        payload = entry["payload"]
        if payload.get("type") == "attach" and payload.get("session_id") == session:
            return entry["connection"]
    return None


def request_type(entry: dict) -> str | None:
    payload = entry["payload"]
    if payload.get("type") != "request":
        return None
    request = payload.get("request")
    return request.get("type") if isinstance(request, dict) else None


async def proxy_connection(reader: asyncio.StreamReader, writer: asyncio.StreamWriter, upstream: Path, log: WireLog):
    connection = log.next_connection
    log.next_connection += 1
    upstream_reader, upstream_writer = await asyncio.open_unix_connection(upstream)

    async def relay(source: asyncio.StreamReader, target: asyncio.StreamWriter, direction: str):
        while line := await source.readline():
            log.append(connection, direction, line)
            target.write(line)
            await target.drain()
        target.close()

    await asyncio.gather(
        relay(reader, upstream_writer, "client"),
        relay(upstream_reader, writer, "server"),
        return_exceptions=True,
    )


async def run(args: argparse.Namespace) -> dict:
    binary = args.binary.resolve()
    repo_dir = args.repo_dir.resolve()
    output = args.output.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise RuntimeError(f"installed binary is not executable: {binary}")
    if not (repo_dir / ".git").exists():
        raise RuntimeError(f"real repository clone is absent: {repo_dir}")
    if not os.environ.get("ESTELLE_API_KEY"):
        raise RuntimeError("ESTELLE_API_KEY is absent; the production probe fails closed")
    version = subprocess.run([binary, "--version"], text=True, capture_output=True, check=True).stdout.strip()
    if version != args.expected_version.replace("v", "estelle ", 1):
        raise RuntimeError(f"installed version mismatch: {version}")

    runtime = output.parent / "five-worker-runtime"
    runtime.mkdir(parents=True, exist_ok=True)
    real_socket = runtime / "server.sock"
    proxy_socket = runtime / "client.sock"
    for path in (real_socket, proxy_socket):
        if path.exists() or path.is_socket():
            path.unlink()

    probe_home = runtime / "home"
    probe_home.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update(
        {
            "HOME": str(probe_home),
            "XDG_CONFIG_HOME": str(probe_home / ".config"),
            "TERM": "xterm-256color",
            "NO_COLOR": "1",
        }
    )
    server_process = subprocess.Popen(
        [binary, "serve", "--socket", real_socket, "--repo", args.repo],
        cwd=repo_dir,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    clients: list[PtyClient] = []
    log = WireLog()
    proxy = None
    try:
        await wait_for("session owner socket", real_socket.exists, timeout=10)
        proxy = await asyncio.start_unix_server(
            lambda reader, writer: proxy_connection(reader, writer, real_socket, log),
            path=proxy_socket,
        )

        async def attach(session: str) -> PtyClient:
            previous = connection_for(log, session)
            client = PtyClient(binary, proxy_socket, session, args.repo, repo_dir, env)
            clients.append(client)
            await wait_for(
                f"{session} attach",
                lambda: (current := connection_for(log, session)) and current != previous and current,
            )
            await wait_for(
                f"{session} composer",
                lambda: "Ask Estelle" in client.visible_text(),
                timeout=10,
            )
            return client

        async def ask(client: PtyClient, question: str = QUESTION) -> int:
            connection = connection_for(log, client.session)
            # Exercise the terminal's advertised bracketed-paste path, then submit separately.
            # This avoids relying on timing in ChatComposer's plain-character paste heuristic.
            client.send(b"\x1b[200~" + question.encode() + b"\x1b[201~")
            await asyncio.sleep(0.15)
            client.send(b"\r")
            try:
                await wait_for(
                    f"{client.session} ask frame",
                    lambda: next(
                        (
                            entry
                            for entry in frames(log)
                            if entry["connection"] == connection and request_type(entry) == "ask"
                        ),
                        None,
                    ),
                )
            except RuntimeError as error:
                raise RuntimeError(
                    f"{error}; terminal tail={client.visible_text()[-600:]!r}"
                ) from error
            return int(connection)

        worker1 = await attach("worker-1")
        await ask(worker1)
        worker2 = await attach("worker-2")
        worker2_connection = await ask(worker2)
        worker2.send(b"\x03")
        negative_cancel = await wait_for(
            "Ctrl+C cancel frame",
            lambda: next(
                (
                    entry
                    for entry in frames(log)
                    if entry["connection"] == worker2_connection and request_type(entry) == "cancel"
                ),
                None,
            ),
        )
        worker4 = await attach("worker-4")
        await ask(worker4)
        worker3 = await attach("worker-3")
        worker3_connection = await ask(worker3)
        await wait_for(
            "worker-3 server start",
            lambda: next(
                (
                    entry
                    for entry in log.entries
                    if entry["connection"] == worker3_connection
                    and entry["direction"] == "server"
                    and entry["payload"].get("type") == "started"
                ),
                None,
            ),
        )
        worker3.terminate()  # terminal disconnect while the server still owns the request

        worker5 = await attach("worker-5")
        worker5_connection = int(connection_for(log, "worker-5"))
        snapshot = await wait_for(
            "five-session snapshot",
            lambda: next(
                (
                    entry["payload"]
                    for entry in log.entries
                    if entry["connection"] == worker5_connection
                    and entry["direction"] == "server"
                    and entry["payload"].get("type") == "snapshot"
                    and len(entry["payload"].get("sessions", [])) == 5
                ),
                None,
            ),
        )
        if not next(row for row in snapshot["sessions"] if row["id"] == "worker-3")["active"]:
            raise RuntimeError("worker-3 finished before the close-view control; liveness was not exercised")
        await wait_for(
            "five rendered tab labels",
            lambda: all(f"worker-{index}" in worker5.visible_text() for index in range(1, 6)),
        )

        worker5.send(b"\x1b[1;3D\x1b[1;3D")
        await wait_for(
            "switch to worker-3",
            lambda: next(
                (
                    entry
                    for entry in frames(log)
                    if entry["connection"] == worker5_connection
                    and entry["payload"].get("type") == "switch"
                    and entry["payload"].get("session_id") == "worker-3"
                ),
                None,
            ),
        )
        close_start = len(log.entries)
        worker5.send(b"\x17")
        close_switch = await wait_for(
            "Ctrl+W view switch",
            lambda: next(
                (
                    entry
                    for entry in log.entries[close_start:]
                    if entry["direction"] == "client"
                    and entry["connection"] == worker5_connection
                    and entry["payload"].get("type") == "switch"
                    and entry["payload"].get("session_id") == "worker-4"
                ),
                None,
            ),
        )
        close_frames = [
            entry["payload"]
            for entry in log.entries[close_start:]
            if entry["direction"] == "client" and entry["connection"] == worker5_connection
        ]
        if any(request_type({"payload": payload}) == "cancel" for payload in close_frames):
            raise RuntimeError("Ctrl+W emitted cancel; the view killed server-owned work")

        replay = await attach("worker-3")
        replay_connection = int(connection_for(log, "worker-3"))

        def terminal_worker3():
            candidates = [
                entry["payload"]
                for entry in log.entries
                if entry["connection"] == replay_connection and entry["direction"] == "server"
            ]
            for payload in reversed(candidates):
                if payload.get("type") == "completed":
                    return payload["turn"]
                if payload.get("type") == "snapshot" and payload.get("turns"):
                    return payload["turns"][-1]
            return None

        terminal = await wait_for("worker-3 terminal replay", terminal_worker3, timeout=90)
        if terminal.get("input", {}).get("question") != QUESTION:
            raise RuntimeError("worker-3 replay did not contain the detached request")
        outcome = terminal.get("outcome", {})
        if outcome.get("type") not in {"answer", "failure"}:
            raise RuntimeError(f"worker-3 replay was not terminal: {outcome.get('type')}")

        receipt = {
            "ok": True,
            "binary": version,
            "repo": args.repo,
            "repo_head": subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=repo_dir, text=True, capture_output=True, check=True
            ).stdout.strip(),
            "rendered_tabs": [f"worker-{index}" for index in range(1, 6)],
            "worker_3_active_at_close": True,
            "close_frame": close_switch["payload"],
            "close_emitted_cancel": False,
            "negative_control": negative_cancel["payload"],
            "worker_3_replay": {
                "question": terminal["input"]["question"],
                "outcome": outcome.get("type"),
                "answer_nonempty": bool(outcome.get("answer", {}).get("text")),
                "failure_lines": outcome.get("lines", []),
            },
            "server_alive_after_close": server_process.poll() is None,
        }
        output.write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps(receipt, indent=2))
        replay.terminate()
        return receipt
    finally:
        for client in clients:
            client.terminate()
        if proxy is not None:
            proxy.close()
            await proxy.wait_closed()
        if server_process.poll() is None:
            server_process.send_signal(signal.SIGINT)
            try:
                server_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server_process.terminate()
                server_process.wait(timeout=5)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--repo-dir", type=Path, required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--expected-version", default="v0.2.16")
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    parsed = parse_args()
    try:
        asyncio.run(run(parsed))
    except Exception as error:
        failure = {"ok": False, "error": str(error)}
        parsed.output.parent.mkdir(parents=True, exist_ok=True)
        parsed.output.write_text(json.dumps(failure, indent=2) + "\n")
        print(json.dumps(failure, indent=2))
        raise
