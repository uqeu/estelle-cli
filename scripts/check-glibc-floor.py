#!/usr/bin/env python3
"""Refuse a Linux binary that demands a newer glibc than our declared floor.

🔴 MEASURED 2026-09-03 IN A REAL E2B FIRECRACKER MICROVM. `npm i -g @fatelabs/estelle@0` on a stock
Debian 12 (bookworm) image installed cleanly and then could not start:

    estelle: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found
    estelle: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found

bookworm ships glibc **2.36**. The binary demanded **2.38 and 2.39**, because
`.github/workflows/release.yml` builds it on `ubuntu-24.04`, which ships 2.39. A Rust binary linked
against a glibc demands that version at LOAD time.

⚠️ AND OUR OWN CI COULD NOT SEE IT. All ten fixed `runs-on:` values across every workflow in this repo
are `ubuntu-24.04` — including every job whose purpose is to verify the published binary. **Every green
those jobs ever produced was measured on the one image where the artifact works.** A guard that runs in
one environment and not another has a coverage hole exactly where it matters.

WHY THIS CHECK RATHER THAN A TEST MATRIX: a matrix job proves the binary runs on the images we thought
of. This reads the binary's OWN dynamic-symbol requirements and compares them to a declared floor — a
deterministic lookup with the same verdict every run, which fails at BUILD time rather than at a
customer's install. It cannot be defeated by a runner-image bump, which is exactly how this shipped.

WHAT IT DOES NOT COVER, said here so a green is not over-read: it checks the GLIBC version tags on
dynamic symbol references. It says nothing about other shared-library floors (OpenSSL, libgcc), nothing
about musl targets, and nothing about whether the binary is CORRECT once it loads.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

#: The oldest glibc we promise to run on, as a (major, minor) tuple.
#:
#: 2.35 is Ubuntu 22.04 LTS. Choosing it covers, by inspection:
#:   Ubuntu 22.04 LTS  2.35   ·  Debian 12 bookworm  2.36  ·  Ubuntu 24.04  2.39  ·  Debian 13  2.41
#: Raising this number is a decision to drop distributions, and it belongs in a commit message that
#: says which ones. It is NOT a knob to turn when the check goes red.
MAX_GLIBC = (2, 35)

_TAG = re.compile(r"GLIBC_(\d+)\.(\d+)")


def required_glibc(binary: Path) -> "set[tuple[int, int]]":
    """Every GLIBC version tag the binary references, via objdump then readelf.

    Two tools because a machine may have either; if NEITHER is present we must not return an empty set,
    because empty would read as "requires nothing" — the absent-versus-zero defect this repo names most
    often. The caller distinguishes them.
    """
    for cmd in (["objdump", "-T", str(binary)], ["readelf", "--dyn-syms", "--wide", str(binary)]):
        try:
            out = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        except (FileNotFoundError, subprocess.TimeoutExpired):
            continue
        if out.returncode == 0 and out.stdout:
            return {(int(a), int(b)) for a, b in _TAG.findall(out.stdout)}
    raise RuntimeError("neither objdump nor readelf is available — the floor was NOT checked")


def main(argv: "list[str]") -> int:
    if len(argv) != 2:
        print("usage: check-glibc-floor.py <binary>", file=sys.stderr)
        return 2
    binary = Path(argv[1])
    if not binary.is_file():
        print(f"🔴 not a file: {binary}", file=sys.stderr)
        return 2

    try:
        needed = required_glibc(binary)
    except RuntimeError as exc:
        # FAIL CLOSED. "I could not check" is not "it passed".
        print(f"🔴 {exc}", file=sys.stderr)
        return 2

    if not needed:
        # A statically linked or musl binary legitimately references no GLIBC tag. Say which, rather
        # than printing a green that looks like a glibc verdict.
        print(f"✅ {binary.name} references NO GLIBC version tags (static or musl) — no floor to breach")
        return 0

    too_new = sorted(v for v in needed if v > MAX_GLIBC)
    hi = max(needed)
    if too_new:
        pretty = ", ".join(f"GLIBC_{a}.{b}" for a, b in too_new)
        print(f"🔴 {binary.name} demands {pretty}, above the declared floor "
              f"GLIBC_{MAX_GLIBC[0]}.{MAX_GLIBC[1]}.", file=sys.stderr)
        print("   A customer on an older distribution installs this successfully and then cannot start "
              "it — measured on Debian 12 (glibc 2.36) in a Firecracker microVM on 2026-09-03.",
              file=sys.stderr)
        print("   Build on an older runner (ubuntu-22.04) or target x86_64-unknown-linux-musl. Do NOT "
              "raise MAX_GLIBC to make this pass without naming the distributions it drops.",
              file=sys.stderr)
        return 1

    print(f"✅ {binary.name} needs at most GLIBC_{hi[0]}.{hi[1]} "
          f"(floor GLIBC_{MAX_GLIBC[0]}.{MAX_GLIBC[1]}) — runs on Ubuntu 22.04 LTS and newer")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
