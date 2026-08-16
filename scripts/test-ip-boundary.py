#!/usr/bin/env python3
"""Behavioral tests for the shipped-binary IP boundary."""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts" / "check-ip-boundary.py"
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"


def run_checker(binary: Path) -> subprocess.CompletedProcess[str]:
    assert binary.is_file(), f"fixture must be a file: {binary}"
    assert ROOT.is_dir(), f"repository root must exist: {ROOT}"
    return subprocess.run(
        [sys.executable, str(CHECKER), str(binary)],
        check=False,
        capture_output=True,
        text=True,
    )


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="estelle-ip-boundary-") as raw_dir:
        fixture_dir = Path(raw_dir)
        clean = fixture_dir / "clean-estelle"
        forbidden = fixture_dir / "server-code-estelle"
        clean.write_bytes(b"\x00estelle transport auth tui acp hooks\x00")
        forbidden.write_bytes(b"\x00from estelle.serve import ground\x00")

        clean_result = run_checker(clean)
        assert clean_result.returncode == 0, clean_result.stderr
        assert "IP boundary proof: clean" in clean_result.stdout, clean_result.stdout

        forbidden_result = run_checker(forbidden)
        assert forbidden_result.returncode != 0, forbidden_result.stdout
        assert "estelle.serve" in forbidden_result.stderr, forbidden_result.stderr

        workflow = WORKFLOW.read_text(encoding="utf-8")
        release_invocation = "python3 scripts/check-ip-boundary.py \"$binary\""
        assert workflow.count(release_invocation) == 1, (
            "every target-native release artifact must cross the IP gate exactly once"
        )

    print("IP boundary test: clean artifact passes and server-symbol mutant fails")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
