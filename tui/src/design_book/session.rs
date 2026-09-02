//! The vocabulary a scripted session is written in — and nothing else.
//!
//! 🔴 **WHY THIS IS NOT THE DESIGN BOOK.** `estelle demo` is a GALLERY: each screen is a full-frame
//! render and advancing means a keypress and a hard cut. The founder watched it and said what he
//! actually wanted: *"It should be a natural sequence where it's like someone's actually using the
//! CLI — not 'okay this is the next page, and as you can see here, this is what it did.'"* So a
//! session is **one transcript that grows**. Nothing here clears the screen, and there is no
//! function in this module that could: [`crate::demo_session`] owns a single `Vec<Line>` that is
//! only ever appended to. A page turn is not a setting that got left off — it is unrepresentable.
//!
//! ## The three rules this module exists to make unbreakable
//!
//! 1. **The script is DATA.** [`Beat`] and [`Say`] are declarative; [`crate::design_book::script`]
//!    holds the films and nothing else. Reordering a film is moving a `const` array element. No
//!    branch of the player reads a film's content to decide what to do with it.
//! 2. **Colour comes from [`Ink`], which is a total map onto [`Palette`].** A script cannot name a
//!    colour, only a ROLE, so the design book's untokened-cell census (which reads 0) cannot be
//!    pushed above zero from here even by a typo — a wrong ink is a compile error.
//! 3. **Columns come from [`crate::cols`]**, through [`Grid`]. A script cannot pad with spaces
//!    because a `Say::Row` carries cells, not a line.
//!
//! ⚠️ **[`Ink`] IS A NEAR-DUPLICATE OF `panes::Tone` AND I AM SAYING SO RATHER THAN HIDING IT.**
//! `panes.rs` already maps five role names onto `Palette` for the same reason. They are separate
//! today because `Tone` is private to that module and carries five of the nine roles a film needs;
//! unifying them means editing `panes.rs`, which the design-book lane is holding. **This is a
//! second owner of one derived fact and it should be collapsed the day both lanes land** — the
//! honest form of that debt is a sentence here, not a silent copy.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, row, rule};
use crate::design_book::{SCREENS, blank, note, owned};
use crate::marks::{Mark, StepMark, headline};
use crate::theme::Palette;

/// The column width every film lays out against. Narrower than the book's 120 because a session
/// frame also owns an ask bar and a margin; a rule that reaches the terminal edge reads as a seam.
pub(crate) const WIDTH: usize = 104;

// ── colour ───────────────────────────────────────────────────────────────────────────────────

/// A palette ROLE, by name. The only way a film may speak about colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ink {
    Dim,
    Mid,
    Bright,
    Red,
    Green,
    Warn,
    Cite,
    Plan,
    Skill,
}

impl Ink {
    /// 🔴 **EVERY VARIANT, ITERABLE.** The colour-token test below walks this rather than a
    /// hand-written list, so a tenth role added to the enum is covered by construction. The last
    /// time a vocabulary enum had a hand-copied test list, `?` reached two shipped screens with no
    /// name and no test (`marks.rs`).
    pub(crate) const ALL: [Self; 9] = [
        Self::Dim,
        Self::Mid,
        Self::Bright,
        Self::Red,
        Self::Green,
        Self::Warn,
        Self::Cite,
        Self::Plan,
        Self::Skill,
    ];

    pub(crate) fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Dim => palette.dim,
            Self::Mid => palette.mid,
            Self::Bright => palette.bright,
            Self::Red => palette.red,
            Self::Green => palette.green,
            Self::Warn => palette.warn,
            Self::Cite => palette.cite,
            Self::Plan => palette.plan,
            Self::Skill => palette.skill,
        }
    }
}

// ── tables ───────────────────────────────────────────────────────────────────────────────────

/// One table's column widths and their inks, declared once as a `const` beside the rows it lays
/// out. ⚠️ The two slices are asserted to be the same length at render time rather than trusted:
/// a `Grid` that lost an ink would silently paint its last column `Dim` and still look like a
/// perfectly good table — the shape this repo has paid for under the name "the always-present
/// field".
#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub cols: &'static [Col],
    pub inks: &'static [Ink],
}

impl Grid {
    pub(crate) const fn new(cols: &'static [Col], inks: &'static [Ink]) -> Self {
        Self { cols, inks }
    }

    /// Split a `|`-delimited script row into cells, padded to the column count.
    ///
    /// ⚠️ Padding rather than asserting is deliberate and it is the ONE place this module is
    /// forgiving: a film row that ends early (`"plan|complete"` against a four-column grid) is a
    /// row the founder trimmed while reordering, and refusing to render it would turn an edit to
    /// DATA into a crash. A row that carries MORE cells than columns is a different fact — that is
    /// a grid he meant to widen — and it asserts.
    fn cells(&self, source: &'static str) -> Vec<&'static str> {
        let mut cells: Vec<&'static str> = source.split('|').map(str::trim).collect();
        assert!(
            cells.len() <= self.cols.len(),
            "script row {source:?} carries {} cells for {} columns",
            cells.len(),
            self.cols.len()
        );
        cells.resize(self.cols.len(), "");
        cells
    }
}

// ── what a reply is made of ──────────────────────────────────────────────────────────────────

/// One rendered unit of a reply. A film is a list of these; the player streams them.
///
/// ⚠️ **`Screen` IS THE WHOLE COVERAGE STORY AND IT HAS ONE OWNER.** A film does not re-draw a
/// design-book screen: it names one, and [`crate::design_book::render`] draws it — the same
/// function `estelle demo` calls, through the same fixture gate. So a screen cannot appear in a
/// film with its fixtures on while the gate is shut, and a screen the book fixes is fixed in the
/// films the same hour.
pub(crate) enum Say {
    Blank,
    /// `── label · mode ────…`. No corners; a rule cannot close into a panel.
    Rule(&'static str, &'static str),
    /// A pulsing mark, a headline and a detail — through [`headline`], which is the only thing in
    /// the crate that can pulse a mark without also pulsing the words beside it.
    Head(Mark, &'static str, &'static str),
    /// Dim prose at indent 2.
    Note(&'static str),
    /// One coloured line at indent 2.
    Text(Ink, &'static str),
    /// A dim header row.
    Cols(Grid, &'static str),
    /// A table row, each cell inked by its column.
    Row(Grid, &'static str),
    /// A table row lifted onto `palette.tint` — the selection band, and the ONLY highlight
    /// vocabulary in this crate. There is no `Box` variant and there will not be one.
    Lift(Grid, &'static str),
    /// A plan step: `✓ verb  detail`, the active one on a full-width tint band.
    Step(StepMark, &'static str, &'static str),
    /// An entire design-book screen, by name, streamed in a line at a time.
    Screen(&'static str),
    /// Silence inside a reply, in milliseconds before `--speed`. Estelle taking its time.
    Wait(u32),
}

impl Say {
    /// The rows this unit draws. `Wait` draws none — it is time, not text.
    pub(crate) fn lines(
        &self,
        palette: &Palette,
        tick: u64,
        pulse: bool,
        fixtures: bool,
    ) -> Vec<Line<'static>> {
        match self {
            Self::Blank => vec![blank()],
            Self::Wait(_) => Vec::new(),
            Self::Rule(label, mode) => vec![owned(rule(
                label,
                mode,
                WIDTH,
                palette.dim,
                palette.mid,
                palette.cite,
            ))],
            Self::Head(mark, text, detail) => {
                vec![headline(*mark, text, detail, palette, tick, pulse)]
            }
            Self::Note(text) => vec![note(palette, text)],
            Self::Text(ink, text) => vec![Line::from(Span::styled(
                format!("  {text}"),
                Style::default().fg(ink.colour(palette)),
            ))],
            Self::Cols(grid, source) => vec![Self::table_row(grid, source, palette, true, false)],
            Self::Row(grid, source) => vec![Self::table_row(grid, source, palette, false, false)],
            Self::Lift(grid, source) => vec![Self::table_row(grid, source, palette, false, true)],
            Self::Step(mark, verb, detail) => vec![Self::step(*mark, verb, detail, palette)],
            Self::Screen(name) => Self::screen(name, palette, tick, pulse, fixtures),
        }
    }

    /// One `cols`-built row. `dim` forces the header ink; `lift` puts it on the tint band.
    fn table_row(
        grid: &Grid,
        source: &'static str,
        palette: &Palette,
        dim: bool,
        lift: bool,
    ) -> Line<'static> {
        assert_eq!(
            grid.cols.len(),
            grid.inks.len(),
            "a grid must ink every column it declares"
        );
        let cells = grid
            .cells(source)
            .into_iter()
            .zip(grid.inks)
            .map(|(text, ink)| {
                let colour = if dim {
                    palette.dim
                } else {
                    ink.colour(palette)
                };
                Cell(text, colour)
            })
            .collect::<Vec<_>>();
        let line = owned(row(grid.cols, &cells, 2));
        if lift {
            line.style(Style::default().bg(palette.tint))
        } else {
            line
        }
    }

    /// A plan step. The ACTIVE one is a FULL-WIDTH band — `cols::row` pads every cell so the tint
    /// reaches the right edge. A band that stops at its own last word reads as a highlight on the
    /// text rather than as "you are here".
    fn step(
        mark: StepMark,
        verb: &'static str,
        detail: &'static str,
        palette: &Palette,
    ) -> Line<'static> {
        const MARK: usize = 1;
        const VERB: usize = 14;
        const GAP: usize = 2;
        let columns = [
            Col::l(MARK),
            Col::l(VERB),
            Col::l(WIDTH - MARK - GAP - VERB - GAP),
        ];
        let verb_ink = match mark {
            StepMark::Active => palette.bright,
            StepMark::NotStarted => palette.dim,
            _ => palette.mid,
        };
        let line = owned(row(
            &columns,
            &[
                Cell(mark.glyph(), mark.colour(palette)),
                Cell(verb, verb_ink),
                Cell(detail, palette.dim),
            ],
            0,
        ));
        match mark.row_background(palette) {
            Some(background) => line.style(Style::default().bg(background)),
            None => line,
        }
    }

    /// A design-book screen by name.
    ///
    /// 🔴 **AN UNKNOWN NAME PANICS RATHER THAN RENDERING NOTHING.** A film that silently drew zero
    /// rows for a mistyped screen is the vacuity shape: the session would play, the beat would be
    /// blank, and the runtime would still be reported as a pass. `every_screen_a_film_names_exists`
    /// makes it a test failure instead of a hole in the footage, and this assert is the second
    /// door for a name added after that test was written.
    fn screen(
        name: &str,
        palette: &Palette,
        tick: u64,
        pulse: bool,
        fixtures: bool,
    ) -> Vec<Line<'static>> {
        let screen = SCREENS
            .iter()
            .find(|screen| screen.name == name)
            .unwrap_or_else(|| {
                panic!("a film names screen {name:?}, which the book does not have")
            });
        crate::design_book::render(screen, palette, tick, pulse, fixtures)
    }
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
    /// Type this at the ordinary rate, notice it, and backspace every character back out. The
    /// backspaces are the tell that a person is at the keyboard.
    Oops(&'static str),
    /// A familiar phrase, typed fast. A burst next to a slow patch is what makes a rate lumpy.
    Burst(&'static str),
    /// Hands off the keyboard, thinking about the next word.
    Pause(u32),
}

// ── one beat ─────────────────────────────────────────────────────────────────────────────────

/// One exchange: what gets typed, how long Estelle takes, what comes back, and how long it sits.
///
/// ⚠️ **`read_ms` IS NOT PADDING AND IT IS THE FIELD MOST LIKELY TO BE TRIMMED BY SOMEONE TRYING
/// TO FIT MORE IN.** The founder asked for two and a half minutes *"so I can talk through it"*, and
/// the extra ninety seconds is for READING and WAITING, not for more beats. A film that runs long
/// loses a beat; it does not lose its silence.
pub(crate) struct Beat {
    pub typed: &'static [Key],
    /// Silence between pressing enter and the first line of the reply. A real grounded answer
    /// takes seconds, and this is where the founder will be talking.
    pub think_ms: u32,
    /// Milliseconds between streamed reply lines. An answer that materialises whole is a slide.
    pub line_ms: u32,
    pub reply: &'static [Say],
    /// How long the finished reply sits before the next beat starts typing.
    pub read_ms: u32,
}

/// One film.
pub(crate) struct Film {
    /// `1`, `2`, `3` — what the founder types after `--session`.
    pub number: u8,
    /// The repo the session is running in, shown in the identity rule and the ask rule.
    pub repo: &'static str,
    pub branch: &'static str,
    pub beats: &'static [Beat],
}

/// 🔴 **THE BOUNDS ARE NAMED CONSTANTS AND THEY ARE CHECKED BY A TEST, NOT BY THE PLAYER ALONE.**
/// Power of Ten #2: every loop has a fixed, stated bound. A scripted session that can hang is a
/// recording session that has to be restarted, and the founder has already had to Ctrl-C out of one
/// attempt at this.
pub(crate) const MAX_BEATS: usize = 32;

/// The wall-clock ceiling for one film at `--speed 1`. Films are asserted to fit well inside it.
pub(crate) const MAX_FILM_MS: u32 = 6 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    /// Every ink maps to a colour the palette actually declares.
    ///
    /// 🔴 **THIS IS THE FILMS' REPLACEMENT FOR THE BOOK'S UNTOKENED-CELL CENSUS, AND IT IS NEEDED
    /// BECAUSE THAT CENSUS CANNOT SEE THEM.** `design_book.py` walks `SCREENS` and the gallery SVGs;
    /// a film is neither, so its colours are outside the count that reads 0. Rather than let the
    /// films be a blind spot in the one measurement the founder checks, the guarantee is moved into
    /// the type system — [`Ink`] is the only colour vocabulary a script has — and asserted here
    /// against the palette in both themes.
    #[test]
    fn every_ink_is_a_palette_token_in_both_themes() {
        for (theme, name) in [(ScreenTheme::Dark, "dark"), (ScreenTheme::Cream, "cream")] {
            let palette = theme.palette();
            let tokens = [
                palette.dim,
                palette.mid,
                palette.bright,
                palette.red,
                palette.green,
                palette.warn,
                palette.cite,
                palette.plan,
                palette.skill,
            ];
            for ink in Ink::ALL {
                assert!(
                    tokens.contains(&ink.colour(&palette)),
                    "{ink:?} is not a palette token on {name}"
                );
            }
        }
    }

    /// A grid pads a short row and refuses a long one.
    #[test]
    fn a_grid_pads_a_trimmed_row_and_refuses_an_overfull_one() {
        const G: Grid = Grid::new(
            &[Col::l(6), Col::l(6), Col::l(6)],
            &[Ink::Mid, Ink::Dim, Ink::Cite],
        );
        assert_eq!(G.cells("a|b"), vec!["a", "b", ""]);
        assert_eq!(G.cells("a|b|c"), vec!["a", "b", "c"]);
        let overfull = std::panic::catch_unwind(|| G.cells("a|b|c|d"));
        assert!(
            overfull.is_err(),
            "a four-cell row must not fit a three-column grid"
        );
    }

    /// The tint band reaches the right edge on the active step and nowhere else.
    #[test]
    fn only_the_active_step_carries_the_band_and_it_is_full_width() {
        let palette = ScreenTheme::Dark.palette();
        let active = Say::Step(StepMark::Active, "repairing", "round 2 of 3");
        let done = Say::Step(StepMark::Done, "refused", "the import does not exist");
        let width = |line: &Line<'_>| -> usize {
            line.spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum()
        };
        let active = active.lines(&palette, 0, true, true);
        let done = done.lines(&palette, 0, true, true);
        assert_eq!(width(&active[0]), WIDTH);
        assert_eq!(width(&done[0]), WIDTH);
        assert_eq!(active[0].style.bg, Some(palette.tint));
        assert_eq!(done[0].style.bg, None);
    }

    /// 🔴 No box corner reaches a film frame, from any `Say`.
    ///
    /// The founder has said this four times. `box_glyphs` guards the SOURCE; this guards the
    /// RENDERED rows of the one vocabulary a film is written in, which is the other half.
    #[test]
    fn no_say_can_draw_a_box_corner() {
        let palette = ScreenTheme::Dark.palette();
        const G: Grid = Grid::new(&[Col::l(8), Col::l(8)], &[Ink::Mid, Ink::Dim]);
        let says = [
            Say::Blank,
            Say::Rule("ask", "sable/claims"),
            Say::Head(Mark::Blocked, "Gate refused", "no model call"),
            Say::Note("nothing was written to your tree."),
            Say::Text(Ink::Green, "merge:true"),
            Say::Cols(G, "role|model"),
            Say::Row(G, "plan|opus"),
            Say::Lift(G, "implement|kimi"),
            Say::Step(StepMark::Active, "repairing", "round 2 of 3"),
        ];
        for say in &says {
            let text: String = say
                .lines(&palette, 0, true, true)
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
                .collect();
            // The nine corners, escaped so this guard cannot match itself. `box_glyphs` owns
            // the list and is compiled into the LIBRARY, which this binary does not declare —
            // the same reason `main.rs:7866` carries its own copy.
            const BOX_CORNERS: [&str; 9] = [
                "\u{250C}", "\u{2510}", "\u{2514}", "\u{2518}", "\u{251C}", "\u{2524}", "\u{252C}",
                "\u{2534}", "\u{253C}",
            ];
            for corner in BOX_CORNERS {
                assert!(
                    !text.contains(corner),
                    "a Say drew the box corner {corner:?}: {text:?}"
                );
            }
        }
    }
}
