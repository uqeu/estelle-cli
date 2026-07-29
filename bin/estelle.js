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

const MCP_URL = process.env.ESTELLE_MCP_URL || "https://api.fatelabs.ca/mcp";
const API = MCP_URL.replace(/\/mcp$/, "");

// ── pretty terminal ────────────────────────────────────────────────────────────
const C = process.stdout.isTTY && !process.env.NO_COLOR;
const s = (code, t) => (C ? `\x1b[${code}m${t}\x1b[0m` : t);
const teal = (t) => s("38;5;36", t), dim = (t) => s("2", t), bold = (t) => s("1", t);
const green = (t) => s("38;5;35", t), amber = (t) => s("38;5;179", t), grey = (t) => s("38;5;244", t);
const red = (t) => s("38;5;196", t);
const ok = green("✓"), arrow = teal("▸"), dot = grey("·");

function banner() {
  console.log("");
  console.log("  " + bold(teal("◆ estelle")) + dim("  code memory + a 0%-hallucination gate, on your plan"));
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
  let existing = null;
  try { existing = JSON.parse(fs.readFileSync(p, "utf8")); } catch (_) { existing = null; }
  const merged = client === "vscode" ? mergeVsCode(existing, key) : mergeConfig(existing, key);
  const total = Object.keys(client === "vscode" ? merged.servers : merged.mcpServers).length;
  if (dry) return { client, path: p, dry: true, total };
  fs.mkdirSync(path.dirname(p), { recursive: true });
  if (fs.existsSync(p)) fs.copyFileSync(p, p + ".bak");
  fs.writeFileSync(p, JSON.stringify(merged, null, 2) + "\n");
  return { client, path: p, wrote: true, backup: existing !== null };
}

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

// ── commands ─────────────────────────────────────────────────────────────────────
async function cmdInit() {
  banner();
  let key = flag("--key", process.env.ESTELLE_API_KEY);
  if (!key) {
    console.log("  " + bold("Connect your coding agent to Estelle."));
    console.log("  " + dim("Paste your Estelle API key (or get one free at ") + teal("fatelabs.ca") + dim(").") );
    console.log("");
    key = await ask("  " + arrow + " Estelle key: ");
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
    if (r.note) console.log("  " + arrow + " " + bold(PRETTY[client] || client) + dim("  →  ") + r.note);
    else if (r.dry) console.log("  " + arrow + " " + bold(PRETTY[client] || client) + dim("  would write ") + grey(r.path));
    else console.log("  " + ok + " " + bold(PRETTY[client] || client) + dim("  " + r.path) + (r.backup ? dim("  (backed up)") : ""));
  }
  console.log("");
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
const SECRET_RE = /sk-[A-Za-z0-9_-]{20,}|sk_live_[A-Za-z0-9]{10,}|ghp_[A-Za-z0-9]{36}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----/;

const filePayload = (files) => files.map((f) => ({ path: f.path, content: f.content }));

// The ingest body. `repo` FILES this sweep under a name so it lands in its own namespace instead of the
// shared unfiled one; omitted only when we genuinely could not work out a name.
const syncBody = (files, repo) =>
  (repo ? { files: filePayload(files), repo } : { files: filePayload(files) });

// Honesty guard: the server discloses anything its caps dropped ("dropped": {reason: n}) — a green
// "Repo swept." over a half-indexed repo is a false completeness claim, so warn LOUDLY instead.
function warnDropped(json) {
  const d = json && json.dropped;
  if (!d || typeof d !== "object") return false;
  const total = Object.values(d).reduce((a, b) => a + (Number(b) || 0), 0);
  if (!total) return false;
  const reasons = Object.entries(d).map(([k, v]) => `${k}: ${v}`).join(", ");
  console.log("  " + amber(`${total} file(s) were NOT indexed`) + dim(` (${reasons}) — recall cannot cite them.`));
  console.log("  " + dim(oneLine(json.warning || "Batch the remainder across further sweeps.")));
  return true;
}

async function syncUpload(files, key, repo) {
  process.stdout.write("  " + dim("Uploading to your Estelle memory… "));
  const r = await apiPost("/sync", syncBody(files, repo), key);
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
  }
  console.log("");
  console.log("  " + amber("Stopped polling — the ingest continues server-side; check the dashboard."));
  process.exitCode = 1;
}

async function cmdSweep() {
  banner();
  const key = flag("--key", process.env.ESTELLE_API_KEY);
  if (!key) { console.log("  " + amber("Need --key <ESTELLE_KEY> to sweep.")); return; }
  const root = flag("--path", process.cwd());
  console.log("  " + dim("Scanning ") + bold(root) + dim(" for source files…"));
  const collected = collectFiles(root);
  const flagged = collected.filter((f) => SECRET_RE.test(f.content));
  const files = collected.filter((f) => !SECRET_RE.test(f.content));
  if (flagged.length) {
    console.log("  " + amber(`Skipped ${flagged.length} file(s) that look like they contain a live secret — they stay on your machine:`));
    for (const f of flagged.slice(0, 8)) console.log("    " + red("✗") + " " + f.path);
  }
  if (!files.length) { console.log("  " + amber("No ingestable source files found here.")); return; }
  const bytes = files.reduce((n, f) => n + f.content.length, 0);
  console.log("  " + dim(`Found ${bold(String(files.length))}${dim(" files")} ${dot} ${(bytes / 1024).toFixed(0)} KB ${dot} these upload to ${API}`));
  const repo = repoNameFor(root);
  if (repo) console.log("  " + dim("Filing under repo ") + bold(repo) + dim(" — recall stays scoped to it."));
  if (files.length < SYNC_MAX_FILES) { await syncUpload(files, key, repo); return; }
  await ingestWithProgress(files, key, repo);
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
function repoNameFor(root) {
  const explicit = flag("--repo", "");
  if (explicit) return explicit;
  try {
    const url = execFileSync("git", ["-C", root, "remote", "get-url", "origin"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
    // git@host:owner/name.git · https://host/owner/name.git · ssh://git@host/owner/name
    const m = url.replace(/\.git$/, "").match(/[:/]([^/:]+)\/([^/]+)$/);
    if (m) return `${m[1]}/${m[2]}`;
  } catch (_) { /* not a git repo, no origin, or no git binary — fall through */ }
  return path.basename(path.resolve(root)) || "";
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
  const key = flag("--key", process.env.ESTELLE_API_KEY);
  if (!key) console.log("  " + amber("Need an Estelle key — pass ") + bold("--key <KEY>") + amber(" or set ") + bold("ESTELLE_API_KEY") + amber("."));
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
    const res = await fetch(`${API}${pathname}`, { headers: { Authorization: `Bearer ${key}` } });
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
  console.log("  " + bold("Verdict  ") + (v.merge ? green(v.verdict || "clean — safe to merge") : amber(v.verdict || "blocked")));
  for (const b of asArray(v.blockers)) console.log("  " + red("✗") + " " + (b.body || `${b.path}:${b.line}`));
  for (const w of asArray(v.warnings)) console.log("  " + amber("!") + " " + (w.body || `${w.path}:${w.line}`));
  console.log("");
  if (!v.merge) process.exitCode = 1;
}

function help() {
  banner();
  const row = (c, d) => console.log("  " + teal(c.padEnd(26)) + dim(d));
  console.log("  " + bold("Usage") + dim("  npx @fatelabs/estelle <command>"));
  console.log("");
  row("init [--key K]", "auto-detect your editors and write the MCP config (then verify it answers)");
  row("sweep [--key K]", "ingest the current repo (git-visible files) into your Estelle memory");
  row("connect <client>", "show the one-liner / config for one client");
  row("remove", "disconnect: delete Estelle from every editor config (alias: disconnect, off; offline)");
  row("ask <question>", "ask Estelle about your codebase (grounded, on your plan)");
  row("recall <query> [--repo R]", "search your Estelle memory + code (--repo scopes a multi-repo account)");
  row("verify <file>", "check a file for hallucinated APIs against your repo");
  row("gate [--base ref]", "run the merge gate on your staged diff (or vs a base ref)");
  console.log("");
  console.log("  " + dim("Flags: ") + grey("--key K  --client cursor  --dry-run  --url <mcp>  --path <dir>  --base <ref>"));
  console.log("  " + dim("Clients: ") + grey(Object.keys(PRETTY).join(", ")));
  console.log("  " + dim("Egress: ") + grey(`init/remove/connect stay local (init pings ${API} once to verify); sweep sends repo files,`));
  console.log("  " + dim("        ") + grey(`gate your staged diff, verify one file, ask/recall your question — all to ${API}.`));
  console.log("");
}

/** `estelle hook <mode>` — the runtime a customer's Claude Code calls on every edit/command (via init). */
async function cmdHook() {
  const mode = process.argv[3] || "ground";
  const hook = require("./hook.js");
  let payload = {};
  try { payload = JSON.parse(fs.readFileSync(0, "utf8")); } catch { /* a bad payload must never break the action */ }
  const key = storedKeyOrEnv();
  await hook.runHook(mode, payload, {
    post: async (p, body) => unwrap(await apiPost(p, body, key)),
    out: (o) => console.log(JSON.stringify(o)),
  });
}

/** `estelle install-hooks` — write Estelle's hooks into Claude Code's settings so it fires with no opt-in. */
function cmdInstallHooks() {
  const hook = require("./hook.js");
  const file = hook.claudeSettingsPath();
  let existing = {};
  try { existing = JSON.parse(fs.readFileSync(file, "utf8")); } catch { /* first time, or no settings yet */ }
  const merged = hook.mergeHooks(existing, "npx -y @fatelabs/estelle");
  fs.mkdirSync(path.dirname(file), { recursive: true });
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
  const code = await repl.runSession({
    version: require("../package.json").version,
    key: flag("--key", storedKeyOrEnv()),
    cwd: path.basename(process.cwd()),
    // apiPost/apiGet return a transport envelope {ok,status,json,text}; the session's contract is the
    // PARSED body, so unwrap here. A non-JSON or failed response becomes a clean error object rather
    // than an empty render — "(no output)" would read as "Estelle had nothing to say", which is a lie
    // when the truth is that the request failed.
    post: async (p, body, key) => unwrap(await apiPost(p, body, key)),
    get: async (p, key) => unwrap(await apiGet(p, key)),
    readFile: (rel) => { try { return fs.readFileSync(path.resolve(process.cwd(), rel), "utf8"); } catch { return null; } },
    prompt, out: (l) => console.log(l),
    c: { teal, dim, bold, green, amber, grey, red },
    now: () => Date.now(),
  });
  rl.close();
  process.exitCode = code || 0;
}

const storedKeyOrEnv = () => require("./repl.js").storedKey();

/** The parsed body of an api{Post,Get} envelope, or an honest error — never a silent empty. */
function unwrap(r) {
  if (r && r.json) return r.json;
  return { error: { message: (r && (r.text || `HTTP ${r.status}`)) || "no response" } };
}

(async () => {
  const cmd = process.argv[2];
  if (!cmd || cmd.startsWith("--") && !has("--help")) await cmdSession();
  else if (cmd === "init") await cmdInit();
  else if (cmd === "sweep") await cmdSweep();
  else if (cmd === "connect") cmdConnect();
  else if (cmd === "remove" || cmd === "disconnect" || cmd === "off") cmdRemove();
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
