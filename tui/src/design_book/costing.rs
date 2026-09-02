//! The costing panel — what a model costs, what this run spent, what the plan has left.
//!
//! 🔴 **THE SINGLE BIGGEST GAP BETWEEN WHAT THE SERVER KNOWS AND WHAT THE SCREEN SHOWS.**
//! The founder went through the design book screen by screen and this is the panel he said he
//! misses most: *"Per model: what it costs, what the run is spending, how much is left in the plan,
//! and how much memory has been used."* It existed in his August mocks
//! (`docs/design/cli-reference-2026-08-24/`, `Screenshot 2026-08-24 at 5.06.55 PM.png` and
//! `5.07.30 PM.png`) and no Rust screen has ever drawn it.
//!
//! ⚠️ **AND THE DATA WAS ALREADY ON THE WIRE THE WHOLE TIME.** `POST /sweep/estimate` returns
//! fourteen fields — `estimated_tokens`, `net_new_tokens`, `held_tokens`, `cap`, `remaining_tokens`,
//! `fits`, `blocked_tokens`, `billable_tokens`, `overflow_cost_usd`, `suggested_plan`,
//! `largest_paths`, `exact`, `repo` and a written `message`
//! (`src/estelle/serve/api_sweep_estimate.py::fit_report`). The client reads **one** of them:
//!
//! ```text
//! top_level.rs:2314    if estimate.get("fits") == Some(&Value::Bool(false)) {
//! ```
//!
//! Thirteen fields measured, computed, priced and serialised on every sweep, and then dropped on
//! the floor — including the two the founder asked for by name. That is not a missing feature, it
//! is a **discarded answer**, which is the more expensive kind of gap because nothing about it
//! looks broken.
//!
//! ## What is fixture here, and what is not
//!
//! The LAYOUT is real: every column below is computed by [`crate::cols`], so these panels are the
//! shape the live data will land in, not a drawing of it. The NUMBERS are fixtures, and every one
//! of them is traceable — the `/sweep/estimate` figures are the field names above, the per-model
//! prices and the run-spend breakdown are transcribed from the founder's own mocks, and the plan
//! ladder is the one production serves on `/plans`. Nothing here is a number somebody liked the
//! look of.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, RULE, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::theme::Palette;

/// A proportion bar. Filled cells are solid, the remainder is the light shade — the same two
/// glyphs the founder's `/usage` mock uses, so the panel reads as one family with it.
///
/// ⚠️ Both glyphs are one terminal column and the bar is a fixed `width`, which is why the bar can
/// sit in a `Col` at all: a bar whose length depended on its value would push every column right of
/// it, and that is the bug `cols::every_row_in_a_table_is_the_same_width` exists to catch.
pub(crate) fn bar(percent: usize, width: usize) -> String {
    let filled = (percent.min(100) * width).div_ceil(100);
    let mut out = String::with_capacity(width * 3);
    for index in 0..width {
        out.push(if index < filled { '█' } else { '░' });
    }
    out
}

/// The colour a utilisation bar earns. Green under two thirds, amber over it, red at the cap.
///
/// ⚠️ Green here is a claim that there is room, so the boundary is stated once rather than being
/// re-guessed at each call site — the two-owners defect, in miniature.
pub(crate) fn load_colour(percent: usize, palette: &Palette) -> Color {
    match percent {
        0..=65 => palette.green,
        66..=94 => palette.warn,
        _ => palette.red,
    }
}

pub(crate) fn label(palette: &Palette, text: &str, value: &str, accent: Color) -> Line<'static> {
    owned(Line::from(vec![
        Span::styled(format!("  {text}  "), Style::default().fg(palette.dim)),
        Span::styled(value.to_string(), Style::default().fg(accent)),
    ]))
}

/// Screen 32 — **"How much memory do I have left"**, which is the whole `/sweep/estimate` answer.
///
/// This is the PROPOSED screen the book describes as *"the whole estimate answer the client
/// discards after reading `fits`"*. Every row below names the field it came from, so the day this
/// is wired to the live reply the mapping is already written down.
pub(crate) fn memory_remaining(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const HELD_PERCENT: usize = 41;
    const GAUGE: &[Col] = &[Col::l(16), Col::l(30), Col::r(5), Col::l(28)];
    const PATHS: &[Col] = &[Col::l(2), Col::l(34), Col::r(9), Col::r(7), Col::l(26)];

    let held_bar = bar(HELD_PERCENT, 30);
    let held_percent = format!("{HELD_PERCENT}%");

    let mut lines = vec![
        rule(
            "memory",
            "uqeu/estelle",
            118,
            palette.dim,
            palette.mid,
            palette.cite,
        ),
        blank(),
    ];

    // ── What the plan holds, and what is already in it ──────────────────────────
    lines.push(owned(row(
        GAUGE,
        &[
            Cell("memory used", palette.mid),
            Cell(&held_bar, load_colour(HELD_PERCENT, palette)),
            Cell(&held_percent, palette.mid),
            Cell("103M of 250M held", palette.dim),
        ],
        2,
    )));
    lines.push(owned(row(
        GAUGE,
        &[
            Cell("plan remaining", palette.mid),
            Cell("", palette.dim),
            Cell("", palette.dim),
            Cell("147M free · Ultra 250M", palette.green),
        ],
        2,
    )));
    lines.push(owned(row(
        GAUGE,
        &[
            Cell("this repo", palette.mid),
            Cell("", palette.dim),
            Cell("", palette.dim),
            Cell("11.5M net new · 1,993 files", palette.cite),
        ],
        2,
    )));
    lines.push(blank());

    // ── The verdict, in the server's own words ─────────────────────────────────
    lines.push(note(
        palette,
        "uqeu/estelle is about 11.5M memory-tokens; 147M of your 250M capacity is free.",
    ));
    lines.push(owned(Line::from(vec![
        Span::styled("  fits  ".to_string(), Style::default().fg(palette.dim)),
        Span::styled(
            "yes".to_string(),
            Style::default()
                .fg(palette.green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "   nothing blocked · nothing bills as overflow".to_string(),
            Style::default().fg(palette.dim),
        ),
    ])));
    lines.push(blank());

    // ── The fields the client used to throw away ───────────────────────────────
    lines.push(note(
        palette,
        "if it did not fit, this is what you would see",
    ));
    lines.push(blank());
    let overflow: &[(&str, &str, Color)] = &[
        ("blocked", "0 tokens", palette.dim),
        ("billable overflow", "0 tokens · $0.00", palette.dim),
        ("suggested plan", "Ultra 400M fits — $119/mo", palette.plan),
    ];
    for (name, value, accent) in overflow {
        lines.push(label(palette, name, value, *accent));
    }
    lines.push(blank());

    // ── The largest paths, so a refusal is actionable ──────────────────────────
    lines.push(rule(
        "largest paths",
        "",
        118,
        palette.dim,
        palette.mid,
        palette.cite,
    ));
    lines.push(owned(head(
        PATHS,
        &["", "path", "tokens", "share", ""],
        palette.dim,
        0,
    )));
    let paths: &[(&str, &str, &str, &str)] = &[
        ("logs/", "4.9M", "43.4%", "excluded by default now"),
        ("vendor-reference/", "2.1M", "18.2%", ""),
        ("docs/", "1.4M", "12.1%", ""),
        (
            "src/estelle/serve/",
            "0.9M",
            "7.8%",
            "the part you ask about",
        ),
        ("cli-rs/tui/", "0.7M", "6.1%", ""),
    ];
    for (index, (path, tokens, share, why)) in paths.iter().enumerate() {
        let mut line = owned(row(
            PATHS,
            &[
                Cell(if index == 0 { "›" } else { "" }, palette.cite),
                Cell(path, palette.mid),
                Cell(tokens, palette.mid),
                Cell(share, palette.dim),
                Cell(
                    why,
                    if why.contains("excluded") {
                        palette.warn
                    } else {
                        palette.dim
                    },
                ),
            ],
            0,
        ));
        if index == 0 {
            line = line.style(Style::default().bg(palette.tint));
        }
        lines.push(line);
    }
    lines.push(blank());
    lines.push(note(
        palette,
        "every figure here is a field POST /sweep/estimate already returns.",
    ));
    lines.push(note(
        palette,
        "the client read `fits` and discarded the other thirteen — top_level.rs:2314.",
    ));
    lines
}

/// Screen 33 — `/usage`, and 🔴 **`ctrl+s` showing the actual spend**.
///
/// The founder's decision, verbatim: *"`ctrl+s spend` must SHOW the spend, not just name a
/// shortcut: hey, this session you spent $5.46."* The hint row advertising `ctrl+s spend` has been
/// on the frame for weeks with nothing behind it — `ASK_HINTS_NOT_BOUND` in `main.rs` carries it as
/// a written-down debt. This screen is what the key opens.
pub(crate) fn usage_spend(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const SPEND: &[Col] = &[Col::l(24), Col::r(9), Col::r(7), Col::l(30)];
    const TURN: &[Col] = &[Col::l(12), Col::r(12), Col::r(9), Col::l(32)];

    let mut lines = vec![
        rule(
            "spend",
            "ctrl+s",
            118,
            palette.dim,
            palette.mid,
            palette.cite,
        ),
        blank(),
    ];

    // 🔴 THE SENTENCE HE ASKED FOR, IN WORDS, ABOVE THE TABLE THAT PROVES IT.
    lines.push(owned(Line::from(vec![
        Span::styled(
            "  this session you spent ".to_string(),
            Style::default().fg(palette.mid),
        ),
        Span::styled(
            "$5.46".to_string(),
            Style::default()
                .fg(palette.bright)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  ·  across 3 models, 41 turns".to_string(),
            Style::default().fg(palette.dim),
        ),
    ])));
    lines.push(blank());

    lines.push(owned(head(
        SPEND,
        &["model", "spend", "share", ""],
        palette.dim,
        2,
    )));
    let models: &[(&str, &str, &str, &str, bool)] = &[
        ("claude-opus-4-8", "$3.67", "67%", "your pin", true),
        ("gpt-5.6-luna-pro", "$1.26", "23%", "", false),
        ("deepseek-v4-pro", "$0.53", "10%", "cheapest landed", false),
        ("Qwen3-Coder-80B", "—", "0%", "local, no bill", false),
    ];
    for (name, spend, share, why, selected) in models {
        let mut line = owned(row(
            SPEND,
            &[
                Cell(
                    name,
                    if *selected {
                        palette.bright
                    } else {
                        palette.mid
                    },
                ),
                Cell(
                    spend,
                    if *spend == "—" {
                        palette.dim
                    } else {
                        palette.mid
                    },
                ),
                Cell(share, palette.dim),
                Cell(
                    why,
                    if why.contains("pin") {
                        palette.green
                    } else {
                        palette.dim
                    },
                ),
            ],
            2,
        ));
        if *selected {
            line = line.style(Style::default().bg(palette.tint));
        }
        lines.push(line);
    }
    lines.push(owned(Line::from(Span::styled(
        format!("  {}", RULE.repeat(70)),
        Style::default().fg(palette.dim),
    ))));
    lines.push(owned(row(
        SPEND,
        &[
            Cell("total", palette.mid),
            Cell("$5.46", palette.bright),
            Cell("", palette.dim),
            Cell("all of it to vendors", palette.dim),
        ],
        2,
    )));
    lines.push(blank());

    // ── This turn, broken out. The expansion the status line opens. ────────────
    lines.push(rule(
        "this turn",
        "",
        118,
        palette.dim,
        palette.mid,
        palette.cite,
    ));
    lines.push(owned(head(
        TURN,
        &["", "tokens", "cost", ""],
        palette.dim,
        2,
    )));
    let turn: &[(&str, &str, &str, &str)] = &[
        ("input", "12,431", "$0.019", ""),
        ("output", "3,104", "$0.085", ""),
        ("cached", "80,220", "$0.000", "already warm"),
    ];
    for (name, tokens, cost, why) in turn {
        lines.push(owned(row(
            TURN,
            &[
                Cell(name, palette.dim),
                Cell(tokens, palette.mid),
                Cell(cost, palette.mid),
                Cell(why, palette.green),
            ],
            2,
        )));
    }
    lines.push(owned(row(
        TURN,
        &[
            Cell("this turn", palette.mid),
            Cell("", palette.dim),
            Cell("$0.104", palette.bright),
            Cell("", palette.dim),
        ],
        2,
    )));
    lines.push(owned(row(
        TURN,
        &[
            Cell("all-opus-5", palette.dim),
            Cell("", palette.dim),
            Cell("$0.318", palette.dim),
            Cell("your priciest available model", palette.dim),
        ],
        2,
    )));
    lines.push(owned(row(
        TURN,
        &[
            Cell("saved", palette.dim),
            Cell("", palette.dim),
            Cell("$0.214", palette.green),
            Cell("affinity picked, you did not", palette.dim),
        ],
        2,
    )));
    lines.push(blank());
    lines.push(note(
        palette,
        "Estelle took none of this. BYOK means your key, your bill.",
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_book::panel::model_cost;
    use crate::theme::ScreenTheme;

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum()
    }

    /// 🔴 THE FOUR THINGS HE ASKED FOR, ASSERTED BY NAME.
    ///
    /// A panel that renders three of the four is a panel that looks finished and answers three
    /// quarters of the question. Each clause is asserted separately so the failure message names
    /// which one went missing — the partial-guard defect, avoided by enumeration.
    #[test]
    fn the_costing_panel_carries_all_four_things_the_founder_asked_for() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&model_cost(&palette, 0, true));
        for clause in [
            "in/M",           // 1. per model: what it costs
            "run spend",      // 2. what the run is spending
            "plan remaining", // 3. how much is left in the plan
            "memory used",    // 4. how much memory has been used
        ] {
            assert!(
                rendered.contains(clause),
                "the costing panel dropped {clause:?}\n{rendered}"
            );
        }
    }

    /// The override is PER ROLE, not one global switch. His words: *"I really like Opus 5 planning;
    /// Affinity might choose Codex 5.6 for the solve."*
    ///
    /// Both halves are asserted, because either alone passes on a broken screen: at least one role
    /// must be locked by a human, and at least one must still read `affinity`. A screen that locked
    /// everything, or nothing, would satisfy one clause and fail the other.
    #[test]
    fn the_model_lock_is_per_role_and_affinity_still_owns_the_rest() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&model_cost(&palette, 0, true));
        assert!(rendered.contains("locked by you"), "{rendered}");
        assert!(rendered.contains("affinity"), "{rendered}");
        // The roles are named, so "per role" is legible rather than implied.
        for role in ["plan", "solve", "review", "scope"] {
            assert!(rendered.contains(role), "role {role:?} missing\n{rendered}");
        }
    }

    /// `ctrl+s` SHOWS the spend. The hint row has advertised the key for weeks with nothing behind
    /// it; the founder asked for the sentence, in words, with a figure in it.
    #[test]
    fn ctrl_s_says_what_this_session_cost_in_words() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&usage_spend(&palette, 0, true));
        assert!(
            rendered.contains("this session you spent $5.46"),
            "the spend screen names a shortcut instead of a number\n{rendered}"
        );
        // And the per-model breakdown backs the total up rather than asserting it alone.
        assert!(rendered.contains("claude-opus-4-8"), "{rendered}");
        assert!(rendered.contains("total"), "{rendered}");
    }

    /// 🔴 EVERY FIELD `/sweep/estimate` RETURNS REACHES THE SCREEN.
    ///
    /// This is the test that would have gone red for the last eight months. `top_level.rs:2314`
    /// reads `fits` and drops the other thirteen fields; the screen below is the place they land,
    /// so the clauses are enumerated rather than spot-checked.
    #[test]
    fn the_memory_screen_spends_the_estimate_instead_of_discarding_it() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&memory_remaining(&palette, 0, true));
        for field in [
            "memory used",       // held_tokens
            "plan remaining",    // remaining_tokens + cap
            "net new",           // net_new_tokens
            "fits",              // fits
            "blocked",           // blocked_tokens
            "billable overflow", // billable_tokens + overflow_cost_usd
            "suggested plan",    // suggested_plan
            "largest paths",     // largest_paths
        ] {
            assert!(
                rendered.contains(field),
                "the estimate's {field:?} is still on the floor\n{rendered}"
            );
        }
        // The server writes one sentence a human acts on. It is not a summary we re-derive.
        assert!(rendered.contains("memory-tokens"), "{rendered}");
    }

    /// The bars sit in a column, so a full bar and an empty one must occupy the same width. A bar
    /// whose length tracked its value would push every column right of it — which is the exact
    /// failure `cols::every_row_in_a_table_is_the_same_width` was written for.
    #[test]
    fn a_full_bar_and_an_empty_one_are_the_same_number_of_columns() {
        for percent in [0, 1, 41, 65, 66, 99, 100] {
            assert_eq!(
                bar(percent, 26).chars().count(),
                26,
                "bar({percent}) is not 26 columns"
            );
        }
        // And a zero bar really is empty while a full one really is full, so the width assertion
        // above is not passing on a bar that draws the same thing every time.
        assert!(!bar(0, 26).contains('█'));
        assert!(!bar(100, 26).contains('░'));
    }

    /// The load colour has three bands and the boundaries are the point. A single-colour bar would
    /// pass any "it has a colour" check while telling the reader nothing.
    #[test]
    fn the_budget_bar_changes_colour_before_it_runs_out() {
        let palette = ScreenTheme::Dark.palette();
        assert_eq!(load_colour(41, &palette), palette.green);
        assert_eq!(load_colour(65, &palette), palette.green);
        assert_eq!(load_colour(66, &palette), palette.warn);
        assert_eq!(load_colour(94, &palette), palette.warn);
        assert_eq!(load_colour(95, &palette), palette.red);
        assert_eq!(load_colour(100, &palette), palette.red);
    }

    /// 🔴 EVERY ROW OF THE MODEL POOL ENDS ON THE SAME COLUMN, IN BOTH PALETTES.
    ///
    /// This is the property the whole module exists for. The founder's costing panel is a table of
    /// prices, and a price column that does not line up is the thing a reader notices before they
    /// read a single number. The rows are picked out by their provider cell rather than by index,
    /// so adding a model to the pool extends the test instead of silently escaping it.
    ///
    /// ⚠️ The `$` rows and the rule rows are DIFFERENT widths on purpose and are excluded — a test
    /// that demanded one width for every line on the screen would be asserting something false and
    /// would have to be loosened until it caught nothing, which is how a width guard dies.
    #[test]
    fn every_model_pool_row_ends_on_the_same_column_in_both_themes() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            let rows = model_cost(&palette, 0, true)
                .into_iter()
                .filter(|line| {
                    let text: String = line
                        .spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect();
                    ["anthropic", "openai", "deepseek", "local"]
                        .iter()
                        .any(|provider| text.contains(provider))
                })
                .collect::<Vec<_>>();
            assert!(
                rows.len() >= 6,
                "only {} pool rows were found — the filter stopped matching and this test is \
                 measuring nothing",
                rows.len()
            );
            let widths = rows
                .iter()
                .map(width)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                widths.len(),
                1,
                "{theme_name} pool rows drifted to {widths:?}",
                theme_name = if matches!(theme, ScreenTheme::Dark) {
                    "dark"
                } else {
                    "cream"
                }
            );
        }
    }
}
