# The Estelle plugin for Claude Code

The Claude Code plugin that wraps Estelle's hosted MCP server.

## 🔴 THREE DOORS, IN THIS ORDER. Pick the first one that fits.

Until now nobody could tell which to use, so here is the whole story in one place. All three end at the
same hosted server — `https://api.fatelabs.ca/mcp` — and none of them runs Estelle on your machine.

| # | door | command | when |
|---|---|---|---|
| **1** | **`estelle init`** — *the path* | `estelle init` | **Default.** Writes the MCP config for every editor you have installed, so one command covers Claude Code, Cursor, Cline, Zed and the rest. |
| **2** | **the remote URL** — *the manual fallback* | `claude mcp add --transport http estelle https://api.fatelabs.ca/mcp --header "Authorization: Bearer $ESTELLE_KEY"` | When you want to write the entry yourself, script it, or you are not installing the CLI. |
| **3** | **this plugin** — *a third convenience* | `/plugin marketplace add uqeu/estelle-cli` then `/plugin install estelle@fatelabs` | Claude Code only. Bundles the server entry so a teammate does not have to paste a URL. |

**Door 1 is the recommendation.** Door 2 is what door 1 writes for you, and it is verified working
end-to-end: a valid key returns a full `initialize` with tools and prompts, and a bogus key returns
`-32001`. Door 3 is the same entry, delivered by Claude Code's plugin system.

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
| MCP server (`claude mcp list`) | **`plugin:estelle:estelle`** | `plugin:<plugin>:<server>` |

**The marketplace name never appears in the MCP name, and `plugin:fatelabs:estelle` appears nowhere at
all.** A pin written from an ambiguous example held a false value until someone ran it.

| file | field | value |
|---|---|---|
| `estelle-plugin/.claude-plugin/plugin.json` | `name` | **`estelle`** |
| `estelle-plugin/.mcp.json` | server key | **`estelle`** |
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

## ⚠️ "Local MCPs" in the `/mcp` menu means CONFIG-SCOPED, not locally hosted

This cost a day. Estelle appeared under *"Local MCPs"* and it was read as a hosting problem. It is not:
that heading means **scoped to a project's config**, and we were there only because we had been added
with `claude mcp add` (which writes `~/.claude.json`) instead of `/plugin install`. Our server entry is
already `{"type":"http","url":"https://api.fatelabs.ca/mcp"}` — the same class as Stripe's.

## ✅ STATUS: SHIPS THE GENERATED ALWAYS-ON HOOKS

| | Codex | Claude Code |
|---|---|---|
| manifest | ✅ `.codex-plugin/plugin.json` | ✅ `estelle-plugin/.claude-plugin/plugin.json` |
| server entry | ✅ inline `mcpServers` | ✅ `.mcp.json` |
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
