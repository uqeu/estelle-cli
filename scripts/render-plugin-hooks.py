#!/usr/bin/env python3
"""Render the marketplace hook bundle from the Rust ``HOOK_TABLE`` owner.

The published bundle is a derived artifact.  The only editable contract is
``tui/src/top_level.rs::HOOK_TABLE``; this script deliberately contains no hook
rows of its own.  ``--check`` is the CI/read-back path and ``--write`` is the
release-time renderer.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
OWNER = ROOT / "tui" / "src" / "top_level.rs"
BUNDLE = ROOT / "estelle-plugin" / "hooks" / "hooks.json"
RUNNER = "npx -y @fatelabs/estelle@0"
MAX_ROWS = 64

ROW = re.compile(
    r"HookRow\s*\{\s*"
    r'event:\s*"([^"]+)",\s*'
    r'matcher:\s*(?:None|Some\("([^"]*)"\)),\s*'
    r'mode:\s*"([^"]+)",\s*'
    r"timeout:\s*(\d+),\s*"
    r"claude_async:\s*(true|false),?\s*"
    r"\}",
    re.S,
)


def owner_rows(source: str) -> list[tuple[str, str | None, str, int, bool]]:
    """Parse only the owner literal, never similarly shaped test fixtures."""
    start = source.find("const HOOK_TABLE")
    if start < 0:
        raise ValueError(f"{OWNER} has no const HOOK_TABLE")
    end = source.find("];", start)
    if end < 0:
        raise ValueError(f"{OWNER} HOOK_TABLE has no closing ];")
    rows = [
        (event, matcher or None, mode, int(timeout), is_async == "true")
        for event, matcher, mode, timeout, is_async in ROW.findall(source[start:end])
    ]
    if not 8 <= len(rows) <= MAX_ROWS:
        raise ValueError(f"parsed {len(rows)} owner rows; expected 8..{MAX_ROWS}")
    if len(set(rows)) != len(rows):
        raise ValueError("HOOK_TABLE contains duplicate rows")
    return rows


def render(rows: list[tuple[str, str | None, str, int, bool]]) -> str:
    hooks: dict[str, list[dict]] = {}
    for event, matcher, mode, timeout, is_async in rows:
        handler: dict[str, object] = {
            "type": "command",
            "command": f"{RUNNER} hook {mode} --event {event}",
            "timeout": timeout,
            "statusMessage": f"Estelle {mode}",
        }
        if is_async:
            handler["async"] = True
        group: dict[str, object] = {"hooks": [handler]}
        if matcher is not None:
            group["matcher"] = matcher
        hooks.setdefault(event, []).append(group)
    document = {
        "description": (
            "Estelle — memory + the grounding gate, always on. GENERATED from "
            "tui/src/top_level.rs::HOOK_TABLE by scripts/render-plugin-hooks.py; "
            "edit the owner and regenerate, never this file."
        ),
        "hooks": hooks,
    }
    return json.dumps(document, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true")
    action.add_argument("--write", action="store_true")
    args = parser.parse_args()

    expected = render(owner_rows(OWNER.read_text(encoding="utf-8")))
    if args.write:
        BUNDLE.write_text(expected, encoding="utf-8")
        print(f"wrote {BUNDLE}")
        return 0
    actual = BUNDLE.read_text(encoding="utf-8")
    if actual != expected:
        print(
            f"{BUNDLE} drifted from {OWNER} HOOK_TABLE; run "
            "scripts/render-plugin-hooks.py --write",
            file=sys.stderr,
        )
        return 1
    rows = owner_rows(OWNER.read_text(encoding="utf-8"))
    print(f"plugin hook bundle matches Rust owner exactly: {len(rows)} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
