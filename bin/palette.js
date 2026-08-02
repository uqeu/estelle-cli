"use strict";
// THE PALETTE — brief §1.2, and the one rule it exists to enforce: **nothing hand-picks an escape code
// again.** They were scattered across `estelle.js` as bare `38;5;NN` strings, which is how a brand colour
// and an error colour drift into each other one file at a time.
//
// ESTELLE IS A LIGHT RED. Codex is blue, Claude is orange, Kimi is dark blue. Ours is the red equivalent of
// Codex's blue — **a brand colour, never a warning.** `teal` was a placeholder nobody chose.
//
// ⛔ THE CONSTRAINT THAT SHAPES EVERYTHING BELOW: errors, refusals and blocked-gate output must stay
// visually distinct from the brand. Two reds a shade apart is exactly the failure the brief warns about, so
// the separation is enforced **twice** and neither half is decorative:
//
//   1. DIFFERENT COLOURS — brand is light (210), failure is saturated (196) and BOLD.
//   2. 🔴 **COLOUR IS NEVER THE ONLY SIGNAL.** Every state carries a GLYPH — ✓ ok, ✗ fail, ! warn. A
//      colourblind reader, a `NO_COLOR` terminal, a CI log and a screenshot pasted into Slack all lose the
//      colour and keep the meaning. A gate verdict that is only distinguishable by hue is a verdict half
//      the readers cannot read, and this product's whole job is telling someone what is safe to merge.

// One switch. `NO_COLOR` is honoured (no-color.org) and a non-TTY never gets escapes, so piping to a file
// or a CI log produces clean text.
const ENABLED = Boolean(process.stdout.isTTY) && !process.env.NO_COLOR;

const wrap = (code) => (text) => (ENABLED ? `\x1b[${code}m${text}\x1b[0m` : String(text));

// THE CODES, declared once and separately from the painters. Separate because "is the brand distinct from
// the failure colour?" is a question about the PALETTE, not about the terminal it happens to be rendered
// on — with `NO_COLOR` set, every painter returns the bare string and comparing OUTPUT would report every
// role identical. Asserting on output is how a palette passes its own test in CI and ships two identical
// reds to a human.
const CODES = {
  brand: "38;5;210",     // light red — the wordmark, accents, "this is Estelle speaking"
  fail: "1;38;5;196",    // saturated + bold, and never used for anything that is not a failure
  ok: "38;5;35",
  warn: "38;5;179",
  grey: "38;5;244",
  dim: "2",
  bold: "1",
};

const brand = wrap(CODES.brand);
const fail = wrap(CODES.fail);
const ok = wrap(CODES.ok);
const warn = wrap(CODES.warn);
const grey = wrap(CODES.grey);
const dim = wrap(CODES.dim);
const bold = wrap(CODES.bold);

// Glyphs, so meaning survives the loss of colour. Deliberately not emoji (house rule) — these are
// single-width and align in a column, which emoji do not.
const GLYPH = { ok: "✓", fail: "✗", warn: "!", bullet: "·", arrow: "▸" };

/** A status line that reads correctly with the colour stripped. `kind` is ok | fail | warn. */
function status(kind, text) {
  const paint = kind === "fail" ? fail : kind === "warn" ? warn : ok;
  const glyph = GLYPH[kind] || GLYPH.ok;
  return `${paint(glyph)} ${text}`;
}

/** True when two ROLES are visually separable. Takes role NAMES and compares the declared codes, not the
 * rendered output — see the note on `CODES`: on a `NO_COLOR` terminal every painter returns the bare
 * string, so an output comparison would call every pair identical and pass while the palette was broken. */
function distinct(roleA, roleB) {
  return Boolean(CODES[roleA]) && Boolean(CODES[roleB]) && CODES[roleA] !== CODES[roleB];
}

module.exports = {
  ENABLED, GLYPH, CODES,
  brand, fail, ok, warn, grey, dim, bold,
  status, distinct,
  // The legacy role names the rest of the CLI already passes around as `c`, so adopting the palette is a
  // one-line change per call site rather than a rename sweep. `teal` is kept as an ALIAS ONTO BRAND — it
  // was never a chosen colour, and pointing it at the brand is what actually recolours the CLI.
  teal: brand, red: fail, green: ok, amber: warn,
};
