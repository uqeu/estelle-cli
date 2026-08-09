# Route inventory — measured 2026-08-07

The foundation for the surface lane ("every route reachable"). Re-measured from source, not carried
from the brief — where my numbers differ from `KIMI-BRIEF.md`, mine are the measured ones and the
method is below so anyone can re-run them.

## Method

- **Server:** regex extraction of every `("METHOD", "/path", handler)` registration in
  `src/estelle/serve/api.py`, plus the method-less `(suffix, fn)` tuples folded into
  `routes.append(("POST"|"GET", suffix, …))` loops (the `/me/*`, `/addons/*` blocks at
  api.py:1271-1311). Output: `/tmp/server_routes.txt` (session-local).
- **Client declarations:** `estelle-client/src/endpoint.rs` endpoint table (53 rows).
- **Client call sites:** grep for `Endpoint::<Name>` across all of `cli-rs/**/*.rs` except
  `endpoint.rs` itself.

## Measured numbers (vs the brief's)

| | brief | measured | note |
|---|---|---|---|
| Server route registrations | 218 | **229 method+path pairs, 207 unique paths** | brief's 218 is a different denominator (unknown which). Mine counts method+path pairs incl. wildcard suffixes `*/envelope`, `*/store`; dynamic `/connect/{provider}/disconnect` expands per provider and is not in the 229. |
| Client declares | 53 | **53** | matches |
| Client reaches (has a call site) | 52 | **51** | `Endpoint::Checkpoint` has **no call site** — the TUI's "checkpoint" is the local `session_gap.rs` mechanism, never `POST /checkpoint`. `Endpoint::GithubAppCallback` has none either — it is the browser redirect target, not a client call. |
| Server paths with no client declaration | ~130 | **153** | mine includes admin/webhook/slack/oauth internal routes the "~130 customer-reachable" figure excludes; filtering those lands at ≈128, consistent. |

## Declared but never called (2)

- `Checkpoint` (`POST /checkpoint`) — dead declaration today. Wire it or delete it; the local
  session-gap checkpoint is a different thing with the same name.
- `GithubAppCallback` (`GET /github/app/callback`) — correctly never called by the client
  (browser redirect). Candidate for removal from the endpoint table, not wiring.

## The 153 undeclared server paths, grouped

**Wired since this inventory:** `GET /graph` (`/graph`), `GET /graph/nodes` (`/graph nodes`),
`GET /me` (`/me`), `GET /me/keys` (`/keys`), `GET /me/team` (`/team`), `GET /memory/cards`
(`/cards`), `GET /entities` (`/entities`), `GET /usage` (`/usage`), `GET /activity` (`/activity`), `GET /runs` (`/runs`),
`GET /outcomes` (`/outcomes`), `GET /memories` (`/memories` — split off the `/memory` alias),
`GET /analytics` (`/analytics`), `GET /audit` (`/audit`), `GET /requests` (`/requests`),
`GET /presence` (`/presence`), `GET /leaderboard` (`/leaderboard`) —
all with the honesty pattern: explicit `building`/`truncated`/invite states, null team renders as
absent, omitted fields render "not returned", unknown is never zero. Client now declares 70,
reaches 68 of 70. `/me/*` writes
(key create/revoke/rotate, team invite/seats, billing) and `memory/cards/{dream,edit,revert}`
remain unwired — mutations are their own commits.

**Customer-reachable — the surface lane's backlog (≈128):**

- Memory graph: `GET /graph` ✅, `GET /graph/nodes` ✅, `POST /graph/edges`, `GET /entities` ✅,
  `POST /entity-links`, `GET /memories` ✅, `POST /memory/edit`, `POST /memory/review`,
  `POST /memory/chat`, `GET /memory/cards` ✅, `POST /memory/cards/dream`, `POST /memory/cards/edit`,
  `POST /memory/cards/revert`, `POST /fact`, `POST /facts`, `POST /extract`, `POST /organize`,
  `POST /govern`, `POST /purge`, `GET /compaction`
- Research/creation: `POST /ideate`, `POST /dream`, `POST /impact`, `POST /tests`,
  `POST /repo/scan`, `POST /docs/ask`, `POST /search/cross`, `POST /session/reinterpret`
- Swarm/Orchestra: `POST /swarm`, `POST /swarm/plan`, `POST /swarm/run`, `POST /orchestra/plan`,
  `POST /orchestra/run`, `GET /runs` ✅, `GET /outcomes` ✅, `POST /pipeline`, `POST /automate`,
  `GET /automations`, `POST /dev/from-ticket`
- Account self-service (`/me/*` + billing/keys/team): `GET /me` ✅, `GET|POST /me/profile`,
  `GET ✅|POST /me/keys` (+`rename`/`revoke`/`rotate`), `POST /me/provider`,
  `POST /me/provider/label`, `GET ✅|POST /me/team` (+`invite`/`invite/accept`/`invite/revoke`/
  `leave`/`remove`/`transfer`/`role`), `POST /me/billing/{checkout,subscribe,cancel,portal,seats}`,
  `POST /me/avatar`, `GET /plans`, `GET /addons`, `POST /addons/{subscribe,remove}`,
  `POST /checkout`, `POST /subscribe`, `GET /usage` ✅, `GET /budget`, `POST /budget`,
  `GET|POST|PUT /settings`, `GET /suite`, `PUT /suite`, `GET /suites`, `POST /suites`,
  `GET /suites/upgrades`, `GET /skills/scored`, `POST /skill`, `POST /skill/feedback`,
  `POST /key`, `POST /key/delete`, `POST /key/web`, `POST /account/worlds`,
  `GET /account/connections`, `GET /connections`, `GET|POST /autonomy/auto-mode`
- Activity/reads: `GET /activity` ✅, `GET /analytics` ✅, `GET /audit` ✅, `GET /requests` ✅,
  `GET /leaderboard` ✅, `GET /team/leaderboard`, `GET|POST /presence` ✅ (read), `GET /board`,
  `POST /board/sync`, `GET|POST /marketplace`
- Agent/monitor extras: `POST /agent/events`, `POST /agent/gate`, `POST /agent/verify`,
  `POST /agent/triage`, `GET /agent/docs`, `POST /autorepair`, `POST /autorepair/revert`,
  `POST /monitor/{alert,alert/delete,drain,ingest,metric,uptime/delete}`
- Integrations: `GET|POST /jira/{issue,issues}`, `GET /jira/board`, `GET|POST /linear/issue`,
  `GET /linear/issues`, `GET|POST /notion/page`, `POST /notion/query`, `GET /github/installs`,
  `GET /github/app/install-url`, `POST /github/{connect,install,sweep…}`, `GET /install`,
  `POST /context`, `POST /context/load`, `GET /contexts`, `POST /resume`, `POST /welcome`,
  `POST /ingest`, `GET /oauth/start`, `POST /instinct`

**Internal / not CLI-addressable (excluded from the lane):** `/admin/*` (13), `/slack/*` (5),
`/webhook*` (4), `/oauth/callback`, `/github/{webhook,app/webhook,app/release}`, `/signup`,
`POST /ingest` (server-side pipeline), `/security/tick`, `/vendor-drift/tick`, `/mcp` GET/DELETE
(owned by the MCP session per SERVER-CONTRACTS-STATUS #7 — do not touch).

## Binding rule (unchanged, from SERVER-CONTRACTS-STATUS)

Every optional capability has an explicit absent/unknown value and a reason. Every observation has
`observed_at`; every live snapshot has `stale_after_s`. **No client calculates a server fact from
elapsed time, HTTP completion, or missing data. Unknown is `null` — never zero, never a
checkmark.** Absent renders as absent, with the reason.
