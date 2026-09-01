use crate::color::blend;
use crate::color::is_light;
use crate::terminal_palette::StdoutColorLevel;
use crate::terminal_palette::best_color;
use crate::terminal_palette::default_bg;
use crate::terminal_palette::default_fg;
use crate::terminal_palette::rgb_color;
use crate::terminal_palette::stdout_color_level;
use ratatui::style::Color;
use ratatui::style::Style;

const LIGHT_BG_ACCENT_RGB: (u8, u8, u8) = (0, 95, 135);
// Decorative table rules should remain visible without competing with cell content.
const TABLE_SEPARATOR_FG_ALPHA: f32 = 0.20;

pub fn user_message_style() -> Style {
    user_message_style_for(default_bg())
}

pub fn proposed_plan_style() -> Style {
    proposed_plan_style_for(default_bg())
}

/// Returns a low-contrast rule style for separators within markdown tables.
pub(crate) fn table_separator_style() -> Style {
    table_separator_style_for(default_fg(), default_bg(), stdout_color_level())
}

/// Returns the shared accent style for active or selected TUI controls.
pub(crate) fn accent_style() -> Style {
    accent_style_for(default_bg())
}

/// Returns the style for a user-authored message using the provided terminal background.
pub fn user_message_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg {
        Some(bg) => Style::default().bg(user_message_bg(bg)),
        None => Style::default(),
    }
}

pub fn proposed_plan_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    match terminal_bg {
        Some(bg) => Style::default().bg(proposed_plan_bg(bg)),
        None => Style::default(),
    }
}

/// Returns the shared accent style for the provided terminal background.
pub(crate) fn accent_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Style {
    if terminal_bg.is_some_and(is_light) {
        Style::default().fg(best_color(LIGHT_BG_ACCENT_RGB)).bold()
    } else {
        Style::default().fg(Color::Cyan).bold()
    }
}

fn table_separator_style_for(
    terminal_fg: Option<(u8, u8, u8)>,
    terminal_bg: Option<(u8, u8, u8)>,
    color_level: StdoutColorLevel,
) -> Style {
    let (Some(fg), Some(bg)) = (terminal_fg, terminal_bg) else {
        return Style::default().dim();
    };
    let separator_rgb = blend(fg, bg, TABLE_SEPARATOR_FG_ALPHA);
    match color_level {
        StdoutColorLevel::TrueColor => Style::default().fg(rgb_color(separator_rgb)),
        StdoutColorLevel::Ansi256 => Style::default().fg(best_color(separator_rgb)),
        StdoutColorLevel::Ansi16 | StdoutColorLevel::Unknown => Style::default().dim(),
    }
}

#[allow(clippy::disallowed_methods)]
pub fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    best_color(user_message_bg_rgb(terminal_bg))
}

pub(crate) fn user_message_bg_rgb(terminal_bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let (top, alpha) = if is_light(terminal_bg) {
        ((0, 0, 0), 0.04)
    } else {
        ((255, 255, 255), 0.12)
    };
    blend(top, terminal_bg, alpha)
}

#[allow(clippy::disallowed_methods)]
pub fn proposed_plan_bg(terminal_bg: (u8, u8, u8)) -> Color {
    user_message_bg(terminal_bg)
}

/// 🔴 **THE BAN THAT STOPPED `Color::Rgb` CANNOT SEE `Color::Red`, AND THE DRIFT WENT THERE.**
///
/// `clippy.toml` disallows `ratatui::style::Color::Rgb` and `::Indexed`. Both are TUPLE variants —
/// which are constructor *functions* — and `clippy::disallowed_methods` only understands
/// functions. The named colours (`Color::Red`, `Color::Yellow`, `Color::DarkGray`, …) are UNIT
/// variants. Adding one to the same list does not tighten the lint; clippy rejects the entry:
///
/// ```text
/// warning: expected a function, found a variant
///  --> clippy.toml:9:5
///   |
/// 9 |     { path = "ratatui::style::Color::Red", reason = "..." },
///   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
///   = help: add `allow-invalid = true` to the entry to suppress this warning
/// ```
///
/// Measured 2026-08-31 against clippy 0.1.95 by putting exactly that entry in `clippy.toml` and
/// running `cargo clippy -p estelle-tui --all-targets`: it produced the config warning above and
/// **zero** diagnostics at any of the call sites. The `allow-invalid = true` help silences the
/// warning about the entry — it does not make the lint fire. There is no `disallowed_variants`
/// lint, and `disallowed_types` would have to ban `Color` itself, which every style signature in
/// the crate needs. **So a test is the only instrument that can hold this line, and this is it.**
///
/// ## What it enforces, and the limit it does not hide
///
/// The 13-role brand palette lives in `tui/src/theme.rs`, declared `mod theme;` in **`main.rs`** —
/// a private module of the *binary* crate `estelle`. The *library* crate `estelle_tui` (every
/// `mod` in `lib.rs`: the composer, the history cells, the diff renderer, the resume picker)
/// cannot name it at all. Telling the library "use the palette" would be a rule nobody there can
/// obey, and it would contradict the reason string the library already follows — *"Use ANSI
/// colors, which work better in various terminal themes"*. So the two halves get two different
/// rules:
///
/// * **Binary crate — where `theme::Palette` IS reachable: zero named colours**, except the
///   budgets written out in [`BIN_BUDGET`], each with the reason it is there.
/// * **Library crate — where it is NOT reachable: a one-way ratchet.** [`LIB_BUDGET`] pins what is
///   there today per file; a file may shrink for free and may never grow, and a file that is not
///   on the list must be at zero. That does not make the library right — it stops the drift from
///   relocating into it while the binary is being cleaned.
///
/// ⚠️ **The real fix for the library half is structural and is NOT done here:** move `theme.rs`
/// into the library crate so `Palette` is reachable from the widgets that paint, then convert the
/// `LIB_BUDGET` entries and delete them. Until that happens the ratchet is a holding action, and
/// a per-file MAX cannot see a swap — one removed from file A and one added to file B nets zero.
/// It catches growth and new doors, which is what it claims and all that it claims.
#[cfg(test)]
mod brand_palette_guard {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::path::PathBuf;

    /// Every `ratatui::style::Color` variant that resolves against the *terminal's* palette rather
    /// than ours. `Rgb`/`Indexed` are absent on purpose: clippy already denies those.
    const NAMED_VARIANTS: &[&str] = &[
        "Black",
        "Red",
        "Green",
        "Yellow",
        "Blue",
        "Magenta",
        "Cyan",
        "Gray",
        "Grey",
        "DarkGray",
        "DarkGrey",
        "LightRed",
        "LightGreen",
        "LightYellow",
        "LightBlue",
        "LightCyan",
        "LightMagenta",
        "White",
    ];

    /// Binary-crate files allowed to name a terminal colour, and why. Anything not listed must be
    /// at zero — that clause is what catches a NEW door rather than a wider one.
    ///
    /// * `test_gallery.rs` is an ANSI→hex **serializer** for the SVG gallery export. It has to
    ///   enumerate the named variants in order to draw them; converting it to the palette would
    ///   make it unable to render a colour some other module produced.
    /// * `main.rs` holds one product colour (`Theme::primary()` for Cream Ink is `Color::Black`,
    ///   where the palette's `bright` is `#1f1c17`) and one test assertion about it. Both belong
    ///   to another lane's file and are reported, not silently converted.
    /// * `transcript.rs` is down to four: two `Color::Yellow` badges, one `Color::DarkGray`
    ///   citation label and one `Color::Red` failure banner, all four blocked on three fields
    ///   `TranscriptPalette` does not carry (see its doc comment), plus four test fixtures.
    const BIN_BUDGET: &[(&str, usize)] = &[
        ("main.rs", 2),
        ("test_gallery.rs", 16),
        ("transcript.rs", 8),
    ];

    /// Library-crate ratchet. These files cannot reach `theme::Palette`; the number is what was
    /// there on 2026-08-31 and it may only go down. Lowering an entry is part of any fix.
    const LIB_BUDGET: &[(&str, usize)] = &[
        ("bottom_pane/chat_composer.rs", 6),
        ("bottom_pane/chat_composer/history_search.rs", 3),
        ("bottom_pane/effort_ignition.rs", 2),
        ("bottom_pane/effort_ignition_tests.rs", 4),
        ("bottom_pane/effort_status_line.rs", 2),
        ("bottom_pane/effort_status_line_tests.rs", 3),
        ("bottom_pane/hooks_browser_view.rs", 3),
        ("bottom_pane/list_selection_view.rs", 4),
        ("bottom_pane/mentions_v2/render.rs", 1),
        ("bottom_pane/request_user_input/mod.rs", 2),
        ("bottom_pane/status_line_style.rs", 28),
        ("bottom_pane/textarea.rs", 1),
        ("chatwidget/permission_popups.rs", 1),
        ("chatwidget/tests/side.rs", 1),
        ("chatwidget/tokens/chart/palette_tests.rs", 3),
        ("chatwidget/windows_sandbox_prompts.rs", 1),
        ("custom_terminal.rs", 1),
        ("diff_render.rs", 19),
        ("history_cell/messages.rs", 1),
        ("history_cell/request_user_input.rs", 5),
        ("insert_history.rs", 3),
        ("markdown_stream.rs", 8),
        ("multi_agents.rs", 2),
        ("onboarding/auth.rs", 9),
        ("public_widgets/history_transcript.rs", 6),
        ("resume_picker.rs", 11),
        ("style.rs", 2),
        ("terminal_hyperlinks.rs", 1),
        ("wrapping.rs", 3),
    ];

    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// Blank out `//` line comments and `/* */` block comments without touching string literals.
    ///
    /// A `///` doc comment that *describes* the old colour is documentation, not a call site —
    /// `live_renderer.rs` carries three such lines today and counting them would report a file
    /// that is already clean as dirty. The `//` inside `"https://…"` is the case that makes a
    /// naive stripper wrong, so the scanner tracks string state; [`comment_stripper_is_not_fooled`]
    /// is the control for that.
    fn strip_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut in_block = false;
        for line in source.lines() {
            // Char-wise, never byte-wise: this crate's source is full of emoji and box glyphs, and
            // slicing a multi-byte char in half panics.
            let chars: Vec<char> = line.chars().collect();
            let mut index = 0usize;
            let mut in_string = false;
            let mut escaped = false;
            while index < chars.len() {
                let current = chars[index];
                let next = chars.get(index + 1).copied();
                if in_block {
                    if current == '*' && next == Some('/') {
                        in_block = false;
                        index += 2;
                    } else {
                        index += 1;
                    }
                    continue;
                }
                if in_string {
                    out.push(current);
                    if escaped {
                        escaped = false;
                    } else if current == '\\' {
                        escaped = true;
                    } else if current == '"' {
                        in_string = false;
                    }
                    index += 1;
                    continue;
                }
                if current == '/' && next == Some('/') {
                    break;
                }
                if current == '/' && next == Some('*') {
                    in_block = true;
                    index += 2;
                    continue;
                }
                if current == '"' {
                    in_string = true;
                }
                out.push(current);
                index += 1;
            }
            out.push('\n');
        }
        out
    }

    /// Count `…Color::<NamedVariant>` occurrences in code, ignoring comments.
    fn named_color_hits(source: &str) -> usize {
        let code = strip_comments(source);
        let bytes = code.as_bytes();
        let mut hits = 0usize;
        let mut from = 0usize;
        while let Some(offset) = code[from..].find("Color::") {
            let start = from + offset;
            from = start + "Color::".len();
            // `DiffColorLevel::` and friends never produce `Color::`, but a preceding identifier
            // char would mean some other type ending in "Color"; over-counting is the safe
            // direction, so only an alphanumeric prefix is rejected.
            let prefixed = start > 0
                && bytes
                    .get(start - 1)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
            if prefixed {
                continue;
            }
            let rest = &code[from..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            if NAMED_VARIANTS.contains(&&rest[..end]) {
                hits += 1;
            }
        }
        hits
    }

    /// Files reachable from `main.rs`, derived live so a NEW binary module is covered the day it
    /// is written. A hardcoded file list would have exactly the hole this guard exists to close.
    fn bin_crate_files() -> Vec<PathBuf> {
        let root = src_root();
        let main = std::fs::read_to_string(root.join("main.rs")).expect("read main.rs");
        let mut files = vec![root.join("main.rs")];
        for line in main.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("mod ") else {
                continue;
            };
            let Some(name) = rest.strip_suffix(';') else {
                continue;
            };
            if !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                continue;
            }
            let flat = root.join(format!("{name}.rs"));
            files.push(if flat.exists() {
                flat
            } else {
                root.join(name).join("mod.rs")
            });
        }
        files
    }

    fn census(files: impl IntoIterator<Item = PathBuf>) -> BTreeMap<String, usize> {
        let root = src_root();
        let mut counts = BTreeMap::new();
        for file in files {
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };
            let hits = named_color_hits(&source);
            if hits > 0 {
                let key = file
                    .strip_prefix(&root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                counts.insert(key, hits);
            }
        }
        counts
    }

    fn assert_within(counts: &BTreeMap<String, usize>, budget: &[(&str, usize)], half: &str) {
        let budget: BTreeMap<&str, usize> = budget.iter().copied().collect();
        let mut over = Vec::new();
        for (file, hits) in counts {
            let allowed = budget.get(file.as_str()).copied().unwrap_or(0);
            if *hits > allowed {
                over.push(format!("  {file}: {hits} named colours, budget {allowed}"));
            }
        }
        assert!(
            over.is_empty(),
            "{half}: a terminal-palette colour reached code the brand palette owns.\n{}\n\n\
             A named variant (Red / Yellow / DarkGray / …) renders as whatever the USER'S terminal \
             theme calls that word, not Estelle's values. Map by MEANING, not hue: failure is `palette.red`, \
             a caution `palette.warn`, secondary text `palette.dim`, a citation `palette.cite`. \
             If the call site has no `Palette`, thread it from the caller. If this line is a \
             deliberate exception, raise the budget in `tui/src/style.rs` WITH the reason — an \
             unexplained number here is the next regression's hiding place.",
            over.join("\n")
        );
    }

    #[test]
    fn the_binary_crate_paints_from_the_brand_palette() {
        assert_within(&census(bin_crate_files()), BIN_BUDGET, "binary crate");
    }

    #[test]
    fn the_library_crate_named_colours_never_grow() {
        let bin: std::collections::BTreeSet<PathBuf> = bin_crate_files().into_iter().collect();
        let mut lib = Vec::new();
        let mut stack = vec![src_root()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") && !bin.contains(&path) {
                    lib.push(path);
                }
            }
        }
        assert_within(&census(lib), LIB_BUDGET, "library crate");
    }

    /// 🔴 **THE POSITIVE CONTROL. A GUARD THAT HAS NEVER BEEN RED IS DECORATION.**
    ///
    /// Both assertions above are "nothing exceeds its budget", which is exactly the shape that
    /// passes forever on a detector that cannot fire. This drives the real scanner over a source
    /// string that DOES contain the drift and asserts it is seen — and over the same text as a
    /// comment and asserts it is not.
    /// The scanner counts inside string literals too (a colour name in a string can be a real
    /// call site in a theme parser), so these controls build their fixtures instead of writing
    /// them out — otherwise this file's own budget would have to absorb them and stop meaning
    /// anything. `NEEDLE` itself scores zero: the identifier after it is empty.
    const NEEDLE: &str = "Color::";

    #[test]
    fn the_scanner_fires_on_a_named_colour_and_not_on_a_comment() {
        assert_eq!(
            named_color_hits(&format!("let s = Style::default().fg({NEEDLE}Yellow);")),
            1
        );
        assert_eq!(
            named_color_hits(&format!(
                "fg(ratatui::style::{NEEDLE}LightBlue).bg({NEEDLE}DarkGray)"
            )),
            2
        );
        // Every banned variant is actually reachable by the matcher.
        for variant in NAMED_VARIANTS {
            assert_eq!(
                named_color_hits(&format!("Style::default().fg({NEEDLE}{variant})")),
                1,
                "the scanner is blind to a named variant: {variant}"
            );
        }
        // …and the two clippy already owns are NOT double-reported here.
        assert_eq!(
            named_color_hits(&format!("{NEEDLE}Rgb(0xc5, 0x24, 0x16)")),
            0
        );
        assert_eq!(named_color_hits(&format!("{NEEDLE}Indexed(22)")), 0);
        // The bare path with nothing after it is not a variant.
        assert_eq!(named_color_hits(NEEDLE), 0);
        // A doc comment describing the old design is documentation, not a call site.
        assert_eq!(
            named_color_hits(&format!("/// was {NEEDLE}Gray before the palette")),
            0
        );
        assert_eq!(
            named_color_hits(&format!("// {NEEDLE}Red\n/* {NEEDLE}Green */")),
            0
        );
    }

    #[test]
    fn comment_stripper_is_not_fooled() {
        // `//` inside a string literal does not start a comment.
        assert_eq!(
            named_color_hits(&format!(r#"let u = "https://x"; fg({NEEDLE}Red)"#)),
            1
        );
        // A block comment spanning lines stays a comment.
        assert_eq!(
            named_color_hits(&format!("/* start\n{NEEDLE}Blue\nend */")),
            0
        );
        // Code after a closing block comment is still code.
        assert_eq!(named_color_hits(&format!("/* x */ fg({NEEDLE}Cyan)")), 1);
        // A type whose name merely ends in "Color" is not `Color`.
        assert_eq!(
            named_color_hits(&format!("DiffColorLevel::Ansi16 Diff{NEEDLE}Red")),
            0
        );
        // Non-ASCII source (this crate is full of it) must not panic the char scanner.
        assert_eq!(
            named_color_hits(&format!("// 🔴 ┌─┐ was here\nfg({NEEDLE}Magenta) // ╌╌")),
            1
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::style::Modifier;

    #[test]
    fn accent_style_uses_darker_cyan_on_light_backgrounds() {
        let style = accent_style_for(Some((255, 255, 255)));

        assert_eq!(style.fg, Some(best_color(LIGHT_BG_ACCENT_RGB)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn accent_style_uses_cyan_on_dark_or_unknown_backgrounds() {
        let expected = Style::default().fg(Color::Cyan).bold();

        assert_eq!(accent_style_for(Some((0, 0, 0))), expected);
        assert_eq!(accent_style_for(/*terminal_bg*/ None), expected);
    }

    #[test]
    fn table_separator_blends_toward_dark_background() {
        let style = table_separator_style_for(
            Some((255, 255, 255)),
            Some((0, 0, 0)),
            StdoutColorLevel::TrueColor,
        );

        assert_eq!(style.fg, Some(rgb_color((51, 51, 51))));
    }

    #[test]
    fn table_separator_blends_toward_light_background() {
        let style = table_separator_style_for(
            Some((0, 0, 0)),
            Some((255, 255, 255)),
            StdoutColorLevel::TrueColor,
        );

        assert_eq!(style.fg, Some(rgb_color((204, 204, 204))));
    }

    #[test]
    fn table_separator_dims_when_palette_aware_color_is_unavailable() {
        let expected = Style::default().dim();

        assert_eq!(
            table_separator_style_for(
                Some((255, 255, 255)),
                Some((0, 0, 0)),
                StdoutColorLevel::Ansi16,
            ),
            expected
        );
        assert_eq!(
            table_separator_style_for(
                /*terminal_fg*/ None,
                Some((0, 0, 0)),
                StdoutColorLevel::TrueColor,
            ),
            expected
        );
    }
}
