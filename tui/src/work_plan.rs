use estelle_client::WorkPlan;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row};
use crate::theme::Palette;

/// The catalog's plan table: glyph, state, step, evidence. The two text columns are `45:41`
/// there, and that ratio is what survives a narrower surface — never the absolute widths, or
/// the evidence column (the half that makes the plan honest) is the half that gets truncated.
const PLAN_GLYPH: usize = 2;
const PLAN_STATE: usize = 4;
const PLAN_STEP: usize = 45;
const PLAN_EVIDENCE: usize = 41;
const PLAN_GAP: usize = 2;
/// `2 + 2 + 4 + 2 + 45 + 2 + 41`
pub(crate) const PLAN_WIDTH: usize =
    PLAN_GLYPH + PLAN_GAP + PLAN_STATE + PLAN_GAP + PLAN_STEP + PLAN_GAP + PLAN_EVIDENCE;

fn plan_columns(width: usize) -> [Col; 4] {
    let fixed = PLAN_GLYPH + PLAN_GAP + PLAN_STATE + PLAN_GAP + PLAN_GAP;
    let text = width.saturating_sub(fixed).max(PLAN_GLYPH);
    let step = (text * PLAN_STEP / (PLAN_STEP + PLAN_EVIDENCE)).max(1);
    let evidence = text.saturating_sub(step).max(1);
    [
        Col::l(PLAN_GLYPH),
        Col::l(PLAN_STATE),
        Col::l(step),
        Col::l(evidence),
    ]
}

fn status(status: &str) -> (&'static str, &'static str) {
    match status {
        "complete" => ("✓", "done"),
        "active" => ("●", "now"),
        "blocked" => ("!", "stop"),
        "protected" => ("▲", "hand"),
        _ => ("☐", "next"),
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

pub(crate) fn lines(plan: &WorkPlan, palette: &Palette) -> Vec<Line<'static>> {
    lines_at(plan, palette, PLAN_WIDTH)
}

/// The plan sized to the surface that will show it. The live session column is narrower than
/// the catalog's page, and a plan whose evidence column falls off the right edge is a plan
/// with no evidence in it.
pub(crate) fn lines_at(plan: &WorkPlan, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let columns = plan_columns(width);
    let mut output = vec![
        Line::from(vec![
            Span::styled(
                "THE PLAN",
                Style::default()
                    .fg(palette.mid)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  · revision {}", plan.revision),
                Style::default().fg(palette.dim),
            ),
        ]),
        head(&columns, &["", "state", "step", "evidence"], palette.dim, 0),
    ];
    for step in &plan.steps {
        let (glyph, state) = status(&step.status);
        let evidence = if step.evidence.trim().is_empty() {
            "— unevidenced"
        } else {
            step.evidence.as_str()
        };
        let glyph_color = match step.status.as_str() {
            "complete" => palette.green,
            "active" => palette.cite,
            "blocked" | "protected" => palette.warn,
            _ => palette.dim,
        };
        output.push(owned(row(
            &columns,
            &[
                Cell(glyph, glyph_color),
                Cell(state, palette.dim),
                Cell(&step.step, palette.mid),
                Cell(evidence, palette.cite),
            ],
            0,
        )));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;

    #[test]
    fn plan_renders_status_evidence_and_the_protected_deploy_headlessly() {
        let plan: WorkPlan = serde_json::from_value(serde_json::json!({
            "revision": 3,
            "steps": [
                {"id": "1", "step": "Inspect parser", "status": "complete", "evidence": "parser.py:parse"},
                {"id": "2", "step": "Write negative control", "status": "active", "evidence": "tests/test_parser.py"},
                {"id": "3", "step": "Show missing proof", "status": "pending", "evidence": ""},
                {"id": "4", "step": "Deploy", "status": "protected", "evidence": "scripts/deploy.sh"}
            ]
        })).expect("plan fixture");
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(lines(&plan, &ScreenTheme::Dark.palette())),
                    frame.area(),
                );
            })
            .expect("render plan");
        let frame = format!("{}", terminal.backend());

        assert!(frame.contains("THE PLAN"));
        assert!(frame.contains("✓") && frame.contains("●") && frame.contains("☐"));
        assert!(frame.contains("▲") && frame.contains("Deploy"));
        assert!(frame.contains("parser.py:parse"));
        assert!(frame.contains("— unevidenced"));
    }

    #[test]
    fn every_plan_status_glyph_is_one_terminal_column() {
        for glyph in ["✓", "●", "☐", "!", "▲"] {
            assert_eq!(unicode_width::UnicodeWidthStr::width(glyph), 1, "{glyph:?}");
        }
    }
}
