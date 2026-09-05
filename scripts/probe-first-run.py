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

#: The header the first-run picker renders. Kept byte-identical to the assertion in
#: ``tui/src/main.rs`` -- see the comment at the check below.
HEADER = "── connect estelle ─"
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
        # 🔴 ONE OWNER FOR THIS STRING, AND IT IS THE UI. The other owner is
        # `tui/src/main.rs`'s `assert!(rendered.contains("── connect estelle ─"))`. They disagreed:
        # the header was lowercased on 2026-08-31 (0263ffa2c), the Rust test moved with it, and THIS
        # probe kept asserting the old uppercase form. Nothing released in between, so the mismatch
        # first surfaced in v0.2.33 -- both Linux jobs failed here AFTER building cleanly and AFTER
        # passing the glibc floor check. If you change the header, change it in BOTH places.
        if HEADER not in visible:
            print(f"first-run picker did not render the expected header {HEADER!r}", file=sys.stderr)
            print("(the picker may still be reachable -- this asserts the HEADER TEXT, not reachability)",
                  file=sys.stderr)
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
