#!/usr/bin/env python3
"""Behavioral tests for the installed-public-binary receipt harness."""

from __future__ import annotations

import json
import importlib.util
import os
import subprocess
import sys
import tempfile
from pathlib import Path


sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
HARNESS = ROOT / "scripts" / "public-binary-receipts.py"

EXPECTED_READ_SURFACES = [
    "/init",
    "/graph",
    "/graph nodes",
    "/me",
    "/keys",
    "/team",
    "/team board",
    "/cards",
    "/entities",
    "/usage",
    "/activity",
    "/runs",
    "/outcomes",
    "/memories",
    "/analytics",
    "/audit",
    "/requests",
    "/presence",
    "/leaderboard",
    "/marketplace",
    "/automations",
    "/suites",
    "/billing",
    "/sessions",
]


def load_harness():
    spec = importlib.util.spec_from_file_location("public_binary_receipts", HARNESS)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_inventory() -> None:
    result = subprocess.run(
        [sys.executable, str(HARNESS), "--list"],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    surfaces = json.loads(result.stdout)
    assert surfaces == EXPECTED_READ_SURFACES, surfaces
    assert len(set(surfaces)) == 24


def test_installed_version() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-receipt-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n[ \"$1\" = --version ] && printf 'estelle 9.9.9\\n'\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().installed_version_receipt("v9.9.9")
        finally:
            os.environ["PATH"] = original_path
        assert receipt["sent"] == "estelle --version"
        assert receipt["came_back"] == "estelle 9.9.9"
        assert receipt["pass"] is True
        assert Path(receipt["resolved_binary"]).samefile(fake_estelle)


def test_tui_surface() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-tui-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "printf '\\033[2J\\033[1;1HAsk Estelle\\n'\n"
            "IFS= read -r command\n"
            "printf '\\033[2J\\033[1;1Hyou  %s\\nSERVER RECEIPT OK\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_surface_receipt(
                "/graph", "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["sent"] == "/graph"
        assert "you  /graph" in receipt["came_back"]
        assert "SERVER RECEIPT OK" in receipt["came_back"]
        assert receipt["pass"] is True


def test_tui_surface_fails_closed() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-tui-fail-") as raw_dir:
        fake_bin = Path(raw_dir) / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "printf 'Ask Estelle\\n'\n"
            "IFS= read -r command\n"
            "printf 'you  %s\\nEstelle returned HTTP 404: absent\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        original_path = os.environ.get("PATH", "")
        os.environ["PATH"] = f"{fake_bin}{os.pathsep}{original_path}"
        try:
            receipt = load_harness().tui_surface_receipt(
                "/graph", "uqeu/estelle", timeout=3
            )
        finally:
            os.environ["PATH"] = original_path
        assert receipt["pass"] is False
        assert "HTTP 404" in receipt["came_back"]


def test_complete_harness_writes_every_receipt() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-public-full-") as raw_dir:
        root = Path(raw_dir)
        fake_bin = root / "bin"
        fake_bin.mkdir()
        fake_estelle = fake_bin / "estelle"
        fake_estelle.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = --version ]; then printf 'estelle 9.9.9\\n'; exit 0; fi\n"
            "printf 'Ask Estelle\\n'\n"
            "IFS= read -r command\n"
            "printf 'you  %s\\nSERVER RECEIPT OK\\n› Ask Estelle\\n' \"$command\"\n",
            encoding="utf-8",
        )
        fake_estelle.chmod(0o755)
        output = root / "receipts.json"
        environment = os.environ.copy()
        environment["PATH"] = f"{fake_bin}{os.pathsep}{environment.get('PATH', '')}"
        result = subprocess.run(
            [
                sys.executable,
                str(HARNESS),
                "--expected-version",
                "v9.9.9",
                "--repo",
                "uqeu/estelle",
                "--output",
                str(output),
                "--timeout",
                "2",
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
            timeout=30,
        )
        assert result.returncode == 0, result.stderr
        report = json.loads(output.read_text(encoding="utf-8"))
        assert report["summary"] == {"passed": 25, "failed": 0}
        assert [row["sent"] for row in report["receipts"][1:]] == EXPECTED_READ_SURFACES
        assert all(row["pass"] for row in report["receipts"])


def main() -> int:
    test_inventory()
    test_installed_version()
    test_tui_surface()
    test_tui_surface_fails_closed()
    test_complete_harness_writes_every_receipt()

    print("public receipt test: all 24 audited read surfaces are mandatory")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
