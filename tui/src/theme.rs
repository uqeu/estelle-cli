//! Shared palette for live production and work-plan surfaces.
//!
//! ⚠️ **RGB IS DELIBERATE IN THIS MODULE, AND ONLY IN THIS MODULE.** The workspace disallows
//! `Color::Rgb` with the reason *"Use ANSI colors, which work better in various terminal themes."*
//! That reason is sound for general UI code and it is why the opt-out is scoped to the one file whose
//! entire job is the product palette: Estelle's identity colours are exact values and an ANSI
//! approximation of them is a different brand.
//!
//! The concern the lint exists for is met a stronger way here: [`ScreenTheme`] carries Dark and Cream
//! variants and the composer picks between them from the DETECTED terminal background
//! (`style::user_message_style_for`), so the palette adapts to the host theme instead of assuming one.
//! Every other module in this crate stays under the lint.
#![allow(clippy::disallowed_methods)]

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, PartialEq)]
pub enum ScreenTheme {
    Dark,
    Cream,
}

pub struct Palette {
    pub ground: Color,
    pub dim: Color,
    pub mid: Color,
    pub bright: Color,
    pub red: Color,
    pub green: Color,
    pub warn: Color,
    pub cite: Color,
    pub plan: Color,
    pub skill: Color,
    pub tint: Color,
    pub diff_add: Color,
    pub diff_del: Color,
    /// 🔴 **THE DIFF GUTTER, DECLARED RATHER THAN TYPED AT THE CALL SITE.**
    ///
    /// The narrow strip carrying the line NUMBER is deliberately stronger than the line ground it
    /// labels — without that separation a hunk reads as one block. That decision is sound and it
    /// was written down; what was wrong is WHERE. Four `Color::from_u32` literals sat inside
    /// `live_renderer::github_diff_lines`, which made the diff pane a second owner of a product
    /// colour, and the design book's colour read-back counted them: **20 cells matching no token**
    /// on `05-proposed-diff`.
    ///
    /// ⚠️ The values are UNCHANGED. This is a move, not a redesign: the role existed in the
    /// product and had never been declared, which is a different defect from a near-miss of a role
    /// that had. A near-miss gets snapped to the token it missed; a missing role gets named.
    pub diff_add_gutter: Color,
    pub diff_del_gutter: Color,
}

impl ScreenTheme {
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                ground: Color::Rgb(0x16, 0x13, 0x0f),
                dim: Color::Rgb(0x6f, 0x6a, 0x5e),
                mid: Color::Rgb(0x94, 0x8e, 0x81),
                bright: Color::Rgb(0xe9, 0xe6, 0xdc),
                red: Color::Rgb(0xc5, 0x24, 0x16),
                green: Color::Rgb(0x5f, 0x9e, 0x6e),
                warn: Color::Rgb(0xc9, 0xa2, 0x27),
                cite: Color::Rgb(0x7f, 0xb3, 0xc8),
                plan: Color::Rgb(0x9f, 0xc4, 0xe0),
                skill: Color::Rgb(0xd4, 0x8f, 0xb0),
                tint: Color::Rgb(0x24, 0x1f, 0x19),
                diff_add: Color::Rgb(0x1b, 0x2e, 0x1d),
                diff_del: Color::Rgb(0x36, 0x1a, 0x18),
                diff_add_gutter: Color::Rgb(0x16, 0x2e, 0x20),
                diff_del_gutter: Color::Rgb(0x4a, 0x22, 0x1d),
            },
            Self::Cream => Palette {
                // 🔴 **FIVE PERCENT DARKER THAN THE WEB CREAM, ON THE FOUNDER'S OWN REPORT.**
                //
                // He read the design book on the light theme and said it *"kind of hurt my eye"* —
                // the terminal fills the WHOLE screen with the ground, where the website only ever
                // paints a page, so the same value is a different amount of light. The instruction
                // was exact: *"Lower the luminance of the light ground only; do not touch ink or
                // red."* So `bright` (#1F1C17, the ink) and `red` are untouched below, and only the
                // two GROUND roles moved: #E9E6DC x 0.95 -> #DDDAD1.
                //
                // ⚠️ **`tint` MOVED WITH IT, AND HAD TO.** `tint` is "one step off the ground" — the
                // band under a selected row and under the active plan step. Darkening the ground
                // alone would have left #DDDAD1 sitting on #DCD7C9, a one-value difference nobody
                // can see, which would have silently deleted every row highlight in the light theme
                // while the change looked like it only touched a background. Both scaled by the
                // same 0.95, so every relationship in the palette is the one he approved.
                //
                // ⚠️ This is the CLI's cream and is now deliberately NOT the web's `#E9E6DC`. A
                // shared name with two correct values is survivable only while somebody has written
                // down that they diverged, which is what this paragraph is for.
                ground: Color::Rgb(0xdd, 0xda, 0xd1),
                dim: Color::Rgb(0x8b, 0x85, 0x78),
                mid: Color::Rgb(0x57, 0x50, 0x43),
                bright: Color::Rgb(0x1f, 0x1c, 0x17),
                red: Color::Rgb(0xb0, 0x21, 0x0f),
                green: Color::Rgb(0x3d, 0x75, 0x50),
                warn: Color::Rgb(0x96, 0x75, 0x1a),
                cite: Color::Rgb(0x38, 0x70, 0x8c),
                plan: Color::Rgb(0x35, 0x6a, 0x8c),
                skill: Color::Rgb(0xb0, 0x6a, 0x8c),
                tint: Color::Rgb(0xd1, 0xcc, 0xbe),
                diff_add: Color::Rgb(0xd2, 0xdf, 0xcc),
                diff_del: Color::Rgb(0xeb, 0xd3, 0xcf),
                diff_add_gutter: Color::Rgb(0xac, 0xee, 0xbb),
                diff_del_gutter: Color::Rgb(0xff, 0xce, 0xcb),
            },
        }
    }
}

pub fn pulse(base: Color, tick: u64, enabled: bool) -> Style {
    let color = if enabled && (tick / 14) % 2 == 1 {
        dampen(base)
    } else {
        base
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn dampen(color: Color) -> Color {
    match color {
        Color::Rgb(red, green, blue) => Color::Rgb(
            (f32::from(red) * 0.55) as u8,
            (f32::from(green) * 0.55) as u8,
            (f32::from(blue) * 0.55) as u8,
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 THE LIGHT GROUND CAME DOWN AND THE INK AND THE RED DID NOT.
    ///
    /// The founder's instruction had three clauses and this test enforces all three separately,
    /// because a change that darkened everything by 5% would satisfy the first and quietly break
    /// the brand — which is the "partial guard reports complete" shape, in a palette.
    ///
    /// ⚠️ Clause 3 is the one nobody would have written: `tint` is the row highlight, and a ground
    /// that moved without it would have left the two values one step apart and deleted every
    /// selected-row band in the light theme. Asserting the GAP rather than the value is what makes
    /// this catch that, whatever the next value turns out to be.
    #[test]
    fn the_light_ground_is_dimmer_and_the_ink_and_red_are_untouched() {
        let cream = ScreenTheme::Cream.palette();

        // 1. The ground came down from the web's #E9E6DC to #DDDAD1 — 5%, hue held.
        assert_eq!(cream.ground, Color::Rgb(0xdd, 0xda, 0xd1));

        // 2. Ink and red are exactly what they were. These are brand values, not theme values.
        assert_eq!(cream.bright, Color::Rgb(0x1f, 0x1c, 0x17));
        assert_eq!(cream.red, Color::Rgb(0xb0, 0x21, 0x0f));

        // 3. The highlight band survived the move. A `tint` left behind would be invisible.
        let gap = |a: Color, b: Color| match (a, b) {
            (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
                u32::from(ar.abs_diff(br)) + u32::from(ag.abs_diff(bg)) + u32::from(ab.abs_diff(bb))
            }
            _ => 0,
        };
        assert!(
            gap(cream.ground, cream.tint) >= 30,
            "the light row highlight is invisible against its own ground: {:?} vs {:?}",
            cream.ground,
            cream.tint
        );
        // The dark theme's own gap is the reference for "visible", and cream must not be worse.
        let dark = ScreenTheme::Dark.palette();
        assert!(gap(cream.ground, cream.tint) >= gap(dark.ground, dark.tint));
    }

    #[test]
    fn pulse_has_two_intensities_on_a_twenty_eight_tick_cycle() {
        let base = Color::Rgb(0xc5, 0x24, 0x16);
        let hot = pulse(base, 0, true);
        let cool = pulse(base, 14, true);
        let next_hot = pulse(base, 28, true);

        assert_ne!(hot.fg, cool.fg);
        assert_eq!(hot.fg, next_hot.fg);
        assert_eq!(hot.fg, Some(base));
    }

    #[test]
    fn pulse_never_uses_rapid_blink_and_reduced_motion_keeps_the_error_visible() {
        let base = Color::Rgb(0xc5, 0x24, 0x16);
        for tick in 0..56 {
            assert!(
                !pulse(base, tick, true)
                    .add_modifier
                    .contains(Modifier::RAPID_BLINK)
            );
        }
        let still = pulse(base, 14, false);
        assert_eq!(still.fg, Some(base));
        assert!(still.add_modifier.contains(Modifier::BOLD));
    }
}
