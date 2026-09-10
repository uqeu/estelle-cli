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

import hashlib
import json
import pathlib
import re
import subprocess
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

#: The plugin door's runner, WRITTEN OUT so this guard is a second opinion rather than an echo of
#: `PLUGIN_HOOK_RUNNER` in `tui/src/top_level.rs`. It was `npx -y @fatelabs/estelle@0` until
#: 2026-09-06, which npm satisfies from a copy already on the customer's disk — measured, twice,
#: with sentinel versions. See that constant's docstring for the table.
PLUGIN_HOOK_RUNNER = "npx -y --package=@fatelabs/estelle@latest -- estelle"

# Ported from vercel-labs/fx `src/builtins/tools.zig:992-1025` at
# 19ae8f5401c734806d3df45e7430c34dfa159bd0: hash the complete model-facing
# contract, including ordering, instead of trusting a count. Claude Code keys
# its copied plugin directory by version, so every byte that ships under
# `estelle-plugin/` plus the marketplace entry is one cache contract here.
# A new version must add a new digest; never rewrite an existing version's
# digest to bless changed bytes under a cache key customers already hold.
PLUGIN_CONTRACT_SHA256_BY_VERSION = {
    "0.2.31": "9d806279081abab96dc19d3aaae4a4c84f955df0458a5668afcf31f5c21ad472",
    "0.2.32": "c811c073c806387d49b310b0be98cdc0a5be07eadf26e12141632258fe3b3f5d",
    # 0.2.33 is NOT PUBLISHED. Its digest moved when the manifest description was corrected and
    # the bundle was regenerated from the Rust owner, which is exactly what an unreleased version's
    # digest is for. The two above it are published and are byte-untouched.
    "0.2.33": "4ff63c24250febb7c079a15570e177ec86b5341659f6725568f82133583fff0a",
    # 0.3.0 consolidates every CLI improvement onto one release (founder, 2026-09-04). The digest
    # moved because the four version writers moved: `estelle-plugin/.claude-plugin/plugin.json`
    # and the marketplace entry both carry the version, and both are inside this contract.
    # `estelle-plugin/hooks/hooks.json` is byte-unchanged — HOOK_TABLE was not touched.
    "0.3.0": "75f530879a621faaa93960b0219fc194a78ace7442c8897db841518255822e95",
    # 0.3.1 is 0.3.0 plus the merge of `fix/hook-never-fails-silent`. Inside THIS contract
    # exactly two bytes-bearing facts moved, both of them the version itself:
    # `estelle-plugin/.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json`
    # 0.3.0 -> 0.3.1. `estelle-plugin/hooks/hooks.json`, `.mcp.json` and `README.md` are
    # BYTE-IDENTICAL to 0.3.0 (`git diff origin/main -- .claude-plugin/ estelle-plugin/`
    # is 2 files, 2 insertions, 2 deletions). HOOK_TABLE was not touched by the merge; the
    # runner change is on the `install-hooks` door only (PORTABLE_HOOK_RUNNER), which is
    # NOT part of the shipped plugin bundle.
    "0.3.1": "f422f956a2b3c07fad287082bfb79494c02f717ddba074d6157df5e8bc1b30e1",
    # 0.3.2 merges the Codex branches: the rollout parser (`host_transcript`), the nullable
    # `transcript_path` that failed all eight hook verbs in Codex, the SessionEnd budget, and the
    # untrusted-hook discovery guard.
    # 🔴 UNLIKE 0.3.1, THE VERSION IS **NOT** THE ONLY BYTE-BEARING FACT THAT MOVED. `hooks.json`
    # changed too, and it is a customer-visible behaviour change: SessionEnd `checkpoint` 30s -> 3s
    # and SessionStart `welcome` 5s -> 30s. The host only ever GRANTED SessionEnd 3 seconds, so
    # asking for 30 was a claim the platform never honoured and nothing checkpointed; the real work
    # is now the `welcome` drain, measured at 19.0s for one 484KB Codex rollout against a 20s
    # HANDOFF_DRAIN_BUDGET -- which is why that hook needed 30 and not 5.
    # `.mcp.json` and `README.md` ARE byte-identical to 0.3.1 (verified with git diff, not assumed).
    "0.3.2": "9a2c0503a2f14b7546fb942299cd196a65f62741ac8204bd8d68abd114d04b66",
    # 0.3.3 changes the RUNNER on all nine plugin-door rows, and nothing else:
    # `npx -y @fatelabs/estelle@0 hook <verb>` -> `npx -y --package=@fatelabs/estelle@latest --
    # estelle hook <verb>`. Every timeout, matcher and async marker is byte-unchanged.
    #
    # 🔴 THE OLD STRING NEVER REACHED THE REGISTRY WHEN THE CUSTOMER ALREADY HAD A COPY. `@0` is
    # the range `>=0.0.0 <1.0.0`, and npm SATISFIES a range from disk instead of fetching.
    # Measured 2026-09-06 in isolated npm prefixes and caches, with sentinel versions that exist
    # nowhere in the registry, while the registry was at 0.3.2:
    #   global @fatelabs/estelle@0.0.1 installed  -> `npx -y @fatelabs/estelle@0` ran 0.0.1
    #   ./node_modules/@fatelabs/estelle@0.0.2    -> `npx -y @fatelabs/estelle@0` ran 0.0.2
    # So a customer froze on whatever build they had, permanently and silently, and no publish
    # could move them. `--package=` alone is NOT the repair: it escapes the global shadow and is
    # still answered by ./node_modules (measured). The DIST-TAG is what forces the registry --
    # a tag has no on-disk meaning, proven by an ETARGET on a tag that does not exist.
    #
    # ⚠️ CUSTOMER COST, STATED RATHER THAN HIDDEN: Codex hashes the raw command into the hook's
    # trust identity, so all nine doors become `Modified` and are DISCOVERED BUT NOT RUN until
    # the customer clears them once with `/hooks`. That is one action per customer per host.
    "0.3.3": "cb82420b8474315632e39fa83dd7d94b06ef97c76b77ead233fa6a18d4841b4b",
    # 0.3.4 adds TWO ROWS to the plugin door and nothing else inside this contract:
    #   SubagentStart -> `hook context`    (timeout 30, not async)
    #   SubagentStop  -> `hook checkpoint` (timeout 30, async)
    # plus the version itself in `estelle-plugin/.claude-plugin/plugin.json` and the marketplace
    # entry, and one corrected sentence in the manifest description (it still claimed the runner was
    # "pinned to a major (ADR 0015)" three releases after the runner became a dist-tag).
    # Every existing row's command, timeout, matcher and async marker is BYTE-UNCHANGED.
    #
    # 🔴 WHY THE TWO ROWS. Measured 2026-09-10 against the published `@fatelabs/estelle@0`: a real
    # `SubagentStart` payload produced ONE BYTE and exit 0, while `UserPromptSubmit` produced 11202
    # from the same binary in the same invocation. A subagent received no repository grounding at
    # all, and the runner reported success. `hook context` now answers the event from a cache the
    # parent's own turn writes, with NO request and NO concurrency slot — twelve subagents each
    # issuing their own `/search` is the recorded 429 that starves the whole account.
    #
    # ⚠️ CUSTOMER COST, STATED RATHER THAN HIDDEN: Codex hashes each hook's command into its trust
    # identity, so the two NEW doors arrive untrusted and are discovered-but-not-run until the
    # customer clears them once with `/hooks`. The nine existing rows are byte-identical and keep
    # the trust they already have.
    "0.3.4": "f83a38978c3facb3dd5524152badaee5fb5972609ff5553cc0f0ff156433926c",
    # 0.3.5 changes THE VERSION STRING AND NOTHING ELSE inside this contract. Verified rather than
    # asserted: `git diff v0.3.4 HEAD -- .claude-plugin/ estelle-plugin/` is exactly two lines,
    # `"version": "0.3.4"` -> `"0.3.5"` in `.claude-plugin/marketplace.json` and
    # `estelle-plugin/.claude-plugin/plugin.json`. Every hook row's event, command, matcher, timeout
    # and async marker is BYTE-UNCHANGED, so no door arrives untrusted and nobody re-clears `/hooks`.
    #
    # ⚠️ THE FIRST COMPARISON I RAN WAS VACUOUS AND SAID "NOTHING CHANGED". `v0.3.4` was not fetched
    # in that checkout, so the diff resolved against nothing and returned empty — the reassuring
    # answer, for the wrong reason. The tag is fetched and resolves to 93dbaa630c53 above.
    #
    # 🔴 WHAT THE RELEASE CARRIES, all of it in the RUNNER rather than this contract: `SubagentStop`
    # re-checkpointed the PARENT's transcript and dropped the subagent's own work. The host has
    # always sent `agent_transcript_path` on that event; `HookPayload` modelled eight fields and none
    # of the three subagent ones, so nothing ever read it. Measured on the founder's machine
    # 2026-09-10: 3,086 subagent transcripts on disk, 0 of 254 session rows naming one.
    "0.3.5": "8321df873b215bcd7e6478cd3b0507150b60e4db0ad931c237353a1778bfd2db",
}

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


def plugin_contract_digest() -> tuple[str, list[str]]:
    paths = [ROOT / ".claude-plugin" / "marketplace.json"]
    paths.extend(sorted(path for path in PLUGIN.rglob("*") if path.is_file()))
    digest = hashlib.sha256()
    names: list[str] = []
    for path in paths:
        name = path.relative_to(ROOT).as_posix()
        payload = path.read_bytes()
        names.append(name)
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name.encode("utf-8"))
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest(), names


manifest = load(PLUGIN / ".claude-plugin" / "plugin.json")
marketplace = load(ROOT / ".claude-plugin" / "marketplace.json")
mcp = load(PLUGIN / ".mcp.json")
shim = load(ROOT / "npm-shim" / "package.json")
readme = (PLUGIN / "README.md").read_text(encoding="utf-8")
hooks_path = PLUGIN / "hooks" / "hooks.json"
owner_version = workspace_version()

# 🔴 THERE IS ONE OWNER OF THIS BUNDLE AND IT IS THE RUST TABLE, NOT A SECOND GENERATOR.
#
# This used to shell out to scripts/render-plugin-hooks.py, a Python re-implementation of the
# renderer.  Two generators of one derived fact will disagree, and these did, in four ways at
# once: the Python one emitted `hook <mode> --event <Event>` where the Rust one emits `hook
# <mode>`, wrote `Estelle <mode>` where Rust writes `Estelle hook <mode>`, took its async marker
# from `claude_async` instead of `plugin_async`, and ignored the `plugin` column entirely, so it
# shipped the `shift` row that the Rust owner marks `plugin: false` with a written reason.  It
# also could not parse the table at all any more: its regex ended at `claude_async` and the struct
# has carried `plugin` and `plugin_async` since.  It has been deleted.
#
# The byte-for-byte check now lives where the owner lives:
# `the_plugin_manifest_is_generated_from_the_one_hook_table` in tui/src/top_level.rs, which
# renders HOOK_TABLE and compares it to this file with `include_str!`.  What is left here is the
# half a Rust test cannot do — the per-version SHA-256 cache contract, below.

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
    # 🔴 NINE EVENTS SINCE v0.3.4, AND THE TWO NEW ONES ARE THE SUBAGENT DOORS. `Stop` fires for the
    # main thread only, and `UserPromptSubmit` fires on the HUMAN's prompt — a subagent is spawned by
    # the parent's Task tool and never submits one — so before those rows a subagent started with no
    # grounding and finished checkpointing nothing, on a session where subagents did the engineering.
    # The set is WRITTEN OUT rather than derived: a roster read off the file it is checking agrees
    # with that file by construction and could never catch a dropped door.
    check("hook package covers every supported Claude event",
          set(hooks.get("hooks", {})) == {
              "PostToolUse", "PreCompact", "PreToolUse", "SessionEnd",
              "SessionStart", "Stop", "SubagentStart", "SubagentStop", "UserPromptSubmit",
          }, str(sorted(hooks.get("hooks", {}))))
    # ⚠️ A DICT KEYED BY COMMAND SILENTLY COLLAPSES ROWS. `checkpoint` is registered on Stop,
    # PreCompact and SessionEnd with the SAME command string, so a dict turns three handlers into
    # one and any count taken off it is wrong. Keep the handlers as a list and index separately.
    handlers = [
        hook
        for event in hooks.get("hooks", {}).values()
        for matcher in event
        for hook in matcher.get("hooks", [])
        if hook.get("command")
    ]
    commands = {hook["command"]: hook for hook in handlers}
    # ELEVEN handlers since v0.3.4 (nine through v0.3.3). The count is asserted rather than the
    # membership because a replacement would keep the count and change the row.
    check("shipping hook bundle has the eleven plugin-door rows", len(handlers) == 11,
          str(len(handlers)))
    # ⚠️ A DECLARED EXEMPTION, ASSERTED AS AN ABSENCE. `shift` fires on every Read. The Rust owner
    # marks it `plugin: false` because adding it is a product decision with a release attached,
    # not a drift fix. A lane regenerated this file WITH it; that is why the absence is now a
    # clause instead of a silence.
    check("shipping hook bundle does NOT carry the shift row",
          not any(" hook shift" in command for command in commands),
          str(sorted(commands)))
    # ⚠️ THESE ARE A SECOND, INDEPENDENT STATEMENT OF `HOOK_TABLE`, AND THEY MOVE DELIBERATELY WITH IT.
    # `welcome` was 5 until 2026-09-05, when `welcome` took on the upload that `SessionEnd` can no longer
    # perform inside Codex's three-second clamp; its ceiling is now 30 with `HANDOFF_DRAIN_BUDGET` (20s)
    # bounding the work underneath. `checkpoint` is deliberately absent from this list: it appears on three
    # events with two different budgets (30 on Stop/PreCompact, 3 on SessionEnd), so a single expected
    # value here would be a claim that is false on one of them.
    for mode, expected in (("ground", 30), ("sync", 30), ("context", 30),
                           ("guard", 10), ("distil", 10), ("welcome", 30)):
        command = f"{PLUGIN_HOOK_RUNNER} hook {mode}"
        matching = [h for c, h in commands.items() if c == command]
        check(f"shipping timeout for {mode} is {expected}s",
              bool(matching) and all(h.get("timeout") == expected for h in matching),
              f"{command!r} -> {[h.get('timeout') for h in matching]!r}")

    # 🔴 NO SHIPPED COMMAND MAY BE ANSWERABLE FROM THE CUSTOMER'S DISK.
    #
    # A SECOND, INDEPENDENT statement of the Rust guard
    # (`no_shipped_hook_command_can_resolve_to_a_disk_local_binary`), and deliberately so: the
    # Rust one reads the same bytes through `include_str!`, so if the renderer and the guard ever
    # share a mistake they agree with each other. This one parses the shipped JSON from disk and
    # knows nothing about `HOOK_TABLE`.
    #
    # MEASURED 2026-09-06 with sentinel versions that exist nowhere in the registry:
    # `npx -y @fatelabs/estelle@0` ran a global `0.0.1` and a `./node_modules` `0.0.2` while the
    # registry was at `0.3.2`. A semver RANGE is a satisfaction test against the disk; a DIST-TAG
    # has no on-disk meaning, so npm must ask the registry (proven by `ETARGET` on a tag that does
    # not exist). The clause is therefore about the SHAPE of the version part, not its value.
    for command in sorted(commands):
        tokens = command.split()
        specs = [t[len("--package="):] for t in tokens if t.startswith("--package=")]
        version = specs[0].rsplit("@", 1)[-1] if specs and "@" in specs[0][1:] else ""
        is_tag = bool(version) and version[0].isalpha() and not re.match(r"^v\d", version)
        check(f"shipped command cannot resolve to a disk-local binary ({tokens[-1]})",
              tokens[:1] == ["npx"] and bool(specs) and is_tag
              and "--" in tokens and "--ignore-existing" not in tokens,
              f"{command!r} (version part {version!r})")

# ── the server entry is the HOSTED one, and carries no credential ─────────────
servers = mcp["mcpServers"]
check("one server, with the pinned name", list(servers) == [MCP_SERVER_NAME], str(list(servers)))
# Read it back defensively: a renamed key must produce a NAMED clause failure, not a KeyError that
# aborts the run before the remaining clauses are ever evaluated. A guard that crashes reports
# "something broke"; a guard that fails reports WHICH promise broke.
entry = servers.get(MCP_SERVER_NAME) or {}
check("server is remote http", entry.get("type") == "http", str(entry.get("type")))
check("server url is the hosted endpoint", entry.get("url") == MCP_URL, str(entry.get("url")))
# Public main removed `Authorization: Bearer ${ESTELLE_API_KEY}` in 2ee1454c4:
# Claude's GUI installer does not run a shell, so it sent the placeholder as a
# literal credential and broke OAuth onboarding. The guard must pin the fixed
# credential-free door, not demand the defect that the manifest removed.
check("the door carries NO credential — a ${VAR} a GUI cannot expand is worse than none",
      "headers" not in entry, str(sorted(entry)))
check("nothing unexplained rides along", set(entry) == {"type", "url"}, str(set(entry)))
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

# ── THE VERSION IS A CACHE KEY: changed bytes require a new version ──────────
contract_digest, contract_files = plugin_contract_digest()
expected_contract_digest = PLUGIN_CONTRACT_SHA256_BY_VERSION.get(owner_version)
check("current version has a pinned whole-plugin contract digest",
      expected_contract_digest is not None,
      f"no digest registered for v{owner_version}")
check("whole shipping plugin contract matches the digest pinned for its version",
      contract_digest == expected_contract_digest,
      f"v{owner_version} expected {expected_contract_digest!r}, got {contract_digest}; "
      f"{len(contract_files)} files hashed")

if failures:
    print(f"🔴 PLUGIN IDENTITY/VERSION GUARD FAILED — {len(failures)} clause(s):", file=sys.stderr)
    for f in failures:
        print(f"  - {f}", file=sys.stderr)
    sys.exit(1)

print(f"✅ identity and version agreement hold — install {INSTALL_ID}, MCP {MCP_NAME}, v{owner_version}")
