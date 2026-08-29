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
        semantic_color: Option<Color>,
    },
    /// Source-backed markdown with application-owned heading and trailing evidence.
    Markdown {
        heading: Vec<Line<'static>>,
        source: String,
        trailing: Vec<Line<'static>>,
        /// Theme-owned colour for code, file paths, symbols and links.
        semantic_color: Option<Color>,
    },
    /// Already structured application lines that still participate as one history cell.
    Lines(Vec<Line<'static>>),
    /// One-line tool receipt whose output is revealed only after explicit expansion.
    Tool {
        id: usize,
        label: String,
        lines: Vec<Line<'static>>,
        expanded: bool,
        semantic_color: Color,
    },
}

/// A committed transcript row that the embedding may map back to a mouse target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractiveHistoryRow {
    pub id: usize,
    pub line: usize,
}

/// Rendered transcript plus the exact visible-line origins of interactive rows.
pub struct RenderedHistoryTranscript {
    pub text: Text<'static>,
    pub interactive_rows: Vec<InteractiveHistoryRow>,
}

/// Render committed items through `HistoryCell::display_lines`, preserving width-aware reflow.
pub fn render_history_transcript(
    items: Vec<HistoryTranscriptItem>,
    width: u16,
    cwd: &Path,
) -> Text<'static> {
    render_interactive_history_transcript(items, width, cwd).text
}

/// Render committed items and retain the line origin of every collapsible tool receipt.
pub fn render_interactive_history_transcript(
    items: Vec<HistoryTranscriptItem>,
    width: u16,
    cwd: &Path,
) -> RenderedHistoryTranscript {
    let mut rendered = Vec::new();
    let mut interactive_rows = Vec::new();
    for item in items {
        let semantic_color = match &item {
            HistoryTranscriptItem::Markdown { semantic_color, .. } => *semantic_color,
            HistoryTranscriptItem::User {
                message,
                semantic_color,
                ..
            } if message.trim_start().starts_with(['/', '!']) => *semantic_color,
            _ => None,
        };
        let cell: Box<dyn HistoryCell> = match item {
            HistoryTranscriptItem::User {
                heading,
                message,
                background,
                ..
            } => {
                if !heading.is_empty() {
                    rendered.extend(PlainHistoryCell::new(heading).display_lines(width));
                    rendered.push(Line::default());
                }
                let user = new_user_prompt(message, Vec::new(), Vec::new(), Vec::new());
                let mut lines = user.display_lines(width);
                if let Some(semantic_color) = semantic_color {
                    for line in &mut lines {
                        for span in &mut line.spans {
                            span.style.fg = Some(semantic_color);
                        }
                    }
                }
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
                ..
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
            HistoryTranscriptItem::Tool {
                id,
                label,
                lines,
                expanded,
                semantic_color,
            } => {
                interactive_rows.push(InteractiveHistoryRow {
                    id,
                    line: rendered.len(),
                });
                let mut tool_lines = vec![Line::from(vec![
                    if expanded { "▾ " } else { "▸ " }.into(),
                    label.fg(semantic_color),
                    format!(
                        "  ·  {} line{}",
                        lines.len(),
                        if lines.len() == 1 { "" } else { "s" }
                    )
                    .dark_gray(),
                ])];
                if expanded {
                    tool_lines.extend(lines);
                }
                Box::new(PlainHistoryCell::new(tool_lines))
            }
        };
        let mut lines = cell.display_lines(width);
        if let Some(semantic_color) = semantic_color {
            for line in &mut lines {
                for span in &mut line.spans {
                    if matches!(span.style.fg, Some(Color::Cyan | Color::LightBlue)) {
                        span.style.fg = Some(semantic_color);
                    }
                }
            }
        }
        rendered.extend(lines.into_iter().map(coalesce_line_spans));
        rendered.push(Line::default());
    }
    RenderedHistoryTranscript {
        text: Text::from(rendered),
        interactive_rows,
    }
}

/// Markdown's wrapping pass intentionally emits one span per word. Merge adjacent spans with the
/// same style after semantic recolouring so plain prose remains one selectable terminal run while
/// links, commands and symbols retain their distinct colour.
fn coalesce_line_spans(line: Line<'static>) -> Line<'static> {
    let mut spans: Vec<ratatui::text::Span<'static>> = Vec::with_capacity(line.spans.len());
    for span in line.spans {
        if let Some(previous) = spans.last_mut()
            && previous.style == span.style
        {
            previous.content.to_mut().push_str(span.content.as_ref());
        } else {
            spans.push(span);
        }
    }
    Line { spans, ..line }
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
                    // A fixed dark swatch for a transcript rendered over a hardcoded ground; the
                    // surrounding widget does not consult the terminal theme.
                    #[allow(clippy::disallowed_methods)]
                    background: Some(Color::Rgb(24, 24, 24)),
                    semantic_color: Some(Color::Blue),
                },
                HistoryTranscriptItem::Markdown {
                    heading: vec![Line::from("estelle  grounded")],
                    source: "**answer**\n\n- cited fact".to_string(),
                    trailing: vec![Line::from("cited  src/lib.rs:4")],
                    semantic_color: Some(Color::Blue),
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

    #[test]
    fn tool_output_is_one_interactive_line_until_expanded() {
        let collapsed = render_interactive_history_transcript(
            vec![HistoryTranscriptItem::Tool {
                id: 7,
                label: "!cargo test".to_string(),
                lines: vec![Line::from("first"), Line::from("second")],
                expanded: false,
                semantic_color: Color::Blue,
            }],
            80,
            Path::new("."),
        );
        let collapsed_debug = format!("{:?}", collapsed.text);
        assert!(collapsed_debug.contains("▸ "));
        assert!(!collapsed_debug.contains("first"));
        assert_eq!(
            collapsed.interactive_rows,
            vec![InteractiveHistoryRow { id: 7, line: 0 }]
        );

        let expanded = render_interactive_history_transcript(
            vec![HistoryTranscriptItem::Tool {
                id: 7,
                label: "!cargo test".to_string(),
                lines: vec![Line::from("first"), Line::from("second")],
                expanded: true,
                semantic_color: Color::Blue,
            }],
            80,
            Path::new("."),
        );
        let expanded_debug = format!("{:?}", expanded.text);
        assert!(expanded_debug.contains("▾ "));
        assert!(expanded_debug.contains("first"));
    }
}
