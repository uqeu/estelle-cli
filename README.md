# @fatelabs/estelle

Connect **Estelle** — code-native memory + a 0%-hallucination grounding gate — to the AI coding agent you
already use. One command, no Python. Egress is per-command: `init`/`remove`/`connect` write or print local
editor config (`init` pings your Estelle endpoint once to verify the key); `sweep` sends this repo's
git-visible source files, `gate` your staged diff, `verify` one file, `ask`/`recall` your question — each
to your Estelle endpoint (default `api.fatelabs.ca`), only when you run it.

```bash
npx @fatelabs/estelle init            # auto-detect your editors, write the MCP config (with a backup)
npx @fatelabs/estelle sweep --key …   # ingest the current repo into your Estelle memory
npx @fatelabs/estelle connect cursor  # just show the one-liner / config for one client
```

`init` finds every installed MCP client (Cursor, Claude Desktop, Cline, Windsurf, Continue, VS Code), writes
Estelle's hosted MCP server into each config **without clobbering your other servers**, and — for Claude Code —
prints the `claude mcp add` line. Restart your editor and your agent gains `find_definition`,
`find_references`, `blast_radius`, and `verify` over your repo, scoped to your key.

Zero dependencies (Node ≥ 18 built-ins only). Your agent keeps running on **your** plan; Estelle rides
alongside over the hosted API. Publish target: `npx @fatelabs/estelle`.
