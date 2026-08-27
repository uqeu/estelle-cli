use estelle_client::WorkPlan;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row};
use crate::theme::Palette;

const PLAN_COLUMNS: &[Col] = &[Col::l(2), Col::l(4), Col::l(45), Col::l(41)];

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
        head(
            PLAN_COLUMNS,
            &["", "state", "step", "evidence"],
            palette.dim,
            0,
        ),
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
            PLAN_COLUMNS,
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
