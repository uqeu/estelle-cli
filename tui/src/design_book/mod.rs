//! The design book's screens, rendered by the REAL renderer.
//!
//! 🔴 **WHY THIS MODULE EXISTS.** The founder reviewed `CLI-DESIGN-BOOK.html` screen by screen and
//! asked one thing of the next pass: *"Is this rendered in Rust or JavaScript? I want you to render
//! all of this now in Rust, so that it's easier for you to port these over."* Twenty-five of the
//! forty-one screens already came out of the production renderer. The other sixteen — and seven
//! SHIPPED renderer states that had no gallery frame — were HTML drawn by hand, which means their
//! columns were spaces somebody counted rather than a layout anything computed.
//!
//! ⚠️ **A HAND-PLACED SPACE IS A LAYOUT CLAIM NOBODY CAN FALSIFY.** That is the whole defect this
//! module closes. Every screen here builds its rows through [`crate::cols`] — the module whose four
//! tests exist because of four real alignment bugs, including `⏺` being three bytes and one column
//! — so a row that does not line up is a test failure rather than a thing a reader notices in a
//! screenshot six weeks later.
//!
//! ## The contract every screen in here keeps
//!
//! 1. **Columns come from [`crate::cols`].** `Col::l`/`Col::r`/`row`/`head`/`rule`. Indentation is
//!    the `indent` argument, never a padded string.
//! 2. **Colours come from [`crate::theme::Palette`].** No `Color::Rgb`, no ANSI `Color::Blue` —
//!    the gallery's SVG maps bare ANSI to `#65A8FF`/`#70C6CC`, two values that are in no token.
//! 3. 🔴 **NO BOXES.** Not one of `┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`. The selected row is highlighted with
//!    `palette.tint`; a list is never framed. The gallery asserts this over every frame, and the
//!    founder said it three times in one review.
//! 4. **Every screen declares a `needle`** — text the rendered buffer must contain. A frame that
//!    renders blank is otherwise a passing frame.

use ratatui::text::Line;

use crate::theme::Palette;

pub(crate) mod answers;
pub(crate) mod costing;
pub(crate) mod loops;
pub(crate) mod panes;
pub(crate) mod skills;
pub(crate) mod surfaces;

/// One book screen: what to call it, how big it is, and the string that proves it rendered.
pub(crate) struct BookScreen {
    /// The gallery frame name. Prefixed with the book's screen number so the book and the gallery
    /// sort together and a reader can find one from the other without a lookup table.
    pub name: &'static str,
    pub width: u16,
    pub height: u16,
    /// 🔴 THE VACUITY GUARD. `write_frame` will happily write an empty buffer, and an empty frame
    /// reads exactly like a rendered one in a directory listing. Asserting a needle is the cheapest
    /// thing that cannot pass on a screen that drew nothing.
    pub needle: &'static str,
    pub render: fn(&Palette, u64, bool) -> Vec<Line<'static>>,
}

/// Every screen the live app cannot produce from its own state, in book order.
pub(crate) const SCREENS: &[BookScreen] = &[
    BookScreen {
        name: "02-login-two-stage",
        width: 130,
        height: 38,
        needle: "who pays for model tokens",
        render: surfaces::login,
    },
    BookScreen {
        name: "06-no-repository-here",
        width: 120,
        height: 30,
        needle: "not a git repository",
        render: surfaces::no_repository,
    },
    BookScreen {
        name: "09-gate-refused",
        width: 120,
        height: 30,
        needle: "Gate refused",
        render: loops::gate_refused,
    },
    BookScreen {
        name: "10-navigation-stale",
        width: 120,
        height: 28,
        needle: "indexed at",
        render: loops::navigation_stale,
    },
    BookScreen {
        name: "11-compaction-refused",
        width: 120,
        height: 26,
        needle: "one message",
        render: loops::compaction_refused,
    },
    BookScreen {
        name: "12-skills-typed",
        width: 120,
        height: 30,
        needle: "skill:",
        render: skills::typed,
    },
    BookScreen {
        name: "13-skills-offered",
        width: 120,
        height: 30,
        needle: "send with the skill",
        render: skills::offered,
    },
    BookScreen {
        name: "14-skills-browse",
        width: 120,
        height: 34,
        needle: "max compose",
        render: skills::browse,
    },
    BookScreen {
        name: "18-every-command",
        width: 130,
        height: 40,
        needle: "advertised and refused",
        render: surfaces::every_command,
    },
    BookScreen {
        name: "19-shell-mode",
        width: 120,
        height: 30,
        needle: "your shell, not Estelle",
        render: surfaces::shell_mode,
    },
    BookScreen {
        name: "25-panels-one-terminal",
        width: 180,
        height: 34,
        needle: "tab strip",
        render: panes::panels,
    },
    BookScreen {
        name: "30-provider-keys",
        width: 120,
        height: 34,
        needle: "how it authenticates",
        render: surfaces::provider_keys,
    },
    BookScreen {
        name: "32-memory-remaining",
        width: 130,
        height: 36,
        needle: "plan remaining",
        render: costing::memory_remaining,
    },
    BookScreen {
        name: "33-usage-spend",
        width: 130,
        height: 34,
        needle: "this session you spent",
        render: costing::usage_spend,
    },
    BookScreen {
        name: "33b-model-cost",
        width: 130,
        height: 34,
        needle: "run spend",
        render: costing::model_cost,
    },
    BookScreen {
        name: "34-answer-table-diagram",
        width: 120,
        height: 36,
        needle: "mermaid",
        render: answers::table_and_diagram,
    },
    BookScreen {
        name: "35-session-tabs",
        width: 140,
        height: 30,
        needle: "sessions",
        render: panes::session_tabs,
    },
    BookScreen {
        name: "36-doctor-failing",
        width: 120,
        height: 30,
        needle: "what this is NOT",
        render: surfaces::doctor_failing,
    },
    BookScreen {
        name: "37-resume-session",
        width: 120,
        height: 30,
        needle: "how it ended",
        render: surfaces::resume_session,
    },
    BookScreen {
        name: "38-sweep-running",
        width: 120,
        height: 28,
        needle: "checking account capacity",
        render: panes::sweep_running,
    },
    BookScreen {
        name: "39-tool-calls",
        width: 120,
        height: 34,
        needle: "lines hidden",
        render: answers::tool_calls,
    },
    BookScreen {
        name: "40-code-graph",
        width: 130,
        height: 32,
        needle: "chokepoint",
        render: answers::code_graph,
    },
    BookScreen {
        name: "41-memory-correct",
        width: 130,
        height: 32,
        needle: "supersedes",
        render: answers::memory_correct,
    },
];

/// Re-own a `Line` whose spans borrow local `String`s.
///
/// ⚠️ **THE REASON THIS EXISTS RATHER THAN `Box::leak`.** [`crate::cols::row`] borrows its cells,
/// so a row built from computed text is a `Line<'_>` tied to locals. `screens.rs` reached for
/// `Box::leak` to get `'static`, which leaks a string per call and is invisible at the call site.
/// Copying the spans is the same cost once and no cost forever after.
pub(crate) fn owned(line: Line<'_>) -> Line<'static> {
    let style = line.style;
    Line::from(
        line.spans
            .into_iter()
            .map(|span| ratatui::text::Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
    .style(style)
}

/// A blank row. Named so a screen never reaches for `Line::from("")` and loses its palette.
pub(crate) fn blank() -> Line<'static> {
    Line::from("")
}

/// One dim line of prose, indented two, the way every catalog screen writes a footnote.
pub(crate) fn note(palette: &Palette, text: &str) -> Line<'static> {
    Line::from(ratatui::text::Span::styled(
        format!("  {text}"),
        ratatui::style::Style::default().fg(palette.dim),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    /// 🔴 A NAME COLLISION WOULD SILENTLY OVERWRITE A FRAME ON DISK.
    ///
    /// `write_frame` writes `{name}.txt`; two screens sharing a name means the book loses one and
    /// the gallery index lists a file whose content belongs to a different screen. Nothing else
    /// would go red.
    #[test]
    fn every_book_screen_has_a_unique_name() {
        let names = SCREENS
            .iter()
            .map(|screen| screen.name)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), SCREENS.len(), "two book screens share a name");
    }

    /// Every screen renders non-empty in BOTH palettes. A screen that only works on the dark
    /// ground is a screen the cream reader cannot use, and the founder reads cream.
    #[test]
    fn every_book_screen_renders_in_both_themes() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for screen in SCREENS {
                let lines = (screen.render)(&palette, 0, true);
                assert!(!lines.is_empty(), "{} rendered nothing", screen.name);
                assert!(
                    lines.len() <= usize::from(screen.height),
                    "{} rendered {} rows into a {}-row frame",
                    screen.name,
                    lines.len(),
                    screen.height
                );
            }
        }
    }

    /// 🔴 **A DESIGN FRAME MAY NOT TRUNCATE ITS OWN COPY.**
    ///
    /// `cols` ends an overlong cell with `…`, which is correct on a live screen — a model name has
    /// to fit and the reader can widen the terminal. On a BOOK frame it is a defect: the founder is
    /// reviewing the words, and a word he cannot read is a word he cannot rule on. The frame widths
    /// in [`SCREENS`] are ours to choose, so an ellipsis here means a column was sized wrong, not
    /// that the content was too long. Three cells were truncated when this was written —
    /// `affinity · cost p…`, `this machine, no …`, `$45 soft…` — all on the costing panel, all
    /// invisible until somebody read the generated book instead of the test output.
    ///
    /// ⚠️ **IT ASSERTS ON A SPAN ENDING IN `…`, NOT ON A LINE CONTAINING ONE, AND THAT DISTINCTION
    /// IS THE WHOLE TEST.** The first version searched the joined line and fired on
    /// `sk-ant-…4f2c` — a deliberately MASKED API key on the provider screen, where the ellipsis is
    /// the mask and eliding it is the point. `cols::truncate` builds `take(width - 1)` + `…`, so a
    /// truncated cell is always a span whose LAST character is the ellipsis; a mask never is. A
    /// guard that cannot tell those two apart gets suppressed within a week, and a suppressed guard
    /// is worth less than none.
    #[test]
    fn no_book_screen_truncates_its_own_copy() {
        let palette = ScreenTheme::Dark.palette();
        let mut checked = 0_usize;
        for screen in SCREENS {
            for line in (screen.render)(&palette, 0, true) {
                for span in &line.spans {
                    checked += 1;
                    assert!(
                        !span.content.ends_with('\u{2026}'),
                        "{} truncated a cell — widen the column, do not shorten the words: {:?}",
                        screen.name,
                        span.content
                    );
                }
            }
        }
        // ⚠️ A guard that iterated nothing would pass identically. The book is ~1,900 rows; a
        // hundred spans is a floor no real gallery can fall under by accident.
        assert!(checked > 100, "only {checked} spans were checked");
    }

    /// 🔴 THE NO-BOX RULE, ENFORCED AT THE SOURCE RATHER THAN ONLY AT THE FRAME.
    ///
    /// The gallery already greps the rendered buffer, but that check runs only when the gallery
    /// runs. This one runs on every `cargo test` and names the screen, so a corner never gets as
    /// far as a frame.
    #[test]
    fn no_book_screen_draws_a_box_corner() {
        const CORNERS: [char; 9] = ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];
        let palette = ScreenTheme::Dark.palette();
        for screen in SCREENS {
            for line in (screen.render)(&palette, 0, true) {
                let text: String = line.spans.iter().map(|span| span.content.as_ref()).collect();
                for corner in CORNERS {
                    assert!(
                        !text.contains(corner),
                        "{} drew a box corner {corner:?} in {text:?}",
                        screen.name
                    );
                }
            }
        }
    }
}
