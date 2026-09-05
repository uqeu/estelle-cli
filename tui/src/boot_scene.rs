//! Estelle's transient boot landscape.
//!
//! This module is deliberately independent of terminal setup and application
//! state. Callers provide elapsed milliseconds and a Ratatui buffer; the scene
//! never writes to stdout or stderr. Its geometry and timing mirror the web
//! `BootLoader`: four rolling ridges condense out of ordered noise, hold, then
//! resolve through coarse Bayer patches into whatever the caller rendered
//! underneath.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;

pub const PLAY_MS: u64 = 1_150;
pub const EXIT_MS: u64 = 440;
pub const EXIT_SKIP_MS: u64 = 200;
pub const FAIL_MS: u64 = 3_000;

/// The landscape is fully condensed at 62% of the play phase, then holds calm.
pub const CONDENSE_MS: u64 = PLAY_MS * 62 / 100;

pub const TIPS: [&str; 5] = [
    "Grounded memory: every answer cited to file and line.",
    "The gate blocks any answer that invents an API, before it is shown.",
    "A swarm fans work across agents; every result returns through the gate.",
    "Context that never runs out: a 4,000-turn session held under 800 tokens.",
    "One merge gate: grounding, secrets, static analysis, CVEs. Pass or block.",
];

const BAYER_8: [u8; 64] = [
    0, 32, 8, 40, 2, 34, 10, 42, 48, 16, 56, 24, 50, 18, 58, 26, 12, 44, 4, 36, 14, 46, 6, 38, 60,
    28, 52, 20, 62, 30, 54, 22, 3, 35, 11, 43, 1, 33, 9, 41, 51, 19, 59, 27, 49, 17, 57, 25, 15,
    47, 7, 39, 13, 45, 5, 37, 63, 31, 55, 23, 61, 29, 53, 21,
];

#[derive(Clone, Copy, Debug)]
struct Ridge {
    y: f64,
    amplitude: f64,
    alpha: f64,
    frequency_1: f64,
    frequency_2: f64,
    phase: f64,
}

// Exact BootLoader far-to-near ridge parameters.
const RIDGES: [Ridge; 4] = [
    Ridge {
        y: 0.56,
        amplitude: 0.03,
        alpha: 0.12,
        frequency_1: 5.1,
        frequency_2: 11.7,
        phase: 1.2,
    },
    Ridge {
        y: 0.66,
        amplitude: 0.042,
        alpha: 0.20,
        frequency_1: 3.9,
        frequency_2: 9.3,
        phase: 4.0,
    },
    Ridge {
        y: 0.77,
        amplitude: 0.054,
        alpha: 0.32,
        frequency_1: 3.1,
        frequency_2: 7.9,
        phase: 2.3,
    },
    Ridge {
        y: 0.895,
        amplitude: 0.062,
        alpha: 0.52,
        frequency_1: 2.4,
        frequency_2: 6.1,
        phase: 5.4,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootPalette {
    Dark,
    Light,
}

/// 🔴 **THE BOOT SCREEN DREW 1,019 CELLS AND READ NOT ONE DESIGN TOKEN.**
///
/// Four colours lived here — a ground, a faint dither dot, a bright foreground and the lily's red —
/// and every one of them was a NEAR MISS of a token `theme::Palette` already ships:
///
/// | boot, before   | dark        | the token it was almost | delta |
/// |----------------|-------------|-------------------------|-------|
/// | `bone`         | `#17140F`   | `Palette::ground`  `#16130F` | 1·1·0 |
/// | `ghost`        | `#605A4E`   | `Palette::dim`     `#6F6A5E` | 15·16·16 |
/// | `ink`          | `#D6D1C5`   | `Palette::bright`  `#E9E6DC` | 19·21·23 |
/// | lily           | `#C91A0C`   | `Palette::red`     `#C52416` | 4·10·10 |
///
/// The light row was worse than a near miss: `#F1EFE9` is the cream this repo **replaced** — the
/// stale value `DESIGN.md` still printed after `globals.css` had moved on — so the first screen a
/// new user ever sees was painted in a colour nothing else in the product uses.
///
/// ⚠️ **WHY THE VALUES ARE STILL WRITTEN HERE RATHER THAN IMPORTED.** `boot_scene` is in the
/// `estelle_tui` LIBRARY and `theme` is declared in the `estelle` BINARY, so this module cannot see
/// it. Moving `theme` into the library is the right fix and is a bigger change than this one; until
/// then the binary owns a test — `the_boot_screen_paints_in_the_products_own_tokens` in `main.rs`,
/// which is the only place both modules are visible — that asserts each of the four EQUALS its
/// token. Drift is therefore a red test rather than a thing somebody notices in a screenshot.
impl BootPalette {
    /// The ground the whole field is painted on. `theme::Palette::ground`.
    pub fn bone(self) -> Color {
        match self {
            Self::Dark => Color::from_u32(0x16_13_0F),
            Self::Light => Color::from_u32(0xDD_DA_D1),
        }
    }

    /// The faint dither dot, the byline and `skip`. `theme::Palette::dim`.
    pub fn ghost(self) -> Color {
        match self {
            Self::Dark => Color::from_u32(0x6F_6A_5E),
            Self::Light => Color::from_u32(0x64_5F_56),
        }
    }

    /// The dense dither, the wordmark and the tip. `theme::Palette::bright`.
    pub fn ink(self) -> Color {
        match self {
            Self::Dark => Color::from_u32(0xE9_E6_DC),
            Self::Light => Color::from_u32(0x1F_1C_17),
        }
    }

    /// The higanbana. `theme::Palette::red`.
    ///
    /// ⚠️ This value was an inline literal at FIVE call sites — one that painted it and four that
    /// searched the buffer for it to assert the flower had been drawn. A colour with five owners is
    /// four chances to change it in one place and silently break a test's subject.
    pub fn lily(self) -> Color {
        match self {
            Self::Dark => Color::from_u32(0x00C5_2416),
            Self::Light => Color::from_u32(0xB0_21_0F),
        }
    }
}

/// Inputs that decide whether the transient boot should be mounted at all.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BootPreferences {
    pub already_seen: bool,
    pub force_replay: bool,
    pub reduced_motion: bool,
    pub effects_off: bool,
    pub agent_mode: bool,
}

impl BootPreferences {
    /// Reduced motion, effects-off, and agent mode always bypass the animation.
    /// A force replay only overrides the once-per-session guard.
    pub fn should_play(self) -> bool {
        !self.reduced_motion
            && !self.effects_off
            && !self.agent_mode
            && (!self.already_seen || self.force_replay)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BootPhase {
    Condensing { progress: f64 },
    Holding,
    Dissolving { progress: f64, skipped: bool },
    Finished,
}

impl BootPhase {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Finished)
    }
}

/// Deterministic state for one boot. Time is supplied by the caller so tests do
/// not sleep and render output never depends on wall-clock jitter.
#[derive(Clone, Debug)]
pub struct BootScene {
    tip_index: usize,
    skipped_at_ms: Option<u64>,
}

impl BootScene {
    pub fn new(tip_index: usize) -> Self {
        Self {
            tip_index: tip_index % TIPS.len(),
            skipped_at_ms: None,
        }
    }

    pub fn tip(&self) -> &'static str {
        TIPS[self.tip_index]
    }

    /// Begin the short exit pass. Returns false when the scene was already
    /// leaving or finished, making repeated key/wheel events harmless.
    pub fn skip(&mut self, elapsed_ms: u64) -> bool {
        if self.skipped_at_ms.is_some() || self.phase(elapsed_ms).is_finished() {
            return false;
        }
        self.skipped_at_ms = Some(elapsed_ms);
        true
    }

    pub fn phase(&self, elapsed_ms: u64) -> BootPhase {
        if elapsed_ms >= FAIL_MS {
            return BootPhase::Finished;
        }
        if let Some(skipped_at_ms) = self.skipped_at_ms {
            let elapsed = elapsed_ms.saturating_sub(skipped_at_ms);
            if elapsed >= EXIT_SKIP_MS {
                return BootPhase::Finished;
            }
            return BootPhase::Dissolving {
                progress: smooth(elapsed as f64 / EXIT_SKIP_MS as f64),
                skipped: true,
            };
        }
        if elapsed_ms < CONDENSE_MS {
            return BootPhase::Condensing {
                progress: ease_out(elapsed_ms as f64 / CONDENSE_MS as f64),
            };
        }
        if elapsed_ms < PLAY_MS {
            return BootPhase::Holding;
        }
        let exit_elapsed = elapsed_ms - PLAY_MS;
        if exit_elapsed >= EXIT_MS {
            return BootPhase::Finished;
        }
        BootPhase::Dissolving {
            progress: smooth(exit_elapsed as f64 / EXIT_MS as f64),
            skipped: false,
        }
    }

    /// Paint the unresolved part of the boot over an existing frame.
    ///
    /// During the dissolve, resolved cells are intentionally left untouched so
    /// the caller's already-rendered application shows through patch by patch.
    pub fn render(&self, area: Rect, buffer: &mut Buffer, elapsed_ms: u64, palette: BootPalette) {
        if area.is_empty() || self.phase(elapsed_ms).is_finished() {
            return;
        }
        let phase = self.phase(elapsed_ms);
        let emerge = match phase {
            BootPhase::Condensing { progress } => progress,
            BootPhase::Holding | BootPhase::Dissolving { .. } => 1.0,
            BootPhase::Finished => return,
        };
        let dissolve = match phase {
            BootPhase::Dissolving { progress, .. } => Some(progress),
            _ => None,
        };
        let width = usize::from(area.width);
        let height = usize::from(area.height);
        let t_seconds = elapsed_ms as f64 / 1_000.0;
        let base_style = Style::default().bg(palette.bone());

        for local_y in 0..height {
            for local_x in 0..width {
                if dissolve.is_some_and(|progress| patch_has_resolved(local_x, local_y, progress)) {
                    continue;
                }
                let coverage = scene_coverage(local_x, local_y, width, height, t_seconds) * emerge;
                let noise_amplitude = match phase {
                    BootPhase::Condensing { .. } => {
                        let play_progress = elapsed_ms as f64 / PLAY_MS as f64;
                        0.06 * (1.0 - (play_progress * 1.15).min(1.0))
                    }
                    _ => 0.0,
                };
                let lily_emerge = clamp01((emerge - 0.42) / 0.58);
                let lily = lily_braille_symbol(local_x, local_y, width, height, lily_emerge);
                let lily_symbol = lily.map(|symbol| symbol.to_string());
                let (symbol, foreground) = if let Some(symbol) = lily_symbol.as_deref() {
                    (symbol, palette.lily())
                } else if coverage > 0.035 && coverage > bayer_at(local_x, local_y) {
                    if coverage > 0.45 {
                        ("∷", palette.ink())
                    } else {
                        ("·", palette.ghost())
                    }
                } else if noise_amplitude > 0.0 && hash2(local_x, local_y, 9) < noise_amplitude {
                    ("·", palette.ghost())
                } else {
                    (" ", palette.ink())
                };
                buffer[(area.x + local_x as u16, area.y + local_y as u16)]
                    .set_symbol(symbol)
                    .set_style(base_style.fg(foreground));
            }
        }

        let name_y = height.saturating_mul(29) / 100;
        paint_centered(
            "Estelle",
            name_y,
            area,
            buffer,
            dissolve,
            Style::default()
                .fg(palette.ink())
                .bg(palette.bone())
                .add_modifier(Modifier::BOLD),
        );
        paint_centered(
            "by Fate Labs",
            name_y.saturating_add(2),
            area,
            buffer,
            dissolve,
            Style::default().fg(palette.ghost()).bg(palette.bone()),
        );
        paint_centered(
            self.tip(),
            height.saturating_sub(3),
            area,
            buffer,
            dissolve,
            Style::default().fg(palette.ink()).bg(palette.bone()),
        );
        paint_text(
            "skip",
            width.saturating_sub(6),
            height.saturating_sub(2),
            area,
            buffer,
            dissolve,
            Style::default().fg(palette.ghost()).bg(palette.bone()),
        );
    }
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn ease_out(value: f64) -> f64 {
    1.0 - (1.0 - clamp01(value)).powi(3)
}

fn smooth(value: f64) -> f64 {
    let value = clamp01(value);
    value * value * (3.0 - 2.0 * value)
}

fn bayer_at(x: usize, y: usize) -> f64 {
    (f64::from(BAYER_8[(y & 7) * 8 + (x & 7)]) + 0.5) / 64.0
}

fn hash2(x: usize, y: usize, seed: usize) -> f64 {
    let value = (x as f64 * 127.1 + y as f64 * 311.7 + seed as f64 * 74.7).sin() * 43_758.545_3;
    value - value.floor()
}

fn ridge_y(ridge: Ridge, unit_x: f64, height: usize) -> f64 {
    height as f64
        * (ridge.y
            + (unit_x * ridge.frequency_1 + ridge.phase).sin() * ridge.amplitude
            + (unit_x * ridge.frequency_2 + ridge.phase * 2.7).sin() * ridge.amplitude * 0.4)
}

fn add_alpha(coverage: f64, alpha: f64) -> f64 {
    1.0 - (1.0 - coverage) * (1.0 - alpha)
}

fn scene_coverage(x: usize, y: usize, width: usize, height: usize, t_seconds: f64) -> f64 {
    let x_f = x as f64 + 0.5;
    let y_f = y as f64 + 0.5;
    let width_f = width.max(1) as f64;
    let height_f = height.max(1) as f64;
    let unit_x = x_f / width_f;
    let mut coverage = 0.0;

    let sun_radius = height_f * 0.062;
    if (x_f - width_f * 0.82).powi(2) + (y_f - height_f * 0.14).powi(2) <= sun_radius.powi(2) {
        coverage = add_alpha(coverage, 0.10);
    }

    for index in 0..3 {
        let index_f = index as f64;
        let cloud_width = width_f * (0.18 + index_f * 0.05);
        let cloud_x = (t_seconds * (1.1 + index_f * 0.5) + index_f * 137.0)
            % (width_f + cloud_width)
            - cloud_width;
        let center_x = cloud_x + cloud_width / 2.0;
        let center_y = height_f * (0.06 + index_f * 0.05);
        let radius_y = (height_f * 0.016).max(0.5);
        let ellipse = ((x_f - center_x) / (cloud_width / 2.0)).powi(2)
            + ((y_f - center_y) / radius_y).powi(2);
        if ellipse <= 1.0 {
            coverage = add_alpha(coverage, 0.06);
        }
    }

    for ridge in RIDGES {
        if y_f >= ridge_y(ridge, unit_x, height) {
            coverage = add_alpha(coverage, ridge.alpha);
        }
    }

    if underbrush_at(x_f, y_f, width_f, height_f, t_seconds) {
        coverage = add_alpha(coverage, 0.55);
    }
    coverage
}

fn underbrush_at(x: f64, y: f64, width: f64, height: f64, t_seconds: f64) -> bool {
    for index in 0..26 {
        let index_f = index as f64;
        let seeded = (index_f * 91.7).sin() * 43_758.545_3;
        let fraction = seeded - seeded.floor();
        let base_x = fraction * width;
        let base_y = ridge_y(RIDGES[3], fraction, height as usize) + 2.0;
        let brush_height = height * (0.032 + ((index * 37) % 11) as f64 / 150.0);
        let p0 = (base_x, base_y + brush_height);
        let p1 = (
            base_x + (t_seconds * 0.3 + index_f).sin() * 2.0,
            base_y + brush_height * 0.4,
        );
        let p2 = (base_x + index_f.sin() * 4.0, base_y - brush_height * 0.2);
        for step in 0..=8 {
            let t = step as f64 / 8.0;
            let inverse = 1.0 - t;
            let curve_x = inverse * inverse * p0.0 + 2.0 * inverse * t * p1.0 + t * t * p2.0;
            let curve_y = inverse * inverse * p0.1 + 2.0 * inverse * t * p1.1 + t * t * p2.1;
            if (x - curve_x).abs() <= 0.55 && (y - curve_y).abs() <= 0.55 {
                return true;
            }
        }
    }
    false
}

fn cubic_distance_from(
    x: f64,
    y: f64,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) -> f64 {
    (0..=28)
        .map(|step| {
            let t = f64::from(step) / 28.0;
            let inverse = 1.0 - t;
            let curve_x = inverse * inverse * inverse * p0.0
                + 3.0 * inverse * inverse * t * p1.0
                + 3.0 * inverse * t * t * p2.0
                + t * t * t * p3.0;
            let curve_y = inverse * inverse * inverse * p0.1
                + 3.0 * inverse * inverse * t * p1.1
                + 3.0 * inverse * t * t * p2.1
                + t * t * t * p3.1;
            (x - curve_x).hypot(y - curve_y)
        })
        .fold(f64::INFINITY, f64::min)
}

fn cubic_distance(x: f64, y: f64, p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    cubic_distance_from(x, y, (0.0, 0.0), p1, p2, p3)
}

fn ribbon_coverage(
    x: f64,
    y: f64,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    width: f64,
) -> f64 {
    let mut coverage: f64 = 0.0;
    for step in 0..=36 {
        let t = f64::from(step) / 36.0;
        let inverse = 1.0 - t;
        let curve_x = inverse * inverse * inverse * p0.0
            + 3.0 * inverse * inverse * t * p1.0
            + 3.0 * inverse * t * t * p2.0
            + t * t * t * p3.0;
        let curve_y = inverse * inverse * inverse * p0.1
            + 3.0 * inverse * inverse * t * p1.1
            + 3.0 * inverse * t * t * p2.1
            + t * t * t * p3.1;
        let belly = (std::f64::consts::PI * t).sin().max(0.0).powf(0.62);
        let tip_rounding = ((t - 0.90) / 0.10).clamp(0.0, 1.0);
        let local_width = width * (0.18 + belly * 0.82) + width * 0.22 * tip_rounding;
        let distance = (x - curve_x).hypot(y - curve_y);
        if distance <= local_width {
            coverage = coverage.max(0.82 + (1.0 - distance / local_width) * 0.18);
        }
    }
    coverage
}

/// One asymmetric higanbana head in local coordinates centered on its heart.
///
/// The reference flower is made from curled ribbons and long filaments, not
/// radial wedges. Keeping this primitive public lets the boot veil and the
/// persistent home ground render exactly the same silhouette.
pub fn spider_lily_coverage(x: f64, y: f64) -> f64 {
    let mut coverage: f64 = 0.0;

    // A spider lily is a crown of recurved ribbons above a stem, not a radial
    // rosette. These paths deliberately overlap and turn back toward the heart
    // like the full petals in the website's `lily.ts` reference.
    for (p0, p1, p2, p3, width) in [
        (
            (-0.03, 0.02),
            (-0.34, -0.08),
            (-1.12, -0.72),
            (-0.86, -0.06),
            0.076,
        ),
        (
            (-0.03, 0.00),
            (-0.30, -0.30),
            (-0.94, -0.98),
            (-0.58, -0.34),
            0.072,
        ),
        (
            (-0.02, -0.01),
            (-0.16, -0.46),
            (-0.62, -1.10),
            (-0.34, -0.49),
            0.069,
        ),
        (
            (-0.01, -0.02),
            (-0.05, -0.50),
            (-0.24, -1.08),
            (-0.02, -0.54),
            0.066,
        ),
        (
            (0.01, -0.02),
            (0.06, -0.52),
            (0.24, -1.02),
            (0.29, -0.48),
            0.068,
        ),
        (
            (0.02, -0.01),
            (0.18, -0.42),
            (0.69, -1.06),
            (0.45, -0.39),
            0.071,
        ),
        (
            (0.03, 0.00),
            (0.34, -0.27),
            (1.02, -0.90),
            (0.70, -0.22),
            0.074,
        ),
        (
            (0.03, 0.02),
            (0.42, -0.06),
            (1.17, -0.60),
            (0.88, 0.03),
            0.078,
        ),
        (
            (-0.02, 0.03),
            (-0.31, 0.04),
            (-0.72, 0.39),
            (-0.46, 0.13),
            0.064,
        ),
        (
            (0.00, 0.04),
            (0.25, 0.10),
            (0.66, 0.42),
            (0.42, 0.12),
            0.064,
        ),
        (
            (-0.01, 0.00),
            (-0.22, -0.18),
            (-0.52, -0.67),
            (-0.22, -0.29),
            0.054,
        ),
        (
            (0.01, 0.00),
            (0.22, -0.19),
            (0.54, -0.66),
            (0.24, -0.27),
            0.054,
        ),
        (
            (0.00, 0.01),
            (-0.12, -0.10),
            (-0.20, -0.47),
            (0.02, -0.25),
            0.048,
        ),
        (
            (0.01, 0.01),
            (0.14, -0.08),
            (0.27, -0.45),
            (0.08, -0.23),
            0.048,
        ),
    ] {
        let petal = ribbon_coverage(x, y, p0, p1, p2, p3, width);
        if petal > 0.0 {
            coverage = coverage.max(petal);
        }
    }

    let heart = ((x + 0.015) / 0.155).hypot((y + 0.005) / 0.125);
    if heart <= 1.0 {
        coverage = coverage.max(1.0 - heart * 0.16);
    }

    // Long stamens fan beyond the petals. Their detached tips are the visual
    // cue that separates higanbana from a generic flower at terminal scale.
    for (p1, p2, tip) in [
        ((-0.08, -0.25), (-0.91, -0.92), (-1.15, -0.52)),
        ((-0.05, -0.30), (-0.73, -1.15), (-0.91, -0.77)),
        ((-0.03, -0.34), (-0.48, -1.23), (-0.63, -0.91)),
        ((-0.01, -0.36), (-0.20, -1.29), (-0.28, -0.97)),
        ((0.01, -0.36), (0.10, -1.30), (0.17, -0.98)),
        ((0.03, -0.34), (0.39, -1.25), (0.51, -0.91)),
        ((0.05, -0.31), (0.67, -1.14), (0.82, -0.76)),
        ((0.08, -0.27), (0.91, -0.96), (1.10, -0.57)),
        ((0.09, -0.20), (1.02, -0.67), (1.19, -0.31)),
    ] {
        if cubic_distance(x, y, p1, p2, tip) <= 0.018 {
            coverage = coverage.max(0.96);
        }
        if (x - tip.0).hypot(y - tip.1) <= 0.035 {
            coverage = 1.0;
        }
    }

    // Only a short stem stub belongs in this scene.
    if (0.10..=0.58).contains(&y) && (x + 0.075 * y * y).abs() <= 0.030 {
        coverage = coverage.max(0.86);
    }
    clamp01(coverage)
}

/// Terminal-cell translation of `web/app/explore/_components/lily.ts`.
/// X and Y use separate radii so the bloom stays round in tall terminal cells.
fn lily_coverage_at(x: f64, y: f64, width: usize, height: usize) -> f64 {
    let height_f = height.max(1) as f64;
    let center_x = width.max(1) as f64 * 0.76;
    let center_y = height_f * 0.66;
    let radius_y = (height_f * 0.21).max(5.5);
    let radius_x = (radius_y * 2.10).min(width.max(1) as f64 * 0.16);
    let local_x = (x - center_x) / radius_x.max(1.0);
    let local_y = (y - center_y) / radius_y;
    spider_lily_coverage(local_x, local_y)
}

fn lily_braille_symbol(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    emerge: f64,
) -> Option<char> {
    const DOTS: [[u32; 4]; 2] = [[0, 1, 2, 6], [3, 4, 5, 7]];
    let mut mask = 0_u32;
    for (column, rows) in DOTS.iter().enumerate() {
        for (row, bit) in rows.iter().enumerate() {
            let sample_x = x as f64 + (column as f64 + 0.5) / 2.0;
            let sample_y = y as f64 + (row as f64 + 0.5) / 4.0;
            let coverage = lily_coverage_at(sample_x, sample_y, width, height) * emerge;
            if coverage > 0.48 {
                mask |= 1 << bit;
            }
        }
    }
    if mask == 0 {
        None
    } else {
        char::from_u32(0x2800 + mask)
    }
}

fn patch_has_resolved(x: usize, y: usize, progress: f64) -> bool {
    const EXIT_PATCH: usize = 3;
    let group_x = x / EXIT_PATCH;
    let group_y = y / EXIT_PATCH;
    let threshold = bayer_at(group_x, group_y) * 0.86 + hash2(group_x, group_y, 5) * 0.14;
    progress >= threshold
}

fn paint_centered(
    text: &str,
    y: usize,
    area: Rect,
    buffer: &mut Buffer,
    dissolve: Option<f64>,
    style: Style,
) {
    let width = usize::from(area.width);
    let x = width.saturating_sub(text.chars().count()) / 2;
    paint_text(text, x, y, area, buffer, dissolve, style);
}

fn paint_text(
    text: &str,
    x: usize,
    y: usize,
    area: Rect,
    buffer: &mut Buffer,
    dissolve: Option<f64>,
    style: Style,
) {
    if y >= usize::from(area.height) {
        return;
    }
    for (offset, character) in text.chars().enumerate() {
        let local_x = x + offset;
        if local_x >= usize::from(area.width)
            || dissolve.is_some_and(|progress| patch_has_resolved(local_x, y, progress))
        {
            continue;
        }
        buffer[(area.x + local_x as u16, area.y + y as u16)]
            .set_char(character)
            .set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(scene: &BootScene, elapsed_ms: u64, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::filled(area, ratatui::buffer::Cell::new("x"));
        scene.render(area, &mut buffer, elapsed_ms, BootPalette::Dark);
        buffer
    }

    fn symbols(buffer: &Buffer) -> String {
        let area = buffer.area;
        let mut output = String::new();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                output.push_str(buffer[(x, y)].symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn phase_condenses_holds_then_dissolves() {
        let scene = BootScene::new(0);
        assert_eq!(scene.phase(0), BootPhase::Condensing { progress: 0.0 });
        assert!(matches!(scene.phase(CONDENSE_MS), BootPhase::Holding));
        assert!(matches!(scene.phase(PLAY_MS - 1), BootPhase::Holding));
        assert_eq!(
            scene.phase(PLAY_MS),
            BootPhase::Dissolving {
                progress: 0.0,
                skipped: false,
            }
        );
        assert!(scene.phase(PLAY_MS + EXIT_MS).is_finished());
        assert!(scene.phase(FAIL_MS).is_finished());
    }

    #[test]
    fn skip_uses_the_short_exit_and_is_idempotent() {
        let mut scene = BootScene::new(1);
        assert!(scene.skip(200));
        assert!(!scene.skip(220));
        assert_eq!(
            scene.phase(200),
            BootPhase::Dissolving {
                progress: 0.0,
                skipped: true,
            }
        );
        assert!(scene.phase(200 + EXIT_SKIP_MS).is_finished());
    }

    #[test]
    fn reduced_motion_and_effects_bypass_the_boot() {
        assert!(BootPreferences::default().should_play());
        assert!(
            !BootPreferences {
                reduced_motion: true,
                force_replay: true,
                ..BootPreferences::default()
            }
            .should_play()
        );
        assert!(
            !BootPreferences {
                effects_off: true,
                ..BootPreferences::default()
            }
            .should_play()
        );
        assert!(
            !BootPreferences {
                agent_mode: true,
                ..BootPreferences::default()
            }
            .should_play()
        );
        assert!(
            !BootPreferences {
                already_seen: true,
                ..BootPreferences::default()
            }
            .should_play()
        );
        assert!(
            BootPreferences {
                already_seen: true,
                force_replay: true,
                ..BootPreferences::default()
            }
            .should_play()
        );
    }

    #[test]
    fn holding_frame_has_brand_landscape_tip_and_sparse_earned_red() {
        let scene = BootScene::new(2);
        let buffer = rendered(&scene, CONDENSE_MS, 180, 50);
        let output = symbols(&buffer);
        assert!(output.contains("Estelle"));
        assert!(output.contains("by Fate Labs"));
        assert!(output.contains(scene.tip()));
        assert!(!output.to_ascii_lowercase().contains("flower"));

        let mut red = 0;
        let mut structural = 0;
        for cell in &buffer.content {
            if cell.fg == BootPalette::Dark.lily() {
                red += 1;
            }
            if matches!(cell.symbol(), "·" | "∷") {
                structural += 1;
            }
        }
        assert!(structural > 500, "landscape did not materialize");
        assert!(red > 0, "deterministic near-hill embers disappeared");
        assert!(
            red * 3 < structural,
            "red stopped being one restrained bloom"
        );
    }

    #[test]
    fn gallery_sized_holding_frame_keeps_one_earned_red() {
        let scene = BootScene::new(0);
        let buffer = rendered(&scene, CONDENSE_MS, 120, 34);
        let red = buffer
            .content
            .iter()
            .filter(|cell| cell.fg == BootPalette::Dark.lily())
            .count();

        assert!(red > 0, "gallery-sized boot lost the earned red ink");
    }

    #[test]
    fn red_ink_forms_one_localized_spider_lily_with_a_short_stem() {
        let scene = BootScene::new(0);
        let buffer = rendered(&scene, CONDENSE_MS, 180, 50);
        let red = buffer
            .content
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| {
                (cell.fg == BootPalette::Dark.lily()).then_some((index % 180, index / 180))
            })
            .collect::<Vec<_>>();

        assert!(red.len() >= 80, "spider lily lost its petal mass");
        assert!(
            red.iter()
                .all(|(x, y)| (110..=164).contains(x) && (21..=39).contains(y)),
            "red ink escaped the single focal bloom: {red:?}"
        );

        let center_x = 137;
        assert!(red.iter().any(|(x, y)| *x < center_x - 16 && *y <= 28));
        assert!(red.iter().any(|(x, y)| *x > center_x + 16 && *y <= 28));
        assert!(
            red.iter().any(|(_, y)| *y < 29),
            "upper petals/stamens are absent"
        );
        assert!(
            red.iter()
                .any(|(x, y)| x.abs_diff(center_x) <= 2 && *y >= 36),
            "short stem is absent"
        );
        assert!(
            red.iter().all(|(_, y)| *y <= 39),
            "stem became a long line into the ground"
        );

        let mut subpixels = Vec::new();
        for (index, cell) in buffer.content.iter().enumerate() {
            if cell.fg != BootPalette::Dark.lily() {
                continue;
            }
            let symbol = cell.symbol().chars().next().expect("red cell symbol");
            let mask = u32::from(symbol)
                .checked_sub(0x2800)
                .filter(|mask| *mask <= 0xff)
                .expect("spider lily must use Braille subpixels");
            let cell_x = index % 180;
            let cell_y = index / 180;
            for (bit, offset_x, offset_y) in [
                (0, 0, 0),
                (1, 0, 1),
                (2, 0, 2),
                (3, 1, 0),
                (4, 1, 1),
                (5, 1, 2),
                (6, 0, 3),
                (7, 1, 3),
            ] {
                if mask & (1 << bit) != 0 {
                    subpixels.push((cell_x * 2 + offset_x, cell_y * 4 + offset_y));
                }
            }
        }
        assert!(
            subpixels.len() >= 70,
            "spider lily lost its curved petal and stamen detail"
        );
        let widest_subpixel_row = (0..200)
            .map(|row| subpixels.iter().filter(|(_, y)| *y == row).count())
            .max()
            .unwrap_or_default();
        assert!(
            widest_subpixel_row <= 100,
            "spider lily collapsed into a horizontal red slab ({widest_subpixel_row} subpixels)"
        );
        let red_cell_capacity = red.len() * 8;
        assert!(
            subpixels.len() * 4 < red_cell_capacity * 3,
            "spider lily lost its negative space and became a solid mass"
        );
    }

    #[test]
    fn bayer_exit_reveals_the_underlying_frame_monotonically() {
        let scene = BootScene::new(0);
        let early = rendered(&scene, PLAY_MS, 96, 30);
        let middle = rendered(&scene, PLAY_MS + EXIT_MS / 2, 96, 30);
        let late = rendered(&scene, PLAY_MS + EXIT_MS - 1, 96, 30);
        let revealed = |buffer: &Buffer| {
            buffer
                .content
                .iter()
                .filter(|cell| cell.symbol() == "x")
                .count()
        };
        assert_eq!(revealed(&early), 0);
        assert!(revealed(&middle) > 0);
        assert!(revealed(&late) > revealed(&middle));
    }

    #[test]
    fn scene_is_deterministic_for_the_same_time_and_tip() {
        let scene = BootScene::new(4);
        assert_eq!(
            symbols(&rendered(&scene, 500, 100, 32)),
            symbols(&rendered(&scene, 500, 100, 32))
        );
    }
}
