# The ACP loop — a walkthrough you run yourself (2026-08-11)

**The sentence this proves:** "my plan hit its limit, so I switched engines — no handoff needed, Estelle
already knew everything." Your plan does the thinking; Estelle does the grounding. Nobody pays twice.

**Proof status, stated before the steps:** every step below is wired and gate-tested (217 tui + 22 client
+ 9 acp tests, clippy clean). Steps 6–8 make REAL calls on YOUR plan and OUR server — wiremock covers them
in the suite, but the live run is yours to make, and it is the whole point. Claude Max OAuth exists
nowhere in the tree; this walkthrough is the ChatGPT half. **The absence is now MEASURED, not inferred:**
opencode (vendor clone, 31 provider plugins) does ChatGPT plan login with the same issuer and device flow
(`packages/core/src/plugin/provider/openai.ts`) and has NO Anthropic plan login at all — no claude.ai/oauth,
no Max, no subscription path (`packages/core/src/plugin/provider/anthropic.ts`). OpenAI publishes a device
flow for Codex; Anthropic publishes no equivalent. That is an ecosystem fact with a file path for a
citation, so nobody re-opens it every month.

## 0. Build the binary

```
cd cli-rs && cargo build --release --bin estelle
./target/release/estelle --version
```

Before trusting ANY prod answer in these steps, establish which build is serving — on 2026-08-13 prod
served ~1137 commits behind for 33 minutes because a dashboard variable change is an unguarded deploy.
Do not argue with the version field: make two sibling routes registered in one code block contradict each
other (a 200 beside a 404 is something no single build can do), and read a 404's BODY — the router's
generic not-found means the route is absent; a handler's own refusal means the route exists and is
declining. If a Railway variable was just changed, re-probe immediately and redeploy from HEAD.

## 1. Give the CLI its Estelle key (the fuel line — memory, graph, gate)

```
./target/release/estelle login
```

Screen: a masked `Estelle key:` prompt. Paste the key (get one at fatelabs.ca/dashboard/keys). It stores
to the OS keyring or `~/.estelle/auth.json` (0600). This key never pays for a model token in this loop —
it buys grounding only.

## 2. Log in with your ChatGPT plan (the engine)

```
./target/release/estelle login --chatgpt
```

Screen: a verification URL (`https://auth.openai.com/...`) and a user code. Open the URL, sign in with
your ChatGPT account, enter the code. Success screen reads:

```
Signed in with ChatGPT (device code).
ChatGPT account: acct-…
ChatGPT-plan credential stored at /Users/you/.estelle/chatgpt/auth.json (mode 0600).
Auth method: chatgpt-device-code
```

That last line is the evidence artifact's first half: the credential on disk is a PLAN credential.

## 3. Open a repo and give Estelle the graph

```
cd ~/your-repo
<path>/estelle sweep
```

Screen: the file inventory, the plan-fit check, then the ingest progress. Uncommitted files are included
— the sweep is `git ls-files --others --exclude-standard`, so working-tree work counts and `.gitignore`d
files never leave the machine.

## 4. Point an ACP editor at the binary

Zed: settings → agent servers →

```json
{ "estelle": { "command": "/Users/you/Desktop/estelle/cli-rs/target/release/estelle", "args": ["acp"] } }
```

Open a thread in the repo. (Any ACP client works; Zed is the reference.)

## 5. Ask for a change, and READ THE LAST LINE

Ask: "add a function `render_receipt` that formats the billing receipt, and wire it into the panel."

The answer streams as usual. **The last line is the receipt:**

```
— engine: your ChatGPT plan (device-code login) · grounding: estelle /search
```

That line is the proof: the model call went to `chatgpt.com/backend-api/codex` under YOUR plan token
(step 2's credential), and the only thing the Estelle key bought was the `/search` grounding above the
question. If instead you read `— engine: estelle server (your API key)`, the plan credential did not load
— check `~/.estelle/chatgpt/auth.json` exists, and if the line above it says your plan credential was
rejected, re-run step 2.

To see it from the outside while it happens: `lnav`/`tail` nothing — the receipt IS the log line. If you
want the network-level view, run the editor's session with `mitmproxy` or watch Activity Monitor's
outbound to `chatgpt.com` — no call to `api.openai.com` and no provider spend on the Estelle receipt.

## 6. The graph follows UNCOMMITTED work, mid-session

Accept the edit in the editor. The PostToolUse sync hook (installed once via `estelle install-hooks`)
reads the post-write bytes from disk and POSTs just that file to `/reindex` — debounced, gitignore-honoring,
secret-filtered, 8s-ceilinged, silent offline. No commit, no push, no manual sync.

## 7. Switch engines mid-task — the acceptance sentence

Hit your plan limit (or simulate one: `mv ~/.estelle/chatgpt/auth.json ~/.estelle/chatgpt/auth.json.away`).
Ask the next question in the same ACP thread. The receipt now reads `— engine: estelle server (your API
key)` and the answer still comes back grounded, because the context never lived in the model. Restore the
file and the next answer flips back to your plan. (The receipt line exists on the ACP path — the TUI and
the Claude Code plugin answer through the server and have no receipt to flip; the continuity is what you
are watching.) That flip, with continuity, is the whole product sentence. If you record one transcript
for the campaign, record this one.

## 8. The founder's acceptance test, still gated on the rotated key

In a scratch repo: write a function, DO NOT commit, then from a DIFFERENT session ask Estelle about that
symbol. It comes back iff the sync hook fired (step 6) and recall answers (`estelle recall "what does
<symbol> do"`). **One actionable unblock line:** paste the rotated Estelle key into `estelle login` once
(step 1) — every hook and command reads that store — then run this step. Until that key exists, this step
is BLOCKED, and it is the only step that is.

## Known limits of this walkthrough

- Server-side header enforcement at `chatgpt.com/backend-api/codex` is not knowable from source; what we
  SEND is asserted at the test boundary, and a rejection falls back loudly (step 5 tells you).
- The model slug comes from `GET /models` under your plan at session start; what your plan is entitled to
  is what you will see.
- `estelle acp` speaks ACP over stdio — an agent protocol, not MCP. The no-local-MCP ruling is untouched.

## The ACP shape review (2026-08-11, against opencode's acp/ — read for shape, nothing copied)

estelle-acp works; these are the gaps a client will notice, ordered, each verified against
`vendor-reference/opencode/packages/opencode/src/acp/`. Not bugs — a roadmap with citations:

1. **Tool-call lifecycle streaming** (`tool_call`/`tool_call_update` with locations and diff content —
  their tool.ts:124-228). We send text chunks only; Zed renders tool cards from this. The most visible gap.
2. **`stopReason` correctness** — `cancelled`/`max_tokens`/`refusal` and `usage` on the prompt response
  (their service.ts:824-873). Clients key UI off it.
3. **`session/request_permission`** — the moment any gated action exists, clients expect it (their
  permission.ts: per-session serialization, diff previews, fail-closed to reject).
4. **`usage_update` notifications + prompt-response usage** (their usage.ts) — the context-remaining bar.
5. **`session/load` + replay, `/resume`, `/list`, `/close`, `/fork`** — we advertise `loadSession:false`,
  so spec-compliant clients won't call them; resume-after-restart is a feature users notice.
6. **Model/mode selectors** (`configOptions` from session/new), **`initialize` authMethods**, rich prompt
  content (images, resource links with line refs), `agent_thought_chunk`, structured JSON-RPC errors
  (`authRequired` etc.).
7. **Client-provided MCP servers** — opencode accepts and dedups them per session; we REJECT them
  (deliberate divergence, estelle-acp/src/lib.rs:62-66). Also missing on BOTH sides: `terminal/*` methods.

Parity notes from the same read: our refresh is STRICTER than theirs (JWT-exp vs trusting `expires_in`; a
permanent-failure taxonomy they lack); single-flight exists on both; neither retries on 401; and their two
coexisting login implementations differ on refresh margin — the rot class is real, and our two edges
(claim fallbacks, metadata preservation) are ported in cli-rs `54e294a`. The three-type auth record
(api/oauth/wellknown) landed in `ef14216` — the client half of the MCP lane's discovery-based auth.
