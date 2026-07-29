"use strict";
// The `estelle` session — you type the name ONCE, then you just talk.
//
// The rest of this CLI is command-style (`estelle sweep`, `estelle gate`), which is right for CI and
// scripts. It is wrong for working: retyping the tool's name before every thought is friction, and it
// gives Estelle no thread to hold. A session gives it one — every turn is grounded, cited, remembered,
// and indexed, without the user asking for any of that.
//
// Patterns taken from the mature agent CLIs (Codex, OpenCode), reimplemented here rather than vendored:
//   * paste the key ONCE, store it at ~/.estelle/auth.json with 0600 — never re-prompt;
//   * on entry, say what Estelle already knows about THIS repo, and offer to index it when it knows nothing;
//   * greet a returning user with the last session instead of an empty prompt;
//   * slash commands for the things a sentence shouldn't have to express.
//
// Deliberately NOT an agent harness: Estelle's engine (the gate, /work, Orchestra, the repro sandbox)
// lives server-side and is reached over HTTP. This file is the conversation, not a second engine.

const fs = require("fs");
const os = require("os");
const path = require("path");
const readline = require("readline");

const AUTH_DIR = path.join(os.homedir(), ".estelle");
const AUTH_FILE = path.join(AUTH_DIR, "auth.json");

// ── credential (paste once, ever) ───────────────────────────────────────────────
function readAuth() {
  try {
    const raw = JSON.parse(fs.readFileSync(AUTH_FILE, "utf8"));
    return typeof raw.key === "string" && raw.key.trim() ? raw.key.trim() : "";
  } catch {
    return "";
  }
}

function writeAuth(key) {
  // 0700/0600: the key is a bearer credential for the whole account — never world-readable, and never
  // echoed back to the terminal once stored.
  fs.mkdirSync(AUTH_DIR, { recursive: true, mode: 0o700 });
  fs.writeFileSync(AUTH_FILE, JSON.stringify({ key }, null, 2) + "\n", { mode: 0o600 });
}

/** The key from (in order) an explicit flag, the environment, or the stored file — "" when unset. */
function storedKey(env) {
  const e = env || process.env;
  return (e.ESTELLE_API_KEY || "").trim() || readAuth();
}

// ── pure helpers (unit-tested; the I/O around them is thin) ─────────────────────

/** A bearer key is only plausible if it is long enough to be one — catches a pasted email or a typo. */
function looksLikeKey(text) {
  const k = String(text || "").trim();
  return k.length >= 20 && !/\s/.test(k);
}

/** `estelle_live_abc…xyz` → `estelle_live_abc…3f9` — enough to recognise, never the whole secret. */
function maskKey(key) {
  const k = String(key || "");
  return k.length <= 12 ? "•".repeat(k.length) : `${k.slice(0, 8)}…${k.slice(-4)}`;
}

/** "6h 37m" / "12m" / "—" from a span of seconds; the session line a returning user reads. */
function humanDuration(seconds) {
  const s = Math.max(0, Math.floor(Number(seconds) || 0));
  if (!s) return "—";
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  if (h && m) return `${h}h ${m}m`;
  if (h) return `${h}h`;
  return `${m || 1}m`;
}

/** "2 days ago" / "just now" — relative time reads faster than a timestamp when you're re-entering work. */
function relativeTime(iso, now) {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const diff = Math.max(0, ((now || Date.now()) - then) / 1000);
  if (diff < 90) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  return `${Math.round(diff / 86400)}d ago`;
}

/** Split a typed line into a slash command + its argument. Anything else is a question for Estelle. */
function parseInput(line) {
  const t = String(line || "").trim();
  if (!t.startsWith("/")) return { kind: "ask", text: t };
  const [head, ...rest] = t.slice(1).split(/\s+/);
  return { kind: "command", name: head.toLowerCase(), arg: rest.join(" ") };
}

// The commands a sentence shouldn't have to express. `/orchestra` is canonical — `swarm` is not a word
// this product uses.
const COMMANDS = {
  help: "what you can do here",
  init: "a grounded brief of this repo (architecture, chokepoints)",
  memory: "what Estelle knows about this repo",
  sweep: "index this repo into memory",
  sessions: "your recent sessions",
  resume: "pick a past session back up",
  work: "plan → implement → gate → repair a change",
  orchestra: "fan a task across a routed fleet",
  gate: "run the merge gate on your staged diff",
  scan: "security scan — secrets, SAST, dependency CVEs",
  improve: "ranked, grounded improvements for this repo",
  verify: "check a file for APIs that don't exist",
  clear: "clear the screen",
  exit: "leave (ctrl-d does it too)",
};

/**
 * Expand `@path` references into the text Estelle should see, so "why is @auth.py slow?" carries the
 * file instead of only its name. Missing files are reported, never silently dropped — a question
 * answered against a file that wasn't actually read is the failure this product exists to prevent.
 */
function expandFileRefs(text, readFile) {
  const refs = [...new Set((String(text || "").match(/@[\w./-]+/g) || []).map((m) => m.slice(1)))];
  const attached = [], missing = [];
  for (const ref of refs) {
    const body = readFile(ref);
    if (body === null || body === undefined) missing.push(ref);
    else attached.push({ path: ref, content: String(body) });
  }
  return { attached, missing };
}

// ── input polish, learned from the mature agent CLIs ────────────────────────────
// These are the interaction details that separate a REPL you tolerate from one you live in. Each is a
// pure function so it can be tested without a terminal.

const PASTE_MIN_LINES = 3, PASTE_MIN_CHARS = 150, HISTORY_MAX = 50;

/**
 * Collapse a big paste to a token the user can see past. A stack trace pasted into a prompt otherwise
 * buries the question you were asking; `[Pasted ~47 lines]` keeps the line readable while the real text
 * still reaches Estelle on submit. Returns the visible text plus the side table that restores it.
 */
function collapsePaste(text, store) {
  const body = String(text || "");
  const lines = body.split("\n").length;
  if (lines < PASTE_MIN_LINES && body.length < PASTE_MIN_CHARS) return { visible: body, marks: store || [] };
  const marks = (store || []).slice();
  const token = `[Pasted ~${lines} line${lines === 1 ? "" : "s"} #${marks.length + 1}]`;
  marks.push({ token, text: body });
  return { visible: token, marks };
}

/** Put every collapsed paste back before the text is sent — what Estelle sees is what you pasted. */
function expandPastes(visible, marks) {
  let out = String(visible || "");
  for (const m of marks || []) out = out.split(m.token).join(m.text);
  return out;
}

/**
 * Frecency: recent AND frequent beats merely frequent. `score * (1 + hits / (1 + ageDays))`, so the file
 * you touched twice this morning outranks one you opened thirty times last year.
 */
function frecencyScore(base, entry, now) {
  if (!entry || !entry.hits) return base;
  const ageDays = Math.max(0, ((now || Date.now()) - (entry.at || 0)) / 86400000);
  return base * (1 + entry.hits / (1 + ageDays));
}

/**
 * Prompt history, JSONL, newest last. SELF-HEALING: a corrupt line is dropped rather than throwing, so a
 * half-written file (a kill -9 mid-append) never costs you the whole history. Consecutive duplicates are
 * collapsed — pressing up should walk distinct thoughts, not repeats.
 */
function parseHistory(raw) {
  const out = [];
  for (const line of String(raw || "").split("\n")) {
    if (!line.trim()) continue;
    try {
      const e = JSON.parse(line);
      const text = typeof e === "string" ? e : e && e.text;
      if (typeof text === "string" && text.trim() && out[out.length - 1] !== text) out.push(text);
    } catch { /* a torn line is skipped, never fatal */ }
  }
  return out.slice(-HISTORY_MAX);
}

/** One history line to append, or "" when this entry adds nothing (blank, or the same as last time). */
function historyLine(text, previous) {
  const t = String(text || "").trim();
  if (!t || t === previous) return "";
  return JSON.stringify({ text: t, at: Date.now() }) + "\n";
}

/**
 * Ctrl+C is reflexive — people hit it to abandon a half-typed thought, not to quit. So it CLEARS a
 * non-empty line and only exits on an empty one. Returns what the session should do.
 */
function interruptAction(currentText) {
  return String(currentText || "").trim() ? "clear" : "exit";
}

/**
 * Show a spinner only if the work outlives `delayMs`, and once shown hold it for `minMs`. Without the
 * delay every 200ms call flashes a spinner; without the hold it vanishes before the eye resolves it.
 */
function spinnerPlan(elapsedMs, shownAtMs, delayMs = 500, minMs = 3000) {
  if (shownAtMs == null) return elapsedMs >= delayMs ? "show" : "wait";
  return elapsedMs - shownAtMs >= minMs ? "may-hide" : "hold";
}

/** A unified diff, coloured. Nothing else in a terminal communicates a change this fast. */
function renderDiff(diff, c) {
  return String(diff || "").split("\n").map((line) => {
    if (line.startsWith("+++") || line.startsWith("---")) return c.dim(line);
    if (line.startsWith("@@")) return c.teal(line);
    if (line.startsWith("+")) return c.green(line);
    if (line.startsWith("-")) return c.red(line);
    return c.dim(line);
  }).join("\n");
}

/** The gate verdict as one scannable line — the receipt a merge decision is actually made on. */
function renderGate(gate, c) {
  if (!gate) return "";
  const parts = [];
  const clean = gate.merge === true || gate.verdict === "clean";
  parts.push(clean ? c.green("✓ gate clean") : c.red("✗ gate blocked"));
  if (gate.referenced != null) parts.push(c.dim(`${gate.grounded_count ?? gate.referenced}/${gate.referenced} symbols`));
  for (const [key, label] of [["ungrounded", "invented"], ["type_errors", "type errors"],
                              ["arity_errors", "arity"], ["secrets", "secrets"]]) {
    const n = (gate[key] || []).length;
    if (n) parts.push(c.red(`${n} ${label}`));
  }
  if (gate.verdict && !clean) parts.push(c.dim(gate.verdict));
  return "  " + parts.join(c.dim(" · "));
}

/** The status block shown on entry — what Estelle already knows, before you ask it anything. */
function statusLines(status) {
  const s = status || {};
  const out = [];
  if (s.email) out.push(["account", s.plan ? `${s.email} · ${s.plan}` : s.email]);
  out.push(["memory", s.files ? `${s.files} files` : "empty — nothing indexed yet"]);
  if (s.repo) out.push(["repo", s.repo]);
  return out;
}

/** The returning-user line, or "" on a first visit. Reads as a handoff, not a log entry. */
function welcomeBack(session, now) {
  if (!session || !session.at) return "";
  const when = relativeTime(session.at, now);
  const span = session.seconds ? ` · ${humanDuration(session.seconds)}` : "";
  const what = String(session.task || "").trim();
  const head = `Last session${when ? ` · ${when}` : ""}${span}`;
  return what ? `${head}\n  You were on: ${what}` : head;
}

/**
 * Run the interactive session. Every dependency is injected so the loop itself is testable and this
 * file never reaches for a socket on its own: ``post``/``get`` are the API calls, ``prompt`` reads a
 * line, ``out`` writes one, ``c`` is the colour palette.
 */
async function runSession(deps) {
  const { post, get, prompt, out, c, cwd, now } = deps;
  let key = deps.key || storedKey();

  out("");
  out(`  ${c.bold(c.teal("estelle"))}  ${c.dim(deps.version || "")}`);
  out("");

  if (!key) {
    // First run: one paste, ever. Never re-prompted, never echoed back.
    out(`  ${c.dim("Paste your Estelle key — get one at fatelabs.ca/keys")}`);
    const pasted = await prompt(`  ${c.teal("key")} ${c.dim("›")} `);
    if (!looksLikeKey(pasted)) {
      out(`  ${c.red("That doesn't look like an Estelle key.")} ${c.dim("Run estelle again when you have one.")}`);
      return 1;
    }
    key = pasted.trim();
    writeAuth(key);
    out(`  ${c.green("✓")} saved ${c.dim(`${maskKey(key)} → ~/.estelle/auth.json`)}`);
    out("");
  }

  // Reuse the endpoints Estelle already serves (/account, /sessions, /deep-search, /gate, /orchestra)
  // rather than minting a /cli/* surface that would duplicate them — the code-minimalism rung that asks
  // "is it already in the repo?" before writing anything new.
  const status = await sessionStatus({ get, key });
  if (status && status.rejected) {
    out(`  ${c.red("That key was rejected.")} ${c.dim("Delete ~/.estelle/auth.json and run estelle again.")}`);
    return 1;
  }
  for (const [label, value] of statusLines({ ...(status || {}), repo: (status && status.repo) || cwd })) {
    out(`  ${c.dim(label.padEnd(8))}${value}`);
  }

  if (status && !status.files) {
    // Estelle knows nothing about this repo — say so and point at the sweep rather than answering from
    // nothing. (`estelle sweep` already walks the tree and uploads; the session doesn't reimplement it.)
    out("");
    out(`  ${c.amber("This repo isn't indexed yet.")} ${c.dim("Run `estelle sweep` so answers can cite your code.")}`);
  }

  const greeting = welcomeBack(status && status.last_session, now && now());
  if (greeting) { out(""); out(`  ${greeting.split("\n").join("\n  ")}`); }

  out("");
  out(`  ${c.dim("ask anything · /help for commands · ctrl-d to leave")}`);
  out("");

  for (;;) {
    const line = await prompt(`${c.teal("›")} `);
    if (line === null) { out(""); return 0; }             // ctrl-d
    const input = parseInput(line);
    if (!input.text && input.kind === "ask") continue;     // a bare Enter is not a question

    if (input.kind === "command") {
      if (input.name === "exit" || input.name === "quit") return 0;
      if (input.name === "clear") { out("\x1b[2J\x1b[H"); continue; }
      if (input.name === "help") {
        for (const [name, what] of Object.entries(COMMANDS)) out(`  ${c.teal("/" + name.padEnd(10))}${c.dim(what)}`);
        out("");
        continue;
      }
    }

    const route = routeInput(input);
    // A command that would fall through to MCP might be a SKILL — run it SERVER-SIDE first via /skill/run, so
    // the playbook is loaded + injected on the server and only the RESULT comes back. The playbook markdown
    // never reaches the client at all (the real IP lock). Not a skill → fall through to the tool call.
    if (route.mcp && input.kind === "command") {
      const status = await runSkill(input.name.replace(/^skill[-_]/, ""), input.arg, { post, prompt, out, c, key });
      if (status !== "not-skill") { out(""); continue; }
      // A `skill_`-prefixed command IS a skill by name. If /skill/run didn't claim it, it is unknown HERE —
      // never fall through to a raw MCP `skill_<name>` tool, which would hand back the playbook markdown.
      // The playbook is Estelle's IP; the CLI only ever runs skills server-side, never displays them.
      if (/^skill[-_]/.test(input.name)) {
        out("");
        out(`  ${c.amber("!")} ${c.dim(`no such skill: ${input.name.replace(/^skill[-_]/, "")}`)}`);
        out("");
        continue;
      }
    }
    if (input.kind === "ask") {
      // @file references travel WITH the question, so an answer about a file was actually read from it.
      const { attached, missing } = expandFileRefs(input.text, deps.readFile || (() => null));
      if (missing.length) out(`  ${c.amber("!")} ${c.dim(`no such file: ${missing.join(", ")}`)}`);
      if (attached.length) route.body.files = attached;
    }
    const raw = await (route.method === "GET"
      ? get(route.query && route.query.id ? `${route.path}?id=${encodeURIComponent(route.query.id)}` : route.path, key)
      : post(route.path, route.body, key)
    ).catch((e) => ({ error: { message: String((e && e.message) || e) } }));
    const res = route.mcp && !(raw && raw.error && raw.error.message) ? mcpText(raw) : raw;
    out("");
    out(renderAnswer(res, c));
    out("");
  }
}

/**
 * Which existing Estelle endpoint serves this input. Every branch lands on a route the server already
 * has — a typed question is a Deep Search, `/orchestra` is the fleet, `/gate` is the merge verdict — so
 * the session adds a conversation, not a parallel API.
 */
function routeInput(input) {
  if (input.kind === "ask") return { path: "/deep-search", body: { question: input.text } };
  const arg = input.arg || "";
  switch (input.name) {
    case "orchestra": return { path: "/orchestra", body: { task: arg } };
    case "work":      return { path: "/work", body: { task: arg } };
    case "gate":      return { path: "/gate", body: {} };
    case "scan":      return { path: "/scan", body: {} };
    case "improve":   return { path: "/improve", body: arg ? { focus: arg } : {} };
    case "verify":    return { path: "/verify", body: { answer: arg } };
    case "init":      return { path: "/wiki", body: null, method: "GET" };
    case "sessions":  return { path: "/sessions", body: null, method: "GET" };
    case "resume":    return { path: "/session", body: null, method: "GET", query: { id: arg } };
    case "memory":    return { path: "/deep-search", body: { question: "what do you know about this repo?" } };
    case "tools":     return mcpCall("tools/list", {});
    // EVERYTHING ELSE IS AN MCP TOOL. Estelle exposes its whole surface — the code-graph navigation
    // (find_definition, blast_radius, subsystems…), verify, the session diary, and all ~190 live skill
    // playbooks as `skill_<name>` — over one JSON-RPC endpoint. Falling through to it means the CLI
    // reaches every capability the MCP door does, automatically, and a skill added tomorrow is callable
    // the same day with no CLI change. Hand-enumerating them would guarantee the two surfaces drift.
    default:          return mcpCall("tools/call", { name: input.name, arguments: { args: arg } });
  }
}

/** One MCP JSON-RPC envelope against POST /mcp — the single door to Estelle's entire tool surface. */
function mcpCall(method, params) {
  return { path: "/mcp", body: { jsonrpc: "2.0", id: 1, method, params }, mcp: true };
}

/** True when an MCP reply is an "unknown tool" error — the signal to try the name as a skill playbook. */
function unknownTool(res) {
  return !!(res && res.error && /unknown tool/i.test(String(res.error.message || "")));
}

// ── skill sessions ───────────────────────────────────────────────────────────────
// A skill isn't a document to print — it's work to run. /deepen-architecture just makes the thing better
// (one-shot, no questions); /grill-me DRIVES a back-and-forth. Both run via POST /skill/run, where the SERVER
// loads and injects the playbook and returns only the result — so the playbook (Estelle's IP) never reaches
// the client. The client sends the skill NAME + the conversation; it never holds the playbook text.

/** A line that leaves an interactive skill and returns to the main prompt. */
function isSkillExit(line) {
  return /^\/(done|exit|stop|quit|back)\b/i.test(String(line || "").trim());
}

/** Pull the human-readable text out of an MCP JSON-RPC reply (or its error). */
function mcpText(res) {
  if (!res) return { error: { message: "no reply" } };
  if (res.error) return { error: { message: res.error.message || String(res.error) } };
  const content = (res.result && res.result.content) || [];
  const text = content.map((c) => (c && c.text) || "").join("\n").trim();
  if (res.result && res.result.isError) return { error: { message: text || "tool failed" } };
  const tools = res.result && res.result.tools;
  if (tools) return { answer: tools.map((t) => t.name).sort().join("  ") };
  return { answer: text || "(no output)" };
}

/** The entry status, assembled from endpoints Estelle already serves. Never throws — a cold start still opens. */
async function sessionStatus(deps) {
  const { get, key } = deps;
  const account = await get("/account", key).catch(() => null);
  if (account && account.error && (account.error.code === 404 || account.error.code === 401)) {
    return { rejected: true };
  }
  // The memory counts are NOT on /account — that endpoint serves plan/balance/provider and never carried a
  // file count, so the flat `account.files` this used to read was always undefined and every swept account
  // was told "memory empty — nothing indexed yet" and to run a sweep it had already run. They live on
  // /overview under `memory` as {memories, repo_files, entities}. Best-effort: a failed fetch just means the
  // header omits the count, never a session that refuses to open.
  const overview = await get("/overview", key).catch(() => null);
  const mem = (overview && overview.memory) || {};
  const sessions = await get("/sessions", key).catch(() => null);
  const recent = (sessions && (sessions.sessions || sessions.items) || [])[0] || null;
  return {
    email: account && (account.email || (account.account && account.account.email)),
    plan: account && (account.plan && (account.plan.name || account.plan)),
    // /overview nests the counts under `memory` as {memories, repo_files, entities}. Reading only the flat
    // `files`/`memory_files` names — which the API has never returned — always yielded 0, so a fully swept
    // account was greeted with "memory empty — nothing indexed yet" and told to run `estelle sweep` it had
    // already run. First impression, and it was wrong. The flat names stay as fallbacks for older servers.
    files: mem.repo_files || mem.memories
           || (account && (account.files || account.memory_files)) || 0,
    last_session: recent && {
      at: recent.at || recent.started_at || recent.ended_at,
      seconds: recent.seconds || recent.duration_seconds,
      task: recent.title || recent.task,
    },
  };
}

/** One skill reply, printed with its banner. Never prints a playbook — the server returns only the result. */
function printSkillReply(name, r, interactive, out, c) {
  out("");
  out(`  ${c.teal("⟢ " + name)}${interactive ? c.dim(" — interactive · /done to finish") : ""}`);
  out("");
  out(String((r && r.reply) || "").split("\n").map((l) => `  ${l}`).join("\n"));
  out("");
}

/**
 * Run a skill SERVER-SIDE via /skill/run. Returns "not-skill" (a 404 — the caller falls through to normal
 * tool routing), "needs-model" (a 402 — told the user, no playbook shown), or "ran". A one-shot skill
 * prints its result and returns; an interactive skill loops, sending the growing conversation to the server
 * each turn (the server re-injects the playbook), until /done. The playbook is never fetched to the client.
 */
async function runSkill(name, arg, deps) {
  const { post, prompt, out, c, key } = deps;
  const first = await post("/skill/run", { skill: name, task: String(arg || "") }, key)
    .catch((e) => ({ error: { message: String((e && e.message) || e) } }));
  if (first && first.error) {
    const code = first.error.code;
    if (code === 404) return "not-skill";                    // not a skill → caller uses normal routing
    if (code === 402) {
      out(`\n  ${c.amber("⟢ " + name)} ${c.dim("needs a model — add a provider key to your account, then try again")}\n`);
      return "needs-model";
    }
    out(`\n  ${c.red("✗")} ${first.error.message}\n`);
    return "ran";
  }
  printSkillReply(name, first, !first.done, out, c);
  if (first.done) return "ran";                              // one-shot: produced the result, no questions

  let messages = [{ role: "user", content: "Begin." }, { role: "assistant", content: first.reply || "" }];
  for (;;) {
    const line = await prompt(`${c.teal("…")} `);
    if (line === null || isSkillExit(line)) { out(`  ${c.dim(`↩ left ${name}`)}`); return "ran"; }
    if (!String(line).trim()) continue;
    messages = [...messages, { role: "user", content: line }];
    const r = await post("/skill/run", { skill: name, messages }, key)
      .catch((e) => ({ error: { message: String((e && e.message) || e) } }));
    if (r && r.error) { out(`\n  ${c.red("✗")} ${r.error.message}\n`); continue; }
    printSkillReply(name, r, false, out, c);
    messages = [...messages, { role: "assistant", content: (r && r.reply) || "" }];
  }
}

/** Render one reply: the prose first, then the receipt. The certificate is the point, so it always shows. */
function renderAnswer(res, c) {
  if (!res || res.error) {
    const msg = (res && res.error && (res.error.message || res.error)) || "no reply";
    return `  ${c.red("✗")} ${msg}`;
  }
  const lines = [];
  const answer = String(res.answer || "").trim();
  if (answer) lines.push(answer.split("\n").map((l) => `  ${l}`).join("\n"));

  if (res.scope_ask) return lines.join("\n");             // "which repo?" is the whole reply
  if (res.diff) lines.push(renderDiff(res.diff, c));
  if (res.gate || res.merge !== undefined || res.verdict) lines.push(renderGate(res.gate || res, c));
  if (res.pr_url) lines.push(`  ${c.dim("→")} ${c.teal(res.pr_url)} ${c.dim("· a human merges it")}`);
  const sources = (res.sources || []).map((s) => (s.line ? `${s.file}:${s.line}` : s.file)).slice(0, 4);
  if (res.grounded === true) {
    lines.push(`  ${c.green("✓ grounded")}${sources.length ? c.dim(`  ${sources.join(" · ")}`) : ""}`);
  } else if (res.grounded === false && (res.ungrounded || []).length) {
    lines.push(`  ${c.red("✗ not in this repo:")} ${res.ungrounded.slice(0, 4).join(", ")}`);
  } else if (res.degraded) {
    lines.push(`  ${c.amber("· degraded")} ${c.dim("— answered from memory without a model")}`);
  }
  return lines.join("\n");
}

module.exports = {
  AUTH_FILE, readAuth, writeAuth, storedKey,
  looksLikeKey, maskKey, humanDuration, relativeTime, parseInput, COMMANDS,
  statusLines, welcomeBack, renderAnswer, runSession, routeInput, sessionStatus,
  expandFileRefs, renderDiff, renderGate, mcpCall, mcpText, unknownTool,
  isSkillExit, runSkill,
  collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine, interruptAction, spinnerPlan,
  HISTORY_MAX,
  readline,
};
