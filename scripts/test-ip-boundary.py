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
        standalone_ranker = fixture_dir / "standalone-ranker-estelle"
        standalone_scorer = fixture_dir / "standalone-scorer-estelle"
        standalone_judge = fixture_dir / "standalone-judge-estelle"
        standalone_chunker = fixture_dir / "standalone-chunker-estelle"
        dependency_ranker = fixture_dir / "dependency-ranker-estelle"
        clean.write_bytes(b"\x00estelle transport auth tui acp hooks\x00")
        forbidden.write_bytes(b"\x00from estelle.serve import ground\x00")
        standalone_ranker.write_bytes(b"\x00_ZN11thin_client6ranker15score_documents\x00")
        standalone_scorer.write_bytes(b"\x00_ZN11thin_client6scorer15score_documents\x00")
        standalone_judge.write_bytes(b"\x00_ZN11thin_client5judge14judge_response\x00")
        standalone_chunker.write_bytes(b"\x00_ZN11thin_client7chunker14chunk_document\x00")
        dependency_ranker.write_bytes(
            b"\x00memchr_memmem_FinderBuilder_build_forward_with_ranker\x00"
        )

        clean_result = run_checker(clean)
        assert clean_result.returncode == 0, clean_result.stderr
        assert "IP boundary proof: clean" in clean_result.stdout, clean_result.stdout

        forbidden_result = run_checker(forbidden)
        assert forbidden_result.returncode != 0, forbidden_result.stdout
        assert "estelle.serve" in forbidden_result.stderr, forbidden_result.stderr

        ranker_result = run_checker(standalone_ranker)
        assert ranker_result.returncode != 0, ranker_result.stdout
        assert "ranker" in ranker_result.stderr, ranker_result.stderr

        scorer_result = run_checker(standalone_scorer)
        assert scorer_result.returncode != 0, scorer_result.stdout
        assert "scorer" in scorer_result.stderr, scorer_result.stderr

        judge_result = run_checker(standalone_judge)
        assert judge_result.returncode != 0, judge_result.stdout
        assert "judge" in judge_result.stderr, judge_result.stderr

        chunker_result = run_checker(standalone_chunker)
        assert chunker_result.returncode != 0, chunker_result.stdout
        assert "chunker" in chunker_result.stderr, chunker_result.stderr

        dependency_result = run_checker(dependency_ranker)
        assert dependency_result.returncode == 0, dependency_result.stderr
        assert "IP boundary proof: clean" in dependency_result.stdout

        workflow = WORKFLOW.read_text(encoding="utf-8")
        release_invocation = "python3 scripts/check-ip-boundary.py \"$binary\""
        assert workflow.count(release_invocation) == 1, (
            "every target-native release artifact must cross the IP gate exactly once"
        )

    print("IP boundary test: thin artifacts pass and server implementation mutants fail")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
