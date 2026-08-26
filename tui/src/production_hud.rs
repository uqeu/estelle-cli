use crate::theme::Palette;
use crate::theme::pulse;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProductionGraph {
    pub issue_key: String,
    pub failing_symbol: String,
    pub failing_file: String,
    pub healthy_subsystems: Vec<String>,
    pub blast_radius: Vec<String>,
    pub chokepoints: Vec<String>,
    pub core_files: Vec<String>,
    pub drill_down: bool,
}

pub fn lines(
    graph: &ProductionGraph,
    palette: &Palette,
    tick: u64,
    pulse_enabled: bool,
) -> Vec<Line<'static>> {
    let mut output = vec![Line::from(vec![
        Span::styled(
            "╌╌ production · code graph ",
            Style::default().fg(palette.dim),
        ),
        Span::styled("╌╌╌╌╌╌╌╌╌╌", Style::default().fg(palette.dim)),
    ])];

    if graph.healthy_subsystems.is_empty() {
        output.push(Line::styled(
            "healthy subsystem context unavailable",
            Style::default().fg(palette.dim),
        ));
    } else {
        output.push(Line::styled(
            "HEALTHY SUBSYSTEMS",
            Style::default().fg(palette.mid),
        ));
        for subsystem in &graph.healthy_subsystems {
            output.push(Line::from(vec![
                Span::styled("● ", Style::default().fg(palette.green)),
                Span::styled(subsystem.clone(), Style::default().fg(palette.green)),
                Span::styled(
                    "  healthy · no unresolved issue bound",
                    Style::default().fg(palette.dim),
                ),
            ]));
        }
    }

    output.push(Line::from(""));
    output.push(Line::styled(
        "FAILING PATH",
        Style::default().fg(palette.mid),
    ));
    output.push(Line::from(vec![
        Span::styled("▲ ", pulse(palette.red, tick, pulse_enabled)),
        Span::styled(
            if graph.failing_symbol.is_empty() {
                "unbound symbol".to_string()
            } else {
                graph.failing_symbol.clone()
            },
            pulse(palette.red, tick, pulse_enabled),
        ),
        Span::styled(
            format!("  {}", graph.failing_file),
            Style::default().fg(palette.dim),
        ),
    ]));

    if graph.blast_radius.is_empty() {
        output.push(Line::styled(
            "  blast radius returned no dependants",
            Style::default().fg(palette.dim),
        ));
    } else {
        for file in &graph.blast_radius {
            output.push(Line::from(vec![
                Span::styled("├─ blast  ", Style::default().fg(palette.warn)),
                Span::styled(file.clone(), Style::default().fg(palette.warn)),
            ]));
        }
    }
    for file in &graph.chokepoints {
        output.push(Line::from(vec![
            Span::styled("├─ choke  ", Style::default().fg(palette.dim)),
            Span::styled(file.clone(), Style::default().fg(palette.mid)),
        ]));
    }
    for file in &graph.core_files {
        output.push(Line::from(vec![
            Span::styled("└─ core   ", Style::default().fg(palette.dim)),
            Span::styled(file.clone(), Style::default().fg(palette.mid)),
        ]));
    }

    output.push(Line::from(""));
    if graph.drill_down {
        output.extend([
            Line::styled("flowchart LR", Style::default().fg(palette.cite)),
            Line::styled(
                "event --> symbol --> diff",
                Style::default().fg(palette.cite),
            ),
            Line::styled(
                format!(
                    "event[production event] --> symbol[{}]",
                    graph.failing_symbol
                ),
                Style::default().fg(palette.dim),
            ),
            Line::styled(
                format!("symbol[{}] --> diff[repair diff]", graph.failing_file),
                Style::default().fg(palette.dim),
            ),
        ]);
        for file in &graph.blast_radius {
            output.push(Line::styled(
                format!("symbol --> impacted[{file}]"),
                Style::default().fg(palette.warn),
            ));
        }
        output.push(Line::styled(
            "Esc returns to the production graph",
            Style::default().fg(palette.dim),
        ));
    } else {
        output.push(Line::styled(
            "Enter opens event → symbol → diff",
            Style::default().fg(palette.dim),
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    #[test]
    fn failing_symbol_is_red_blast_radius_is_amber_and_healthy_context_is_green() {
        let palette = ScreenTheme::Dark.palette();
        let graph = ProductionGraph {
            failing_symbol: "charge_card".to_string(),
            failing_file: "billing.py".to_string(),
            healthy_subsystems: vec!["auth".to_string(), "search".to_string()],
            blast_radius: vec!["checkout.py".to_string()],
            ..Default::default()
        };
        let rendered = lines(&graph, &palette, 0, true);
        let spans = rendered
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| {
            span.content.contains("charge_card") && span.style.fg == Some(palette.red)
        }));
        assert!(spans.iter().any(|span| {
            span.content.contains("checkout.py") && span.style.fg == Some(palette.warn)
        }));
        assert!(
            spans.iter().any(|span| {
                span.content.contains("auth") && span.style.fg == Some(palette.green)
            })
        );
    }

    #[test]
    fn enter_drill_down_is_the_event_to_symbol_to_diff_mermaid_path() {
        let palette = ScreenTheme::Dark.palette();
        let graph = ProductionGraph {
            failing_symbol: "charge_card".to_string(),
            failing_file: "billing.py".to_string(),
            blast_radius: vec!["checkout.py".to_string()],
            drill_down: true,
            ..Default::default()
        };
        let text = lines(&graph, &palette, 0, false)
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("flowchart LR"));
        assert!(text.contains("event --> symbol --> diff"));
        assert!(text.contains("billing.py"));
        assert!(text.contains("checkout.py"));
    }
}
