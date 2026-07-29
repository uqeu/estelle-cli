"use strict";
// Estelle AS A HOOK — the mechanism that makes "always on" true for a CUSTOMER, not just our dev machine.
//
// An MCP server cannot force the model to call it; the host decides. Hooks are the other direction — the
// HOST (Claude Code, Cursor) runs them on every matching action, no opt-in. `estelle init` writes a hooks
// block that calls `npx @fatelabs/estelle hook <mode>`, so the customer's own Claude Code fires Estelle on
// every edit and every command automatically. This file is that hook, in the CLI's own language (a customer
// has the npm package, never our Python).
//
//   ground  (PreToolUse  Write|Edit)  → grounding gate on the code about to be written
//   guard   (PreToolUse  Bash)         → warn on the classic destructive commands
//   sync    (PostToolUse Write|Edit)   → reindex the changed file so memory stays current
//
// Advisory, never blocking (a false-positive hard-block is its own damage), and fail-loud when Estelle is
// unreachable rather than passing silently — a quiet "OK" would be the false certification this product exists
// to prevent. Pure decisions are separated from I/O so they are unit-testable without a server or a terminal.

const fs = require("fs");
const os = require("os");
const path = require("path");

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

/** Turn a /verify report into the finding line a human reads, or "" when the code is clean. */
function groundFindings(report) {
  if (!report || report.error) return "";
  return GROUND_FIELDS
    .filter(([field]) => (report[field] || []).length)
    .map(([field, label]) => `${label}: ${(report[field] || []).slice(0, 5).join(", ")}`)
    .join(" · ");
}

/** One PreToolUse hook envelope: a line for the human, and the finding fed back to the model. */
function hookMessage(message, context) {
  const out = { systemMessage: message };
  if (context) out.hookSpecificOutput = { hookEventName: "PreToolUse", additionalContext: context };
  return out;
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
    ],
  };
}

/** True if a hook group is one of Estelle's (its command runs `… hook <mode>`). */
function isEstelleHook(group) {
  return (group.hooks || []).some((h) => / hook (ground|guard|sync)\b/.test(String(h.command || "")));
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
  const { file, code } = editedFile(payload);
  if (mode === "ground") {
    if (!file.endsWith(".py") || !code.trim()) return 0;
    const report = await post("/verify", { answer: code }).catch(() => null);
    if (report === null) { out({ systemMessage: `⚠ Estelle unreachable — ${path.basename(file)} was NOT grounded.` }); return 0; }
    const findings = groundFindings(report);
    if (findings) {
      out(hookMessage(`⛔ Estelle gate flagged ${path.basename(file)} — ${findings}`,
        `Estelle's grounding gate flagged ${file}: ${findings}. Confirm each symbol exists before relying on it.`));
    }
    return 0;
  }
  if (mode === "sync") {
    if (!/\.(py|md|ts|js|tsx|jsx|go|rs)$/.test(file)) return 0;
    let text;
    try { text = fs.readFileSync(file, "utf8"); } catch { return 0; }
    const rel = path.relative(process.cwd(), path.resolve(file));
    if (rel.startsWith("..")) return 0;                        // never feed a file outside the repo into memory
    await post("/reindex", { files: [{ path: rel, content: text }] }).catch(() => {});   // best-effort
    return 0;
  }
  return 0;
}

module.exports = {
  dangerousCommand, editedFile, groundFindings, hookMessage,
  claudeHookConfig, mergeHooks, removeHooks, isEstelleHook, runHook,
  claudeSettingsPath: () => path.join(os.homedir(), ".claude", "settings.json"),
};
