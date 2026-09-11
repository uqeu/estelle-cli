---
description: Show what Estelle actually knows about this repo right now — currency, last sweep, which hosts have run a hook, and whether the gate can answer
---

Run these and report the result to the user **as measurements, never as reassurance**. If a number is
missing, say it is missing; an absent reading and a healthy one must never read the same.

```bash
python3 scripts/estelle_currency.py --status 2>&1 | head -30
```

Then read the repo's live currency straight off the server — this is the one fact a label cannot fake:

```bash
REPO=$(python3 scripts/estelle_currency.py --status 2>/dev/null | awk '$1=="repo"{print $2; exit}')
[ -n "$REPO" ] && curl -s --max-time 45 -X POST \
  -H "Authorization: Bearer $ESTELLE_API_KEY" -H "Content-Type: application/json" \
  -d "{\"repo\":\"$REPO\",\"head\":true}" \
  "${ESTELLE_API_URL:-https://api.fatelabs.ca}/graph/edges" 2>/dev/null | head -c 600 \
  || echo "no repo resolved — say so rather than reporting a healthy default"
```

## How to read it back to the user

Report, in this order, and in plain words:

1. **Can the gate answer?** `status: current` means yes. `stale` means the graph is behind the
   checkout, so **real code written since that commit will be reported as invented** — say that
   consequence out loud, it is the one that costs them.
2. **When did a hook last run, per host?** The `host` lines. ⛔ *"never observed"* is NOT *"zero runs"* —
   on Codex an untrusted `hooks.json` runs zero handlers and prints no error, so an absent heartbeat
   means **either the host was never used here or its hooks are silently not executing**. Say both.
3. **Is a catch-up in flight?** `in flight yes` means a sweep is running now; the numbers will move.
4. **Are the git hooks installed?** The `hooks` line. If it is empty, nothing advances the marker when
   HEAD moves, and the graph will go stale and stay stale.

⛔ **Do not summarise this as "Estelle is working."** Report which of the four questions above you could
answer and which you could not. A status command that always says "healthy" is the inert guard wearing a
green tick — this repo has paid for that shape more than once.

## 5. Do the WRITE door and the READ door belong to the same person?

🔴 **MEASURED 2026-09-07, and it silently emptied a complete memory.** Estelle authenticates twice by
two independent routes: the **hooks** read `$ESTELLE_API_KEY` / `~/.estelle/auth.json`, while the **MCP
server** at the same URL carries its own OAuth session. **Nothing asserted the two resolve to the same
account.** On the founder's machine they did not: the hooks wrote **227 sessions (35 codex, 38
claude-code)** to one account while every read tool queried another and saw **4**. `estelle_resume`
answered *"(no saved session)"* over a memory that was complete, correct, and stored — the worst
possible failure for a memory product, because the customer concludes the memory is empty and stops
using it.

Probe **both doors and compare**. This is the pair that cannot both be true if the install is healthy:

```bash
curl -s --max-time 45 "${ESTELLE_API_URL:-https://api.fatelabs.ca}/me" \
  -H "Authorization: Bearer $ESTELLE_API_KEY" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print('WRITE door (hooks) =', d.get('email'), '| plan', d.get('plan'))"
```

Then call the **`list_sessions` MCP tool** and read the `members` field of any row: that email is the
**READ door**. Report the two side by side.

- **They match** → say so, and that memory written by a hook is readable by a tool.
- **They differ** → ⛔ **this is the defect above.** State it plainly: *"your sessions are being written
  to X and read from Y, so Estelle will look empty while your memory is intact."* The fix is to bind the
  MCP server to the same credential the hooks use — an `Authorization: Bearer ${ESTELLE_API_KEY}` header
  on the `estelle` entry in `.mcp.json` — then reconnect with `/mcp`.

⚠️ **`list_sessions` returning ZERO rows does not mean "no memory".** It means *this identity* has no
sessions, which is exactly what the defect looks like from the read side. Never report an empty list as
"you have no history" without having compared the two doors first — an absent value and a real zero are
different facts, and only one of them is the customer's fault.
