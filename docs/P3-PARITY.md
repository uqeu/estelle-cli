> ## LOOP-REV: 14
> Start: `6e1fff21` - docs: post the gate-fix read-back for World 2
>
> Finish read-back: `f8384799` - docs(T2/T3 CLOSED): end to end over the wire

# P3 command parity record

## Result

P3 implements the 35-command session surface and all 14 top-level commands that belong to P3. The three
hook command names remain visible and return explicit P4 deferrals; they do not install or invoke a hollow
hook. The optimized binary starts the maintained Codex composer, resolves account and repo state after the
first frame, displays the Estelle command palette, reaches `api.fatelabs.ca`, and exits cleanly.

The TUI has one terminal writer: Ratatui's render loop. Questions, remote commands, git reads, shell
commands, apply and undo all finish through typed UI events. A 93-second request renders `thinking  93s  |
Esc cancels`; cancelling invalidates the request ID, so a late response cannot alter the transcript.

## Command denominator

All **35/35** accepted session names are claimed. Twenty-five have typed remote routes and ten are local by
design. Unknown commands send zero requests. `/route` aliases `/routing`; `/quit` aliases `/exit`; a unique
one-edit typo resolves without guessing between multiple commands. (`/graph` was added 2026-08-07 as the
first wire of the surface lane; the read family — `/me`, `/keys`, `/team`, `/cards`, `/entities` —
followed; see `ROUTE-INVENTORY-2026-08-07.md`.)

| session command | execution |
|---|---|
| help | local table generated from the command registry |
| init | scoped `GET /wiki` |
| graph | scoped `GET /graph`; counts, roots, explicit `building`/unswept states. `/graph nodes` → scoped `GET /graph/nodes`; nodes/edges, explicit `truncated` |
| me | `GET /me`; plan, balance, budget, provider, seats; pending invites explicit, omitted fields "not returned" |
| keys | `GET /me/keys`; prefixes + expiry/revoked state, raw keys never returned |
| team | `GET /me/team`; role, seat ledger, members, admin invites; null team renders as absent |
| cards | `GET /memory/cards`; non-zero folder counts, cards with provenance, edited flags |
| entities | scoped `GET /entities`; symbols with defining files, scope disclosed |
| usage | `GET /usage`; requests + tokens by day with series totals |
| activity | `GET /activity`; calls + tokens by endpoint, serving-model split shown |
| runs | `GET /runs`; team run history, ungrounded runs flagged with reasons |
| outcomes | `GET /outcomes`; accept/revert/reject counts and rates, conservatism cue surfaced |
| analytics | `GET /analytics`; runs/sessions/turns with repo/skill/outcome tallies |
| audit | `GET /audit`; privileged-action trail with chain state and reason |
| memory | scoped `POST /deep-search` |
| sweep | local hand-off to the top-level whole-tree command |
| sessions | `GET /sessions` |
| resume | `GET /session?id=...` |
| work | scoped `POST /work`; keeps the returned review diff |
| orchestra | scoped `POST /orchestra` with a non-empty task list |
| context | local transcript, queue and active-request state |
| gate | measured local diff, then scoped `POST /gate` |
| scan | measured local diff, then scoped `POST /scan` |
| improve | scoped `POST /improve` |
| verify | scoped `POST /verify` |
| apply | cancellable `git apply` of the last `/work` diff |
| undo | reverse only the last explicit Estelle apply |
| mode | local ceiling view; never writes an account override |
| routing | scoped `POST /route`; server owns the model policy |
| status | local endpoint, credential, repo, mode and connection state |
| skills | `GET /skills` |
| tools | JSON-RPC `tools/list` through `POST /mcp` |
| shell | local help; `!command` is the cancellable execution path |
| clear | local transcript reset |
| exit | local clean shutdown |

`/memories` (a graft name, not a session-registry row) was split off its `/memory` alias on
2026-08-07: it now routes to scoped `GET /memories` — the held-memory listing with the server's
trust tiers (`grounded`/`acquired`, `externally authored`) and the explicit `truncated` cap.
`/memory` keeps the deep-search answer.

All **17/17** canonical top-level names are claimed before TUI fallback. Fourteen execute their P3
contract; three name the phase that owns them instead of pretending to work.

| top-level command | P3 state |
|---|---|
| login | secure key read, remote verification, encrypted store |
| init | safe editor JSON merge, backup, `0600`, real MCP initialize, dry run |
| sweep | git-visible or plain walker, secret skip, size preflight, sync or live background ingest |
| reindex | changed plus deleted git evidence, preserves untouched graph, dry run |
| connect | offline instructions with a placeholder, never a stored key |
| remove / disconnect / off | offline safe removal with backup; other servers preserved |
| github | status, loopback OAuth link, proven installation selection, repos and explicit sweep |
| monitor | overview, issues, issue, alerts, uptime and logs |
| research | status, watch, off, drift, repair and scoped ask |
| memory | receipts, retract, forget, learned reflexes and unlearn |
| ask | scoped OpenAI-shaped completion |
| recall | scoped search with citations |
| verify | local file bytes to scoped verify |
| gate | staged or base diff to scoped merge gate |
| hook | explicit P4 refusal |
| install-hooks | explicit P4 refusal; no harness modified |
| uninstall-hooks | explicit P4 refusal; no harness modified |

## Fail-before-green evidence

| invariant | red proof | restored result |
|---|---|---|
| command inventory | `init` was not claimed by Clap | 17/17 canonical names parse before TUI fallback |
| execution contracts | `login` had no classified path | every top-level name is local, remote, compound or P4 |
| scoped GET repo | wiremock saw no `repo` on `GET /wiki` | `repo=fatelabs/estelle` is on the wire |
| non-empty GET query | `receipts --limit 2` failed live with reqwest `unsupported value` | wiremock and production both receive `limit=2` |
| shared secret boundary | a server message rendered a complete sentinel | all transcript and suite output passes the mask |
| shortened key namespace | production receipts printed `estelle_live_0b95827...` | namespace omitted; target, rows and reason retained |
| `/work` seam | routing the app seam to `/deep-search` left no diff | composer to client to typed renderer stores the diff |
| mode ownership | `/mode plan` queued an account-scope POST | zero requests; local ceiling never grants server power |
| Estelle palette | `/mo` displayed Codex `/model` | `/mode` and `/memory`, sourced from the 23-name registry |
| verify refusal | production reason was replaced by a generic fallback | exact fail-closed reason renders; no false verdict |
| suite empty states | live output said only `issues: none` / `rules: 0` | never-instrumented, no-pager and no-check states are explicit |
| GitHub callback | callback parser did not exist | fixed loopback path requires code and state, denial stays denial |
| callback origin | absolute attacker origin with the right path was accepted | exact `http://127.0.0.1:8788` origin required |
| GitHub installation | selection/status decisions did not exist | ambiguity returns to the human; bound installation is named |
| plain directory sweep | collection failed when git had no inventory | bounded walker finds source and skips hidden/build trees |
| large sweep | no 200-file transport decision existed | 199 uses `/sync`; 200+ uses `/ingest/start` and scoped progress |
| research flags | empty `watch` would have enrolled daily | empty update stays empty; cadence and custom APIs are typed |
| memory flags and scope | `--reason` became part of the subject and retract/forget used an unscoped client | bodies are distinct; repo-scoped erasures always send `repo` |
| editor merge | safe writer was absent | existing JSON preserved, backup created, mode `0600` |
| snapshot sensitivity | deliberate status and palette changes produced reviewed diffs | only the read Estelle baselines were accepted |

## Production read-back

The following came from the real server with the stored credential. No destructive memory, research,
GitHub or sweep mutation was run merely to make a demo green.

```text
$ estelle github
GitHub identity: not linked
Run: estelle github link

$ estelle monitor issues
monitor issues
No errors have reached Estelle yet. Point OTLP or Sentry at api.fatelabs.ca/monitor/ingest.

$ estelle monitor alerts
monitor alerts
No alert rules exist. Nothing will page you when production breaks.

$ estelle monitor uptime
monitor uptime
No uptime checks are registered.

$ estelle research
research status
NOT enrolled; nothing is being watched on a schedule. Run: estelle research watch stripe openai --cadence daily

$ estelle memory learned
memory learned
Estelle has not graduated any reflexes for this account; nothing is being applied on its own.
```

The corrected query path was also read back from production:

```text
$ estelle memory receipts --limit 2
memory receipts
count: 2
receipts: 2
- scope=source, target=key:aud0801_s11_deploy_target, rows=1, reason=audit probe, requested_by=phlotu@gmail.com
- scope=source, target=key:aud0801_s10_deploy_target, rows=1, reason=audit probe, requested_by=phlotu@gmail.com
```

The key namespace is absent. The proof fields remain.

Production verify refused rather than certifying an unswept repo:

```text
Not verified. this repo has not been swept, so there is nothing to ground against - run
`npx @fatelabs/estelle sweep` first. Refusing rather than passing.
```

A real `ask` stayed alive for about 103 seconds and returned a bounded answer saying the requested module
was not present in repo memory. The TUI renders the elapsed timer throughout that wait; the top-level
wrapper prints the completed answer.

## Snapshot review

All five baselines below were read. No `.snap.new` file was accepted unread.

### Empty composer

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories


















plan  |  server ...

> Compose new task

  ? for shortcuts                                            100% context left
```

### Composer with text

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories


















plan  |  server ...

> trace the charge path

                                                             100% context left
```

### Estelle slash menu

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories
















  /mode       read or lower the server autonomy ceiling
  /memory     what Estelle knows about this repo
plan  |  server ...

> /mo

                                                             100% context left
```

### Long-running query

```text
estelle  ...  |  uqeu/estelle  ...  |  ... files  ... chunks  ... memories

you  Which repair changed charge.ts?
















thinking  93s  |  Esc cancels

> Compose new task

  ? for shortcuts                                            100% context left
```

### Every failure class

```text
estelle  failed
Estelle rejected the stored credential.
The API reported that this credential is not authorized.
Authenticate again, then retry the question.

estelle  failed
Estelle returned HTTP 502: the server returned a non-Estelle error body
The failure is on the Estelle service path.
Retry once; if it repeats, narrow the question and report the status.

estelle  failed
Estelle returned HTTP 400: repo is required
The API refused this request as sent.
Correct the request or account state, then retry.

estelle  failed
The Estelle request exceeded 300 seconds.
The server did not complete the grounded answer in time.
Retry or ask a narrower question.

estelle  failed
The Estelle request could not reach a response.
The network path failed before the server returned a result.
Check connectivity and retry.

estelle  failed
The request was cancelled.
The client stopped waiting before the server answered.
Submit the question again when ready.

estelle  failed
The Estelle request failed: the response body was empty
The client could not accept the server result.
Retry; if it repeats, report this exact failure.
```

## Measurements and corrections

- `Cargo.lock` packages: **1,307**, unchanged from P2 and 39 below ADR 0016's 1,346 forecast.
- Release artifact: **23,767,048 bytes**, Mach-O arm64.
- Direct TUI dependency exceptions from P0: **19**, unchanged. No hollow crate was introduced.
- Estelle TUI binary tests: **43 passed**.
- `estelle-client`: **16 passed**, one credentialed live-network test ignored.

The port spec's endpoint audit was stale in two connected places. Production and the web client use
`GET /github/identity/authorize-url?redirect_uri=...`, but section 6 omitted it. Section 6 names
`/github/callback`; the endpoint enum had `/github/app/callback`; the terminal OAuth flow actually owns the
fixed local callback `http://127.0.0.1:8788/github/callback`. The typed client now contains the measured
authorize endpoint. It does not invent a server callback route for the local listener.

One server-lane finding remains outside this branch: a production `recall repository scope` returned
prompt-injection canaries (`FALCONGRIT8823`, `ROSEBUD-PRO-ONLY-9931`) as ordinary recalled content before
the relevant code. The CLI did not suppress or reinterpret the server answer; this needs the grounding
and memory lane, not a client-side denylist.

The six direct TUI dependencies forecast for deletion still compile real preserved Codex consumers, as
reported in P0/P2. Their count did not fall during the transport swap, and P3 does not stub them to make the
forecast look right.

## Validation

- `cargo test -p estelle-tui --bin estelle`: 43 passed.
- `cargo test -p estelle-client`: 16 passed, one explicit live test ignored.
- strict Clippy for `estelle-client --all-targets`: passed with warnings denied.
- strict Clippy for the Estelle TUI binary: passed with warnings denied.
- `cargo fmt --check -p estelle-client -p estelle-tui`: passed.
- `cargo build --release -p estelle-tui --bin estelle`: passed.
- optimized artifact launched in a real PTY, rendered `/help`, and exited through `/exit`.
- `println!` / `eprintln!` / `print!` / `eprint!` scan of the Estelle binary and client: zero matches.

The production pane, ACP, grafts and hooks remain unstarted. They belong to P4-P6 and require the server
work named in the brief.
