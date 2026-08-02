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
// 0.2.0 — the alternate screen. `screen.js` decides WHICH ROWS are visible (pure); `altscreen.js` owns
// WHEN the escape codes are emitted. This file owns neither, and both are inert on a non-TTY.
const altscreen = require("./altscreen.js");
const screenModel = require("./screen.js");
// Writing a returned diff to the working tree, and the shift+tab switch that says under which ceiling.
// Both are new primitives rather than new endpoints — the server has no say in what happens on your disk,
// which is exactly why the guards for it live client-side and fail closed.
const apply = require("./apply.js");
const modeUi = require("./mode-ui.js");
// THE CURATED TURN. Every other host makes Estelle a guest in someone else's prompt; here it OWNS the
// transcript, so it decides what each turn carries and what never gets carried again. See curate.js.
const curate = require("./curate.js");
// #93 — the two halves of "a slash command must never fall through to a model call": what counts as a
// KNOWN command (three-valued, so an unreadable registry never refuses), and what every routed reply
// actually LOOKS like, so no command can render a blank screen again.
const known = require("./known-commands.js");
const replies = require("./replies.js");
// MODULE 1 (CLI-MASTER-BRIEF §A2) — the append-only typed-entry log that owns everything drawn above
// the composer. NOTHING prints to stdout directly any more; this is what makes a scrolled-back session
// readable as a conversation. NOT the same object as `curate`'s transcript — see the note at the echo.
const transcript = require("./transcript.js");
// MODULE 2's first caller — the scope question every grounded surface returns and nothing could ask.
const scopeAsk = require("./scope-ask.js");
// BRACKETED PASTE — measured absent 2026-08-02 (`grep -rn 2004 cli/bin/` returned nothing), so a
// 20-line paste fired 20 turns. A Module 2 concern: it decides what a keystroke MEANS.
const pasteMode = require("./paste.js");
const ask = require("./ask.js");

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
  skills: "browse the playbooks Estelle picks from (it picks; you never have to)",
  tools: "every tool Estelle exposes over MCP",
  shell: "run a shell command right here — e.g. !git status",
  clear: "clear the screen",
  exit: "leave (ctrl-d does it too)",
};

// `shell` is documented but never DISPATCHED as a slash command — the `!` form is the real one, and a
// `/shell` that fell through to an MCP tools/call would be a confusing 'unknown tool'. It is answered
// LOCALLY in `handleLocal` with the `!` form, so typing it costs nothing and explains itself.
const HELP_ONLY = new Set(["shell"]);

// Spellings that resolve onto a command already in COMMANDS. Deliberately NOT menu rows: an alias is the
// same door under a second name, and listing both would imply two capabilities where there is one. They
// are declared here so the unknown-command guard cannot refuse a spelling the CLI's own copy uses.
const COMMAND_ALIASES = { route: "routing", quit: "exit" };

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
  // 🔴 #94 — "0 code files indexed" BESIDE "repo uqeu/estelle · indexed", FOUR LINES APART.
  //
  // MEASURED ON PROD 2026-08-02 against the founder's own account (`acct_1m3ad3xf2w`,
  // phlotu@gmail.com, identity asserted before any count):
  //
  //   GET /overview -> memory.repo_files = 0, memories = 16991, entities = 0
  //   GET /repos    -> ["isoproof-bravo", "uqeu/estelle"]
  //   POST /deep-search {repo:"uqeu/estelle"} -> THREE REAL FILE PATHS and a correct answer
  //       (src/estelle/serve/grounding_scope.py, tests/test_grounding_scope.py,
  //        src/estelle/serve/orchestra_scope.py)
  //
  // So the CLI was reporting both numbers FAITHFULLY, and the ANSWER PATH WORKS. `repo_files` is a
  // BROKEN COUNT, not a broken index — the code is there and is retrievable. The server-side count is
  // the World-1 lane's to fix; what is wrong HERE is printing a zero we can prove is false.
  //
  // ⛔ "0" IS A CLAIM, AND WE HOLD THE EVIDENCE AGAINST IT. `filed` comes from GET /repos in the same
  // breath: if the account has a filed repo, "0 code files" cannot be true, and the campaign's own rule
  // says a number we cannot trust reads as CANNOT-ANSWER rather than as fact. Saying "unavailable" is
  // the honest line; saying "0" is the third instance of two views disagreeing on one header.
  const filedAnything = Array.isArray(s.filed) && s.filed.length > 0;
  // 🔴 #94's REAL ANSWER, once the server could give one. `/overview` now returns `memory.by_repo` —
  // a durable per-repo `{repo, files, chunks}` — so the header can finally say something about THE REPO
  // YOU ARE IN instead of an account-wide total sitting under a repo name.
  //
  // Measured on the founder's account 2026-08-02: account-wide `repo_files` is 8,450, but the row for
  // `uqeu/estelle` is **1,993 files / 13,757 chunks**. Neither number is the other, and the one a
  // customer wants when the line above says `repo uqeu/estelle` is the SECOND. Reporting the account
  // total beside a repo name is the same lie #23 fixed one field over — two true numbers under a false
  // heading.
  const here = repoRow(s.byRepo, s.repo);
  if (here) {
    return `${commas(here.files)} code files in ${s.repo}`
      + (here.chunks ? ` · ${commas(here.chunks)} chunks` : "")
      + (memories ? ` · ${commas(memories)} memories across this account` : "");
  }
  const fileText = files ? `${commas(files)} code files indexed`
    : filedAnything ? "code file count unavailable"
    : "0 code files indexed";
  return [memories ? `${commas(memories)} memories` : "", fileText]
    .filter(Boolean).join(" · ") + " (this account, all repos)";
}

/** The `by_repo` row for the repo we are standing in, or null.
 *
 * Matched the same way `repoStatusLine` matches `filed`: `/repos` stores both `owner/name` and bare
 * names, so a tail comparison is what stops a correctly-swept repo reading as absent. Two matchers with
 * different ideas of "same repo" is exactly the defect class this header keeps producing. */
function repoRow(byRepo, repo) {
  const name = String(repo || "").trim();
  if (!name || !Array.isArray(byRepo)) return null;
  const tail = (n) => String(n).split("/").pop().toLowerCase();
  const hit = byRepo.find((r) => r && typeof r.repo === "string" && r.repo
    && (r.repo.toLowerCase() === name.toLowerCase() || tail(r.repo) === tail(name)));
  return hit && Number(hit.files) > 0 ? { files: Number(hit.files) || 0, chunks: Number(hit.chunks) || 0 } : null;
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
  const { post, get, prompt, c, cwd, now } = deps;

  // ── 0.2.0: OWN THE SCREEN, OR LEAVE IT EXACTLY AS IT WAS ──────────────────────
  //
  // Brief §2.2: "the screen is borrowed, not owned." Inside the alternate screen the terminal keeps NO
  // scrollback, so the application must keep its own — that is `screen.js`, and this is where the two meet.
  //
  // ⛔ THE INERT PATH IS BYTE-IDENTICAL TO WHAT SHIPPED BEFORE. On a non-TTY, with TERM=dumb, or with
  // ESTELLE_ALT_SCREEN=0, `alt` is null and `out` is the caller's own function unchanged. A release that
  // touches every render path must be able to be turned off without a downgrade, and piping the session
  // must never get escape codes spliced into it.
  const wantAlt = deps.altScreen !== undefined
    ? Boolean(deps.altScreen)
    : altscreen.shouldUse(process.env, process.stdout);
  const alt = wantAlt ? (deps.altScreenImpl || altscreen.create(process.stdout)) : null;
  let view = null, uninstallAlt = null;
  // Two rows are reserved at the bottom: one blank, one for the prompt readline draws into. The transcript
  // is painted above them, so readline never has to move the cursor over text it does not own — which is
  // the mechanism §2.3 blamed for "scrolling corrupts the view".
  const RESERVED_ROWS = 2;
  const viewport = () => Math.max(1, alt.size().rows - RESERVED_ROWS);
  // THE STATUS LINE — "transcript above (Module 1), composer below (Module 2), status line under that.
  // Nothing else writes. Ever." The spinner sets this; the render pass draws it. It is the last row of
  // the frame rather than an ad-hoc stdout write, which is what made it stack (symptom d, again).
  let statusLine = null, pendingLine = null;
  const repaint = () => {
    if (!alt || !view) return;
    const rows = screenModel.visible(view, viewport()).lines.slice();
    if (pendingLine) rows.push(pendingLine);
    if (statusLine) rows.push(`  ${c.dim(statusLine)}`);
    alt.paint(rows);
  };
  const setStatus = (text) => { statusLine = text || null; repaint(); };
  // THE QUEUE PREVIEW is another row the ONE render pass owns — under the status line, per the layout.
  // `estelle.js` hands us the setter rather than writing it itself, which is the whole rule.
  if (deps.onPendingRenderer) deps.onPendingRenderer((text) => { pendingLine = text || null; repaint(); });
  if (alt && alt.enter()) {
    view = screenModel.create({ width: alt.size().columns });
    uninstallAlt = alt.install(process);
    // A resize must RE-WRAP rather than leave clipped rows; screen.js owns that and this is the trigger.
    if (process.stdout.on) {
      process.stdout.on("resize", () => {
        if (!view) return;
        view = screenModel.reflow(view, alt.size().columns);
        repaint();
      });
    }
  }
  // THE SEAM. Every line the session prints goes through here, so the scrollback cannot miss one — an
  // `out` that bypassed the model would leave the transcript with holes only visible after scrolling.
  const out = view
    ? (line) => { view = screenModel.append(view, line === undefined ? "" : String(line), alt.size().columns); repaint(); }
    : deps.out;
  const releaseScreen = () => {
    if (uninstallAlt) { uninstallAlt(); uninstallAlt = null; }
    if (alt) alt.leave();
  };
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
    // 🔴 SYMPTOM (h) — THE KEY PROMPT ECHOED THE CREDENTIAL IN PLAINTEXT. Now it goes through Module 2,
    // which is the ONE place that knows a secret must be read masked. The old line reached for
    // `deps.promptSecret` and silently fell back to the ECHOING reader whenever it was absent — so the
    // masking depended on a caller remembering to pass it, which is exactly the "every prompt hand-rolls
    // its own printing" shape §A2 exists to end.
    // NOTE `readLine`, not `prompt`: the secret path must never reach a reader that writes history.
    const keyAsk = await ask.ask({ kind: "secret", label: "key" },
                                 { out, c, promptSecret: deps.promptSecret, readLine: deps.readLine || prompt });
    const pasted = keyAsk.ok ? keyAsk.value : null;
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
                  // TWO transcripts, deliberately. `transcript` is curate's — it feeds the MODEL and it
                  // EVICTS. `record` is the CUSTOMER's — append-only, never evicted, and it is what a
                  // scroll-back reads. Merging them would let the human record lose a turn because the
                  // model's window needed the space.
                  transcript: curate.emptyTranscript(), record: transcript.create() };
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
  // 🔴 SYMPTOM (d) — THE FOOTER PRINTED SEVEN TIMES on shift+tab. It shipped as fixed in 0.1.9 and the
  // in-place redraw in `mode-ui.js` is correct ARITHMETIC — one line erased, one written. The arithmetic
  // was never the problem.
  //
  // 0.2.0 made it an ad-hoc write RACING A REDRAW, which is the founder's own diagnosis of all eight
  // symptoms. Inside the alternate screen `screen.js` owns every row and `repaint()` redraws the whole
  // viewport from the transcript; `mode-ui` wrote its banner straight to stdout with cursor-relative
  // erases. Two things drawing on one screen with no agreement about who owns a row — so the banner
  // survived in whatever position the next repaint left it, seven presses deep.
  //
  // THE FIX IS MODULE 1'S RULE, NOT BETTER ARITHMETIC: nothing prints to stdout directly. On alt-screen
  // the mode change becomes a transcript NOTICE, so it is painted by the same pass as everything else —
  // and it lands in the scroll-back, which is where "what I chose" belongs anyway. `appendOnce` drops a
  // byte-identical repeat, so holding the key down cannot stack duplicates.
  const modeWrite = view
    ? (banner) => {
        const line = String(banner || "").replace(/\s+$/, "");
        if (!line.trim()) return;
        const before = state.record;
        state.record = transcript.appendOnce(state.record, transcript.notice(line.trim()));
        if (state.record !== before) for (const l of transcript.renderEntry(transcript.notice(line.trim()), c)) out(l);
      }
    : null;
  const unbind = (deps.bindKeys || (() => () => {}))(cycle, modeWrite);
  // THE SLASH MENU. The skill list is fetched ONCE here and cached for the session: the menu must be LOCAL
  // and instant, and a keystroke that blocks on a network call would be worse than no menu at all. A failed
  // fetch degrades to the built-in commands only — never to a hang, and never to an empty menu that looks
  // like the commands vanished.
  //
  // ⛔ AND IT IS ALSO THE UNKNOWN-COMMAND GUARD (#93). The same two fetches answer both questions — what
  // may the menu offer, and what may a `/` be refused for — because a menu built from one list and a guard
  // built from another is exactly the two-views-disagreeing defect this campaign keeps finding. `null` is
  // load-bearing in both: a registry we could not READ must never be read as a registry that is EMPTY.
  let skillRows = [];
  let skillNames = null;                       // Set once /skills answers; stays null if it could not be read
  const skillsReady = get("/skills", key).catch(() => null).then((listed) => {
    if (!listed || listed.error || !Array.isArray(listed.skills)) return;
    skillRows = listed.skills.map((s) => ({ name: s.name, short: s.short || s.summary || "" }));
    skillNames = new Set(listed.skills.map((s) => String(s.name || "")));
  }).catch(() => {});
  // The MCP tool list — LAZY AND MEMOISED, deliberately not fetched at startup.
  //
  // `routeInput`'s `default:` hands any unrecognised name to `tools/call`, so telling a real tool from a
  // typo needs the tool names. Fetching them eagerly would put a new round-trip on EVERY session start to
  // serve a question most sessions never ask — and #101 is an open latency item, so paying startup cost
  // for a rare path is the wrong trade. Fetched on the first slash command that is neither a built-in nor
  // a known skill, then held for the session: at most ONE registry read, ever, and zero from then on.
  // It is a registry read, never a model call, which is the property #93 actually requires.
  let toolNames = null;
  let toolsFetch = null;
  const toolsReady = () => {
    if (toolNames || toolsFetch) return toolsFetch || Promise.resolve();
    toolsFetch = post("/mcp", { jsonrpc: "2.0", id: 1, method: "tools/list", params: {} }, key)
      .catch(() => null).then((r) => {
        const tools = r && r.result && r.result.tools;
        if (Array.isArray(tools)) toolNames = new Set(tools.map((t) => String((t && t.name) || "")));
      }).catch(() => {});
    return toolsFetch;
  };
  const menuFor = () => slashMenu.menuRows(
    // `HELP_ONLY`, NOT `local.HELP_ONLY`. It is a constant in THIS file (:112), used correctly as a bare
    // identifier fifty lines below — the `local.` prefix was copied from neighbouring code that legitimately
    // uses it, and `undefined.has(n)` crashed the whole REPL on the first `/` a customer ever pressed.
    // Shipped in 0.1.10 past 348 green tests, because this closure — the seam between the pure menu module
    // and the keypress handler — was the one thing neither side's tests called.
    COMMANDS, new Set(Object.keys(COMMANDS).filter((n) => !HELP_ONLY.has(n))), skillRows);
  const unbindMenu = (deps.bindMenu || (() => () => {}))(menuFor);
  // SCROLLING, which only exists because we took the terminal's away (§2.2 -> §2.3). Inside the alternate
  // screen there is no native scrollback, so PageUp/PageDown and shift+arrows move `screen.js`'s viewport
  // and repaint. Bound ONLY when alt-screen is active: off-TTY there is nothing to scroll and nobody to
  // press a key, and binding raw-mode handlers there would corrupt a piped stream.
  const unbindScroll = view
    ? (deps.bindScroll || altscreen.scrollBinder(process.stdin))((delta) => {
        if (!view) return;
        const page = Math.max(1, viewport() - 1);
        view = delta === "top" ? screenModel.scroll(view, -1e9, viewport())
             : delta === "bottom" ? screenModel.toBottom(view)
             : screenModel.scroll(view, delta * (Math.abs(delta) === 1 ? 1 : page), viewport());
        repaint();
      })
    : () => {};
  // BRACKETED PASTE. Attached only on a TTY, and RELEASED with everything else — leaving `?2004h` set
  // would keep wrapping every paste in markers in the customer's shell after we exit, which is the 0.1.3
  // class: writing to something we do not own and not putting it back.
  const releasePaste = (deps.bindPaste || pasteMode.attach)(process.stdin, {
    write: (sequence) => { if (process.stdout.write) process.stdout.write(sequence); },
    insert: (text) => { if (deps.insert) deps.insert(text); },
  });
  // EVERY ORDINARY EXIT GOES THROUGH HERE — /exit, ctrl-d, a refused key. `alt.install()` covers the
  // violent paths (crash, SIGINT, SIGTERM); this covers the polite ones. Both are needed: install alone
  // would leave the screen borrowed on a clean `/exit`, which is the most common exit of all.
  const leave = (code) => { unbind(); unbindMenu(); unbindScroll(); releasePaste(); releaseScreen(); return code; };

  for (;;) {
    let line = await prompt(promptFor());
    if (line === null) { out(""); return leave(0); }      // ctrl-d

    // 🔴 SYMPTOM (a) — THE CUSTOMER'S OWN INPUT WAS NEVER IN THE TRANSCRIPT. Founder, 2026-08-02:
    // *"Scrolling back I see Estelle's answers and NOT MY QUESTIONS. I cannot tell what I asked. This is
    // the one that makes the whole CLI unreadable."*
    //
    // The cause is specific and was invisible from the code: readline echoes what you type onto the
    // PROMPT ROW, which alt-screen reserves at the bottom and `screen.js` never captures. So the line
    // appeared while typing and was gone the instant the view repainted — the record and the echo were
    // two different things, and only one of them survived.
    //
    // Echoed HERE, through `transcript.renderEntry`, which is the ONE renderer (Module 1). Note this is a
    // DIFFERENT transcript from `state.transcript`: that one is `curate`'s, it feeds the MODEL, and it
    // EVICTS. The customer's record must never lose a turn because the model's window needed the space.
    if (String(line).trim()) {
      state.record = transcript.append(state.record, transcript.user(String(line)));
      for (const l of transcript.renderEntry(transcript.user(String(line)), c)) out(l);
    }

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
      // An alias is resolved BEFORE anything else looks at the name, so exactly one spelling reaches the
      // dispatcher, the guard and the router. Resolving it in three places is how two of them drift.
      if (Object.prototype.hasOwnProperty.call(COMMAND_ALIASES, input.name)) {
        input.name = COMMAND_ALIASES[input.name];
      }
      const done = await handleLocal(input, { out, c, state, serverMode, key, deps, skillRows });
      if (done === "exit") return leave(0);
      if (done === "handled") continue;

      // 🔴 #93 — A LEADING SLASH THAT MATCHES NOTHING COSTS ZERO. Everything below this line sends a
      // request; an unknown command used to cost TWO (POST /skill/run, then POST /mcp) before the customer
      // was told the name means nothing. A `/` is an unambiguous statement of intent, so the answer is
      // local and instant.
      //
      // `/skills` was already in flight from session entry, so this waits on existing work rather than
      // adding any. The tool list is fetched ONLY if the skills list did not already claim the name —
      // one registry read on the first unrecognised command of a session, and none after that.
      await skillsReady;
      const commands = new Set([...Object.keys(COMMANDS), ...Object.keys(COMMAND_ALIASES)]);
      const reg = () => known.registry({ commands, skills: skillNames, tools: toolNames });
      let verdict = known.classify(input.name, reg());
      if (verdict.verdict === "unknown" || (verdict.verdict === "unverified" && !toolNames)) {
        await toolsReady();
        verdict = known.classify(input.name, reg());
      }
      if (verdict.verdict === "unknown") {
        for (const l of known.refusalLines(input.name, reg(), c)) out(l);
        out("");
        continue;
      }
      // "unverified" — we could not read a registry, so we have NO BASIS to refuse and we fall through,
      // saying so. Refusing here would tell a customer their working command does not exist because OUR
      // fetch failed, which is the #76 defect (a failure to ask rendered as an answer) on a new surface.
      if (verdict.verdict === "unverified") {
        out(`  ${c.dim(`(${verdict.why} — trying ${"/" + input.name} anyway)`)}`);
      }
      // A NAME WE KNOW IS A TOOL SKIPS THE SKILL PROBE ENTIRELY. The fall-through below tries every
      // unmatched command as a skill first (`POST /skill/run`, 404, then the tool call) because it had no
      // way to tell them apart. It does now, so a known tool costs ONE call instead of two — the same
      // round-trip #93 was about, on the path that actually works.
      state.knownTool = verdict.verdict === "tool";
    }

    // The one command that can produce a change. A KNOWN read_only stops it HERE rather than spending a
    // round-trip to be refused; an unknown dial does not, because the server is the enforcement point.
    if (input.kind === "command" && input.name === "work") {
      const why = local.workRefusal(state.mode, await serverMode());
      if (why) { out(`  ${c.amber("!")} ${why}`); out(""); continue; }
    }

    // COMMANDS THAT CANNOT MEAN ANYTHING WITHOUT AN ARGUMENT. `/orchestra` and `/work` used to post an
    // empty task and render the server's complaint as if it were an answer — the same shape as `/gate`
    // posting an empty `{}`. Refused here, locally, with the form that works.
    if (input.kind === "command" && !String(input.arg || "").trim()
        && (input.name === "orchestra" || input.name === "work")) {
      out(`  ${c.amber("!")} ${c.dim(`${"/" + input.name} needs a task — e.g. `)}`
        + `${c.teal(`/${input.name} add a --json flag to the sweep command`)}`);
      out("");
      continue;
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

    const route = routeInput(input, ctx, { repo, lastAsk: state.lastAsk });
    // A command that would fall through to MCP might be a SKILL — run it SERVER-SIDE first via /skill/run, so
    // the playbook is loaded + injected on the server and only the RESULT comes back. The playbook markdown
    // never reaches the client at all (the real IP lock). Not a skill → fall through to the tool call.
    if (route.mcp && input.kind === "command" && !state.knownTool) {
      const status = await runSkill(input.name.replace(/^skill[-_]/, ""), input.arg,
        { post, prompt, out, c, key, keys: deps.askKeys, state });
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
      // The subject `/routing` reports on. Recorded from the RAW line the customer typed, never the
      // assembled prompt — they asked what THEIR message costs, not what our curated turn costs.
      state.lastAsk = input.text;
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
    const send = (body) => withSpinner(
      input.kind === "ask" ? "thinking" : `running /${input.name}`,
      () => (route.method === "GET"
        ? get(route.query && route.query.id ? `${route.path}?id=${encodeURIComponent(route.query.id)}` : route.path, key)
        : post(route.path, body, key)
      ).catch((e) => ({ error: { message: String((e && e.message) || e) } })),
      // On alt-screen the spinner goes through the render pass; off it, the in-place write is correct.
      view ? { status: setStatus } : { write: deps.write });
    const raw = await send(route.body);
    let res = route.mcp && !(raw && raw.error && raw.error.message) ? mcpText(raw) : raw;

    // 🔴 SYMPTOM (b) — A QUESTION WITH NO INPUT MECHANISM. `/deep-search`, `/verify` and the gate all
    // return the same fail-closed envelope when several swept repos could answer: `{scope_ask, candidates,
    // question, reason}` — a complete question WITH its answer set. The session printed it as prose,
    // because nothing in the CLI could ask anything but free text. Now it is a real, selectable choice
    // (Module 2), the answer is RECORDED (Module 1), and the SAME request is retried with the scope.
    //
    // Fail-closed is preserved and is the whole point: cancelling leaves the refusal standing. It never
    // defaults to a repo, never takes the first candidate, and never retries unscoped — grounding against
    // the wrong codebase is precisely what this question exists to prevent.
    if (res && res.scope_ask && route.method !== "GET") {
      if (!scopeAsk.isResolvable(res)) {
        for (const l of scopeAsk.unresolvedLines(res, c)) out(l);
      } else {
        const asked = await scopeAsk.resolve(res, { out, c, prompt, keys: deps.askKeys }, state.record);
        state.record = asked.transcript;
        for (const l of transcript.renderEntry(transcript.lastOf(asked.transcript, "choice"), c)) out(l);
        if (asked.repo) {
          res = await send({ ...route.body, repo: asked.repo });
          if (res && res.scope_ask) {
            // The server still cannot resolve it. Say so once and STOP — re-asking the same question is
            // symptom (c), and a loop that keeps asking is worse than the prose it replaced.
            for (const l of scopeAsk.unresolvedLines(res, c)) out(l);
          }
        } else {
          state.record = transcript.append(state.record, scopeAsk.unresolvedEntry(res));
          for (const l of scopeAsk.unresolvedLines(res, c)) out(l);
        }
      }
    }
    // #89: an unlabelled default reads exactly like a verdict on the customer's last turn. Say which
    // question was answered whenever there was no message to route.
    if (route.defaultOnly) {
      out(`  ${c.dim("(no message yet — this is the default for a chat turn. Ask something, then /routing"
                     + " shows what YOUR message routed to.)")}`);
    }
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
    // The command NAME travels with the reply. Without it `renderAnswer` cannot reach the per-shape
    // renderers, and six commands render a blank screen — see the note on that function.
    out(renderAnswer(offerApply ? { ...res, diff: "" } : res, c,
                     { command: input.kind === "command" ? input.name : "", now: now && now() }));
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
  const { out, c, state, serverMode, key, deps } = ctx;   // ctx.skillRows: the session's cached /skills list
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

    case "skills": {
      // DECISION G's replacement for 246 menu rows: ONE browser, for someone who wants to look. The
      // point is that looking is OPTIONAL — Estelle picks the playbook by relevance, so this exists to
      // answer "what can it draw on?", never to make the customer choose.
      const rows = (ctx.skillRows || []).slice();
      if (!rows.length) {
        out(`  ${c.dim("the skill list has not loaded yet — try again in a moment.")}`);
        out("");
        return "handled";
      }
      const q = String(input.arg || "").trim().toLowerCase();
      const hits = q ? rows.filter((s) => `${s.name} ${s.short}`.toLowerCase().includes(q)) : rows;
      out(`  ${c.bold(`${hits.length} of ${rows.length} playbooks`)}`
        + c.dim(q ? ` matching "${q}"` : " — Estelle picks one by relevance; /skills <word> to filter"));
      const width = hits.reduce((w, s) => Math.max(w, s.name.length), 0);
      for (const s of hits.slice(0, 30)) out(`  ${c.teal(s.name.padEnd(width))}  ${c.dim(s.short)}`);
      if (hits.length > 30) out(`  ${c.dim(`… ${hits.length - 30} more — /skills <word> to narrow`)}`);
      out("");
      return "handled";
    }

    case "shell":
      // In /help as `!<cmd>`, never as `/shell` — but a customer who types the word they READ is not
      // wrong, and until now it fell through to an MCP tools/call for a tool that has never existed:
      // two round-trips to be told "unknown tool". Answered here, locally, for nothing.
      out(`  ${c.dim("a shell command runs with a leading ")}${c.teal("!")}${c.dim(" — e.g. ")}${c.teal("!git status")}`);
      out(`  ${c.dim("  it runs on YOUR machine; no server is involved.")}`);
      out("");
      return "handled";

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
    // 🔴 `tasks`, PLURAL, AND THE REPO — measured on prod 2026-08-02. This sent `{task: arg}` and the
    // server answered `400 swarm needs a non-empty 'tasks' list of strings` on EVERY call, so `/orchestra`
    // — a headline capability, ranked 7th in the slash menu — has never once worked from the session.
    // `api_orchestra.py:73-75` reads `body["tasks"]`; at PROPOSE or above it also requires a real
    // `owner/name` repo (`:87-88`) because each task opens a worktree. Both are sent; the repo is omitted
    // when we could not tell, never guessed, for the same reason `scoped()` omits it.
    case "orchestra": return { path: "/orchestra", body: scoped({ tasks: arg ? [arg] : [] }) };
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
    case "route": {
      // #89 (the display half). This used to send `{task_kind:"chat"}` whenever no argument was given —
      // which asks "what does a GENERIC chat turn route to", a different question from "what did MY
      // message route to", answered in the same words. The founder read `chat: default → balanced` after
      // typing "hi" and reasonably took it as a fact about his greeting. The product defect was real and
      // is fixed in routing.py; THIS was the display lying alongside it.
      const subject = arg || String((opts && opts.lastAsk) || "").trim();
      if (subject) return { path: "/route", body: { prompt: subject } };
      // Nothing to route yet. Answer the generic question, but FLAG it so the caller can say which
      // question it answered — an unlabelled default reads exactly like a verdict on your last turn.
      return { path: "/route", body: { task_kind: "chat" }, defaultOnly: true };
    }
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
    // #94: the durable per-repo rows, so the header can answer about THIS repo rather than the account.
    // Absent on an older server, which is why every consumer treats it as optional rather than required.
    byRepo: Array.isArray(mem.by_repo) ? mem.by_repo : null,
    // `entities_unknown` is the server saying "I could not count", which is a DIFFERENT fact from zero —
    // and rendering a could-not-count as `0 entities` is the same defect #94 was, one field over.
    entitiesUnknown: mem.entities_unknown === true,
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
  const body = { skill: name, task: String(arg || "") };
  let first = await post("/skill/run", body, key)
    .catch((e) => ({ error: { message: String((e && e.message) || e) } }));
  // 🔴 THE SCREENSHOT'S PATH. `/skill/run` answers a multi-repo account with `scope_ask_response`
  // (`skill_run.py:275`) — the question, the candidates, and no model call. `printSkillReply` rendered
  // `reply` as prose, the customer had no way to answer, the next turn re-sent without a repo, and the
  // identical block came back. That is symptoms (b) AND (c), from one missing input layer.
  //
  // ⛔ AND THE COST IS ASYMMETRIC HERE, which is why it is answered before the loop starts rather than
  // inside it: `scope_ask` means no playbook ran and no model was called, so re-asking is cheap and
  // guessing is catastrophic — an interactive skill grounded on the wrong repo would spend a whole
  // conversation being confidently wrong.
  if (first && first.scope_ask && scopeAsk.isResolvable(first) && deps.state) {
    const asked = await scopeAsk.resolve(first, { out, c, prompt, keys: deps.keys }, deps.state.record);
    deps.state.record = asked.transcript;
    for (const l of transcript.renderEntry(transcript.lastOf(asked.transcript, "choice"), c)) out(l);
    if (!asked.repo) {
      deps.state.record = transcript.append(deps.state.record, scopeAsk.unresolvedEntry(first));
      for (const l of scopeAsk.unresolvedLines(first, c)) out(l);
      return "ran";                       // fail-closed: the refusal stands, no playbook runs unscoped
    }
    body.repo = asked.repo;
    first = await post("/skill/run", body, key)
      .catch((e) => ({ error: { message: String((e && e.message) || e) } }));
  }
  if (first && first.scope_ask) {         // still unresolved — say it ONCE, never re-ask (symptom c)
    for (const l of scopeAsk.unresolvedLines(first, c)) out(l);
    return "ran";
  }
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
    const r = await post("/skill/run", { skill: name, messages, ...(body.repo ? { repo: body.repo } : {}) }, key)
      .catch((e) => ({ error: { message: String((e && e.message) || e) } }));
    if (r && r.error) { out(`\n  ${c.red("✗")} ${r.error.message}\n`); continue; }
    printSkillReply(name, r, false, out, c);
    messages = [...messages, { role: "assistant", content: (r && r.reply) || "" }];
  }
}

/**
 * Render one reply: the prose first, then the receipt. The certificate is the point, so it always shows.
 *
 * `opts.command` is the slash command that produced this reply, and it is what makes the per-shape
 * renderers in `replies.js` reachable. 🔴 IT IS ALSO WHAT CLOSES #93's SECOND HALF: six routed commands
 * (`/sessions`, `/resume`, `/init`, `/scan`, `/improve`, `/verify`) return no `answer` field, so this
 * function used to return the EMPTY STRING for them — a real round-trip, a spinner, and a blank screen.
 * Nothing below may return "" for a non-error reply; `replies.describe` is the floor.
 */
function renderAnswer(res, c, opts) {
  if (!res || res.error) {
    const msg = (res && res.error && (res.error.message || res.error)) || "no reply";
    return `  ${c.red("✗")} ${msg}`;
  }
  const command = String((opts && opts.command) || "");
  const lines = [];
  const answer = String(res.answer || "").trim();
  if (answer) lines.push(answer.split("\n").map((l) => `  ${l}`).join("\n"));

  // `/verify` IS E-030'S DEFECT IN A FOURTH CONSUMER. `verifyLines` already renders all three states and
  // all eight finding buckets for `estelle verify`; the session drew its own single-field version, so a
  // `scope_ask` came back as a blank screen and a `third_party` fabrication — the case verify exists for —
  // rendered as nothing at all. One renderer, both surfaces, so they cannot describe one envelope
  // differently. Measured on prod: `{grounded:false, scope_ask:true, ungrounded:[]}` printed zero lines.
  if (command === "verify") return [...lines, ...verifyLines(res, c)].join("\n");

  if (res.scope_ask) return lines.join("\n") || `  ${c.amber("!")} ${c.dim(String(res.question || res.unverified_reason || "Which repo should I check this against?"))}`;
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
  // THE SHAPES THIS FUNCTION NEVER KNEW HOW TO DRAW. Keyed by COMMAND, not sniffed from a field: two
  // endpoints returning `{count}` mean entirely different things, and guessing would render a session
  // list as a scan.
  if (!lines.length) {
    const own = replies.linesFor(command, res, c, (opts && opts.now) || Date.now());
    if (own && own.length) return own.join("\n");
  }
  // ⛔ THE FLOOR. A non-error reply may never render to nothing: that is indistinguishable on screen from
  // a request that never happened, and it is exactly how six commands shipped blank past a green suite.
  // Naming the fields the server sent turns the next unrendered shape into a bug report instead of a
  // silence — defect class 3, closed at the one place every reply passes through.
  if (!lines.length) return replies.describe(res, c).join("\n");
  return lines.join("\n");
}

module.exports = {
  // A GETTER, not a snapshot: the path depends on $HOME, and a value frozen at require-time would answer
  // for whatever HOME was when this module first loaded rather than the one in force at the call.
  get AUTH_FILE() { return auth.authFile(); },
  readAuth, writeAuth, storedKey,
  looksLikeKey, maskKey, humanDuration, relativeTime, parseInput, COMMANDS, COMMAND_ALIASES,
  known, replies,
  statusLines, welcomeBack, renderAnswer, runSession, routeInput, sessionStatus, handleLocal, HELP_ONLY,
  resolveRepo, repoStatusLine, memoryStatusLine, repoRow, commandHint, verifyLines,
  expandFileRefs, renderDiff, renderGate, mcpCall, mcpText, unknownTool,
  isSkillExit, runSkill,
  collapsePaste, expandPastes, frecencyScore, parseHistory, historyLine, interruptAction, spinnerPlan,
  withSpinner, HISTORY_MAX,
  readline,
};
