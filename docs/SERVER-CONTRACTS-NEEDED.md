# Server contracts needed by the Estelle CLI

This is the ranked server-lane backlog for customer-visible CLI surfaces. The client MUST NOT infer any
field in this document from elapsed time, prose, request completion, or an empty response. `null` means the
server does not know or the capability is absent; zero is a measured number only.

Every scoped request carries `repo=<owner/name>`. Every account-owned request derives the account from the
credential rather than accepting a caller-provided account identifier. Unknown and cross-tenant identifiers
return the same 404 response.

## Audited control surfaces - existing wire versus client reachability

These are not requests for replacement endpoints. They record which server-owned controls already exist so
the CLI does not create a second control plane while filling in its interactive settings surfaces.

| Capability | Server contract | CLI reachability today |
|---|---|---|
| Billing/catalog settings | `GET /settings`, `POST /settings`, and `PUT /settings` are distinct reads/writes (`src/estelle/serve/api.py:911-916`). | No Estelle-client endpoint exists. `/settings` opens local terminal preferences; it cannot read or change these server settings. |
| Suite settings | `GET/POST /settings/suite` carries `{team, personal, schema}` and `{suite, key, value, scope}` (`src/estelle/serve/api.py:1104-1109`, `src/estelle/serve/api_settings.py:48-129`). | Bound. Startup reads every returned suite (`cli-rs/tui/src/main.rs:2788-2799`); the picker exposes `code`, `monitor`, `review`, `repair`, `prod`, `guardian`, `research`, `memory`, `agent`, and `global`; writes send the exact typed value and scope (`cli-rs/tui/src/main.rs:2815-2838`). A refused write has no local fallback. |
| Global autonomy | `POST /autonomy` reads on an empty body or writes `{level, acknowledge_risk?}`; writes are grant-gated and `branch`/`execute` require explicit risk acknowledgement (`src/estelle/serve/api_account.py:275-314`). | Bound. The TUI displays `plan`/`edit`/`branch`/`auto`, posts the wire value, acknowledges the two elevated levels, and changes local state only when the server returns its enforced ceiling (`cli-rs/tui/src/main.rs:2019-2040`, `cli-rs/tui/src/main.rs:2864-2882`). |
| Scoped autonomy | `GET/POST /autonomy/scope` exposes global, repo, surface, personal-cap and resolved views (`src/estelle/serve/api_settings.py:195-270`). Every scope response is explicitly `enforced: false`; executors still honour only the global dial (`src/estelle/serve/api_settings.py:215-217`). | The endpoint is registered at `cli-rs/estelle-client/src/endpoint.rs:70`; startup performs the GET at `cli-rs/tui/src/main.rs:2123-2135`. There is no POST call site, so the TUI can display but cannot change scoped values. |
| Provider/model selection | `GET /providers` returns the callable pool and active provider/model; `POST /provider/select` writes `{provider, model?}` account-wide (`src/estelle/serve/api_account.py:34-76`). | Bound account-wide through the model picker (`cli-rs/tui/src/main.rs:2885-2905`). The success copy explicitly preserves auto routing and does not claim which model served a request (`cli-rs/tui/src/main.rs:2056-2071`). Per-role pins remain a missing contract documented below. |
| Agent ceiling | The enforced hard ceiling is `MAX_SWARM_TASKS = 50`, composed with plan admission (`src/estelle/serve/api_orchestra.py:13`, `src/estelle/serve/api_orchestra.py:76-77`). The former `agent.fanout_ceiling` setting was deleted because no executor read it (`src/estelle/serve/settings_schema.py:137-142`). | There is no truthful writable `max agents` control to bind. Display admitted/allowed counts from Orchestra decisions; do not synthesize a setting. |
| Account/team identity | `GET /account` returns the account plus a team block `{id, name, role, is_admin, is_owner, owner_email}` (`src/estelle/serve/api_account.py:580-606`). | `AccountResponse` types only email/plan/balance and flattens the rest (`cli-rs/estelle-client/src/types.rs:9-19`); the TUI currently consumes only `plan` (`cli-rs/tui/src/main.rs:1384-1388`). Team identity is on the wire but not surfaced. There is no separate organization identity to infer. |
| Working memory | The client collects changed, staged and non-ignored untracked Git files (`cli-rs/tui/src/top_level.rs:706-726`) and sends the bounded current prompt separately from Repo graph (`cli-rs/tui/src/main.rs:1664-1703`). The server persists `messages[:-1]`, not the current ask (`scripts/estelle_server.py:907-915`). | No new settings or memory endpoint is required. Working memory remains a client-owned, per-turn input; the UI must disclose that it is sent to Estelle's BYOK answer path while not merged into the team Repo graph. |
| Monitor | The client reaches cursor-paginated `GET /issues` and `GET /monitor/overview` (`cli-rs/tui/src/main.rs:2033-2055`, `cli-rs/tui/src/main.rs:2083-2098`). | Production app/service identity is not present in an issue row (`src/estelle/serve/api_issues_feed.py:54-95`) or the overview envelope (`src/estelle/serve/api_monitor.py:424-463`). A future app selector needs an explicit server field; repo, culprit, or transaction must not be relabelled as app identity. |

## 1. Repair patch on production issues - BLOCKS proposed-diff review

**Surface unlocked:** selecting a production issue opens the existing read-only proposed-repair diff pane.

**Today:** `GET /issues` exposes repair status and an optional PR, but not the patch. The pane can say that a
draft exists; it cannot show what Estelle proposes to change.

Extend each issue's `repair` object:

```json
{
  "repair": {
    "status": "proposed",
    "detail": "bounded retry at the bound symbol",
    "pr": null,
    "patch": {
      "format": "unified_diff",
      "base_sha": "8e17a9952",
      "text": "diff --git a/billing/charge.rs b/billing/charge.rs\n...",
      "observed_at": 1785203400.0
    },
    "patch_absent_reason": null
  }
}
```

- `patch` is `null | RepairPatch`; it is never an empty object or empty string.
- `format` is currently only `unified_diff`.
- `base_sha` is the exact Git object the patch applies to.
- `text` is the exact patch evaluated by the repair gate and, if a PR exists, the patch proposed by that PR.
- `observed_at` is Unix epoch seconds for this patch revision.
- When `patch` is null, `patch_absent_reason` is required and non-empty. Values are stable codes plus optional
  prose, for example `not_proposed`, `expired`, `not_persisted`, or `unavailable`.
- A patch is read-only in the issue feed. Applying it remains the existing explicit `/apply` path.

**Missing-field rendering:** `repair.patch == null` keeps the production pane's honest status line and adds
`diff unavailable - <patch_absent_reason>`. It never opens a blank diff pane.

## 2. Estelle Orchestra live view - BLOCKS the live grid

**Surface unlocked:** the fixed-height, five-column Estelle Orchestra instrument visible while `/orchestra`
or ACP fan-out is executing.

**Today:** `POST /orchestra` and `POST /orchestra/run` return a rich completed envelope, but there is no job
identity, revisioned per-worker lifecycle, current action, or incremental read. The CLI renders completed
typed snapshots when supplied, but cannot truthfully make the grid appear live.

### Required wire

1. `POST /orchestra/run` accepts one `task` or an explicit `tasks` array and returns `202` only after the
   server has planned and durably created the fleet.
2. `GET /orchestra/status?fleet_id=<id>&after_revision=<n>&wait_s=20&repo=<owner/name>` long-polls until the
   revision advances, the fleet becomes terminal, or 20 seconds pass.
3. Every response carries the full latest `fleet` snapshot. The client replaces by revision; it never
   reconstructs missed transitions.

```json
{
  "fleet": {
    "id": "fleet-41",
    "batch": "Retry missing 5 assignments",
    "models": ["Claude Opus 4.1", "gpt-5.5"],
    "state": "running",
    "attempt": "retry",
    "revision": 8,
    "observed_at": 1785203400.0,
    "stale_after_s": 60,
    "completed": 1,
    "total": 2,
    "narrator": {
      "text": "a007 lost 4 assignments, a034 lost 1. Retrying those two slices.",
      "evidence": "observed"
    },
    "agents": [
      {
        "index": 1,
        "status": "running",
        "attempt": "retry",
        "state_observed_at": 1785203400.0,
        "current_action": "Checking kill switch invariants",
        "progress": {"completed": 2, "total": 5},
        "assignments": {"attempted": 4, "completed": null, "lost": 4},
        "failure_cause": {"text": "driver timeout", "evidence": "measured"}
      },
      {
        "index": 2,
        "status": "unknown",
        "state_observed_at": 1785203380.0,
        "unknown_reason": "worker has not reported state",
        "progress": null,
        "assignments": {"attempted": null, "completed": null, "lost": null}
      }
    ]
  },
  "todo": {
    "observed_at": 1785203400.0,
    "stale_after_s": 60,
    "items": [
      {
        "title": "Isolation width 10",
        "status": "done",
        "result": "owner 10/10, cross-tenant 0/10",
        "evidence": "measured"
      },
      {"title": "Mutation lane", "status": "in_progress", "result": null, "evidence": "observed"}
    ]
  }
}
```

### Fleet invariants

- `id` is stable and `revision` is strictly increasing. `ETag` equals the revision; `If-None-Match` may
  return 304. `429` and 5xx responses carry `Retry-After`.
- `total` is the number of admitted slots, not the requested ceiling. Unknown is null, never zero.
- `agents` has one row for every admitted index, including `queued` workers.
- `models` contains only server-reported participant names. The client removes exact duplicates while
  retaining order; it never expands a routing policy into model names. Unknown renders `models unknown`.
- Lifecycle states are `created`, `starting`, `queued`, `running`, `awaiting_approval`, `completed`,
  `failed`, `timed_out`, `killed`, `lost`, `blocked`, `needs_input`, `cancelled`, and `unknown`.
- `unknown` requires `unknown_reason`. Missing state never defaults to `running`.
- Every terminal outcome has a distinct glyph and colour. A stopped process is not successful;
  `completed` requires measured successful exit.
- `attempt` is `first`, `retry`, or `unknown`. Recovery retains loss counts and failure causes.
- Assignment counts are nullable. Unknown never defaults to zero.
- `failure_cause` and `narrator` evidence is `measured`, `observed`, `derived`, `inferred`, or `unknown`.
  Derived, inferred, and unknown statements render with a visible marker.
- Every row has `state_observed_at`; every snapshot has `observed_at` and `stale_after_s`. Expired state is
  labelled stale.
- `current_action` is bounded observed text, never chain-of-thought. It is required while running.
- `progress` exists only with a real worker-owned denominator and carries both `completed` and `total`.
  Missing progress renders an indeterminate liveness glyph with no fill.
- Fleet `completed` and `total` are nullable measured counts. The aggregate is a sum over real counts, never
  an average of per-worker percentages.
- A task rejected before admission is not an agent row; it remains in `decision.refused`.

### Todo invariants

- Todo is session state independent of Orchestra and may render alone.
- Status is `pending`, `in_progress`, `done`, or explicit `unknown`.
- Done items retain their result. The collapsed view retains five rows and reports hidden and hidden-done
  counts. `/todo` toggles the surface; `Ctrl+T` changes density.
- Todo also carries `observed_at` and `stale_after_s`; an old ledger is visibly stale.

**Missing-field rendering:** no `fleet` means no grid. A completed report remains ordinary transcript. No
`todo` means no Todo surface. The CLI never creates rows or progress from request timing.

## 3. Agent-health read path - BLOCKS the home section

**Surface unlocked:** live health for the customer's production agents, not Estelle's own workers.

**Today:** `POST /agent/events` ingests when `ESTELLE_AGENT_ENABLED` permits it, but the CLI has no scoped
read contract. The home section says `State unavailable - no read contract` and names the ingest action.

Add `GET /agent/health?repo=<owner/name>&window_s=3600`:

```json
{
  "enabled": true,
  "enabled_absent_reason": null,
  "observed_at": 1785203400.0,
  "stale_after_s": 120,
  "counts": {"reporting": 7, "degraded": 1, "silent": 2},
  "agents": [
    {
      "id": "checkout-agent",
      "state": "degraded",
      "state_absent_reason": null,
      "events": 19,
      "last_seen": 1785203370.0,
      "current_signal": "tool timeout"
    }
  ]
}
```

- `enabled` is `true | false | null`. Null requires `enabled_absent_reason`.
- Counts are `u64 | null`; null never renders as zero.
- Agent state is `healthy`, `degraded`, `silent`, `disabled`, or `unknown`. Unknown requires
  `state_absent_reason`.
- `last_seen` and snapshot staleness are mandatory when enabled.

**Missing-field rendering:** disabled says `Agent telemetry not enabled` and names the enable action. Null
says why the server could not determine it. No response retains today's no-read-contract message.

## 4. Account GitHub and proposed-PR feed - DEGRADES the home section

**Surfaces unlocked:** GitHub connection state and the review queue of PRs Estelle opened.

**Today:** the CLI has explicit GitHub commands, but the permanent home pane cannot read connection state or
an account-wide proposed-repair queue. It says `Connection state not loaded` and points to
`estelle github status`.

Add:

- `GET /github/status` -> `{connected, provider, login, observed_at, absent_reason}`.
- `GET /prs?repo=<owner/name>&cursor=<opaque>&limit=50` -> `{prs, next_cursor, has_more}`.
- Each PR carries `{number, title, url, repo, issue_key, repair_status, gate, created_at, updated_at}`.
- `connected` is `true | false | null`; null requires `absent_reason`.
- `gate` is `null | {state, verdict, verified, observed_at}`. A null gate requires
  `gate_absent_reason`; it never becomes a checkmark.

**Missing-field rendering:** disconnected says how to connect. Unknown says why. An empty PR page says
`No Estelle PRs await review`; it does not display zero as a health verdict.

## 5. Account-wide model selection - COMPLETE CLIENT BINDING

**Surface unlocked:** the arrow-key model picker can make an explicit account-wide change through the
existing provider-selection door.

**Today:** `/model` lists the server pool and active account-wide provider/model. Enter posts the selected
provider and model to `POST /provider/select`. Session-scoped mutation is explicitly unsupported, and the
client does not fake it.

Bind Enter on a selectable pool row to the existing request:

```json
{
  "provider": "anthropic",
  "model": "claude-opus-4.1"
}
```

The current response is `{ok, provider, provider_model}`. The server reuses the stored key and rejects a
provider the account has not connected (`src/estelle/serve/api_account.py:34-49`). Authorization and
provider-key resolution remain server-owned. ACP provider credentials never enter this path.

The picker changes only the existing account-wide active provider/model. Do not add `PATCH /model`, a
session pin, a locally persisted routing-role picker, or a second model store. Server routing owns roles.
The client reports that auto routing remains active and only names a concrete model as `observed` after a
server response identifies the model that actually served work.

## 6. ACP lifecycle beyond session creation - DEGRADES ACP hosts

**Surface unlocked:** ACP clients can observe and control an Estelle session rather than only create one.

**Reachable today:** `estelle acp` resolves the ordinary Estelle credential and serves protocol v1 over
stdio. The adapter implements `initialize`, `session/new`, `session/prompt`, and `session/cancel`. New
sessions bind the launch directory to one repository and reject client-provided MCP servers and additional
directories. A prompt accepts text and resource links, turns resource links into disclosed prose without
reading local files, performs one scoped `/deep-search`, emits one text chunk, and ends the turn. Both ACP
request cancellation and `session/cancel` cancel that HTTP request. Unknown sessions, empty prompts, rich
prompt blocks, and unsupported methods fail explicitly. Initialize advertises every optional capability
false and advertises no auth method.

The current local ACP session stores only `{ACP session id -> repo}`. It has no server session identity or
conversation history, so successive prompts are independent deep searches rather than a resumable Estelle
conversation. The deep-search response already contains sources, grounded state, ungrounded symbols,
degraded/scope-ask state, and citations, but the adapter currently emits only `rendered_answer`; this loss is
adapter-local work and does not require a new server endpoint.

Required server ownership before advertising richer ACP capabilities:

- A stable Estelle session ID, versioned resume token, and repository binding for list/load/resume/fork/close
  and multi-turn history. An ACP-generated UUID is not a durable server session.
- A revisioned session event stream for incremental assistant text, tool calls and results, permission
  decisions, plan/mode/config changes, usage/context, Working memory, Todo, and the Orchestra lifecycle from
  section 2. ACP remains a view/transport, never a second agent or orchestrator.
- One request/run identity joining prompt, tool work, and Orchestra work so cancellation terminates all work
  attributable to that turn and returns its measured terminal state.
- Explicit ingestion contracts before enabling image, audio, embedded-context, additional-directory, or
  client-provided MCP capabilities. Resource links alone never authorize the server to read a host file.
- An ACP authentication method only if hosts must onboard through ACP. Today authentication deliberately
  occurs before protocol startup through the Estelle credential store; host credentials and Estelle BYOK
  provider credentials remain separate doors.

Adapter-local work, once the corresponding data exists, includes emitting source/citation metadata instead
of dropping it, mapping typed Estelle failures rather than collapsing all failures to `internal_error`, and
rendering measured server events as ACP updates without synthesizing lifecycle state.

**Missing capability rendering:** advertise false. Never return a success-shaped stub.

## 7. MCP beyond the verified tool catalogue - DEGRADES MCP clients

**Surface unlocked:** complete MCP interoperability where the Estelle server owns a corresponding feature.

**Reachable today:** `estelle mcp-server` exposes Estelle's remote catalogue as a tools-only MCP server over
stdio. It performs a real RMCP handshake, forwards paginated `tools/list`, forwards `tools/call` to `/mcp`,
and forcibly replaces any caller-supplied `repo` with the repository resolved when the server was launched.
`estelle mcp -- <command>` is a separate one-shot MCP client: it starts an external stdio server, performs a
real handshake, lists its tools, optionally calls one named tool, then cancels the service. Inside the TUI,
`/tools` and `/mcp` list Estelle's `/mcp` catalogue only; they do not connect to or invoke external servers.

The server advertises only `tools`. It does not advertise or implement resources, resource templates,
subscriptions, prompts, completions, logging, progress, request-scoped cancellation, roots, sampling,
elicitation, or catalogue-change notifications. It has no MCP HTTP/SSE/streamable-HTTP transport, protected
resource metadata, OAuth discovery, or provider authorization flow. The backend JSON-RPC envelope uses a
per-POST constant id and has no durable request/run identity, so asynchronous progress and cancellation
cannot be correlated honestly yet.

Required before advertising each server-owned capability:

- Resources: stable URI and template schemes, MIME type, account/repo scope, pagination, staleness, and
  change-notification contracts for Repo graph, Working memory, production issues, and repair artifacts.
- Prompts: a server-owned prompt catalogue with typed arguments and revisions; no local duplicate registry.
- Progress and cancellation: a request identity bound to the originating MCP request and the exact server
  run, with measured progress denominators and terminal outcomes. Service shutdown is not request cancel.
- Roots, sampling, and elicitation: explicit trust and approval boundaries. Estelle must not silently ask an
  MCP host to read arbitrary local roots, choose a provider, run inference, or collect customer input.
- HTTP transport and OAuth: protected-resource metadata, authorization-server discovery, scopes, token
  audience, refresh/revocation behavior, and cross-tenant 404 invariants before either is advertised.
- Errors: typed MCP errors with stable codes and sanitized data, never leaked upstream HTML, credentials, or
  a success-shaped empty result.

Persistent external-server configuration, connect/disconnect/reconnect state, an interactive TUI tool
catalogue, tool-call approvals, and provenance labels are adapter-local product work. They do not require
the Estelle server unless an external tool is deliberately attached to an Estelle server session. ACP must
continue rejecting client MCP servers until that ownership and trust contract exists.

**Missing capability rendering:** omit it from initialize/capability responses. A tool-only MCP client is
valid and honest.

## 8. Sandbox stream - DEGRADES S3 to a verdict line

**Surface unlocked:** real boot/clone/install/test output inside the production pane.

**Today:** the repair response is a stamped result, not a feed. S3 correctly renders one real verdict line.
No spinner is attached to an absent stream.

Add a scoped ordered event stream for a repair attempt:

`GET /repairs/<repair_id>/events?after=<sequence>` using SSE or long-poll, with events:

```json
{
  "sequence": 17,
  "observed_at": 1785203400.0,
  "phase": "test",
  "stream": "stdout",
  "text": "test checkout_retry ... ok",
  "terminal": null
}
```

- Phase is `booting`, `cloning`, `installing`, `testing`, or `unknown`.
- Stream is `stdout`, `stderr`, or `system`.
- Terminal is null or `completed`, `failed`, `timed_out`, `killed`, `cancelled`, `unknown`.
- The server sanitizes secrets before persistence and emission.
- The client labels the pane `sandbox - a clone, never production` and never replays it as live after the
  terminal event without an explicit historical marker.

**Missing-field rendering:** retain the one-line stamped verdict. This gap does not block the pane.

## 9. Per-role model routing - BLOCKS custom role pinning

**Surface unlocked:** a truthful `routing · auto` or `routing · custom` settings surface and a role-aware
Orchestra participants line for `planning`, `implementation`, `coordinator`, `chat`, and `research`.

**Today:** `GET /providers` and `POST /provider/select` expose one account-wide provider/model door. They do
not return the server's automatic choice per role, whether a role was pinned, or the source/revision of that
decision. The CLI therefore supports account-wide selection only. It does not persist role pins locally or
expand an auto policy into invented participant names.

Add a server-owned settings shape under the `global` suite:

```json
{
  "routing": {
    "mode": "custom",
    "revision": 12,
    "observed_at": 1785203400.0,
    "roles": {
      "planning": {
        "auto_model": "claude-opus-4.1",
        "pinned_model": "claude-opus-4.1",
        "effective_model": "claude-opus-4.1"
      },
      "implementation": {
        "auto_model": "gemini-2.5-pro",
        "pinned_model": null,
        "effective_model": "gemini-2.5-pro"
      }
    }
  }
}
```

- Every role is present. Each model field is `string | null`; null requires an adjacent non-empty absence
  reason. An omitted role never defaults to the account-wide provider.
- Writes carry the expected revision and the complete desired pin map. A conflict returns the latest
  revision; the client never silently overwrites a newer routing decision.
- `effective_model` is the server's resolved result, not a client calculation. The Orchestra feed reports
  the role and the model actually assigned to each worker separately.
- `coordinator` is the role name. `Orchestra` remains the subsystem name.

**Missing-field rendering:** retain `routing auto` without model names. The model picker remains explicitly
account-wide. No custom role controls appear.

## 10. Durable Working memory proof - BLOCKS persistence claims

**Surface unlocked:** a session-private Working memory pane that can truthfully say a note survived a
restart or later session while remaining absent from the team Repo graph.

**Today:** the CLI safely collects changed, staged, and non-ignored untracked Git files for the current turn
and sends them separately from Repo graph. That proves a bounded per-turn input path; it does not prove
durable session memory. The server persists prior messages for its answer path, but there is no public
identity/read/delete contract that can prove persistence and isolation from another session or account.

Required contract and two-sided acceptance proof:

- A server-issued memory/session identity bound to credential and repo, never a caller-provided account.
- A write acknowledgement with revision and `observed_at`, followed by a separate authenticated read after
  client restart that returns the exact note or an explicit absent reason.
- A read using a different session identity must not return the private note.
- Repo graph reads for the same repo must not return the private note unless a separate explicit commit or
  promotion operation occurred.
- Close/delete returns a receipt; a subsequent read returns absent rather than an empty success shape.
- Cross-tenant and unknown identities remain indistinguishable 404s.

**Missing-field rendering:** `memory unavailable` is omitted from passive chrome. Opening `/memory` states
that current uncommitted files are sent per turn and that durable private persistence is not yet proven.

## Shared absence and evidence rules

- Every optional capability has an explicit absent/unknown value and a reason field.
- Every observation has `observed_at`; every live snapshot has `stale_after_s`.
- Derived or inferred values carry evidence and render differently from measured/observed values.
- Empty sections state what is true and what the customer can do next.
- No client calculates a server fact from elapsed time, HTTP completion, or missing data.
