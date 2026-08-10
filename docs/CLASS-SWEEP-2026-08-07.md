# Class sweep — client capabilities vs server doors — 2026-08-07

The class: a client capability with no server door, or client data travelling in a field meant
for something else. Three members were found before this sweep existed: the image path
(inherited-but-unreachable), the `/usage` graft shadow (stub-before-routing), and D16 (smuggled
through `question`). This sweep looks for the REST. Method: every outbound body the estelle
binary sends (from `commands.rs::remote_request`, `main.rs::spawn_*`/`answer_question`,
`top_level.rs`) cross-referenced key-by-key against what the server handler actually READS
(`body.get`/`params.get` in `src/estelle/serve/`, wiring in `scripts/estelle_server.py`), with
file:line evidence on both sides. Seven route groups, subagent-verified.

## Verdicts that need action

| # | Finding | Evidence | State |
|---|---|---|---|
| S1 | **`/improve <focus>` drops the user's argument.** Client sends `{"focus": ...}`; the server reads `body.get("path")`. The whole repo is scanned instead of the focus — silently. The one true key-name contract break. | client `commands.rs:589-596`; server `api_intel.py:734-735` | 🔴 **FIXED this commit** — client now sends `path`; red-first test pins it |
| S2 | **`repo` on `POST /retract` and `POST /forget` implies repo-scoped erasure; the server erases across ALL namespaces the caller owns.** The client requires a repo to fire the command and injects the key; the handler never reads it. A user can believe `memory forget X` in repo A left repo B's copy intact. It didn't. | client `endpoint.rs:80-81` + `lib.rs:327-333`; server `api_memory.py:564-589`, `api_erasure.py:82-129` | ✅ **FIXED (founder's call, both halves):** the CLI discloses the true radius before anything is sent (`--yes` required, "Nothing was sent" otherwise) and the repo requirement is dropped — the field demanded by the client and ignored by the server was the lie itself. Server-side honouring of `repo` is queued with the server lane |
| S3 | `working_memory` on `/deep-search` is a dead field carrying up to 80 KB of local file contents per non-conversational question. Known and consented at D16 — the server ignores it until register 14b ships the typed contract. | client `types.rs:114-117`; server: grep finds zero reads | 🟡 known, awaiting 14b. **Egress with zero server effect until then — flagged, not re-decided here** |
| S4 | `repo` injected but never read on `POST /scan`, `POST /route`, `GET /monitor/overview`. Harmless dead weight; the scan one means scoping is payload-only. | server `api_intel.py:656-676`, `api_account.py:9-31`, `api_monitor.py:443-463` | 🟡 recorded; safe to remove from endpoint `requires_repo` flags in a hygiene pass |
| S5 | `GET /monitor/overview`: client sends `window_s`, server reads `window`. Masked because the server default (3600) equals the client's hardcoded value; any future client change no-ops silently. | client `main.rs:2917`; server `api.py:831` | 🟡 **fix with the next monitor-pane touch** (one-word change, not worth its own commit) |
| S6 | The graph-currency marker `head` is read by `/sync`, `/ingest/start`, `/reindex` and the client never sends it — the indexed-HEAD baseline is permanently UNKNOWN for exactly the ingest path (CLI) that built the graph. The server docstring says the client must supply it; the client has the SHA locally and never puts it on the wire. | server `api_memory.py:36-39,147,220,277-278`; client `top_level.rs:1055,1116,1287` | ✅ **FIXED** — `with_measured_head` puts `git rev-parse HEAD` on all three bodies; the field is omitted when HEAD is unreadable, never invented. Red-first test on a temp repo |
| S7 | Server contract doors the client can't open (safe defaults today): `deep:true` on /gate (so TUI `/review` is wire-identical to `/gate` — deep review unreachable), `files` on /scan (lockfile CVE scan), `messages` on /skill/run (interactive skills restart single-turn), `partial` on /sync, `cwd` on /deep-search, `tokens`/`needs_reasoning` on /route, `surface` + POST on /autonomy/scope (register #72's remainder). | per-route evidence in the group audits | ⚪ recorded as capability gaps, none faked |

## Clean bills

- `/search`, `/gate` body, `/verify` (see N1), `/work`, `/orchestra`, `/skill/run`, `/autonomy`,
  `/settings/suite`, `/provider/select`, the sweep/ingest/reindex bodies, `/issues`,
  `/monitor/*` reads, `/vendor-drift*` (repair's `sources` carries local bytes in a DESIGNED,
  documented field — not smuggling), the github flow, `/instincts`, `/deletion-receipts`,
  `/unlearn` — every client-sent key is read; no server-required key is missing.
- GET reads wired this session (graph, me, keys, team, cards, entities, usage, activity, runs,
  outcomes, analytics, audit, requests, presence, leaderboard, billing, memories) send empty
  bodies + `?repo=` where scoped; the servers read exactly that.

## Shape 2 — shadowing

Closed by `no_graft_stub_shadows_a_wired_remote_route` (mutant-proven). `/usage` was the only
shadow; the test now guards the class.

## Shape 1 — inherited-but-unreachable

The estelle binary drives its own `main.rs`; the inherited Codex lib compiles in but is never
called by it. Members with outward effect, state each:

| Member | State |
|---|---|
| Image paste (`clipboard_paste.rs`) | ✅ probed 2026-08-07 — unreachable; guard test goes red if wired unsafely |
| Feedback upload (Sentry) | ✅ **removed end to end** (`a891d12`) — was the worst member of this class |
| `/feedback`-shaped app-server request | ✅ answers "removed — nothing was sent" |
| Codex chatwidget UI, slash registry, onboarding, bottom_pane popups, model popups | ⚪ compiled, never instantiated by `main.rs`; harmless until wired — and this week proved what "wired carelessly" means |
| `estelle ask` headless raw chat (D6) | 🔴 FILED — routes around `/deep-search`; its own defect entry, not fixed mid-lane |
| `Endpoint::Checkpoint` declaration | ⚪ dead declaration (the TUI checkpoint is the local session_gap mechanism) — delete or wire |
| `Endpoint::GithubAppCallback` | ⚪ correctly never called (browser redirect target) — candidate for removal from the table |

## Notes for the server lane

- N1: `/verify`'s `answer` carries whole local files (headless verify + the ground hook); the
  server's typed `context` field exists and goes unused. Provenance (path/line) is lost in the
  string. Server-side call whether that matters.
- N2: `POST /monitor/logs` (ingest) shares a path with GET (search). A client POSTing `{query}`
  there would be parsed as log lines. No client does today; worth a server-side shape guard.
- N3: `/github/connect` exists server-side with `{repo, ref?}`; the client never calls it (its
  connect is `/github/app/setup`). Recorded so nobody wires the wrong one.
