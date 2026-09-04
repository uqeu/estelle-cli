#!/usr/bin/env python3
"""Read a published npm shim back and compare the customer files to source."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import time


PACKAGE = "@fatelabs/estelle"
REGISTRY_READBACK_ATTEMPTS = 61
REGISTRY_READBACK_DELAY_SECONDS = 5
COMMAND_TIMEOUT_SECONDS = 30
# 🔴 package/LICENSE WAS MISSING HERE AND THE READ-BACK HAS BEEN RED SINCE IT WAS ADDED.
# npm packs a LICENSE file whether or not `files:` names it, so `npm-shim/LICENSE` (added in
# 4cfe4d309) made every published artifact a five-member tarball while this set still described
# four. Measured 2026-09-04 against the live @fatelabs/estelle@0.2.33 on the registry:
# "unexpected npm artifact members: ['package/LICENSE', ...]". The pin was correct about its
# four and silent about the fifth — a partial guard reporting complete.
EXPECTED_MEMBERS = {
    "package/LICENSE",
    "package/README.md",
    "package/bin/estelle.js",
    "package/install.js",
    "package/package.json",
}


def npm_env(cache: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["npm_config_cache"] = str(cache)
    return env


def read_registry_version(version: str, cache: Path) -> None:
    assert REGISTRY_READBACK_ATTEMPTS > 0
    assert COMMAND_TIMEOUT_SECONDS > 0
    command = ["npm", "view", f"{PACKAGE}@{version}", "version", "--json", "--prefer-online"]
    for attempt in range(1, REGISTRY_READBACK_ATTEMPTS + 1):
        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=COMMAND_TIMEOUT_SECONDS,
            env=npm_env(cache),
            check=False,
        )
        if result.returncode == 0 and json.loads(result.stdout) == version:
            return
        if attempt < REGISTRY_READBACK_ATTEMPTS:
            time.sleep(REGISTRY_READBACK_DELAY_SECONDS)
    raise RuntimeError(f"npm registry did not return {PACKAGE}@{version} after bounded read-back")


def pack_registry_artifact(version: str, output: Path, cache: Path) -> Path:
    assert output.is_dir()
    assert cache.is_dir()
    subprocess.run(
        [
            "npm",
            "pack",
            f"{PACKAGE}@{version}",
            "--ignore-scripts",
            "--pack-destination",
            str(output),
        ],
        capture_output=True,
        text=True,
        timeout=COMMAND_TIMEOUT_SECONDS,
        env=npm_env(cache),
        check=True,
    )
    archives = list(output.glob("*.tgz"))
    if len(archives) != 1:
        raise RuntimeError(f"npm pack returned {len(archives)} archives; expected exactly one")
    return archives[0]


def verify_customer_files(archive: Path, expected_directory: Path, version: str) -> None:
    assert archive.is_file()
    assert expected_directory.is_dir()
    with tarfile.open(archive, "r:gz") as package:
        members = package.getmembers()
        names = {member.name for member in members}
        if names != EXPECTED_MEMBERS or any(not member.isfile() for member in members):
            raise RuntimeError(f"unexpected npm artifact members: {sorted(names)}")
        manifest = json.load(package.extractfile("package/package.json"))  # type: ignore[arg-type]
        if manifest.get("name") != PACKAGE or manifest.get("version") != version:
            raise RuntimeError("npm artifact manifest does not identify the requested package version")
        for member in sorted(EXPECTED_MEMBERS):
            source = expected_directory / member.removeprefix("package/")
            packed = package.extractfile(member)
            if packed is None or packed.read() != source.read_bytes():
                raise RuntimeError(f"npm artifact differs from release source: {member}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--expected-directory", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", args.version):
        raise SystemExit("version must be an exact stable SemVer")
    with tempfile.TemporaryDirectory(prefix="estelle-npm-readback-") as temporary:
        root = Path(temporary)
        cache = root / "cache"
        output = root / "pack"
        cache.mkdir()
        output.mkdir()
        read_registry_version(args.version, cache)
        archive = pack_registry_artifact(args.version, output, cache)
        verify_customer_files(archive, args.expected_directory.resolve(), args.version)
    print(f"npm read-back: {PACKAGE}@{args.version}, exact four-file customer artifact")


if __name__ == "__main__":
    main()
