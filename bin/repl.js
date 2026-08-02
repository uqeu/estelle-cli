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

const readline = require("readline");
// The half of the session that never touches the network: the `!` shell escape, the autonomy ceiling, and
// the git diff /gate and /scan grade. Kept out of here so this file stays a router.
const local = require("./session-commands.js");
const slashMenu = require("./slash-menu.js");
// Writing a returned diff to the working tree, and the shift+tab switch that says under which ceiling.
// Both are new primitives rather than new endpoints — the server has no say in what happens on your disk,
// which is exactly why the guards for it live client-side and fail closed.
const apply = require("./apply.js");
const modeUi = require("./mode-ui.js");
// THE CURATED TURN. Every other host makes Estelle a guest in someone else's prompt; here it OWNS the
// transcript, so it decides what each turn carries and what never gets carried again. See curate.js.
const curate = require("./curate.js");

// ── credential (paste once, ever) ───────────────────────────────────────────────
// The session WRITES the key here; ten other commands READ it. Both sides now go through one leaf module
// so they cannot drift apart again — they already had, and the cost was the first-run flow: this file
// saved the key, printed "Run `estelle sweep`", and sweep resolved its key from the environment only.
const auth = require("./auth.js");
const { readAuth, writeAuth, storedKey } = auth;

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
  context: "what this session is carrying into your next turn",
  gate: "run the merge gate on your staged diff (or /gate <ref>)",
  scan: "security scan — secrets, SAST, dependency CVEs",
  improve: "ranked, grounded improvements for this repo",
  verify: "check a file for APIs that don't exist",
  apply: "write the last /work diff to your working tree",
  undo: "put the last apply back",
  mode: "show / lower the autonomy ceiling (shift+tab cycles it)",
  status: "endpoint, key, filed repo, ceiling",
  shell: "run a shell command right here — e.g. !git status",
  clear: "clear the screen",
  exit: "leave (ctrl-d does it too)",
};

// `shell` is documented but never DISPATCHED as a slash command — the `!` form is the real one, and a
// `/shell` that fell through to an MCP tools/call would be a confusing 'unknown tool'.
const HELP_ONLY = new Set(["shell"]);

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

// The input polish (paste collapsing, history, the spinner) lives in input-ui.js and is re-exported at
// the bottom of this file, so repl.js stays a router and nothing that imports them has to change.
const inputUi = require("./input-ui.js");
const { collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine,
        interruptAction, spinnerPlan, withSpinner, HISTORY_MAX } = inputUi;


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
  // ACCOUNT — the id is here because it is the handle a support conversation needs, and `account_id` is
  // designed to be safe to share (api_account.py:506). The email alone cannot identify a workspace.
  if (s.email) {
    out.push(["account", [s.email, s.plan, s.account_id].filter(Boolean).join(" · ")]);
  }
  // REPO — the name AND what Estelle actually holds for it, on one line. Two numbers that answer "does it
  // know my code yet?" without a second command.
  if (s.repo) {
    const held = [s.files ? `${s.files} files` : "", s.memories ? `${s.memories} memories` : ""]
      .filter(Boolean).join(" · ");
    out.push(["repo", held ? `${s.repo} · ${held}` : s.repo]);
  } else {
    out.push(["memory", s.files ? `${s.files} files` : "empty — nothing indexed yet"]);
  }
  if (s.session_id) out.push(["session", String(s.session_id)]);
  // ROUTING, never `model`. Estelle's whole ORCHESTRA thesis is that the customer does NOT pick a model —
  // every BYOK key is one pool and the router chooses per task (INV-64/68). A `model:` line in this header
  // would advertise the one thing we say we do not do. See brief §1.1a.
  const providers = Number(s.providers || 0);
  out.push(["routing", providers
    ? `auto · ${providers} provider${providers === 1 ? "" : "s"} · /routing to see picks`
    : "auto · /routing to see picks"]);
  if (s.directory) out.push(["directory", String(s.directory)]);
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
 * line, ``out`` writes one, ``c`` is the colour palette, ``diff`` computes the unified diff /gate and
 * /scan grade (git, by default).
 */
async function runSession(deps) {
  const { post, get, prompt, out, c, cwd, now } = deps;
  let key = deps.key || storedKey();

  out("");
  // BOXED (§1.1b). Codex and Kimi both box their headers; the brief's "No rectangle" bullet was an invented
  // constraint from a garbled voice note, never a founder decision, and it has been deleted — see
  // PLAN-ERRATA and composer.js's `headerBox`.
  {
    const box = require("./composer.js").headerBox(
      "estelle", deps.version ? `v${String(deps.version).replace(/^v/, "")}` : "",
      (process.stdout && process.stdout.columns) || 80);
    // Padding is computed from the PLAIN text lengths, never from the coloured string — an escape code is
    // zero columns wide on screen and eight characters long in memory, and measuring the painted string is
    // how a box comes out ragged the moment colour is on.
    const left = "estelle  by Fate Labs";
    const right = deps.version ? `v${String(deps.version).replace(/^v/, "")}` : "";
    const inner = box.head.length - 2;
    const gap = Math.max(1, inner - left.length - right.length - 2);
    out(`  ${c.dim(box.head)}`);
    out(`  ${c.dim("│")} ${c.bold(c.teal("estelle"))}${c.dim("  by Fate Labs")}`
        + `${" ".repeat(gap)}${c.dim(right)} ${c.dim("│")}`);
    out(`  ${c.dim(box.foot)}`);
  }
  // THE UPDATE PROMPT (brief §2.6). Fire-and-forget: the session has already printed its banner and will
  // start regardless, so a slow or unreachable registry costs nothing. Customers sat on the destructive
  // 0.1.3 across three release attempts because nothing ever told them a fix existed — see update-check.js.
  (deps.updateNotice || (async () => ""))()
    .then((notice) => { if (notice) out(`  ${c.amber(notice)}`); })
    .catch(() => {});                    // offline is SILENT, never a warning about our own connectivity
  out("");

  if (!key) {
    // First run: one paste, ever. Never re-prompted, never echoed back.
    // `/keys` 404s — measured 2026-08-01, both URLs fetched. The dashboard route is `/dashboard/keys`.
    // The very first instruction a new customer follows was wrong.
    out(`  ${c.dim("Paste your Estelle key — get one at fatelabs.ca/dashboard/keys")}`);
    // MASKED. The comment above has claimed "never echoed back" since this shipped, and nothing enforced
    // it — the session's readline attaches stdout in terminal mode, which echoes. `promptSecret` is
    // supplied only on a TTY; on a pipe (CI, every scripted test) there is no terminal echo to suppress
    // and the queued reader is the correct path. See cli/bin/secret-prompt.js.
    const pasted = await (deps.promptSecret || prompt)(`  ${c.teal("key")} ${c.dim("›")} `);
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
  const header = statusLines({ ...(status || {}), repo: (status && status.repo) || cwd,
                               directory: deps.cwdPath || "" });
  const labelWidth = header.reduce((w, [label]) => Math.max(w, label.length), 0) + 2;
  for (const [label, value] of header) out(`  ${c.dim(label.padEnd(labelWidth))}${value}`);

  // DISPLAY and DECISION are separate on purpose. The header now shows `repo_files` and `memories` as the
  // two different numbers they are (§1.1) — but "is this repo indexed?" must still be answered by EITHER,
  // or an account holding 16,991 memories with a 0 file count gets told to run a sweep it already ran.
  // That exact regression is what the old `repo_files || memories` fallback existed to prevent; splitting
  // the display without splitting the decision would have quietly reintroduced it.
  if (status && !status.files && !status.memories) {
    // Estelle knows nothing about this repo — say so and point at the sweep rather than answering from
    // nothing. (`estelle sweep` already walks the tree and uploads; the session doesn't reimplement it.)
    out("");
    out(`  ${c.amber("This repo isn't indexed yet.")} ${c.dim("Run `estelle sweep` so answers can cite your code.")}`);
  }

  const greeting = welcomeBack(status && status.last_session, now && now());
  if (greeting) { out(""); out(`  ${greeting.split("\n").join("\n  ")}`); }

  out("");
  out(`  ${c.dim("ask anything · /help for commands · shift+tab cycles the mode · ctrl-d to leave")}`);
  out("");

  // The autonomy ceiling, fetched LAZILY. Only /mode, /status and the write path read it, and a fourth
  // blocking GET on every session start would cost every user latency for a value most turns never touch.
  // `null` means "not asked yet"; `""` means "asked and could not get an answer" — a distinction the
  // fail-closed rendering depends on.
  // `transcript` is the session Estelle OWNS. It is not a log to replay: every turn is assembled from it, and
  // a failed attempt that falls out of the verbatim window leaves its LESSON behind rather than its wreckage.
  const state = { mode: "propose", serverMode: null, transcript: curate.emptyTranscript() };
  const serverMode = async () => {
    if (state.serverMode !== null) return state.serverMode;
    const scope = await get("/autonomy/scope", key).catch(() => null);
    const dial = scope && !scope.error ? scope.global : "";
    state.serverMode = local.modeRank(dial) >= 0 ? dial : "";
    // The local ceiling STARTS where the account's dial is, so /mode can only ever lower it. Starting it
    // anywhere else would either overstate what Estelle may do, or understate it and refuse work the
    // customer has actually paid for and consented to.
    if (state.serverMode) state.mode = state.serverMode;
    return state.serverMode;
  };

  // THE MODE, MADE VISIBLE. Codex renders its mode in a persistent footer under the composer; a readline
  // session has no footer to render into, so the prompt itself carries it — it is the one thing on screen
  // before every line you type.
  //
  // Before the dial has been ASKED (it is lazy, see above) the prompt shows the local mode plainly. A "?"
  // there would claim "we checked and could not tell", which is a different and more alarming fact than
  // "we have not needed to check yet".
  const promptFor = () => {
    const label = state.serverMode === null ? state.mode : modeUi.promptLabel(state.mode, state.serverMode);
    return `${c.dim(label)} ${c.teal("›")} `;
  };
  // shift+tab. The KEY is Codex's (`KeyCode::BackTab` → `cycle_collaboration_mode`); the SEMANTICS are not
  // — this only ever moves the local ceiling, which can lower what happens and never raise it.
  const cycle = async () => {
    const dial = await serverMode();
    state.mode = modeUi.nextMode(state.mode, dial);
    return { banner: modeUi.modeBanner(state.mode, dial, c), prompt: promptFor() };
  };
  const unbind = (deps.bindKeys || (() => () => {}))(cycle);
  // THE SLASH MENU. The skill list is fetched ONCE here and cached for the session: the menu must be LOCAL
  // and instant, and a keystroke that blocks on a network call would be worse than no menu at all. A failed
  // fetch degrades to the built-in commands only — never to a hang, and never to an empty menu that looks
  // like the commands vanished.
  let skillRows = [];
  (async () => {
    const listed = await get("/skills", key).catch(() => null);
    const all = (listed && listed.skills) || [];
    skillRows = all.map((s) => ({ name: s.name, short: s.short || s.summary || "" }));
  })();
  const menuFor = () => slashMenu.menuRows(
    COMMANDS, new Set(Object.keys(COMMANDS).filter((n) => !local.HELP_ONLY.has(n))), skillRows);
  const unbindMenu = (deps.bindMenu || (() => () => {}))(menuFor);
  const leave = (code) => { unbind(); unbindMenu(); return code; };

  for (;;) {
    const line = await prompt(promptFor());
    if (line === null) { out(""); return leave(0); }      // ctrl-d

    // `!cmd` runs on YOUR machine — no server, and deliberately no autonomy ceiling. The ceiling governs
    // what ESTELLE may do unsupervised; a command the human just typed IS the trusted trigger the whole
    // contract is defined against (autonomy.py invariant I1), so gating it there would be a category
    // error. What does apply is the danger heuristic, imported from the hook rather than re-written.
    const bang = local.parseBang(line);
    if (bang) { await local.runShell(bang.command, { out, c, prompt }); out(""); continue; }

    const input = parseInput(line);
    if (!input.text && input.kind === "ask") continue;     // a bare Enter is not a question

    if (input.kind === "command") {
      const done = await handleLocal(input, { out, c, state, serverMode, key, deps });
      if (done === "exit") return leave(0);
      if (done === "handled") continue;
    }

    // The one command that can produce a change. A KNOWN read_only stops it HERE rather than spending a
    // round-trip to be refused; an unknown dial does not, because the server is the enforcement point.
    if (input.kind === "command" && input.name === "work") {
      const why = local.workRefusal(state.mode, await serverMode());
      if (why) { out(`  ${c.amber("!")} ${why}`); out(""); continue; }
    }

    const ctx = {};
    if (input.kind === "command" && (input.name === "gate" || input.name === "scan")) {
      // "git broke" and "nothing staged" are DIFFERENT answers and only one is safe to shrug at. Sending
      // neither is the fix: an empty body is what made both of these error on every call.
      const diff = (deps.diff || local.stagedDiff)(input.arg);
      if (diff === null) {
        out(`  ${c.red("✗ could not compute the diff")} `
          + `${c.dim(input.arg ? `(is ${input.arg} a valid ref?)` : "(git failed)")} `
          + `${c.red("— BLOCKED (fail-closed).")}`);
        out("");
        continue;
      }
      if (!local.diffBody(diff)) {
        out(`  ${c.amber(input.arg ? `No diff vs ${input.arg}.` : "Nothing staged.")} `
          + c.dim("Stage some changes, or name a base ref: /gate main"));
        out("");
        continue;
      }
      ctx.diff = diff;
    }

    const route = routeInput(input, ctx);
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
      // THE CURATED TURN. /deep-search takes a question, not a message array, so the working set is rendered
      // into it: the recent turns verbatim, then the lessons distilled from the failed attempts that are no
      // longer verbatim. What does NOT travel is the wreckage those lessons came from — this is the eviction
      // half of the thesis, which only works because Estelle assembles this prompt itself.
      route.body.question = curate.renderTurn(curate.workingSet(state.transcript), input.text);
    }
    const raw = await withSpinner(
      input.kind === "ask" ? "thinking" : `running /${input.name}`,
      () => (route.method === "GET"
        ? get(route.query && route.query.id ? `${route.path}?id=${encodeURIComponent(route.query.id)}` : route.path, key)
        : post(route.path, route.body, key)
      ).catch((e) => ({ error: { message: String((e && e.message) || e) } })),
      { write: deps.write });
    const res = route.mcp && !(raw && raw.error && raw.error.message) ? mcpText(raw) : raw;
    // Record what was ASKED and what came back — the raw line the user typed, never the assembled prompt, or
    // the next turn would carry a copy of the turn before it and grow quadratically.
    state.transcript = curate.record(
      curate.record(state.transcript, { role: "user", content: line }),
      { role: "assistant", content: curate.replyText(res) });
    out("");
    // THE DIFF USED TO STOP HERE. /work planned, implemented, gated and repaired a change, drew the diff,
    // and threw it away — the CLI had no path from a returned diff to a file on disk, so there was nothing
    // a mode switch could ever have accepted. When there IS one, the apply flow renders it (as its own
    // receipt) and decides under the ceiling, so renderAnswer must not draw it twice.
    const offerApply = !!(res && res.diff && input.kind === "command" && input.name === "work");
    out(renderAnswer(offerApply ? { ...res, diff: "" } : res, c));
    if (offerApply) {
      state.lastDiff = res.diff;
      out("");
      await apply.runApply(res.diff, {
        out, c, prompt, cwd: deps.cwdPath, root: deps.root,
        localMode: state.mode, serverMode: await serverMode(),
      });
    }
    out("");
  }
}

/**
 * The commands answered HERE, with no request at all — the screen, the help, the ceiling, and what this
 * session is pointed at. Returns "exit" to leave the session, "handled" when it answered, and "" to fall
 * through to the network router (which is what every OTHER slash command must still do, or the CLI and
 * the MCP door drift apart the moment a tool is added).
 */
async function handleLocal(input, ctx) {
  const { out, c, state, serverMode, key, deps } = ctx;
  switch (input.name) {
    case "exit": case "quit":
      return "exit";

    case "clear":
      out("\x1b[2J\x1b[H");
      return "handled";

    case "help":
      for (const [name, what] of Object.entries(COMMANDS)) {
        out(`  ${c.teal((HELP_ONLY.has(name) ? "!<cmd>" : "/" + name).padEnd(11))}${c.dim(what)}`);
      }
      out("");
      return "handled";

    case "mode": {
      const dial = await serverMode();
      const wanted = input.arg ? local.parseMode(input.arg) : "";
      if (input.arg && !wanted) {
        out(`  ${c.amber("!")} ${c.dim(`no mode called "${input.arg}" — one of ${local.MODES.join(" · ")}`)}`);
        out(`  ${c.dim(`  (plan = read_only, auto = execute)`)}`);
        out("");
        return "handled";
      }
      if (wanted) state.mode = wanted;
      for (const line of local.modeReport(state.mode, dial, c)) out(line);
      out("");
      return "handled";
    }

    case "context": {
      // The receipt for the curated turn. A context policy nobody can inspect is a context policy nobody can
      // trust, so this prints exactly what the next question will carry and what it costs.
      const report = curate.contextReport(state.transcript);
      for (const [label, value] of report.lines) out(`  ${c.dim(label.padEnd(10))}${value}`);
      for (const lesson of report.lessons) out(`  ${c.dim("·")} ${c.dim(lesson)}`);
      out("");
      return "handled";
    }

    case "status": {
      for (const [label, value] of local.statusRows({
        api: (deps && deps.api) || "", keyMasked: key ? maskKey(key) : "",
        repo: (deps && deps.repo) || "", mode: state.mode, serverMode: await serverMode(),
      })) out(`  ${c.dim(label.padEnd(10))}${value}`);
      out("");
      return "handled";
    }

    case "apply": {
      // The diff /work just produced, applied on purpose rather than only when it was first offered —
      // "let me read that again first" is the normal way a careful person reviews a change.
      if (!state.lastDiff) {
        out(`  ${c.amber("!")} ${c.dim("no diff yet — run /work <task> first.")}`);
        out("");
        return "handled";
      }
      await apply.runApply(state.lastDiff, {
        out, c, prompt: deps.prompt, cwd: deps.cwdPath, root: deps.root,
        localMode: state.mode, serverMode: await serverMode(),
      });
      out("");
      return "handled";
    }

    case "undo": {
      const r = apply.undoLast(deps.root || apply.repoRoot(deps.cwdPath) || deps.cwdPath || process.cwd());
      if (!r.ok) out(`  ${c.amber("!")} ${c.dim(r.why)}`);
      else out(`  ${c.green("✓ put back")} ${c.dim((r.restored || []).join(" · "))}`);
      out("");
      return "handled";
    }

    case "sweep":
      // Advertised in /help since the session shipped, but never routed: it fell through to an MCP
      // tools/call for a tool that has never existed, so it answered "unknown tool". The sweep reads your
      // LOCAL tree and uploads it — there is no server call that can do it, so say where it lives.
      out(`  ${c.amber("!")} ${c.dim("a sweep reads your local files — run it as a command: ")}${c.teal("estelle sweep")}`);
      out(`  ${c.dim("  or ")}${c.teal("estelle reindex")}${c.dim(" for just what changed since HEAD.")}`);
      out("");
      return "handled";

    default:
      return "";
  }
}

/**
 * Which existing Estelle endpoint serves this input. Every branch lands on a route the server already
 * has — a typed question is a Deep Search, `/orchestra` is the fleet, `/gate` is the merge verdict — so
 * the session adds a conversation, not a parallel API.
 */
function routeInput(input, ctx) {
  if (input.kind === "ask") return { path: "/deep-search", body: { question: input.text } };
  const arg = input.arg || "";
  // /gate and /scan grade a unified DIFF, and only the client can compute one from git. They used to route
  // with an empty `{}`, so the server answered "the merge gate needs a 'diff'" on EVERY call while /help
  // advertised both as working commands — the two most load-bearing commands in the product, broken in the
  // session for as long as the session has existed. The caller computes the diff and passes it here.
  const diff = local.diffBody((ctx || {}).diff);
  switch (input.name) {
    case "orchestra": return { path: "/orchestra", body: { task: arg } };
    case "work":      return { path: "/work", body: { task: arg } };
    case "gate":      return { path: "/gate", body: diff || {} };
    case "scan":      return { path: "/scan", body: diff || {} };
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
    // TWO DIFFERENT NUMBERS, and conflating them was a small lie in the header: `repo_files` is how much
    // of YOUR CODE is indexed, `memories` is everything Estelle holds (code + conversations + decisions).
    // Reporting memories under the label "files" told a customer with 880 indexed files that it had 44,374
    // of them. Brief §1.1 asks for both, separately labelled.
    files: mem.repo_files || (account && (account.files || account.memory_files)) || 0,
    memories: mem.memories || 0,
    // The handle a support conversation needs. `account_id` is designed to be safe to share.
    account_id: account && account.account_id,
    // How many provider keys are in the routed pool. Never a MODEL name — see brief §1.1a.
    providers: (overview && overview.provider && overview.provider.configured) ? 1 : 0,
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
  // A GETTER, not a snapshot: the path depends on $HOME, and a value frozen at require-time would answer
  // for whatever HOME was when this module first loaded rather than the one in force at the call.
  get AUTH_FILE() { return auth.authFile(); },
  readAuth, writeAuth, storedKey,
  looksLikeKey, maskKey, humanDuration, relativeTime, parseInput, COMMANDS,
  statusLines, welcomeBack, renderAnswer, runSession, routeInput, sessionStatus, handleLocal, HELP_ONLY,
  expandFileRefs, renderDiff, renderGate, mcpCall, mcpText, unknownTool,
  isSkillExit, runSkill,
  collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine, interruptAction, spinnerPlan,
  withSpinner, HISTORY_MAX,
  readline,
};
