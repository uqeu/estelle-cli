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
// ONE definition of "which repo am I in", shared with `estelle sweep` and BOTH always-on hooks. The
// header used to derive its own from basename(cwd) and disagree with all three — see `resolveRepo`.
const repoName = require("./repo-name.js");
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

// Shell tools whose names people type out of habit. A leading one means they want a SHELL, not an answer.
const SHELL_HEADS = new Set(["git", "npm", "npx", "node", "ls", "cd", "cat", "pnpm", "yarn", "make", "docker"]);

/**
 * Did they type a COMMAND at Estelle instead of asking it something?
 *
 * 🔴 THE DEFECT, observed on 0.1.10: typing `estelle sweep` — **the command in our own README** — matched
 * neither a slash command nor `!<cmd>`, so it was sent to the model as a question and came back *"I can't
 * tell what 'estelle sweep' refers to from the provided repository context."* Technically correct and
 * terrible: **the customer paid for a model call to be told their own product's documented command is not
 * a repo symbol.**
 *
 * Returns `{kind, suggest}` or **null**, and null is the important half. This must not eat prose: *"how
 * does estelle sweep work"* is a genuine question ABOUT the command, and answering it with a did-you-mean
 * would be worse than the defect. So the match is anchored — the line must BEGIN with the invocation and
 * contain nothing but it and a known subcommand.
 */
function commandHint(line) {
  const t = String(line || "").trim();
  if (!t) return null;
  const words = t.split(/\s+/);
  if (SHELL_HEADS.has(words[0]) && !/^npx$/.test(words[0])) {
    return { kind: "shell", suggest: `!${t}` };
  }
  // `estelle <cmd>` and `npx @fatelabs/estelle <cmd>` — the two forms the README and the CLI itself print.
  const rest = words[0] === "estelle" ? words.slice(1)
    : (words[0] === "npx" && /estelle/.test(words[1] || "")) ? words.slice(2)
    : (words[0] === "npx" && words[1] === "-y" && /estelle/.test(words[2] || "")) ? words.slice(3)
    : null;
  if (!rest || !rest.length) return null;
  const name = rest[0].toLowerCase();
  // Only a KNOWN command suggests. `estelle blorp` is not a typo we can fix, and guessing at it would put
  // us back to inventing an answer — which is the one thing this product exists not to do.
  if (!Object.prototype.hasOwnProperty.call(COMMANDS, name)) return null;
  return { kind: "slash", suggest: `/${name}${rest.length > 1 ? " " + rest.slice(1).join(" ") : ""}` };
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
  routing: "which model Estelle would pick for a task, and why",
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


/**
 * The `verify` verdict, as THREE states rather than two (#88).
 *
 * 🔴 E-015'S DEFECT IN A THIRD CONSUMER. `/verify` answers `grounded: false` for BOTH "I found invented
 * APIs" and "I could not check at all". E-015 found that join and fixed it in the Python hook AND the Node
 * hook — `cmdVerify` was never touched, so two of three consumers were fixed and the defect survived its
 * own errata entry.
 *
 * Measured on prod for a file with nothing invented in it: `{grounded: false, scope_ask: true,
 * ungrounded: [], question: "Which repo should I check this against? …", candidates: [...]}`. The CLI
 * printed **"Ungrounded references (not defined in your repo):" and then nothing** — a heading for an
 * empty list — and discarded a complete, well-written question the server had already composed. The
 * customer is told their file references undefined APIs when the truth is "I need to know which repo".
 *
 * Pure, and separated from the command, so the three states can be asserted without a network.
 */
function verifyLines(v, c) {
  const res = v || {};
  const ungrounded = Array.isArray(res.ungrounded) ? res.ungrounded : [];
  if (res.grounded) {
    return [`  ${c.green("Grounded.")}${c.dim("  Every API this file references exists in your swept repo.")}`];
  }
  // A QUESTION IS NOT A FINDING. The server already wrote it, with candidates and a reason — rendering it
  // as a defect in the customer's file is a claim we have no basis for.
  if (res.scope_ask) {
    const lines = [`  ${c.amber("!")} ${c.bold("Which repo should I check this against?")}`];
    for (const r of (res.candidates || []).slice(0, 8)) lines.push(`    ${c.dim("·")} ${r}`);
    if (res.unverified_reason || res.reason) lines.push(`  ${c.dim(String(res.unverified_reason || res.reason))}`);
    lines.push(`  ${c.dim("Re-run with the repo, e.g. ")}${c.bold("estelle verify <file> --repo owner/name")}`);
    lines.push(`  ${c.dim("Nothing was verified — this is a question, not a verdict on your file.")}`);
    return lines;
  }
  // 🔴 EVERY FINDING BUCKET, not just `ungrounded`. The original code read ONE of the eight the server
  // returns, so an arity error, a type error, a fabricated THIRD-PARTY call, a missing required argument
  // and a style violation ALL rendered as the heading "Ungrounded references:" followed by an empty list.
  // Measured on prod: a file calling `requests.fetch_all_pages` comes back with `third_party` populated
  // and `ungrounded` EMPTY — so the one case `verify` is most often pointed at was invisible.
  // Reading one field and treating its emptiness as "nothing found" is the same shape as the bug above it.
  const BUCKETS = [
    ["ungrounded", "Not defined in your repo"],
    ["third_party", "Called on a third-party library that does not define it"],
    ["arity_errors", "Wrong number of arguments"],
    ["type_errors", "Type mismatch"],
    ["missing_required", "Missing a required argument"],
    ["incomplete_work", "Left incomplete"],
    ["style_violations", "Against this repo's conventions"],
  ];
  const found = BUCKETS.map(([k, label]) => [label, Array.isArray(res[k]) ? res[k] : []])
                       .filter(([, items]) => items.length);
  if (found.length) {
    const lines = [];
    for (const [label, items] of found) {
      lines.push(`  ${c.amber(label + ":")}`);
      for (const u of items) {
        lines.push(`    ${c.red("✗")} ${typeof u === "string" ? u : (u.symbol || u.name || JSON.stringify(u))}`);
      }
    }
    return lines;
  }
  // THE THIRD STATE, which had no name in the old code: not grounded, nothing flagged, no question. That
  // is "I could not check", and saying anything else points defect class 3 at the customer's file.
  const why = String(res.unverified_reason || res.reason || "Estelle could not ground this file");
  return [`  ${c.amber("!")} ${c.bold("Could not verify.")}${c.dim("  " + why)}`,
          `  ${c.dim("Nothing was flagged in your file — this is a limit on OUR side, not a finding about yours.")}`];
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

/**
 * WHICH REPO AM I IN — the REPL's answer, which must be the SAME answer every other path gives.
 *
 * `repo-name.js` is the one definition (E-015 extracted it so the sweep and both hooks could stop
 * disagreeing about the namespace). The REPL header was a FOURTH consumer that never adopted it: it used
 * `path.basename(process.cwd())`, so on the founder's machine the session displayed `estelle` while every
 * path that WRITES memory used `uqeu/estelle`. A name that disagrees with the one memory was written under
 * is the same defect as no name at all — it addresses a namespace nothing filled.
 *
 * This is a one-line delegation ON PURPOSE. A second derivation "just for the header" is exactly how the
 * first split happened, and a wrapper that only forwards is the cheapest possible guarantee that it cannot
 * drift again.
 */
function resolveRepo(root) {
  return repoName.repoNameFor(root || process.cwd());
}

/**
 * The `repo` header line — the name, and WHETHER THE SERVER ACTUALLY HAS IT.
 *
 * 🔴 THE DEFECT THIS EXISTS TO FIX (#23, live in the customer path on 0.1.10). The old line was
 * `repo <basename(cwd)> · <memories> memories`, which glued together **two facts from two different
 * places** and implied a third that was false:
 *   * the NAME came from the local directory and had never touched the server;
 *   * the COUNT came from `GET /overview`, which is `namespace_stats` — the WHOLE ACCOUNT, every
 *     namespace aggregated (`api_console.py:370`);
 *   * so the line asserted "this repo holds 16,991 memories" when the account held 16,991 memories and
 *     that repo held NOTHING — `GET /repos` -> ["isoproof-bravo"], `repo_files: 0`.
 *
 * `indexed` is deliberately THREE-VALUED — true / false / **null for "could not ask"**. A failed `/repos`
 * fetch must never render as "not indexed": that is a claim about the server manufactured from a failure
 * to reach it, which is precisely the #76 defect one surface over. A failure to ASK is not evidence.
 */
function repoStatusLine(status) {
  const s = status || {};
  const repo = String(s.repo || "").trim();
  if (!repo) return { indexed: null, value: "no repo detected in this directory" };
  // `filed === null` means the list could not be read. Distinct from `[]`, which means "asked, and this
  // account has filed nothing" — a real and different answer.
  if (!Array.isArray(s.filed)) return { indexed: null, value: `${repo} · index state unknown` };
  // `/repos` stores both `owner/name` and bare names (a repo swept with an explicit `--repo` lands bare),
  // so membership compares the tail too. A naive `includes` would call a correctly-swept repo unindexed
  // and nag the customer to re-run a sweep they already ran — the exact regression the header's own
  // history warns about.
  const tail = (n) => String(n).split("/").pop().toLowerCase();
  const isFiled = s.filed.some((f) => String(f).toLowerCase() === repo.toLowerCase() || tail(f) === tail(repo));
  return isFiled
    ? { indexed: true, value: `${repo} · indexed` }
    : { indexed: false, value: `${repo} · not indexed — run \`estelle sweep\` to index it` };
}

/**
 * What the ACCOUNT holds — deliberately its OWN line, and labelled as the account's.
 *
 * 🔴 THIS SPLIT IS THE FIX. Both numbers come from `GET /overview` -> `namespace_stats(account_key, email)`
 * (`api_console.py:370`), which aggregates EVERY namespace. Rendering them beside a repo name produced
 * `repo estelle · 16991 memories` on an account whose `repo_files` was **0** — a sentence a customer can
 * only read as "this repo holds 16,991 memories", which was false in both halves at once.
 *
 * An earlier fix split `files` from `memories` because reporting memories under the label "files" was a
 * lie about WHICH NUMBER. It was right and it did not go far enough: the numbers were still attached to
 * the wrong SCOPE. Two true numbers under a false heading is still a false line.
 */
function memoryStatusLine(status) {
  const s = status || {};
  const files = Number(s.files || 0), memories = Number(s.memories || 0);
  if (!files && !memories) return "empty — nothing indexed yet";
  return [memories ? `${commas(memories)} memories` : "", `${commas(files)} code files indexed`]
    .filter(Boolean).join(" · ") + " (this account, all repos)";
}
const commas = (n) => String(Math.max(0, Math.floor(Number(n) || 0))).replace(/\B(?=(\d{3})+(?!\d))/g, ",");

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
  // The name AND whether the server actually HAS it. `repoStatusLine` owns that judgement (and the
  // three-valued indexed/unknown split) so the same answer can be unit-tested without a terminal.
  out.push(["repo", repoStatusLine(s).value]);
  // The account's holdings, on their OWN line and labelled as the account's — never folded into the repo
  // line, where they read as a claim about that repo. See memoryStatusLine.
  out.push(["memory", memoryStatusLine(s)]);
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
  // #23: the header used `cwd` — `path.basename(process.cwd())` — while `deps.repo` sat in the SAME deps
  // object holding `repoNameFor(process.cwd())`, the name the sweep and both hooks actually write under
  // (estelle.js:1466, whose own comment says "what /status calls the repo is exactly what your memory
  // calls it"). The right answer was already in the room and the header read the wrong field, so the
  // session displayed `estelle` while every writing path used `uqeu/estelle`.
  const repo = deps.repo || resolveRepo(deps.root || deps.cwdPath);
  // The account's ceiling, known BEFORE anything is drawn — see the note on `state` below. Hoisted here
  // because the intro line and the header both need it, and both are printed before the loop starts.
  const dialAtStart = status && typeof status.dial === "string" && local.modeRank(status.dial) >= 0
    ? status.dial : null;
  const header = statusLines({ ...(status || {}), repo, directory: deps.cwdPath || "" });
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
  // The same rule as the footer: do not advertise shift+tab to an account that has nowhere to cycle to.
  // Three surfaces used to promise this key — the footer, `/mode`'s description and this line — and on a
  // clamped account all three were wrong at once.
  const canCycle = modeUi.cycleModes(dialAtStart === null ? "" : dialAtStart).length > 1;
  out(`  ${c.dim("ask anything · /help for commands · "
      + (canCycle ? "shift+tab cycles the mode · " : "") + "ctrl-d to leave")}`);
  out("");

  // The autonomy ceiling, fetched LAZILY. Only /mode, /status and the write path read it, and a fourth
  // blocking GET on every session start would cost every user latency for a value most turns never touch.
  // `null` means "not asked yet"; `""` means "asked and could not get an answer" — a distinction the
  // fail-closed rendering depends on.
  // `transcript` is the session Estelle OWNS. It is not a log to replay: every turn is assembled from it, and
  // a failed attempt that falls out of the verbatim window leaves its LESSON behind rather than its wreckage.
  // 🔴 NEVER DRAW A RUNG WE ARE ABOUT TO TAKE AWAY. This used to start at `propose` with the dial unread,
  // so a clamped account launched showing `edit`, and the first shift+tab silently dropped it to `plan`.
  // The founder read that as the mode being unreliable — and he was right to: we advertised a privilege
  // and withdrew it one keypress later. The dial now arrives WITH the header (sessionStatus fetches it
  // alongside /account and /overview), so the first thing drawn is already the true one. When the fetch
  // fails, `serverMode` stays null and the label carries a `?` — "we have not been able to check" is a
  // different and more honest statement than a rung we are guessing at.
  const state = { mode: dialAtStart || "read_only", serverMode: dialAtStart,
                  transcript: curate.emptyTranscript() };
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
    // ALWAYS the display name, never the raw rung. The unknown-dial branch used to print `state.mode`
    // straight through, so a session that could not reach the dial showed `read_only ›` — the snake_case
    // enum the rename exists to get off the customer's screen, on the one line they look at most.
    //
    // The `?` marker is now CORRECT rather than alarming: the dial is fetched with the header, so a null
    // serverMode means "we asked and could not tell", which is exactly what `?` says. It used to mean
    // "we have not needed to ask yet", which is why this branch avoided it.
    return `${c.dim(modeUi.promptLabel(state.mode, state.serverMode || ""))} ${c.teal("›")} `;
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
    // `HELP_ONLY`, NOT `local.HELP_ONLY`. It is a constant in THIS file (:112), used correctly as a bare
    // identifier fifty lines below — the `local.` prefix was copied from neighbouring code that legitimately
    // uses it, and `undefined.has(n)` crashed the whole REPL on the first `/` a customer ever pressed.
    // Shipped in 0.1.10 past 348 green tests, because this closure — the seam between the pure menu module
    // and the keypress handler — was the one thing neither side's tests called.
    COMMANDS, new Set(Object.keys(COMMANDS).filter((n) => !HELP_ONLY.has(n))), skillRows);
  const unbindMenu = (deps.bindMenu || (() => () => {}))(menuFor);
  const leave = (code) => { unbind(); unbindMenu(); return code; };

  for (;;) {
    let line = await prompt(promptFor());
    if (line === null) { out(""); return leave(0); }      // ctrl-d

    // `!cmd` runs on YOUR machine — no server, and deliberately no autonomy ceiling. The ceiling governs
    // what ESTELLE may do unsupervised; a command the human just typed IS the trusted trigger the whole
    // contract is defined against (autonomy.py invariant I1), so gating it there would be a category
    // error. What does apply is the danger heuristic, imported from the hook rather than re-written.
    const bang = local.parseBang(line);
    if (bang) { await local.runShell(bang.command, { out, c, prompt }); out(""); continue; }

    const input = parseInput(line);
    if (!input.text && input.kind === "ask") continue;     // a bare Enter is not a question

    // A COMMAND TYPED AT ESTELLE IS NOT A QUESTION. Offer the right form and let Enter take it — never
    // run it silently, because "I thought you meant X" acting on its own is how a tool loses trust it
    // cannot buy back. Declining costs one keypress; the alternative cost a model call and a non-answer.
    if (input.kind === "ask") {
      const hint = commandHint(input.text);
      if (hint) {
        const take = await prompt(`  ${c.dim("did you mean")} ${c.teal(hint.suggest)}${c.dim("?  enter to run, or type your question")} `);
        if (take === null) { out(""); return leave(0); }
        if (!String(take).trim()) { line = hint.suggest; }
        else { line = String(take); }
        const redone = parseInput(line);
        if (redone.kind === "ask" && !redone.text) continue;
        Object.assign(input, redone);
      }
    }

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

    const route = routeInput(input, ctx, { repo });
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
function routeInput(input, ctx, opts) {
  // 🔴 THE SCOPE TRAVELS WITH THE QUESTION (#23, reproduced live in the customer path on 0.1.10).
  //
  // This used to send `{question}` and nothing else, so the server had no signal and resolved to the BASE
  // namespace — and the session then answered from UNFILED memory while the header, four lines above on
  // the same screen, displayed a repo name. Measured on the founder's account: the header said
  // "repo estelle · 16991 memories"; the answer cited `omega_11`/`omega_9`/`omega_4` and said "no
  // repository has been filed under a name yet". Two paths resolving scope differently, with nothing
  // responsible for making them agree — defect class 1, and the fifth instance in one session.
  //
  // The repo comes from `resolveRepo`, which is the SAME `repo-name.js` definition the sweep and both
  // hooks write with. Empty means "we could not tell", and is OMITTED rather than sent as a guess: a wrong
  // scope signal is worse than none, because the server would honour it.
  const repo = String((opts && opts.repo) || "").trim();
  const scoped = (body) => (repo ? { ...body, repo } : body);
  if (input.kind === "ask") return { path: "/deep-search", body: scoped({ question: input.text }) };
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
    // #84 — THE HEADER ADVERTISED THIS ON EVERY LAUNCH AND IT WAS NOT A COMMAND. `routing auto · 1
    // provider · /routing to see picks` printed for every customer, and typing it fell through to the
    // MCP `tools/call` default, which answered "unknown tool". The first thing a customer reads promised
    // something that errors — the standing rule's INVERSE: a door with no capability.
    // `POST /route` already answers exactly this question (api_account.py:9 — "which model + reasoning
    // effort Estelle would route a task to"), so the capability existed and only the door was missing.
    // `/route` is accepted too, because our own copy uses both spellings.
    case "routing":
    case "route":     return { path: "/route", body: arg ? { prompt: arg } : { task_kind: "chat" } };
    case "verify":    return { path: "/verify", body: { answer: arg } };
    case "init":      return { path: "/wiki", body: null, method: "GET" };
    case "sessions":  return { path: "/sessions", body: null, method: "GET" };
    case "resume":    return { path: "/session", body: null, method: "GET", query: { id: arg } };
    case "memory":    return { path: "/deep-search", body: scoped({ question: "what do you know about this repo?" }) };
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
  // WHICH REPOS THE SERVER ACTUALLY HAS. Without this the header can only guess, and it guessed wrong:
  // it paired a local directory name with the ACCOUNT-WIDE memory count and read as "this repo holds
  // 16,991 memories" for an account whose repo_files was 0. `null` on failure is load-bearing and is NOT
  // the same as `[]` — see repoStatusLine: unknown must never render as "not indexed".
  const repos = await get("/repos", key).catch(() => null);
  // The autonomy ceiling, fetched HERE rather than lazily on first use. It was lazy to avoid a blocking
  // GET at startup; the cost of that was a mode footer drawn from a guess and corrected a keypress later.
  const scope = await get("/autonomy/scope", key).catch(() => null);
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
    // `null` when /repos could not be read — a failure to ASK is not evidence that nothing is filed.
    filed: repos && Array.isArray(repos.repos) ? repos.repos : null,
    // The account's autonomy ceiling, so the first footer is drawn from the truth. Absent on failure.
    dial: scope && !scope.error && typeof scope.global === "string" ? scope.global : undefined,
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
  // THE ROUTING RECEIPT (#84). `POST /route` answers with a shape nothing here knew how to draw, so the
  // freshly-wired `/routing` opened onto BLANK LINES — the command was reachable and the customer saw
  // nothing, which is the same defect one layer over from the one being fixed. Caught by running it
  // through the customer door and reading the OUTPUT, not by re-reading the routing table.
  //
  // `routed: false` is a real answer, not an absence: it means the active provider has no tiers and the
  // account's own default model is used. Rendering that as silence would be the third version of the
  // same mistake.
  if (res.provider !== undefined && res.routed !== undefined) {
    const bits = [res.provider, res.model].filter(Boolean).join(" · ");
    lines.push(res.routed
      ? `  ${c.bold(res.model || "?")} ${c.dim("· " + [res.provider, res.tier, res.effort].filter(Boolean).join(" · "))}`
      : `  ${c.bold(bits || "your default model")} ${c.dim("· no tiers on this provider, so your default is used")}`);
    if (res.reason) lines.push(`  ${c.dim(String(res.reason))}`);
    return lines.join("\n");
  }
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
  resolveRepo, repoStatusLine, memoryStatusLine, commandHint, verifyLines,
  expandFileRefs, renderDiff, renderGate, mcpCall, mcpText, unknownTool,
  isSkillExit, runSkill,
  collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine, interruptAction, spinnerPlan,
  withSpinner, HISTORY_MAX,
  readline,
};
