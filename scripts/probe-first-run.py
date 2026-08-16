#!/usr/bin/env python3
"""Prove the installed binary reaches its credential picker in a real pseudo-terminal."""

import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios
import time


def rendered_screen(data: bytes, rows: int = 30, columns: int = 120) -> str:
    """Apply the small ANSI cursor subset Ratatui uses and return visible screen text."""
    screen = [[" "] * columns for _ in range(rows)]
    row = 0
    column = 0
    text = data.decode("utf-8", errors="ignore")
    index = 0
    while index < len(text):
        character = text[index]
        if character == "\x1b":
            if index + 1 < len(text) and text[index + 1] == "[":
                end = index + 2
                while end < len(text) and not ("@" <= text[end] <= "~"):
                    end += 1
                if end >= len(text):
                    break
                raw = text[index + 2 : end]
                final = text[end]
                params = raw.lstrip("?<>!")
                values = [int(value) if value.isdigit() else 1 for value in params.split(";")]
                if final in ("H", "f"):
                    row = max(0, min(rows - 1, (values[0] if values else 1) - 1))
                    column = max(0, min(columns - 1, (values[1] if len(values) > 1 else 1) - 1))
                elif final == "G":
                    column = max(0, min(columns - 1, (values[0] if values else 1) - 1))
                elif final == "A":
                    row = max(0, row - (values[0] if values else 1))
                elif final == "B":
                    row = min(rows - 1, row + (values[0] if values else 1))
                elif final == "C":
                    column = min(columns - 1, column + (values[0] if values else 1))
                elif final == "D":
                    column = max(0, column - (values[0] if values else 1))
                elif final == "J" and raw == "2":
                    screen = [[" "] * columns for _ in range(rows)]
                elif final == "K":
                    screen[row][column:] = [" "] * (columns - column)
                index = end + 1
                continue
            if index + 1 < len(text) and text[index + 1] == "]":
                end = index + 2
                while end < len(text):
                    if text[end] == "\a":
                        end += 1
                        break
                    if text[end : end + 2] == "\x1b\\":
                        end += 2
                        break
                    end += 1
                index = end
                continue
            index += 2
            continue
        if character == "\r":
            column = 0
        elif character == "\n":
            row = min(rows - 1, row + 1)
        elif character == "\b":
            column = max(0, column - 1)
        elif character >= " ":
            screen[row][column] = character
            column = min(columns - 1, column + 1)
        index += 1
    return "\n".join("".join(line).rstrip() for line in screen)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: probe-first-run.py /path/to/estelle", file=sys.stderr)
        return 2
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(sys.argv[1], [sys.argv[1]])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 120, 0, 0))
    os.kill(pid, signal.SIGWINCH)
    observed = bytearray()
    deadline = time.monotonic() + 5
    try:
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.25)
            if ready:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                observed.extend(chunk)
        visible = rendered_screen(observed)
        if "CONNECT ESTELLE" not in visible:
            print("first-run picker was not reachable", file=sys.stderr)
            print(visible, file=sys.stderr)
            return 1
        os.write(fd, b"\x03")
        return 0
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except ChildProcessError:
            pass
        except ProcessLookupError:
            os.waitpid(pid, 0)


if __name__ == "__main__":
    raise SystemExit(main())
