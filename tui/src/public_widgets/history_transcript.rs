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

/// The design's tool-call glyphs, from `screens.rs`'s `tools` and `everything` screens.
/// ⚠️ These are the transcript's identity: `⏺` opens a call, `⎿` continues it. They are not
/// a disclosure control — expansion state is still carried by the click target, not the glyph.
const TOOL_MARKER: &str = "⏺ ";
const TOOL_CONTINUATION: &str = "  ⎿  ";
const TOOL_INDENT: &str = "     ";

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
        /// The glyph and colour that OPEN this message, replacing the generic `• ` bullet
        /// `AgentMarkdownCell` prefixes every assistant message with.
        ///
        /// 🔴 **THIS REPLACES A PREFIX RATHER THAN ADDING ONE.** The markdown cell already emits
        /// `"• "` on the first line and `"  "` on every continuation, so the indent is already
        /// correct and a mark prepended on top of it would read `● • text`. Swapping the glyph in
        /// place keeps one marker per message and costs no layout.
        mark: Option<(String, Color)>,
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
        let mark = match &item {
            HistoryTranscriptItem::Markdown { mark, .. } => mark.clone(),
            _ => None,
        };
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
                    lines = lines
                        .into_iter()
                        .map(|line| band(line, width).bg(background))
                        .collect();
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
                // The design writes a tool call as `⏺ Task(...)` with an `⎿` continuation —
                // the glyphs the founder's demo shows and the ones `screens.rs` has always
                // used. The disclosure triangle was the boxed language.
                let mut tool_lines = vec![Line::from(vec![
                    TOOL_MARKER.into(),
                    label.fg(semantic_color),
                    format!(
                        "  ·  {} line{}",
                        lines.len(),
                        if lines.len() == 1 { "" } else { "s" }
                    )
                    .dark_gray(),
                ])];
                if expanded {
                    tool_lines.extend(lines.into_iter().enumerate().map(|(index, line)| {
                        let lead = if index == 0 {
                            TOOL_CONTINUATION
                        } else {
                            TOOL_INDENT
                        };
                        let mut spans = vec![ratatui::text::Span::from(lead)];
                        spans.extend(line.spans);
                        Line::from(spans)
                    }));
                }
                Box::new(PlainHistoryCell::new(tool_lines))
            }
        };
        let mut lines = cell.display_lines(width);
        if let Some((glyph, colour)) = mark {
            open_with_mark(&mut lines, &glyph, colour);
        }
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

/// Swap the markdown cell's generic `• ` bullet on the FIRST line for a meaningful mark.
///
/// The founder's words: *"Claude does not say Claude, Claude just writes a dot."* The dot was
/// already there — dim, generic, and sitting under a line that said `estelle  conversation`. What
/// changed is that the dot now MEANS something (`●` grounded, `○` from the model, `▲` degraded)
/// and the line above it is gone.
///
/// ⚠️ Falls back to INSERTING when the first span is not the bullet it expects. A future markdown
/// change that drops the prefix would otherwise silently lose the mark, and a missing grounding
/// signal is the one failure this whole surface exists to prevent.
fn open_with_mark(lines: &mut [Line<'static>], glyph: &str, colour: Color) {
    let Some(first) = lines.first_mut() else {
        return;
    };
    let marker = ratatui::text::Span::styled(
        format!("{glyph} "),
        ratatui::style::Style::default().fg(colour),
    );
    match first.spans.first() {
        Some(span) if span.content.trim() == "\u{2022}" => first.spans[0] = marker,
        _ => first.spans.insert(0, marker),
    }
}

/// Pad a line out to the full render width so a background applied to it reads as a BAND.
///
/// 🔴 **`Line::bg` COLOURS THE TEXT, NOT THE ROW.** A style on a `Line` reaches only the cells its
/// spans actually write, so the user's turn was lifted for exactly as many columns as the sentence
/// happened to be long and then fell back to the ground — measured at column 71 of 80. What makes
/// a long conversation scannable is the row reaching the right edge, so the eye can find its own
/// questions down the gutter; a ragged right edge is a highlighter on the words instead.
///
/// The padding is added BEFORE the background so the trailing run carries it too. `Line::width`
/// is the display width (wide glyphs count two), not the char count, which is why a CJK question
/// still lands flush instead of overshooting. A line already at or past `width` is returned
/// untouched — never truncated, because clipping the user's own words to paint a band would be
/// trading the content for the decoration.
fn band(line: Line<'static>, width: u16) -> Line<'static> {
    let padding = usize::from(width).saturating_sub(line.width());
    if padding == 0 {
        return line;
    }
    let mut spans = line.spans;
    // ⚠️ **THE PAD IS BORROWED, NOT ALLOCATED, AND THAT IS A MEASURED DECISION.** `" ".repeat(n)`
    // per user line cost ~50ms over a 6669-entry scrollback — the whole of the frame budget that
    // `a_long_transcript_must_not_make_a_frame_exceed_its_budget` allows, spent on spaces.
    // Slicing a static keeps the common case free; the allocation survives only for a terminal
    // wider than the constant, where one allocation per line is not the dominant cost anyway.
    spans.push(match PAD.get(..padding) {
        Some(pad) => ratatui::text::Span::raw(pad),
        None => ratatui::text::Span::raw(" ".repeat(padding)),
    });
    Line { spans, ..line }
}

/// Spaces for [`band`] to slice. Sized past any terminal width worth optimising for; a wider one
/// falls back to allocating, which is correct and merely slower.
const PAD: &str = "                                                                                                                                                                                                                                                                ";

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
                    heading: Vec::new(),
                    source: "**answer**\n\n- cited fact".to_string(),
                    trailing: vec![Line::from("cited  src/lib.rs:4")],
                    semantic_color: Some(Color::Blue),
                    mark: None,
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
        // ⚠️ THE GLYPH NO LONGER CARRIES THE DISCLOSURE STATE, AND THAT IS A DELIBERATE LOSS.
        // The design opens every tool call with `⏺` in both states; what distinguishes them is
        // the `⎿` BODY, which is present only when expanded. The `▸`/`▾` triangle belonged to
        // the boxed language this renderer no longer speaks.
        assert!(collapsed_debug.contains("⏺ "), "{collapsed_debug}");
        assert!(!collapsed_debug.contains("⎿"), "{collapsed_debug}");
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
        assert!(expanded_debug.contains("⏺ "), "{expanded_debug}");
        assert!(expanded_debug.contains("⎿"), "{expanded_debug}");
        assert!(expanded_debug.contains("first"));
    }
}
