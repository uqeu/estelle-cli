//! The Orchestra worker table, in the catalog's design language, for BOTH callers.
//!
//! 🔴 **THIS EXISTS SO THE CATALOG AND THE LIVE SESSION CANNOT DRAW DIFFERENT WORKER TABLES.**
//! Screen 9 of `screens.rs` ("everything at once") drew the design's worker rows — glyph, worker,
//! model, elapsed, cost — as **fixture strings**, and the live session drew a five-across grid of
//! plain text re-coloured by keyword matching. Two presentations of one fact, and only the one
//! nobody ships was designed. This module is the single renderer; `screens.rs` and
//! `live_renderer.rs` both call it, so a change to the table changes both by construction.
//!
//! ⚠️ **THE COST COLUMN IS EMPTY BECAUSE THE CONTRACT HAS NO PER-WORKER COST, NOT BECAUSE THE
//! RENDERER FORGOT.** `FleetAgent` (`docs/ORCHESTRA-VIEW-DATA-CONTRACT.md`, "Fleet invariants")
//! carries `index`, `status`, `state_observed_at`, `current_action`, `progress`, `assignments`,
//! `failure_cause` and `attempt` — and **no model and no cost**. The design's
//! `✓ w1 opus-4-8 41s $0.212` therefore cannot be rendered from live data today. The column keeps
//! its place, every cell reads `—`, and [`MISSING_PER_WORKER_SPEND`] names the absent contract on
//! the frame. Filling those cells from the fleet's plan floor divided by worker count would be a
//! fabricated number in the interface whose entire job is refusing those.

use estelle_client::{FleetAgent, FleetAgentStatus, FleetSnapshot};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row};
use crate::theme::Palette;

/// Said on the frame, once, whenever a worker table is drawn. The empty `cost` cells are a
/// measurement that does not exist; this line is where the reader is told which one.
/// ⚠️ Short enough to survive the catalog's own 46-column left mockup uncut. A disclosure that
/// truncates to `no server contrac…` is a disclosure the reader has to guess at; the detail it
/// used to carry (that `FleetAgent` has neither field) lives in this module's docs instead.
pub(crate) const MISSING_PER_WORKER_SPEND: &str = "per-worker model + cost · no server contract";

/// The design's worker row: glyph, worker, state, last seen, cost. The two fixed-width number
/// columns and the glyph are constants; the state column takes whatever is left, because the state
/// text is the half a narrower terminal should spend its columns on.
///
/// 🔴 **`age` WAS A COLUMN HEADING NOBODY COULD READ, AND THE FOUNDER SAID SO.**
/// *"I don't know what age ahead means, so that needs to be explained as well."* He was reading a
/// column headed `age` whose every cell said `ahead`, and both words were ours.
///
/// `age` meant *how long ago the server last observed this worker's state* — `now -
/// state_observed_at` — which is not the worker's run time and reads as if it were. `ahead` meant
/// *the server dated this row in the future relative to your clock*, which is a clock-skew report
/// wearing a one-word disguise. Neither was wrong; both were written for the person who had just
/// implemented them. They are `last seen` and `clock ahead` now.
const GLYPH: usize = 2;
const WORKER: usize = 4;
/// `clock ahead` is eleven columns wide, so the column has to be too — a heading or a value
/// truncated to `clock ah…` would have replaced one unreadable label with another.
const LAST_SEEN: usize = 11;
const COST: usize = 7;
const GAP: usize = 2;
/// `2 + 2 + 4 + 2 + <state> + 2 + 11 + 2 + 7`, with the state column excluded.
const FIXED: usize = GLYPH + GAP + WORKER + GAP + GAP + LAST_SEEN + GAP + COST;
/// The narrowest state cell the table will draw before it simply truncates.
const MIN_STATE: usize = 8;

fn columns(width: usize) -> [Col; 5] {
    let state = width.saturating_sub(FIXED).max(MIN_STATE);
    [
        Col::l(GLYPH),
        Col::l(WORKER),
        Col::l(state),
        Col::r(LAST_SEEN),
        Col::r(COST),
    ]
}

/// The glyph and colour for a worker's state.
///
/// ⚠️ Every terminal outcome gets its OWN glyph: the contract is explicit that "a stopped process
/// is not a successful process" and "a timeout may never render a checkmark".
fn glyph(status: FleetAgentStatus, palette: &Palette) -> (&'static str, Color) {
    match status {
        FleetAgentStatus::Completed => ("✓", palette.green),
        FleetAgentStatus::Running => ("◐", palette.cite),
        FleetAgentStatus::Queued => ("·", palette.dim),
        FleetAgentStatus::Created | FleetAgentStatus::Starting => ("◌", palette.dim),
        FleetAgentStatus::AwaitingApproval => ("◆", palette.warn),
        FleetAgentStatus::Failed | FleetAgentStatus::Killed => ("×", palette.red),
        FleetAgentStatus::TimedOut => ("◷", palette.warn),
        FleetAgentStatus::Blocked => ("!", palette.warn),
        FleetAgentStatus::Cancelled => ("−", palette.dim),
        // ⚠️ `?` used to be a bare literal here and a second bare literal in the todo ledger, in
        // neither enum and in neither test — see `marks::Mark`'s header for what that cost.
        FleetAgentStatus::Lost | FleetAgentStatus::NeedsInput | FleetAgentStatus::Unknown => (
            crate::marks::Mark::Unknown.glyph(),
            crate::marks::Mark::Unknown.colour(palette),
        ),
    }
}

/// The state cell: the worker's own reported action when it has one, else its status word.
///
/// An `unknown` worker prints its `unknown_reason`, because the contract requires one and a row
/// that says only "Unknown" has thrown away the half that says why.
fn state(agent: &FleetAgent) -> String {
    let action = agent
        .current_action
        .as_deref()
        .map(str::trim)
        .filter(|action| !action.is_empty())
        .map(estelle_client::mask_secret);
    let label = match agent.status {
        FleetAgentStatus::Unknown => {
            let reason = agent
                .unknown_reason
                .as_deref()
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .unwrap_or("reason absent");
            return format!("Unknown · {reason}");
        }
        FleetAgentStatus::Created => "Created",
        FleetAgentStatus::Starting => "Starting",
        FleetAgentStatus::Queued => "Queued",
        FleetAgentStatus::Running => "Working",
        FleetAgentStatus::AwaitingApproval => "Awaiting approval",
        FleetAgentStatus::Completed => "Completed",
        FleetAgentStatus::Failed => "Failed",
        FleetAgentStatus::TimedOut => "Timed out",
        FleetAgentStatus::Killed => "Killed",
        FleetAgentStatus::Lost => "Lost",
        FleetAgentStatus::Blocked => "Blocked",
        FleetAgentStatus::NeedsInput => "Needs input",
        FleetAgentStatus::Cancelled => "Cancelled",
    };
    match (agent.progress.as_ref().filter(|it| it.total > 0), action) {
        (Some(progress), Some(action)) => format!(
            "[{}/{}] {action}",
            progress.completed.min(progress.total),
            progress.total
        ),
        (Some(progress), None) => format!(
            "[{}/{}] {label}",
            progress.completed.min(progress.total),
            progress.total
        ),
        (None, Some(action)) => action,
        (None, None) => label.to_string(),
    }
}

/// How long ago this row's state was last observed, in words a reader does not have to be told.
///
/// ⚠️ This is the AGE OF THE OBSERVATION, not the worker's elapsed run time — the design's `41s`
/// column. The contract carries `state_observed_at` and no start time, so the honest column is
/// the one the wire supports. What changed is the NAME: `last seen` says which of those two it is
/// without a footnote, and `age` did not.
fn last_seen(agent: &FleetAgent, now_epoch_s: f64) -> String {
    let elapsed = now_epoch_s - agent.state_observed_at;
    if !elapsed.is_finite() || !agent.state_observed_at.is_finite() {
        return crate::marks::Mark::Unknown.glyph().to_string();
    }
    // ⚠️ A row dated AHEAD of this client's clock has no age. `0s` would claim it was observed
    // just now, which is the one thing we know it was not — so the condition is NAMED. It used to
    // be named `ahead`, one word, which told the reader that something was ahead of something and
    // nothing about what or why. `clock ahead` says which two things disagree.
    if elapsed < 0.0 {
        return "clock ahead".to_string();
    }
    if elapsed < 90.0 {
        format!("{elapsed:.0}s")
    } else if elapsed < 5400.0 {
        format!("{:.0}m", elapsed / 60.0)
    } else {
        format!("{:.0}h", elapsed / 3600.0)
    }
}

fn owned(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
}

/// The whole panel: the task line, the participant roster, the worker table, the missing-spend
/// disclosure, the fleet's plan floor and the completion bar.
pub(crate) fn lines(
    fleet: &FleetSnapshot,
    palette: &Palette,
    width: usize,
    now_epoch_s: f64,
) -> Vec<Line<'static>> {
    let table = columns(width);
    let batch = fleet.batch.trim();
    let batch = if batch.is_empty() {
        "unnamed batch"
    } else {
        batch
    };
    let admitted = fleet
        .total
        .map_or_else(|| "?".to_string(), |total| total.to_string());
    let mut output = vec![
        Line::from(vec![
            // `●`, not Claude Code's `⏺` tool glyph — the design's own filled-circle marker.
            Span::styled("● ", Style::default().fg(palette.green)),
            Span::styled(
                format!("Task({batch} · {admitted} workers)"),
                Style::default()
                    .fg(palette.green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  models · ", Style::default().fg(palette.dim)),
            Span::styled(
                fleet
                    .model_roster()
                    .unwrap_or_else(|| "models unknown".to_string()),
                Style::default().fg(palette.mid),
            ),
        ]),
    ];
    if let Some(narrator) = &fleet.narrator {
        let text = narrator.text.trim();
        if !text.is_empty() {
            let marker = match narrator.evidence {
                estelle_client::FleetEvidence::Measured
                | estelle_client::FleetEvidence::Observed => String::new(),
                estelle_client::FleetEvidence::Derived => "Derived: ".to_string(),
                estelle_client::FleetEvidence::Inferred => "Inferred: ".to_string(),
                estelle_client::FleetEvidence::Unknown => "Unverified: ".to_string(),
            };
            output.push(Line::styled(
                format!("  {marker}{}", estelle_client::mask_secret(text)),
                Style::default().fg(palette.dim),
            ));
        }
    }
    output.push(Line::from(""));
    output.push(head(
        &table,
        &["", "wkr", "state", "last seen", "cost"],
        palette.dim,
        0,
    ));

    for agent in &fleet.agents {
        let (mark, mark_colour) = glyph(agent.status, palette);
        let state_colour = match agent.status {
            FleetAgentStatus::Failed | FleetAgentStatus::Killed => palette.red,
            FleetAgentStatus::Queued | FleetAgentStatus::Cancelled => palette.dim,
            _ => palette.mid,
        };
        output.push(owned(row(
            &table,
            &[
                Cell(mark, mark_colour),
                Cell(&format!("w{}", agent.index), palette.dim),
                Cell(&state(agent), state_colour),
                Cell(&last_seen(agent, now_epoch_s), palette.dim),
                // ⚠️ NOT A ZERO AND NOT A BLANK: an em dash for "the server does not report this".
                Cell("—", palette.dim),
            ],
            0,
        )));
    }
    if fleet.agents.is_empty() {
        output.push(Line::styled(
            "no worker rows in this snapshot",
            Style::default().fg(palette.dim),
        ));
    }

    output.push(Line::styled(
        MISSING_PER_WORKER_SPEND,
        Style::default().fg(palette.dim),
    ));
    output.push(Line::styled(
        fleet
            .plan_floor_line()
            .unwrap_or_else(|| "Plan floor · not reported".to_string()),
        Style::default().fg(palette.dim),
    ));
    output.push(progress_line(fleet, palette, width, now_epoch_s));
    output
}

/// The fleet completion bar. Green to the measured completed fraction, dim beyond it, and the
/// counts spelled out — an unknown total prints `?`, never the received row count dressed up as
/// a denominator.
fn progress_line(
    fleet: &FleetSnapshot,
    palette: &Palette,
    width: usize,
    now_epoch_s: f64,
) -> Line<'static> {
    let total = fleet.total.unwrap_or(fleet.agents.len() as u64);
    let total_label = fleet
        .total
        .map_or_else(|| "?".to_string(), |total| total.to_string());
    let completed = fleet.completed.map(|value| value.min(total));
    let completed_label = completed.map_or_else(|| "?".to_string(), |value| value.to_string());
    let bar_width = width.saturating_sub(24).clamp(8, 48);
    let filled = completed.filter(|_| total > 0).map_or(0, |completed| {
        (completed as usize * bar_width) / total as usize
    });
    let finished = matches!(fleet.state.as_str(), "complete" | "completed");
    let label = if finished { "Completed" } else { "Working..." };
    let spinner = if finished {
        "✓"
    } else {
        ["◐", "◓", "◑", "◒"][((now_epoch_s * 10.0) as usize) % 4]
    };
    Line::from(vec![
        Span::styled(
            format!("{spinner} {label:<10} ["),
            Style::default().fg(if finished {
                palette.green
            } else {
                palette.cite
            }),
        ),
        Span::styled("━".repeat(filled), Style::default().fg(palette.green)),
        Span::styled(
            "─".repeat(bar_width.saturating_sub(filled)),
            Style::default().fg(palette.dim),
        ),
        Span::styled(
            format!("] {completed_label}/{total_label}"),
            Style::default().fg(palette.dim),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;
    use serde_json::json;

    fn fleet(value: serde_json::Value) -> FleetSnapshot {
        serde_json::from_value(value).expect("typed fleet fixture")
    }

    fn text(lines: &[Line<'_>]) -> String {
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

    fn running() -> FleetSnapshot {
        fleet(json!({
            "id": "fleet-41",
            "batch": "gate cluster",
            "models": ["opus-4-8", "sonnet-5", "opus-4-8"],
            "state": "running",
            "observed_at": 4102444800.0,
            "stale_after_s": 60,
            "completed": 1,
            "total": 4,
            "plan_floor_usd": 0.00447,
            "agents": [
                {"index": 1, "status": "completed", "state_observed_at": 4102444759.0, "current_action": "Bound checkout_timeout", "progress": {"completed": 3, "total": 3}},
                {"index": 2, "status": "running", "state_observed_at": 4102444772.0, "current_action": "Reading the retry gate", "progress": {"completed": 2, "total": 4}},
                {"index": 3, "status": "timed_out", "state_observed_at": 4102444769.0, "current_action": "Comparing the proposed patch"},
                {"index": 4, "status": "queued", "state_observed_at": 4102444800.0}
            ]
        }))
    }

    #[test]
    fn every_worker_row_is_the_same_width_at_every_surface_the_live_frame_offers() {
        let palette = ScreenTheme::Dark.palette();
        for width in [46usize, 60, 80, 110, 160] {
            let rendered = lines(&running(), &palette, width, 4102444800.0);
            let widths = rendered
                .iter()
                .skip(4)
                .take(4)
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.chars().count())
                        .sum::<usize>()
                })
                .collect::<Vec<_>>();
            assert_eq!(widths.len(), 4, "width {width} lost worker rows");
            assert!(
                widths.windows(2).all(|pair| pair[0] == pair[1]),
                "width {width} produced ragged worker rows: {widths:?}"
            );
        }
    }

    #[test]
    fn the_cost_column_is_an_absence_and_the_frame_names_the_missing_contract() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&lines(&running(), &palette, 100, 4102444800.0));

        assert!(rendered.contains("cost"), "{rendered}");
        assert_eq!(rendered.matches('—').count(), 4, "{rendered}");
        assert!(rendered.contains(MISSING_PER_WORKER_SPEND), "{rendered}");
        // The one dollar figure on the panel is the fleet's PLAN FLOOR, carrying its own limit.
        assert!(rendered.contains("Plan floor · $0.004470"), "{rendered}");
        assert!(
            rendered.contains("not expected or final spend"),
            "{rendered}"
        );
        assert_eq!(rendered.matches('$').count(), 1, "{rendered}");
    }

    /// 🔴 THE NEGATIVE CONTROL FOR THE ABOVE. If a future edit ever fills the cost cells from a
    /// derived number, the em-dash count changes and the test above goes red — but only if the
    /// dash is really coming from the cost column. This asserts it is, by removing every worker.
    #[test]
    fn a_fleet_with_no_workers_has_no_cost_dashes_and_says_so() {
        let palette = ScreenTheme::Dark.palette();
        let empty = fleet(json!({
            "id": "fleet-empty", "batch": "nothing admitted", "state": "running",
            "observed_at": 4102444800.0, "total": 0, "agents": []
        }));
        let rendered = text(&lines(&empty, &palette, 100, 4102444800.0));

        assert!(!rendered.contains('—'), "{rendered}");
        assert!(
            rendered.contains("no worker rows in this snapshot"),
            "{rendered}"
        );
        assert!(rendered.contains("Plan floor · not reported"), "{rendered}");
        assert!(rendered.contains("models unknown"), "{rendered}");
    }

    #[test]
    fn a_timeout_never_renders_a_checkmark_and_unknown_carries_its_reason() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = lines(&running(), &palette, 100, 4102444800.0);
        let timed_out = rendered
            .iter()
            .find(|line| text(std::slice::from_ref(*line)).contains("Comparing the proposed patch"))
            .map(|line| text(std::slice::from_ref(line)))
            .expect("the timed-out worker row");
        assert!(timed_out.contains('◷'), "{timed_out}");
        assert!(!timed_out.contains('✓'), "{timed_out}");

        let unknown = fleet(json!({
            "id": "f", "batch": "b", "state": "running", "observed_at": 4102444800.0, "total": 1,
            "agents": [{"index": 1, "status": "unknown", "state_observed_at": 4102444800.0,
                        "unknown_reason": "worker has not reported state"}]
        }));
        let rendered = text(&lines(&unknown, &palette, 100, 4102444800.0));
        assert!(
            rendered.contains("Unknown · worker has not reported state"),
            "{rendered}"
        );
    }

    #[test]
    fn the_age_column_reads_the_observation_clock_and_refuses_an_impossible_one() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&lines(&running(), &palette, 100, 4102444800.0));
        assert!(rendered.contains("41s"), "{rendered}");
        assert!(rendered.contains("28s"), "{rendered}");

        let future = fleet(json!({
            "id": "f", "batch": "b", "state": "running", "observed_at": 4102444800.0, "total": 1,
            "agents": [{"index": 1, "status": "running", "state_observed_at": 4102449999.0}]
        }));
        let rendered = text(&lines(&future, &palette, 100, 4102444800.0));
        assert!(rendered.contains("ahead"), "{rendered}");
        assert!(!rendered.contains("0s"), "{rendered}");
    }

    /// Inherited from `fleet_progress_colour_boundary_encodes_the_completed_fraction`, which
    /// asserted this on the deleted keyword-colouring helper. The property is the same; the owner
    /// moved. The bar's colour is now a fact about `completed/total`, not about the characters.
    #[test]
    fn the_progress_bar_colour_boundary_encodes_the_completed_fraction() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = lines(&running(), &palette, 100, 4102444800.0);
        let bar = rendered.last().expect("a progress line");
        let filled = bar
            .spans
            .iter()
            .find(|span| span.content.contains('━'))
            .expect("a completed run");
        let remaining = bar
            .spans
            .iter()
            .find(|span| span.content.contains('─'))
            .expect("an outstanding run");
        assert_eq!(filled.style.fg, Some(palette.green));
        assert_eq!(remaining.style.fg, Some(palette.dim));
        // 1 of 4 completed: the green run must be a quarter of the bar, not all of it.
        let total = filled.content.chars().count() + remaining.content.chars().count();
        assert_eq!(filled.content.chars().count(), total / 4);
    }

    /// Inherited from `fleet_terminal_glyphs_have_distinct_colours_as_well_as_shapes`.
    /// ⚠️ The contract requires distinct terminal outcomes to LOOK distinct in both channels,
    /// so a truecolour-blind terminal still tells a timeout from a completion.
    #[test]
    fn terminal_outcomes_have_distinct_glyphs_as_well_as_colours() {
        let palette = ScreenTheme::Dark.palette();
        let outcomes = [
            FleetAgentStatus::Completed,
            FleetAgentStatus::Failed,
            FleetAgentStatus::TimedOut,
            FleetAgentStatus::Lost,
            FleetAgentStatus::Cancelled,
        ];
        let glyphs = outcomes
            .iter()
            .map(|status| glyph(*status, &palette).0)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            glyphs.len(),
            outcomes.len(),
            "two outcomes share a glyph: {glyphs:?}"
        );
        assert_ne!(
            glyph(FleetAgentStatus::Completed, &palette).1,
            glyph(FleetAgentStatus::TimedOut, &palette).1
        );
        assert_ne!(
            glyph(FleetAgentStatus::Completed, &palette).0,
            "✓".repeat(2)
        );
        assert_eq!(glyph(FleetAgentStatus::Completed, &palette).0, "✓");
        assert_ne!(glyph(FleetAgentStatus::TimedOut, &palette).0, "✓");
    }

    /// ⚠️ TWO OWNERS FOR THE PARTICIPANT ROSTER, PINNED TOGETHER.
    ///
    /// `FleetSnapshot::model_roster` is this panel's owner. `commands::fleet_view_lines` — the
    /// `/orchestra` REPLY text, a different medium with its own tests — still derives the roster
    /// itself. That duplicate could not be collapsed in this change because `commands.rs` carries
    /// another lane's uncommitted diff and removing it would have swept that diff into this
    /// commit. This test is the guard in the meantime: the two owners must agree, on the same
    /// fixture, including the deduplication and the legacy single-`model` fallback.
    #[test]
    fn the_reply_text_and_the_live_table_report_the_same_participant_roster() {
        for fixture in [
            json!({"id": "f", "batch": "b", "state": "running", "observed_at": 1.0, "total": 0,
                   "models": ["opus-4-8", "sonnet-5", "opus-4-8"], "agents": []}),
            json!({"id": "f", "batch": "b", "state": "running", "observed_at": 1.0, "total": 0,
                   "model": "legacy-fallback", "agents": []}),
        ] {
            let snapshot = fleet(fixture);
            let roster = snapshot.model_roster().expect("a named roster");
            let reply = crate::commands::fleet_view_lines(&snapshot, 120).join("\n");
            assert!(
                reply.contains(&format!("Participants · {roster}")),
                "the reply text and the table disagree about the roster\n{reply}"
            );
        }
    }
}
