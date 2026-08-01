"use strict";
// THE SWITCH YOU CAN SEE — shift+tab to cycle the ceiling, and a prompt that always says which one you
// are in. Codex and Claude Code both give you this; Estelle's session had a `/mode` command and nothing
// else, so the ceiling was invisible unless you asked for it.
//
// COPIED FROM CODEX: the INTERACTION only. Codex binds `KeyCode::BackTab` in
// `codex-rs/tui/src/chatwidget/interaction.rs` to `cycle_collaboration_mode()` and renders
// `"Plan mode (shift+tab to cycle)"` in `codex-rs/tui/src/bottom_pane/footer.rs`. Their modes are a PROMPT
// policy (a `developer_instructions` block swapped into the turn); the thing that actually gates execution
// is a SEPARATE axis (`AskForApproval` + `SandboxPolicy`) that shift+tab does not touch.
//
// NOT COPIED: the permission semantics. Estelle's rungs ARE the permission, they live server-side in
// `serve/autonomy.py` (per-account, fail-closed, consented, audited), and this key can only ever LOWER
// what happens — see the long note in session-commands.js. So two rules shape the cycle:
//
//   * it walks only the rungs the ACCOUNT can reach. Offering `execute` to a `propose` account would be a
//     switch wired to nothing, which is the exact complaint this work answers;
//   * the prompt shows the EFFECTIVE mode — min(local, server) — because that is the only value that is
//     true of the next command. A clamp is shown as a clamp, never hidden.

const local = require("./session-commands.js");

/** SHIFT+TAB, and nothing else.
 *
 * VERIFIED against a real pty, not assumed: a terminal sends back-tab as `ESC [ Z`, and Node's keypress
 * decoder turns that into `{name:"tab", shift:true}` — the same event crossterm reports to Codex as
 * `KeyCode::BackTab`. It is also the only tab form that is FREE to bind: readline inserts a literal `\t`
 * into the line buffer for a plain Tab, so binding that would eat the user's input. ctrl/meta are excluded
 * so a terminal that decorates the sequence differently falls through to readline rather than firing. */
function isCycleKey(key) {
  return !!(key && key.name === "tab" && key.shift && !key.ctrl && !key.meta);
}

/** The rungs this account can actually reach, lowest first.
 *
 * A KNOWN dial stops the list there. An UNKNOWN one offers all four — fail-closed governs what we DO, not
 * what we let someone select, and the effective mode is still recomputed as min(local, server) at the
 * moment of action. Hiding rungs on an unverified dial would tell a paying customer their account is
 * read_only when the truth is that one GET failed. */
function cycleModes(server) {
  const top = local.modeRank(server);
  return top < 0 ? local.MODES.slice() : local.MODES.slice(0, top + 1);
}

/** The next reachable rung, wrapping. A current value that is not IN the set (a `/mode execute` typed on a
 * propose account, or a corrupt one) restarts at the bottom rather than being carried around the ceiling. */
function nextMode(current, server) {
  const modes = cycleModes(server);
  const at = modes.indexOf(String(current || ""));
  return at < 0 ? modes[0] : modes[(at + 1) % modes.length];
}

/** The mode as it appears IN the prompt — short, because it is on every line.
 *
 *   `propose`           local and effective agree;
 *   `execute→propose`   the account clamps it, and the arrow says so on every single line;
 *   `propose?`          the dial has not been verified, so this is a hope, not a grant. */
function promptLabel(localMode, server) {
  const effective = local.effectiveMode(localMode, server);
  if (local.modeRank(server) < 0) return `${localMode}?`;
  return localMode === effective ? localMode : `${localMode}→${effective}`;
}

/** The one line printed when the mode changes: what you are in, what it permits, and how to move again.
 * Modelled on Codex's footer label (`Plan mode (shift+tab to cycle)`), with the clamp made explicit —
 * a user who believes the key just enabled auto-apply, and finds out when it opens a PR instead, has been
 * misled by us. */
function modeBanner(localMode, server, c) {
  const known = local.modeRank(server) >= 0;
  const effective = local.effectiveMode(localMode, server);
  const what = local.MODE_WHAT[effective] || "";
  const head = `  ${c.bold(effective)} ${c.dim("· " + what)}`;
  const tail = c.dim("  (shift+tab to cycle)");
  if (!known) return `${head}  ${c.amber("· dial unverified — assuming read_only")}${tail}`;
  if (localMode !== effective) {
    return `${head}  ${c.amber(`· ${localMode} clamped by your account's dial (${server})`)}${tail}`;
  }
  return head + tail;
}

/** Bind shift+tab on a real terminal. Returns the unbind; a NO-OP on anything else.
 *
 * `cycle` is async and returns `{banner, prompt}` — the caller owns the mode, this file owns the keyboard.
 *
 * NON-TTY IS THE IMPORTANT CASE: piped stdin (`printf … | estelle`), CI, and every scripted test deliver
 * no keypress events at all, and raw-mode handling there would corrupt the output stream for no benefit.
 * So we never even attach. Note we also never call `setRawMode` — readline already owns that when it has a
 * terminal, and a second owner is how a session exits with a wedged tty. */
function keyBinder(stdin, deps) {
  const d = deps || {};
  return function bind(cycle) {
    if (!stdin || !stdin.isTTY) return () => {};
    const rl = require("readline");
    ((d.readline || rl).emitKeypressEvents)(stdin);            // idempotent — node guards on a symbol
    const write = d.write || ((s) => process.stdout.write(s));
    const onKey = (_ch, key) => {
      if (!isCycleKey(key)) return;
      // Fire-and-forget with a catch: the cycle fetches the account's dial, and a network failure there
      // must cost you a mode change, never the session you were in the middle of.
      Promise.resolve()
        .then(cycle)
        .then((r) => {
          if (!r) return;
          write("\r\x1b[2K");                                  // wipe the half-drawn prompt line
          write(String(r.banner || "") + "\n");
          if (d.rl) { d.rl.setPrompt(String(r.prompt || "")); d.rl.prompt(true); }
        })
        .catch(() => { write("\r\x1b[2K"); });
    };
    stdin.on("keypress", onKey);
    return () => stdin.removeListener("keypress", onKey);
  };
}

module.exports = { isCycleKey, cycleModes, nextMode, promptLabel, modeBanner, keyBinder };
