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
  const name = local.modeName;                      // display only — the value underneath is the rung
  if (local.modeRank(server) < 0) return `${name(localMode)}?`;
  return localMode === effective ? name(localMode) : `${name(localMode)}→${name(effective)}`;
}

/** The one line printed when the mode changes: what you are in, what it permits, and how to move again.
 * Modelled on Codex's footer label (`Plan mode (shift+tab to cycle)`), with the clamp made explicit —
 * a user who believes the key just enabled auto-apply, and finds out when it opens a PR instead, has been
 * misled by us. */
function modeBanner(localMode, server, c) {
  const known = local.modeRank(server) >= 0;
  const effective = local.effectiveMode(localMode, server);
  const what = local.MODE_WHAT[effective] || "";
  // The NAME a human reads, not the ladder's enum. The founder designed these modes and could not tell
  // what `read_only` meant while looking at it; the canonical value is unchanged underneath.
  const head = `  ${c.bold(local.modeName(effective))} ${c.dim("· " + what)}`;
  const tail = c.dim("  (shift+tab to cycle)");
  if (!known) return `${head}  ${c.amber("· dial unverified — assuming read_only")}${tail}`;

  // 🔴 NEVER OFFER A CYCLE ON A ONE-RUNG LADDER. Observed by the founder on his own account: the ceiling
  // is `read_only`, so `cycleModes` returns a single rung and shift+tab cycles the mode TO ITSELF forever
  // while the footer still advertised "(shift+tab to cycle)". He spammed it believing the CLI was broken.
  // The code was behaving exactly as designed and told him nothing — defect class 3 (unknown rendered as
  // OK) in the UI layer. This file already reasoned carefully about not hiding rungs on an UNVERIFIED
  // dial; the case it never covered is a KNOWN dial with exactly one rung.
  //
  // So: say the ceiling, and say the REMEDY. A dead end with no exit named is worse than no hint at all.
  if (cycleModes(server).length <= 1) {
    return `${head}  ${c.dim(`· your account's ceiling — /mode explains it, `
      + `raise it at ${local.SETTINGS_URL} to unlock ${local.modeName("propose")}`)}`;
  }
  if (localMode !== effective) {
    return `${head}  ${c.amber(`· ${local.modeName(localMode)} clamped by your account's dial `
      + `(${local.modeName(server)})`)}${tail}`;
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
    // ONE FOOTER, REDRAWN IN PLACE. Observed on the shipped CLI: four shift+tabs left FOUR identical
    // footers stacked up the screen, because each press wiped only the half-drawn PROMPT line and then
    // appended a fresh banner. The interaction was ported from Codex; the RENDERING was not — Codex owns
    // a bottom pane and repaints it, we print and move on.
    //
    // `drawn` is what makes "replace" safe: the previous banner is only the line directly above when
    // nothing has been emitted since we drew it. Submitting a line is the event that ends that, so the
    // flag clears on `line` and the next press appends rather than eating a line of the customer's
    // transcript. Erasing the wrong line is a worse bug than the one being fixed.
    let drawn = false;
    const onLine = () => { drawn = false; };
    if (d.rl && typeof d.rl.on === "function") d.rl.on("line", onLine);
    const onKey = (_ch, key) => {
      if (!isCycleKey(key)) return;
      // Fire-and-forget with a catch: the cycle fetches the account's dial, and a network failure there
      // must cost you a mode change, never the session you were in the middle of.
      Promise.resolve()
        .then(cycle)
        .then((r) => {
          if (!r) return;
          write("\r\x1b[2K");                                  // wipe the half-drawn prompt line
          if (drawn) write("\x1b[1A\r\x1b[2K");                 // …and the footer we drew last time
          write(String(r.banner || "") + "\n");
          drawn = true;
          if (d.rl) { d.rl.setPrompt(String(r.prompt || "")); d.rl.prompt(true); }
        })
        .catch(() => { write("\r\x1b[2K"); });
    };
    stdin.on("keypress", onKey);
    return () => {
      stdin.removeListener("keypress", onKey);
      if (d.rl && typeof d.rl.removeListener === "function") d.rl.removeListener("line", onLine);
    };
  };
}

module.exports = { isCycleKey, cycleModes, nextMode, promptLabel, modeBanner, keyBinder };
