#!/usr/bin/env node
/**
 * estelle — connect Estelle to the AI coding agent you already use, in one command.
 *
 *   npx @fatelabs/estelle init            # auto-detect your editors, write the MCP config
 *   npx @fatelabs/estelle init --client cursor
 *   npx @fatelabs/estelle sweep           # ingest the current repo into your Estelle memory
 *   npx @fatelabs/estelle connect cursor  # print the one-liner / config for one client
 *   npx @fatelabs/estelle remove          # disconnect: delete Estelle from every editor config (offline)
 *
 * Zero dependencies (Node built-ins only) so `npx` is instant. Egress is per-command and only when you
 * run it: `init`/`remove`/`connect` write or print local editor config (init also pings your Estelle
 * endpoint once to VERIFY the key works); `sweep` sends this repo's git-visible source files; `gate`
 * sends your staged diff; `verify` sends one file; `ask`/`recall` send your question — each to your
 * Estelle endpoint (default api.fatelabs.ca), authed by your key.
 * Your agent keeps running on YOUR plan; Estelle rides alongside over the hosted API, scoped to your key.
 */
"use strict";
const fs = require("fs");
const path = require("path");
const os = require("os");
const readline = require("readline");
const { execFileSync } = require("child_process");
// The one module that knows where a stored key lives. Required at the TOP, not lazily through repl.js,
// because "what key am I running as" is asked by almost every command and must not depend on loading the
// interactive session's dependency tree. See resolveKey() below — that is the only door in this file.
const auth = require("./auth.js");
const repoName = require("./repo-name.js");
const { askSecret } = require("./secret-prompt.js");

const MCP_URL = process.env.ESTELLE_MCP_URL || "https://api.fatelabs.ca/mcp";
const API = MCP_URL.replace(/\/mcp$/, "");
// Where an over-capacity sweep sends you. The marketing site, not the API host — an upgrade link that
// points at api.* is a dead end at exactly the moment the customer wants to pay.
const PRICING_URL = "https://fatelabs.ca/pricing";

// ── pretty terminal ────────────────────────────────────────────────────────────
// ONE palette (brief §1.2). These were seven hand-picked `38;5;NN` strings, which is how a brand colour and
// an error colour drift into each other one file at a time — and how the CLI ended up TEAL, a colour nobody
// chose. Estelle is a LIGHT RED; `teal` is now an alias onto the brand, so adopting the module is what
// actually recolours the product. See palette.js for the two-way separation from the failure colour.
const palette = require("./palette.js");
const { brand, dim, bold, green, amber, grey, red, teal } = palette;
const ok = palette.ok(palette.GLYPH.ok), arrow = brand(palette.GLYPH.arrow),
      dot = grey(palette.GLYPH.bullet);

function banner() {
  console.log("");
  // No tagline. "code memory + a 0%-hallucination gate" named two features out of many and read as a
  // product blurb in a place nobody wants one — a CLI banner should say WHOSE tool this is and get out
  // of the way. The wordmark is the whole message; what Estelle does is what the next line does.
  console.log("  " + bold(teal("estelle")) + dim("  by Fate Labs"));
  console.log("");
}

// ── MCP client config (kept in lockstep with estelle.serve.install) ────────────
// $HOME configs that REALLY read the {mcpServers:{…}} + Bearer schema — safe to auto-write.
const CLIENT_FILES = {
  cursor: ".cursor/mcp.json",
  cline: "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
  windsurf: ".codeium/windsurf/mcp_config.json",
  // JetBrains' Junie agent reads a real $HOME MCP file with the same {mcpServers}+Bearer schema — auto-writable.
  jetbrains: ".junie/mcp/mcp.json",
};
// VS Code reads MCP servers from the WORKSPACE `.vscode/mcp.json` under a top-level `servers` key —
// NOT `mcpServers`, and NOT a `~/.vscode/` file (the same `servers` schema install.py documents for
// Visual Studio). Writing the generic blob there would be silently ignored.
const VSCODE_WORKSPACE_FILE = path.join(".vscode", "mcp.json");
// Clients the generic writer CANNOT correctly configure — Claude Desktop only launches local stdio
// `command` servers from its config, and Continue reads MCP servers from config.yaml, not a JSON
// mcpServers block. Writing a config they ignore and printing ✓ would be a lie; print the truth instead.
const NOTE_CLIENTS = {
  "claude-desktop": () => "Claude Desktop launches local stdio servers only — add Estelle's hosted MCP via "
    + `Settings → Connectors → Add custom connector → ${MCP_URL}`,
  continue: () => "Continue reads MCP servers from ~/.continue/config.yaml — add under `mcpServers:` a block "
    + `with type: streamable-http and url: ${MCP_URL} (docs.continue.dev → MCP)`,
};
// Where each client leaves a footprint on disk (relative to $HOME) — used only for DETECTION.
const DETECT_DIRS = {
  cursor: ".cursor",
  "claude-desktop": "Library/Application Support/Claude",
  cline: "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev",
  windsurf: ".codeium/windsurf",
  continue: ".continue",
  vscode: ".vscode",
  jetbrains: ".junie",
};
const PRETTY = { cursor: "Cursor", "claude-desktop": "Claude Desktop", cline: "Cline",
  windsurf: "Windsurf", continue: "Continue", vscode: "VS Code", "claude-code": "Claude Code",
  jetbrains: "JetBrains (Junie)" };

const serverEntry = (key) => ({ type: "http", url: MCP_URL, headers: { Authorization: `Bearer ${key}` } });

function mergeConfig(existing, key) {
  const base = existing && typeof existing === "object" ? { ...existing } : {};
  const servers = base.mcpServers && typeof base.mcpServers === "object" ? { ...base.mcpServers } : {};
  servers.estelle = serverEntry(key);
  return { ...base, mcpServers: servers };
}
// VS Code's schema: top-level `servers`, same entry shape.
function mergeVsCode(existing, key) {
  const base = existing && typeof existing === "object" ? { ...existing } : {};
  const servers = base.servers && typeof base.servers === "object" ? { ...base.servers } : {};
  servers.estelle = serverEntry(key);
  return { ...base, servers };
}
function configPath(client) {
  if (client === "vscode") return path.resolve(VSCODE_WORKSPACE_FILE); // the WORKSPACE file, not $HOME
  return CLIENT_FILES[client] ? path.join(os.homedir(), CLIENT_FILES[client]) : null;
}

function detectClients() {
  const home = os.homedir();
  return Object.keys(DETECT_DIRS).filter((c) => {
    if (c === "vscode") return fs.existsSync(path.join(home, ".vscode")) || fs.existsSync(path.resolve(".vscode"));
    return fs.existsSync(path.join(home, DETECT_DIRS[c]));
  });
}
// Pinned to a VETTED release, bumped deliberately — `@latest` would make a third-party package an
// un-staged update channel into every customer's editor config.
const installMcp = (client) => `npx -y install-mcp@1.10.2 ${MCP_URL} --client ${client}`;
// The key is referenced via $ESTELLE_API_KEY so the secret never lands in shell history or `ps` output.
const claudeAdd = () => `claude mcp add --transport http estelle ${MCP_URL} --header "Authorization: Bearer $ESTELLE_API_KEY"`;
const exportHint = "export ESTELLE_API_KEY=<your Estelle key>   # set it first — keeps the key out of the command";

function writeClient(client, key, dry) {
  if (NOTE_CLIENTS[client]) return { client, note: NOTE_CLIENTS[client]() }; // never write a config the client ignores
  const p = configPath(client);
  if (!p) return { client, note: "run: " + claudeAdd() }; // Claude Code = a command, not a file
  // SAME RULE AS `install-hooks`, SAME SPELLING. This function had the F0 defect too: a missing config and
  // an UNPARSEABLE one both became `existing = null`, so a customer's editor config with one trailing comma
  // was replaced by an Estelle-only file. The `.bak` saved the bytes, but the report below was
  // `backup: existing !== null` — FALSE in exactly the unparseable case — so the "(backed up)" note was
  // suppressed precisely when the customer most needed to be told where their config went.
  //
  // ONE PREDICATE DOING DOUBLE DUTY: `existing !== null` meant BOTH "a prior config parsed" AND was reported
  // as "we made a backup". Those agree in every case except the one that matters. `backup` is now the
  // literal fact — did we copy a file — and parse failure is its own outcome rather than a silent default.
  const hadFile = fs.existsSync(p);
  let existing = null;
  if (hadFile) {
    let raw;
    try { raw = fs.readFileSync(p, "utf8"); } catch (e) { return { client, path: p, refused: `cannot read it (${e.code || e.message})` }; }
    try { existing = JSON.parse(raw); } catch (e) { return { client, path: p, refused: String(e.message).split("\n")[0] }; }
    // Valid JSON is not enough: `[]`, `"text"` and `null` all parse and would merge into nonsense.
    if (existing === null || typeof existing !== "object" || Array.isArray(existing)) {
      return { client, path: p, refused: "not a JSON object (found " + (Array.isArray(existing) ? "an array" : existing === null ? "null" : typeof existing) + ")" };
    }
  }
  const merged = client === "vscode" ? mergeVsCode(existing, key) : mergeConfig(existing, key);
  const total = Object.keys(client === "vscode" ? merged.servers : merged.mcpServers).length;
  if (dry) return { client, path: p, dry: true, total };
  fs.mkdirSync(path.dirname(p), { recursive: true });
  if (hadFile) fs.copyFileSync(p, p + ".bak");
  fs.writeFileSync(p, JSON.stringify(merged, null, 2) + "\n");
  return { client, path: p, wrote: true, backup: hadFile };
}

// EVERY call the CLI makes to the API is BOUNDED. It was not: `fetch` has no default timeout, so a server
// that accepted a connection and never answered hung the customer's terminal with no way out but Ctrl-C —
// and `sweep`/`gate` had no bound at all. The Python hook has bounded at 8s the whole time, so the two
// halves of the same product disagreed about whether a wait may be infinite, and only the one a customer
// runs in the foreground was unbounded. Defect class 7.
//
// The number is generous on purpose: a first `sweep` on a large repo legitimately takes minutes, so this is
// a LIVENESS bound — "this connection is dead" — not a latency budget. AbortSignal.timeout rejects with an
// AbortError, which the existing catch already turns into { ok: false, status: 0 }, so every caller's error
// path is unchanged.
const API_TIMEOUT_MS = Number(process.env.ESTELLE_HTTP_TIMEOUT_MS || 120000);

// POST a real MCP `initialize` to the endpoint — the config only counts as CONNECTED when Estelle answers.
async function verifyMcp(key) {
  try {
    const res = await fetch(MCP_URL, {
      method: "POST",
      headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/json",
        Accept: "application/json, text/event-stream" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize",
        params: { protocolVersion: "2025-03-26", capabilities: {},
          clientInfo: { name: "estelle-cli", version: "0.1.0" } } }),
      // `init` blocks a human at a prompt on this one, so it gets a SHORTER bound than the data calls: a
      // connectivity check that hangs is worse than one that says "could not reach Estelle" in 15 seconds.
      signal: AbortSignal.timeout(15000),
    });
    const text = await res.text().catch(() => "");
    if (res.ok && text.includes('"result"')) return { ok: true };
    return { ok: false, why: `HTTP ${res.status}` };
  } catch (e) {
    return { ok: false, why: String((e && e.message) || e) };
  }
}

// ── prompts ─────────────────────────────────────────────────────────────────────
function ask(q) {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((res) => rl.question(q, (a) => { rl.close(); res(a.trim()); }));
}
function flag(name, def) {
  const i = process.argv.indexOf(name);
  return i > -1 && process.argv[i + 1] ? process.argv[i + 1] : def;
}
const has = (name) => process.argv.includes(name);
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

/**
 * THE one place a command asks "what key am I running as": `--key` > `$ESTELLE_API_KEY` > the key the
 * session stored at ~/.estelle/auth.json.
 *
 * Every command used to inline `flag("--key", process.env.ESTELLE_API_KEY)` and stop there, which meant
 * the REPL could save a key, print "Run `estelle sweep`", and have that exact command answer
 * "Need --key" — the first thing a new customer does after signing in, failing. argv stays this file's
 * business (only the entry point owns it); WHERE the stored key lives is auth.js's, and there is now one
 * of each rather than a copy per call site.
 */
const resolveKey = () => flag("--key", auth.storedKey());

// ── commands ─────────────────────────────────────────────────────────────────────
async function cmdInit() {
  banner();
  // A customer who already pasted a key into the session should not be asked for it again to wire an
  // editor up — resolveKey finds it, and the prompt below stays for the genuine first run.
  let key = resolveKey();
  if (!key) {
    console.log("  " + bold("Connect your coding agent to Estelle."));
    console.log("  " + dim("Paste your Estelle API key (or get one free at ") + teal("fatelabs.ca") + dim(").") );
    console.log("");
    // MASKED, always. `ask()` attaches stdout in terminal mode, which echoes — so this prompt printed the
    // customer's live key into their scrollback on the first command they ever run. See secret-prompt.js.
    key = await askSecret("  " + arrow + " Estelle key: ");
    if (!key) { console.log("\n  " + amber("No key — nothing written.") + dim("  Get one at fatelabs.ca, then re-run.")); return; }
  }
  const dry = has("--dry-run");
  const only = flag("--client");
  const clients = only ? [only] : detectClients();
  console.log("");
  if (!clients.length) {
    console.log("  " + amber("No editor detected.") + " Use the one-liner for yours:");
    for (const c of ["cursor", "claude", "windsurf", "vscode"]) console.log("    " + dim(installMcp(c)));
    console.log("    " + dim(exportHint));
    console.log("    " + dim(claudeAdd()) + dim("   # Claude Code"));
    return;
  }
  console.log("  " + dim("Detected ") + clients.map((c) => bold(PRETTY[c] || c)).join(dim(", ")));
  console.log("");
  let wroteAny = false;
  for (const client of clients) {
    const r = writeClient(client, key, dry);
    wroteAny = wroteAny || Boolean(r.wrote);
    if (r.refused) {
      // Refuse THIS client and keep going — one unreadable config must not block wiring up the others —
      // but the run as a whole exits non-zero, because a partial install is not a success.
      console.log("  " + red("✗") + " " + bold(PRETTY[client] || client)
                  + dim("  →  refusing to overwrite a config it cannot read"));
      console.log("    " + dim(r.path));
      console.log("    " + dim(r.refused));
      console.log("    " + dim("Fix or move that file and re-run — Estelle will not discard settings it cannot parse."));
      process.exitCode = 1;
    } else if (r.note) console.log("  " + arrow + " " + bold(PRETTY[client] || client) + dim("  →  ") + r.note);
    else if (r.dry) console.log("  " + arrow + " " + bold(PRETTY[client] || client) + dim("  would write ") + grey(r.path));
    else console.log("  " + ok + " " + bold(PRETTY[client] || client) + dim("  " + r.path) + (r.backup ? dim("  (backed up)") : ""));
  }
  console.log("");
  // A REFUSAL MUST NOT BE FOLLOWED BY A SUCCESS NARRATIVE. This block used to print unconditionally, so a
  // customer whose config was refused was still told "Restart your editor — your agent now has
  // find_definition · …" for tools it had just declined to wire up. Nothing was written; saying otherwise
  // is the same defect class as the bug this whole path exists to fix — a confident claim about something
  // that did not happen. Caught by reading the ACTUAL stdout of the shipped 0.1.8, not the source.
  if (!wroteAny && !dry) {
    console.log("  " + bold("Nothing was written.") + dim("  Fix the config above and re-run — your editor is unchanged."));
    console.log("");
    return;
  }
  console.log("  " + bold("Next"));
  console.log("  " + dot + " Restart your editor — your agent now has " + teal("find_definition · find_references · blast_radius · verify"));
  console.log("  " + dot + " Ingest this repo so the tools know your code:  " + bold("npx @fatelabs/estelle sweep --key <KEY>"));
  console.log("  " + dot + " Turn Estelle off any time: " + bold("npx @fatelabs/estelle remove") + dim("  (works offline)"));
  console.log("");
  if (dry || !wroteAny) return;
  // A ✓ is only honest if Estelle actually answers — verify before claiming the connection works.
  process.stdout.write("  " + dim(`Verifying Estelle answers at ${MCP_URL}… `));
  const v = await verifyMcp(key);
  if (v.ok) {
    console.log(ok);
    // PERSIST THE KEY WE JUST VALIDATED — this is the half that was missing, and it broke the README's
    // own flow end to end: `init` prompted, used the key to write the MCP configs, and DISCARDED it, so
    // the literal next command the tool prints ("npx @fatelabs/estelle sweep") answered "Need --key".
    // The READ side was already unified (auth.js:4-8 documents that fix); the WRITE was added to the REPL
    // (repl.js:203) and never to `init` — and `init` is the path the README documents. A two-path defect
    // whose completion criterion was "the path I was looking at" rather than "both paths, asserted".
    // Skipped when $ESTELLE_API_KEY supplied the key: that is the CI shape, a deliberate per-process
    // override, and writing a credential to the disk of a shared runner is not what that caller asked for.
    if (!process.env.ESTELLE_API_KEY && key !== auth.readAuth()) {
      auth.writeAuth(key);                                     // 0700/0600, the same as the REPL's
      console.log("  " + ok + " " + dim(`key saved to ${auth.authFile()} — no more --key`));
    }
    console.log("  " + green("You're connected.") + dim("  Ask your agent anything about your codebase — it won't invent an API."));
  } else {
    console.log(amber("failed"));
    console.log("  " + amber(`Configs are written, but Estelle did NOT answer (${v.why}) — connection unverified.`));
    console.log("  " + dim(`Check your key and that ${MCP_URL} is reachable, then re-run init.`));
    process.exitCode = 1;
  }
  console.log("");
}

// Small repos upload in one synchronous /sync; bigger ones use the background ingest + live progress.
const SYNC_MAX_FILES = 200;
const POLL_MS = Number(process.env.ESTELLE_POLL_MS || 2000);
const MAX_POLLS = 7200; // ~4 h at the default cadence — the ingest keeps running server-side regardless
// Key shapes the gate itself scans for — a file embedding a live-looking secret NEVER leaves the machine.
// Defined once in secrets.js so the sweep, the always-on sync hook and the Python hook cannot drift apart:
// until this was split out, the sweep had the rule and BOTH hooks had nothing.
const { SECRET_RE } = require("./secrets.js");
const gh = require("./github-connect.js");

/** Best-effort browser launch for the GitHub authorize step. The URL is ALWAYS printed first, so a headless
 *  box, an SSH session or a locked-down opener degrades to "copy this link" rather than to a dead end.
 *  execFileSync with an argv array — never a shell — so the URL cannot be interpreted as a command. */
function openUrl(url) {
  const argv = process.platform === "darwin" ? ["open", [url]]
    : process.platform === "win32" ? ["cmd", ["/c", "start", "", url]]
    : ["xdg-open", [url]];
  try {
    execFileSync(argv[0], argv[1], { stdio: "ignore", timeout: 5000 });
  } catch (_) { /* no opener here — the printed link is the path */ }
}

const filePayload = (files) => files.map((f) => ({ path: f.path, content: f.content }));

/** Split a collected set into what may travel and what may NOT — a file embedding a live-looking secret
 * stays on the machine. Pure, so the rule is testable without touching a disk or a network. */
const partitionSecrets = (collected) => ({
  files: collected.filter((f) => !SECRET_RE.test(f.content)),
  flagged: collected.filter((f) => SECRET_RE.test(f.content)),
});

// The ingest body. `repo` FILES this sweep under a name so it lands in its own namespace instead of the
// shared unfiled one; omitted only when we genuinely could not work out a name.
const syncBody = (files, repo) =>
  (repo ? { files: filePayload(files), repo } : { files: filePayload(files) });

// ── the size gate: does this repo even FIT before we upload it? ────────────────
//
// A sweep used to start blind. Point a Free account (10M memory-tokens) at a 70M-token repo and the upload
// ran to completion while the server quietly declined file after file at the cap — the first experience of
// Estelle was a half-indexed repo with no warning it was ever going to fit. So we ask first, and we ask
// CHEAPLY: paths and byte sizes only, never content, so measuring a 70M repo costs one small request.
const estimateBody = (files, repo) => ({
  ...(repo ? { repo } : {}),
  files: files.map((f) => ({ path: f.path, bytes: Buffer.byteLength(f.content, "utf8") })),
});

/** Memory-tokens as the label the pricing page uses ("42M", "1.2M", "12K"). */
function mtok(n) {
  const v = Number(n) || 0;
  if (v >= 1e6) return (v / 1e6).toFixed(1).replace(/\.0$/, "") + "M";
  if (v >= 1e3) return (v / 1e3).toFixed(1).replace(/\.0$/, "") + "K";
  return String(Math.round(v));
}

/** The refusal, rendered as plain lines: what the repo needs, what the account has, the cheapest plan that
 * would fit, and the two ways out that are not "pay us" (a narrower path, or excluding the big directories).
 * Pure — a block a user cannot act on is a dead end, and this fires at the moment they want the product. */
function sweepFitLines(est) {
  if (!est || typeof est !== "object") return [];
  const lines = [`${est.repo || "This repo"} needs about ${mtok(est.estimated_tokens)} memory-tokens.`];
  if (est.cap) {
    lines.push(`Your plan holds ${mtok(est.cap)} (${mtok(est.remaining_tokens)} free) — `
      + `${mtok(est.blocked_tokens)} would not fit.`);
  }
  const plan = est.suggested_plan;
  if (plan) lines.push(`${plan.plan} (${mtok(plan.cap)}) fits — $${plan.monthly_usd}/mo: ${PRICING_URL}`);
  const top = (Array.isArray(est.largest_paths) ? est.largest_paths : []).slice(0, 3);
  if (top.length) lines.push("Biggest: " + top.map((t) => `${t.path} ${mtok(t.tokens)}`).join("  ·  "));
  lines.push("Or sweep a narrower path:  npx @fatelabs/estelle sweep --path <dir>");
  return lines;
}

function showTooBig(est) {
  console.log("  " + amber("This sweep will not fit your capacity — nothing was uploaded."));
  for (const line of sweepFitLines(est)) console.log("  " + dim(line));
  process.exitCode = 1;
}

/** Ask the server what this repo would cost the account's capacity. Returns false when it will NOT fit
 * (message already printed) — the caller must not upload.
 *
 * A failed estimate does NOT block: the SAME measurement is enforced server-side inside /sync and
 * /ingest/start, so the repo is gated either way and an older Estelle (404 here) has no gate on either
 * path — refusing locally would only break `sweep` against every server predating this release. This call
 * buys an earlier, kinder failure; it is not the thing standing between a 70M repo and a 10M cap. */
async function preflightSize(files, key, repo) {
  const r = await apiPost("/sweep/estimate", estimateBody(files, repo), key);
  if (!r.ok) {
    if (r.status !== 404) console.log("  " + dim(`Could not size this repo first (${errText(r) || r.status}).`));
    return true;
  }
  const est = r.json || {};
  if (est.fits === false) { showTooBig(est); return false; }
  if (est.cap) {
    console.log("  " + dim(`Fits: about ${mtok(est.estimated_tokens)} of your ${mtok(est.cap)} capacity`
      + `${est.billable_tokens ? ` (${mtok(est.billable_tokens)} bills as overflow)` : ""}.`));
  }
  return true;
}

// Honesty guard: the server discloses anything its caps dropped ("dropped": {reason: n}) — a green
// "Repo swept." over a half-indexed repo is a false completeness claim, so warn LOUDLY instead.
function warnDropped(json) {
  const d = json && json.dropped;
  if (!d || typeof d !== "object") return false;
  const total = Object.values(d).reduce((a, b) => a + (Number(b) || 0), 0);
  if (!total) return false;
  const reasons = Object.entries(d).map(([k, v]) => `${k}: ${v}`).join(", ");
  console.log("  " + amber(`${total} file(s) were NOT indexed`) + dim(` (${reasons}) — recall cannot cite them.`));
  // The FALLBACK text an older server leaves us to write. It used to say "batch the remainder across
  // further sweeps", which is the destructive advice: /sync rebuilds the code graph from exactly what it
  // receives, so a second sweep of the remainder leaves a graph holding only the remainder. Never tell a
  // customer to do that — point at the two routes that keep the graph whole.
  console.log("  " + dim(oneLine(json.warning ||
    "Re-run the sweep over the whole repo, or narrow it with --path. Do NOT sweep the remainder separately: " +
    "a sweep rebuilds the code graph from exactly what it sends.")));
  return true;
}

/** A refusal the SERVER made on size grounds carries the estimate — render it, not a bare HTTP code. The
 * server is the enforcement point (the preflight above is only an earlier, kinder failure), so this is the
 * path a stale CLI, a raced upgrade, or a repo that grew between the two calls actually takes. */
function failedOnSize(r) {
  return r.status === 402 && r.json && r.json.estimate && r.json.estimate.fits === false;
}

async function syncUpload(files, key, repo) {
  process.stdout.write("  " + dim("Uploading to your Estelle memory… "));
  const r = await apiPost("/sync", syncBody(files, repo), key);
  if (failedOnSize(r)) { console.log(""); return showTooBig(r.json.estimate); }
  if (!r.ok) return fail(r);
  console.log(ok);
  console.log("");
  if (warnDropped(r.json)) {
    console.log("  " + green("Repo partially swept.") + dim("  Indexed files are recallable; the dropped ones are not."));
  } else {
    console.log("  " + green("Repo swept.") + dim("  Your agent can now recall and verify against your real code."));
  }
  console.log("");
}

// The progress endpoint is scoped per repo, exactly like the sweep that writes it.
const progressPath = (repo) =>
  (repo ? `/ingest/progress?repo=${encodeURIComponent(repo)}` : "/ingest/progress");

async function ingestWithProgress(files, key, repo) {
  const r = await apiPost("/ingest/start", syncBody(files, repo), key);
  if (failedOnSize(r)) return showTooBig(r.json.estimate);
  if (!r.ok) {
    const msg = errText(r);
    // A server that predates /ingest/start 404s the route (not the key) — fall back to the sync path.
    if (r.status === 404 && !/api key/i.test(msg)) return syncUpload(files, key, repo);
    return fail(r);
  }
  warnDropped(r.json); // the start envelope discloses cap-dropped files — surface it before the progress bar
  console.log("  " + dim("Ingestion started — a one-time setup. Live progress (safe to Ctrl-C; it continues server-side):"));
  let stale = 0;
  for (let polls = 0; polls < MAX_POLLS; polls++) {
    await sleep(POLL_MS);
    // Poll the SAME key the ingest writes under. Without the repo this read the base namespace and
    // reported "idle 0%" forever while the ingest ran to completion server-side — the progress bar said
    // nothing was happening through an entire 1500-file sweep.
    const p = await apiGet(progressPath(repo), key);
    if (!p.ok) { if (++stale > 5) { console.log(""); return fail(p); } continue; }
    stale = 0;
    const v = p.json || {};
    const pct = v.percent != null ? v.percent : 0;
    const eta = v.state === "ingesting" && v.eta_label ? ` ${dot} ~${v.eta_label} left` : "";
    process.stdout.write("\r  " + dim(`${v.state || "ingesting"} ${dot} ${pct}%${eta}          `));
    if (v.state === "done") {
      console.log("");
      console.log("");
      console.log("  " + green("Repo swept.") + dim("  " + oneLine(v.message || "Your agent can now recall and verify against your real code.")));
      console.log("");
      return;
    }
    if (v.state === "error") {
      console.log("");
      console.log("  " + amber("Ingest failed: ") + dim(oneLine(v.message || v.error || "unknown error") + "  — safe to retry: re-run the sweep."));
      process.exitCode = 1;
      return;
    }
    // TERMINAL, and the one this loop used to miss. A worker killed by a restart writes no done/error, so
    // the snapshot stayed "ingesting" for ever and we told the user "the ingest continues server-side" —
    // which was false. The server now derives it; stopping here is what makes it actionable.
    if (v.state === "stalled") {
      console.log("");
      console.log("  " + amber("Ingest stopped: ") + dim(oneLine(v.message || v.error || "no progress") + `  (reached ${pct}%)`));
      process.exitCode = 1;
      return;
    }
  }
  console.log("");
  console.log("  " + amber("Stopped polling — the ingest continues server-side; check the dashboard."));
  process.exitCode = 1;
}

async function cmdSweep() {
  banner();
  const key = resolveKey();
  // Same rule as reindex: no key is a failure, not a no-op. A green CI step that ingested nothing is how a
  // repo silently stops being swept while every dashboard still says it is connected.
  if (!key) { console.log("  " + amber("Need an Estelle key to sweep — pass ") + bold("--key <KEY>") + amber(", set ") + bold("ESTELLE_API_KEY") + amber(", or run ") + bold("estelle") + amber(" once to save one.")); process.exitCode = 1; return; }
  const root = flag("--path", process.cwd());
  console.log("  " + dim("Scanning ") + bold(root) + dim(" for source files…"));
  const { files, flagged } = partitionSecrets(collectFiles(root));
  if (flagged.length) {
    console.log("  " + amber(`Skipped ${flagged.length} file(s) that look like they contain a live secret — they stay on your machine:`));
    for (const f of flagged.slice(0, 8)) console.log("    " + red("✗") + " " + f.path);
  }
  if (!files.length) { console.log("  " + amber("No ingestable source files found here.")); return; }
  const bytes = files.reduce((n, f) => n + f.content.length, 0);
  console.log("  " + dim(`Found ${bold(String(files.length))}${dim(" files")} ${dot} ${(bytes / 1024).toFixed(0)} KB ${dot} these upload to ${API}`));
  const repo = repoNameFor(root);
  if (repo) console.log("  " + dim("Filing under repo ") + bold(repo) + dim(" — recall stays scoped to it."));
  // Measure BEFORE uploading. Anything else means a repo that cannot fit still travels the wire in full and
  // lands half-indexed, which is the bug this command had.
  if (!(await preflightSize(files, key, repo))) return;
  if (files.length < SYNC_MAX_FILES) { await syncUpload(files, key, repo); return; }
  await ingestWithProgress(files, key, repo);
}

// INCREMENTAL update — the counterpart to `sweep`, and the one you want day to day.
//
// `sweep` hands /sync the whole tree, and /sync REBUILDS the grounding surface from exactly what it receives.
// That is correct for a fresh start and wrong for keeping up: sync a handful of changed files and every other
// file's symbols vanish, after which the gate flags real APIs as ungrounded. /reindex is the incremental path
// (the one the GitHub push webhook uses) — changed files are replaced, untouched files survive.
//
//   estelle reindex                 # every file git reports as changed vs HEAD
//   estelle reindex src/a.py src/b.py

/** Say out loud that some paths were left out, and why. Every skip is SPOKEN: a silent one is how a customer
 * concludes a file is indexed when it never was. Capped at 8 because a wall of paths gets skimmed, and a
 * warning nobody reads is the same as no warning. */
function announceSkipped(headline, paths, note) {
  if (!paths.length) return;
  console.log("  " + amber(headline));
  for (const p of paths.slice(0, 8)) console.log("    " + red("✗") + " " + p);
  if (note) console.log("  " + dim(note));
}

async function cmdReindex() {
  banner();
  const key = resolveKey();
  // A missing key is a FAILURE, not a no-op: "Need --key" then exit 0 leaves CI green while the memory was
  // never touched, and every later gate then grounds against a graph that quietly stopped moving.
  if (!key) { console.log("  " + amber("Need an Estelle key to reindex — pass ") + bold("--key <KEY>") + amber(", set ") + bold("ESTELLE_API_KEY") + amber(", or run ") + bold("estelle") + amber(" once to save one.")); process.exitCode = 1; return; }
  const root = flag("--path", process.cwd());

  // Explicit paths win; otherwise ask git what actually changed. Never fall back to "everything" — a silent
  // whole-repo reindex would be a slow surprise, and `sweep` is the command that means that.
  const named = positionalPaths(process.argv.slice(3));
  // git reports changed paths relative to the REPO ROOT, so that is the base they must be resolved
  // against — resolving them against the cwd is what made a subdirectory run delete live files.
  // Explicitly named paths are the user's own and stay relative to where they typed them.
  const gitRoot = named.length ? "" : (repoRoot(root) || root);
  const base = named.length ? root : gitRoot;
  const requested = named.length ? named : changedFiles(gitRoot);
  if (!requested.length) {
    console.log("  " + green("Nothing changed.") + dim("  Your memory is already current."));
    console.log("");
    return;
  }

  // Everything the graph is keyed by is repo-relative, so normalise here and REFUSE anything that escapes
  // the root — the same rule the PostToolUse sync hook enforces.
  const changed = [];
  const outside = [];
  for (const p of requested) {
    const rel = repoRelative(base, p);
    if (rel) changed.push(rel); else outside.push(p);
  }
  announceSkipped(`Skipped ${outside.length} path(s) outside this repo — reindex only files under ${base}:`, outside);

  // The extension allowlist applies to NAMED paths too, not just to git's list. Refusing is the right call
  // even though the user typed the path: an ingested binary or .env is invisible once stored and stays there.
  const { kept, skipped } = partitionIngestable(changed);
  announceSkipped(`Skipped ${skipped.length} path(s) a code graph can't hold — source and docs only:`, skipped);

  const collected = [];
  for (const rel of kept) pushFile(collected, base, path.resolve(base, rel));
  const { files, flagged } = partitionSecrets(collected);
  announceSkipped(`Skipped ${flagged.length} file(s) that look like they contain a live secret:`,
                  flagged.map((f) => f.path));

  // Only GIT may declare a deletion. A path that is merely absent from disk proves nothing — a typo is absent
  // too — and `removed` is the field that makes nodes disappear, so it takes the stronger evidence.
  const proven = new Set(deletedFiles(base));
  const missing = kept.filter((rel) => !fs.existsSync(path.resolve(base, rel)));
  const removed = missing.filter((rel) => proven.has(rel));
  const unexplained = missing.filter((rel) => !proven.has(rel));
  announceSkipped(`Skipped ${unexplained.length} path(s) that are not on disk and that git does not report as deleted:`,
                  unexplained, "Check the spelling — a path you typed wrong is not a deletion, so nothing was dropped.");
  // Non-zero only for a path the USER named, where the mistake is theirs to see: the file they meant is
  // silently NOT reindexed, so a green exit hides it. Git's own list can go stale mid-run (a broken symlink,
  // an untracked file removed between the two commands) and that is not the customer's error to fail on.
  if (unexplained.length && named.length) process.exitCode = 1;
  if (!files.length && !removed.length) {
    console.log("  " + amber("Nothing ingestable in the changed set."));
    return;
  }

  const repo = repoNameFor(root);
  const parts = [];
  if (files.length) parts.push(`${files.length} changed`);
  if (removed.length) parts.push(`${removed.length} removed`);
  console.log("  " + dim("Reindexing ") + bold(parts.join(" + ")) + dim(` ${dot} untouched files keep their symbols`));
  if (repo) console.log("  " + dim("Filing under repo ") + bold(repo));

  // The plan is printed above; --dry-run stops here. Before the parser learned which flags take a value this
  // was accidentally safe (the flag ate the path and nothing was collected) — now the path survives, so the
  // stop has to be deliberate. Uploading against an explicit "don't" is not a smaller mistake than a typo.
  if (has("--dry-run")) {
    console.log("  " + dim("--dry-run · nothing was sent."));
    console.log("");
    return;
  }

  const body = reindexBody(files, repo, removed);
  process.stdout.write("  " + dim("Updating your Estelle memory… "));
  const r = await apiPost("/reindex", body, key);
  if (!r.ok) return fail(r);
  console.log(ok);
  console.log("");
  // The green claim is only true of the paths that were actually handled. Saying "Memory current." over a run
  // where a named path did nothing is the same false-completeness the sweep's warnDropped exists to prevent —
  // the customer reads it, believes the file is indexed, and the graph stays stale for exactly that file.
  if (unexplained.length) {
    console.log("  " + amber("Memory updated — but not for every path you named.") + dim("  The skipped paths above were left alone."));
  } else {
    console.log("  " + green("Memory current.") + dim("  Only the changed files moved; the rest of the graph is intact."));
  }
  console.log("");
}

// ── the reindex decisions, pure so they are testable without a repo, a disk or a server ──────────────

// The flags that consume the argument AFTER them. Only these may swallow one: skipping unconditionally made
// a valueless flag eat the path behind it (`reindex --dry-run a.py` reindexed nothing and reported "Nothing
// changed"), and a run that quietly did no work looks exactly like a run that did.
const VALUE_FLAGS = new Set(["--key", "--path", "--repo", "--url", "--base", "--client"]);

/** The positional file paths in `estelle reindex [files…]`, with flags and THEIR VALUES dropped — so
 * `--key K` never becomes a "file" the CLI then reads and uploads. */
function positionalPaths(args) {
  const out = [];
  for (let i = 0; i < args.length; i++) {
    const arg = String(args[i]);
    if (arg.startsWith("-")) { if (VALUE_FLAGS.has(arg)) i++; continue; }   // a flag, and its value if it takes one
    out.push(args[i]);
  }
  return out;
}

/** Split repo-relative paths into what a code graph may hold and what it may not.
 *
 * `changedFromGitOutput` already filters git's own list, but a path the USER names reaches the uploader
 * directly, and typing a path is not a licence to ingest it: a PNG read as UTF-8 poisons the graph and a
 * .env puts the customer's config in memory, both silently and both durably. `SECRET_RE` is no backstop —
 * it matches key SHAPES, so `STRIPE=whatever` is not a secret to it and would travel. */
function partitionIngestable(paths) {
  const list = paths || [];
  return {
    kept: list.filter((p) => EXT.has(path.extname(p))),
    skipped: list.filter((p) => !EXT.has(path.extname(p))),
  };
}

/** One path in the form the graph is keyed by — relative to the repo root — or "" when it escapes.
 *
 * Two things ride on this. An ABSOLUTE or `./`-prefixed path would not match the relative path the sweep
 * stored, so a `removed` entry would silently drop nothing and a changed file would land beside its old
 * copy instead of replacing it. And a path OUTSIDE the root must be refused outright: `reindex
 * ../other-repo/x.py` would otherwise file someone else's code under this repo's namespace — the exact
 * rule the PostToolUse sync hook already enforces (hook.js: `if (rel.startsWith("..")) return 0`). */
function repoRelative(root, p) {
  const base = path.resolve(String(root || ""));
  const rel = path.relative(base, path.resolve(base, String(p || "")));
  if (!rel || rel === ".." || rel.startsWith(".." + path.sep) || path.isAbsolute(rel)) return "";
  return rel;
}

/** The ingestable paths in one `git … --name-only` blob. Extension-filtered here because git reports
 * lockfiles, images and binaries as "changed" too, and none of those belong in a code graph. */
function changedFromGitOutput(stdout) {
  const out = [];
  for (const line of String(stdout || "").split("\n")) {
    const rel = line.trim();
    if (rel && EXT.has(path.extname(rel))) out.push(rel);
  }
  return out;
}

/** The /reindex request body. `files` REPLACE their own nodes, `removed` DROPS paths that are gone, and
 * every path NOT named survives untouched — that last part is the whole difference from /sync, which
 * rebuilds the grounding surface from exactly what it receives. `removed` is omitted when empty rather
 * than sent as [], so an ordinary update can never be read as "delete nothing" bookkeeping. */
function reindexBody(files, repo, removed) {
  const body = syncBody(files, repo);
  if (removed && removed.length) body.removed = removed;
  return body;
}

// What git says changed vs HEAD, plus anything untracked. Empty outside a git repo — there is no honest way
// to know what "changed" means without one, and guessing would either miss edits or resend the world.
/** The repo ROOT for `dir`, or "" when it is not inside a git work tree.
 *
 * Load-bearing: git reports changed paths relative to the repo root, so the root — not the cwd — is the
 * only correct base to resolve them against. */
function repoRoot(dir) {
  try {
    return execFileSync("git", ["-C", dir, "rev-parse", "--show-toplevel"],
                        { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch (_) { return ""; }
}

function changedFiles(root) {
  const out = new Set();
  // Tracked-and-modified, then untracked-but-not-ignored. execFileSync with an argv array (never a shell
  // string) so a path with a space or a quote in it cannot become a second command.
  //
  // `--full-name` on ls-files is NOT cosmetic. Without it these two commands report on DIFFERENT bases:
  // `diff --name-only` is always repo-root-relative, while `ls-files` is relative to the CWD. Run
  // `estelle reindex` from any subdirectory and a modified top-level file resolved against the cwd points
  // at a path that does not exist — so it was classified as DELETED and sent in `removed`, telling the
  // server to drop the nodes of a live file, while the CLI printed "the rest of the graph is intact."
  // That is precisely the graph-gutting that reindex exists to avoid. Both are root-relative now, and
  // the caller resolves them against the ROOT.
  const runs = [
    ["-C", root, "diff", "--name-only", "HEAD"],
    ["-C", root, "ls-files", "--others", "--exclude-standard", "--full-name"],
  ];
  for (const argv of runs) {
    try {
      const stdout = execFileSync("git", argv, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
      for (const rel of changedFromGitOutput(stdout)) out.add(rel);
    } catch (_) { /* not a git repo, or no HEAD yet — reindex then needs explicit paths */ }
  }
  return [...out];
}

/** The paths git reports as DELETED vs HEAD, relative to `base` — the ONLY evidence that may become a
 * `removed` entry.
 *
 * "Not on disk" is a different fact and a much weaker one: a mistyped path is not on disk either, so reading
 * absence as deletion turned `estelle reindex src/atuh.py` into an unconfirmed instruction to drop
 * `src/atuh.py`'s nodes. Empty outside a git repo, which is the honest answer — with no git there is no
 * record of a deletion, and inventing one drops symbols for code that is still there.
 *
 * `--relative` is what makes the answer COMPARABLE. Plain `diff --name-only` is always repo-root-relative,
 * and the caller's paths are relative to wherever the user typed them; re-basing one onto the other by hand
 * breaks the moment a path crosses a symlink (every macOS temp dir does), and a deletion that fails to match
 * silently degrades into "you mistyped that". Let git answer in the caller's own base instead. */
function deletedFiles(base) {
  try {
    const stdout = execFileSync("git", ["-C", base, "diff", "--relative", "--name-only", "--diff-filter=D", "HEAD"],
                                { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    return changedFromGitOutput(stdout);
  } catch (_) { return []; }   // not a git repo, or no HEAD yet — then nothing is provably deleted
}

const SKIP_DIRS = new Set([".git", "node_modules", "dist", "build", "__pycache__", ".venv", "venv",
  "vendor", ".next", "target", ".cache", "coverage", ".mypy_cache", ".pytest_cache"]);
const EXT = new Set([".py", ".js", ".ts", ".tsx", ".jsx", ".go", ".rs", ".java", ".rb", ".php", ".c",
  ".cpp", ".h", ".hpp", ".cs", ".swift", ".kt", ".scala", ".md"]);

function pushFile(out, base, full) {
  try {
    const st = fs.lstatSync(full); // lstat: a symlink is NEVER followed — it can point outside the repo
    if (!st.isFile() || st.size > 400_000) return;
    out.push({ path: path.relative(base, full), content: fs.readFileSync(full, "utf8") });
  } catch (_) { /* skip unreadable */ }
}

// In a git repo, sweep exactly what git sees (`--exclude-standard` honors .gitignore) — gitignored
// working files are where secrets and scratch notes live, and the consent was "ingest the REPO".
function gitFileList(root) {
  try {
    const out = execFileSync("git", ["-C", root, "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    return out.split("\0").filter(Boolean);
  } catch (_) { return null; } // not a git repo (or no git binary) → the plain walker
}

// The name this repo is FILED under in your memory. Without one, every sweep piles into a single unfiled
// namespace: `/repos` reports none, "which repos do you have?" is genuinely unanswerable, and recall blends
// every codebase you ever swept into one answer — the exact contamination per-repo scoping exists to stop.
// The name is right there in git, so take it from the origin remote (owner/name, the form the GitHub-App
// sweep and the dashboard already use) and fall back to the directory name outside git or with no remote.
// The derivation itself now lives in `repo-name.js` so the always-on HOOK writes to the same namespace this
// sweep does — it did not, and the gate read a surface the hook had never updated (see that file's header).
// `--repo` stays here: argv is the entry point's business, exactly as with `auth.js`/`resolveKey`.
function repoNameFor(root) {
  return flag("--repo", "") || repoName.repoNameFor(root);
}

function walkFiles(root, base, out) {
  let entries = [];
  try { entries = fs.readdirSync(root, { withFileTypes: true }); } catch (_) { return out; }
  for (const e of entries) {
    if (out.length >= 4000) break;
    if (e.isSymbolicLink()) continue; // never follow a link out of the repo
    const full = path.join(root, e.name);
    if (e.isDirectory()) { if (!SKIP_DIRS.has(e.name) && !e.name.startsWith(".")) walkFiles(full, base, out); continue; }
    if (!EXT.has(path.extname(e.name))) continue;
    pushFile(out, base, full);
  }
  return out;
}

function collectFiles(root) {
  const listed = gitFileList(root);
  if (listed === null) return walkFiles(root, root, []);
  const out = [];
  for (const rel of listed) {
    if (out.length >= 4000) break;
    if (!EXT.has(path.extname(rel))) continue;
    const dirs = rel.split("/").slice(0, -1);
    if (dirs.some((d) => SKIP_DIRS.has(d) || d.startsWith("."))) continue;
    pushFile(out, root, path.join(root, rel));
  }
  return out;
}

function cmdConnect() {
  banner();
  const client = process.argv[3] || "cursor";
  // Deliberately NOT resolveKey(). This command's whole output is a config snippet printed to the
  // terminal, so resolving the stored key here would splash a saved bearer credential across the screen
  // (and any transcript or screen-share) as a side effect of asking how to connect. A placeholder is the
  // right answer; an explicit --key or $ESTELLE_API_KEY is the caller saying "yes, print it".
  const key = flag("--key", process.env.ESTELLE_API_KEY || "<YOUR_ESTELLE_KEY>");
  console.log("  " + bold(`Connect ${PRETTY[client] || client} to Estelle`));
  console.log("");
  if (NOTE_CLIENTS[client]) { console.log("  " + arrow + " " + NOTE_CLIENTS[client]()); console.log(""); return; }
  console.log("  " + dim("One line (writes the config for you, with a backup):"));
  console.log("    " + teal(`npx @fatelabs/estelle init --client ${client} --key <YOUR_ESTELLE_KEY>`));
  console.log("");
  console.log("  " + dim("Community installer (version-pinned on purpose):"));
  console.log("    " + dim(installMcp(client)));
  console.log("");
  console.log("  " + dim("Claude Code:"));
  console.log("    " + dim(exportHint));
  console.log("    " + teal(claudeAdd()));
  console.log("");
  if (configPath(client)) {
    console.log("  " + dim(`Or paste into ${configPath(client)} :`));
    const blob = client === "vscode" ? mergeVsCode(null, key) : mergeConfig(null, key);
    console.log(JSON.stringify(blob, null, 2).split("\n").map((l) => "    " + grey(l)).join("\n"));
    console.log("");
  }
}

// Remove Estelle's entry from a config file's `topKey` map — .bak first, everything else untouched.
function removeEntry(p, topKey) {
  let existing = null;
  try { existing = JSON.parse(fs.readFileSync(p, "utf8")); } catch (_) { return false; }
  if (!existing || typeof existing !== "object") return false;
  const servers = existing[topKey];
  if (!servers || typeof servers !== "object" || !("estelle" in servers)) return false;
  const rest = { ...servers };
  delete rest.estelle;
  fs.copyFileSync(p, p + ".bak");
  fs.writeFileSync(p, JSON.stringify({ ...existing, [topKey]: rest }, null, 2) + "\n");
  return true;
}

// The off switch init promises: walk every config init can write and delete the estelle entry. Offline.
function cmdRemove() {
  banner();
  console.log("  " + bold("Disconnect Estelle from your editors") + dim("  (local config edits only — nothing leaves this machine)"));
  console.log("");
  let removed = 0;
  for (const client of Object.keys(CLIENT_FILES)) {
    const p = configPath(client);
    if (p && fs.existsSync(p) && removeEntry(p, "mcpServers")) {
      removed += 1;
      console.log("  " + ok + " " + bold(PRETTY[client] || client) + dim("  estelle removed from " + p + "  (.bak saved)"));
    }
  }
  const vs = configPath("vscode");
  if (fs.existsSync(vs) && removeEntry(vs, "servers")) {
    removed += 1;
    console.log("  " + ok + " " + bold(PRETTY.vscode) + dim("  estelle removed from " + vs + "  (.bak saved)"));
  }
  if (!removed) console.log("  " + dim("No Estelle MCP entry found in any known editor config — nothing to remove."));
  console.log("");
  console.log("  " + dim("Claude Code: ") + teal("claude mcp remove estelle"));
  console.log("  " + dim("Claude Desktop (if added via Connectors): Settings → Connectors → remove Estelle."));
  console.log("  " + dim("Restart your editors to drop the live connection."));
  console.log("");
}

// ── hosted-API verbs (ask / recall / verify / gate) — any terminal becomes a full Estelle client ─────
function needKey() {
  const key = resolveKey();
  if (!key) {
    console.log("  " + amber("Need an Estelle key — pass ") + bold("--key <KEY>") + amber(", set ") + bold("ESTELLE_API_KEY") + amber(", or run ") + bold("estelle") + amber(" once to save one."));
    // A RED pipeline, never a silent green. This mattered most for `gate` and `verify`, the CI commands:
    // exiting 0 with no key meant no key -> the gate never ran -> the build passed, which is a fail-OPEN
    // and the exact opposite of the rule `failClosed` states a few lines below ("an error is NEVER a
    // pass … all BLOCK"). A missing credential is the same class as a revoked one.
    process.exitCode = 1;
  }
  return key;
}
// Everything after the command, joined, up to the first --flag — so `estelle ask how does auth work` works unquoted.
function argText(from) {
  const parts = [];
  for (let i = from; i < process.argv.length; i++) {
    if (process.argv[i].startsWith("--")) break;
    parts.push(process.argv[i]);
  }
  return parts.join(" ").trim();
}

async function apiPost(pathname, body, key) {
  try {
    const res = await fetch(`${API}${pathname}`, {
      method: "POST",
      headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(API_TIMEOUT_MS),
    });
    const text = await res.text();
    let json = null; try { json = JSON.parse(text); } catch (_) { /* non-JSON body */ }
    return { ok: res.ok, status: res.status, json, text };
  } catch (e) {
    return { ok: false, status: 0, json: null, text: String((e && e.message) || e) };
  }
}
// PUT is a real verb here, not a stylistic choice: the vendor watchlist is ONE object the caller replaces,
// and the server routes GET and PUT on the same path to tell "read it" from "this is now the whole thing".
async function apiPut(pathname, body, key) {
  try {
    const res = await fetch(`${API}${pathname}`, {
      method: "PUT",
      headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(API_TIMEOUT_MS),
    });
    const text = await res.text();
    let json = null; try { json = JSON.parse(text); } catch (_) { /* non-JSON body */ }
    return { ok: res.ok, status: res.status, json, text };
  } catch (e) {
    return { ok: false, status: 0, json: null, text: String((e && e.message) || e) };
  }
}
async function apiGet(pathname, key) {
  try {
    const res = await fetch(`${API}${pathname}`,
      { headers: { Authorization: `Bearer ${key}` }, signal: AbortSignal.timeout(API_TIMEOUT_MS) });
    const text = await res.text();
    let json = null; try { json = JSON.parse(text); } catch (_) { /* non-JSON body */ }
    return { ok: res.ok, status: res.status, json, text };
  } catch (e) {
    return { ok: false, status: 0, json: null, text: String((e && e.message) || e) };
  }
}
// An API failure always exits non-zero — a red pipeline, never a silent green.
// The server's error envelope is {"error": {"message", "code"}} — so the error field is an OBJECT, and
// stringifying it printed a literal "[object Object]" instead of what went wrong. An error you cannot read
// is its own bug: it sent me hunting a 500 by hand that the response had already described.
function errText(r) {
  const e = r && r.json && (r.json.error !== undefined ? r.json.error : r.json.message);
  if (e && typeof e === "object") return String(e.message || e.detail || JSON.stringify(e));
  if (e) return String(e);
  return String((r && r.text) || "");
}

function fail(r) {
  console.log(r.status ? amber(`HTTP ${r.status}`) : amber("connection failed"));
  const msg = errText(r);
  if (msg) console.log("  " + dim(msg.slice(0, 500)));
  process.exitCode = 1;
}
// The gate/verify contract mirrors the server's: an error is NEVER a pass. A revoked key, an unpaid
// plan, a 5xx, or an unreachable server all BLOCK — same as api_dev's error→merge=False discipline.
function failClosed(r, what) {
  fail(r);
  console.log("  " + red(`✗ ${what} could not verify this change — BLOCKED (fail-closed).`));
}
const asArray = (x) => (Array.isArray(x) ? x : []);
const indent = (t) => String(t).split("\n").map((l) => "  " + l).join("\n");
const oneLine = (t) => String(t || "").replace(/\s+/g, " ").trim().slice(0, 70);

async function cmdAsk() {
  banner();
  const key = needKey(); if (!key) return;
  const q = argText(3);
  if (!q) { console.log("  " + amber('Ask what?') + dim('  e.g. estelle ask "explain the retry logic"')); return; }
  process.stdout.write("  " + dim("Asking Estelle… "));
  const r = await apiPost("/v1/chat/completions", { model: "estelle", messages: [{ role: "user", content: q }] }, key);
  if (!r.ok) return fail(r);
  console.log(ok); console.log("");
  const content = r.json && r.json.choices && r.json.choices[0] && r.json.choices[0].message && r.json.choices[0].message.content;
  console.log(content ? indent(content) : "  " + dim("(no answer)"));
  console.log("");
}

async function cmdRecall() {
  banner();
  const key = needKey(); if (!key) return;
  const q = argText(3);
  if (!q) { console.log("  " + amber('Recall what?') + dim('  e.g. estelle recall "how is billing metered?"')); return; }
  process.stdout.write("  " + dim("Searching your brain… "));
  // --repo scopes the search to one of your swept repos (INV-15). Omitted, Estelle works out which repo
  // the question is about — and asks when it genuinely can't tell, rather than answering across them.
  const repo = flag("--repo", "");
  const r = await apiPost("/search", repo ? { query: q, repo } : { query: q }, key);
  if (!r.ok) return fail(r);
  console.log(ok); console.log("");
  const v = r.json || {};
  if (v.scope_ask) {
    // Multi-repo account, nothing settled WHICH repo (INV-15). Estelle asks rather than blending two
    // codebases — rendering this as "(no memory recall)" would be a lie: the answer exists.
    console.log(indent(String(v.question || "Which repo should I answer about?")));
    console.log(""); console.log("  " + dim('Re-run with the repo, e.g. estelle recall "…" --repo owner/name'));
    console.log("");
    return;
  }
  console.log(v.recall ? indent(v.recall) : "  " + dim("(no memory recall)"));
  const code = asArray(v.code);
  if (code.length) {
    console.log(""); console.log("  " + bold("Code"));
    for (const m of code.slice(0, 8)) console.log("  " + dot + " " + teal(`${m.source_file}:${m.line}`) + dim("  " + oneLine(m.text)));
  }
  console.log("");
}

// ── github: link an identity, bind an installation, sweep the repos you pick ────────────────────────
// The terminal half of GitHub connect. It is NOT a weaker door than the dashboard: the server demands the
// same two proofs from both — an OAuth code (control of the GitHub installation) AND an authenticated
// caller (control of the Estelle account). The CLI already holds the second as its API key, and the first
// arrives on a loopback listener, so a link forwarded to someone else delivers its code to THEIR machine.
async function cmdGithub() {
  banner();
  const key = needKey(); if (!key) return;
  const sub = (process.argv[3] || "").trim();
  if (sub === "link") return cmdGithubLink(key);
  if (sub === "connect") return cmdGithubConnect(key, process.argv[4]);
  if (sub === "repos") return cmdGithubRepos(key);
  if (sub === "sweep") return cmdGithubSweep(key);
  const id = await apiGet("/github/identity", key);
  if (!id.ok) return fail(id);
  const linked = (id.json || {}).linked;
  const installs = linked ? await apiGet("/github/identity/installations", key) : null;
  // Whether anything is CONNECTED is the server's answer, not this identity's installation list: on a team
  // the binding belongs to the team, so a teammate's Connect is what makes this account connected. Without
  // this read the status always said "Run: estelle github connect", even right after a successful one.
  const repos = linked ? await apiGet("/github/repos", key) : null;
  console.log("");
  for (const line of gh.statusLines(id.json, installs && installs.json && installs.json.installations,
                                    repos && repos.ok ? repos.json : null)) {
    console.log("  " + (line.startsWith("Run:") ? dim(line) : line));
  }
  console.log("");
}

async function cmdGithubLink(key) {
  const start = await apiGet(
    `/github/identity/authorize-url?redirect_uri=${encodeURIComponent(gh.redirectUri())}`, key);
  if (!start.ok) return fail(start);
  const url = (start.json || {}).authorize_url;
  console.log("  " + bold("Authorize Estelle on GitHub") + dim("  (opens in your browser)"));
  console.log("  " + teal(url));
  console.log("  " + dim("Waiting for the redirect on ") + grey(gh.redirectUri()) + dim(" …"));
  const waiting = gh.awaitCallback({});
  openUrl(url);
  let back;
  try {
    back = await waiting;
  } catch (e) {
    console.log("  " + red(String((e && e.message) || e)));
    process.exitCode = 1;
    return;
  }
  const linked = await apiPost("/github/identity/link", { code: back.code, state: back.state }, key);
  if (!linked.ok) return fail(linked);
  const login = (linked.json || {}).login;
  console.log("  " + ok + " " + bold("GitHub identity linked") + (login ? dim(`  (${login})`) : ""));
  console.log("  " + dim("Next: ") + teal("estelle github connect"));
  console.log("");
}

async function cmdGithubConnect(key, want) {
  const listed = await apiGet("/github/identity/installations", key);
  if (!listed.ok) return fail(listed);
  const choice = gh.pickInstallation((listed.json || {}).installations, want);
  if (choice.none) {
    console.log("  " + amber("That identity can't see any Estelle App installation."));
    console.log("  " + dim("Install the Estelle GitHub App on the org or user that owns the repos, then retry."));
    process.exitCode = 1;
    return;
  }
  if (!choice.chosen) {
    // Never pick the first: binding the wrong org sweeps the wrong private code into this namespace.
    if (choice.unknown) console.log("  " + amber(`No installation matches "${choice.unknown}".`));
    console.log("  " + bold("Which installation?") + dim("  estelle github connect <id|owner>"));
    for (const row of choice.needs) console.log("  " + dot + " " + teal(String(row.id)) + dim("  " + (row.account || "")));
    process.exitCode = 1;
    return;
  }
  // sweep:false — connect FIRST, choose SECOND. Binding must never ingest an org's every repo before
  // anyone asked whether that is wanted or whether it fits the plan.
  const bound = await apiPost("/github/app/setup",
                              { installation_id: choice.chosen.id, sweep: false }, key);
  if (!bound.ok) return fail(bound);
  console.log("  " + ok + " " + bold("Connected") + dim(`  installation ${choice.chosen.id} `
    + (choice.chosen.account ? `(${choice.chosen.account})` : "")));
  console.log("  " + dim("Nothing was ingested yet. Next: ") + teal("estelle github repos"));
  console.log("");
}

async function cmdGithubRepos(key) {
  const r = await apiGet("/github/repos", key);
  if (!r.ok) return fail(r);
  const v = r.json || {};
  if (!v.connected) {
    console.log("  " + amber("No GitHub App installation is connected to this account."));
    console.log("  " + dim("Run: ") + teal("estelle github connect"));
    return;
  }
  console.log("  " + bold("Granted repos") + dim("  (token cost is an ESTIMATE from GitHub's size, not a measurement)"));
  for (const repo of v.repos || []) {
    console.log("  " + dot + " " + teal(repo.full_name) + dim(`  ~${mtok(repo.estimated_tokens)}`)
      + (repo.swept ? green("  swept") : ""));
  }
  console.log("");
  console.log("  " + dim("Ingest what you pick: ") + teal("estelle github sweep owner/name [owner/other]"));
  console.log("");
}

async function cmdGithubSweep(key) {
  const repos = argText(4).split(/[\s,]+/).filter(Boolean);
  if (!repos.length) {
    console.log("  " + amber("Which repos?") + dim("  estelle github sweep owner/name [owner/other]"));
    process.exitCode = 1;
    return;
  }
  process.stdout.write("  " + dim("Queueing… "));
  const r = await apiPost("/github/sweep", { repos }, key);
  if (failedOnSize(r)) { console.log(""); return showTooBig(r.json.estimate); }
  if (!r.ok) return fail(r);
  console.log(ok);
  for (const job of (r.json || {}).queued || []) console.log("  " + dot + " " + teal(job.repo) + dim("  " + job.job));
  console.log("");
}

// ── monitor + research: the two suites that shipped with no first-party door at all ─────────────────
// Estelle Monitor has fifteen working, entitlement-gated routes on production and no dashboard page (that
// lives in the web worktree). The Vendor Drift Monitor has a durable watchlist, a measured injection guard
// and a scheduler that ticks. Between them, the only door either had was a hand-written curl.
//
// The COMMANDS live in their own modules beside the renderers they use; this file stays a dispatcher. What
// it owns is the CTX — the HTTP verbs, the colours, the argv accessors and the output sink — so a command
// module never reaches for a global and can be exercised without one.
const suiteCtx = () => ({
  argv: process.argv,
  banner, needKey, flag, has, argText, announceSkipped, errText, asArray, indent, oneLine,
  get: apiGet, post: apiPost, put: apiPut,
  out: (line) => console.log(line),
  write: (text) => process.stdout.write(text),
  // A command that could not do what it was asked exits RED. Named rather than assigning process.exitCode
  // inside a module, so "this run failed" stays one decision in one place.
  markFailed: () => { process.exitCode = 1; },
  failWith: fail,
  // `null` for a file that is not there — distinguishable from an empty file, which is what lets the repair
  // command say "that call site is not on this machine" instead of sending an empty string as its contents.
  readFile: (rel) => { try { return fs.readFileSync(path.resolve(process.cwd(), rel), "utf8"); } catch { return null; } },
  now: () => Date.now(),
  c: { teal, dim, bold, green, amber, grey, red, ok, arrow, dot },
});

const cmdMonitor = () => require("./monitor.js").run(suiteCtx());
const cmdResearch = () => require("./research.js").run(suiteCtx());
// Erasure and its PROOF. /purge and /forget have written an append-only receipt since the cascade landed
// and nothing could read one back — a deletion nobody can verify is a deletion nobody will believe.
const cmdMemory = () => require("./memory.js").run(suiteCtx());

async function cmdVerify() {
  banner();
  const key = needKey(); if (!key) return;
  const target = process.argv[3];
  if (!target || target.startsWith("--")) { console.log("  " + amber("Verify what?") + dim("  pass a file: estelle verify path/to/file.py")); return; }
  let code;
  try { code = fs.readFileSync(target, "utf8"); } catch (e) { console.log("  " + amber(`Can't read ${target}: ${String((e && e.message) || e)}`)); return; }
  process.stdout.write("  " + dim(`Grounding ${target} against your repo… `));
  const r = await apiPost("/verify", { answer: code }, key);
  if (!r.ok) return failClosed(r, "verify");
  console.log(ok); console.log("");
  const v = r.json || {};
  const ungrounded = asArray(v.ungrounded);
  if (v.grounded) {
    console.log("  " + green("Grounded.") + dim("  Every API this file references exists in your swept repo."));
  } else {
    console.log("  " + amber("Ungrounded references (not defined in your repo):"));
    for (const u of ungrounded) console.log("    " + red("✗") + " " + (typeof u === "string" ? u : (u.name || JSON.stringify(u))));
    process.exitCode = 1;
  }
  console.log("");
}

// The unified diff to gate: staged changes by default, or `--base <ref>` to diff the branch against a ref.
// Returns null when git itself fails (bad ref, shallow clone, not a repo) — the caller must BLOCK, since
// "git broke" is indistinguishable from "diff never examined", which is not a pass.
function gitDiff(base) {
  try {
    const args = base ? ["diff", "--no-color", `${base}...HEAD`] : ["diff", "--cached", "--no-color"];
    return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  } catch (_) { return null; }
}
async function cmdGate() {
  banner();
  const key = needKey(); if (!key) return;
  const base = flag("--base");
  const diff = gitDiff(base);
  if (diff === null) {
    console.log("  " + red("✗ could not compute the diff (git failed" + (base ? `; is ${base} a valid ref?` : "") + ") — BLOCKED (fail-closed)."));
    process.exitCode = 1;
    return;
  }
  if (!diff.trim()) {
    console.log("  " + amber(base ? `No diff vs ${base}.` : "No staged changes to gate.") + dim("  Stage some changes, or pass --base <ref>."));
    return;
  }
  process.stdout.write("  " + dim("Running the merge gate… "));
  const r = await apiPost("/gate", { diff }, key);
  if (!r.ok) return failClosed(r, "the merge gate");
  console.log(ok); console.log("");
  const v = r.json || {};
  // THREE states, three colours. `verified === false` means the gate could not READ part of the change
  // (an unparseable hunk, an unswept repo, a timeout) — it is not a certification, so it must never render
  // in the same green as a verdict that actually checked the diff. Green used to cover both, because the
  // CLI keyed on `merge` alone, exactly like the CI scripts this gate exists to answer.
  const couldNotVerify = v.verified === false;
  const verdictText = v.verdict || (v.merge ? "clean — safe to merge" : "blocked");
  console.log("  " + bold("Verdict  ") +
    (couldNotVerify ? amber(verdictText) : v.merge ? green(verdictText) : amber(verdictText)));
  if (couldNotVerify && v.merge) {
    console.log("  " + amber("!") + " " + dim("the gate could not check part of this change — it is UNVERIFIED there, not verified-clean."));
  }
  for (const b of asArray(v.blockers)) console.log("  " + red("✗") + " " + (b.body || `${b.path}:${b.line}`));
  for (const w of asArray(v.warnings)) console.log("  " + amber("!") + " " + (w.body || `${w.path}:${w.line}`));
  console.log("");
  if (!v.merge) process.exitCode = 1;
}

function help() {
  banner();
  // Padded to the WIDEST command, not to a number someone once eyeballed: `github [link|connect|repos|sweep]`
  // already overflowed 26 and ran its description into the command, and every new subcommand group made it
  // worse. A help screen that runs two columns together is a help screen people stop reading.
  const row = (c, d) => console.log("  " + teal(c.padEnd(38)) + dim(d));
  console.log("  " + bold("Usage") + dim("  npx @fatelabs/estelle <command>"));
  console.log("");
  row("init [--key K]", "auto-detect your editors and write the MCP config (then verify it answers)");
  row("sweep [--key K]", "ingest the current repo (git-visible files) into your Estelle memory");
  row("reindex [files…]", "update only what changed (fast) — keeps the rest of the graph intact");
  row("connect <client>", "show the one-liner / config for one client");
  row("remove", "disconnect: delete Estelle from every editor config (alias: disconnect, off; offline)");
  // GitHub connect SHIPPED web-only: a terminal-only user had no way to give Estelle their code at all.
  row("github [link|connect|repos|sweep]", "connect your GitHub repos from the terminal (browser once)");
  // Monitor and Research shipped with FIFTEEN and FOUR working routes respectively and no first-party door
  // between them — no dashboard page, no CLI, only curl. These two rows are that door.
  row("monitor [issues|issue|alerts|uptime|logs]", "your production errors, alerts and uptime (Pro)");
  row("research [watch|drift|repair|ask]", "watch your vendors' APIs for drift, and ground a fix on their live docs");
  // The erasure PROOF path. /purge and /forget wrote receipts from the day they cascaded and nothing could
  // read one back — a deletion nobody can verify is a deletion nobody will believe.
  row("memory [receipts|retract|forget]", "erase what Estelle holds — and read the receipt that proves it");
  row("ask <question>", "ask Estelle about your codebase (grounded, on your plan)");
  row("recall <query> [--repo R]", "search your Estelle memory + code (--repo scopes a multi-repo account)");
  row("verify <file>", "check a file for hallucinated APIs against your repo");
  row("gate [--base ref]", "run the merge gate on your staged diff (or vs a base ref)");
  // These SHIPPED undocumented. A real command nobody can discover is the same as a missing one — and
  // these two are the always-on switch, which is the whole point of the hooks.
  row("install-hooks", "fire Estelle on every edit + command in Claude Code (no opt-in needed)");
  row("uninstall-hooks", "stop that — your own hooks and the MCP server are untouched");
  console.log("");
  console.log("  " + dim("Bare ") + bold("estelle") + dim(" opens the session: ask anything, /help for commands, ")
    + bold("!cmd") + dim(" for a shell command."));
  console.log("  " + dim("Flags: ") + grey("--key K  --client cursor  --dry-run  --url <mcp>  --path <dir>  --base <ref>"));
  console.log("  " + dim("Clients: ") + grey(Object.keys(PRETTY).join(", ")));
  // The egress note is a promise, so it has to name the size pre-flight too: `sweep` now sends a
  // paths-and-byte-sizes list before it sends anything else. It carries no content, but it is still egress.
  console.log("  " + dim("Egress: ") + grey(`init/remove/connect stay local (init pings ${API} once to verify); sweep sends a`));
  console.log("  " + dim("        ") + grey("paths+sizes list first (to check the repo fits your plan), then the repo files;"));
  console.log("  " + dim("        ") + grey(`gate your staged diff, verify one file, ask/recall your question — all to ${API}.`));
  console.log("");
}

/** `estelle hook <mode>` — the runtime a customer's Claude Code calls on every edit/command (via init). */
async function cmdHook() {
  const mode = process.argv[3] || "ground";
  const hook = require("./hook.js");
  let payload = {};
  try { payload = JSON.parse(fs.readFileSync(0, "utf8")); } catch { /* a bad payload must never break the action */ }
  const key = resolveKey();
  await hook.runHook(mode, payload, {
    post: async (p, body) => unwrap(await apiPost(p, body, key)),
    out: (o) => console.log(JSON.stringify(o)),
  });
}

/** `estelle install-hooks` — write Estelle's hooks into Claude Code's settings so it fires with no opt-in. */
function cmdInstallHooks() {
  const hook = require("./hook.js");
  const file = hook.claudeSettingsPath();
  // A MISSING file and an UNPARSEABLE one are not the same thing, and treating them the same destroyed
  // customers' settings: one trailing comma in ~/.claude/settings.json parsed as a throw, `existing`
  // stayed {}, and the merge rewrote the file as Estelle-hooks-only — their permissions, model and env
  // gone, with no backup. `init` twelve hundred lines up has always done this correctly (writeClient:
  // copyFileSync to .bak before writing). Missing -> start empty. Present-but-unparseable -> ABORT and
  // say why; the customer's file is the only copy of their configuration and we do not get to guess.
  let existing = {};
  const present = fs.existsSync(file);
  if (present) {
    const raw = fs.readFileSync(file, "utf8");
    try {
      existing = JSON.parse(raw);
    } catch (err) {
      console.log("");
      console.log("  " + red("✗") + " " + bold("Refusing to write — your Claude Code settings could not be read"));
      console.log("    " + dim(file));
      console.log("    " + dim(String(err && err.message ? err.message : err)));
      console.log("");
      console.log("    " + dim("Estelle will not overwrite a file it cannot parse — that would discard your"));
      console.log("    " + dim("permissions, model and env. Fix the JSON (a trailing comma is the usual cause)"));
      console.log("    " + dim("and run this again. Nothing has been changed."));
      console.log("");
      process.exitCode = 1;
      return;
    }
    if (existing === null || typeof existing !== "object" || Array.isArray(existing)) {
      console.log("");
      console.log("  " + red("✗") + " " + bold("Refusing to write — settings is not a JSON object"));
      console.log("    " + dim(file) + dim("  (found " + (Array.isArray(existing) ? "an array" : typeof existing) + ")"));
      console.log("    " + dim("Nothing has been changed."));
      console.log("");
      process.exitCode = 1;
      return;
    }
  }
  // PINNED TO A MAJOR (ADR 0015). Two things at once: an unpinned `npx -y @fatelabs/estelle` re-resolves
  // the package on EVERY edit — the dominant cost on the hooks path, far above Node startup — and it lets
  // any future publish reach an existing customer's machine unreviewed (PLAN-06 supply-chain item #10).
  // `@0` still takes patch and minor fixes automatically; it will not take a breaking major silently.
  const merged = hook.mergeHooks(existing, "npx -y @fatelabs/estelle@0");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  if (present) fs.copyFileSync(file, file + ".bak");  // back up regardless — same rule init already follows
  fs.writeFileSync(file, JSON.stringify(merged, null, 2) + "\n");
  console.log("");
  console.log("  " + green("✓") + " Estelle now fires on every edit and command in Claude Code");
  console.log("    " + dim("ground code before it lands · guard risky bash · keep memory current"));
  console.log("    " + dim("written to ") + grey(file));
  console.log("    " + dim("open /hooks in Claude Code (or restart) to activate."));
  console.log("");
}

/** `estelle uninstall-hooks` — remove ONLY Estelle's hooks; the user's own hooks and the MCP server stay. */
function cmdUninstallHooks() {
  const hook = require("./hook.js");
  const file = hook.claudeSettingsPath();
  let existing;
  try { existing = JSON.parse(fs.readFileSync(file, "utf8")); } catch { existing = null; }
  if (!existing) { console.log("\n  " + dim("No Claude Code settings found — nothing to remove.") + "\n"); return; }
  fs.writeFileSync(file, JSON.stringify(hook.removeHooks(existing), null, 2) + "\n");
  console.log("");
  console.log("  " + green("✓") + " Estelle's hooks removed — it no longer fires automatically");
  console.log("    " + dim("your own hooks are untouched · the MCP server (tools) is a separate switch"));
  console.log("    " + dim("re-enable any time with ") + teal("npx @fatelabs/estelle install-hooks"));
  console.log("");
}

/** Bare `estelle` opens the SESSION: type the name once, then talk. Commands still work for CI/scripts. */
async function cmdSession() {
  const repl = require("./repl.js");
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout,
                                        historySize: 200, terminal: process.stdin.isTTY });
  // Lines are QUEUED rather than pulled one-at-a-time with rl.question(). Two reasons: piped stdin
  // (`printf '…' | estelle`, and every scripted test) delivers every line and then closes immediately —
  // question() would race the close and drop the input — and a multi-line PASTE arrives as several
  // lines at once, which a queue handles and a single question() does not. Close only ends the session
  // once the queue has drained, so nothing typed is ever thrown away.
  const queue = [], waiting = [];
  let closed = false;
  rl.on("line", (line) => (waiting.length ? waiting.shift()(line) : queue.push(line)));
  rl.on("close", () => { closed = true; while (waiting.length) waiting.shift()(null); });

  // Ctrl+C is reflexive — people press it to abandon a half-typed thought, not to quit. Clear the line
  // when there is one; only leave on an empty prompt.
  rl.on("SIGINT", () => {
    if (repl.interruptAction(rl.line) === "exit") { rl.close(); return; }
    rl.write(null, { ctrl: true, name: "u" });                 // wipe the line, keep the session
    process.stdout.write("\n");
    rl.prompt();
  });

  // Prompt history persists across sessions (JSONL, self-healing on a torn file) and seeds readline's
  // own ↑/↓ buffer, so a restart doesn't cost you what you already asked.
  const histFile = path.join(os.homedir(), ".estelle", "history.jsonl");
  let history = [];
  try { history = repl.parseHistory(fs.readFileSync(histFile, "utf8")); } catch { /* first run */ }
  if (rl.history) rl.history = history.slice().reverse();       // readline wants newest-first
  const remember = (text) => {
    const line = repl.historyLine(text, history[history.length - 1]);
    if (!line) return;
    history.push(String(text).trim());
    try {
      fs.mkdirSync(path.dirname(histFile), { recursive: true, mode: 0o700 });
      fs.appendFileSync(histFile, line, { mode: 0o600 });
    } catch { /* history is a convenience — never fail a turn over it */ }
  };
  const prompt = async (q) => {
    let line;
    if (queue.length) line = queue.shift();
    else if (closed) return null;
    else {
      if (process.stdin.isTTY) process.stdout.write(q);
      line = await new Promise((res) => waiting.push(res));
    }
    if (line !== null && String(line).trim()) remember(line);
    return line;
  };
  // The tab says what is running in it, and hands itself back on exit — an app that leaves the customer's
  // terminal renamed after it quits has kept something that was not its. See terminal-title.js.
  require("./terminal-title.js").claimTitle();
  // The first-run key prompt reads through a MASKED reader, so a pasted credential never reaches the
  // terminal, scrollback or a screenshot. The session's own readline is paused for the duration: two
  // readers on one stdin race, and the loser silently eats the line. Only bound on a TTY — a pipe echoes
  // nothing by construction and the queued reader below already handles multi-line pastes correctly.
  const promptSecret = process.stdin.isTTY
    ? async (q) => { rl.pause(); try { return await askSecret(q); } finally { rl.resume(); } }
    : null;
  const code = await repl.runSession({
    version: require("../package.json").version,
    key: resolveKey(),
    promptSecret,
    cwd: path.basename(process.cwd()),
    // `/status` answers "what am I actually pointed at?", so the endpoint and the repo NAME are measured
    // here rather than re-derived in the session — `repoNameFor` is the same function the sweep files
    // under, so what /status calls the repo is exactly what your memory calls it.
    api: API,
    repo: repoNameFor(process.cwd()),
    // Where an applied diff may write, and where the containment check is measured from. The git TOP-LEVEL,
    // not cwd: `git apply` resolves a patch's paths against the directory it runs in, so applying from a
    // subdirectory without this writes to the wrong place.
    cwdPath: process.cwd(),
    root: require("./apply.js").repoRoot(process.cwd()),
    // shift+tab cycles the ceiling. On a non-TTY (piped stdin, CI) this binds nothing at all — there is no
    // human to press it, and raw-mode key handling there would corrupt the output stream.
    bindKeys: require("./mode-ui.js").keyBinder(process.stdin, { rl }),
    // THE SLASH MENU (brief §2.1). Bound here because this file owns the readline; slash-menu.js owns the
    // keyboard and the ordering, the same split mode-ui.js uses. Inert on a non-TTY — a piped session gets
    // no keypress events and drawing a menu there would corrupt the stream.
    bindMenu: require("./slash-menu.js").attachMenu(process.stdin, { rl }),
    // One registry call a day, 2s timeout, silent when offline (update-check.js). Bound here so the REPL
    // stays a router and the test suite can inject a fake without touching the network.
    updateNotice: () => require("./update-check.js")
      .checkForUpdate(require("../package.json").version)
      .catch(() => ""),
    // apiPost/apiGet return a transport envelope {ok,status,json,text}; the session's contract is the
    // PARSED body, so unwrap here. A non-JSON or failed response becomes a clean error object rather
    // than an empty render — "(no output)" would read as "Estelle had nothing to say", which is a lie
    // when the truth is that the request failed.
    post: async (p, body, key) => unwrap(await apiPost(p, body, key)),
    get: async (p, key) => unwrap(await apiGet(p, key)),
    readFile: (rel) => { try { return fs.readFileSync(path.resolve(process.cwd(), rel), "utf8"); } catch { return null; } },
    prompt, out: (l) => console.log(l),
    // The thinking indicator writes ONLY to a terminal. Piped output (CI, a scripted run) must stay exactly
    // what it was — animation frames spliced into it would corrupt whatever is reading.
    write: process.stdout.isTTY ? (s) => process.stdout.write(s) : null,
    c: { teal, dim, bold, green, amber, grey, red },
    now: () => Date.now(),
  });
  rl.close();
  process.exitCode = code || 0;
}

/** The parsed body of an api{Post,Get} envelope, or an honest error — never a silent empty. */
function unwrap(r) {
  if (r && r.json) return r.json;
  return { error: { message: (r && (r.text || `HTTP ${r.status}`)) || "no response" } };
}

// The pure decisions, exported for the unit tests. Requiring this file must NEVER run a command, hence the
// main guard below — a test that imports the CLI would otherwise drop into the interactive session.
module.exports = {
  positionalPaths, repoRelative, changedFromGitOutput, reindexBody, partitionSecrets, partitionIngestable,
  syncBody, filePayload, errText, EXT, SECRET_RE,
  estimateBody, sweepFitLines, mtok, failedOnSize,
};

if (require.main === module) {
  (async () => {
    const cmd = process.argv[2];
    if (!cmd || cmd.startsWith("--") && !has("--help")) await cmdSession();
    else if (cmd === "init") await cmdInit();
    else if (cmd === "sweep") await cmdSweep();
    else if (cmd === "reindex") await cmdReindex();
    else if (cmd === "connect") cmdConnect();
    else if (cmd === "remove" || cmd === "disconnect" || cmd === "off") cmdRemove();
    else if (cmd === "github") await cmdGithub();
    else if (cmd === "monitor") await cmdMonitor();
    else if (cmd === "research") await cmdResearch();
    else if (cmd === "memory") await cmdMemory();
    else if (cmd === "ask") await cmdAsk();
    else if (cmd === "recall") await cmdRecall();
    else if (cmd === "verify") await cmdVerify();
    else if (cmd === "gate") await cmdGate();
    else if (cmd === "hook") await cmdHook();
    else if (cmd === "install-hooks") cmdInstallHooks();
    else if (cmd === "uninstall-hooks") cmdUninstallHooks();
    else help();
  })().catch((e) => {
    // No server payload (or bug) may ever surface as a raw stack dump — styled error, non-zero exit.
    console.log(red("estelle: unexpected error — " + String((e && e.message) || e)));
    process.exitCode = 1;
  });
}
