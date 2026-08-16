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

sys.dont_write_bytecode = True
from terminal_screen import rendered_screen


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
