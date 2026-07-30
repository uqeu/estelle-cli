# Estelle

Your coding agent forgets your codebase every time you open a new session. Estelle is the memory it should
have had, plus a gate that checks the agent's claims against your real code before it tells you something
confidently wrong.

You keep your own model and your own plan. Estelle rides alongside.

## Install

```
npx @fatelabs/estelle init
```

Detects the editors you have installed and writes Estelle's MCP server into each config, leaving your other
servers alone. Restart your editor and your agent has the new tools.

## You need a key

Estelle is an account, not a local tool. Get one free at **[fatelabs.ca](https://fatelabs.ca)** — no card.
`init` will ask for it.

## Keeping it current

`sweep` is a **fresh start**: it rebuilds your memory from the whole tree. Use it once per repo.

`reindex` is the **daily one**: it updates only what changed and leaves the rest of the graph alone.

```
npx @fatelabs/estelle reindex          # whatever git says you changed
npx @fatelabs/estelle reindex src/a.py
```

Don't reach for `sweep` to catch up. It rebuilds from exactly the files you hand it, so a partial sweep
throws the rest away and the gate starts calling real code hallucinated.

In Claude Code, `install-hooks` does the reindex for you on every edit.

## Everything else

Commands, editor setup, the API, how memory and the grounding gate work:
**[fatelabs.ca/docs](https://fatelabs.ca/docs)**

## About this package

Zero dependencies, Node 18 or newer. Published from CI with npm provenance, so every version is
cryptographically attested to the commit it was built from.

This repository is the client only. The Estelle server is closed source and none of it is here.
Source-available, not open source: read it, audit it, verify it. All rights reserved. See LICENSE.

khai@fatelabs.ca
