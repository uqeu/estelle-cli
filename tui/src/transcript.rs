//! Estelle-owned transcript semantics over the adopted history-cell renderer.
//!
//! The history library owns wrapping, markdown and cell layout. This module owns only facts the
//! generic component cannot infer: speakers, grounding state, secret masking, semantic colour and
//! which local tool receipts are collapsed.

use std::path::Path;

use crate::marks;
use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use estelle_client::Client;
use estelle_client::CommandReply;
use estelle_client::Error;
use estelle_client::Source;
use estelle_client::mask_secret;
use estelle_tui::HistoryTranscriptItem;
use estelle_tui::InteractiveHistoryRow;
use estelle_tui::RenderedHistoryTranscript;
use estelle_tui::render_interactive_history_transcript;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;

pub(crate) enum TranscriptEntry {
    SessionHandoff(Vec<String>),
    User(String),
    Answer {
        text: String,
        grounded: Option<bool>,
        degraded: bool,
        sources: Vec<Source>,
    },
    /// The server's refusal to certify the answer that follows, in the server's own words.
    ///
    /// 🔴 **ITS OWN ENTRY, NOT A FIELD ON [`TranscriptEntry::Answer`].** The disclosure has to
    /// lead — it is what tells the reader the citations below it may point into code that has
    /// moved — and an entry can be pushed ahead of the answer without touching the twelve places
    /// that build one. ⚠️ It is emitted ONLY when the server sends the block, which it does only
    /// when the index is behind AND the answer leans on the code; a current index produces no
    /// entry at all, so this can never read as "no data".
    Stale(estelle_client::CodeCurrency),
    System(String),
    Command {
        name: String,
        lines: Vec<String>,
    },
    /// A block its OWN module already rendered, spans and styles intact.
    ///
    /// 🔴 **EVERY OTHER VARIANT CARRIES `String`, AND THAT IS WHY THE FILM LOST ITS COLOUR.**
    /// [`TranscriptEntry::Command`] re-styles each of its lines through [`semantic_line`], so a
    /// caller that had ALREADY computed the right colours had no way in except to flatten to
    /// `String` first. `demo_session` did exactly that: it called the real
    /// [`crate::gate_refusal::lines`], collected `span.content` and dropped every `Style` — so
    /// `Gate refused` was computed in `#c52416` BOLD and drawn as body text. The founder's words
    /// were *"you got rid of all the colours … you neutered Estelle in the demo."* He was reading
    /// the flattening, one layer below where the colour was correct.
    ///
    /// ⚠️ **THE REDACTOR STILL RUNS, PER SPAN.** A styled block is not a second door around the
    /// fence: every span's content goes through `mask_secret` exactly as `Command`'s lines do, and
    /// `a_painted_block_is_still_masked` proves a credential cannot ride in on a `Style`.
    ///
    /// ⚠️ **`name` IS OPTIONAL BECAUSE SOME BLOCKS OPEN THEMSELVES.** `gate_refusal` opens on its
    /// own `── gate · refused ──` rule and `orchestra_view` on its own `● Task(…)`; printing a
    /// `● /gate` receipt above either one draws two marks for one event.
    Painted {
        name: Option<String>,
        lines: Vec<Line<'static>>,
    },
    Tool {
        label: String,
        lines: Vec<String>,
        expanded: bool,
    },
    Failure([String; 3]),
}

/// 🔴 **THIS STRUCT IS THREE ROLES SHORT OF THE FRAME IT PAINTS, AND THE GAP IS WHERE THE BRAND
/// LEAKED OUT.** Every colour the transcript needs that is NOT here fell back to a named ANSI
/// variant — `Color::Yellow` for the degraded/not-grounded badge, `Color::Red` for the failure
/// banner, `Color::DarkGray` for the citation label — which renders as whatever the *host
/// terminal* thinks yellow is, not Estelle's `#c9a227`. The clippy ban that stopped `Color::Rgb`
/// could not see those (`tui/src/style.rs`, `brand_palette_guard`), so nothing complained
/// for months.
///
/// The four secondary-text sites were fixable in place because [`Self::ghost`] already exists and
/// [`TranscriptEntry::System`] already used it for the same slot. The remaining three need roles
/// this struct does not carry:
///
/// | site | today | wants | `theme::Palette` role |
/// |---|---|---|---|
/// | degraded / not-grounded badge | `Color::Yellow` | caution | `warn` |
/// | `cited` label | `Color::DarkGray` | citation | `cite` |
/// | failure banner | `Color::Red` | failure | `red` |
///
/// ⚠️ **The fix is three fields here and ONE line at the only construction site**, which is
/// `live_renderer::render_transcript_with_citations` — it already holds a `Theme`, and
/// `Theme::screen_palette()` already returns the 13-role [`crate::theme::Palette`]. That file was
/// owned by another lane while this was written, so the change is named rather than made; making
/// it here would have meant editing a file someone else had open.
#[derive(Clone, Copy)]
pub(crate) struct TranscriptPalette {
    pub(crate) primary: Color,
    pub(crate) ghost: Color,
    pub(crate) semantic: Color,
    pub(crate) user_background: Option<Color>,
    /// ✅ THE THREE ROLES THE DOCSTRING ABOVE NAMED AS MISSING, PLUS THE TWO THE REPLY MARK NEEDS.
    /// Every one of these previously fell back to a named ANSI variant, which renders as whatever
    /// the HOST TERMINAL thinks that colour is rather than Estelle's. They come from
    /// `theme::Palette` by MEANING now, wired at the single construction site.
    pub(crate) warn: Color,
    pub(crate) cite: Color,
    pub(crate) failure: Color,
    pub(crate) grounded: Color,
    pub(crate) ungrounded: Color,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolClickTarget {
    pub(crate) entry: usize,
    pub(crate) area: Rect,
}

pub(crate) fn visible_tool_targets(
    rows: Vec<InteractiveHistoryRow>,
    root: Rect,
    scroll: u16,
) -> Vec<ToolClickTarget> {
    rows.into_iter()
        .filter_map(|row| {
            let visible = row.line.checked_sub(usize::from(scroll))?;
            (visible < usize::from(root.height)).then(|| ToolClickTarget {
                entry: row.id,
                area: Rect::new(
                    root.x,
                    root.y + u16::try_from(visible).unwrap_or(u16::MAX),
                    root.width,
                    1,
                ),
            })
        })
        .collect()
}

pub(crate) fn toggle_tool_at(
    entries: &mut [TranscriptEntry],
    targets: &[ToolClickTarget],
    column: u16,
    row: u16,
) {
    let target = targets.iter().find(|target| {
        column >= target.area.x
            && column < target.area.x.saturating_add(target.area.width)
            && row >= target.area.y
            && row < target.area.y.saturating_add(target.area.height)
    });
    if let Some(target) = target
        && let Some(TranscriptEntry::Tool { expanded, .. }) = entries.get_mut(target.entry)
    {
        *expanded = !*expanded;
    }
}

pub(crate) fn handle_mouse(
    entries: &mut [TranscriptEntry],
    targets: &[ToolClickTarget],
    scroll: &mut usize,
    mouse: MouseEvent,
) {
    const WHEEL_LINES: usize = 3;
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            toggle_tool_at(entries, targets, mouse.column, mouse.row)
        }
        MouseEventKind::ScrollUp => *scroll = scroll.saturating_add(WHEEL_LINES),
        MouseEventKind::ScrollDown => *scroll = scroll.saturating_sub(WHEEL_LINES),
        _ => {}
    }
}

/// The staleness verdict, drawn the way screen 10 of the design book draws it.
///
/// 🔴 **THE SERVER'S SENTENCE, NOT A SECOND ONE.** `detail` comes from `GraphHealth.describe()`,
/// which is also what the navigation tools refuse with — so the CLI cannot date a repo differently
/// from the tool that declined a lookup one second earlier. The two SHAs are pulled out onto their
/// own line because that is the fact a reader acts on, and they are shortened by the client's
/// `CodeCurrency::short`, which returns the WHOLE head rather than a clipped one when it is short.
///
/// ⚠️ **`detail` MAY BE EMPTY AND THAT IS A REAL STATE**, not a bug: `_describe` falls back to an
/// empty string when the health record cannot render itself. An empty sentence draws no row rather
/// than a row saying nothing.
fn stale_lines(
    currency: &estelle_client::CodeCurrency,
    palette: TranscriptPalette,
) -> Vec<Line<'static>> {
    use estelle_client::CodeCurrency;

    let headline = if currency.is_stale() {
        "Index is behind your tree"
    } else {
        "Index currency is unknown"
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", marks::Mark::Blocked.glyph()),
            Style::default().fg(palette.warn),
        ),
        Span::styled(
            headline.to_string(),
            Style::default()
                .fg(palette.warn)
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    if !currency.indexed_head.is_empty() && !currency.current_head.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                currency.status.to_uppercase(),
                Style::default().fg(palette.warn),
            ),
            Span::styled(
                " — indexed at ".to_string(),
                Style::default().fg(palette.ghost),
            ),
            Span::styled(
                CodeCurrency::short(&currency.indexed_head).to_string(),
                Style::default().fg(palette.warn),
            ),
            Span::styled(
                ", repo is now ".to_string(),
                Style::default().fg(palette.ghost),
            ),
            Span::styled(
                CodeCurrency::short(&currency.current_head).to_string(),
                Style::default().fg(palette.cite),
            ),
        ]));
    }
    if !currency.detail.trim().is_empty() {
        lines.push(semantic_line(
            &mask_secret(currency.detail.trim()),
            palette.semantic,
            Some(palette.ghost),
        ));
    }
    lines
}

/// `● /model` rather than `estelle  /model`. The command's OWN name is the informative half and it
/// stays; the program's name was the noise.
///
/// ⚠️ **ONE OWNER.** [`TranscriptEntry::Command`] and [`TranscriptEntry::Painted`] both open on
/// this row, and when it was written out twice the two drifted on the mark's colour.
fn command_header(name: &str, palette: TranscriptPalette) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{} ", marks::Mark::Landed.glyph()),
            Style::default().fg(palette.grounded),
        ),
        Span::styled(
            format!("/{}", mask_secret(name)),
            Style::default().fg(palette.semantic),
        ),
    ])
}

/// One already-styled line, with the redactor run over every span and every style kept.
///
/// 🔴 **PER SPAN, NOT PER LINE.** `mask_secret` replaces a whole line that merely CONTAINS a
/// credential shape, and a `Line` here is a sequence of independently coloured runs — masking the
/// concatenation would have collapsed the row to one span and taken the colour with it, which is
/// the very defect [`TranscriptEntry::Painted`] exists to end. Masking each span keeps the
/// boundaries and still refuses the credential.
fn masked(line: &Line<'static>) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(mask_secret(span.content.as_ref()), span.style))
            .collect::<Vec<_>>(),
    )
    .style(line.style)
}

pub(crate) fn render(
    entries: &[TranscriptEntry],
    include_citations: bool,
    palette: TranscriptPalette,
    width: u16,
    cwd: &Path,
) -> RenderedHistoryTranscript {
    let mut items = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            TranscriptEntry::SessionHandoff(lines) => {
                let mut rendered = vec![Line::styled(
                    "Since the last session",
                    Style::default()
                        .fg(palette.primary)
                        .add_modifier(Modifier::BOLD),
                )];
                rendered.extend(lines.iter().map(|line| {
                    semantic_line(&mask_secret(line), palette.semantic, Some(palette.ghost))
                }));
                items.push(HistoryTranscriptItem::Lines(rendered));
            }
            // 🔴 **THE WORD `you` IS GONE, AND THE BAND IS WHAT SAYS IT NOW.**
            //
            // The founder, reading the waiting screen: *"Delete the word 'you'. I don't want to
            // see that 'you' any more."* — and on the next screen: *"When a message arrives it
            // should be visually highlighted the way ChatGPT and Codex highlight yours. Same
            // treatment, our palette."* Those are one instruction, not two: the label was standing
            // in for a highlight that was not reliably drawn.
            //
            // ⚠️ Deleting the label alone would have been a REGRESSION, because
            // `user_turn_background` returned `None` on any terminal that does not answer an OSC
            // background query — so on those terminals the turn would have lost its only marker
            // and become indistinguishable from Estelle's own output. The label could only go once
            // the band was made unconditional; see `user_turn_background` in `main.rs` for the
            // fallback that made this safe.
            TranscriptEntry::User(message) => items.push(HistoryTranscriptItem::User {
                heading: Vec::new(),
                message: mask_secret(message),
                background: palette.user_background,
                semantic_color: Some(palette.semantic),
            }),
            TranscriptEntry::Answer {
                text,
                grounded,
                degraded,
                sources,
            } => {
                // 🔴 THE REPLY OPENS WITH A MARK. The old heading was `estelle  <label>` on its
                // own line: `estelle` named the program the user had just launched, and the
                // label was an internal routing word. Both are gone.
                //
                // ⚠️ **`conversation` WAS LOAD-BEARING AND ITS MEANING IS PRESERVED, NOT
                // DELETED.** It rendered only when `grounded is None`, and the sole producer of
                // that is `conversational_reply` — so it was the one thing distinguishing
                // *answered from the model* from *answered from your code, with citations*, which
                // is the entire claim this product makes. The MARK carries it now, in two
                // channels so a colour-flattening terminal keeps the distinction:
                //
                //   `●` green  answered from your code
                //   `○` dim    answered from the model
                //   `▲` warn   degraded, or explicitly not grounded
                //
                // A word survives ONLY on the two states a warn mark cannot disambiguate between.
                // A healthy reply says nothing, which is the point.
                // 🔴 **`○` BELONGS TO THE QUEUE AND MUST NOT ALSO MEAN "UNGROUNDED".** The first
                // version of this gave an ungrounded reply `Mark::Queued`, which is the SAME glyph
                // the waiting band puts on a message that has not been sent. The founder's screen
                // then showed one column mixing `○ It looks like you sent "d."` (a reply) with
                // `○ d` (a message not yet sent) — indistinguishable, and it is why the queue
                // looked like it was answering itself out of order. One meaning per name.
                //
                // A reply always LANDED, so a reply is always `●`. Grounding is the COLOUR, and
                // the second channel is structural rather than chromatic: a grounded answer
                // carries `cited …` lines in the evidence gutter and an ungrounded one does not.
                let (mark, ink, qualifier) = if *degraded {
                    (marks::Mark::Blocked, palette.warn, Some("degraded"))
                } else if *grounded == Some(false) {
                    (marks::Mark::Blocked, palette.warn, Some("not grounded"))
                } else if *grounded == Some(true) {
                    (marks::Mark::Landed, palette.grounded, None)
                } else {
                    (marks::Mark::Landed, palette.ungrounded, None)
                };
                let heading = qualifier
                    .map(|word| {
                        vec![Line::from(vec![Span::styled(
                            word.to_string(),
                            Style::default().fg(palette.warn),
                        )])]
                    })
                    .unwrap_or_default();
                let trailing = if include_citations {
                    sources
                        .iter()
                        .map(|source| {
                            Line::from(vec![
                                Span::styled("cited  ", Style::default().fg(palette.cite)),
                                Span::styled(
                                    source_label(source),
                                    Style::default().fg(palette.semantic),
                                ),
                            ])
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                items.push(HistoryTranscriptItem::Markdown {
                    heading,
                    source: mask_secret(text),
                    trailing,
                    semantic_color: Some(palette.semantic),
                    // ⚠️ The GLYPH comes from `marks::Mark` so the five-mark vocabulary keeps one
                    // owner; the COLOUR is chosen above, because two different groundings now
                    // share one glyph and a `match` on the mark could no longer tell them apart.
                    mark: Some((mark.glyph().to_string(), ink)),
                });
            }
            TranscriptEntry::Stale(currency) => {
                items.push(HistoryTranscriptItem::Lines(stale_lines(currency, palette)))
            }
            TranscriptEntry::System(message) => {
                items.push(HistoryTranscriptItem::Lines(vec![semantic_line(
                    &mask_secret(message),
                    palette.semantic,
                    Some(palette.ghost),
                )]))
            }
            TranscriptEntry::Painted { name, lines } => {
                // 🔴 **THE ONE ARM THAT DOES NOT RE-STYLE WHAT IT WAS GIVEN.** Every span arrives
                // with the colour its own renderer chose and leaves with it. The only thing done
                // to it is the redactor, which runs per span so a `Style` cannot smuggle a
                // credential past a fence that only ever looked at `String`s.
                let mut rendered = name
                    .as_deref()
                    .map(|name| vec![command_header(name, palette)])
                    .unwrap_or_default();
                rendered.extend(lines.iter().map(|line| masked(line)));
                items.push(HistoryTranscriptItem::Lines(rendered));
            }
            TranscriptEntry::Command { name, lines } => {
                let mut rendered = vec![command_header(name, palette)];
                rendered.extend(lines.iter().map(|line| {
                    let safe = if name == "skills" {
                        mask_skill_catalog_line(line)
                    } else {
                        mask_secret(line)
                    };
                    semantic_line(&safe, palette.semantic, None)
                }));
                items.push(HistoryTranscriptItem::Lines(rendered));
            }
            TranscriptEntry::Tool {
                label,
                lines,
                expanded,
            } => items.push(HistoryTranscriptItem::Tool {
                id: index,
                label: mask_secret(label),
                lines: lines
                    .iter()
                    .map(|line| {
                        semantic_line(&mask_secret(line), palette.semantic, Some(palette.ghost))
                    })
                    .collect(),
                expanded: *expanded,
                semantic_color: palette.semantic,
            }),
            TranscriptEntry::Failure(lines) => {
                // The refusal mark, `■`, in place of the words `estelle  failed`.
                let mut rendered = vec![Line::from(vec![
                    Span::styled(
                        format!("{} ", marks::Mark::Refused.glyph()),
                        Style::default().fg(palette.failure),
                    ),
                    Span::styled(
                        "failed",
                        Style::default()
                            .fg(palette.failure)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])];
                rendered.extend(
                    lines
                        .iter()
                        .map(|line| semantic_line(&mask_secret(line), palette.semantic, None)),
                );
                items.push(HistoryTranscriptItem::Lines(rendered));
            }
        }
    }
    render_interactive_history_transcript(items, width, cwd)
}

pub(crate) fn source_label(source: &Source) -> String {
    source.line.map_or_else(
        || source.file.clone(),
        |line| format!("{}:{line}", source.file),
    )
}

/// Caller-owned, content-masked journal projection for `/govern` compact mode.
pub(crate) fn compaction_messages(entries: &[TranscriptEntry]) -> Vec<Value> {
    let stop = if entries.last().is_some_and(
        |entry| matches!(entry, TranscriptEntry::User(text) if text.trim() == "/compact"),
    ) {
        entries.len().saturating_sub(1)
    } else {
        entries.len()
    };
    entries[..stop]
        .iter()
        .filter_map(|entry| match entry {
            TranscriptEntry::SessionHandoff(lines) => message("system", lines.join("\n")),
            TranscriptEntry::User(text) => message("user", text.clone()),
            TranscriptEntry::Answer { text, .. } => message("assistant", text.clone()),
            TranscriptEntry::Stale(currency) => message("system", currency.detail.clone()),
            TranscriptEntry::System(text) => message("system", text.clone()),
            TranscriptEntry::Command { name, lines } => {
                message("assistant", format!("/{name}\n{}", lines.join("\n")))
            }
            // The styles carry no meaning a language model can read, so the projection is the
            // block's own text — the same text a reader sees, with the colour dropped.
            TranscriptEntry::Painted { name, lines } => message(
                "assistant",
                format!(
                    "{}{}",
                    name.as_deref()
                        .map(|name| format!("/{name}\n"))
                        .unwrap_or_default(),
                    lines
                        .iter()
                        .map(|line| line
                            .spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>())
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            ),
            TranscriptEntry::Tool { label, lines, .. } => {
                message("assistant", format!("{label}\n{}", lines.join("\n")))
            }
            TranscriptEntry::Failure(lines) => message("assistant", lines.join("\n")),
        })
        .collect()
}

pub(crate) struct CompactionOutcome {
    pub(crate) replacement: Option<Vec<TranscriptEntry>>,
    pub(crate) receipt: TranscriptEntry,
    pub(crate) generation_after: Option<u64>,
}

/// Validate the content-bearing projection as well as the content-free receipt. A refusal may only
/// retain the caller's exact journal; a completed compaction must yield a replacement projection.
pub(crate) fn compaction_outcome(
    reply: &CommandReply,
    source: &[Value],
    generation: u64,
) -> CompactionOutcome {
    let failure = |what: &str, why: String, next: &str| CompactionOutcome {
        replacement: None,
        receipt: TranscriptEntry::Failure([what.to_string(), why, next.to_string()]),
        generation_after: None,
    };
    let view = match crate::commands::compaction_view(reply, generation) {
        Ok(view) => view,
        Err(reason) => {
            return failure(
                "The compaction receipt was not safe to apply.",
                reason,
                "The caller-owned journal remains active and its generation did not move.",
            );
        }
    };
    let Some(governed) = reply.extra.get("governed").and_then(Value::as_array) else {
        return failure(
            "Compaction returned no replacement projection.",
            "The content-free receipt exists, but governed is absent or not a list.".to_string(),
            "Keep the local journal and retry after the server contract is repaired.",
        );
    };
    if matches!(view.status.as_str(), "blocked" | "unchanged") && governed != source {
        return failure(
            "Compaction refusal changed the active transcript.",
            "The server receipt said no replacement, but governed did not equal the caller-owned source."
                .to_string(),
            "Keep the local journal and report this contract violation.",
        );
    }
    let replacement = if view.status == "compacted" {
        match projection_entries(governed) {
            Ok(entries) => Some(entries),
            Err(reason) => {
                return failure(
                    "Compaction returned an unusable replacement projection.",
                    reason,
                    "Keep the local journal and report the malformed governed message.",
                );
            }
        }
    } else {
        None
    };
    CompactionOutcome {
        replacement,
        receipt: TranscriptEntry::System(view.line),
        generation_after: Some(view.generation_after),
    }
}

fn projection_entries(messages: &[Value]) -> Result<Vec<TranscriptEntry>, String> {
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("governed message {index} has no string role"))?;
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("governed message {index} has no string content"))?;
            Ok(match role {
                "user" => TranscriptEntry::User(content.to_string()),
                "assistant" => TranscriptEntry::Answer {
                    text: content.to_string(),
                    grounded: None,
                    degraded: false,
                    sources: Vec::new(),
                },
                _ => TranscriptEntry::System(content.to_string()),
            })
        })
        .collect()
}

pub(crate) async fn request_compaction(
    client: Client,
    messages: Vec<Value>,
    session_id: String,
    generation: u64,
    task: String,
    model: String,
    cancel: &CancellationToken,
) -> Result<CommandReply, Error> {
    client
        .post(
            estelle_client::Endpoint::Govern,
            &json!({
                "messages": messages,
                "session_id": session_id,
                "generation": generation,
                "task": task,
                "model": model,
                "compact": true,
                "force": true,
            }),
            cancel,
        )
        .await
}

fn message(role: &str, content: String) -> Option<Value> {
    let content = mask_secret(&content);
    (!content.trim().is_empty()).then(|| json!({"role": role, "content": content}))
}

fn semantic_line(text: &str, semantic: Color, base: Option<Color>) -> Line<'static> {
    let mut spans = Vec::new();
    let mut start = 0;
    let mut whitespace = text.chars().next().is_some_and(char::is_whitespace);
    let mut previous_word = String::new();
    for (offset, character) in text.char_indices().skip(1) {
        let next_whitespace = character.is_whitespace();
        if next_whitespace == whitespace {
            continue;
        }
        push_segment(
            &mut spans,
            &text[start..offset],
            whitespace,
            &mut previous_word,
            semantic,
            base,
        );
        start = offset;
        whitespace = next_whitespace;
    }
    if start < text.len() {
        push_segment(
            &mut spans,
            &text[start..],
            whitespace,
            &mut previous_word,
            semantic,
            base,
        );
    }
    Line::from(spans)
}

fn push_segment(
    spans: &mut Vec<Span<'static>>,
    segment: &str,
    whitespace: bool,
    previous_word: &mut String,
    semantic: Color,
    base: Option<Color>,
) {
    let word = segment.trim_matches(|character: char| {
        matches!(
            character,
            '`' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}'
        )
    });
    let follows_semantic_label = matches!(
        previous_word.as_str(),
        "command" | "file" | "link" | "path" | "symbol"
    );
    let semantic_token = !whitespace
        && (follows_semantic_label
            || word.starts_with("http://")
            || word.starts_with("https://")
            || word.starts_with('/')
            || word.starts_with('!')
            || word.contains("::")
            || word.contains('/')
            || [".rs", ".py", ".ts", ".tsx", ".js", ".md"]
                .iter()
                .any(|extension| word.contains(extension)));
    let style = if semantic_token {
        Style::default().fg(semantic)
    } else {
        base.map_or_else(Style::default, |color| Style::default().fg(color))
    };
    spans.push(Span::styled(segment.to_string(), style));
    if !whitespace && !word.is_empty() {
        *previous_word = word.to_ascii_lowercase();
    }
}

fn mask_skill_catalog_line(line: &str) -> String {
    let mask_name = |name: &str| {
        let valid = !name.is_empty()
            && name.len() <= 96
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !estelle_client::is_secret_shaped(name);
        if valid {
            name.to_string()
        } else {
            mask_secret(name)
        }
    };
    if let Some((name, description)) = line.split_once("  |  ") {
        return format!("{}  |  {}", mask_name(name), mask_secret(description));
    }
    if line.ends_with(" playbooks") {
        return mask_secret(line);
    }
    mask_name(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 **A STYLE IS NOT A DOOR AROUND THE FENCE.**
    ///
    /// [`TranscriptEntry::Painted`] exists so a block can keep the colours its own renderer chose,
    /// and the obvious way to build it would have been to hand the spans straight through. That
    /// would have been a second path into the transcript with no redactor on it — the shape the
    /// repo has now paid for three times under a different tool's name. So [`masked`] runs
    /// `mask_secret` over EVERY span, and this presses both halves.
    ///
    /// ⚠️ **THE POSITIVE CONTROL IS THE STYLE.** A test that only asserted "the credential is
    /// gone" would pass over an implementation that flattened the line to one grey span — which is
    /// precisely the defect `Painted` was added to end. So the surviving spans are asserted to
    /// keep their own colours.
    #[test]
    fn a_painted_block_is_masked_per_span_and_keeps_every_colour() {
        let secret = "sk-ant-api03-notarealkey-demo-fixture-0000000000";
        let line = Line::from(vec![
            Span::styled("token ", Style::default().fg(Color::Green)),
            Span::styled(secret.to_string(), Style::default().fg(Color::Red)),
            Span::styled(" rotated", Style::default().fg(Color::Blue)),
        ]);
        let out = masked(&line);

        // The credential is gone.
        let text = out
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(
            !text.contains(secret),
            "a credential survived a styled row: {text:?}"
        );
        assert!(text.contains("[credential hidden]"), "{text:?}");

        // 🔴 And the row is still THREE differently coloured spans, not one flattened one.
        assert_eq!(out.spans.len(), 3, "masking collapsed the row's spans");
        assert_eq!(out.spans[0].style.fg, Some(Color::Green));
        assert_eq!(out.spans[1].style.fg, Some(Color::Red));
        assert_eq!(out.spans[2].style.fg, Some(Color::Blue));
        assert_eq!(out.spans[0].content, "token ");
        assert_eq!(out.spans[2].content, " rotated");

        // The vacuity control: the redactor is genuinely the thing doing the work here.
        assert_eq!(mask_secret(secret), "[credential hidden]");
    }

    #[test]
    fn semantic_tokens_are_blue_without_recolouring_prose() {
        let line = semantic_line(
            "open src/main.rs:42 then /verify charge::run and https://fatelabs.ca",
            Color::Blue,
            Some(Color::Gray),
        );
        let blue = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(Color::Blue))
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            blue,
            [
                "src/main.rs:42",
                "/verify",
                "charge::run",
                "https://fatelabs.ca"
            ]
        );
        assert_eq!(line.spans[0].style.fg, Some(Color::Gray));
    }

    #[test]
    fn compact_command_is_not_part_of_the_journal_it_requests_to_replace() {
        let messages = compaction_messages(&[
            TranscriptEntry::User("keep me".to_string()),
            TranscriptEntry::Answer {
                text: "kept answer".to_string(),
                grounded: Some(true),
                degraded: false,
                sources: Vec::new(),
            },
            TranscriptEntry::User("/compact".to_string()),
        ]);
        assert_eq!(messages.len(), 2);
        assert!(
            messages
                .iter()
                .all(|message| message["content"] != "/compact")
        );
    }

    #[test]
    fn compacted_projection_replaces_the_journal_and_advances_exactly_once() {
        let source = vec![json!({"role": "user", "content": "old"})];
        let reply: CommandReply = serde_json::from_value(json!({
            "governed": [
                {"role": "system", "content": "bounded summary"},
                {"role": "user", "content": "recent turn"}
            ],
            "compaction": {
                "status": "compacted",
                "reason": "history_exceeded_usable_window",
                "generation_before": 4,
                "generation_after": 5
            }
        }))
        .expect("govern reply");

        let outcome = compaction_outcome(&reply, &source, 4);

        assert_eq!(outcome.generation_after, Some(5));
        assert_eq!(outcome.replacement.as_ref().map(Vec::len), Some(2));
        assert!(matches!(
            outcome
                .replacement
                .as_ref()
                .and_then(|entries| entries.first()),
            Some(TranscriptEntry::System(text)) if text == "bounded summary"
        ));
    }

    #[test]
    fn malformed_compacted_projection_cannot_erase_the_journal() {
        let source = vec![json!({"role": "user", "content": "old"})];
        let reply: CommandReply = serde_json::from_value(json!({
            "governed": [{"role": "system"}],
            "compaction": {
                "status": "compacted",
                "reason": "history_exceeded_usable_window",
                "generation_before": 4,
                "generation_after": 5
            }
        }))
        .expect("govern reply");

        let outcome = compaction_outcome(&reply, &source, 4);

        assert!(outcome.replacement.is_none());
        assert_eq!(outcome.generation_after, None);
        assert!(matches!(outcome.receipt, TranscriptEntry::Failure(_)));
    }
}
