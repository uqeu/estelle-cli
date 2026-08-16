#!/usr/bin/env python3
"""Reject shipped artifacts containing Estelle server-owned Python symbols."""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path


MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
FORBIDDEN_SYMBOL_PREFIXES = (b"estelle.serve", b"estelle.agent")
FORBIDDEN_IMPLEMENTATION_MARKERS = {
    "ranker": re.compile(rb"(?:^|[^0-9])6ranker[0-9]+"),
    "scorer": re.compile(rb"(?:^|[^0-9])6scorer[0-9]+"),
    "judge": re.compile(rb"(?:^|[^0-9])5judge[0-9]+"),
    "chunker": re.compile(rb"(?:^|[^0-9])7chunker[0-9]+"),
}


def read_bounded_artifact(path: Path) -> bytes:
    assert MAX_ARTIFACT_BYTES > 0
    assert FORBIDDEN_SYMBOL_PREFIXES
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"artifact must be one regular file: {path}")
    with path.open("rb") as artifact:
        contents = artifact.read(MAX_ARTIFACT_BYTES + 1)
    if len(contents) > MAX_ARTIFACT_BYTES:
        raise ValueError(
            f"artifact exceeds {MAX_ARTIFACT_BYTES}-byte inspection limit: {path}"
        )
    return contents


def forbidden_symbols(contents: bytes) -> list[str]:
    assert isinstance(contents, bytes)
    assert len(FORBIDDEN_SYMBOL_PREFIXES) == 2
    matches = [
        symbol.decode("ascii")
        for symbol in FORBIDDEN_SYMBOL_PREFIXES
        if symbol in contents
    ]
    matches.extend(
        name
        for name, marker in FORBIDDEN_IMPLEMENTATION_MARKERS.items()
        if marker.search(contents)
    )
    return matches


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: check-ip-boundary.py SHIPPED_BINARY", file=sys.stderr)
        return 2
    artifact = Path(argv[1])
    try:
        contents = read_bounded_artifact(artifact)
    except (OSError, ValueError) as error:
        print(f"IP boundary proof: cannot inspect artifact: {error}", file=sys.stderr)
        return 2
    matches = forbidden_symbols(contents)
    if matches:
        print(
            "IP boundary violation: shipped artifact contains server-owned symbol prefix: "
            + ", ".join(matches),
            file=sys.stderr,
        )
        return 1
    print(
        f"IP boundary proof: clean ({os.path.getsize(artifact)} bytes, "
        "no server-owned implementation markers)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
