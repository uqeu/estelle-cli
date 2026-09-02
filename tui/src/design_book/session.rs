//! The vocabulary a scripted session is written in — every word of it borrowed from the product.
//!
//! 🔴 **THIS FILE USED TO CARRY A SECOND RENDERER'S VOCABULARY AND THAT WAS THE DEFECT.** It had an
//! `Ink` enum mapping nine names onto `Palette`, a `Grid` that painted per-cell colour, and variants
//! for rules, marks, steps and bands — a complete parallel language for drawing a frame. The
//! founder ran the result and rejected it: no production rail, no boot, no user-turn band, cut off
//! at column 100. Every one of those was a consequence of having a second renderer at all.
//!
//! So the vocabulary is now **the product's own transcript entries**. A [`Say`] does not describe
//! how something looks; it names WHICH KIND OF TURN it is — a grounded answer, a tool receipt, a
//! system note, a refusal — and `crate::transcript` decides how that looks, once, for the live app
//! and the film together. Colour, spacing and the tint band are no longer this file's business, and
//! that is the point: they cannot drift from the product because they ARE the product.
//!
//! ⚠️ **WHAT WAS GIVEN UP, SAID OUT LOUD.** A tool receipt paints its lines in one semantic colour
//! (`transcript.rs:396`), so the per-cell colour the old `Grid` could express is gone — a red
//! refusal row inside a table is now the receipt's colour, not red. That is a real loss and it buys
//! something worth more: the film cannot render a table the live app would render differently.
//! Where colour carries the meaning, use [`Say::Failure`], which is the product's own refusal
//! banner and is painted `palette.red` by the one owner of that decision.
//!
//! ## The one thing this file still computes
//!
//! [`Say::Table`] lays a `|`-delimited script row out through [`crate::cols`] and flattens the
//! result to a string. **A tool receipt takes `Vec<String>`, so the alignment has to be computed
//! before it gets there** — and computing it here means a column that does not line up is a `cols`
//! test failure rather than a thing a reader notices in a screenshot six weeks later. A script may
//! never pad with spaces.

use crate::cols::{Cell, Col, row};
use crate::design_book::SCREENS;

/// Render one design-book screen and flatten it to plain rows a tool receipt can carry.
///
/// 🔴 **THE FIXTURE GATE IS THREADED, NOT ASSUMED.** The caller already chooses between a beat's
/// real reply and the honest shut-gate block, so this could have rendered with `fixtures: true`
/// unconditionally and been correct today. It takes the flag anyway: a second door into the
/// fixtures that happens to be locked by its only caller is one refactor away from being a leak,
/// and this repo has already paid for that shape once in this very feature.
///
/// ⚠️ **AN UNKNOWN NAME PANICS RATHER THAN RENDERING NOTHING.** A film that silently drew zero rows
/// for a mistyped screen is the vacuity shape: the session would play, the beat would be blank, and
/// the runtime would still be reported as a pass.
pub(crate) fn screen_lines(name: &str, fixtures: bool) -> Vec<String> {
    let screen = SCREENS
        .iter()
        .find(|screen| screen.name == name)
        .unwrap_or_else(|| panic!("a film names screen {name:?}, which the book does not have"));
    let palette = crate::theme::ScreenTheme::Dark.palette();
    crate::design_book::render(screen, &palette, 0, false, fixtures)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Lay one `|`-delimited script row out through [`crate::cols`] and flatten it.
///
/// ⚠️ A row that carries MORE cells than the grid has columns asserts; one that carries FEWER is
/// padded. Padding is the forgiving direction on purpose — a row the founder trimmed while
/// reordering is an edit to DATA, and turning that into a crash would make the script hostile to
/// the person it exists for. Too many cells is a different fact: that is a grid he meant to widen.
pub(crate) fn table_row(columns: &'static [Col], source: &'static str) -> String {
    let mut cells: Vec<&str> = source.split('|').map(str::trim).collect();
    assert!(
        cells.len() <= columns.len(),
        "script row {source:?} carries {} cells for {} columns",
        cells.len(),
        columns.len()
    );
    cells.resize(columns.len(), "");
    let cells = cells
        .into_iter()
        .map(|text| Cell(text, ratatui::style::Color::Reset))
        .collect::<Vec<_>>();
    row(columns, &cells, 0)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

// ── what a reply is made of ──────────────────────────────────────────────────────────────────

/// One unit of a reply. Each variant names a kind of turn the PRODUCT already has.
pub(crate) enum Say {
    /// Prose, streamed in word-sized chunks so it reads as generation rather than as a slide.
    /// `grounded` is the product's own field and drives the product's own badge.
    Answer { text: &'static str, grounded: bool },
    /// A tool receipt: a label and its output lines, arriving a line at a time.
    Tool {
        label: &'static str,
        lines: &'static [&'static str],
    },
    /// A tool receipt whose lines are an aligned table, declared as `|`-delimited data.
    Table {
        label: &'static str,
        columns: &'static [Col],
        rows: &'static [&'static str],
    },
    /// A system note.
    System(&'static str),
    /// The product's own three-line refusal banner, painted by the one owner of that colour.
    Failure([&'static str; 3]),
    /// A whole design-book screen, by name, flattened into a tool receipt.
    Screen(&'static str),
    /// Silence inside a reply, in milliseconds before `--speed`. Estelle taking its time.
    Wait(u32),
}

// ── what a person does at the keyboard ───────────────────────────────────────────────────────

/// One instruction to the hands.
///
/// 🔴 **THE STUMBLES ARE DATA, NOT A RATE.** The founder's note was *"they might make some spelling
/// mistakes here and there, they might rewind or backspace or spell something wrong — just think
/// how I type to you."* A sprinkled error probability produces UNIFORM imperfection, which reads as
/// a machine imitating a person; a person does not misspell every fourth word. So a film scripts
/// two or three specific stumbles at specific words and leaves everything else clean.
pub(crate) enum Key {
    /// Type this at the ordinary rate.
    Type(&'static str),
    /// Type this, notice it, and backspace every character back out.
    Oops(&'static str),
    /// A familiar phrase, typed fast. A burst next to a slow patch is what makes a rate lumpy.
    Burst(&'static str),
    /// Hands off the keyboard, thinking about the next word.
    Pause(u32),
}

// ── one beat ─────────────────────────────────────────────────────────────────────────────────

/// One exchange: what gets typed, how long Estelle takes, what comes back, and how long it sits.
///
/// ⚠️ **`read_ms` IS NOT PADDING AND IT IS THE FIELD MOST LIKELY TO BE TRIMMED BY SOMEONE TRYING TO
/// FIT MORE IN.** The founder asked for two and a half minutes *"so I can talk through it"*, and the
/// extra time is for READING and WAITING, not for more beats. A film that runs long loses a beat;
/// it does not lose its silence.
pub(crate) struct Beat {
    pub typed: &'static [Key],
    /// Silence between pressing enter and the first word of the reply.
    pub think_ms: u32,
    pub reply: &'static [Say],
    /// How long the finished reply sits before the next beat starts typing.
    pub read_ms: u32,
}

/// One film.
pub(crate) struct Film {
    /// `1`, `2`, `3` — what the founder types after `--session`.
    pub number: u8,
    /// The repo the session runs in. Drives the real header and the real ask rule.
    pub repo: &'static str,
    pub branch: &'static str,
    pub beats: &'static [Beat],
}

/// 🔴 **THE BOUNDS ARE NAMED CONSTANTS AND THEY ARE CHECKED BY A TEST, NOT BY THE PLAYER ALONE.**
/// Power of Ten #2: every loop has a fixed, stated bound. A scripted session that can hang is a
/// recording session that has to be restarted, and he has already had to Ctrl-C out of one attempt.
pub(crate) const MAX_BEATS: usize = 32;

/// The wall-clock ceiling for one film at `--speed 1`. Films are asserted to fit well inside it.
pub(crate) const MAX_FILM_MS: u32 = 6 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid pads a trimmed row and refuses an overfull one, and the alignment is `cols`'.
    #[test]
    fn a_table_row_is_padded_when_short_and_refused_when_overfull() {
        static COLUMNS: &[Col] = &[Col::l(6), Col::l(6), Col::l(6)];
        // `Col::l(6)` pads to six then adds the two-column gap, so `b` lands on column 8.
        let short = table_row(COLUMNS, "a|b");
        assert_eq!(short.find('b'), Some(8), "{short:?}");
        let full = table_row(COLUMNS, "a|b|c");
        assert!(full.ends_with('c'), "{full:?}");
        let overfull = std::panic::catch_unwind(|| table_row(COLUMNS, "a|b|c|d"));
        assert!(
            overfull.is_err(),
            "a four-cell row must not fit a three-column grid"
        );
    }

    /// Two rows of one table end on the same column — the property `cols` exists to guarantee and
    /// the reason a script may never pad with spaces.
    /// ⚠️ **THE FIRST VERSION OF THIS TEST SEARCHED FOR THE DIGIT `3` AND FOUND THE ONE IN
    /// `urllib3`.** A needle that also occurs in another column cannot measure a column boundary.
    /// Right-aligned cells end on the row's own last column, so the property is asserted as an
    /// EQUAL TOTAL WIDTH plus a right-aligned final cell — neither of which a substring can fake.
    #[test]
    fn every_row_of_a_table_is_the_same_width() {
        static COLUMNS: &[Col] = &[Col::l(24), Col::l(16), Col::r(9)];
        let a = table_row(COLUMNS, "claims/fetcher.py:88|urllib3 Retry|3");
        let b = table_row(COLUMNS, "claims/upstream.py:141|while loop|5");
        assert_eq!(a.chars().count(), b.chars().count());
        assert!(a.ends_with('3'), "{a:?}");
        assert!(b.ends_with('5'), "{b:?}");
    }

    /// 🔴 A screen the book does not have is a panic, not a blank beat.
    #[test]
    fn an_unknown_screen_name_panics_rather_than_drawing_nothing() {
        let missing = std::panic::catch_unwind(|| screen_lines("99-not-a-screen", true));
        assert!(missing.is_err());
        // The positive control: a real screen draws rows. Without it the check above would pass
        // over a `screen_lines` that panicked on everything.
        assert!(!screen_lines("09-gate-refused", true).is_empty());
    }

    /// With the gate shut a screen renders its honest empty state, not its fixture.
    #[test]
    fn a_screen_rendered_with_the_gate_shut_names_its_contract_instead_of_its_numbers() {
        let open = screen_lines("09-gate-refused", true).join(" ");
        let shut = screen_lines("09-gate-refused", false).join(" ");
        assert!(open.contains("fastapi_turbo"), "{open:?}");
        assert!(!shut.contains("fastapi_turbo"), "{shut:?}");
        assert!(shut.contains("gate_refusal"), "{shut:?}");
    }
}
