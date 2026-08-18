#!/usr/bin/env python3
"""🔴 THE INSTALLED NAME AND THE VERSION ARE PINNED HERE, NOT REMEMBERED.

`plugin:<marketplace>:<plugin>` — the same shape as `plugin:stripe:stripe` in a live `/mcp` listing. So
`plugin:fatelabs:estelle` is TWO fields in TWO files, and `name` is also the SKILL NAMESPACE: every
playbook becomes `/estelle:<name>`. Renaming it after the first publish renames every command a customer
has learned.

⚠️ AND FOUR FILES STATE A VERSION. Before this guard existed, three of them disagreed: the plugin manifest
said 0.1.0 while the CLI shipped 0.2.20. A derived fact with four writers and no reader is a fact that has
already drifted — you just have not looked yet. `release.yml` refuses a tag that disagrees with any of
them; this runs the same comparison without needing a tag.

Exit 0 = every clause holds. Exit 1 = a named clause failed, with the two values printed.
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
PLUGIN = ROOT / "estelle-plugin"

#: WRITTEN OUT, not derived. A value read from the file it is checking agrees with that file by
#: construction and could never catch a rename.
PLUGIN_NAME = "estelle"
MARKETPLACE_NAME = "fatelabs"
INSTALLED_NAME = f"plugin:{MARKETPLACE_NAME}:{PLUGIN_NAME}"
MCP_URL = "https://api.fatelabs.ca/mcp"
REPOSITORY = "https://github.com/uqeu/estelle-cli"

failures: list[str] = []


def check(clause: str, ok: bool, detail: str = "") -> None:
    if not ok:
        failures.append(f"{clause}{': ' + detail if detail else ''}")


def load(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def workspace_version() -> str:
    """The OWNER of the version. Everything else is a copy that must match it."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    block = re.search(r"^\[workspace\.package\]$(.*?)^\[", text, re.M | re.S)
    assert block, "Cargo.toml has no [workspace.package] block"
    found = re.search(r'^version\s*=\s*"([^"]+)"', block.group(1), re.M)
    assert found, "[workspace.package] has no version"
    return found.group(1)


manifest = load(PLUGIN / ".claude-plugin" / "plugin.json")
marketplace = load(ROOT / ".claude-plugin" / "marketplace.json")
mcp = load(PLUGIN / ".mcp.json")
shim = load(ROOT / "npm-shim" / "package.json")
readme = (PLUGIN / "README.md").read_text(encoding="utf-8")
owner_version = workspace_version()

# ── identity ──────────────────────────────────────────────────────────────────
check("plugin name is the pinned skill namespace",
      manifest["name"] == PLUGIN_NAME, f"{manifest['name']!r} != {PLUGIN_NAME!r}")
check("marketplace name is pinned",
      marketplace["name"] == MARKETPLACE_NAME, f"{marketplace['name']!r} != {MARKETPLACE_NAME!r}")
check("README states the installed name", INSTALLED_NAME in readme)

# ── the manifest must point at a repo that EXISTS and is PUBLIC ───────────────
# It declared https://github.com/fatelabs/estelle, which 404s. `uqeu/estelle` is private, so a
# marketplace listing can only be served from the public CLI repo.
check("manifest repository is the public repo",
      manifest.get("repository") == REPOSITORY, f"{manifest.get('repository')!r} != {REPOSITORY!r}")
check("no doc points at the nonexistent fatelabs/estelle repo",
      "github.com/fatelabs/estelle" not in json.dumps(manifest))

# ── the marketplace must actually resolve this plugin ─────────────────────────
entries = marketplace.get("plugins", [])
check("marketplace lists exactly one plugin", len(entries) == 1, f"{len(entries)} entries")
if entries:
    entry = entries[0]
    check("marketplace entry names the plugin", entry.get("name") == PLUGIN_NAME)
    source = entry.get("source", "")
    check("marketplace source resolves to a real directory",
          (ROOT / source).resolve() == PLUGIN.resolve(), f"source={source!r}")
    check("marketplace source contains the manifest",
          (ROOT / source / ".claude-plugin" / "plugin.json").is_file())

# ── layout: a stray file here is the top reason `claude plugin validate` fails ─
inside = sorted(p.name for p in (PLUGIN / ".claude-plugin").iterdir())
check("plugin.json is the ONLY file in .claude-plugin/", inside == ["plugin.json"], str(inside))

# ── the server entry is the HOSTED one, and carries no credential ─────────────
servers = mcp["mcpServers"]
check("one server, named for the plugin", list(servers) == [PLUGIN_NAME], str(list(servers)))
entry = servers[PLUGIN_NAME]
check("server is remote http", entry.get("type") == "http")
check("server url is the hosted endpoint", entry.get("url") == MCP_URL)
check("authorization EXPANDS from the environment, never a literal key",
      entry.get("headers", {}).get("Authorization") == "Bearer ${ESTELLE_API_KEY}")
check("nothing unexplained rides along", set(entry) == {"type", "url", "headers"}, str(set(entry)))
check("no live key value is committed",
      "estelle_live_" not in json.dumps(mcp) and "estelle_live_" not in json.dumps(manifest))

# ── ONE OWNER PER DERIVED FACT: four copies of the version must agree ─────────
for label, value in (
    ("plugin.json", manifest.get("version")),
    ("marketplace.json entry", entries[0].get("version") if entries else None),
    ("npm-shim/package.json", shim.get("version")),
):
    check(f"version agrees with Cargo.toml workspace ({label})",
          value == owner_version, f"{label}={value!r} but Cargo.toml={owner_version!r}")

if failures:
    print(f"🔴 PLUGIN IDENTITY/VERSION GUARD FAILED — {len(failures)} clause(s):", file=sys.stderr)
    for f in failures:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)

print(f"✅ plugin identity and version agreement hold ({INSTALLED_NAME} @ {owner_version})")
