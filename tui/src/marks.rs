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

use crate::theme::{ATTENTION, DREAD, Palette, PulseShape, pulse_with};

/// The status vocabulary, chosen by the founder from the rendered specimen sheet (set 2A).
///
/// Six marks, six meanings, no synonyms. A seventh state does not get a seventh glyph invented for
/// it here — it gets mapped onto the meaning it actually has, or it gets raised as a gap.
///
/// 🔴 **THIS SAID "FIVE" FOR MONTHS WHILE THE PRODUCT DREW SIX.**
///
/// `?` was on two shipped screens — the orchestra worker table (`orchestra_view::glyph`, for
/// `Lost | NeedsInput | Unknown`) and the todo ledger (`commands.rs`, for `TodoStatus::Unknown`) —
/// as a bare string literal in each, in neither enum, in neither test. The docstring above it was
/// not describing the product; it was describing the file. **A name that overclaims its own body is
/// the documentation form of the inert guard**: a reader who wants to know the mark vocabulary
/// reads "five marks, no synonyms" and stops, and the count is never re-measured.
///
/// ⚠️ **AND `?` COULD NOT HAVE BEEN MAPPED ONTO ONE OF THE FIVE.** It is not landed, not blocked,
/// not in flight, not queued and not refused — it means *the server did not tell us*, which is a
/// genuinely sixth meaning. Folding it into `Queued` would have made every unreported worker look
/// idle, and into `Blocked` would have called for a human who is not needed. The honest fix for a
/// sixth meaning is a sixth name, and the honest fix for a stale count is to change the number the
/// same day the fact changes.
///
/// Its colour is [`Palette::mid`] and not [`Palette::warn`]: unknown is the ABSENCE of a signal,
/// not a call for attention, and `warn` already means "a human is needed here".
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
    /// `?` unknown — the server did not report a state for this row
    Unknown,
}

impl Mark {
    /// 🔴 **EVERY TEST IN THIS MODULE ITERATES THIS, AND THAT IS THE POINT.**
    ///
    /// The three property tests below each carried their own hand-written copy of the five
    /// variants. A sixth mark added to the enum would have compiled, shipped, and been covered by
    /// none of them — which is exactly how `?` reached two screens with no test and no name. One
    /// list, one place to forget, and forgetting it is a compile error rather than a silent gap.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const ALL: [Self; 6] = [
        Self::Landed,
        Self::Blocked,
        Self::InFlight,
        Self::Queued,
        Self::Refused,
        Self::Unknown,
    ];

    pub(crate) const fn glyph(self) -> &'static str {
        match self {
            Self::Landed => "●",
            Self::Blocked => "▲",
            Self::InFlight => "◐",
            Self::Queued => "○",
            Self::Refused => "■",
            Self::Unknown => "?",
        }
    }

    pub(crate) fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Landed => palette.green,
            Self::Blocked => palette.warn,
            Self::InFlight => palette.cite,
            Self::Queued => palette.dim,
            Self::Refused => palette.red,
            Self::Unknown => palette.mid,
        }
    }

    /// 🔴 **A REFUSAL PULSES DIFFERENTLY FROM EVERYTHING ELSE, AND THAT IS THE POINT.**
    ///
    /// The founder, 2026-09-02: *"Anything that's red and an error should be pulsing — a very slow
    /// pulse, DOOM… DOOM… DOOM — that's like 'oh shit, I should look at that'."* A working spinner
    /// and a blocked merge were sharing one 1.4s cycle, so the loudest thing on the screen moved at
    /// exactly the speed of the most routine.
    ///
    /// ⚠️ **ONLY `Refused` GETS IT, AND `Blocked` DELIBERATELY DOES NOT.** `Refused` is the red
    /// one — the gate saying no. `Blocked` is `warn`, and it means *needs a human*, which is a
    /// request rather than an alarm; giving both the dread pulse would spend the loudest signal we
    /// have on the second-loudest thing and neither would read as urgent afterwards. His words were
    /// "red AND an error", and this is the only mark that is both.
    ///
    /// ⚠️ The choice lives HERE rather than at the call sites because the last time a pulse
    /// decision was made per-call-site, five of them pulsed the words instead of the mark.
    pub(crate) fn shape(self) -> PulseShape {
        match self {
            Self::Refused => DREAD,
            _ => ATTENTION,
        }
    }

    /// The mark on its own, pulsing when asked. Every other span on the row stays steady.
    pub(crate) fn span(self, palette: &Palette, tick: u64, pulse_enabled: bool) -> Span<'static> {
        Span::styled(
            format!("{} ", self.glyph()),
            pulse_with(self.shape(), self.colour(palette), tick, pulse_enabled),
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
        for mark in Mark::ALL {
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

    /// 🔴 **THE REFUSAL IS THE ONLY MARK ON THE DREAD PULSE, AND IT IS ASSERTED BOTH WAYS.**
    ///
    /// One half says `Refused` uses the slow heavy shape. The other says every other mark does
    /// NOT — without it, a change that gave the whole vocabulary the dread pulse would pass, and
    /// the loudest signal on the screen would have been spent on a queued row.
    ///
    /// ⚠️ It asserts on the RENDERED span at a tick where the two shapes disagree, not only on
    /// `shape()`. `shape()` returning the right constant proves nothing if `span` stops reading it.
    #[test]
    fn only_the_refusal_mark_pulses_slowly() {
        assert_eq!(Mark::Refused.shape(), DREAD);
        for mark in Mark::ALL {
            if mark == Mark::Refused {
                continue;
            }
            assert_eq!(mark.shape(), ATTENTION, "{mark:?} took the dread pulse");
        }

        // Tick 20 is inside ATTENTION's second (damped) half and inside DREAD's long dark, so it
        // cannot separate them. Tick 30 can: ATTENTION is back at full strength (30 % 28 == 2),
        // DREAD is still dark (30 % 56 == 30). A mark that quietly reverted to the fast cycle
        // shows full colour here.
        let palette = ScreenTheme::Dark.palette();
        let refused = Mark::Refused.span(&palette, 30, true);
        assert_ne!(
            refused.style.fg,
            Some(palette.red),
            "the refusal was at full strength 1.5s into its cycle — that is the fast pulse"
        );
        let queued = Mark::Queued.span(&palette, 30, true);
        assert_eq!(
            queued.style.fg,
            Some(palette.dim),
            "a queued row is on the ordinary pulse and should be lit at tick 30"
        );
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
        let marks = Mark::ALL;
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
            let colours = Mark::ALL
                .iter()
                .map(|mark| format!("{:?}", mark.colour(&palette)))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(colours.len(), Mark::ALL.len(), "two marks share a colour");
        }
    }
}
