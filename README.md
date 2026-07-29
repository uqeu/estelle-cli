# Estelle

Your coding agent forgets your codebase every time you open a new session. Estelle is the memory it should
have had. One command, and the agent you already use can find any symbol, trace what breaks if you change a
file, and check its own claims against the real code before it tells you something confidently wrong.

You keep your model and your plan. Estelle rides alongside.

```
npx @fatelabs/estelle init
```

That detects the editors you have installed, writes Estelle's MCP server into each config, and leaves your
other servers alone. Restart your editor and your agent has new tools.

## You need a key first

Estelle is an account, not a local tool. Get a free key at **https://fatelabs.ca** — no card, and the free
tier holds a real codebase. `init` will ask for it, or pass `--key`.

Then point Estelle at your code:

```
npx @fatelabs/estelle sweep --key $ESTELLE_KEY
```

That reads the files git already tracks and builds your memory. Run it again whenever you want the memory to
catch up.

## What your agent gets

`find_definition` · `find_references` · `blast_radius` (everything that breaks if this file changes) ·
`verify` (does this code call APIs that actually exist here?) · and memory that survives between sessions.

`verify` fails closed. On a repo that was never swept it says so, instead of returning a pass. Nothing swept
means nothing was checked, and a check that could not run must never look like one that did.

## Without an editor

```
npx @fatelabs/estelle ask "why does /refresh 401 after deploy?"
npx @fatelabs/estelle recall "rate limiter" --repo api
npx @fatelabs/estelle verify src/api/routes.py
npx @fatelabs/estelle gate --base main
```

`gate` runs the merge gate over your staged diff and answers one question: should this merge?

## Make it unconditional

An agent that chooses when to check its work will eventually choose not to. In Claude Code you can make it
automatic:

```
npx @fatelabs/estelle install-hooks
```

Now every edit gets grounded and memory stays current while you work. `uninstall-hooks` removes only ours.

## What leaves your machine

Per command, and only when you run it. `init`, `connect` and `remove` stay local apart from one ping to
check your key works. `sweep` sends the files git tracks, `gate` your staged diff, `verify` one file, `ask`
and `recall` your question. Nothing else, and nothing in the background.

Zero dependencies. Node 18 or newer.

## Editors

Written automatically: Cursor, Cline, Windsurf, VS Code, JetBrains, plus the `claude mcp add` line for
Claude Code.

Guided instead of written: Claude Desktop and Continue. Their config formats cannot be written safely from
outside, so `init` prints the exact step rather than writing a file they would ignore.

## Why this repo is public

npm provenance requires it. Every published version is cryptographically attested to the commit it was built
from, so you can verify that what runs on your machine is what you can read here. This is the client. The
Estelle server is closed source and none of it is here.

Source-available, not open source. Read it, audit it, verify it. See LICENSE.

khai@fatelabs.ca
