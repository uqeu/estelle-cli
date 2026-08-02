"use strict";
// THE ALTERNATE SCREEN — the terminal boundary for 0.2.0. Brief §2.2 (`cli-redesign-brief.md:259-278`,
// the release nicknamed "§9"). It ships ALONE because it touches every render path and a regression has to
// stay bisectable.
//
// §2.2, observed: "the screen is borrowed, not owned." Estelle prints on top of the user's existing
// scrollback, so it reads as command output rather than as an application. Codex and Kimi both clear on
// launch and draw into a clean frame.
//
// THE SPLIT, and it is the whole design:
//
//   screen.js    WHICH ROWS ARE VISIBLE — pure, no I/O, tested as DATA.
//   altscreen.js WHEN THE ESCAPE CODES ARE EMITTED — this file, tested against DECLARED CONSTANTS.
//   repl.js      the conversation. Owns neither.
//
// 🔴 WHY THE CODES ARE DECLARED SEPARATELY FROM THE PAINTERS — the same reasoning as `palette.js`'s CODES,
// which learned it the hard way: with colour disabled every painter returns the bare string, so an OUTPUT
// comparison reports every role identical and the test goes green while a human sees two identical reds.
// The alt-screen trap is that shape one layer up — COMPARING WHAT WAS WRITTEN PROVES NOTHING ABOUT WHAT IS
// ON SCREEN. Naming a constant lets a test assert the CONTRACT; retyping "\x1b[?1049h" in the test means a
// typo in both places compares equal and passes.
//
// ⛔ THE FAILURE THIS FILE EXISTS TO PREVENT: exiting without leaving the alternate screen HAS EATEN THE
// USER'S SHELL. Their scrollback is gone, their prompt is gone, and the fix is `reset(1)` — which they have
// to know. It is the worst outcome available in this release and it is reachable from a crash, a Ctrl-C, a
// SIGTERM from a parent process, or an unhandled rejection three layers down. Hence `install()`.

// ── the declared codes ──────────────────────────────────────────────────────────
//
// 1049 is the private mode that switches buffer AND saves/restores the cursor in one sequence. 47 and 1047
// switch without restoring the cursor; 1048 only saves it. Using the wrong one is how a "working"
// implementation leaves the cursor in the wrong place on return.
const CODES = {
  enter: "\x1b[?1049h",
  leave: "\x1b[?1049l",
  hideCursor: "\x1b[?25l",
  showCursor: "\x1b[?25h",
  home: "\x1b[H",          // cursor to row 1, col 1 — a frame must start from a known position
  clearLine: "\x1b[2K",    // clear the whole row, so a short frame cannot leave a long one's tail behind
  clearBelow: "\x1b[J",    // clear from the cursor to the end of the screen
};

/**
 * May we use the alternate screen at all?
 *
 * A NON-TTY IS THE IMPORTANT CASE. `estelle | tee log`, CI, and every scripted test read this stream;
 * emitting escape codes into a pipe corrupts the output for every downstream reader, and there is no human
 * watching a frame anyway. Same reasoning as `mode-ui.js`'s key binder, which refuses to attach off-TTY.
 *
 * `TERM=dumb` means the terminal has told us it cannot do this — believe it rather than probing.
 * `ESTELLE_ALT_SCREEN=0` is the escape hatch, because a release that touches every render path needs one
 * that does not require a downgrade.
 */
function shouldUse(env, stream) {
  const e = env || {};
  if (String(e.ESTELLE_ALT_SCREEN || "") === "0") return false;
  if (String(e.TERM || "") === "dumb") return false;
  return Boolean(stream && stream.isTTY);
}

/**
 * A handle on the alternate screen for one stream.
 *
 * Every method is a NO-OP until `enter()` succeeds, so a caller does not have to branch on whether
 * alt-screen is active — the inert path is the same code path, which is what keeps the non-TTY behaviour
 * byte-identical to what shipped before this release.
 */
function create(stream) {
  const out = stream || process.stdout;
  let active = false;

  const write = (s) => { try { out.write(s); } catch (_) { /* a closed stream is not a crash */ } };

  return {
    get active() { return active; },

    /** Switch to the alternate buffer. IDEMPOTENT — nested enters leak a buffer the leave will not pop. */
    enter() {
      if (active) return false;
      active = true;
      write(CODES.enter + CODES.hideCursor + CODES.home + CODES.clearBelow);
      return true;
    },

    /**
     * Return to the user's screen. IDEMPOTENT, and SILENT when we never entered — an unmatched leave
     * prints garbage into a pipe, which is the thing `shouldUse` exists to prevent.
     *
     * ORDER IS LOAD-BEARING: show the cursor BEFORE switching buffers. Restoring it afterwards restores it
     * on the alternate screen, which looks fixed while the user's prompt stays invisible.
     */
    leave() {
      if (!active) return false;
      active = false;
      write(CODES.showCursor + CODES.leave);
      return true;
    },

    /** The viewport, with a floor — a zero-size terminal must not produce a zero-row viewport. */
    size() {
      return { rows: Math.max(1, Number(out.rows) || 24), columns: Math.max(20, Number(out.columns) || 80) };
    },

    /**
     * Paint a frame. `rows` is exactly what `screen.js` said is visible, already wrapped to width.
     *
     * Each row is cleared as it is written. Without that, a long line followed by a short one leaves the
     * long line's tail on screen — precisely the corruption §2.3 reported as "scrolling corrupts the view".
     */
    paint(rows) {
      if (!active) return false;
      const list = Array.isArray(rows) ? rows : [];
      let buf = CODES.home;
      for (const row of list) buf += CODES.clearLine + row + "\r\n";
      buf += CODES.clearBelow;
      write(buf);
      return true;
    },

    /**
     * 🔴 THE RESTORE GUARANTEE. Registers a handler on every path a terminal app can die on and returns an
     * uninstall.
     *
     * `exit` alone is not enough: it does not fire for SIGINT or SIGTERM, and an uncaught exception prints
     * a stack and exits without unwinding. Each of those leaves the user in the alternate screen.
     *
     * SIGINT RESTORES AND RE-RAISES. Swallowing Ctrl-C is its own bug — a terminal app that traps it and
     * keeps running is one a user cannot get out of. 130 is the conventional 128+SIGINT status.
     */
    install(proc) {
      const p = proc || process;
      const registered = [];
      const on = (ev, fn) => { p.on(ev, fn); registered.push([ev, fn]); };

      const onExit = () => { this.leave(); };
      const onSignal = (code) => () => { this.leave(); p.exit(code); };
      const onCrash = (err) => {
        // Leave FIRST, then report. A stack trace printed inside the alternate screen vanishes with the
        // buffer the moment anything restores it, and the user is left with neither the error nor a shell.
        this.leave();
        try { process.stderr.write(String((err && err.stack) || err) + "\n"); } catch (_) { /* ignore */ }
        p.exit(1);
      };

      on("exit", onExit);
      on("SIGINT", onSignal(130));
      on("SIGTERM", onSignal(143));
      on("SIGHUP", onSignal(129));
      on("uncaughtException", onCrash);
      on("unhandledRejection", onCrash);

      return () => {
        for (const [ev, fn] of registered) {
          try { p.removeListener(ev, fn); } catch (_) { /* ignore */ }
        }
        registered.length = 0;
      };
    },
  };
}

/**
 * Bind the scroll keys. Returns the unbind; a NO-OP on anything that is not a TTY.
 *
 * Modelled on `mode-ui.js`'s `keyBinder` deliberately — same shape, same non-TTY refusal, same reason: a
 * second owner of raw mode is how a session exits with a wedged tty, and readline already owns it when it
 * has a terminal. We only listen.
 *
 * `onScroll` receives -1/+1 for a line, -2/+2 for a page, or "top"/"bottom". The CALLER owns the viewport
 * arithmetic (screen.js does it); this file owns the keyboard and nothing else.
 */
function scrollBinder(stdin) {
  return function bind(onScroll) {
    if (!stdin || !stdin.isTTY) return () => {};
    require("readline").emitKeypressEvents(stdin);       // idempotent — node guards on a symbol
    const onKey = (_ch, key) => {
      if (!key || key.ctrl || key.meta) return;
      if (key.name === "pageup") return onScroll(-2);
      if (key.name === "pagedown") return onScroll(2);
      if (key.name === "up" && key.shift) return onScroll(-1);
      if (key.name === "down" && key.shift) return onScroll(1);
      // END returns to live. A reader who has scrolled up needs one keystroke back to the bottom, or the
      // session feels stuck — and "stuck" is what the founder reported about the mode key for the same
      // reason: an interaction with no visible way out.
      if (key.name === "end") return onScroll("bottom");
    };
    stdin.on("keypress", onKey);
    return () => { try { stdin.removeListener("keypress", onKey); } catch (_) { /* ignore */ } };
  };
}

module.exports = { CODES, shouldUse, create, scrollBinder };
