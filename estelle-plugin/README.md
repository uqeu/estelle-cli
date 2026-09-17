# The Estelle plugin for Claude Code

The Claude Code plugin that ships Estelle's always-on hooks, skills and agents. **It does not ship an
MCP server** — the server is one user-scope entry named `Estelle`, written by `estelle init`. See
[why](#-why-this-plugin-stopped-shipping-an-mcp-server-037).

## 🔴 THREE DOORS, IN THIS ORDER. Pick the first one that fits.

Until now nobody could tell which to use, so here is the whole story in one place. All three end at the
same hosted server — `https://api.fatelabs.ca/mcp` — and none of them runs Estelle on your machine.

| # | door | command | when |
|---|---|---|---|
| **1** | **`estelle init`** — *the path* | `estelle init` | **Default.** Writes the MCP config for every editor you have installed — Claude Code, Cursor, Cline, Windsurf, JetBrains, VS Code. Claude Code gets a **user-scope** entry named **`Estelle`**. |
| **2** | **the remote URL** — *the manual fallback* | `claude mcp add --scope user --transport http Estelle https://api.fatelabs.ca/mcp --header 'Authorization: Bearer ${ESTELLE_API_KEY}'` | When you want to write the entry yourself, script it, or you are not installing the CLI. **Single quotes matter** — the `${...}` must reach the config unexpanded. |
| **3** | **this plugin** — *hooks, skills, agents* | `/plugin marketplace add uqeu/estelle-cli` then `/plugin install estelle@fatelabs` | Claude Code only. Ships the always-on hooks, 23 skills and 4 agents. **Since 0.3.7 it ships no MCP server**, so it is not a substitute for door 1 or 2. |

**Door 1 is the recommendation.** Door 2 is what door 1 writes for you, and it is verified working
end-to-end: a valid key returns a full `initialize` with tools and prompts, and a bogus key returns
`-32001`. **Door 3 is no longer a third copy of that entry** — it is the hooks/skills/agents half, and
it needs door 1 or door 2 beside it.

Install the CLI (doors 1 and 3 both assume it for real work):

```sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh
```

## 🔴 THERE ARE TWO IDENTIFIERS, AND WE HAD WRITTEN DOWN A THIRD THAT DOES NOT EXIST

This repo asserted the installed name was `plugin:<marketplace>:<plugin>` = `plugin:fatelabs:estelle`,
reasoning from `plugin:stripe:stripe` in a live `/mcp` listing. **That example cannot distinguish the
readings**, because Stripe's marketplace, plugin and MCP server are all called `stripe`. Ours are not,
so installing it settled the question.

**MEASURED 2026-08-18**, installing this bundle into a `HOME` with no prior config:

| | value | shape |
|---|---|---|
| install id (`claude plugin install`) | **`estelle@fatelabs`** | `<plugin>@<marketplace>` |
| MCP server (`claude mcp list`), **through 0.3.6** | **`plugin:estelle:estelle`** | `plugin:<plugin>:<server>` |
| MCP server (`claude mcp list`), **0.3.7 onward** | **`Estelle`** | a user-scope entry; no plugin prefix exists to strip |

**The marketplace name never appears in the MCP name, and `plugin:fatelabs:estelle` appears nowhere at
all.** A pin written from an ambiguous example held a false value until someone ran it.

| file | field | value |
|---|---|---|
| `estelle-plugin/.claude-plugin/plugin.json` | `name` | **`estelle`** |
| ~~`estelle-plugin/.mcp.json`~~ | ~~server key~~ | **deleted in 0.3.7** |
| `~/.claude.json` (user scope, written by `estelle init`) | server key | **`Estelle`** |
| `.claude-plugin/marketplace.json` (this repo's ROOT) | `name` | **`fatelabs`** |

⚠️ **The plugin `name` is also the SKILL NAMESPACE.** Every playbook becomes `/estelle:<name>`.
Changing it later renames every command a customer has learned, so all three are pinned by
`scripts/test-plugin-identity.py` rather than left to a careful reader.

## 🔴 WHY THIS LIVES IN `uqeu/estelle-cli` AND NOT WHERE THE DOCS USED TO SAY

The manifest previously declared `repository: "https://github.com/fatelabs/estelle"`. **That repository
does not exist** — the authenticated GitHub API returns 404 for it. `uqeu/estelle` exists and is
**private**. `uqeu/estelle-cli` is the only PUBLIC repository, so it is the only place a marketplace
listing can live: `/plugin marketplace add` has to clone it as an anonymous user.

The ship-order note that said *"`/plugin marketplace add fatelabs/estelle`"* was therefore describing a
command that could never have worked. The marketplace **name** is still `fatelabs` — that is a field
inside `marketplace.json`, independent of the repo path a user types — so the install id
`estelle@fatelabs` is unchanged.

**PROVEN FROM A CLEAN MACHINE**, not from this laptop: in a `HOME` with no `~/.claude.json` (so the
founder's existing working remote entry could not mask a broken plugin),
`claude plugin marketplace add uqeu/estelle-cli` cloned anonymously over HTTPS and validated,
`claude plugin install estelle@fatelabs` installed at `gitCommitSha c8ea2ba46`, and `claude mcp list`
registered `plugin:estelle:estelle -> https://api.fatelabs.ca/mcp (HTTP)`. With no key set, the server
answered `-32001 "unknown or missing Estelle API key"` — **its own refusal, which is the proof the door
reaches it**; a broken plugin returns no server at all.

## 🔴 WHY THIS PLUGIN STOPPED SHIPPING AN MCP SERVER (0.3.7)

Because a second server at the same URL was **never a second server** — it was a coin flip over which
one existed, and the two code paths called it differently.

**MEASURED 2026-09-17**, four arms against one isolated `HOME` holding this plugin and nothing else,
each arm differing from the last in one field, `claude mcp list` read after each:

| arm | user-scope entry | url | `plugin:estelle:estelle` in the listing? |
|---|---|---|---|
| **A** | *none* | — | ✅ present — `! Needs authentication` |
| **B** | `Estelle` | **same** | ❌ **gone** |
| **C** | `EstelleUserScope` | **same** | ❌ **gone** |
| **D** | `Estelle` | *different* | ✅ present, alongside `Estelle` |

**The dedup key is the URL, not the name.** C is the arm that proves it: a name that collides with
nothing still erased the plugin's server, and D keeps both under a colliding name. So any customer who
has ever run `estelle init` or `claude mcp add` already had the plugin's copy silently deleted.

⚠️ **AND THE TWO PATHS RESOLVED IT IN OPPOSITE DIRECTIONS, WHICH IS HOW IT HID.** On the founder's
machine `claude mcp list` printed `Estelle` and no plugin row, while a session that had been running
since before the user-scope entry landed still held `mcp__plugin_estelle_estelle__*` tools. A fresh
`claude -p` on the same config answered with `mcp__Estelle__*`. **Two owners of one derived fact, and
the stale one was the one the agents were written against.**

🔴 **THE COST WAS FOUR INERT AGENTS, AND NOTHING WENT RED.** `agents/*.md` pinned
`tools: … mcp__plugin_estelle_estelle__verify, …`. Once the user-scope entry shadowed the plugin's
server those names resolved to nothing, so `estelle-grounder` — the agent whose whole job is to refuse
an ungrounded claim — was left with `Read, Grep, Glob` and **could not ground anything**. It does not
error; it just stops being able to check. *Green over the clause nobody wrote.* The identity guard now
enumerates every `mcp__` token in every shipped agent and fails on any that does not name the pinned
server, which is the clause that would have caught this the day it was written.

**Why the plugin loses rather than the user-scope entry:** the plugin's `.mcp.json` could not carry a
credential. `${ESTELLE_API_KEY}` was removed from it in `2ee1454c4` because Claude's GUI installer does
not run a shell and sent the placeholder as a literal token. A header-less entry falls back to **its own
OAuth session** — a *second identity*, which is exactly the 2026-09-07 split where the hooks wrote 227
sessions to one account while every read tool queried another and saw 4. The user-scope entry binds
`Authorization` to `${ESTELLE_API_KEY}`, **the same variable the hooks read**, so identity cannot drift.
(Claude Code does expand `${VAR}` in a user-scope header — arm B warned `Missing environment variables:
ESTELLE_API_KEY` rather than sending the literal, which is the same mechanism failing *safe*.)

⚠️ **What this costs:** a Claude Code user who installs the plugin and nothing else now has **no MCP
tools** until door 1 or door 2 runs. That is the price of one owner, and `estelle init` pays it in one
command.

## ⚠️ "Local MCPs" in the `/mcp` menu means CONFIG-SCOPED, not locally hosted

This cost a day. Estelle appeared under *"Local MCPs"* and it was read as a hosting problem. It is not:
that heading means **scoped to a project's config**, and we were there only because we had been added
with `claude mcp add` (which writes `~/.claude.json`) instead of `/plugin install`. Our server entry is
already `{"type":"http","url":"https://api.fatelabs.ca/mcp"}` — the same class as Stripe's.

## ✅ STATUS: SHIPS THE GENERATED ALWAYS-ON HOOKS

| | Codex | Claude Code |
|---|---|---|
| manifest | ✅ `.codex-plugin/plugin.json` | ✅ `estelle-plugin/.claude-plugin/plugin.json` |
| server entry | ✅ inline `mcpServers` | ⛔ **deliberately none** — `~/.claude.json` owns it, named `Estelle` |
| agents | — | ✅ 4, and their `tools:` name `mcp__Estelle__*` |
| marketplace listing | — | ✅ `.claude-plugin/marketplace.json` (this repo's root) |
| hooks | ✅ generated | ✅ `estelle-plugin/hooks/hooks.json` |

⛔ **`hooks/hooks.json` is GENERATED, not hand-written.** The hook contract still has one owner: the
installer's hook configuration. The marketplace package carries that generated artifact verbatim, and
the identity guard refuses a package that drops it. `estelle install-hooks` remains door 1 for users
who want the same hooks across editors rather than only inside Claude Code.

## 🔴 The version is not written here twice

`version` in `plugin.json` and in `marketplace.json` must equal the workspace version in `Cargo.toml`
and the `npm-shim/package.json` version. That is four copies of one fact, so it is enforced rather than
remembered: `.github/workflows/release.yml` refuses to cut a release unless the tag matches **all** of
them, and `scripts/test-plugin-identity.py` checks them against each other on every run.

## Developing

```bash
claude --plugin-dir ./estelle-plugin       # load it
/reload-plugins                             # iterate
claude plugin validate ./estelle-plugin     # run before anything ships
python3 scripts/test-plugin-identity.py     # identity + version agreement
```
