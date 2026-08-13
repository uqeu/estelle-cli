# The ACP loop — a walkthrough you run yourself (2026-08-11)

**The sentence this proves:** "my plan hit its limit, so I switched engines — no handoff needed, Estelle
already knew everything." Your plan does the thinking; Estelle does the grounding. Nobody pays twice.

**Proof status, stated before the steps:** every step below is wired and gate-tested (217 tui + 22 client
+ 9 acp tests, clippy clean). Steps 6–8 make REAL calls on YOUR plan and OUR server — wiremock covers them
in the suite, but the live run is yours to make, and it is the whole point. Claude Max OAuth exists
nowhere in the tree; this walkthrough is the ChatGPT half. That is a separate question with its own
evidence, not a hedge.

## 0. Build the binary

```
cd cli-rs && cargo build --release --bin estelle
./target/release/estelle --version
```

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
