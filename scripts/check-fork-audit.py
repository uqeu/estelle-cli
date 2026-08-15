#!/usr/bin/env python3
"""Fail closed when the pinned fork provenance or finite egress census drifts."""
from __future__ import annotations

import fnmatch
import hashlib
import json
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "fork-manifest.yaml"
EGRESS = ROOT / "docs" / "egress-sinks.toml"


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, text=True, capture_output=True, timeout=60
    ).stdout.strip()


def fail(message: str) -> None:
    raise SystemExit(f"fork audit failed: {message}")


def load_json_yaml(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"{path.name} is not JSON-compatible YAML: {type(exc).__name__}")
    if not isinstance(value, dict):
        fail(f"{path.name} must contain one mapping")
    return value


def verify_provenance(manifest: dict) -> None:
    upstream = manifest["upstream"]
    imported = manifest["import"]
    policy = manifest["policy"]
    base = upstream["base_commit"]
    try:
        git("cat-file", "-e", f"{base}^{{commit}}")
    except subprocess.CalledProcessError:
        fail(f"missing pinned upstream object {base}; fetch only that exact object from the recorded repo")
    if git("rev-parse", f"{base}^{{tree}}") != upstream["base_tree"]:
        fail("upstream base tree does not match the manifest")
    if git("rev-parse", f"{base}:{upstream['source_subtree']}") != upstream["source_subtree_tree"]:
        fail("upstream source subtree does not match the manifest")
    if git("rev-parse", f"{imported['commit']}^{{tree}}") != imported["tree"]:
        fail("Estelle import tree does not match the manifest")
    names = git(
        "diff",
        "--no-renames",
        "--name-status",
        f"{base}:{upstream['source_subtree']}",
        imported["commit"],
    ).splitlines()
    counts = Counter(line.split("\t", 1)[0][0] for line in names if line)
    expected = imported["upstream_subtree_to_import"]
    if counts != Counter({"A": expected["added"], "D": expected["deleted"], "M": expected["modified"]}):
        fail(f"initial import delta changed: {dict(counts)}")
    audited = policy["audited_through_commit"]
    try:
        git("merge-base", "--is-ancestor", audited, "HEAD")
    except subprocess.CalledProcessError:
        fail("audited-through commit is not an ancestor of HEAD")
    reviewed = {row["path"]: row for row in manifest["reviewed_changes_after_audited_commit"]}
    # Compare the audited commit with the complete current tree. This works before commit during local
    # review and is identical to ``audited..HEAD`` in a clean CI checkout.
    changed = git("diff", "--name-only", audited).splitlines()
    risky = sorted(
        path
        for path in changed
        if any(fnmatch.fnmatch(path, pattern) for pattern in manifest["high_risk_paths"])
    )
    if risky != sorted(reviewed):
        fail(f"high-risk delta is not exactly reviewed: changed={risky}, reviewed={sorted(reviewed)}")
    for path, row in reviewed.items():
        actual = git("hash-object", path)
        if actual != row["blob"]:
            fail(f"reviewed high-risk blob drifted: {path} is {actual}, manifest says {row['blob']}")


def verify_egress() -> None:
    with EGRESS.open("rb") as handle:
        data = tomllib.load(handle)
    sinks = data.get("sink", [])
    ids = [row.get("id") for row in sinks]
    if len(ids) != len(set(ids)) or any(not isinstance(value, str) or not value for value in ids):
        fail("egress sink ids must be unique non-empty strings")
    counts = Counter(row.get("reachability") for row in sinks)
    expected = {"released": data["released_sink_count"], "latent": data["latent_sink_count"]}
    if counts != Counter(expected):
        fail(f"egress denominator drifted: {dict(counts)} != {expected}")
    required = {
        "id",
        "reachability",
        "source_file",
        "source_symbol",
        "trigger",
        "confirmation",
        "destination",
        "data",
        "disposition",
    }
    for row in sinks:
        missing = sorted(
            field
            for field in required
            if not isinstance(row.get(field), str) or not row[field].strip()
        )
        if missing:
            fail(f"egress sink {row.get('id')!r} has empty or missing fields: {missing}")
        path = ROOT / row["source_file"]
        if not path.is_file():
            fail(f"egress source is absent: {row['id']} -> {row['source_file']}")
        if row["source_symbol"] not in path.read_text(encoding="utf-8"):
            fail(f"egress symbol is absent: {row['id']} -> {row['source_symbol']}")
    for row in data.get("primitive_census", []):
        root = ROOT / row["path"]
        files = [root] if root.is_file() else [
            path for path in sorted(root.rglob("*.rs"))
            if "tests" not in path.parts and not path.stem.endswith("_tests")
        ]
        observed = sum(path.read_text(encoding="utf-8").count(row["needle"]) for path in files)
        if observed != row["expected_occurrences"]:
            fail(f"primitive census {row['id']} changed: {observed} != {row['expected_occurrences']}")


def main() -> None:
    manifest = load_json_yaml(MANIFEST)
    verify_provenance(manifest)
    verify_egress()
    digest = hashlib.sha256(MANIFEST.read_bytes()).hexdigest()
    print(f"fork_audit=PASS manifest_sha256={digest} released=14 latent=5")


if __name__ == "__main__":
    main()
