"use strict";
// Estelle AS A HOOK — the mechanism that makes "always on" true for a CUSTOMER, not just our dev machine.
//
// An MCP server cannot force the model to call it; the host decides. Hooks are the other direction — the
// HOST (Claude Code, Cursor) runs them on every matching action, no opt-in. `estelle init` writes a hooks
// block that calls `npx @fatelabs/estelle hook <mode>`, so the customer's own Claude Code fires Estelle on
// every edit and every command automatically. This file is that hook, in the CLI's own language (a customer
// has the npm package, never our Python).
//
//   ground     (PreToolUse  Write|Edit) → grounding gate on the code about to be written
//   guard      (PreToolUse  Bash)       → warn on the classic destructive commands
//   sync       (PostToolUse Write|Edit) → reindex the changed file so memory stays current
//   distil     (PostToolUse Bash)       → replace a verbose tool result with a curated one BEFORE it enters
//              the window. The only lever that shrinks a Claude Code context: no hook can remove what is
//              already in there, but PostToolUse can decide what goes in. See distill.js.
//   checkpoint (Stop · PreCompact · SessionEnd) → store the session so it survives the window, the tool,
//              and the outage. This is the one that makes "pick up where you left off" true: it does not
//              ask the model to call anything, it reads the host's own transcript.
//
// Advisory, never blocking (a false-positive hard-block is its own damage), and fail-loud when Estelle is
// unreachable rather than passing silently — a quiet "OK" would be the false certification this product exists
// to prevent. Pure decisions are separated from I/O so they are unit-testable without a server or a terminal.
//
// TWO IMPLEMENTATIONS, ONE CONTRACT. scripts/hooks/estelle_hook.py is the same hook in Python (what the
// founder runs); this is what customers get from npm. They drifted in BOTH directions — this one shipped
// without the abstention check, that one without the error-envelope check, and NEITHER had any secret
// redaction on the always-on sync path. `groundVerdict` / `maySync` / `dangerousCommand` are now the named
// shared decisions, and tests/test_hook_contract.py runs both implementations over the same fixtures and
// fails on any divergence. Two modes are deliberately single-sided and documented as such: `checkpoint` is
// JS-only (the dev hook has no customer session to preserve) and the guard's ship-gap check is Python-only
// (it imports estelle.serve.ship_gap, which a customer's machine does not have).

const fs = require("fs");
const os = require("os");
const path = require("path");
const { hasSecret } = require("./secrets.js");
const distill = require("./distill.js");
const sessionGap = require("./session-gap.js");

// The classic shell foot-guns — kept identical in spirit to scripts/hooks/estelle_hook.py so the dev hook
// and the shipped hook agree. Conservative by design: it flags shapes that are almost always a mistake to
// run blind, and stays silent on ordinary work, because a guard that cries wolf gets muted within a day.
// Only a recursive-force rm whose TARGET is genuinely catastrophic — root, home, a wildcard at root, or a
// system directory. A plain `rm -rf /tmp/x` or `rm -rf ~/Downloads/build` is ordinary cleanup and must NOT
// fire, or the guard gets muted and then misses the real `rm -rf /`.
// Target must be truly catastrophic: root, home-root, a wildcard at root, a bare /Users or /home (the whole
// thing — a DEEP path under them like /Users/khai/proj/dist is normal work and must not fire), or a system
// dir where deleting anything is bad. /private and /tmp are excluded — that is where scratch lives.
const RM_DANGER = /\brm\s+(?:-\S+\s+)*(-[a-z]*r[a-z]*f[a-z]*|-[a-z]*f[a-z]*r[a-z]*)\s+(?:-\S+\s+)*(\/(\s|\*|$)|~\/?(\s|$)|\$HOME\/?(\s|$)|\/(Users|home)(\s|\*|$)|\/(etc|usr|var|bin|lib|sbin|boot|opt|root|dev|sys|proc|System|Library)(\/\S*)?(\s|$))/;
const DANGER = [
  [RM_DANGER, "recursive force-delete of a critical path"],
  [/:\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:/, "a fork bomb"],
  [/\bcurl\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b/, "piping a download straight into a shell"],
  [/\bwget\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b/, "piping a download straight into a shell"],
  [/\b(dd|mkfs\.\w+)\b.*\bof=\/dev\/(disk|sd|nvme)/, "writing directly to a disk device"],
  [/>\s*\/dev\/(disk|sd|nvme)\w*/, "overwriting a disk device"],
  [/\bgit\s+push\b.*--force\b.*\b(origin\s+)?(main|master)\b/, "a force-push to the main branch"],
  [/\bchmod\s+-R\s+777\s+\//, "making a broad path world-writable"],
];

/** The reason a command looks dangerous, or "" when it doesn't. Pure, conservative, errs toward silence. */
function dangerousCommand(command) {
  const text = String(command || "");
  for (const [pattern, reason] of DANGER) if (pattern.test(text)) return reason;
  return "";
}

/** `(file, code)` from a Write/Edit hook payload — content on a Write, new_string on an Edit. */
function editedFile(payload) {
  const t = (payload && payload.tool_input) || {};
  return { file: String(t.file_path || ""), code: String(t.content || t.new_string || "") };
}

const GROUND_FIELDS = [["ungrounded", "not defined in this repo"], ["arity_errors", "signature mismatch"],
                       ["type_errors", "type error"], ["third_party", "invented library API"]];

/** Why the grounding call could not answer — "" when it DID answer (clean or not).
 *
 * The distinction `groundFindings` cannot make: it returns "" for both "verified, nothing wrong" and
 * "there is no verdict here at all", and the caller reads "" as a pass. `unwrap` in estelle.js resolves
 * EVERY failure — a refused key, a 402, a 500, a dead socket — into `{error:{…}}`, so the only failure the
 * hook used to notice (`report === null`) could not happen in production and the gate went quietly green on
 * all of them. A gate that could not run must say so; a silent pass is a false clean. */
function groundFailure(report) {
  if (report === null || report === undefined) return "unreachable";
  const e = report.error;
  if (!isError(e)) return "";
  // findingText, not String: JSON's spelling, so a bare `true` is not "true" here and "True" in Python.
  const why = typeof e === "object" ? String(e.message || e.detail || "refused") : findingText(e);
  return `could not verify (${why})`;
}

/** Whether an `error` field means the gate could not answer. Kept identical to the Python hook's `_is_error`.
 *
 * The two hooks both wrote this as a bare truthiness test, and that is the one place Python and JS disagree:
 * `{}` and `[]` are truthy here and FALSY there, so `{"error": {}}` read "unreachable" to this hook and
 * "clean" to the Python one — a fail-OPEN in the always-on path. Named so the shared rule has one home in
 * each language instead of being re-derived from `!e`. */
function isError(e) {
  if (e !== null && typeof e === "object") return true;  // {} / [] are still an error envelope
  return Boolean(e);
}

/** A `/verify` finding field as the list of strings to show. Kept identical to `_finding_values`.
 *
 * Neither hook validated this untrusted server field: `{"ungrounded": "abc"}` made THIS one throw
 * (`String.prototype.join` does not exist) and `{"ungrounded": 5}` made it answer clean. A scalar is one
 * finding — never a crash, never a pass. */
function findingValues(value) {
  if (value === null || value === undefined || value === false || value === "" || value === 0) return [];
  if (Array.isArray(value)) return value.map(findingText);
  return [findingText(value)];
}

/** One finding rendered for a human, in JSON's spelling. Kept identical to the Python hook's `_finding_text`.
 *
 * These values came off the wire as JSON, so JSON is the only spelling the two hooks can agree on: bare
 * `String`/`str` renders null as "null" here and "None" there, and a nested object as "[object Object]" here
 * and "{'a': 1}" there. Both were real divergences on the same server response. Plain strings pass through
 * unquoted — a finding is almost always a symbol name, and quoting every one to fix a malformed-payload edge
 * would make the normal message worse. */
function findingText(value) {
  if (typeof value === "string") return value;
  try {
    const json = JSON.stringify(value);
    return json === undefined ? String(value) : json;   // undefined has no JSON form
  } catch { return String(value); }
}

/** Turn a /verify report into the finding line a human reads, or "" when the code is clean.
 *
 * Still "" for an error envelope — but that answer now means "ask groundFailure first", and the caller
 * does. Kept split so "clean" and "could not run" stay two different questions with two different answers. */
function groundFindings(report) {
  if (!report || isError(report.error)) return "";
  return GROUND_FIELDS
    .map(([field, label]) => [label, findingValues(report[field])])
    .filter(([, values]) => values.length)
    .map(([label, values]) => `${label}: ${values.slice(0, 5).join(", ")}`)
    .join(" · ");
}

/** THE shared grounding decision, in one fail-closed order: could the gate run at all → did it ABSTAIN →
 * did it find something → clean.
 *
 * `{kind, detail}` where kind is "unreachable" | "unverified" | "flagged" | "clean". The middle one is the
 * case this implementation was missing: an ABSTENTION is not a pass. When the gate cannot certify (a grounding
 * surface too thin to cover the repo) every finding list comes back EMPTY, which reads exactly like "clean" —
 * so the edit would proceed ungrounded with the hook silent. That is the same fail-open the surface-coverage
 * guard exists to close, and a hook must not reopen it downstream of the fix.
 *
 * Contract-tested against scripts/hooks/estelle_hook.py's `ground_verdict` on the same fixtures. */
function groundVerdict(report) {
  const failure = groundFailure(report);
  if (failure) return { kind: "unreachable", detail: failure };
  const unverified = String((report && report.unverified_reason) || "");
  if (unverified) return { kind: "unverified", detail: unverified };
  const findings = groundFindings(report);
  if (findings) return { kind: "flagged", detail: findings };
  return { kind: "clean", detail: "" };
}

// What may be reindexed. The extension allowlist keeps binaries and lockfiles out of a code graph; the secret
// check keeps a live-looking credential off the wire. Kept identical to the Python hook's `may_sync`.
// The leading `[^/\\]` is not decoration: without it this matched a bare dotfile named `.py`, while the
// Python hook's `PurePath(".py").suffix` is `""` and refused it. The two always-on hooks disagreed about
// whether to UPLOAD a file — the one direction a drift here is allowed to matter — and the customer-facing
// side was the permissive one. A dotfile has no extension; both hooks now say so.
const SYNC_EXT = /[^/\\]\.(py|md|ts|js|tsx|jsx|go|rs)$/;

/** "" when this file may be reindexed, else the reason it may not.
 *
 * The redaction here is the gap this closed: `estelle sweep` has refused to upload a file embedding a
 * live-looking key since it shipped, and the hook — the path that fires on EVERY edit whether anyone opted in
 * or not — had no such check at all. The always-on path was the one without it. */
function maySync(file, content) {
  if (!SYNC_EXT.test(String(file || ""))) return "not an indexable file type";
  if (hasSecret(content)) return "contains something shaped like a live credential";
  return "";
}

// A checkpoint is a NETWORK WRITE of the customer's conversation, so what it carries is a security
// decision, not a formatting one. Bounded so a twelve-hour session cannot post an unbounded body — the
// server dedupes by content hash, so re-sending the same tail every turn costs nothing to store, but the
// REQUEST still has to stay sane. The cap keeps the tail: recent turns are what a resume actually needs.
const CHECKPOINT_MAX_MESSAGES = 400;
const CHECKPOINT_MAX_CHARS = 4000;

/** The text one transcript content-block contributes to the checkpoint, or "" when it must not travel.
 *
 * Kept: `text` (the conversation itself — and where the brief distiller finds decisions and open threads)
 * and a short marker for `tool_use`, because "what was I doing" is half of continuity.
 * Dropped: `tool_result` — raw command output, which routinely contains env dumps, tokens and customer
 * data; and `thinking` — the model's private reasoning, which is not a decision record. Neither belongs
 * on the wire. */
function blockText(block) {
  if (!block || typeof block !== "object") return "";
  if (block.type === "text") return String(block.text || "");
  if (block.type === "tool_use") return `[tool: ${String(block.name || "?")}]`;
  return "";
}

/** The conversation `[{role, content}]` inside a Claude Code transcript (JSONL), ready to checkpoint.
 *
 * The host writes this file itself and hands every hook its path, which is what makes always-on
 * checkpointing possible WITHOUT the model choosing to cooperate. Never throws: a hook that dies on a
 * malformed line would take the user's turn with it. */
function transcriptMessages(text) {
  const out = [];
  for (const line of String(text || "").split("\n")) {
    if (!line.trim()) continue;
    let record;
    try { record = JSON.parse(line); } catch { continue; }        // a partial/corrupt line is skipped, not fatal
    if (!record || (record.type !== "user" && record.type !== "assistant")) continue;
    if (record.isSidechain) continue;                             // a subagent is a DIFFERENT conversation
    const message = record.message || {};
    const raw = message.content;
    const content = (Array.isArray(raw) ? raw.map(blockText).filter(Boolean).join("\n")
                                        : String(raw || "")).trim();
    if (!content) continue;
    out.push({ role: String(message.role || record.type), content: content.slice(0, CHECKPOINT_MAX_CHARS) });
  }
  return out.slice(-CHECKPOINT_MAX_MESSAGES);
}

/** The client facts a resume needs, read from the newest transcript record that carries each one.
 *
 * "Pick up where you left off" is a lie without these: a brief that cannot say WHICH repo and WHICH
 * branch the work was on makes the next agent re-derive it, which is exactly the re-explaining the
 * feature exists to remove. Claude Code stamps every record with `cwd`, `gitBranch`, `version` and the
 * `model`, and Estelle threw all of it away.
 *
 * NEWEST wins — a session that switched branch mid-run must resume on the branch it ended on. A fact
 * that is absent is OMITTED rather than defaulted: a guessed branch is worse than no branch. */
function transcriptContext(text) {
  const ctx = {};
  const put = (key, value) => { if (value !== undefined && value !== null && value !== "") ctx[key] = String(value); };
  for (const line of String(text || "").split("\n")) {
    if (!line.trim()) continue;
    let record;
    try { record = JSON.parse(line); } catch { continue; }
    if (!record || record.isSidechain) continue;
    put("cwd", record.cwd);
    put("branch", record.gitBranch);
    put("client_version", record.version);
    put("entrypoint", record.entrypoint);
    put("effort", record.effort);
    put("model", (record.message || {}).model);
  }
  if (ctx.cwd) ctx.repo = ctx.cwd.split("/").filter(Boolean).pop() || ctx.cwd;
  return ctx;
}

/** One PreToolUse hook envelope: a line for the human, and the finding fed back to the model. */
function hookMessage(message, context) {
  const out = { systemMessage: message };
  if (context) out.hookSpecificOutput = { hookEventName: "PreToolUse", additionalContext: context };
  return out;
}

// The tools whose input names a file the customer WROTE. Reads are deliberately absent: "code you touched
// has moved" is about what you were changing, and every session reads a hundred files it does not care about.
const WRITE_TOOLS = /^(Write|Edit|MultiEdit|NotebookEdit)$/;

/** The files this session actually wrote, most-recently-touched FIRST, deduped and bounded.
 *
 * Read from the host's own transcript — the same source the checkpoint uses, and the reason the whole feature
 * needs no cooperation from the model. Order is load-bearing: `movedFiles` preserves it, so the file the
 * customer was in when they stopped is the first one the welcome names.
 *
 * Never throws: a malformed transcript line costs a filename, never the session. */
function transcriptFiles(text, limit) {
  const cap = limit || sessionGap.MAX_TRACKED_FILES;
  const written = [];
  for (const line of String(text || "").split("\n")) {
    if (!line.trim()) continue;
    let record;
    try { record = JSON.parse(line); } catch { continue; }
    if (!record || record.isSidechain || record.type !== "assistant") continue;
    const blocks = (record.message || {}).content;
    if (!Array.isArray(blocks)) continue;
    for (const block of blocks) {
      if (!block || block.type !== "tool_use" || !WRITE_TOOLS.test(String(block.name || ""))) continue;
      const file = String((block.input || {}).file_path || (block.input || {}).notebook_path || "");
      if (file) written.push(file);
    }
  }
  // Reverse BEFORE deduping, so a file edited early and again at the end keeps its LATEST position. Getting
  // this backwards is subtle and it matters: the file the customer was in when they stopped is the one the
  // welcome names first, and a file they opened once at the start would otherwise outrank it.
  const seen = [];
  for (const file of written.reverse()) if (!seen.includes(file)) seen.push(file);
  return seen.slice(0, cap);
}

/** The SessionStart envelope. `additionalContext` is what the MODEL is told, `systemMessage` is what the
 * HUMAN sees — the same two channels the grounding gate uses, for the same reason: the customer should not
 * have to read the model's context to know what Estelle noticed. */
function welcomeMessage(text) {
  return { systemMessage: text,
           hookSpecificOutput: { hookEventName: "SessionStart", additionalContext: text } };
}

/** The hooks block `estelle init` writes into a Claude Code settings.json so Estelle fires with no opt-in. */
function claudeHookConfig(runner) {
  const cmd = (mode, timeout, async) => ({
    type: "command", command: `${runner} ${mode}`, timeout, ...(async ? { async: true } : {}),
    statusMessage: `Estelle ${mode}`,
  });
  return {
    PreToolUse: [
      { matcher: "Write|Edit", hooks: [cmd("hook ground", 15)] },
      { matcher: "Bash", hooks: [cmd("hook guard", 10)] },
    ],
    PostToolUse: [
      { matcher: "Write|Edit", hooks: [cmd("hook sync", 20, true)] },
      // NOT async, unlike sync. This one has to answer BEFORE the result reaches the model — that is the
      // whole point of it — so it cannot be fire-and-forget. Short timeout: a distiller that makes the user
      // wait has already lost more than the tokens it saves.
      { matcher: "Bash", hooks: [cmd("hook distil", 10)] },
    ],
    // The three moments that decide whether a session survives. Stop is every turn — that is the
    // "always on" guarantee. PreCompact is the single highest-value checkpoint there is: the last
    // instant before the window is thrown away. SessionEnd is the outage case — the tool dies and the
    // work has to still be there in another one.
    Stop: [{ hooks: [cmd("hook checkpoint", 30, true)] }],
    PreCompact: [{ hooks: [cmd("hook checkpoint", 30)] }],
    SessionEnd: [{ hooks: [cmd("hook checkpoint", 30)] }],
    // The other end of the same wire: what you are told when you come BACK. Short timeout and no network
    // call at all — this is the first thing that happens in a session, and a welcome that costs a customer
    // even a second has already taken more than it gives. It reads local state and runs `git log`, and any
    // failure at all is silence.
    SessionStart: [{ hooks: [cmd("hook welcome", 5)] }],
  };
}

/** True if a hook group is one of Estelle's (its command runs `… hook <mode>`). */
function isEstelleHook(group) {
  return (group.hooks || []).some(
    (h) => / hook (ground|guard|sync|distil|checkpoint|welcome)\b/.test(String(h.command || "")));
}

/** Merge Estelle's hooks into an existing settings object without clobbering the user's own hooks. */
function mergeHooks(existing, runner) {
  const base = existing && typeof existing === "object" ? existing : {};
  const hooks = { ...(base.hooks || {}) };
  const ours = claudeHookConfig(runner);
  for (const event of Object.keys(ours)) {
    const mine = (hooks[event] || []).filter((g) => !isEstelleHook(g));   // drop a prior Estelle block first
    hooks[event] = [...mine, ...ours[event]];
  }
  return { ...base, hooks };
}

/** Remove ONLY Estelle's hooks — the user's own hooks stay, empty event arrays are pruned. The clean
 * disable, so nobody has to hand-edit settings.json (and MCP stays untouched — a separate switch). */
function removeHooks(existing) {
  const base = existing && typeof existing === "object" ? existing : {};
  if (!base.hooks) return base;
  const hooks = {};
  for (const event of Object.keys(base.hooks)) {
    const kept = (base.hooks[event] || []).filter((g) => !isEstelleHook(g));
    if (kept.length) hooks[event] = kept;                    // prune the event entirely if nothing survives
  }
  const out = { ...base };
  if (Object.keys(hooks).length) out.hooks = hooks; else delete out.hooks;
  return out;
}

/** Run one hook mode. `post`/`get` are the API calls, `out` writes the JSON envelope. Never throws. */
async function runHook(mode, payload, deps) {
  const { post, out } = deps;
  if (mode === "guard") {
    const reason = dangerousCommand((payload.tool_input || {}).command);
    if (reason) {
      out(hookMessage(`⛔ Estelle: this command looks like ${reason} — read it again before running.`,
        `Estelle's Bash guard flagged the command as ${reason}. Confirm the target is intended; advisory, not a block.`));
    }
    return 0;
  }
  if (mode === "distil") {
    // PostToolUse on Bash: replace a verbose result with a curated one so the wreckage never enters the
    // window. `distil` returns null for everything it is not certain about, and null means "say nothing",
    // which the host reads as "keep the original" — the failure mode is verbosity, never a lost result.
    const result = distill.distil(payload);
    if (!result) return 0;
    const spillPath = distill.spill(result.original);
    out(distill.replacement(`${result.text}\n\n${distill.receipt(result, spillPath)}`));
    return 0;
  }
  const { file, code } = editedFile(payload);
  if (mode === "ground") {
    if (!file.endsWith(".py") || !code.trim()) return 0;
    const report = await post("/verify", { answer: code }).catch(() => null);
    // Every way this can fail to produce a verdict, in ONE fail-closed order. It used to test
    // `report === null` only, which the real `post` never returns — so a refused or erroring /verify printed
    // nothing and the edit proceeded as if grounded; and an ABSTENTION read as clean for the same reason.
    const { kind, detail } = groundVerdict(report);
    if (kind === "unreachable") {
      out({ systemMessage: `⚠ Estelle ${detail} — ${path.basename(file)} was NOT grounded.` });
    } else if (kind === "unverified") {
      out(hookMessage(`⚠ Estelle CANNOT verify ${path.basename(file)} — ${detail}`,
        `Estelle's grounding gate ABSTAINED on this edit to ${file}: ${detail}. This is NOT a pass - no symbol `
        + "in this edit was checked. Do not treat any API used here as confirmed to exist."));
    } else if (kind === "flagged") {
      out(hookMessage(`⛔ Estelle gate flagged ${path.basename(file)} — ${detail}`,
        `Estelle's grounding gate flagged ${file}: ${detail}. Confirm each symbol exists before relying on it.`));
    }
    return 0;
  }
  if (mode === "checkpoint") {
    // Silent by design, unlike `ground`. The gate must SHOUT when it cannot verify, because a quiet "OK"
    // would be a false certification. A checkpoint that cannot run certifies nothing — and a warning on
    // every single turn is how a user learns to ignore Estelle entirely.
    const sessionId = String(payload.session_id || payload.sessionId || "");
    const transcript = String(payload.transcript_path || payload.transcriptPath || "");
    if (!sessionId || !transcript) return 0;
    let raw;
    try { raw = fs.readFileSync(transcript, "utf8"); } catch { return 0; }
    const messages = transcriptMessages(raw);
    if (!messages.length) return 0;
    // `event` is WHY this fired. A PreCompact checkpoint is the pre-wall snapshot, a SessionEnd one is the
    // outage snapshot, a Stop one is routine — a resume that cannot tell them apart cannot rank them.
    // NOTE what is deliberately absent: account_id and team_id. The server resolves those from the API
    // key. A client that ASSERTS its own identity is the hole, not the feature.
    const client = { name: "claude-code", event: String(payload.hook_event_name || ""),
                     ...transcriptContext(raw) };
    if (!client.cwd && payload.cwd) client.cwd = String(payload.cwd);
    // Record WHERE THIS SESSION STOPPED before the network call, because the next session's welcome depends
    // on it and a checkpoint POST that fails must not also cost the customer their gap. Local, bounded, and
    // silent on failure: the files this session wrote, the commit it was at, and the moment it ended.
    sessionGap.recordSession(client.cwd, transcriptFiles(raw), new Date().toISOString());
    await post("/checkpoint", { session_id: sessionId, messages, client }).catch(() => {});
    return 0;
  }
  if (mode === "welcome") {
    // The returning-customer brief. Silent by design in every failure mode — no state, a short gap, no git,
    // a rewritten history: all of them produce nothing. The ONE thing it must never do is speak when it
    // cannot tell whether it should, because that is the sentence that would be made up.
    const cwd = String(payload.cwd || process.cwd() || "");
    let out_;
    try { out_ = sessionGap.welcome(cwd, new Date().toISOString()); } catch { return 0; }
    if (out_ && out_.show && out_.text) out(welcomeMessage(out_.text));
    return 0;
  }
  if (mode === "sync") {
    if (!SYNC_EXT.test(file)) return 0;
    let text;
    try { text = fs.readFileSync(file, "utf8"); } catch { return 0; }
    const refusal = maySync(file, text);
    if (refusal) {
      // A refusal is SAID, not swallowed. A file silently left out of the index is a hole in the grounding
      // surface, and a hole nobody was told about is the fail-open this whole product exists to close.
      out({ systemMessage: `⚠ Estelle did not index ${path.basename(file)} — ${refusal}.` });
      return 0;
    }
    const rel = path.relative(process.cwd(), path.resolve(file));
    if (rel.startsWith("..")) return 0;                        // never feed a file outside the repo into memory
    await post("/reindex", { files: [{ path: rel, content: text }] }).catch(() => {});   // best-effort
    return 0;
  }
  return 0;
}

module.exports = {
  dangerousCommand, editedFile, groundFindings, groundFailure, groundVerdict, maySync, SYNC_EXT, hookMessage,
  claudeHookConfig, mergeHooks, removeHooks, isEstelleHook, runHook,
  transcriptMessages, transcriptContext, transcriptFiles, welcomeMessage, WRITE_TOOLS,
  blockText, CHECKPOINT_MAX_MESSAGES, CHECKPOINT_MAX_CHARS,
  claudeSettingsPath: () => path.join(os.homedir(), ".claude", "settings.json"),
};
