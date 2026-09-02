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
    /// The narrow strip carrying the line NUMBER, so a hunk has an edge rather than reading as one
    /// block. Four `Color::from_u32` literals sat inside `live_renderer::github_diff_lines`, which
    /// made the diff pane a second owner of a product colour, and the design book's colour
    /// read-back counted them: **20 cells matching no token** on `05-proposed-diff`.
    ///
    /// 🔴 **THIS DOC USED TO SAY THE GUTTER IS "DELIBERATELY STRONGER THAN THE LINE GROUND IT
    /// LABELS", AND ON THE DARK THEME THAT WAS NOT TRUE.** Measured 2026-09-02: dark
    /// `diff_add_gutter` against dark `diff_add` is **1.006:1** — the same colour, to a reader.
    /// The sentence had been sitting on the field it described, unfalsifiable by reading it, since
    /// the role was declared. The dark value is left alone here because nobody has asked for the
    /// dark diff to change and a redesign is not a comment fix; **the claim is corrected instead,
    /// and the number is written where the claim was.** The cream pair, re-seated in the same pass,
    /// does keep the separation: 1.184:1 (add) and 1.227:1 (del).
    ///
    /// ⚠️ **THE COINCIDENCE WITH `diff_render.rs` HAS ENDED, ON PURPOSE.** Until 2026-09-02 the
    /// cream values here were byte-identical to `diff_render.rs:67-68`
    /// (`LIGHT_TC_ADD_NUM_BG_RGB` / `..._DEL_...`), because both were GitHub's light diff colours.
    /// They were never one owner: `diff_render` is a separate rich-diff renderer with its own
    /// truecolor/256/ANSI ladder and its own reason to look like GitHub. **These roles are the
    /// Estelle CREAM theme's, and GitHub's pastels are seated on GitHub's WHITE page — on the
    /// #DDDAD1 ground they measured 1.045 and 1.005 from the paper.** So this pair is re-seated
    /// and that pair is left alone; if the rich-diff renderer is ever measured on our own ground it
    /// will need the same treatment, and that is a change to `diff_render.rs`, not to this file.
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
                //
                // ── 2026-09-02 · EVERY ACCENT BELOW WAS RE-SEATED FOR A LIGHT GROUND ───────────
                //
                // 🔴 **THE FOUNDER'S RULE, AND IT IS GENERAL:** *"On the cream ground, every
                // lighter colour becomes a DARKER version of itself. Cream is a light background;
                // a pale accent has nothing to sit against."* He said it about the skill pink and
                // it was true of six roles.
                //
                // ⚠️ **THE PALETTE WAS NOT DESIGNED FOR CREAM, IT WAS TRANSLATED INTO IT.** Every
                // accent had been carried over at roughly its DARK-theme lightness, and measured
                // against its own ground it read WEAKER on cream than the same role reads on dark:
                //
                //     role    cream was          dark      after
                //     dim     #8B8578  2.62      3.44      #645F56  4.53
                //     skill   #B06A8C  2.84      7.37      #6F2046  7.62
                //     warn    #96751A  3.09      7.65      #5C4810  6.29
                //     cite    #38708C  3.89      8.10      #264B5E  6.68
                //     green   #3D7550  3.89      5.82      #2D553A  6.08
                //     plan    #356A8C  4.18     10.09      #1B3247  9.43
                //
                // Five of those six failed WCAG AA for normal text on their own ground, and two
                // failed 3:1. `mid` (5.70 vs 5.68) and `red` (4.89 vs 3.21) were already right and
                // are untouched. `the_cream_accents_are_never_weaker_than_their_dark_twins` is the
                // rule as an assertion, so this cannot quietly revert one value at a time.
                //
                // 🔴 **`skill` IS THE DARK MAROON HE ASKED FOR.** *"If it's pink then you can make
                // it dark red. Dark red? For the pink, so it's like a dark maroon."* #6F2046 is
                // 7.62:1 on cream against #B06A8C's 2.84:1, and it is 85 RGB-units from `red`, so
                // the two roles cannot be read as each other.
                //
                // 🔴 **`cite` AND `plan` WERE THE SAME COLOUR AND NOBODY COULD HAVE SEEN IT.**
                // #38708C and #356A8C differ by (3, 6, 0) — one value, rendered twice under two
                // role names, for as long as the cream theme has existed. On dark they are 43
                // RGB-units apart with `plan` the PALER of the two; applying the rule inverts that
                // relationship rather than deleting it, so on cream `plan` is now the deeper blue,
                // 36 units from `cite`. ⚠️ `marks.rs`'s no-two-marks-share-a-colour test could not
                // have caught this: `plan` is not a `Mark`.
                ground: Color::Rgb(0xdd, 0xda, 0xd1),
                dim: Color::Rgb(0x64, 0x5f, 0x56),
                mid: Color::Rgb(0x57, 0x50, 0x43),
                bright: Color::Rgb(0x1f, 0x1c, 0x17),
                red: Color::Rgb(0xb0, 0x21, 0x0f),
                green: Color::Rgb(0x2d, 0x55, 0x3a),
                warn: Color::Rgb(0x5c, 0x48, 0x10),
                cite: Color::Rgb(0x26, 0x4b, 0x5e),
                plan: Color::Rgb(0x1b, 0x32, 0x47),
                skill: Color::Rgb(0x6f, 0x20, 0x46),
                tint: Color::Rgb(0xd1, 0xcc, 0xbe),
                // 🔴 **THE DIFF BANDS WERE AT 1.01:1 AGAINST THE PAGE — A HUNK WITH NO EDGE.**
                //
                // The same rule, on the roles where it did the most damage. `diff_add` #D2DFCC sat
                // 1.010 from the cream ground and `diff_del_gutter` #FFCECB sat 1.005 — on the dark
                // theme the same four roles sit at 1.28 / 1.16 / 1.28 / 1.36. A reader on cream was
                // being shown an added line and a deleted line as two invisible rectangles.
                // Darkened to the dark theme's own separations, with cream ink still ≥ 7.9:1 on
                // every one of them.
                diff_add: Color::Rgb(0xb1, 0xc8, 0xa7),
                diff_del: Color::Rgb(0xe4, 0xc3, 0xbe),
                diff_add_gutter: Color::Rgb(0x8f, 0xbe, 0x85),
                diff_del_gutter: Color::Rgb(0xef, 0xa4, 0x9e),
            },
        }
    }
}

/// One beat of a pulse: how long the whole cycle is, how much of it is lit, and how far the
/// unlit half falls. A `tick` is 50ms (`live_renderer::pulse_tick`), so the periods below are
/// stated in ticks and read in seconds.
///
/// 🔴 **TWO PROFILES, ONE PIECE OF ARITHMETIC.** The founder asked for errors to read differently
/// from everything else: *"anything that's red and an error should be pulsing — a very slow pulse,
/// DOOM… DOOM… DOOM — that's like 'oh shit, I should look at that'."* That is a different FEELING,
/// not a different mechanism, and giving it a second `if tick % n` somewhere else in the tree is
/// how two owners of one derived fact start disagreeing about what "pulsing" means. So the shape
/// is a value and [`pulse_with`] is the only place the clock is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PulseShape {
    /// Ticks in one full cycle.
    period: u64,
    /// Ticks at full strength at the START of each cycle. The rest of the cycle is damped.
    beat: u64,
    /// What the damped part of the cycle keeps, in percent of each channel.
    keep_percent: u16,
}

/// The ordinary attention pulse: a 1.4s cycle, evenly split. This is what a live mark does.
pub const ATTENTION: PulseShape = PulseShape {
    period: 28,
    beat: 14,
    keep_percent: 55,
};

/// 🔴 **DREAD. The pulse a refusal gets, and nothing else.**
///
/// Twice the period (2.8s) and a SHORT beat — 0.5s lit against 2.3s dark — because an even split
/// reads as an animation and a long dark gap reads as a heartbeat. The trough is deeper than
/// [`ATTENTION`]'s (40% against 55%) so the swell is heavy rather than a flicker. Slower and
/// heavier, in both axes, is the whole request.
///
/// ⚠️ **STILL NEVER [`Modifier::RAPID_BLINK`].** A pulse this slow is legible to a reader with
/// vestibular sensitivity in a way a blink is not, and with `enabled == false` (reduced motion)
/// the colour holds at full strength — the error stays VISIBLE, it just stops moving.
pub const DREAD: PulseShape = PulseShape {
    period: 56,
    beat: 10,
    keep_percent: 40,
};

/// The ordinary attention pulse. Kept as a free function because every mark calls it by name.
pub fn pulse(base: Color, tick: u64, enabled: bool) -> Style {
    pulse_with(ATTENTION, base, tick, enabled)
}

/// The slow, heavy pulse a refusal or an error state gets.
pub fn dread(base: Color, tick: u64, enabled: bool) -> Style {
    pulse_with(DREAD, base, tick, enabled)
}

/// 🔴 **THE ONE OWNER OF "WHAT COLOUR IS THIS PULSE SHOWING RIGHT NOW".**
///
/// ⚠️ The comparison is `>=`, not `>`: with `beat` ticks lit, the lit window is `0..beat`, so tick
/// `beat` itself must already be dark. Written the other way the beat is one tick long at every
/// call site that passes `beat = 1`, and nothing would ever have gone red.
pub fn pulse_with(shape: PulseShape, base: Color, tick: u64, enabled: bool) -> Style {
    let color = if enabled && tick % shape.period >= shape.beat {
        dampen(base, shape.keep_percent)
    } else {
        base
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn dampen(color: Color, keep_percent: u16) -> Color {
    let scale =
        |channel: u8| u8::try_from(u16::from(channel) * keep_percent / 100).unwrap_or(u8::MAX);
    match color {
        Color::Rgb(red, green, blue) => Color::Rgb(scale(red), scale(green), scale(blue)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.x relative luminance of one sRGB channel.
    fn channel(value: u8) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG 2.x contrast ratio. Panics on a non-RGB colour, because every value in this module is
    /// one and a silent 1.0 would make the guard below pass over a role somebody turned into ANSI.
    fn contrast(a: Color, b: Color) -> f64 {
        let luminance = |color: Color| match color {
            Color::Rgb(r, g, b) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b),
            other => panic!("{other:?} is not an RGB token"),
        };
        let (mut hi, mut lo) = (luminance(a), luminance(b));
        if hi < lo {
            std::mem::swap(&mut hi, &mut lo);
        }
        (hi + 0.05) / (lo + 0.05)
    }

    /// 🔴 **THE FOUNDER'S CREAM RULE, AS AN ASSERTION RATHER THAN A PARAGRAPH.**
    ///
    /// *"On the cream ground, every lighter colour becomes a DARKER version of itself. Cream is a
    /// light background; a pale accent has nothing to sit against."*
    ///
    /// The measurable form: **a role must not read weaker on cream than the same role reads on
    /// dark**, each measured against its OWN ground. Six of the eight accent roles failed it the
    /// day it was written (`dim` 2.62 vs 3.44, `skill` 2.84 vs 7.37, `warn` 3.09 vs 7.65,
    /// `cite` 3.89 vs 8.10, `green` 3.89 vs 5.82, `plan` 4.18 vs 10.09) and five of the six also
    /// failed WCAG AA outright.
    ///
    /// 🔴 **"WEAKER" IS MEASURED AS A FRACTION OF THE THEME'S OWN HEADROOM, NOT AS A RAW RATIO,
    /// AND THAT IS NOT A LOOPHOLE — IT IS THE ONLY VERSION OF THE RULE THAT MEANS ANYTHING.**
    /// A raw ratio is not comparable across grounds: the dark ground is near-black (L = 0.073) so
    /// its ink reaches 14.83, while the cream ground (L = 0.843) caps its ink at 12.15. Demanding
    /// raw parity would force cream `warn` past #4C3B0D, a near-black olive that has stopped being
    /// a caution colour — i.e. the rule would destroy the hue it exists to protect. So each role is
    /// scored as `contrast(role, ground) / contrast(ink, ground)`: **what fraction of the light
    /// this theme actually has does this role use.** Both themes are then on one scale.
    ///
    /// ⚠️ **THE ABSOLUTE FLOOR IS ASSERTED TOO.** A ratio-of-ratios can be satisfied by two equally
    /// bad values, so WCAG AA (4.5:1) is checked independently. Neither clause alone is the rule.
    ///
    /// ⚠️ **`bright` IS EXEMPT AND THE EXEMPTION IS WRITTEN DOWN, NOT SILENT.** It is the ink and
    /// it is the denominator — it scores 1.000 in both themes by construction, so including it
    /// would be an assertion that cannot fail. It is checked against an absolute floor instead.
    #[test]
    fn the_cream_accents_are_never_weaker_than_their_dark_twins() {
        let cream = ScreenTheme::Cream.palette();
        let dark = ScreenTheme::Dark.palette();
        let cream_headroom = contrast(cream.bright, cream.ground);
        let dark_headroom = contrast(dark.bright, dark.ground);

        let roles: [(&str, Color, Color); 8] = [
            ("dim", cream.dim, dark.dim),
            ("mid", cream.mid, dark.mid),
            ("red", cream.red, dark.red),
            ("green", cream.green, dark.green),
            ("warn", cream.warn, dark.warn),
            ("cite", cream.cite, dark.cite),
            ("plan", cream.plan, dark.plan),
            ("skill", cream.skill, dark.skill),
        ];
        for (name, on_cream, on_dark) in roles {
            let light = contrast(on_cream, cream.ground);
            let night = contrast(on_dark, dark.ground);
            assert!(
                light / cream_headroom >= night / dark_headroom,
                "cream `{name}` uses {:.3} of the light theme's headroom where dark uses {:.3} \
                 ({light:.2}:1 against {night:.2}:1) — a pale accent on a light ground has \
                 nothing to sit against",
                light / cream_headroom,
                night / dark_headroom
            );
            assert!(
                light >= 4.5,
                "cream `{name}` is {light:.2}:1, under WCAG AA for normal text"
            );
        }

        // The written exemption: the ink is the denominator above, so it is checked absolutely.
        assert!(
            cream_headroom >= 7.0,
            "cream ink is only {cream_headroom:.2}:1"
        );
        assert!(
            dark_headroom >= 7.0,
            "dark ink is only {dark_headroom:.2}:1"
        );
    }

    /// 🔴 **A BAND IS A BACKGROUND, SO ITS TEST IS SEPARATION, NOT LEGIBILITY.**
    ///
    /// Same rule, the half a foreground check cannot see. On cream, `diff_add` sat **1.010:1**
    /// from the page and `diff_del_gutter` at **1.005:1** — an added line and a deleted line drawn
    /// as invisible rectangles, while every WCAG check on the text sitting inside them passed.
    /// The dark theme's own separations are the reference for "visible".
    #[test]
    fn the_cream_bands_separate_from_their_ground_as_well_as_the_dark_ones_do() {
        let cream = ScreenTheme::Cream.palette();
        let dark = ScreenTheme::Dark.palette();

        let bands: [(&str, Color, Color); 5] = [
            ("tint", cream.tint, dark.tint),
            ("diff_add", cream.diff_add, dark.diff_add),
            ("diff_del", cream.diff_del, dark.diff_del),
            (
                "diff_add_gutter",
                cream.diff_add_gutter,
                dark.diff_add_gutter,
            ),
            (
                "diff_del_gutter",
                cream.diff_del_gutter,
                dark.diff_del_gutter,
            ),
        ];
        for (name, on_cream, on_dark) in bands {
            let light = contrast(on_cream, cream.ground);
            let night = contrast(on_dark, dark.ground);
            assert!(
                light >= night * 0.99,
                "cream `{name}` is {light:.3} from its ground where dark is {night:.3} — the band \
                 has no edge on the light theme"
            );
            // ⚠️ THE OTHER DIRECTION, AND IT IS NOT DECORATION. Darkening a band until it separates
            // is trivial; darkening it until the ink on top stops being readable is the way that
            // fix goes wrong, and nothing above would notice.
            assert!(
                contrast(cream.bright, on_cream) >= 7.0,
                "cream ink is {:.2}:1 on `{name}` — the band ate its own text",
                contrast(cream.bright, on_cream)
            );
        }
    }

    /// 🔴 **TWO ROLE NAMES RENDERING ONE COLOUR IS A ROLE THAT DOES NOT EXIST.**
    ///
    /// Cream `cite` was #38708C and cream `plan` was #356A8C — a difference of (3, 6, 0), which is
    /// no difference. `marks.rs::no_two_marks_share_a_colour_in_either_theme` could not catch it
    /// because `plan` is not a `Mark`, so the two had been the same value for the life of the
    /// theme with every guard green. This asserts over the ACCENT roles, in both themes.
    #[test]
    fn no_two_accent_roles_render_the_same_colour_in_either_theme() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            let accents: [(&str, Color); 7] = [
                ("dim", palette.dim),
                ("mid", palette.mid),
                ("red", palette.red),
                ("green", palette.green),
                ("warn", palette.warn),
                ("cite", palette.cite),
                ("plan", palette.plan),
            ];
            for (index, (a_name, a)) in accents.iter().enumerate() {
                for (b_name, b) in &accents[index + 1..] {
                    let apart = match (a, b) {
                        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
                            f64::from(ar.abs_diff(*br)).powi(2)
                                + f64::from(ag.abs_diff(*bg)).powi(2)
                                + f64::from(ab.abs_diff(*bb)).powi(2)
                        }
                        _ => 0.0,
                    }
                    .sqrt();
                    assert!(
                        apart >= 24.0,
                        "`{a_name}` and `{b_name}` are {apart:.1} RGB-units apart — one colour \
                         under two role names"
                    );
                }
            }
        }
    }

    /// 🔴 **A GUTTER MUST SEPARATE FROM THE BAND IT LABELS, AND ON DARK IT DOES NOT.**
    ///
    /// This is the claim the field's own doc comment used to make about itself. Measured: the dark
    /// add pair is 1.006:1 — one colour. The test asserts the CREAM pair, which this pass fixed,
    /// and records the dark number rather than asserting it, because changing the dark diff is a
    /// redesign nobody asked for and a guard that goes red on untouched code gets suppressed.
    /// ⚠️ When the dark pair is re-seated, delete the `dark_add` line and add it to the loop.
    #[test]
    fn the_cream_gutter_is_stronger_than_the_band_it_labels() {
        let cream = ScreenTheme::Cream.palette();
        for (name, gutter, band) in [
            ("add", cream.diff_add_gutter, cream.diff_add),
            ("del", cream.diff_del_gutter, cream.diff_del),
        ] {
            let separation = contrast(gutter, band);
            assert!(
                separation >= 1.1,
                "the cream {name} gutter is {separation:.3} from its own band — no edge"
            );
        }

        // The dark pair, measured and NOT asserted. 1.006 is the number the old doc comment
        // claimed was a separation; it is written here so the next reader finds it as a fact
        // rather than as a sentence.
        let dark = ScreenTheme::Dark.palette();
        let dark_add = contrast(dark.diff_add_gutter, dark.diff_add);
        assert!(
            (dark_add - 1.006).abs() < 0.01,
            "the dark add gutter moved off its measured 1.006 — update the note on `diff_add_gutter`"
        );
    }

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
            assert!(
                !dread(base, tick, true)
                    .add_modifier
                    .contains(Modifier::RAPID_BLINK)
            );
        }
        let still = pulse(base, 14, false);
        assert_eq!(still.fg, Some(base));
        assert!(still.add_modifier.contains(Modifier::BOLD));
        // Reduced motion holds the DREAD pulse at full strength too. An error that stops moving
        // must not also stop being red — that would trade one accessibility problem for a worse one.
        let held = dread(base, 30, false);
        assert_eq!(held.fg, Some(base));
    }

    /// 🔴 **DREAD IS SLOWER AND HEAVIER THAN ATTENTION, IN BOTH AXES, AND BOTH ARE ASSERTED.**
    ///
    /// The founder's words were *"a very slow pulse, DOOM… DOOM… DOOM"*, which is two separate
    /// claims: a longer gap between beats, and a deeper trough. A test that checked only the period
    /// would pass over a dread pulse that swelled and fell exactly as gently as a live mark — the
    /// half of his instruction that carries the feeling.
    ///
    /// ⚠️ The dark fraction is asserted rather than the beat length, because the beat is what makes
    /// this read as a heartbeat rather than a blinker: an even split at any period is an animation.
    #[test]
    fn dread_is_slower_and_deeper_than_the_attention_pulse() {
        assert!(
            DREAD.period > ATTENTION.period,
            "dread must have the longer cycle"
        );
        assert!(
            DREAD.keep_percent < ATTENTION.keep_percent,
            "dread must fall further between beats"
        );

        // ATTENTION is an even split; DREAD is mostly dark. Stated as a fraction so the numbers
        // can move without the SHAPE quietly becoming a blinker again.
        assert_eq!(ATTENTION.beat * 2, ATTENTION.period);
        assert!(
            DREAD.beat * 4 < DREAD.period,
            "dread's lit beat is {} of {} ticks — that is a blink, not a heartbeat",
            DREAD.beat,
            DREAD.period
        );

        // And it actually moves: full strength on the beat, damped through the long dark.
        let base = Color::Rgb(0xc5, 0x24, 0x16);
        assert_eq!(dread(base, 0, true).fg, Some(base));
        assert_eq!(dread(base, DREAD.beat - 1, true).fg, Some(base));
        assert_ne!(dread(base, DREAD.beat, true).fg, Some(base));
        assert_ne!(dread(base, DREAD.period - 1, true).fg, Some(base));
        assert_eq!(dread(base, DREAD.period, true).fg, Some(base));
    }

    /// 🔴 **THE REFACTOR THAT INTRODUCED `PulseShape` MUST NOT HAVE MOVED THE ORDINARY PULSE.**
    ///
    /// Every live mark in the product calls `pulse`, and its timing was `(tick / 14) % 2 == 1`
    /// before the shape became a value. This pins the OLD arithmetic against the new one over two
    /// full cycles, so a rewrite that also silently retimed every rail row goes red here rather
    /// than being noticed in a screenshot.
    #[test]
    fn the_attention_pulse_kept_the_timing_it_had_before_the_shape_became_a_value() {
        let base = Color::Rgb(0x5f, 0x9e, 0x6e);
        for tick in 0..56 {
            let was_damped_before = (tick / 14) % 2 == 1;
            let is_damped_now = pulse(base, tick, true).fg != Some(base);
            assert_eq!(
                was_damped_before, is_damped_now,
                "tick {tick} changed meaning when the pulse was refactored"
            );
        }
    }
}
