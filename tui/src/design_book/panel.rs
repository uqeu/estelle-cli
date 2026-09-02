//! Screen 33b — the costing panel the founder said he misses most, on one screen.
//!
//! 🔴 **SPLIT OUT OF `costing.rs` WHEN THAT FILE PASSED 800 LINES.** The house limit is 800 hard,
//! `cargo fmt` only ever adds rows, and a file over the limit stops being read and starts being
//! skimmed. The three costing screens were one module because they share a vocabulary; they are
//! two modules now because they no longer share a screen-full of it.
//!
//! ⚠️ The bar, the load colour and the label row still live in [`super::costing`] and are imported
//! rather than copied. A helper duplicated during a split is a second owner nobody asked for, and
//! two owners of one derived fact disagree within the week.

use ratatui::style::Style;
use ratatui::text::Line;

use crate::cols::{Cell, Col, head, row, rule};
use crate::design_book::costing::{bar, load_colour};
use crate::design_book::{blank, note, owned};
use crate::theme::Palette;

/// 🔴 **THE COSTING PANEL ITSELF** — screen 33b, the four things he asked for, on one screen.
///
/// *"Per model: what it costs, what the run is spending, how much is left in the plan, and how much
/// memory has been used."* All four, side by side, because the point of the panel is the
/// COMPARISON: a per-Mtok price means nothing without the run it is being spent on, and a run spend
/// means nothing without the budget it is drawing down.
///
/// ⚠️ **AFFINITY DECIDES BY DEFAULT AND THE HUMAN CAN OVERRIDE, PER ROLE.** The founder was
/// explicit that this is not one global switch: *"I really like Opus 5 planning; Affinity might
/// choose Codex 5.6 for the solve."* So the lock column is per ROLE — a `plan` lock and a `solve`
/// lock — and an unlocked role reads `affinity`, never a blank.
pub(crate) fn model_cost(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const POOL: &[Col] = &[
        Col::l(2),
        Col::l(20),
        Col::l(10),
        Col::r(7),
        Col::r(8),
        Col::r(9),
        Col::l(9),
        Col::l(26),
    ];
    const BUDGET: &[Col] = &[Col::l(15), Col::l(26), Col::r(5), Col::l(40)];
    const MEMORY_PERCENT: usize = 41;
    const RUN_PERCENT: usize = 12;

    let memory_bar = bar(MEMORY_PERCENT, 26);
    let memory_percent = format!("{MEMORY_PERCENT}%");
    let run_bar = bar(RUN_PERCENT, 26);
    let run_percent = format!("{RUN_PERCENT}%");

    let mut lines = vec![
        rule(
            "cost",
            "ctrl+m locks a role",
            118,
            palette.dim,
            palette.mid,
            palette.cite,
        ),
        blank(),
        owned(head(
            POOL,
            &[
                "",
                "model",
                "provider",
                "in/M",
                "out/M",
                "run spend",
                "role",
                "",
            ],
            palette.dim,
            0,
        )),
    ];

    // `role` is the override, and `affinity` is what it reads when nobody has overridden it.
    let pool: &[(&str, &str, &str, &str, &str, &str, &str, &str)] = &[
        (
            "●",
            "claude-opus-5",
            "anthropic",
            "$5.00",
            "$25.00",
            "$2.41",
            "plan",
            "locked by you",
        ),
        (
            "●",
            "gpt-5.6-codex",
            "openai",
            "$5.00",
            "$25.00",
            "$1.26",
            "solve",
            "affinity",
        ),
        (
            "●",
            "claude-sonnet-5",
            "anthropic",
            "$3.00",
            "$15.00",
            "$1.26",
            "solve",
            "affinity · cost pick",
        ),
        (
            "●",
            "deepseek-v4-pro",
            "deepseek",
            "$0.95",
            "$3.80",
            "$0.53",
            "review",
            "affinity",
        ),
        (
            "●",
            "haiku-4.5",
            "anthropic",
            "$0.80",
            "$4.00",
            "$0.00",
            "scope",
            "affinity",
        ),
        (
            "○",
            "Qwen3-Coder-80B",
            "local",
            "—",
            "—",
            "—",
            "—",
            "this machine, no bill",
        ),
    ];
    for (index, (mark, name, provider, input, output, spend, role, note_text)) in
        pool.iter().enumerate()
    {
        let locked = note_text.contains("locked");
        let mut line = owned(row(
            POOL,
            &[
                Cell(
                    mark,
                    if *mark == "●" {
                        palette.green
                    } else {
                        palette.dim
                    },
                ),
                Cell(name, if locked { palette.bright } else { palette.mid }),
                Cell(provider, palette.dim),
                Cell(input, palette.mid),
                Cell(output, palette.mid),
                Cell(
                    spend,
                    if *spend == "—" {
                        palette.dim
                    } else {
                        palette.bright
                    },
                ),
                Cell(role, if locked { palette.warn } else { palette.plan }),
                Cell(note_text, if locked { palette.warn } else { palette.dim }),
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
        "affinity decides by default. enter locks the highlighted model to its role; only that role.",
    ));
    lines.push(blank());

    // ── The budget half. Always visible, which is the founder's global rule 2. ──
    lines.push(rule(
        "budget",
        "always visible",
        118,
        palette.dim,
        palette.mid,
        palette.plan,
    ));
    lines.push(blank());
    lines.push(owned(row(
        BUDGET,
        &[
            Cell("run spend", palette.mid),
            Cell(&run_bar, load_colour(RUN_PERCENT, palette)),
            Cell(&run_percent, palette.mid),
            Cell("$5.46 this session · $45 soft cap", palette.dim),
        ],
        2,
    )));
    lines.push(owned(row(
        BUDGET,
        &[
            Cell("memory used", palette.mid),
            Cell(&memory_bar, load_colour(MEMORY_PERCENT, palette)),
            Cell(&memory_percent, palette.mid),
            Cell("103M of 250M · 6 repos held", palette.dim),
        ],
        2,
    )));
    lines.push(owned(row(
        BUDGET,
        &[
            Cell("plan remaining", palette.mid),
            Cell("", palette.dim),
            Cell("", palette.dim),
            Cell("147M free · resets 1 Sep", palette.green),
        ],
        2,
    )));
    lines.push(blank());
    lines.push(note(
        palette,
        "run spend is YOUR key billed by YOUR vendor. memory is the Estelle plan. they are two bills.",
    ));
    lines.push(note(
        palette,
        "a provider that does not publish a limit gets a dash here, never an estimate.",
    ));
    lines
}
