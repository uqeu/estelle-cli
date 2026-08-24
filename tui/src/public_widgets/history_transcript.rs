//! Public, application-owned transcript adapter for the internal history-cell renderer.

use std::path::Path;

use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Text;

use crate::history_cell::AgentMarkdownCell;
use crate::history_cell::CompositeHistoryCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::PlainHistoryCell;
use crate::history_cell::new_user_prompt;

/// One committed transcript item rendered by the maintained history-cell machinery.
pub enum HistoryTranscriptItem {
    /// A user turn with the native filled, wrapped user-message treatment.
    User {
        heading: Vec<Line<'static>>,
        message: String,
        background: Option<Color>,
    },
    /// Source-backed markdown with application-owned heading and trailing evidence.
    Markdown {
        heading: Vec<Line<'static>>,
        source: String,
        trailing: Vec<Line<'static>>,
    },
    /// Already structured application lines that still participate as one history cell.
    Lines(Vec<Line<'static>>),
}

/// Render committed items through `HistoryCell::display_lines`, preserving width-aware reflow.
pub fn render_history_transcript(
    items: Vec<HistoryTranscriptItem>,
    width: u16,
    cwd: &Path,
) -> Text<'static> {
    let mut rendered = Vec::new();
    for item in items {
        let cell: Box<dyn HistoryCell> = match item {
            HistoryTranscriptItem::User {
                heading,
                message,
                background,
            } => {
                if !heading.is_empty() {
                    rendered.extend(PlainHistoryCell::new(heading).display_lines(width));
                    rendered.push(Line::default());
                }
                let user = new_user_prompt(message, Vec::new(), Vec::new(), Vec::new());
                let mut lines = user.display_lines(width);
                if let Some(background) = background {
                    lines = lines.into_iter().map(|line| line.bg(background)).collect();
                }
                rendered.extend(lines);
                rendered.push(Line::default());
                continue;
            }
            HistoryTranscriptItem::Markdown {
                heading,
                source,
                trailing,
            } => {
                let mut parts: Vec<Box<dyn HistoryCell>> = Vec::new();
                if !heading.is_empty() {
                    parts.push(Box::new(PlainHistoryCell::new(heading)));
                }
                parts.push(Box::new(AgentMarkdownCell::new_with_inline_visualizations(
                    source, cwd, /*inline_visualization_context*/ None,
                )));
                if !trailing.is_empty() {
                    parts.push(Box::new(PlainHistoryCell::new(trailing)));
                }
                Box::new(CompositeHistoryCell::new(parts))
            }
            HistoryTranscriptItem::Lines(lines) => Box::new(PlainHistoryCell::new(lines)),
        };
        rendered.extend(cell.display_lines(width));
        rendered.push(Line::default());
    }
    Text::from(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_cell_wraps_and_markdown_cell_renders_structure() {
        let text = render_history_transcript(
            vec![
                HistoryTranscriptItem::User {
                    heading: vec![Line::from("you")],
                    message: "a user message that must wrap".to_string(),
                    background: Some(Color::Rgb(24, 24, 24)),
                },
                HistoryTranscriptItem::Markdown {
                    heading: vec![Line::from("estelle  grounded")],
                    source: "**answer**\n\n- cited fact".to_string(),
                    trailing: vec![Line::from("cited  src/lib.rs:4")],
                },
            ],
            18,
            Path::new("."),
        );
        let rendered = format!("{text:?}");
        assert!(rendered.contains("a user message"));
        assert!(rendered.contains("answer"));
        assert!(rendered.contains("cited  src/lib.rs:4"));
    }
}
