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

/// How many lines of a tool's output an EXPANDED row draws.
///
/// 🔴 **THE FOUNDER'S RULE, IN A CONSTANT: A BASH STEP PRINTING 400 LINES SHOWS THE LAST 12 AND A
/// COUNT.** Expanding used to mean *print all 400 into the scrollback*, which is not disclosure —
/// it is the same wall of text with an extra keypress in front of it, and it pushes every earlier
/// turn off the screen. The TAIL is what a reader wants: a shell command says what happened at the
/// end.
///
/// ⚠️ **AND IT IS THE EXPANDED ROW, NOT THE COLLAPSED ONE.** The design book's own screen 39
/// footer reads *"a collapsed call is one row"*, and its expanded frame is headed
/// *"expanded, tail first"* over `212 lines hidden` and exactly twelve rows of output. A first
/// pass here put the tail in the COLLAPSED row and broke that sentence; the book is canonical.
///
/// 🔴 **NEVER A SILENT TRUNCATION, AND THE ESCAPE HATCH IS NAMED.** The count of what is not drawn
/// is its own row, and it names the key that hands the reader ALL of it — `ctrl+y`, which copies
/// the whole output rather than what is on screen. An output that fits hides nothing and prints no
/// such row: absent and zero are different bytes here too.
const TOOL_TAIL_LINES: usize = 12;

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
                    lines = band_the_message_only(lines, width, background);
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
                mut lines,
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
                    let total = lines.len();
                    let hidden = total.saturating_sub(TOOL_TAIL_LINES);
                    if hidden > 0 {
                        tool_lines.push(Line::from(vec![
                            ratatui::text::Span::from(TOOL_CONTINUATION),
                            format!(
                                "{hidden} line{} hidden \u{b7} ctrl+y copies all {total}",
                                if hidden == 1 { "" } else { "s" }
                            )
                            .dark_gray(),
                        ]));
                    }
                    let shown = lines.split_off(hidden);
                    tool_lines.extend(shown.into_iter().enumerate().map(|(index, line)| {
                        // The `⎿` elbow opens the BODY. When a hidden-count row is present it has
                        // already taken the elbow, so the first visible line indents like the
                        // rest — two elbows under one call would read as two calls.
                        let lead = if index == 0 && hidden == 0 {
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
/// Lift ONLY the rows the message occupies, and strip the inherited fill from the rows it does not.
///
/// 🔴 **ONE SELECTION BAND, PAINTED TWICE.** [`UserHistoryCell::display_lines`] opens and closes
/// every user turn with `Line::from("").style(user_message_style())` — a blank row carrying
/// Codex's own fill. Upstream that fill is a BLEND against the terminal's reported background, so
/// the two blanks read as padding nobody sees. Estelle re-bands the same lines with
/// [`crate::theme::Palette::tint`], a colour `theme.rs` asserts is at least 30 points off the
/// ground — so the padding stopped being padding and became a lit strip ABOVE and BELOW the
/// message. The founder photographed exactly that on session-home: one message, three bands.
///
/// ⚠️ **THE FIRST AND LAST NON-BLANK ROW ARE THE BOUND, NOT "EVERY NON-BLANK ROW".** A message may
/// contain its own blank line between two paragraphs, and unlifting that row would split one band
/// into two — the same defect inverted. Everything between the outermost content rows stays lit.
///
/// ⚠️ **AND THE ROWS OUTSIDE IT MUST BE CLEARED, NOT MERELY SKIPPED.** Skipping leaves Codex's fill
/// in place, which is invisible in a test (`default_bg()` is `None` there) and visible on the
/// founder's terminal, which answers the OSC query. Two owners of one band is what this is.
fn band_the_message_only(
    lines: Vec<Line<'static>>,
    width: u16,
    background: Color,
) -> Vec<Line<'static>> {
    let first = lines.iter().position(|line| !is_blank(line));
    let last = lines.iter().rposition(|line| !is_blank(line));
    let (Some(first), Some(last)) = (first, last) else {
        return lines.into_iter().map(clear_background).collect();
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if index >= first && index <= last {
                band(line, width).bg(background)
            } else {
                clear_background(line)
            }
        })
        .collect()
}

/// A row with nothing but whitespace on it — the padding the cell frames its message with.
fn is_blank(line: &Line<'static>) -> bool {
    line.spans.iter().all(|span| span.content.trim().is_empty())
}

/// Drop every background this row carries, on the line AND on each span.
///
/// A `Line` style and a `Span` style are two different places a fill can hide, and clearing one
/// leaves the other painting.
fn clear_background(mut line: Line<'static>) -> Line<'static> {
    line.style.bg = None;
    for span in &mut line.spans {
        span.style.bg = None;
    }
    line
}

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

    use ratatui::style::Style;
    use ratatui::text::Span;

    /// A dark swatch standing in for `palette.tint` — the fill Estelle lifts the message onto.
    #[allow(clippy::disallowed_methods)]
    const LIFT: Color = Color::Rgb(0x24, 0x1f, 0x19);

    /// Codex's own fill on the cell's padding rows: present on a terminal that answers the OSC
    /// background query, absent in a test, which is exactly why skipping those rows is not enough.
    #[allow(clippy::disallowed_methods)]
    const INHERITED: Color = Color::Rgb(0x2a, 0x2a, 0x2a);

    fn line_backgrounds(lines: &[Line<'static>]) -> Vec<Option<Color>> {
        lines
            .iter()
            .map(|line| {
                line.style
                    .bg
                    .or_else(|| line.spans.iter().find_map(|span| span.style.bg))
            })
            .collect()
    }

    /// 🔴 ONE BAND, AND IT ENDS WHERE THE MESSAGE ENDS.
    ///
    /// The cell frames a user turn with a blank row above and below, both carrying Codex's fill.
    /// Banding all three rows put a lit strip above AND below the line the founder had typed.
    #[test]
    fn the_band_covers_the_message_and_neither_blank_around_it() {
        let framed = vec![
            Line::from("").style(Style::default().bg(INHERITED)),
            Line::from("› what changed while I was away?").style(Style::default().bg(INHERITED)),
            Line::from("").style(Style::default().bg(INHERITED)),
        ];

        let banded = band_the_message_only(framed, 60, LIFT);

        assert_eq!(
            line_backgrounds(&banded),
            vec![None, Some(LIFT), None],
            "the band must cover the message row and neither blank around it"
        );
    }

    /// The negative control the assertion above needs: a message with a paragraph break inside it
    /// is still ONE band. Unlifting every blank row would split it, which is the same defect with
    /// the sign flipped.
    #[test]
    fn an_internal_blank_line_stays_inside_the_one_band() {
        let framed = vec![
            Line::from("").style(Style::default().bg(INHERITED)),
            Line::from("› first paragraph"),
            Line::from(""),
            Line::from("  second paragraph"),
            Line::from("").style(Style::default().bg(INHERITED)),
        ];

        let banded = band_the_message_only(framed, 60, LIFT);

        assert_eq!(
            line_backgrounds(&banded),
            vec![None, Some(LIFT), Some(LIFT), Some(LIFT), None],
            "the rows between the outermost content rows stay lit"
        );
    }

    /// The inherited fill is CLEARED, not merely skipped — on a span as well as on the line.
    /// A span-level fill survives a line-level clear and paints on the founder's terminal while
    /// staying invisible in a test, which is how two owners of one band went unnoticed.
    #[test]
    fn the_padding_rows_lose_the_fill_they_inherited_on_spans_too() {
        let framed = vec![
            Line::from(Span::styled("   ", Style::default().bg(INHERITED))),
            Line::from("› one line"),
            Line::from(Span::styled("   ", Style::default().bg(INHERITED))),
        ];

        let banded = band_the_message_only(framed, 60, LIFT);

        for (index, line) in banded.iter().enumerate() {
            if index == 1 {
                continue;
            }
            assert!(
                line.spans.iter().all(|span| span.style.bg.is_none()),
                "row {index} still carries a span fill: {line:?}"
            );
        }
    }

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

    /// 🔴 **A COLLAPSED CALL IS ONE ROW; AN EXPANDED ONE IS THE TAIL PLUS A COUNT.**
    ///
    /// Expanding used to print all 400 lines into the scrollback, which is the same wall of text
    /// with a keypress in front of it. It draws the last `TOOL_TAIL_LINES` now, over a row saying
    /// how many it did not draw and which key hands the reader all of them.
    ///
    /// ⚠️ **THE FIXTURE IS SIZED FROM THE CONSTANT, NOT FROM 12.** A test that wrote `12` would go
    /// red for the wrong reason the day the constant moved, and green for the wrong reason if
    /// someone changed the constant to match a broken render.
    #[test]
    fn an_expanded_tool_call_shows_its_tail_and_counts_what_it_hid() {
        let body = |count: usize| {
            (0..count)
                .map(|index| Line::from(format!("line {index:03}")))
                .collect::<Vec<_>>()
        };
        let render = |lines: Vec<Line<'static>>, expanded: bool| {
            render_interactive_history_transcript(
                vec![HistoryTranscriptItem::Tool {
                    id: 7,
                    label: "!cargo test".to_string(),
                    lines,
                    expanded,
                    semantic_color: Color::Blue,
                }],
                80,
                Path::new("."),
            )
        };

        let long = TOOL_TAIL_LINES + 6;

        // COLLAPSED IS STILL ONE ROW. The book's own screen-39 footer says so, and a first pass
        // here put the tail in this state and broke that sentence.
        let collapsed = render(body(long), false);
        let collapsed_debug = format!("{:?}", collapsed.text);
        assert!(collapsed_debug.contains("⏺ "), "{collapsed_debug}");
        assert!(!collapsed_debug.contains("⎿"), "{collapsed_debug}");
        assert!(
            !collapsed_debug.contains("line 0"),
            "a collapsed call drew output:\n{collapsed_debug}"
        );
        assert_eq!(
            collapsed.interactive_rows,
            vec![InteractiveHistoryRow { id: 7, line: 0 }]
        );

        let expanded = render(body(long), true);
        let expanded_debug = format!("{:?}", expanded.text);
        assert!(expanded_debug.contains("⎿"), "{expanded_debug}");
        assert!(
            expanded_debug.contains(&format!("6 lines hidden \u{b7} ctrl+y copies all {long}")),
            "what is hidden must be counted, and the key that hands it over must be named:\n{expanded_debug}"
        );
        assert!(
            !expanded_debug.contains("line 000"),
            "a hidden line was drawn, so the count is wrong:\n{expanded_debug}"
        );
        assert!(
            expanded_debug.contains(&format!("line {:03}", long - 1)),
            "the LAST line is the one an expanded row exists to show:\n{expanded_debug}"
        );

        // An output that fits hides nothing, so it counts nothing. Absent and zero stay apart.
        let short = render(body(2), true);
        let short_debug = format!("{:?}", short.text);
        assert!(short_debug.contains("line 000"), "{short_debug}");
        assert!(
            !short_debug.contains("hidden"),
            "an output that fits must not claim it hid rows:\n{short_debug}"
        );
    }
}
