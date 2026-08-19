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
MCP_SERVER_NAME = "estelle"
MCP_URL = "https://api.fatelabs.ca/mcp"
REPOSITORY = "https://github.com/uqeu/estelle-cli"

#: 🔴 TWO IDENTIFIERS, AND THIS REPO USED TO CONFLATE THEM INTO ONE WRONG STRING.
#:
#: It asserted the installed name was `plugin:<marketplace>:<plugin>` = `plugin:fatelabs:estelle`,
#: reasoning from `plugin:stripe:stripe` in a live /mcp listing. That example cannot distinguish the
#: readings, because Stripe's marketplace, plugin and server are all called "stripe".
#:
#: MEASURED 2026-08-18 by installing this bundle into a HOME with no prior config
#: (marketplace `fatelabs`, plugin `estelle`, server `estelle`):
#:     claude plugin install  ->  estelle@fatelabs          <- <plugin>@<marketplace>
#:     claude mcp list        ->  plugin:estelle:estelle    <- plugin:<plugin>:<server>
#: The marketplace name does NOT appear in the MCP name, and `plugin:fatelabs:estelle` appears
#: nowhere at all. Only a real install could tell these apart, which is why a pin written from an
#: ambiguous example held a false value until someone ran it.
INSTALL_ID = f"{PLUGIN_NAME}@{MARKETPLACE_NAME}"
MCP_NAME = f"plugin:{PLUGIN_NAME}:{MCP_SERVER_NAME}"

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
hooks_path = PLUGIN / "hooks" / "hooks.json"
owner_version = workspace_version()

# ── identity ──────────────────────────────────────────────────────────────────
check("plugin name is the pinned skill namespace",
      manifest["name"] == PLUGIN_NAME, f"{manifest['name']!r} != {PLUGIN_NAME!r}")
check("marketplace name is pinned",
      marketplace["name"] == MARKETPLACE_NAME, f"{marketplace['name']!r} != {MARKETPLACE_NAME!r}")
check("README states the install id", INSTALL_ID in readme)
check("README states the MCP name", MCP_NAME in readme)
# The README is where the correction is EXPLAINED, so it must be allowed to quote the false string.
# What must never carry it is a file that DEFINES identity, and the README must keep the correction
# rather than quietly dropping it and leaving the old claim to creep back.
_identity_files = json.dumps(manifest) + json.dumps(marketplace) + json.dumps(mcp)
check("no identity file carries the falsified plugin:fatelabs:estelle",
      "plugin:fatelabs:estelle" not in _identity_files)
# Whitespace-normalised: the phrase legitimately wraps across lines in Markdown, and a check that
# breaks on a re-wrap is a check that will be deleted rather than satisfied.
_readme_flat = " ".join(readme.split())
check("README keeps the correction rather than silently dropping it",
      "appears nowhere at all" in _readme_flat)

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

# ── the marketplace package must include the always-on hook half ──────────────
check("published plugin contains generated hooks/hooks.json", hooks_path.is_file())
if hooks_path.is_file():
    hooks = load(hooks_path)
    check("hook package is labelled GENERATED", "GENERATED" in hooks.get("description", ""))
    check("hook package covers every supported Claude event",
          set(hooks.get("hooks", {})) == {
              "PostToolUse", "PreCompact", "PreToolUse", "SessionEnd",
              "SessionStart", "Stop", "UserPromptSubmit",
          }, str(sorted(hooks.get("hooks", {}))))

# ── the server entry is the HOSTED one, and carries no credential ─────────────
servers = mcp["mcpServers"]
check("one server, with the pinned name", list(servers) == [MCP_SERVER_NAME], str(list(servers)))
# Read it back defensively: a renamed key must produce a NAMED clause failure, not a KeyError that
# aborts the run before the remaining clauses are ever evaluated. A guard that crashes reports
# "something broke"; a guard that fails reports WHICH promise broke.
entry = servers.get(MCP_SERVER_NAME) or {}
check("server is remote http", entry.get("type") == "http", str(entry.get("type")))
check("server url is the hosted endpoint", entry.get("url") == MCP_URL, str(entry.get("url")))
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

print(f"✅ identity and version agreement hold — install {INSTALL_ID}, MCP {MCP_NAME}, v{owner_version}")
