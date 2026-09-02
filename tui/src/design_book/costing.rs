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

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, RULE, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::theme::Palette;

/// 🔴 **THE GAUGE MOVED TO ITS PRODUCTION OWNER, [`crate::sweep_estimate`].**
///
/// `bar` and `load_colour` were defined here, which made the DESIGN BOOK the owner of two things
/// the shipped capacity panel needs — the same inversion that moved `owned` out of this module and
/// into `cols`. They are re-exported rather than moved-and-updated so `panel.rs`'s existing
/// `use crate::design_book::costing::{bar, load_colour}` keeps working: one definition, no churn.
pub(crate) use crate::sweep_estimate::{bar, load_colour};

/// Screen 30 — **"How much memory do I have left"**, which is the whole `/sweep/estimate` answer.
///
/// 🔴 **THIS FUNCTION DRAWS NOTHING. IT SUPPLIES A FIXTURE AND CALLS THE SHIPPED RENDERER.**
/// It used to build the panel itself, row by row, under a badge claiming the product produced
/// those numbers — two renderers, one screen, and no test comparing them. The layout now lives in
/// [`crate::sweep_estimate::estimate_panel`], which the live cost pane also calls, so this frame is
/// the product's own output over staged data rather than a drawing of it.
///
/// ⚠️ **THE FIXTURE IS THE SERVER'S SHAPE, FIELD FOR FIELD.** All fourteen keys of
/// `api_sweep_estimate.fit_report` are present with the server's own types, including the two
/// nullable ones. A fixture missing a key would render [`crate::sweep_estimate::UNKNOWN`] and the
/// frame would quietly show a dash where the product shows a number — which is why
/// `the_fixture_carries_every_field_the_server_sends` names all fourteen rather than trusting this
/// literal to stay complete.
pub(crate) fn memory_remaining(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    crate::sweep_estimate::estimate_panel(&memory_fixture(), palette)
}

/// A `POST /sweep/estimate` reply for a repo that fits, in the server's exact shape.
///
/// The numbers are this repo measured against the Ultra 250M rung: `uqeu/estelle` is about 11.5M
/// memory-tokens (the market census figure), 103M of 250M is held, and `logs/` dominates at 43.4%
/// — the share the corpus census actually found. They are STAGED, not measured today, which is
/// what the `FIXTURE DATA` gate exists to say.
pub(crate) fn memory_fixture() -> serde_json::Value {
    serde_json::json!({
        "repo": "uqeu/estelle",
        "estimated_tokens": 11_500_000,
        "net_new_tokens": 11_500_000,
        "held_tokens": 103_000_000,
        "cap": 250_000_000,
        "remaining_tokens": 147_000_000,
        "fits": true,
        "blocked_tokens": 0,
        "billable_tokens": 0,
        "overflow_cost_usd": 0.0,
        "suggested_plan": serde_json::Value::Null,
        "largest_paths": [
            {"path": "logs/", "tokens": 4_990_000, "files": 1_204},
            {"path": "vendor-reference/", "tokens": 2_093_000, "files": 318},
            {"path": "docs/", "tokens": 1_391_000, "files": 214},
            {"path": "src/estelle/serve/", "tokens": 897_000, "files": 143},
            {"path": "cli-rs/tui/", "tokens": 701_000, "files": 114}
        ],
        "exact": true,
        "message": "uqeu/estelle is about 11.5M memory-tokens; 147M of your 250M capacity is free."
    })
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

    /// 🔴 **EVERY FIELD `/sweep/estimate` RETURNS REACHES THE SCREEN — PROVEN BY REMOVING IT.**
    ///
    /// This is the test that would have gone red for the last eight months: `top_level.rs` read
    /// `fits` and dropped the other thirteen, and the live cost pane still keeps only four
    /// (`capacity_from_value` parses `held_tokens`, `cap`, `remaining_tokens`, `exact`).
    ///
    /// ⚠️ **IT USED TO ASSERT EIGHT LABELS AND THAT WAS THE WEAKER TEST.** A label is a word this
    /// module chose; matching it proves the word was typed, never that the FIELD was read. Three of
    /// its eight labels went stale the moment the frame started calling the shipped renderer, which
    /// words the same facts differently — a test that fails on a rename and passes on a dropped
    /// field is pointed at the wrong thing.
    ///
    /// So each of the fourteen keys is DELETED from the fixture in turn and the frame must change.
    /// A field the renderer ignores produces an identical frame and fires here by name. This cannot
    /// be satisfied by typing a word, and it cannot go stale when the wording changes.
    #[test]
    fn every_field_the_estimate_returns_changes_the_screen() {
        let palette = ScreenTheme::Dark.palette();
        let whole = text(&memory_remaining(&palette, 0, true));
        let fixture = memory_fixture();

        // The fourteen keys of `api_sweep_estimate.fit_report`, written out rather than derived
        // from the fixture's own keys — a list read off the object it is checking can never notice
        // the object is missing one.
        const FIELDS: [&str; 14] = [
            "repo",
            "estimated_tokens",
            "net_new_tokens",
            "held_tokens",
            "cap",
            "remaining_tokens",
            "fits",
            "blocked_tokens",
            "billable_tokens",
            "overflow_cost_usd",
            "suggested_plan",
            "largest_paths",
            "exact",
            "message",
        ];
        for field in FIELDS {
            let mut without = fixture.clone();
            assert!(
                without
                    .as_object_mut()
                    .expect("the fixture is an object")
                    .remove(field)
                    .is_some(),
                "the fixture never carried {field:?} — the removal below would prove nothing"
            );
            let starved = text(&crate::sweep_estimate::estimate_panel(&without, &palette));
            assert_ne!(
                starved, whole,
                "dropping {field:?} changed nothing on screen — it is still on the floor"
            );
        }
    }

    /// 🔴 **AN ABSENT FIELD DRAWS A DASH, NEVER A ZERO.**
    ///
    /// The differential above proves each field is READ. It cannot prove the frame is HONEST when
    /// one is missing, because any change satisfies it — including `0` or `$0.00`, which is the
    /// defect this repo has paid for repeatedly: a cost the server measured as nothing and a cost
    /// the server never sent are opposite facts.
    ///
    /// ⚠️ The control is the first assertion: the fixture's `blocked_tokens` IS a measured zero, so
    /// `0` must be on the frame when the field is present. Without that, a renderer printing `—`
    /// for everything would pass the second half.
    #[test]
    fn an_absent_number_is_a_dash_and_a_measured_zero_is_a_zero() {
        let palette = ScreenTheme::Dark.palette();
        let measured = text(&memory_remaining(&palette, 0, true));
        assert!(
            measured.contains("blocked  0"),
            "a measured zero stopped printing as 0\n{measured}"
        );

        let mut without = memory_fixture();
        let object = without.as_object_mut().expect("the fixture is an object");
        object.remove("blocked_tokens");
        object.remove("overflow_cost_usd");
        object.remove("billable_tokens");
        let starved = text(&crate::sweep_estimate::estimate_panel(&without, &palette));
        assert!(
            !starved.contains("blocked  0"),
            "an absent count printed as a measured zero\n{starved}"
        );
        assert!(
            starved.contains(crate::sweep_estimate::UNKNOWN),
            "an absent count printed neither a number nor a dash\n{starved}"
        );
    }

    /// 🔴 **NOT ONE FIGURE ON THE CAPACITY PANEL MAY BE ELIDED.**
    ///
    /// `cols::row` ends an overlong cell in `…`, which is right for a model name and wrong for a
    /// token count: `250M held 103M · 147M free  (41% used)` truncated to
    /// `250M held 103M · 147M free …` drops the utilisation figure and the reader cannot tell that
    /// anything is missing. That is exactly what the first version of this panel shipped, at
    /// `Col::l(28)`, and the gallery caught it only because an unrelated needle moved.
    ///
    /// ⚠️ **THE POSITIVE CONTROL IS THE SECOND HALF.** An assertion that `…` is absent passes over
    /// a frame that rendered nothing, so the same rows are re-measured with a deliberately narrow
    /// column set and `…` is asserted PRESENT — proving the detector fires.
    #[test]
    fn no_figure_on_the_capacity_panel_is_truncated() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let rendered = text(&memory_remaining(&theme.palette(), 0, true));
            assert!(
                !rendered.contains('…'),
                "the capacity panel elided a figure\n{rendered}"
            );
            // The whole point of the panel: the free figure and the utilisation both survive.
            let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat.contains("147M free"), "{rendered}");
            assert!(flat.contains("(41% used)"), "{rendered}");
        }
    }

    /// The control for the test above: `cols::row` really does elide, so its absence means
    /// something.
    #[test]
    fn the_truncation_detector_fires_on_a_narrow_column() {
        let palette = ScreenTheme::Dark.palette();
        let narrow = crate::cols::owned(crate::cols::row(
            &[crate::cols::Col::l(10)],
            &[crate::cols::Cell(
                "250M held 103M · 147M free  (41% used)",
                palette.mid,
            )],
            2,
        ));
        let text = narrow
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<String>();
        assert!(
            text.contains('…'),
            "cols::row stopped eliding — the guard above now proves nothing: {text:?}"
        );
    }

    /// The server writes one sentence a human acts on, and it is not a summary we re-derive.
    #[test]
    fn the_servers_own_sentence_is_on_the_frame_verbatim() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&memory_remaining(&palette, 0, true));
        let sentence = memory_fixture()["message"]
            .as_str()
            .expect("the fixture carries a message")
            .to_string();
        let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains(&sentence),
            "the server's sentence did not survive the frame\n{rendered}"
        );
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
