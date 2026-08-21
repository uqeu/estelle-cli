#!/usr/bin/env python3
"""Prove the npm artifact read-back accepts source bytes and rejects a changed customer file."""

from importlib.util import module_from_spec, spec_from_file_location
import io
import json
from pathlib import Path
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SPEC = spec_from_file_location("verify_npm_release", ROOT / "scripts" / "verify-npm-release.py")
assert SPEC is not None
assert SPEC.loader is not None
VERIFY = module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def write_fixture(root: Path, changed_readme: bool) -> tuple[Path, Path]:
    assert root.is_dir()
    assert VERIFY.EXPECTED_MEMBERS
    source = root / "source"
    source.mkdir()
    files = {
        "README.md": b"customer readme\n",
        "bin/estelle.js": b"#!/usr/bin/env node\n",
        "install.js": b"export const install = true;\n",
        "package.json": json.dumps({"name": VERIFY.PACKAGE, "version": "1.2.3"}).encode(),
    }
    for name, content in files.items():
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    archive = root / ("mutant.tgz" if changed_readme else "control.tgz")
    with tarfile.open(archive, "w:gz") as package:
        for name, content in files.items():
            packed = b"changed after publish\n" if changed_readme and name == "README.md" else content
            info = tarfile.TarInfo(f"package/{name}")
            info.size = len(packed)
            package.addfile(info, io.BytesIO(packed))
    return archive, source


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="estelle-npm-verifier-test-") as temporary:
        root = Path(temporary)
        control_root = root / "control"
        mutant_root = root / "mutant"
        control_root.mkdir()
        mutant_root.mkdir()
        control, source = write_fixture(control_root, changed_readme=False)
        VERIFY.verify_customer_files(control, source, "1.2.3")
        mutant, mutant_source = write_fixture(mutant_root, changed_readme=True)
        try:
            VERIFY.verify_customer_files(mutant, mutant_source, "1.2.3")
        except RuntimeError as error:
            assert "README.md" in str(error)
        else:
            raise AssertionError("npm artifact verifier accepted a changed customer file")
    print("npm artifact proof: exact source passes; changed README mutant fails")


if __name__ == "__main__":
    main()
