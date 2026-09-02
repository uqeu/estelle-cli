//! `POST /sweep/estimate`, read whole — all fourteen fields, not just `fits`.
//!
//! 🔴 **THIS IS NOT A MISSING CONTRACT. IT IS A DISCARDED ANSWER.**
//! `src/estelle/serve/api_sweep_estimate.py::fit_report` measures, prices and serialises fourteen
//! fields on every single sweep — `repo`, `estimated_tokens`, `net_new_tokens`, `held_tokens`,
//! `cap`, `remaining_tokens`, `fits`, `blocked_tokens`, `billable_tokens`, `overflow_cost_usd`,
//! `suggested_plan`, `largest_paths`, `exact` and a written `message`. Until this module the client
//! read **one** of them:
//!
//! ```text
//! top_level.rs   if estimate.get("fits") == Some(&Value::Bool(false)) {
//! ```
//!
//! Thirteen fields computed on the server and dropped on the floor by the client — including
//! *"how much memory do I have left"*, which the founder asked for by name. Nothing about that gap
//! looks broken, which is why it survived: no error, no empty screen, no failing test.
//!
//! ## The redaction filter that ate the numbers
//!
//! ⚠️ The refusal path was worse than silent. It rendered the reply through
//! `top_level::concise_value`, whose `sensitive_key` guard drops any key whose name contains
//! `token` — a filter written for API keys, applied to a body whose six most important fields are
//! **token COUNTS**. So `estimated_tokens`, `net_new_tokens`, `held_tokens`, `remaining_tokens`,
//! `blocked_tokens` and `billable_tokens` were all struck out as if they were credentials, and the
//! remaining keys were cut to the first five — which drops `message`, the one sentence the server
//! writes for a human. A user was told their sweep did not fit, with no number saying by how much
//! and no sentence saying what to do. **One meaning per name**: a token you spend and a token you
//! must never print are two different words that happen to be spelled alike.
//!
//! This module is an ALLOWLIST rather than a filter: it names every field it prints, so a field
//! nobody designed for cannot reach the terminal, and a redaction rule cannot silently eat a
//! measurement it was never aimed at.
//!
//! ## Two bills, never added
//!
//! Everything here is the **Estelle plan** — memory capacity, and PAYG overflow that Estelle
//! invoices. It is not run spend, which is the customer's own key billed by their own vendor.
//! [`BILL_OWNER`] says so on the frame, once, because a cost with no owner named is a cost a reader
//! will add to the other one.
//!
//! ## What is re-derived here, and what is not
//!
//! The VERDICT is never recomputed. `fits`, `blocked_tokens`, `billable_tokens`,
//! `overflow_cost_usd` and `suggested_plan` are read as sent: `decide_capacity` prices a funded
//! account's overflow instead of blocking it, so a client that inferred `fits` from the numbers
//! would contradict the gate that actually admits the sweep — two owners of one derived fact.
//! Only the FORMATTING is ours, and [`human_tokens`] deliberately mirrors the server's own
//! `human_tokens` so the table and the server's sentence do not print the same number two ways.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

use crate::cols::{Cell, Col, head, owned, row as col_row, rule};
use crate::theme::Palette;

/// How many dominating directories are printed. The server sends its own top five (`_TOP_PATHS`);
/// this is the CLIENT's bound on a list that arrives over a wire, stated once, so a server that
/// grows its own limit cannot grow this output without somebody changing this line.
const TOP_PATHS_SHOWN: usize = 5;

/// What an UNKNOWN value prints as.
///
/// ⛔ **NEVER `0`, NEVER `$0.00`.** Absent and zero must not share bytes: a cost the server
/// measured as nothing and a cost the server never sent are opposite facts, and printing them alike
/// is the defect this repo has paid for six times.
pub(crate) const UNKNOWN: &str = "—";

/// The width the label column is padded to, so the values line up in a terminal without a table
/// engine. A named constant because it appears in both the row builder and the paths block.
const LABEL_WIDTH: usize = 18;

/// 🔴 **THE TWO BILLS ARE NEVER ADDED.** Said on the frame whenever plan capacity is drawn.
pub(crate) const BILL_OWNER: &str =
    "memory is the Estelle plan bill · run spend is your key, billed by your vendor";

/// `n` memory-tokens as the short human label the pricing page uses.
///
/// ⚠️ This mirrors `api_sweep_estimate.human_tokens` on purpose. The server's `message` field is
/// already humanised by that function, and this table sits directly under that sentence: a client
/// with its own rounding would print `114.5M` beside the server's `115M` for one number, and a
/// reader cannot tell a formatting difference from a measurement difference.
fn human_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M")
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0).replace(".0K", "K")
    } else {
        n.to_string()
    }
}

/// A monthly price the way the plan page writes it: `49`, not `49.00`.
///
/// ⚠️ Mirrors the server's `${monthly_usd:g}` in `_message` for the same reason [`human_tokens`]
/// mirrors its counterpart — the server's sentence and this table sit on the same screen, and a
/// customer must not see one plan quoted at two prices because two formatters disagreed.
fn price(usd: f64) -> String {
    if usd.fract() == 0.0 {
        format!("{usd:.0}")
    } else {
        format!("{usd:.2}")
    }
}

/// One labelled row. Every field the server sends gets one, present or not — an enumerated list is
/// the only shape in which "thirteen of them are missing" is visible rather than invisible.
fn row(label: &str, value: impl AsRef<str>) -> String {
    format!("  {label:<LABEL_WIDTH$}{}", value.as_ref())
}

/// A token count as text, or [`UNKNOWN`] when the server did not send the field at all.
fn tokens(estimate: &Value, field: &str) -> String {
    estimate
        .get(field)
        .and_then(Value::as_i64)
        .map_or_else(|| UNKNOWN.to_string(), human_tokens)
}

/// `cap` and `remaining_tokens` on one row, with the two sentinels the server's contract defines.
///
/// 🔴 **THREE STATES, NOT TWO.** `cap == 0` means UNLIMITED (matches `plans`/`metering`), and
/// `remaining_tokens: null` means the same. A MISSING key means a server that never sent one.
/// Printing `0` for the first inverts the meaning — "no capacity at all" for "no ceiling at all" —
/// and printing `unlimited` for the third invents an answer.
fn capacity_row(estimate: &Value) -> String {
    let cap = estimate.get("cap").and_then(Value::as_i64);
    let held = estimate.get("held_tokens").and_then(Value::as_i64);
    let remaining = estimate.get("remaining_tokens");
    let cap_text = match cap {
        None => UNKNOWN.to_string(),
        Some(0) => "unlimited".to_string(),
        Some(value) => human_tokens(value),
    };
    let free = match (remaining, cap) {
        (None, _) => UNKNOWN.to_string(),
        (Some(Value::Null), _) | (_, Some(0)) => "unlimited".to_string(),
        (Some(value), _) => value
            .as_i64()
            .map_or_else(|| UNKNOWN.to_string(), human_tokens),
    };
    let held_text = held.map_or_else(|| UNKNOWN.to_string(), human_tokens);
    // The one derived figure on the screen, and it is derived only when BOTH of its inputs are
    // present and the cap is a real ceiling. A percentage of an unknown is not a percentage.
    let used = match (held, cap) {
        (Some(held), Some(cap)) if cap > 0 => {
            format!("  ({}% used)", held.saturating_mul(100) / cap)
        }
        _ => String::new(),
    };
    format!("{cap_text} held {held_text} · {free} free{used}")
}

/// `billable_tokens` + `overflow_cost_usd`, which are one fact in two fields.
///
/// ⛔ A measured zero says **nothing bills**; it does not say `$0.00`, because a price of nothing
/// reads as a price somebody computed and a reader then looks for what it was charged against.
fn overflow_row(estimate: &Value) -> String {
    let billable = estimate.get("billable_tokens").and_then(Value::as_i64);
    let cost = estimate.get("overflow_cost_usd").and_then(Value::as_f64);
    match (billable, cost) {
        (None, None) => UNKNOWN.to_string(),
        (Some(0), Some(0.0)) => "nothing bills as overflow".to_string(),
        (billable, cost) => format!(
            "{} billable · {} PAYG, invoiced by Estelle",
            billable.map_or_else(|| UNKNOWN.to_string(), human_tokens),
            cost.map_or_else(|| UNKNOWN.to_string(), |cost| format!("${cost:.2}")),
        ),
    }
}

/// `suggested_plan`, whose three states are again distinct: an object is a recommendation, `null`
/// is the server saying none is needed, and an absent key is no answer at all.
fn suggested_row(estimate: &Value) -> String {
    match estimate.get("suggested_plan") {
        None => UNKNOWN.to_string(),
        Some(Value::Null) => "not needed".to_string(),
        Some(plan) => {
            let name = plan.get("plan").and_then(Value::as_str).unwrap_or(UNKNOWN);
            let cap = plan
                .get("cap")
                .and_then(Value::as_i64)
                .map_or_else(|| UNKNOWN.to_string(), human_tokens);
            let price = plan
                .get("monthly_usd")
                .and_then(Value::as_f64)
                .map_or_else(|| UNKNOWN.to_string(), |usd| format!("${}/mo", price(usd)));
            format!("{name} ({cap}) — {price}")
        }
    }
}

/// `exact`, which is the difference between a guess and the billing unit and must never be silent.
fn unit_row(estimate: &Value) -> String {
    match estimate.get("exact").and_then(Value::as_bool) {
        None => UNKNOWN.to_string(),
        Some(true) => "counted from file content".to_string(),
        Some(false) => "estimated from file sizes (~4 chars/token, runs ~8% low)".to_string(),
    }
}

/// The whole `/sweep/estimate` reply as printable lines — the answer the client used to throw away.
///
/// One row per field the server sends, in the server's own order, plus its written `message` and
/// the bill-owner note. A field this function does not name is not printed at all.
pub(crate) fn estimate_lines(estimate: &Value) -> Vec<String> {
    let repo = estimate
        .get("repo")
        .and_then(Value::as_str)
        .filter(|repo| !repo.trim().is_empty())
        .unwrap_or(UNKNOWN);
    let fits = match estimate.get("fits").and_then(Value::as_bool) {
        None => UNKNOWN,
        Some(true) => "yes",
        Some(false) => "no",
    };

    let mut lines = vec![
        row("repo", repo),
        row(
            "this repo",
            format!("{} memory-tokens", tokens(estimate, "estimated_tokens")),
        ),
        row(
            "new to memory",
            format!("{} net new", tokens(estimate, "net_new_tokens")),
        ),
        row("plan capacity", capacity_row(estimate)),
        row("fits", fits),
        row("blocked", tokens(estimate, "blocked_tokens")),
        row("overflow", overflow_row(estimate)),
        row("suggested plan", suggested_row(estimate)),
        row("measured", unit_row(estimate)),
    ];

    match estimate.get("largest_paths").and_then(Value::as_array) {
        None => lines.push(row("largest paths", UNKNOWN)),
        Some(paths) => {
            lines.push(row("largest paths", format!("{} listed", paths.len())));
            for path in paths.iter().take(TOP_PATHS_SHOWN) {
                let name = path.get("path").and_then(Value::as_str).unwrap_or(UNKNOWN);
                let held = path
                    .get("tokens")
                    .and_then(Value::as_i64)
                    .map_or_else(|| UNKNOWN.to_string(), human_tokens);
                let files = path
                    .get("files")
                    .and_then(Value::as_i64)
                    .map_or_else(|| UNKNOWN.to_string(), |files| files.to_string());
                lines.push(format!(
                    "  {:LABEL_WIDTH$}{name}  {held}  ({files} files)",
                    ""
                ));
            }
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "  {}",
        estimate
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(UNKNOWN)
    ));
    lines.push(format!("  {BILL_OWNER}"));
    lines
}

// ── The same fourteen fields, drawn ────────────────────────────────────────────────────────────

/// A proportion bar. Filled cells solid, the remainder the light shade.
///
/// 🔴 **THE PRODUCTION OWNER OF THE GAUGE.** It was written in `design_book/costing.rs`, which made
/// the DESIGN BOOK the owner of a glyph the product needs — the same inversion `design_book::owned`
/// was moved out of the book to fix. `costing` and `panel` re-export it from here, so their call
/// sites are unchanged and there is one definition.
///
/// ⚠️ Both glyphs are one terminal column and the bar is a fixed `width`, which is why the bar can
/// sit in a [`Col`] at all: a bar whose length depended on its value would push every column right
/// of it out of alignment.
pub(crate) fn bar(percent: usize, width: usize) -> String {
    let filled = (percent.min(100) * width).div_ceil(100);
    (0..width)
        .map(|index| if index < filled { '█' } else { '░' })
        .collect()
}

/// The colour a utilisation bar earns. Green under two thirds, amber over it, red at the cap.
///
/// ⚠️ Green here is a claim that there is room, so the boundary is stated once rather than
/// re-guessed at each call site.
pub(crate) fn load_colour(percent: usize, palette: &Palette) -> Color {
    match percent {
        0..=65 => palette.green,
        66..=94 => palette.warn,
        _ => palette.red,
    }
}

/// The panel width. The rules stop short of a 130-column frame so it has a margin, not a seam.
const PANEL_WIDTH: usize = 118;

/// How many largest-path rows the PANEL draws. Separate from [`TOP_PATHS_SHOWN`] on purpose: that
/// bounds a text dump, this bounds a fixed-height pane, and tying them together would mean a pane
/// growing because somebody widened a log.
const PANEL_PATHS_SHOWN: usize = 5;

/// One dim line of prose, indented two.
fn note(palette: &Palette, text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(palette.dim),
    ))
}

/// `held / cap` as a whole percent — `None` unless BOTH are present and the cap is a real ceiling.
///
/// ⛔ **AN UNLIMITED PLAN HAS NO PERCENTAGE, AND NEITHER HAS AN ABSENT ONE.** `cap == 0` means
/// unlimited (the server's own sentinel, mirrored in [`capacity_row`]); a percentage of unlimited
/// is not `0%`, it is a question with no answer, and a `0%` bar reads as "nothing used".
fn used_percent(estimate: &Value) -> Option<usize> {
    let held = estimate.get("held_tokens").and_then(Value::as_i64)?;
    let cap = estimate.get("cap").and_then(Value::as_i64)?;
    (cap > 0).then(|| usize::try_from(held.saturating_mul(100) / cap).unwrap_or(100))
}

/// A field's share of `estimated_tokens`, as `43.4%` — or [`UNKNOWN`] when either side is absent.
///
/// ⚠️ Derived, and derived HERE only. The server does not send a share; a second call site
/// computing one against a different denominator would put two percentages for one path on one
/// screen.
fn share_of_total(tokens: Option<i64>, total: Option<i64>) -> String {
    match (tokens, total) {
        (Some(tokens), Some(total)) if total > 0 => {
            format!("{:.1}%", tokens as f64 * 100.0 / total as f64)
        }
        _ => UNKNOWN.to_string(),
    }
}

/// **The whole `/sweep/estimate` answer as a drawn panel — screen 30 of the design book.**
///
/// 🔴 **ONE RENDERER, TWO DATA SOURCES.** The live cost pane feeds this the body the server sent;
/// `design_book::costing::memory_remaining` feeds it a fixture in the same shape. Before this, the
/// book drew its own version of this panel and the badge over it claimed the product produced those
/// numbers — the two-owners shape, with a `LIVE DATA` label on top.
///
/// 🔴 **AND THE LIVE PANE WAS KEEPING FOUR FIELDS OF FOURTEEN.** `capacity_from_value` parses
/// `held_tokens`, `cap`, `remaining_tokens` and `exact`, and drops `repo`, `estimated_tokens`,
/// `net_new_tokens`, `fits`, `blocked_tokens`, `billable_tokens`, `overflow_cost_usd`,
/// `suggested_plan`, `largest_paths` and `message`. Ten measured fields, including the sentence the
/// server writes for a human and the list that makes a refusal actionable, thrown away by the pane
/// whose whole job is to answer *"how much memory do I have left"*.
///
/// ⚠️ **EVERY VALUE IS READ, NOT RE-DERIVED.** `fits`, `blocked_tokens`, `billable_tokens`,
/// `overflow_cost_usd` and `suggested_plan` come off the wire as sent — a client that recomputed
/// the verdict would contradict the gate that actually admits the sweep. The only two derived
/// figures on the frame are the utilisation percentage and each path's share, and both are
/// `None`/[`UNKNOWN`] rather than `0` when an input is missing.
pub(crate) fn estimate_panel(estimate: &Value, palette: &Palette) -> Vec<Line<'static>> {
    // ⚠️ **THREE COLUMNS, NOT FOUR — THE PERCENTAGE HAS ONE OWNER.** The first version drew the
    // utilisation figure in its own `Col::r(5)` AND again inside `capacity_row`'s
    // `(41% used)`, so one derived number appeared twice on one row and a change to either
    // formatter would have printed two answers side by side. `capacity_row` keeps it, because that
    // string is also what the stdout sweep path prints; the bar beside it is the picture, not a
    // second claim.
    //
    // ⚠️ The value column is 55. At 28 `capacity_row`'s longest honest output —
    // `250M held 103M · 147M free  (41% used)`, 38 columns — came off the frame as
    // `250M held 103M · 147M free …`, ELIDING the remaining capacity, which is the one number the
    // screen exists to show. 16+2+30+2+55 = 105 inside a 118-column rule.
    const GAUGE: &[Col] = &[Col::l(16), Col::l(30), Col::l(55)];
    const PATHS: &[Col] = &[Col::l(2), Col::l(34), Col::r(9), Col::r(7), Col::l(26)];

    let repo = estimate
        .get("repo")
        .and_then(Value::as_str)
        .filter(|repo| !repo.trim().is_empty())
        .unwrap_or(UNKNOWN)
        .to_string();
    let total = estimate.get("estimated_tokens").and_then(Value::as_i64);

    let mut lines = vec![
        owned(rule(
            "memory",
            &repo,
            PANEL_WIDTH,
            palette.dim,
            palette.mid,
            palette.cite,
        )),
        Line::from(""),
    ];

    // ── What the plan holds, and what is already in it ──────────────────────────
    let percent = used_percent(estimate);
    // ⛔ An unknown utilisation draws NO bar, not an empty one: a 0%-filled bar is a picture of
    // "nothing used", which is the opposite of "not measured".
    let gauge = percent.map(|percent| bar(percent, 30)).unwrap_or_default();
    lines.push(owned(col_row(
        GAUGE,
        &[
            Cell("memory used", palette.mid),
            Cell(&gauge, load_colour(percent.unwrap_or_default(), palette)),
            Cell(&capacity_row(estimate), palette.dim),
        ],
        2,
    )));
    lines.push(owned(col_row(
        GAUGE,
        &[
            Cell("this repo", palette.mid),
            Cell("", palette.dim),
            Cell(
                &format!(
                    "{} memory-tokens · {} net new",
                    tokens(estimate, "estimated_tokens"),
                    tokens(estimate, "net_new_tokens")
                ),
                palette.cite,
            ),
        ],
        2,
    )));
    lines.push(owned(col_row(
        GAUGE,
        &[
            Cell("measured", palette.mid),
            Cell("", palette.dim),
            Cell(&unit_row(estimate), palette.dim),
        ],
        2,
    )));
    lines.push(Line::from(""));

    // ── The verdict, in the server's own words ─────────────────────────────────
    lines.push(note(
        palette,
        estimate
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(UNKNOWN),
    ));
    let (verdict, verdict_colour) = match estimate.get("fits").and_then(Value::as_bool) {
        None => (UNKNOWN, palette.dim),
        Some(true) => ("yes", palette.green),
        Some(false) => ("no", palette.red),
    };
    lines.push(Line::from(vec![
        Span::styled("  fits  ".to_string(), Style::default().fg(palette.dim)),
        Span::styled(
            verdict.to_string(),
            Style::default()
                .fg(verdict_colour)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   {}", overflow_row(estimate)),
            Style::default().fg(palette.dim),
        ),
    ]));
    lines.push(Line::from(""));

    // ── The fields a refusal turns on ──────────────────────────────────────────
    for (name, value, accent) in [
        ("blocked", tokens(estimate, "blocked_tokens"), palette.dim),
        ("suggested plan", suggested_row(estimate), palette.plan),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("  {name}  "), Style::default().fg(palette.dim)),
            Span::styled(value, Style::default().fg(accent)),
        ]));
    }
    lines.push(Line::from(""));

    // ── The largest paths, so a refusal is actionable ──────────────────────────
    lines.push(owned(rule(
        "largest paths",
        "",
        PANEL_WIDTH,
        palette.dim,
        palette.mid,
        palette.cite,
    )));
    lines.push(owned(head(
        PATHS,
        &["", "path", "tokens", "share", ""],
        palette.dim,
        0,
    )));
    match estimate.get("largest_paths").and_then(Value::as_array) {
        // ⛔ An absent list is not an empty one. "the server sent no list" and "this repo has no
        // dominating directory" are opposite facts and must not draw the same blank table.
        None => lines.push(note(palette, "largest paths   —   the server sent no list")),
        Some(paths) if paths.is_empty() => {
            lines.push(note(palette, "no directory dominates this repo"));
        }
        Some(paths) => {
            for (index, path) in paths.iter().take(PANEL_PATHS_SHOWN).enumerate() {
                let name = path.get("path").and_then(Value::as_str).unwrap_or(UNKNOWN);
                let held = path.get("tokens").and_then(Value::as_i64);
                let files = path
                    .get("files")
                    .and_then(Value::as_i64)
                    .map_or_else(|| UNKNOWN.to_string(), |files| format!("{files} files"));
                let mut line = owned(col_row(
                    PATHS,
                    &[
                        Cell(if index == 0 { "›" } else { "" }, palette.cite),
                        Cell(name, palette.mid),
                        Cell(
                            &held.map_or_else(|| UNKNOWN.to_string(), human_tokens),
                            palette.mid,
                        ),
                        Cell(&share_of_total(held, total), palette.dim),
                        Cell(&files, palette.dim),
                    ],
                    0,
                ));
                if index == 0 {
                    line = line.style(Style::default().bg(palette.tint));
                }
                lines.push(line);
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(note(palette, BILL_OWNER));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 🔴 **THE FIXTURES ARE THE REAL SERVER'S OUTPUT, NOT A DRAWING OF IT.** Each was produced by
    /// calling the shipped function, so a test double cannot be friendlier than production:
    ///
    /// ```text
    /// cd <estelle>; PYTHONPATH=src:scripts python3 -c "
    /// from estelle.serve.api_sweep_estimate import fit_report
    /// from estelle.serve.capacity import CapacityState
    /// from estelle.serve.plans import OVERFLOW_RATE_PER_MILLION
    /// print(fit_report(repo='uqeu/estelle', state=CapacityState(held=103_000_000,
    ///   cap=250_000_000, has_funds=True, rate_per_million=OVERFLOW_RATE_PER_MILLION,
    ///   plan_cap=250_000_000, extra=0), estimated_tokens=114_500_000,
    ///   per_path=[('logs/a.txt', 4_900_000), ('docs/b.md', 1_400_000)], exact=False))"
    /// ```
    const FITS: &str = r#"{
      "repo": "uqeu/estelle", "estimated_tokens": 114500000, "net_new_tokens": 11500000,
      "held_tokens": 103000000, "cap": 250000000, "remaining_tokens": 147000000, "fits": true,
      "blocked_tokens": 0, "billable_tokens": 0, "overflow_cost_usd": 0.0,
      "suggested_plan": null,
      "largest_paths": [{"path": "logs/", "tokens": 4900000, "files": 1},
                        {"path": "docs/", "tokens": 1400000, "files": 1}],
      "exact": false,
      "message": "uqeu/estelle is about 114.5M memory-tokens; 147M of your 250M capacity is free."
    }"#;

    /// The same function, on an unfunded account the sweep does not fit.
    const BLOCKED: &str = r#"{
      "repo": "acme/monolith", "estimated_tokens": 70000000, "net_new_tokens": 68000000,
      "held_tokens": 2000000, "cap": 10000000, "remaining_tokens": 8000000, "fits": false,
      "blocked_tokens": 60000000, "billable_tokens": 0, "overflow_cost_usd": 0.0,
      "suggested_plan": {"plan": "ultra", "monthly_usd": 49.0,
                         "memory_tokens": 100000000, "cap": 100000000},
      "largest_paths": [{"path": "vendor/", "tokens": 41000000, "files": 1},
                        {"path": "src/", "tokens": 9000000, "files": 1}],
      "exact": false,
      "message": "acme/monolith is about 70M memory-tokens; your plan holds 10M (8M free), so 60M would not fit. ultra (100M) fits — $49/mo. Or sweep a narrower path and exclude the biggest directories: vendor/ (41M), src/ (9M)."
    }"#;

    /// 🔴 **THE SHAPE THAT PROVES `fits` MUST NOT BE RECOMPUTED.** Same repo, same numbers, a FUNDED
    /// account: `decide_capacity` prices the 60M as PAYG overflow instead of blocking it, so the
    /// server answers `fits: true` **with 60M billable**. A client that inferred the verdict from
    /// the token counts would refuse a sweep the server just admitted. This is production output,
    /// not a hypothetical.
    const OVERFLOW: &str = r#"{
      "repo": "acme/monolith", "estimated_tokens": 70000000, "net_new_tokens": 68000000,
      "held_tokens": 2000000, "cap": 10000000, "remaining_tokens": 8000000, "fits": true,
      "blocked_tokens": 0, "billable_tokens": 60000000, "overflow_cost_usd": 21.0,
      "suggested_plan": null,
      "largest_paths": [{"path": "vendor/", "tokens": 41000000, "files": 1}],
      "exact": false,
      "message": "acme/monolith is about 70M memory-tokens — 60M of it is past your 10M cap and bills as PAYG overflow (about $21.00)."
    }"#;

    fn reply(raw: &str) -> Value {
        serde_json::from_str(raw).expect("fixture is the server's own JSON")
    }

    fn text(estimate: &Value) -> String {
        estimate_lines(estimate).join("\n")
    }

    /// The line whose label column starts with `label` — so an assertion can name a FIELD rather
    /// than a substring that may have come from anywhere on the screen.
    fn line_for(rendered: &str, label: &str) -> String {
        rendered
            .lines()
            .find(|line| line.trim_start().starts_with(label))
            .unwrap_or_else(|| panic!("no row labelled {label}:\n{rendered}"))
            .to_string()
    }

    /// 🔴 **THE HEADLINE CLAUSE — ONE ASSERTION PER FIELD, BY NAME.** Fourteen fields are measured
    /// and serialised on every sweep and the client read one. Enumerated rather than counted, so a
    /// field that stops being rendered cannot pass as a smaller diff.
    #[test]
    fn every_one_of_the_fourteen_fields_reaches_the_reader() {
        let rendered = text(&reply(BLOCKED));
        for (field, needle) in [
            ("repo", "acme/monolith"),
            ("this repo", "70M"),          // estimated_tokens
            ("new to memory", "68M"),      // net_new_tokens
            ("plan capacity", "2M"),       // held_tokens
            ("fits", "no"),                // fits
            ("blocked", "60M"),            // blocked_tokens
            ("overflow", "nothing bills"), // billable_tokens + overflow_cost_usd
            ("suggested plan", "ultra"),   // suggested_plan
            ("measured", "estimated"),     // exact
            ("largest paths", "2 listed"), // largest_paths
        ] {
            assert!(
                line_for(&rendered, field).contains(needle),
                "field {field} lost its value {needle}:\n{rendered}"
            );
        }
        // cap and remaining_tokens share the capacity row; the message is printed verbatim.
        let capacity = line_for(&rendered, "plan capacity");
        assert!(capacity.contains("10M"), "cap missing:\n{rendered}");
        assert!(
            capacity.contains("8M free"),
            "remaining missing:\n{rendered}"
        );
        assert!(
            rendered.contains("vendor/"),
            "largest path rows missing:\n{rendered}"
        );
        assert!(
            rendered.contains("would not fit"),
            "message missing:\n{rendered}"
        );
    }

    /// ⛔ **AN UNKNOWN COST IS NEVER `$0.00`.**
    #[test]
    fn an_absent_cost_prints_a_dash_and_never_a_zero_dollar() {
        let mut estimate = reply(BLOCKED);
        let object = estimate.as_object_mut().expect("object");
        object.remove("overflow_cost_usd");
        object.remove("billable_tokens");
        let rendered = text(&estimate);
        assert!(
            !rendered.contains("$0.00"),
            "a price invented for an absent one:\n{rendered}"
        );
        let overflow = line_for(&rendered, "overflow");
        assert!(overflow.contains(UNKNOWN), "{overflow}");
        assert!(
            !overflow.contains('$'),
            "a dollar figure over two absent fields:\n{overflow}"
        );
    }

    /// A cost measured AS zero is a statement, not a gap — and the sentence a reader needs is
    /// *nothing bills*, not a price of nothing.
    #[test]
    fn a_measured_zero_overflow_says_nothing_bills() {
        let rendered = text(&reply(FITS));
        assert!(
            line_for(&rendered, "overflow").contains("nothing bills as overflow"),
            "{rendered}"
        );
        assert!(!rendered.contains("$0.00"), "{rendered}");
    }

    /// `cap == 0` and `remaining_tokens == null` are the server's word for UNLIMITED. Printing
    /// either as `0` inverts the meaning: no capacity at all, for no ceiling at all.
    #[test]
    fn unlimited_capacity_never_prints_as_zero() {
        let mut estimate = reply(FITS);
        estimate["cap"] = json!(0);
        estimate["remaining_tokens"] = Value::Null;
        let capacity = line_for(&text(&estimate), "plan capacity");
        assert!(
            capacity.contains("unlimited held 103M · unlimited free"),
            "{capacity}"
        );
        assert!(!capacity.contains(" 0 "), "{capacity}");
        assert!(
            !capacity.contains("% used"),
            "a percentage of no ceiling:\n{capacity}"
        );
    }

    /// 🔴 **ABSENT IS NOT NULL.** `remaining_tokens: null` means unlimited; a MISSING key means a
    /// build that never sent one. Opposite answers; they must not print alike.
    #[test]
    fn an_absent_remaining_is_unknown_not_unlimited() {
        let mut estimate = reply(FITS);
        estimate
            .as_object_mut()
            .expect("object")
            .remove("remaining_tokens");
        let capacity = line_for(&text(&estimate), "plan capacity");
        assert!(capacity.contains(UNKNOWN), "{capacity}");
        assert!(
            !capacity.contains("unlimited"),
            "an absent field read as unlimited:\n{capacity}"
        );
    }

    /// 🔴 **THE VERDICT IS THE SERVER'S, NEVER RE-DERIVED** — driven by the real funded-overflow
    /// reply, where the numbers look like a block and the answer is `fits: true`.
    ///
    /// ⚠️ **Stated limit, from the mutation run.** This kills the derivation a client would
    /// plausibly write — `remaining_tokens >= net_new_tokens`, which reads 8M >= 68M and answers
    /// NO over a server that answered YES. It does NOT kill `blocked_tokens == 0`, because the
    /// server sets `blocked_tokens` to zero at exactly the moment it decides to bill instead of
    /// block, so that particular second owner happens to agree. It is still a second owner, and it
    /// is still forbidden; this test is simply not the thing that would catch it.
    #[test]
    fn fits_is_read_from_the_field_and_not_recomputed() {
        let rendered = text(&reply(OVERFLOW));
        let estimate = reply(OVERFLOW);
        let remaining = estimate["remaining_tokens"].as_i64().expect("remaining");
        let net_new = estimate["net_new_tokens"].as_i64().expect("net new");
        assert!(
            remaining < net_new,
            "the fixture must disagree with capacity arithmetic"
        );
        assert!(line_for(&rendered, "fits").contains("yes"), "{rendered}");
        assert!(
            line_for(&rendered, "overflow").contains("60M billable"),
            "{rendered}"
        );
        assert!(
            line_for(&rendered, "overflow").contains("$21.00"),
            "{rendered}"
        );
        assert!(
            line_for(&rendered, "overflow").contains("invoiced by Estelle"),
            "{rendered}"
        );
    }

    /// 🔴 **THE REGRESSION THAT MADE THIS WORTH DOING.** `concise_value`'s `sensitive_key` guard
    /// drops any key containing `token`, so every token COUNT was struck out as if it were a token
    /// SECRET and the refusal named no number at all.
    #[test]
    fn the_token_counts_are_not_redacted_as_credentials() {
        let rendered = text(&reply(BLOCKED));
        for (field, needle) in [
            ("this repo", "70M"),
            ("new to memory", "68M"),
            ("plan capacity", "2M"),
            ("blocked", "60M"),
        ] {
            assert!(
                line_for(&rendered, field).contains(needle),
                "a token COUNT was filtered as a token SECRET: {field}\n{rendered}"
            );
        }
    }

    /// 🔴 **TWO BILLS, NEVER ADDED.**
    #[test]
    fn the_screen_names_which_bill_it_is_showing() {
        let rendered = text(&reply(FITS));
        assert!(rendered.contains("Estelle plan bill"), "{rendered}");
        assert!(rendered.contains("billed by your vendor"), "{rendered}");
    }

    /// Every loop has a stated bound and the bound is a named constant.
    #[test]
    fn the_largest_paths_table_is_bounded_by_a_named_constant() {
        let mut estimate = reply(FITS);
        estimate["largest_paths"] = Value::Array(
            (0..50)
                .map(|index| json!({"path": format!("d{index}/"), "tokens": 1000, "files": 1}))
                .collect(),
        );
        let rendered = text(&estimate);
        let drawn = (0..50)
            .filter(|index| rendered.contains(&format!("d{index}/")))
            .count();
        // 🔴 **THE EXPECTED NUMBER IS WRITTEN OUT, NOT READ FROM THE CONSTANT.** The first draft
        // asserted `drawn == TOP_PATHS_SHOWN`, and the mutation run killed it: raising the constant
        // to 7 moved BOTH sides of the comparison and nothing went red. A derived expectation
        // cannot catch a regression in the thing it derives from.
        assert_eq!(
            TOP_PATHS_SHOWN, 5,
            "the bound moved — change the literal below deliberately"
        );
        assert_eq!(drawn, 5, "{rendered}");
        // …and the reader still says how many there were, so a truncation cannot read as a total.
        assert!(
            line_for(&rendered, "largest paths").contains("50 listed"),
            "{rendered}"
        );
    }

    /// 🔴 **AN ALLOWLIST, NOT A FILTER.** The old path enumerated whatever the object happened to
    /// carry and subtracted what looked sensitive — a redaction rule written for a shape somebody
    /// assumed. This reader names what it prints.
    #[test]
    fn a_field_this_reader_does_not_name_is_never_echoed() {
        let mut estimate = reply(FITS);
        estimate["debug_upstream_header"] = json!("SENTINEL_THAT_MUST_NOT_REACH_THE_TERMINAL");
        let rendered = text(&estimate);
        assert!(
            !rendered.contains("SENTINEL_THAT_MUST_NOT_REACH_THE_TERMINAL"),
            "{rendered}"
        );
        assert!(!rendered.contains("debug_upstream_header"), "{rendered}");
    }

    /// The server writes one sentence for a human, and `concise_value` cut it: it takes the first
    /// five surviving keys and `message` is the fourteenth.
    #[test]
    fn the_servers_own_sentence_is_printed_verbatim() {
        let rendered = text(&reply(FITS));
        assert!(
            rendered.contains(
                "uqeu/estelle is about 114.5M memory-tokens; 147M of your 250M capacity is free."
            ),
            "{rendered}"
        );
    }

    /// 🔴 **A READER WITH NO CALLER IS THE DEFECT THIS MODULE EXISTS TO FIX.** Twelve green tests
    /// over a function nobody calls would reproduce the exact shape of the bug — a capability
    /// measured, serialised and never read — one layer further out. So the wiring is asserted too,
    /// and it is asserted on the SOURCE because the call sits behind an `async` network round trip
    /// that no unit test reaches.
    ///
    /// ⚠️ Stated limit: this proves the call site EXISTS, not that it runs. The behaviour on both
    /// branches is covered by the twelve tests above; what this adds is that `estimate_lines` has
    /// a caller at all, and that the refusal no longer goes through the redactor that ate the
    /// numbers.
    #[test]
    fn the_sweep_path_actually_calls_this_reader() {
        let caller = include_str!("top_level.rs");
        assert!(
            caller.contains("crate::sweep_estimate::estimate_lines(&estimate)"),
            "the estimate reader has no caller — the same gap, one layer out"
        );
        let refusal = caller
            .split("does not fit the account capacity")
            .nth(1)
            .expect("the refusal branch");
        assert!(
            !refusal[..refusal.len().min(200)].contains("concise_value"),
            "the refusal went back through the filter that redacts token COUNTS as token SECRETS"
        );
    }

    /// An unreadable estimate is not a fitting one. Nothing to print is still a printed line.
    #[test]
    fn an_empty_reply_says_so_rather_than_printing_nothing() {
        let rendered = text(&json!({}));
        assert!(!rendered.is_empty(), "silence over an unreadable estimate");
        assert!(line_for(&rendered, "fits").contains(UNKNOWN), "{rendered}");
        assert!(
            line_for(&rendered, "plan capacity").contains(UNKNOWN),
            "{rendered}"
        );
    }
}
