# Server contracts — status overlay, 2026-08-06

`SERVER-CONTRACTS-NEEDED.md` was written **Aug 4 16:46**. The server has moved since. This file checks
each of its ten asks against the routes that exist **today**, so nobody builds something that already
landed or assumes something exists because it was requested.

**Method:** every "server today" cell was verified by reading the route table in
`src/estelle/serve/api.py` and the handler modules, not inferred from the request. Where a route is
absent, the absence was confirmed by grep across `src/estelle/serve/*.py` and
`scripts/estelle_server.py`.

---

## The ten, re-scored

| # | The CLI's ask | Server today | Verdict |
|---|---|---|---|
| 1 | `repair.patch` on `GET /issues` | `/issues` **exists** and returns `repair: {status, detail, pr}` (`api_issues_feed.py:87-88`). No `patch` field. | 🟢 **CHEAPEST WIN.** `api_issues_feed.py:3-4` states the repair draft is "proven end to end on real wire bytes, and **every piece is in the database**." This is a field addition on an existing door, not new machinery. |
| 2 | `GET /orchestra/status` long-poll, revisioned fleet snapshots | **Absent.** `/orchestra`, `/swarm`, `/swarm/plan`, `/swarm/run` exist and are **synchronous** — they return a completed envelope. No fleet id, no revision, no incremental read. | 🔴 **BIGGEST BUILD.** Needs durable fleet identity + a revision counter + a long-poll read. This is register #30/#41. Everything else on this list is smaller. |
| 3 | `GET /agent/health` | `/agent/events` (ingest), `/agent/gate`, `/agent/verify`, `/agent/docs`, `/agent/triage` all exist. **No health read.** | 🔴 **THE PIPELINE'S MISSING HALF** — see below. |
| 4 | `GET /github/status`, `GET /prs` | `/github/identity`, `/github/installs`, `/github/repos`, `/github/connect` exist. No `/github/status`, no `/prs`. | 🟡 **SMALL.** `identity` already answers most of "am I connected". `/prs` is genuinely new. |
| 5 | Account-wide model selection | `/providers` + `/provider/select` exist and the client is bound to both. | ✅ **DONE.** Nothing owed. |
| 6 | Durable ACP session identity + event stream | `/session`, `/sessions`, `/resume`, `/context`, `/context/load`, `/contexts`, `/checkpoint` all exist. | 🟡 **CLOSER THAN THE DOC ASSUMES.** Verify what these already guarantee before building a second session store. |
| 7 | MCP beyond the tool catalogue | `/mcp` exists. **No longer tools-only** — `prompts/list` + `prompts/get` are live, and `initialize` carries `instructions`. See the row below. | 🔵 **IN FLIGHT** — a parallel session owns `serve/mcp_*.py` and `serve/mcp.py`. Do not touch. |
| 8 | `GET /repairs/<id>/events` | **Absent.** `/autorepair`, `/autorepair/revert` exist. | ⚪ **DEFER.** Codex's own note: this degrades gracefully to a one-line verdict and blocks nothing. |
| 9 | Per-role model routing | `/route` exists; `learned_routing.py` exists. No per-role pin map under the `global` suite. **SIX roles, not five: `planning · implementation · review · coordinator · chat · research`** (founder, 2026-08-07). A strong plan cannot catch line-level slop without reading every line, which defeats delegating implementation; the economics work because a DIFF IS SHORT — plan strong (cheap), implement local (free, where the tokens burn), review strong (cheap). The reviewer already exists (`adversarial_review.py`, `_review_refute_for` takes the writer as an argument so a model never reviews itself) — it has no role pin. | 🟡 **MEDIUM.** |
| 10 | Durable Working-memory proof | `/context`, `/context/load`, `/contexts`, `/checkpoint` exist. | 🟡 **VERIFY BEFORE BUILDING.** The ask is for a *proof* (write → restart → read → isolation), which may be satisfiable with what exists. |

---

## #7 — what the MCP surface is TODAY (measured, 2026-08-06/07)

All figures are `cl100k` tokens from a real `tools/list` against `api.fatelabs.ca` with a live key, and
the AFTER row is **read back off prod**, not projected.

| | tools | tokens | % of a 200k window |
|---|---|---|---|
| before | 283 (246 playbooks + 37 real) | 49,465 | 24.7% |
| **after** | **37** | **4,508** | **2.3%** |
| delta | −246 | **−44,957** | **90.9% of the payload removed** |

The 246 playbooks are now **MCP prompts** — user-initiated, so they cost **zero** model context until
someone invokes one. Claude Code surfaces them as `/mcp__estelle__<name>`; Cursor lists prompts as
supported. `prompts/get` dispatches to the same `deliver_skill` closure the tool used, so entitlement,
personalization, revocation and the multi-repo scope question are unchanged.

**BREAKING:** `skill_<name>` tools no longer exist in any spelling. A cached call gets `-32602`. Verified
live.

`initialize` now also returns **`instructions`** (1,299 bytes). With Claude Code's tool search on by
default — *"only tool names and server instructions load at session start"* — that string is what decides
whether a host searches our tools at all.

### Client support, checked before promising anything

| Feature | Claude Code | Cursor | Codex |
|---|---|---|---|
| Tools · Resources · Elicitation | ✅ | ✅ | ✅ |
| **Prompts** | ✅ `/mcp__server__prompt` | ✅ | — not found |
| **Apps** | ⚠️ not found | ✅ published table | ✅ `ui://` fixtures |
| **Tasks (SEP-2663)** | ⚠️ not found | 🔴 word absent from the page | 🔴 client never opts in |

⛔ **Do not build Tasks.** `rmcp-3.0.0/src/handler/server.rs:206`: *"the server MUST NOT return
CreateTaskResult unless the request declared the tasks extension capability."* No client in our set
declares it. ⛔ **Do not build lazy tool schemas** — all 283 schemas were ONE identical 35-token shape, the
spec requires `inputSchema` in `tools/list`, and Claude Code already defers tool definitions host-side.
⛔ **Do not build sampling** — deprecated `2026-07-28` (SEP-2577).

**Still owed on #7** (none of it widens the surface; all of it makes what we advertise real):
- `GET /mcp` returns **404**, byte-identical to an unknown path. The spec is a MUST: 405 or
  `text/event-stream`. A client probing GET cannot tell "does not stream" from "no server here".
  `DELETE /mcp` returns 501; the spec's code is 405. Both need a route registration in `api.py`
  (paste-ready in `docs/mcp-surface-measured-2026-08-06.md` §7).
- We answer `2025-11-25` to a `2026-07-28` request, and results carry no `resultType`. A guard now
  couples the two so the version cannot be advertised without the field.
- 10 tools are advertised to plans that cannot call them (`tools/list` is byte-identical on Free and
  Ultra; entitlement is enforced inside the call). Composition-root change.

---

## The one that actually matters: #3 closes the pipeline

The product story is **Guardian → Orchestra/Affinity/Research → Review/Gate → Agent/Monitor**, and the
last arrow is where a deployed Estelle agent reports back what broke.

`POST /agent/events` **ingests** that. It is live, and since `7ab82ba3` a bound production failure earns
a durable `failure --bound_to--> symbol` edge in the graph.

**There is no read path.** No `GET /agent/health`, and no customer-facing route for it anywhere. So the
loop is: customer's agent breaks → Estelle ingests it → binds it to a symbol → writes the edge →
**and the customer cannot see any of it from the CLI.** The CLI's home section says
`State unavailable - no read contract` and that sentence is literally true.

**#3 is the single wire that turns the pipeline from a diagram into a product surface.** It is also
modest: the data is already ingested, bound, and stored. Build the read.

---

## Two gaps Codex never filed

**A. Affinity has no customer door at all.** Verified: the only affinity route in `api.py` is
`POST /admin/affinity/seed`, behind the `X-Admin-Token` gate (`api.py:607-609`). `serve/affinity.py`
exists and scores models on mean-repairs as a router tie-break — but **no customer, through any client,
can read it.** The CLI cannot show Affinity because there is nothing to call. If Affinity is part of
the pitch, it needs a public read route; that is not on Codex's list because Codex never found a door
to bind to.

**B. Register #72 is largely stale.** It reads "CLI cannot change ANY account setting or the autonomy
mode — the endpoints exist, the client never declares them." But `Autonomy`, `AutonomyScope`, and
`SettingsSuite` **are** declared in `estelle-client/src/endpoint.rs`, and
`SERVER-CONTRACTS-NEEDED.md` records global autonomy and suite settings as **bound**, with call sites
at `tui/src/main.rs:2019-2040`, `2815-2838`, `2864-2882`. What actually remains:
- `/settings` (billing/catalog `GET`/`POST`/`PUT`) has **no** client endpoint — `/settings` in the TUI
  opens *local terminal preferences* instead.
- `/autonomy/scope` is read at startup (`main.rs:2123-2135`) but has **no POST call site** — display-only.

Rewrite #72 to those two lines rather than leaving it as written.

---

## Recommended order

1. **#1 repair patch** — the data is in the database; this is a field on an existing response.
2. **#3 agent health** — closes Guardian → Monitor, the pipeline's missing arrow.
3. **#4 `/prs`** — the review queue; `github/identity` already covers most of `status`.
4. **Affinity read route** — gap A above; currently unreachable by any customer.
5. **#9 per-role routing**, then **#2 Orchestra live**, the largest.
6. **#8 defer.** **#5 done.** **#7 owned by the MCP session.** **#6 and #10 verify before building.**
