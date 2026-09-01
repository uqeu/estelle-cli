//! The five status marks, and the one rule about pulsing them.
//!
//! 🔴 **THE MARK PULSES. THE TEXT NEVER DOES.** From the design spec
//! (`docs/cli-design/…THE-ESTELLE-TERMINAL…`): *"Only the mark pulses. Never the whole row. A
//! pulsing line of text is unreadable and gives people headaches."* Five call sites were pulsing
//! the LABEL alongside the mark — `gate_refusal` pulsed the words "Gate refused", `production_hud`
//! pulsed the failing symbol's name, and three catalog screens pulsed their headline text.
//!
//! ⚠️ **SO THE FIX IS A FUNCTION, NOT FIVE CORRECTIONS.** [`headline`] takes the mark and the text
//! and applies the pulse to exactly one of them; a caller cannot pulse the text through this
//! module even by accident, and `only_the_mark_pulses_the_words_never_do` presses every tick of
//! the cycle to prove the text span's colour does not move while the mark's does. Correcting five
//! sites by hand would have left the sixth to whoever writes it next.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::{Palette, pulse};

/// The status vocabulary, chosen by the founder from the rendered specimen sheet (set 2A).
///
/// Five marks, five meanings, no synonyms. A sixth state does not get a sixth glyph invented for
/// it here — it gets mapped onto the meaning it actually has, or it gets raised as a gap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mark {
    /// `●` landed · healthy
    Landed,
    /// `▲` blocked · needs a human
    Blocked,
    /// `◐` in flight
    InFlight,
    /// `○` queued · idle
    Queued,
    /// `■` refused
    Refused,
}

impl Mark {
    pub(crate) const fn glyph(self) -> &'static str {
        match self {
            Self::Landed => "●",
            Self::Blocked => "▲",
            Self::InFlight => "◐",
            Self::Queued => "○",
            Self::Refused => "■",
        }
    }

    pub(crate) fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Landed => palette.green,
            Self::Blocked => palette.warn,
            Self::InFlight => palette.cite,
            Self::Queued => palette.dim,
            Self::Refused => palette.red,
        }
    }

    /// The mark on its own, pulsing when asked. Every other span on the row stays steady.
    pub(crate) fn span(self, palette: &Palette, tick: u64, pulse_enabled: bool) -> Span<'static> {
        Span::styled(
            format!("{} ", self.glyph()),
            pulse(self.colour(palette), tick, pulse_enabled),
        )
    }
}

/// The PLAN STEP vocabulary, which is deliberately NOT [`Mark`].
///
/// ⚠️ **TWO LISTS, TWO JOBS, AND THEY MUST NOT BE MERGED.** The demo frame draws a plan step with
/// `✓ ▶ ▲ □` and a rail row with `● ▲ ◐ ○ ■`. `▲` means the same thing in both (blocked · needs a
/// human) and everything else differs: a plan step is a thing you WILL do, so "not started" is a
/// `□` you can imagine ticking, while a rail row is a thing that IS, so idle is a hollow `○`.
/// Collapsing them onto one enum would force one of the two surfaces to lie about its own tense.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StepMark {
    /// `✓` done
    Done,
    /// `▶` active — and the only step that also carries the tint row highlight
    Active,
    /// `▲` blocked · needs a human
    Blocked,
    /// `□` not started
    NotStarted,
}

impl StepMark {
    /// The wire's `status` string, mapped. An unrecognised status is NOT started rather than
    /// done — the safe direction to be wrong in is the one that does not claim work happened.
    pub(crate) fn from_status(status: &str) -> Self {
        match status {
            "complete" | "completed" | "done" => Self::Done,
            "active" | "running" | "in_progress" => Self::Active,
            "blocked" | "protected" => Self::Blocked,
            _ => Self::NotStarted,
        }
    }

    pub(crate) const fn glyph(self) -> &'static str {
        match self {
            Self::Done => "✓",
            Self::Active => "▶",
            Self::Blocked => "▲",
            Self::NotStarted => "□",
        }
    }

    pub(crate) fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Done => palette.green,
            Self::Active => palette.bright,
            Self::Blocked => palette.warn,
            Self::NotStarted => palette.dim,
        }
    }

    /// Only the ACTIVE step lifts its row off the ground. `palette.tint` is exactly this role —
    /// "one step off the ground" — and it is why no new colour was invented for the band.
    pub(crate) fn row_background(self, palette: &Palette) -> Option<Color> {
        (self == Self::Active).then_some(palette.tint)
    }
}

/// A headline row: a pulsing mark, then steady text, then optional dim detail.
///
/// 🔴 This is the ONLY sanctioned way to put a pulse next to words. `text` is rendered in the
/// mark's colour, bold, and **without** the pulse style — see this module's header for why.
pub(crate) fn headline(
    mark: Mark,
    text: &str,
    detail: &str,
    palette: &Palette,
    tick: u64,
    pulse_enabled: bool,
) -> Line<'static> {
    let mut spans = vec![
        mark.span(palette, tick, pulse_enabled),
        Span::styled(
            text.to_string(),
            Style::default()
                .fg(mark.colour(palette))
                .add_modifier(Modifier::BOLD),
        ),
    ];
    let detail = detail.trim();
    if !detail.is_empty() {
        spans.push(Span::styled(
            format!("  ·  {detail}"),
            Style::default().fg(palette.dim),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    /// 🔴 THE PROPERTY NOBODY GUARDED, WHICH IS EXACTLY WHY THE ROW PULSED.
    ///
    /// Both halves are asserted, because either alone is passable by a broken renderer: the text
    /// must hold ONE colour across the whole cycle, and the mark must NOT — a version that simply
    /// stopped pulsing everything would satisfy the first half and fail the second.
    #[test]
    fn only_the_mark_pulses_the_words_never_do() {
        let palette = ScreenTheme::Dark.palette();
        for mark in [
            Mark::Landed,
            Mark::Blocked,
            Mark::InFlight,
            Mark::Queued,
            Mark::Refused,
        ] {
            let steady = mark.colour(&palette);
            let mut mark_colours = std::collections::HashSet::new();
            for tick in 0..56 {
                let line = headline(mark, "Gate refused", "round 1 of 3", &palette, tick, true);
                assert_eq!(
                    line.spans[1].style.fg,
                    Some(steady),
                    "{mark:?} moved its TEXT colour at tick {tick}"
                );
                assert!(
                    !line.spans[1]
                        .style
                        .add_modifier
                        .contains(Modifier::RAPID_BLINK),
                    "{mark:?} text blinked at tick {tick}"
                );
                mark_colours.insert(line.spans[0].style.fg);
            }
            assert_eq!(
                mark_colours.len(),
                2,
                "{mark:?} did not pulse its MARK across the cycle"
            );
        }
    }

    #[test]
    fn reduced_motion_keeps_both_the_mark_and_the_words_legible() {
        let palette = ScreenTheme::Dark.palette();
        for tick in 0..56 {
            let line = headline(Mark::Refused, "Gate refused", "", &palette, tick, false);
            assert_eq!(line.spans[0].style.fg, Some(palette.red));
            assert_eq!(line.spans[1].style.fg, Some(palette.red));
        }
    }

    #[test]
    fn the_plan_vocabulary_is_separate_from_the_rail_vocabulary() {
        let palette = ScreenTheme::Dark.palette();
        // `▲` is the one glyph the two lists share, and it means the same thing in both.
        assert_eq!(StepMark::Blocked.glyph(), Mark::Blocked.glyph());
        assert_eq!(
            StepMark::Blocked.colour(&palette),
            Mark::Blocked.colour(&palette)
        );
        // Everything else differs, because a plan step and a rail row are in different tenses.
        assert_ne!(StepMark::Done.glyph(), Mark::Landed.glyph());
        assert_ne!(StepMark::NotStarted.glyph(), Mark::Queued.glyph());
        assert_ne!(StepMark::Active.glyph(), Mark::InFlight.glyph());
    }

    #[test]
    fn only_the_active_step_lifts_its_row_and_it_uses_the_tint_role() {
        let palette = ScreenTheme::Dark.palette();
        assert_eq!(
            StepMark::Active.row_background(&palette),
            Some(palette.tint)
        );
        for step in [StepMark::Done, StepMark::Blocked, StepMark::NotStarted] {
            assert_eq!(
                step.row_background(&palette),
                None,
                "{step:?} lifted its row"
            );
        }
    }

    /// ⚠️ An unknown status must not read as finished work. This is the direction the error is
    /// allowed to point in.
    #[test]
    fn an_unrecognised_step_status_is_not_started_never_done() {
        for status in ["", "pending", "queued", "who knows", "COMPLETE"] {
            assert_eq!(
                StepMark::from_status(status),
                StepMark::NotStarted,
                "{status:?}"
            );
        }
        assert_eq!(StepMark::from_status("complete"), StepMark::Done);
        assert_eq!(StepMark::from_status("active"), StepMark::Active);
        assert_eq!(StepMark::from_status("protected"), StepMark::Blocked);
    }

    #[test]
    fn every_mark_is_one_terminal_column_and_no_two_share_a_glyph() {
        let marks = [
            Mark::Landed,
            Mark::Blocked,
            Mark::InFlight,
            Mark::Queued,
            Mark::Refused,
        ];
        let glyphs = marks
            .iter()
            .map(|mark| mark.glyph())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(glyphs.len(), marks.len(), "two marks share a glyph");
        for mark in marks {
            assert_eq!(
                unicode_width::UnicodeWidthStr::width(mark.glyph()),
                1,
                "{mark:?} is not one column"
            );
        }
        for step in [
            StepMark::Done,
            StepMark::Active,
            StepMark::Blocked,
            StepMark::NotStarted,
        ] {
            assert_eq!(
                unicode_width::UnicodeWidthStr::width(step.glyph()),
                1,
                "{step:?} is not one column"
            );
        }
    }

    /// ⚠️ The colours carry the meaning as well as the shapes, so a reader who cannot tell `●`
    /// from `○` at their font size still has the second channel. Two marks sharing a colour would
    /// silently remove it.
    #[test]
    fn no_two_marks_share_a_colour_in_either_theme() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            let colours = [
                Mark::Landed,
                Mark::Blocked,
                Mark::InFlight,
                Mark::Queued,
                Mark::Refused,
            ]
            .iter()
            .map(|mark| format!("{:?}", mark.colour(&palette)))
            .collect::<std::collections::HashSet<_>>();
            assert_eq!(colours.len(), 5, "two marks share a colour");
        }
    }
}
