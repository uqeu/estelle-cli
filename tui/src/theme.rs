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
            },
            Self::Cream => Palette {
                ground: Color::Rgb(0xe9, 0xe6, 0xdc),
                dim: Color::Rgb(0x8b, 0x85, 0x78),
                mid: Color::Rgb(0x57, 0x50, 0x43),
                bright: Color::Rgb(0x1f, 0x1c, 0x17),
                red: Color::Rgb(0xb0, 0x21, 0x0f),
                green: Color::Rgb(0x3d, 0x75, 0x50),
                warn: Color::Rgb(0x96, 0x75, 0x1a),
                cite: Color::Rgb(0x38, 0x70, 0x8c),
                plan: Color::Rgb(0x35, 0x6a, 0x8c),
                skill: Color::Rgb(0xb0, 0x6a, 0x8c),
                tint: Color::Rgb(0xdc, 0xd7, 0xc9),
                diff_add: Color::Rgb(0xd2, 0xdf, 0xcc),
                diff_del: Color::Rgb(0xeb, 0xd3, 0xcf),
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
