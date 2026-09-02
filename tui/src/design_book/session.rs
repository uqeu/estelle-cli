//! The vocabulary a scripted session is written in, and the one place its columns are laid out.
//!
//! 🔴 **EVERY MULTI-COLUMN CELL WRAPPED TO COLUMN 0, IN SIX BEATS, FOR ONE REASON.** A film beat used
//! to flatten a design-book screen into strings and hand them to the transcript. But a book screen
//! renders at **its own fixed width** — 108 for the gate, 92 for memory, 84 for tools — while the
//! session pane is whatever is left after the production rail takes the right-hand side. Every line
//! wider than the pane was then re-wrapped by the transcript, which knows nothing about columns and
//! starts every continuation at the left margin. So `no such package on PyPI; nearest` was followed
//! by `is fastapi` at column 0, underneath the *other* column. The prose read as scrambled.
//!
//! ⚠️ **IT WAS NEVER A GATE-REFUSAL BUG.** It looked like one because that beat has the longest
//! prose, and fixing it there would have left the same defect in five other beats. The cause is
//! *rendering at a width the destination does not have*, so the fix is one function —
//! [`table_lines`] — and every block in every film goes through it.
//!
//! ## What a wrapped cell does here
//!
//! A cell too long for its column is **word-wrapped into continuation rows**, and each continuation
//! is emitted through [`crate::cols::row`] with the earlier cells blank. The continuation therefore
//! starts at its own column's x, because `cols` padded the blanks in front of it — the position is
//! *computed*, never typed. ⚠️ That is also why the wrap counts CHARACTERS and not bytes: `⏺` is
//! three bytes and one column, and byte-based wrapping is precisely what produces the ragged left
//! edge this module exists to prevent. `crate::cols` has a test for that glyph for the same reason.
//!
//! ## Why the vocabulary is the product's own
//!
//! A [`Say`] does not describe how something looks; it names WHICH KIND OF TURN it is, and
//! `crate::transcript` decides the rest — once, for the live app and the film together. There is
//! deliberately **no `Screen` variant any more**: naming a gallery frame put `09-gate-refused` on
//! screen in a film for an investor, and a `Tool` receipt put `· 23 lines` beside it. Both are
//! `demo --list`'s vocabulary, not a person's.

use crate::cols::{Cell, Col, row};

/// The narrowest column this module will shrink to before it gives up and lets a cell wrap more.
const MIN_COL: usize = 8;

/// Word-wrap `text` to `width` COLUMNS, never bytes.
///
/// ⚠️ **THIS IS THE THIRD WORD-WRAP IN THE TREE AND I AM SAYING SO RATHER THAN HIDING IT.**
/// `wrapping::wrap_ranges_trim` is the real owner and is compiled into the LIBRARY, which the
/// `estelle` binary does not declare; `gate_refusal::wrapped` is private to its module. Neither is
/// reachable from here, so the choice was a local copy or a visibility change in a file another
/// lane is holding. **When those merge, this should be deleted in favour of `wrapping`.**
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut line = String::new();
    let mut line_cols = 0usize;
    for word in text.split_whitespace() {
        let word_cols = word.chars().count();
        // A word longer than the column is broken rather than allowed to push the row — the same
        // decision `cols::row` makes when it truncates, except nothing is lost here.
        if word_cols > width {
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line_cols = 0;
            }
            let mut rest = word.chars().collect::<Vec<_>>();
            while rest.len() > width {
                out.push(rest.drain(..width).collect());
            }
            line = rest.into_iter().collect();
            line_cols = line.chars().count();
            continue;
        }
        let need = if line.is_empty() {
            word_cols
        } else {
            word_cols + 1
        };
        if line_cols + need > width {
            out.push(std::mem::take(&mut line));
            line_cols = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_cols += 1;
        }
        line.push_str(word);
        line_cols += word_cols;
    }
    if !line.is_empty() {
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Shrink a column spec until it fits `width`, taking from the widest column first.
///
/// 🔴 **THE PANE IS NOT A CONSTANT AND THAT IS THE WHOLE POINT.** The session column is whatever is
/// left after the production rail, so it is ~121 columns on a 200-wide terminal and ~91 on a
/// 150-wide one. A table laid out against a number somebody typed will overflow one of those.
fn fitted(columns: &[Col], width: usize) -> Vec<Col> {
    let mut out = columns.to_vec();
    let gaps: usize = out
        .iter()
        .take(out.len().saturating_sub(1))
        .map(|col| col.gap)
        .sum();
    let mut total: usize = out.iter().map(|col| col.w).sum::<usize>() + gaps;
    // Bounded: every pass removes at least one column of width, and the loop stops when the widest
    // column has reached the floor. Power of Ten #2 — the bound is the total width itself.
    let mut guard = total + 1;
    while total > width && guard > 0 {
        guard -= 1;
        let Some(widest) = out
            .iter_mut()
            .filter(|col| col.w > MIN_COL)
            .max_by_key(|col| col.w)
        else {
            break;
        };
        widest.w -= 1;
        total -= 1;
    }
    out
}

/// Lay a `|`-delimited block out at `width`, wrapping any over-long cell INSIDE its own column.
///
/// This is the one function the founder's "the spacing is messed up" resolves to. Every row of every
/// table in every film comes through here, and every line it returns was positioned by
/// [`crate::cols::row`] — so a continuation cannot start at the left margin, and a column that does
/// not line up is a `cols` test failure rather than something a reader notices on camera.
pub(crate) fn table_lines(columns: &[Col], rows: &[&'static str], width: usize) -> Vec<String> {
    let columns = fitted(columns, width);
    let mut out = Vec::new();
    for source in rows {
        let mut cells: Vec<&str> = source.split('|').map(str::trim).collect();
        assert!(
            cells.len() <= columns.len(),
            "script row {source:?} carries {} cells for {} columns",
            cells.len(),
            columns.len()
        );
        cells.resize(columns.len(), "");
        let wrapped: Vec<Vec<String>> = cells
            .iter()
            .zip(&columns)
            .map(|(text, col)| wrap(text, col.w))
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        for index in 0..height {
            let line_cells = wrapped
                .iter()
                .map(|chunks| chunks.get(index).map_or("", String::as_str))
                .zip(&columns)
                .map(|(text, _)| Cell(text, ratatui::style::Color::Reset))
                .collect::<Vec<_>>();
            out.push(
                row(&columns, &line_cells, 0)
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
            );
        }
    }
    out
}

// ── what a reply is made of ──────────────────────────────────────────────────────────────────

/// One unit of a reply. Each variant names a kind of turn the PRODUCT already has.
pub(crate) enum Say {
    /// Prose, streamed in word-sized chunks so it reads as generation rather than as a slide.
    Answer { text: &'static str, grounded: bool },
    /// A command receipt: `● /gate` and its output. ⚠️ **Deliberately `Command` and not `Tool`** —
    /// a tool receipt renders `· 23 lines` beside its label (`history_transcript.rs:167`), which is
    /// a debug metric, and it put one on screen in a film for an investor.
    Command {
        name: &'static str,
        lines: &'static [&'static str],
    },
    /// A command receipt whose output is an aligned, wrapped table.
    Table {
        name: &'static str,
        columns: &'static [Col],
        rows: &'static [&'static str],
    },
    /// The gate refusing, drawn by [`crate::gate_refusal`] itself at the REAL pane width — one
    /// owner, and the only way its cells wrap inside their columns.
    Gate,
    /// A system note.
    System(&'static str),
    /// The product's own three-line refusal banner, painted by the one owner of that colour.
    Failure([&'static str; 3]),
    /// Silence inside a reply, in milliseconds before `--speed`. Estelle taking its time.
    Wait(u32),
}

// ── what a person does at the keyboard ───────────────────────────────────────────────────────

/// One instruction to the hands.
///
/// 🔴 **THE STUMBLES ARE DATA, NOT A RATE.** A sprinkled error probability produces UNIFORM
/// imperfection, which reads as a machine imitating a person; a person does not misspell every
/// fourth word. So a film scripts two or three specific stumbles and leaves everything else clean.
pub(crate) enum Key {
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
/// ⚠️ **`read_ms` IS NOT PADDING.** He asked for two and a half minutes *"so I can talk through
/// it"*, and the extra time is for READING, not for more beats. A film that runs long loses a beat;
/// it does not lose its silence.
pub(crate) struct Beat {
    pub typed: &'static [Key],
    pub think_ms: u32,
    pub reply: &'static [Say],
    pub read_ms: u32,
}

/// One film.
pub(crate) struct Film {
    pub number: u8,
    /// The repo the session runs in. Drives the real header and the real ask rule.
    pub repo: &'static str,
    pub branch: &'static str,
    pub beats: &'static [Beat],
}

/// 🔴 **THE BOUNDS ARE NAMED CONSTANTS AND CHECKED BY A TEST, NOT BY THE PLAYER ALONE.**
pub(crate) const MAX_BEATS: usize = 32;

/// The wall-clock ceiling for one film at `--speed 1`.
pub(crate) const MAX_FILM_MS: u32 = 6 * 60 * 1000;

/// The narrowest session pane a film lays out against, used when the terminal cannot be measured.
pub(crate) const FALLBACK_PANE: usize = 88;

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 **THE DEFECT, AS A UNIT TEST.** A long cell wraps inside its column, and every
    /// continuation line starts at that column's x — never at the left margin.
    #[test]
    fn an_overlong_cell_wraps_inside_its_own_column() {
        // ⚠️ **COLUMN 0 IS WIDE ENOUGH FOR ITS OWN CELL, AND THAT IS THE POINT OF THE FIXTURE.**
        // The first version used `Col::l(18)` against a 20-character claim, so column 0 wrapped
        // too and its second chunk legitimately appeared at indent 0 — the test read that as the
        // defect it was written to catch. A guard whose fixture triggers the symptom by itself
        // cannot tell the symptom from the bug.
        static COLUMNS: &[Col] = &[Col::l(22), Col::l(40)];
        let lines = table_lines(
            COLUMNS,
            &[
                "import fastapi_turbo | no such package on PyPI; nearest is fastapi (0.115.6). The import would fail at load, not at test time.",
            ],
            80,
        );
        assert!(lines.len() > 1, "the cell did not wrap at all: {lines:?}");
        // The second column starts at 18 + 2 (the gap). Every continuation must begin there.
        for line in &lines[1..] {
            let indent = line.chars().take_while(|c| *c == ' ').count();
            assert_eq!(
                indent, 24,
                "a continuation began at column {indent} instead of its column's start: {line:?}"
            );
        }
        // Nothing is lost on the way.
        let joined = lines.join(" ");
        for word in ["fastapi_turbo", "0.115.6", "test", "time."] {
            assert!(
                joined.contains(word),
                "wrapping dropped {word:?}: {lines:?}"
            );
        }
    }

    /// A table fits the pane it is given, at every width the film can be recorded at.
    #[test]
    fn no_row_is_ever_wider_than_the_pane() {
        static COLUMNS: &[Col] = &[Col::l(22), Col::l(14), Col::l(46)];
        let rows = &[
            "claude-opus-5 | anthropic | plan locked by you, and this cell is deliberately far too long for its column",
            "kimi-k2.7-code | moonshot | healthy",
        ];
        for width in [120usize, 100, 91, 80, 64, 48] {
            for line in table_lines(COLUMNS, rows, width) {
                assert!(
                    line.chars().count() <= width,
                    "a {} column row overran a {width} column pane: {line:?}",
                    line.chars().count()
                );
            }
        }
    }

    /// ⚠️ **CHARACTERS, NOT BYTES.** `⏺` is three bytes and one column; a byte-based wrap breaks
    /// the row early and produces exactly the ragged left edge this module exists to prevent.
    #[test]
    fn wrapping_counts_columns_not_bytes() {
        let glyphs = "⏺ ⎿ ● ▲ ◐ ○ ■ ✓ ▶ □";
        let lines = wrap(glyphs, 10);
        for line in &lines {
            assert!(
                line.chars().count() <= 10,
                "{line:?} is {} columns wide",
                line.chars().count()
            );
        }
        // A byte-based wrap would have produced far more lines than a column-based one.
        assert!(lines.len() <= 3, "byte-counting wrap: {lines:?}");
    }

    /// A row that carries more cells than the grid has columns is a script defect, not a layout one.
    #[test]
    fn a_row_with_too_many_cells_is_refused() {
        static COLUMNS: &[Col] = &[Col::l(6), Col::l(6)];
        let overfull = std::panic::catch_unwind(|| table_lines(COLUMNS, &["a|b|c"], 40));
        assert!(overfull.is_err());
        // Positive control: the same grid lays a well-formed row out without complaint.
        assert_eq!(table_lines(COLUMNS, &["a|b"], 40).len(), 1);
    }

    /// Two rows of one table still end on the same column after fitting.
    #[test]
    fn fitting_keeps_every_row_the_same_shape() {
        static COLUMNS: &[Col] = &[Col::l(24), Col::l(16), Col::r(9)];
        let lines = table_lines(
            COLUMNS,
            &[
                "claims/fetcher.py:88 | urllib3 Retry | 3",
                "claims/upstream.py:141 | while loop | 5",
            ],
            60,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), lines[1].chars().count());
        assert!(lines[0].ends_with('3'));
        assert!(lines[1].ends_with('5'));
    }

    /// `fitted` never returns a spec wider than the pane while any column is above the floor.
    #[test]
    fn fitting_shrinks_the_widest_column_first_and_stops_at_the_floor() {
        static COLUMNS: &[Col] = &[Col::l(10), Col::l(60)];
        let narrow = fitted(COLUMNS, 40);
        assert!(narrow[1].w < 60, "the widest column did not shrink");
        assert!(
            narrow[0].w >= MIN_COL,
            "the narrow column went under the floor"
        );
        let total: usize = narrow.iter().map(|col| col.w).sum::<usize>() + narrow[0].gap;
        assert!(
            total <= 40,
            "fitted spec is {total} wide for a 40 column pane"
        );
        // Alignment is preserved through the fit — a right-aligned column stays right-aligned.
        static RIGHT: &[Col] = &[Col::l(10), Col::r(30)];
        assert!(matches!(fitted(RIGHT, 25)[1].a, crate::cols::Align::R));
    }
}
