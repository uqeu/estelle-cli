use estelle_client::WorkPlan;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row};
use crate::marks::StepMark;
use crate::theme::Palette;

/// The demo frame's plan table: mark, step number, step, evidence gutter.
///
/// 🔴 **THE EVIDENCE IS ON THE SAME ROW, IN A GUTTER** — variant A of the rendered specimen
/// (`docs/cli-design/specimen/04-evidence.txt`), not the indented-beneath variant B. That is the
/// harder layout and it is the one the founder picked: evidence beneath its step doubles the
/// height of every plan, and a plan you have to scroll is a plan nobody reads to the end of.
///
/// The two text columns are `45:41` on the catalog's page and that RATIO is what survives a
/// narrower surface — never the absolute widths, or the evidence column (the half that makes the
/// plan honest) is the half that falls off the right edge.
const PLAN_MARK: usize = 2;
const PLAN_NUMBER: usize = 2;
const PLAN_STEP: usize = 45;
const PLAN_EVIDENCE: usize = 41;
const PLAN_GAP: usize = 2;
const PLAN_INDENT: usize = 2;
/// `2 + 2 + 2 + 2 + 45 + 2 + 41`, plus the leading indent.
pub(crate) const PLAN_WIDTH: usize = PLAN_INDENT
    + PLAN_MARK
    + PLAN_GAP
    + PLAN_NUMBER
    + PLAN_GAP
    + PLAN_STEP
    + PLAN_GAP
    + PLAN_EVIDENCE;

fn plan_columns(width: usize) -> [Col; 4] {
    let fixed = PLAN_INDENT + PLAN_MARK + PLAN_GAP + PLAN_NUMBER + PLAN_GAP + PLAN_GAP;
    let text = width.saturating_sub(fixed).max(PLAN_MARK);
    let step = (text * PLAN_STEP / (PLAN_STEP + PLAN_EVIDENCE)).max(1);
    let evidence = text.saturating_sub(step).max(1);
    [
        Col::l(PLAN_MARK),
        Col::r(PLAN_NUMBER),
        Col::l(step),
        Col::l(evidence),
    ]
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

/// The plan sized to the surface that will show it. The live session column is narrower than the
/// catalog's page, and a plan whose evidence column falls off the right edge is a plan with no
/// evidence in it.
pub(crate) fn lines_at(plan: &WorkPlan, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let columns = plan_columns(width);
    let mut output = vec![
        crate::session_view::section_rule(
            "plan",
            &format!("revision {}", plan.revision),
            width,
            palette,
            palette.plan,
        ),
        head(
            &columns,
            &["", "", "step", "evidence"],
            palette.dim,
            PLAN_INDENT,
        ),
    ];
    for (index, step) in plan.steps.iter().enumerate() {
        let mark = StepMark::from_status(&step.status);
        let evidence = if step.evidence.trim().is_empty() {
            "— unevidenced"
        } else {
            step.evidence.as_str()
        };
        let mut row_line = owned(row(
            &columns,
            &[
                Cell(mark.glyph(), mark.colour(palette)),
                Cell(&(index + 1).to_string(), palette.dim),
                Cell(&step.step, step_colour(mark, palette)),
                Cell(evidence, palette.cite),
            ],
            PLAN_INDENT,
        ));
        // 🔴 THE ACTIVE STEP IS A FULL-WIDTH BAND, NOT A BRIGHTER GLYPH. `cols::row` pads every
        // cell to its column, so the line is exactly `width` columns and the background reaches
        // the right edge. A band that stopped at the end of the text would read as a highlight on
        // the words rather than as "you are here".
        if let Some(background) = mark.row_background(palette) {
            row_line = row_line.style(Style::default().bg(background));
        }
        output.push(row_line);
    }
    output.extend(warnings(plan, palette, width));
    output
}

/// The active step is the one the reader's eye should land on, so it gets the brightest ink; a
/// step not started yet is dim, because it has not earned attention.
fn step_colour(mark: StepMark, palette: &Palette) -> Color {
    match mark {
        StepMark::Active => palette.bright,
        StepMark::NotStarted => palette.dim,
        _ => palette.mid,
    }
}

/// The two warnings the demo frame prints under the plan, in warn, unmarked, at the step indent.
///
/// ⚠️ Both are derived from the plan the server sent — never invented. "No evidence" is literally
/// an empty `evidence` field, and "blocked" is literally the status. If neither is true of any
/// step, nothing is printed rather than a reassurance nobody measured.
fn warnings(plan: &WorkPlan, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let indent = PLAN_INDENT + PLAN_MARK + PLAN_GAP + PLAN_NUMBER + PLAN_GAP;
    let budget = width.saturating_sub(indent).max(8);
    let mut output = Vec::new();
    for (index, step) in plan.steps.iter().enumerate() {
        let number = index + 1;
        let mark = StepMark::from_status(&step.status);
        let text = if step.evidence.trim().is_empty() && mark != StepMark::NotStarted {
            format!("step {number} has no evidence — Estelle is guessing there")
        } else if mark == StepMark::Blocked {
            format!(
                "step {number} is blocked — {}",
                step.evidence.trim().to_lowercase()
            )
        } else {
            continue;
        };
        output.push(Line::styled(
            format!(
                "{}{}",
                " ".repeat(indent),
                text.chars().take(budget).collect::<String>()
            ),
            Style::default().fg(palette.warn),
        ));
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

    fn fixture() -> WorkPlan {
        serde_json::from_value(serde_json::json!({
            "revision": 3,
            "steps": [
                {"id": "1", "step": "Inspect parser", "status": "complete", "evidence": "parser.py:parse"},
                {"id": "2", "step": "Write negative control", "status": "active", "evidence": ""},
                {"id": "3", "step": "Show missing proof", "status": "pending", "evidence": ""},
                {"id": "4", "step": "Deploy", "status": "protected", "evidence": "deploy is human-gated"}
            ]
        })).expect("plan fixture")
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

    #[test]
    fn the_plan_renders_the_demo_frames_marks_numbers_and_gutter() {
        let plan = fixture();
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(lines(&plan, &ScreenTheme::Dark.palette())),
                    frame.area(),
                );
            })
            .expect("render plan");
        let frame = format!("{}", terminal.backend());

        assert!(frame.contains("── plan · revision 3 ─"), "{frame}");
        // The demo's four step marks, and NOT the rail's `● ○ ■`.
        assert!(frame.contains('✓') && frame.contains('▶') && frame.contains('□'));
        assert!(frame.contains('▲') && frame.contains("Deploy"));
        assert!(
            !frame.contains('●') && !frame.contains('○') && !frame.contains('■'),
            "{frame}"
        );
        // Evidence sits on the SAME row as its step, in the gutter.
        let row = frame
            .lines()
            .find(|line| line.contains("Inspect parser"))
            .expect("the first step's row");
        assert!(
            row.contains("parser.py:parse"),
            "evidence left the row: {row:?}"
        );
        // ⚠️ `TestBackend`'s Display wraps each row in quotes, so strip that before reading the
        // first glyph — asserting on the raw line would have passed for the wrong reason.
        assert!(
            row.trim_start()
                .trim_start_matches('"')
                .trim_start()
                .starts_with('✓'),
            "{row:?}"
        );
        assert!(frame.contains("— unevidenced"), "{frame}");
    }

    /// 🔴 THE BAND MUST REACH THE RIGHT EDGE. A background that stops at the end of the text is a
    /// highlight on the words; the demo frame lifts the whole row. Asserted on the rendered
    /// BUFFER, because a `Line` style that never made it to a cell would still look right here.
    #[test]
    fn only_the_active_step_is_lifted_and_the_band_spans_the_full_row() {
        let palette = ScreenTheme::Dark.palette();
        let plan = fixture();
        // ⚠️ Sized to the surface, exactly as the live frame sizes it — `lines()` draws at the
        // catalog's own PLAN_WIDTH, which is narrower than this terminal, and a band that stopped
        // two columns short would then be correct rather than a defect.
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new(lines_at(&plan, &palette, 100)), frame.area());
            })
            .expect("render plan");
        let buffer = terminal.backend().buffer().clone();

        let lifted = (0..buffer.area.height)
            .filter(|y| buffer[(0, *y)].bg == palette.tint)
            .collect::<Vec<_>>();
        assert_eq!(lifted.len(), 1, "exactly one step is active");
        let row = lifted[0];
        for x in 0..buffer.area.width {
            assert_eq!(
                buffer[(x, row)].bg,
                palette.tint,
                "the band stopped at column {x} of {}",
                buffer.area.width
            );
        }
        // The negative control: the row above it is NOT lifted, so the assertion above is not
        // simply reading a background painted over the whole pane.
        assert_ne!(buffer[(0, row - 1)].bg, palette.tint);
    }

    #[test]
    fn the_warnings_are_derived_from_the_plan_and_never_invented() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&lines(&fixture(), &palette));
        assert!(
            rendered.contains("step 2 has no evidence — Estelle is guessing there"),
            "{rendered}"
        );
        assert!(
            rendered.contains("step 4 is blocked — deploy is human-gated"),
            "{rendered}"
        );
        // 🔴 THE NEGATIVE CONTROL. Step 3 has no evidence either, and it must NOT warn: a step
        // nobody has started yet is not a step Estelle is guessing at. Without this, the warning
        // would fire on every fresh plan and mean nothing.
        assert!(
            !rendered.contains("step 3 has no evidence"),
            "a not-started step was accused of guessing\n{rendered}"
        );

        // A plan with evidence everywhere and nothing blocked prints NO warnings — the absence is
        // the measurement, not a reassuring line nobody checked.
        let clean: WorkPlan = serde_json::from_value(serde_json::json!({
            "revision": 1,
            "steps": [{"id": "1", "step": "Read", "status": "complete", "evidence": "a.py:1"}]
        }))
        .expect("clean plan");
        let rendered = text(&lines(&clean, &palette));
        assert!(!rendered.contains("no evidence"), "{rendered}");
        assert!(!rendered.contains("blocked"), "{rendered}");
    }

    #[test]
    fn the_evidence_gutter_survives_every_width_the_session_column_offers() {
        let palette = ScreenTheme::Dark.palette();
        for width in [46usize, 60, 80, 110, 160] {
            let rendered = lines_at(&fixture(), &palette, width);
            let row = rendered
                .iter()
                .map(|line| text(std::slice::from_ref(line)))
                .find(|line| line.contains("Inspect parser"))
                .unwrap_or_default();
            assert!(
                row.contains("parser.py") || row.contains('…'),
                "width {width} dropped the evidence entirely: {row:?}"
            );
        }
    }

    #[test]
    fn every_plan_status_glyph_is_one_terminal_column() {
        for glyph in ["✓", "▶", "▲", "□"] {
            assert_eq!(unicode_width::UnicodeWidthStr::width(glyph), 1, "{glyph:?}");
        }
    }
}
