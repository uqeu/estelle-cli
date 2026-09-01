//! The live Estelle terminal renderer.
//!
//! This module is the single owner of the customer-visible frame. Snapshot commands and tests
//! must enter through `render_frame`; a second hand-built representation is forbidden.

use super::*;
use crate::cols::{self, Cell, Col};
#[cfg(test)]
pub(super) fn render_transcript(entries: &[TranscriptEntry]) -> Text<'static> {
    render_transcript_with_citations(entries, true, Theme::Dark, 120).text
}

pub(super) fn render_transcript_with_citations(
    entries: &[TranscriptEntry],
    include_citations: bool,
    theme: Theme,
    width: u16,
) -> estelle_tui::RenderedHistoryTranscript {
    transcript::render(
        entries,
        include_citations,
        TranscriptPalette {
            primary: theme.primary(),
            ghost: theme.ghost(),
            semantic: theme.semantic(),
            user_background: user_turn_background(theme),
        },
        width,
        Path::new("."),
    )
}

pub(super) fn header_line(app: &App, _width: u16) -> Line<'static> {
    let palette = app.theme.screen_palette();
    let mut spans = vec![
        Span::styled(
            "ESTELLE",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(app.theme.ghost())),
        Span::styled(
            app.repo.to_string(),
            Style::default().fg(app.theme.primary()),
        ),
    ];
    if let Some(plan) = app.header.plan.as_deref() {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            plan.to_string(),
            Style::default().fg(palette.mid),
        ));
    }
    if let Some(team) = app
        .account
        .as_ref()
        .and_then(|account| account.team.as_ref())
    {
        let label = team.name.as_deref().unwrap_or(team.id.as_str());
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            format!("{label} · {}", team.role.as_deref().unwrap_or("member")),
            Style::default().fg(palette.mid),
        ));
    }
    if let Some(indexed) = app.header.indexed {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            if indexed {
                "repo graph current"
            } else {
                "repo graph absent"
            },
            Style::default().fg(if indexed {
                app.theme.primary()
            } else {
                FATE_RED
            }),
        ));
    }
    if let Some(files) = app.header.files {
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(app.theme.ghost()),
        ));
        spans.push(Span::styled(
            format!("{} files", commas(files)),
            Style::default().fg(palette.mid),
        ));
    }
    Line::from(spans)
}

pub(super) fn session_tabs_line(app: &App) -> Line<'static> {
    if app.session_tabs.is_empty() {
        return Line::default();
    }
    let palette = app.theme.screen_palette();
    let mut spans = vec![Span::styled(
        "sessions  ",
        Style::default()
            .fg(palette.dim)
            .add_modifier(Modifier::BOLD),
    )];
    for session in &app.session_tabs {
        if app.hidden_session_tabs.contains(&session.id) {
            continue;
        }
        let marker = if session.active { "+" } else { "·" };
        let label = format!(" {marker} {} ", session.id);
        let style = if session.id == app.session_id {
            Style::default()
                .fg(app.theme.background())
                .bg(app.theme.primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.ghost())
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        "Alt+Left/Right switch · Ctrl+W close view",
        Style::default().fg(app.theme.ghost()),
    ));
    Line::from(spans)
}

pub(super) fn commas(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

pub(super) fn observed_model(reply: &CommandReply) -> Option<&str> {
    reply
        .extra
        .get("active")
        .and_then(Value::as_object)
        .and_then(|active| active.get("model"))
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .or_else(|| {
            reply
                .routed
                .as_deref()
                .filter(|model| !model.trim().is_empty())
        })
}

/// The footer carries the design's key hints ahead of the live status.
///
/// ⚠️ `KEY_HINTS` is the catalog's screen-9 wording verbatim. The demo mockup shows
/// `enter send` and `esc stop` beside them; neither exists in the restored design code, so
/// neither is printed here.
/// The demo frame's status row: the run state on the left, spend and gate on the right.
///
/// 🔴 **THIS ROW DID NOT EXIST.** The bottom of the frame was a hint line and a status tail
/// (`tab focus · shift+tab autonomy · … | plan · routing auto`) crammed onto one row under the
/// composer. The demo puts the run state where the eye already is — directly above the thing you
/// type into — and pushes money and refusals to the right edge where a number belongs.
///
/// ⚠️ **AN ABSENT CELL IS OMITTED, NEVER ZEROED.** `$0.000` and `gate · 0 refused` are claims
/// that a measurement happened. Spend has no producer in this client at all (see
/// `SPEND_HAS_NO_PRODUCER`), so its cell is simply not drawn.
pub(super) fn status_bar_line(app: &App, now: Instant, width: usize) -> Line<'static> {
    let palette = app.theme.screen_palette();
    let (mark, left) = run_state(app, now);
    let mut right = String::new();
    if let Some(spend) = app.session_spend_usd {
        right.push_str(&format!("${spend:.3}"));
    }
    let gate = (app.gate_refusals > 0).then(|| format!("gate · {} refused", app.gate_refusals));
    let mut spans = vec![
        mark.span(&palette, pulse_tick(app, now), true),
        Span::styled(left.clone(), Style::default().fg(palette.mid)),
    ];
    let tail_width = right.chars().count()
        + gate.as_ref().map_or(0, |gate| gate.chars().count() + 4)
        + usize::from(!right.is_empty());
    let used = 2 + left.chars().count();
    let gap = width.saturating_sub(used).saturating_sub(tail_width).max(1);
    if !right.is_empty() || gate.is_some() {
        spans.push(Span::raw(" ".repeat(gap)));
    }
    if !right.is_empty() {
        spans.push(Span::styled(right, Style::default().fg(palette.green)));
    }
    if let Some(gate) = gate {
        let (label, count) = gate.split_at("gate · ".chars().count());
        spans.push(Span::styled(
            format!("    {label}"),
            Style::default().fg(palette.mid),
        ));
        spans.push(Span::styled(
            count.to_string(),
            Style::default().fg(palette.red),
        ));
    }
    Line::from(spans)
}

/// `✓ Done · 7 of 7 landed · production green` / `◐ Working · 4 of 7 landed · <active step>`.
///
/// The counts come from the plan the server sent, and the trailing phrase is the ACTIVE STEP's
/// own text — the demo's footer names the step, so the footer and the plan cannot disagree about
/// what is happening.
fn run_state(app: &App, now: Instant) -> (marks::Mark, String) {
    if let Some(plan) = app
        .work_progress
        .as_ref()
        .and_then(|progress| progress.plan.as_ref())
    {
        let total = plan.steps.len();
        let landed = plan
            .steps
            .iter()
            .filter(|step| marks::StepMark::from_status(&step.status) == marks::StepMark::Done)
            .count();
        let active = plan
            .steps
            .iter()
            .find(|step| marks::StepMark::from_status(&step.status) == marks::StepMark::Active)
            .map(|step| step.step.clone());
        return match active {
            Some(step) => (
                marks::Mark::InFlight,
                format!("Working · {landed} of {total} landed · {step}"),
            ),
            None if landed == total && total > 0 => (
                marks::Mark::Landed,
                format!("Done · {landed} of {total} landed"),
            ),
            None => (
                marks::Mark::Queued,
                format!("Idle · {landed} of {total} landed"),
            ),
        };
    }
    if let Some(active) = &app.active {
        // The 30-second escalation survives the redesign. It is the line that tells a user the
        // silence is the SERVER's and not the terminal's, and losing it to a prettier row would
        // have been the redesign quietly removing an honesty feature.
        let elapsed = now.saturating_duration_since(active.started).as_secs();
        let local_shell = active.label.starts_with("local shell");
        let mut text = format!(
            "Working · {} · {}",
            active.label,
            estelle_tui::fmt_elapsed_compact(elapsed)
        );
        if elapsed >= 30 {
            text.push_str(if local_shell {
                " · local command has not exited"
            } else {
                " · still waiting for Estelle"
            });
            if !local_shell {
                text.push_str(" · no response received yet");
            }
        }
        return (marks::Mark::InFlight, text);
    }
    if !app.queue.is_empty() {
        return (marks::Mark::Queued, format!("{} queued", app.queue.len()));
    }
    (marks::Mark::Landed, "Ready".to_string())
}

pub(super) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

pub(super) fn truncate_display(value: &str, max_width: usize) -> String {
    if value
        .chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>()
        <= max_width
    {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut rendered = String::new();
    let mut width: usize = 0;
    for ch in value.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width.saturating_add(ch_width).saturating_add(1) > max_width {
            break;
        }
        rendered.push(ch);
        width += ch_width;
    }
    rendered.push('…');
    rendered
}

pub(super) fn render_picker(frame: &mut Frame<'_>, picker: &PickerSurface, area: Rect, app: &App) {
    let login_context = match picker.title.as_str() {
        "Connect Estelle" => Some([
            Line::from("Estelle grounds your coding agent in your real codebase."),
            Line::from(
                "It runs on the model plan or API key you already have — Estelle never bills you for model tokens.",
            ),
        ]),
        "Choose how model tokens are paid" => Some([
            Line::from("Estelle identity and model payment are separate."),
            Line::from(
                "Choose the account that pays for inference; Estelle never bills model tokens.",
            ),
        ]),
        _ => None,
    };
    let context_height = login_context.as_ref().map_or(0, |lines| lines.len());
    let height = u16::try_from(
        picker
            .rows
            .len()
            .saturating_add(context_height)
            .saturating_add(3),
    )
    .unwrap_or(u16::MAX)
    .min(area.height.max(3));
    let modal = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(height),
        width: area.width,
        height,
    };
    frame.render_widget(Clear, modal);
    let palette = app.theme.screen_palette();
    let inner_width = usize::from(modal.width.saturating_sub(3));
    let label_width = (inner_width / 3).clamp(12, 24);
    let detail_width = inner_width.saturating_sub(label_width.saturating_add(3));
    let mut lines = login_context.into_iter().flatten().collect::<Vec<_>>();
    lines.extend(
        picker
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == picker.selected;
                let badge = if index < 9 {
                    (index + 1).to_string()
                } else {
                    " ".to_string()
                };
                Line::from(vec![
                    Span::styled(
                        format!(
                            "{} {} {:<label_width$}  ",
                            if selected { ">" } else { " " },
                            badge,
                            truncate_display(&row.label, label_width),
                        ),
                        if selected {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(palette.mid)
                        },
                    ),
                    Span::styled(
                        truncate_display(&row.detail, detail_width),
                        Style::default().fg(palette.dim),
                    ),
                ])
            })
            .chain(std::iter::once(Line::styled(
                "↑↓ navigate · 1-9 or Enter select · Esc close",
                Style::default().fg(palette.dim),
            ))),
    );
    // 🔴 `┌ SETTINGS ─────┐` WAS FIVE OF THE EIGHT BOXED FRAMES ON ITS OWN — this one block drew
    // the settings, skills, model-pool, autonomy and monitor-settings surfaces. It is now the
    // design's rule, from the same builder the session and production columns use.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(modal);
    frame.render_widget(
        Paragraph::new(session_view::title_rule(
            &picker.title,
            usize::from(rows[0].width),
            &palette,
            palette.cite,
        ))
        .style(Style::default().bg(app.theme.background())),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(lines).style(
            Style::default()
                .fg(app.theme.primary())
                .bg(app.theme.background()),
        ),
        rows[1],
    );
}

pub(super) fn render_resume_picker(
    frame: &mut Frame<'_>,
    picker: &ExternalResumePicker,
    area: Rect,
    app: &App,
) {
    let height = picker.desired_height().min(area.height.max(4));
    let modal = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(height),
        width: area.width,
        height,
    };
    frame.render_widget(Clear, modal);
    let mut lines = picker.lines(
        modal.width.saturating_sub(2),
        app.theme.primary(),
        app.theme.ghost(),
    );
    lines.push(Line::styled(
        if picker.is_empty() {
            "Esc close · Enter cannot submit an empty result"
        } else {
            "↑↓ navigate · 1-9 or Enter resume · Esc close"
        },
        Style::default().fg(app.theme.ghost()),
    ));
    let palette = app.theme.screen_palette();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(modal);
    frame.render_widget(
        Paragraph::new(session_view::title_rule(
            "resume · a previous session",
            usize::from(rows[0].width),
            &palette,
            palette.cite,
        ))
        .style(Style::default().bg(app.theme.background())),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(lines).style(
            Style::default()
                .fg(app.theme.primary())
                .bg(app.theme.background()),
        ),
        rows[1],
    );
}

pub(super) fn dither_glyph(x: usize, y: usize) -> &'static str {
    let hash = x
        .wrapping_mul(31)
        .wrapping_add(y.wrapping_mul(17))
        .wrapping_add(x.wrapping_mul(y));
    if hash.is_multiple_of(5) { "∷" } else { "·" }
}

pub(super) fn lily_coverage(x: f64, y: f64, bloom_x: f64, bloom_y: f64, radius: f64) -> f64 {
    // Terminal cells are taller than they are wide, so unit-x is compressed before
    // drawing the same shared spider-lily primitive used by the boot veil.
    let dx = (x - bloom_x) * 2.10 / radius;
    let dy = (y - bloom_y) / radius;
    if dx.abs() > 1.35 || !(-1.35..=1.25).contains(&dy) {
        return 0.0;
    }
    spider_lily_coverage(dx, dy)
}

pub(super) fn red_lily_coverage(x: f64, y: f64) -> f64 {
    lily_coverage(x, y, 0.78, 0.70, 0.14) * 0.96
}

pub(super) fn red_lily_braille(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    opacity: f64,
) -> Option<char> {
    const DOTS: [[u32; 4]; 2] = [[0, 1, 2, 6], [3, 4, 5, 7]];
    let mut mask = 0_u32;
    for (column, rows) in DOTS.iter().enumerate() {
        for (row, bit) in rows.iter().enumerate() {
            let unit_x = (x as f64 + (column as f64 + 0.5) / 2.0) / width.max(1) as f64;
            let unit_y = (y as f64 + (row as f64 + 0.5) / 4.0) / height.max(1) as f64;
            if red_lily_coverage(unit_x, unit_y) * opacity > 0.48 {
                mask |= 1 << bit;
            }
        }
    }
    (mask != 0).then(|| char::from_u32(0x2800 + mask).unwrap_or(' '))
}

pub(super) fn scene_coverage(x: usize, y: usize, width: usize, height: usize) -> f64 {
    let u = x as f64 / width.max(1) as f64;
    let v = y as f64 / height.max(1) as f64;
    let mut coverage: f64 = 0.0;

    let sun = ((u - 0.85) / 0.07).powi(2) + ((v - 0.13) / 0.07).powi(2);
    if sun < 1.0 {
        coverage = coverage.max(0.09);
    }

    for (cloud_x, cloud_y, cloud_width) in
        [(0.15, 0.06, 0.18), (0.49, 0.11, 0.23), (0.77, 0.16, 0.28)]
    {
        let cloud = ((u - cloud_x) / cloud_width).powi(2) + ((v - cloud_y) / 0.016).powi(2);
        if cloud < 1.0 {
            coverage = coverage.max(0.05);
        }
    }

    for (base, amplitude, ink, frequency_one, frequency_two, phase) in [
        (0.60, 0.026, 0.10, 5.1, 11.7, 1.2),
        (0.70, 0.038, 0.16, 3.9, 9.3, 4.0),
        (0.80, 0.050, 0.24, 3.1, 7.9, 2.3),
        (0.91, 0.058, 0.34, 2.4, 6.1, 5.4),
    ] {
        let ridge = base
            + (u * frequency_one + phase).sin() * amplitude
            + (u * frequency_two + phase * 2.7).sin() * amplitude * 0.4;
        if v >= ridge {
            coverage = coverage.max(ink);
        }
    }

    for (bloom_x, bloom_y, radius, alpha) in [
        (0.05, 0.70, 0.050, 0.32),
        (0.20, 0.745, 0.055, 0.36),
        (0.44, 0.68, 0.050, 0.32),
        (0.60, 0.645, 0.042, 0.26),
        (0.93, 0.66, 0.050, 0.34),
        (0.33, 0.82, 0.075, 0.44),
        (0.66, 0.80, 0.065, 0.40),
        (0.88, 0.845, 0.080, 0.46),
    ] {
        coverage = coverage.max(lily_coverage(u, v, bloom_x, bloom_y, radius) * alpha);
    }
    coverage
}

#[derive(Debug)]
pub(super) struct SymbolGroundLayout {
    cells: Vec<char>,
    ink: Vec<u8>,
}

type SymbolGroundCache = Mutex<HashMap<(usize, usize), Arc<SymbolGroundLayout>>>;

static SYMBOL_GROUND_CACHE: OnceLock<SymbolGroundCache> = OnceLock::new();

// No `dimmed` variant: the scene's lifecycle owner is "has the first message been submitted",
// not "is the composer empty". It renders full-strength until submission, then not at all.
pub(super) fn symbol_ground_layout(width: usize, height: usize) -> Arc<SymbolGroundLayout> {
    let key = (width, height);
    let cache = SYMBOL_GROUND_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned();
    if let Some(layout) = cached {
        return layout;
    }

    let opacity = 1.0;
    let mut cells = vec![' '; width.saturating_mul(height)];
    let mut ink = vec![0_u8; width.saturating_mul(height)];
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let index = y * width + x;
            let coverage = scene_coverage(x, y, width, height) * opacity;
            let threshold = (f64::from(BAYER_8[y % 8][x % 8]) + 0.5) / 64.0;
            if let Some(symbol) = red_lily_braille(x, y, width, height, opacity) {
                cells[index] = symbol;
                ink[index] = 2;
                x += 1;
                continue;
            }
            if coverage <= threshold {
                x += 1;
                continue;
            }
            let glyph = if coverage > 0.30 {
                "∷"
            } else {
                dither_glyph(x, y)
            };
            for (offset, character) in glyph.chars().enumerate() {
                if x + offset < width {
                    cells[index + offset] = character;
                    ink[index + offset] = u8::from(coverage > 0.24);
                }
            }
            x += glyph.chars().count().saturating_add(1);
        }
    }

    let layout = Arc::new(SymbolGroundLayout { cells, ink });
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, Arc::clone(&layout));
    layout
}

/// The idle art's share of the pane: a corner flourish, not a field.
///
/// The founder's words were "shrink it to an idle flourish" - he likes the dither and it was in
/// the wrong place at the wrong size, filling most of the session column behind the empty state.
/// Anchored bottom-right so it never sits under the text of the empty state, which reads from the
/// top-left, and bounded so a very tall terminal does not turn it back into a field.
const FLOURISH_MAX_WIDTH: u16 = 44;
const FLOURISH_MAX_HEIGHT: u16 = 8;
/// Below this the pane has no room to spare and the art is dropped entirely.
const FLOURISH_MIN_WIDTH: u16 = 24;

pub(super) fn flourish_area(area: Rect) -> Option<Rect> {
    let width = area.width.min(FLOURISH_MAX_WIDTH);
    let height = (area.height / 3).min(FLOURISH_MAX_HEIGHT);
    if width < FLOURISH_MIN_WIDTH || height < 2 {
        return None;
    }
    Some(Rect {
        x: area.right().checked_sub(width)?,
        y: area.bottom().checked_sub(height)?,
        width,
        height,
    })
}

pub(super) fn render_symbol_ground(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let palette = app.theme.screen_palette();
    let width = usize::from(area.width);
    let height = usize::from(area.height);
    if width == 0 || height == 0 {
        return;
    }
    let layout = symbol_ground_layout(width, height);
    let mut rows = Vec::with_capacity(height);
    for y in 0..height {
        let row_start = y * width;
        let cells = &layout.cells[row_start..row_start + width];
        let ink = &layout.ink[row_start..row_start + width];
        let mut spans = Vec::new();
        let mut start = 0;
        while start < width {
            let ink_level = ink[start];
            let mut end = start + 1;
            while end < width && ink[end] == ink_level {
                end += 1;
            }
            spans.push(Span::styled(
                cells[start..end].iter().collect::<String>(),
                match ink_level {
                    2 => Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD),
                    1 => Style::default().fg(if app.theme == Theme::CreamInk {
                        palette.bright
                    } else {
                        FATE_INK
                    }),
                    _ => Style::default()
                        .fg(app.theme.ghost())
                        .add_modifier(Modifier::DIM),
                },
            ));
            start = end;
        }
        rows.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(rows), area);

    let composer_width = width.saturating_sub(4).max(1);
    for (age, cursor) in app.dither_wake.iter().rev().skip(1).enumerate() {
        let x = cursor % composer_width;
        let y = height
            .saturating_sub(1)
            .saturating_sub(cursor / composer_width);
        let glyph = if (cursor + age).is_multiple_of(3) {
            "∷"
        } else {
            "·"
        };
        frame.render_widget(
            Paragraph::new(glyph).style(Style::default().fg(FATE_RED)),
            Rect::new(
                area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX)),
                area.y.saturating_add(u16::try_from(y).unwrap_or(u16::MAX)),
                u16::try_from(glyph.len()).unwrap_or(1),
                1,
            ),
        );
    }
    let cursor = app.composer.cursor();
    let x = cursor % composer_width;
    let y = height
        .saturating_sub(1)
        .saturating_sub(cursor / composer_width);
    frame.render_widget(
        Paragraph::new("∷").style(Style::default().fg(app.theme.primary())),
        Rect::new(
            area.x.saturating_add(u16::try_from(x).unwrap_or(u16::MAX)),
            area.y.saturating_add(u16::try_from(y).unwrap_or(u16::MAX)),
            1,
            1,
        ),
    );
}

pub(super) fn session_handoff_lines(app: &App) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    if let Some(context) = &app.session_context {
        lines.extend(context.human_lines.iter().take(4).cloned());
    }
    if let Some(account) = &app.account {
        let identity = match (account.email.as_deref(), account.plan.as_deref()) {
            (Some(email), Some(plan)) => format!("Signed in · {email} · {plan}"),
            (Some(email), None) => format!("Signed in · {email}"),
            (None, Some(plan)) => format!("Account · {plan}"),
            (None, None) => "Account connected".to_string(),
        };
        lines.push(identity);
        if let Some(team) = &account.team {
            let name = team.name.as_deref().unwrap_or(&team.id);
            let role = team.role.as_deref().unwrap_or("role not returned");
            lines.push(format!("Team · {name} · {role}"));
        }
    }
    (!lines.is_empty()).then_some(lines)
}

pub(super) fn render_empty_state(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let palette = app.theme.screen_palette();
    let sweep = match app.header.indexed {
        Some(true) => "Refresh this repo's index",
        Some(false) => "Index this repo before asking grounded questions",
        None => "Index or refresh this repo",
    };
    let mut lines = vec![Line::styled(
        format!("Ask about {}", app.repo),
        Style::default()
            .fg(app.theme.primary())
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(context) = &app.session_context {
        lines.push(Line::default());
        lines.push(Line::styled(
            "Since your last session",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ));
        lines.extend(
            context
                .human_lines
                .iter()
                .take(4)
                .map(|line| Line::styled(line.clone(), Style::default().fg(palette.mid))),
        );
    }
    if let Some(account) = &app.account {
        lines.push(Line::default());
        let identity = match (account.email.as_deref(), account.plan.as_deref()) {
            (Some(email), Some(plan)) => format!("Signed in · {email} · {plan}"),
            (Some(email), None) => format!("Signed in · {email}"),
            (None, Some(plan)) => format!("Account · {plan}"),
            (None, None) => "Account connected".to_string(),
        };
        lines.push(Line::styled(identity, Style::default().fg(palette.mid)));
        if let Some(team) = &account.team {
            let name = team.name.as_deref().unwrap_or(&team.id);
            let role = team.role.as_deref().unwrap_or("role not returned");
            lines.push(Line::styled(
                format!("Team · {name} · {role}"),
                Style::default().fg(palette.mid),
            ));
        }
    }
    lines.extend([
        Line::default(),
        Line::from(vec![
            Span::styled("/review  ", Style::default().fg(app.theme.primary())),
            Span::styled("Read current changes", Style::default().fg(palette.mid)),
        ]),
        Line::from(vec![
            Span::styled("/sweep   ", Style::default().fg(app.theme.primary())),
            Span::styled(sweep, Style::default().fg(palette.mid)),
        ]),
        Line::from(vec![
            Span::styled("?        ", Style::default().fg(app.theme.primary())),
            Span::styled("Show shortcuts", Style::default().fg(palette.mid)),
        ]),
    ]);
    lines.truncate(usize::from(area.height));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// The live refusal, in the design's language, through the ONE renderer the catalog also calls.
///
/// 🔴 **THIS USED TO BE A SECOND DESIGN.** It drew `┌ gate · deterministic · no model ┐` with
/// `Borders::ALL`, a braille scatter `Chart` of changed lines per file, `{:>6}  {path}`
/// hand-positioned rows and three literal `Color::Gray`/`FATE_RED` styles — while `screens.rs`
/// drew the dashed-rule block the design specifies. The block below is now
/// [`crate::gate_refusal::lines`], the same function screen 10 renders, so the refusal a customer
/// sees and the refusal the catalog shows cannot drift apart.
///
/// The modal still `Clear`s and paints its own ground, because it overlays the transcript and a
/// transparent overlay is unreadable — that is an overlay, not a boxed panel.
pub(super) fn render_gate_modal(
    frame: &mut Frame<'_>,
    modal: &GateModal,
    content_area: Rect,
    app: &App,
    now: Instant,
) {
    let width = content_area.width.saturating_sub(4).min(86);
    let height = content_area.height.saturating_sub(2).min(18);
    let area = centered_rect(width, height, content_area);
    let palette = app.theme.screen_palette();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().fg(palette.mid).bg(palette.ground)),
        area,
    );

    let blockers = modal
        .reasons
        .iter()
        .map(|reason| gate_refusal::Blocker {
            claim: reason.as_str(),
            finding: None,
        })
        .collect::<Vec<_>>();
    let files = modal
        .files
        .iter()
        .map(|file| (file.path.clone(), file.changed_lines))
        .collect::<Vec<_>>();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(gate_refusal::lines(
            &gate_refusal::Refusal {
                detail: &format!("verdict {}", modal.verdict),
                note: Some("Gate protected this repository. Nothing was written."),
                blockers: &blockers,
                files: &files,
            },
            &palette,
            usize::from(rows[0].width),
            pulse_tick(app, now),
            true,
        )),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Enter or Esc closes · Ask Estelle",
            Style::default().fg(palette.dim),
        )),
        rows[1],
    );
}

/// Unix epoch seconds — the clock the wire's own `state_observed_at` is expressed in.
///
/// ⚠️ `render_frame`'s `now` is an `Instant`, which is a monotonic reading with no epoch, so it
/// cannot be compared to a server timestamp. A row whose observation is dated AHEAD of this clock
/// therefore renders its age as `?` rather than as `0s`; see `orchestra_view::age`.
pub(super) fn epoch_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

/// The animation clock the design's `pulse` reads, in the one place that derives it.
pub(super) fn pulse_tick(app: &App, now: Instant) -> u64 {
    now.saturating_duration_since(app.boot_started)
        .as_millis()
        .checked_div(50)
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(0)
}

pub(super) fn render_context_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let palette = app.theme.screen_palette();
    let mut lines = vec![
        Line::styled(
            "Repo graph · team's swept copy",
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(format!(
            "{} files indexed",
            app.header
                .files
                .map(commas)
                .unwrap_or_else(|| "count pending".to_string())
        )),
    ];
    if app.citations.is_empty() {
        lines.push(Line::styled(
            "No grounded sources in the current answer.",
            Style::default().fg(palette.dim),
        ));
    } else {
        for source in app.citations.iter().take(8) {
            lines.push(Line::from(source_label(source)));
            let symbol = source
                .extra
                .get("symbol")
                .and_then(Value::as_str)
                .filter(|symbol| !symbol.trim().is_empty())
                .unwrap_or("symbol not disclosed");
            lines.push(Line::styled(
                format!("  symbol  {symbol}"),
                Style::default().fg(palette.dim),
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Working memory · local request context",
        Style::default()
            .fg(app.theme.primary())
            .add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::styled(
        "Sent through the configured Estelle model path.",
        Style::default().fg(palette.dim),
    ));
    lines.push(Line::styled(
        "Not added to the team's Repo graph.",
        Style::default().fg(palette.dim),
    ));
    if app.working_memory_paths.is_empty() {
        lines.push(Line::styled(
            "No eligible local files were attached to the last question.",
            Style::default().fg(palette.dim),
        ));
    } else {
        lines.extend(
            app.working_memory_paths
                .iter()
                .take(8)
                .map(|path| Line::from(path.clone())),
        );
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Alt+M or /context closes",
        Style::default().fg(palette.dim),
    ));
    // ⚠️ This box was the one that rendered BESIDE the new language in a single row:
    // `── session · uqeu/estelle ───  │  ┌ CONTEXT  Alt+M · /context ────┐`.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(session_view::title_rule(
            "context · alt+m · /context",
            usize::from(rows[0].width),
            &palette,
            if app.focus == FocusSurface::Auxiliary {
                palette.cite
            } else {
                palette.dim
            },
        )),
        rows[0],
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[1]);
}

#[cfg(test)]
pub(super) fn production_health_lines(
    response: &estelle_client::MonitorIssuesResponse,
    overview: Option<&estelle_client::MonitorOverviewResponse>,
) -> Vec<String> {
    let unresolved = response
        .issues
        .iter()
        .filter(|issue| issue.status != "resolved")
        .collect::<Vec<_>>();
    if unresolved.is_empty() {
        if let Some(resolved) = response.issues.first() {
            return vec![
                "prod · healthy".to_string(),
                format!("resolved · {}", issue_title(resolved)),
            ];
        }
        return vec!["prod · healthy".to_string()];
    }

    let mut lines = vec![format!(
        "prod · {} unresolved issue{}",
        unresolved.len(),
        if unresolved.len() == 1 { "" } else { "s" }
    )];
    if let Some(overview) = overview {
        let buckets = overview.error_buckets();
        if !buckets.is_empty() {
            lines.push(format!(
                "error counts · {}",
                error_count_sparkline(&buckets)
            ));
            let requests_available = overview.requests_source() != Some("unavailable")
                && buckets.iter().all(|bucket| bucket.requests.is_some());
            if requests_available {
                let errors = buckets.iter().map(|bucket| bucket.errors).sum::<u64>();
                let requests = buckets
                    .iter()
                    .filter_map(|bucket| bucket.requests)
                    .sum::<u64>();
                lines.push(format!("measured · {errors} errors / {requests} requests"));
            } else {
                lines.push("request denominator unavailable".to_string());
            }
            if let Some(p99_ms) = buckets
                .iter()
                .filter_map(|bucket| bucket.p99_ms)
                .reduce(f64::max)
            {
                lines.push(format!("p99 · {p99_ms:.0} ms"));
            }
        }
    }

    for issue in unresolved.into_iter().take(3) {
        lines.push(String::new());
        if issue.effective_repair_status().contains("sandbox") {
            let verdict = issue
                .effective_gate_verdict()
                .or(issue.gate_absent_reason.as_deref())
                .unwrap_or("verdict unavailable");
            lines.push(format!("sandbox · a clone, never production · {verdict}"));
            continue;
        }
        lines.push(format!("caught · {}", issue_title(issue)));
        let events = issue.event_count();
        lines.push(format!("grouped · {events} events"));
        if let Some(read) = issue
            .extra
            .get("read")
            .and_then(Value::as_str)
            .filter(|read| !read.trim().is_empty())
        {
            lines.push(format!("read · {read}"));
        }
        if let Some(range) = &issue.symbol_range {
            lines.push(format!(
                "traced to · {}:{}-{}",
                range.file, range.line_start, range.line_end
            ));
        } else if let Some((file, line)) = issue.bound_location() {
            lines.push(format!("traced to · {file}:{line}"));
        } else if let Some(symbol) = issue
            .bound
            .as_ref()
            .and_then(|bound| bound.symbol.as_deref())
            .filter(|symbol| !symbol.trim().is_empty())
        {
            lines.push(format!("traced to · {symbol} · range unavailable"));
        } else if !issue.symbol.is_empty() {
            lines.push(format!("traced to · {} · range unavailable", issue.symbol));
        } else if !issue.culprit.is_empty() {
            lines.push(format!("traced to · {} · range unavailable", issue.culprit));
        }
        let bind_status = issue.effective_bind_status();
        if bind_status.trim().is_empty() {
            lines.push("bind · unbound · reason not recorded".to_string());
        } else if bind_status != "bound" {
            let bind_detail = issue.effective_bind_detail();
            let detail = if bind_detail.trim().is_empty() {
                "reason not recorded"
            } else {
                bind_detail
            };
            lines.push(format!("bind · {bind_status} · {detail}"));
        }
        let repair_pr = issue.effective_repair_pr();
        let repair_status = issue.effective_repair_status();
        if !repair_pr.is_empty() {
            lines.push(format!("PR · {repair_pr}"));
        } else if repair_status == "proposed" {
            lines.push("drafted repair · awaiting human review".to_string());
        } else if !repair_status.is_empty() && repair_status != "none" {
            lines.push(format!("repair · {repair_status}"));
        }
        if let Some(verdict) = issue.effective_gate_verdict() {
            lines.push(format!("gate · {verdict}"));
        } else if let Some(reason) = issue
            .gate_absent_reason
            .as_deref()
            .filter(|reason| !reason.trim().is_empty())
        {
            lines.push(format!("gate · {reason}"));
        }
    }
    lines
}

/// The production rail's model: the app, its services, its agents, Estelle's own status, the
/// repair queue and GitHub — in the design's language.
///
/// 🔴 **THE RAIL SPOKE A DIFFERENT DESIGN FROM THE COLUMN IT SITS IN.** Its sections opened on
/// shouted bold headings (`APP HEALTH`, `AGENT HEALTH`, `ESTELLE STATUS`, `ESTELLE QUEUE`,
/// `GITHUB`) in `app.theme.primary()` while the frame around it opened on `── production · repo ──`
/// from the palette. Every section now opens on a rule from [`crate::cols::rule`] and every colour
/// comes from [`crate::theme::Palette`], so the rail and its frame are one design.
///
/// ⚠️ **AN EMPTY STATE HERE NAMES THE READ THAT HAS NOT ANSWERED.** `Loading a real Monitor
/// window...` and `Waiting for the live issue feed...` told the reader that something was expected
/// and nothing about WHAT. Each absence below names its endpoint, so a rail that stays empty is a
/// bug report the customer can act on.
pub(super) fn production_workspace_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let palette = app.theme.screen_palette();
    let mut lines = app_health_lines(app, &palette, width);
    for section in [
        agent_health_lines(app, &palette, width),
        estelle_status_lines(app, &palette, width),
        queue_lines(app, &palette, width),
        github_lines(app, &palette, width),
    ] {
        lines.push(Line::from(""));
        lines.extend(section);
    }
    lines
}

/// The rail's row: a mark, a label, a value — specimen sheet variant A, the founder's pick.
///
/// 🔴 **THE RAIL WAS PROSE.** `State unavailable · no read contract · send POST /agent/events.`
/// wrapped across two rows in a 30-column rail and read as an apology. A row cannot wrap, so the
/// honesty has to fit in a value: `agents   no read contract`. The endpoint moves into the value
/// where it fits and is dropped where it does not — the label still says WHICH thing is missing,
/// which is the part that cannot be lost.
const RAIL_LABEL: usize = 16;
const RAIL_GAP: usize = 2;
const RAIL_MARK: usize = 2;
const MIN_RAIL_VALUE: usize = 8;

fn rail_columns(width: usize) -> [Col; 3] {
    let fixed = RAIL_MARK + RAIL_GAP + RAIL_GAP;
    let text = width.saturating_sub(fixed).max(RAIL_LABEL + MIN_RAIL_VALUE);
    let label = RAIL_LABEL.min(text.saturating_sub(MIN_RAIL_VALUE));
    let value = text.saturating_sub(label).max(MIN_RAIL_VALUE);
    [Col::l(RAIL_MARK), Col::l(label), Col::l(value)]
}

/// One `mark · label · value` row.
fn rail_row(
    mark: marks::Mark,
    label: &str,
    value: &str,
    palette: &theme::Palette,
    width: usize,
    value_colour: Color,
) -> Line<'static> {
    let columns = rail_columns(width);
    Line::from(
        cols::row(
            &columns,
            &[
                Cell(mark.glyph(), mark.colour(palette)),
                Cell(label, palette.mid),
                Cell(value, value_colour),
            ],
            0,
        )
        .spans
        .into_iter()
        .map(|span| Span::styled(span.content.into_owned(), span.style))
        .collect::<Vec<_>>(),
    )
}

/// The thin rule that splits one group of rail rows from the next (specimen variant A).
fn rail_split(palette: &theme::Palette, width: usize) -> Line<'static> {
    Line::styled(
        format!(
            "{}{}",
            " ".repeat(RAIL_MARK + RAIL_GAP),
            cols::RULE.repeat(width.saturating_sub(RAIL_MARK + RAIL_GAP).max(4))
        ),
        Style::default().fg(palette.dim),
    )
}

fn dim_line(text: String, palette: &theme::Palette) -> Line<'static> {
    Line::styled(text, Style::default().fg(palette.dim))
}

fn mid_line(text: String, palette: &theme::Palette) -> Line<'static> {
    Line::styled(text, Style::default().fg(palette.mid))
}

fn alert_line(text: String, palette: &theme::Palette) -> Line<'static> {
    Line::styled(text, Style::default().fg(palette.red))
}

/// The app band: which app this is, its measured error window, and one row per monitored service.
fn app_health_lines(app: &App, palette: &theme::Palette, width: usize) -> Vec<Line<'static>> {
    let repo = app
        .prod_issues
        .as_ref()
        .and_then(|response| response.repo.as_deref())
        .unwrap_or_else(|| app.repo.as_str());
    let app_name = app.prod_overview.as_ref().and_then(|overview| {
        ["app", "app_name", "service", "service_name"]
            .into_iter()
            .find_map(|key| overview.extra.get(key).and_then(Value::as_str))
    });
    let org = app.prod_overview.as_ref().and_then(|overview| {
        ["org", "organization", "organization_name"]
            .into_iter()
            .find_map(|key| overview.extra.get(key).and_then(Value::as_str))
    });
    let identity = match (app_name, org) {
        (Some(app_name), Some(org)) => format!("{org}/{app_name}"),
        (Some(app_name), None) => app_name.to_string(),
        (None, _) => format!("repo {repo}"),
    };
    let mut lines = vec![session_view::section_rule(
        "app",
        &identity,
        width,
        palette,
        palette.green,
    )];

    if !app.auth_resolved {
        lines.push(dim_line("Connecting to Estelle...".to_string(), palette));
        return lines;
    }
    if app.client.is_none() {
        lines.push(dim_line("Live Monitor unavailable.".to_string(), palette));
        lines.push(dim_line("Run /login here.".to_string(), palette));
        return lines;
    }
    if let Some(error) = &app.prod_issue_error {
        lines.push(alert_line(error.clone(), palette));
        lines.push(dim_line(
            "The client will retry in the background.".to_string(),
            palette,
        ));
        return lines;
    }
    let Some(overview) = &app.prod_overview else {
        lines.push(dim_line(
            "GET /monitor/overview has not returned yet.".to_string(),
            palette,
        ));
        return lines;
    };

    let buckets = overview.error_buckets();
    if buckets.is_empty() {
        lines.push(dim_line(
            "No measured error window was returned.".to_string(),
            palette,
        ));
    } else {
        let errors = buckets.iter().map(|bucket| bucket.errors).sum::<u64>();
        let requests = buckets
            .iter()
            .filter_map(|bucket| bucket.requests)
            .sum::<u64>();
        let has_denominator = overview.requests_source() != Some("unavailable")
            && buckets.iter().all(|bucket| bucket.requests.is_some());
        lines.push(rail_row(
            if errors > 0 {
                marks::Mark::Blocked
            } else {
                marks::Mark::Landed
            },
            "error counts",
            &format!("{} {errors}", error_count_sparkline(&buckets)),
            palette,
            width,
            palette.mid,
        ));
        if has_denominator {
            lines.push(rail_row(
                marks::Mark::Landed,
                "measured",
                &format!("{errors}/{requests} requests"),
                palette,
                width,
                palette.mid,
            ));
        } else {
            lines.push(rail_row(
                marks::Mark::Queued,
                "requests",
                "request denominator unavailable",
                palette,
                width,
                palette.dim,
            ));
        }
    }

    lines.push(rail_split(palette, width));
    let uptime = &overview.uptime;
    lines.push(session_view::section_rule(
        "services",
        &format!("{}/{} up", uptime.up, uptime.checks),
        width,
        palette,
        if uptime.down > 0 {
            palette.red
        } else {
            palette.green
        },
    ));
    lines.extend(production_hud::service_lines(overview, palette, width));
    lines
}

/// The agent band. `enabled: null` stays unknown and prints the server's own reason; it never
/// becomes `0 reporting`, which is a measurement.
fn agent_health_lines(app: &App, palette: &theme::Palette, width: usize) -> Vec<Line<'static>> {
    let mode = match app
        .prod_agent_health
        .as_ref()
        .and_then(|health| health.enabled)
    {
        Some(true) => "reporting",
        Some(false) => "not enabled",
        None => "unread",
    };
    let mut lines = vec![session_view::section_rule(
        "agents",
        mode,
        width,
        palette,
        palette.cite,
    )];
    if let Some(error) = &app.prod_agent_health_error {
        lines.push(alert_line(error.clone(), palette));
        lines.push(dim_line(
            "The client will retry in the background.".to_string(),
            palette,
        ));
        return lines;
    }
    let Some(health) = &app.prod_agent_health else {
        lines.push(rail_row(
            marks::Mark::Queued,
            "agents",
            "no read yet · GET /agent/health",
            palette,
            width,
            palette.dim,
        ));
        return lines;
    };
    match health.enabled {
        Some(false) => {
            lines.push(rail_row(
                marks::Mark::Queued,
                "telemetry",
                "Agent telemetry not enabled",
                palette,
                width,
                palette.dim,
            ));
            lines.push(rail_row(
                marks::Mark::Queued,
                "to enable",
                "POST /agent/events",
                palette,
                width,
                palette.dim,
            ));
            return lines;
        }
        None => {
            lines.push(rail_row(
                marks::Mark::Blocked,
                "agent health",
                health
                    .enabled_absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason"),
                palette,
                width,
                palette.warn,
            ));
            return lines;
        }
        Some(true) => {}
    }
    if let Some(counts) = &health.counts {
        let count = |value: Option<u64>, label: &str| match value {
            Some(value) => format!("{value} {label}"),
            None => format!("{label} unknown"),
        };
        lines.push(mid_line(
            format!(
                "{} · {} · {}",
                count(counts.reporting, "reporting"),
                count(counts.degraded, "degraded"),
                count(counts.silent, "silent")
            ),
            palette,
        ));
    } else {
        lines.push(dim_line(
            "Agent counts unavailable · server returned no measurement.".to_string(),
            palette,
        ));
    }
    match (health.observed_at, health.stale_after_s) {
        (Some(observed_at), Some(stale_after_s)) => lines.push(dim_line(
            format!("observed {observed_at:.0} · stale threshold {stale_after_s}s"),
            palette,
        )),
        _ => lines.push(dim_line(
            "Snapshot freshness unavailable.".to_string(),
            palette,
        )),
    }
    for agent in health.agents.iter().take(3) {
        let (state, colour) = match agent.state {
            estelle_client::AgentHealthState::Healthy => ("healthy", palette.green),
            estelle_client::AgentHealthState::Degraded => ("degraded", palette.warn),
            estelle_client::AgentHealthState::Silent => ("silent", palette.red),
            estelle_client::AgentHealthState::Disabled => ("disabled", palette.dim),
            estelle_client::AgentHealthState::Unknown => ("unknown", palette.dim),
        };
        let events = agent
            .events
            .map(|events| format!("{events}ev"))
            .unwrap_or_else(|| "events?".to_string());
        let signal = agent
            .current_signal
            .as_deref()
            .filter(|signal| !signal.trim().is_empty())
            .or(agent.state_absent_reason.as_deref())
            .unwrap_or("signal unavailable");
        lines.push(Line::from(vec![
            Span::styled(state.to_string(), Style::default().fg(colour)),
            Span::styled(
                format!(" {} · {events} · {signal}", agent.id),
                Style::default().fg(palette.mid),
            ),
        ]));
        if let Some(last_seen) = agent.last_seen {
            lines.push(dim_line(
                format!("       last seen {last_seen:.0}"),
                palette,
            ));
        }
    }
    if health.agents.len() > 3 {
        lines.push(dim_line(
            format!("+{} more agents", health.agents.len() - 3),
            palette,
        ));
    }
    lines
}

/// What Estelle itself has caught, bound and traced.
fn estelle_status_lines(app: &App, palette: &theme::Palette, width: usize) -> Vec<Line<'static>> {
    let Some(response) = app.prod_issues.as_ref() else {
        return vec![
            session_view::section_rule("estelle", "unread", width, palette, palette.dim),
            rail_row(
                marks::Mark::Queued,
                "issues",
                "no read yet · GET /issues",
                palette,
                width,
                palette.dim,
            ),
        ];
    };
    let unresolved = response
        .issues
        .iter()
        .filter(|issue| issue.status != "resolved")
        .collect::<Vec<_>>();
    let mut lines = vec![session_view::section_rule(
        "estelle",
        &format!("{} unresolved", unresolved.len()),
        width,
        palette,
        if unresolved.is_empty() {
            palette.green
        } else {
            palette.red
        },
    )];
    if unresolved.is_empty() {
        lines.push(dim_line(
            "No errors have reached Estelle yet.".to_string(),
            palette,
        ));
        lines.push(dim_line(
            "Point OTLP or Sentry at api.fatelabs.ca/monitor/ingest.".to_string(),
            palette,
        ));
        return lines;
    }
    for issue in unresolved.iter().take(2) {
        let events = issue.event_count();
        let location = issue
            .bound_location()
            .map(|(file, line)| format!("{file}:{line}"))
            .unwrap_or_else(|| "unbound · reason not recorded".to_string());
        lines.push(mid_line(
            format!("caught · {}", issue_title(issue)),
            palette,
        ));
        lines.push(dim_line(
            format!("grouped · {events} event(s) · traced to · {location}"),
            palette,
        ));
        if let Some(range) = &issue.symbol_range
            && range.line_end > range.line_start
        {
            lines.push(dim_line(
                format!(
                    "       range {}:{}-{}",
                    range.file, range.line_start, range.line_end
                ),
                palette,
            ));
        }
    }
    if unresolved.len() > 2 {
        lines.push(dim_line(
            format!("+{} more · open /monitor issues", unresolved.len() - 2),
            palette,
        ));
    }
    lines
}

/// The repair queue: what Estelle has drafted, where it is going, and the gate's verdict on it.
fn queue_lines(app: &App, palette: &theme::Palette, width: usize) -> Vec<Line<'static>> {
    let queued = app
        .prod_issues
        .as_ref()
        .into_iter()
        .flat_map(|response| response.issues.iter())
        .filter(|issue| issue.status != "resolved")
        .filter(|issue| {
            !issue.effective_repair_status().trim().is_empty()
                && issue.effective_repair_status() != "none"
        })
        .take(3)
        .collect::<Vec<_>>();
    let mut lines = vec![session_view::section_rule(
        "queue",
        &format!("{} repair(s)", queued.len()),
        width,
        palette,
        if queued.is_empty() {
            palette.dim
        } else {
            palette.warn
        },
    )];
    if queued.is_empty() {
        lines.push(rail_row(
            marks::Mark::Queued,
            "repairs",
            "none reported",
            palette,
            width,
            palette.dim,
        ));
        lines.push(rail_row(
            marks::Mark::Queued,
            "select",
            "/monitor issues",
            palette,
            width,
            palette.dim,
        ));
        return lines;
    }
    for issue in queued {
        let repair_pr = issue.effective_repair_pr();
        let repair_status = issue.effective_repair_status();
        let destination = if repair_pr.trim().is_empty() {
            "awaiting human review".to_string()
        } else {
            repair_pr.to_string()
        };
        let label = if repair_status == "proposed" {
            "drafted repair"
        } else {
            repair_status
        };
        lines.push(mid_line(
            format!("{label} · {} · {destination}", issue_title(issue)),
            palette,
        ));
        if let Some(verdict) = issue.effective_gate_verdict() {
            lines.push(dim_line(format!("       gate · {verdict}"), palette));
        } else if let Some(reason) = issue
            .gate_absent_reason
            .as_deref()
            .filter(|reason| !reason.trim().is_empty())
        {
            lines.push(dim_line(format!("       gate absent · {reason}"), palette));
        }
        if let Some(patch) = issue.effective_repair_patch() {
            let short_sha = patch.base_sha.chars().take(12).collect::<String>();
            lines.push(dim_line(
                format!("       patch · {} · base {short_sha}", patch.format),
                palette,
            ));
            lines.extend(github_diff_lines(&patch.text, 96, app));
        } else {
            let reason = issue
                .effective_patch_absent_reason()
                .unwrap_or("unavailable");
            lines.push(dim_line(
                format!("       diff unavailable - {reason}"),
                palette,
            ));
        }
    }
    lines
}

/// The GitHub band. An unknown connection stays unknown — a proposed-PR list is not read, and
/// certainly not inferred, without a measured App binding.
fn github_lines(app: &App, palette: &theme::Palette, width: usize) -> Vec<Line<'static>> {
    let connected = app
        .prod_github_status
        .as_ref()
        .and_then(|status| status.connected);
    let mode = match connected {
        Some(true) => "connected",
        Some(false) => "unbound",
        None => "unknown",
    };
    let mut lines = vec![session_view::section_rule(
        "github",
        mode,
        width,
        palette,
        match connected {
            Some(true) => palette.green,
            Some(false) => palette.dim,
            None => palette.warn,
        },
    )];
    if let Some(error) = &app.prod_github_status_error {
        lines.push(alert_line(error.clone(), palette));
    } else if let Some(status) = &app.prod_github_status {
        match status.connected {
            Some(true) => {
                let identity = status
                    .login
                    .as_deref()
                    .filter(|login| !login.trim().is_empty())
                    .map(|login| format!(" · @{login}"))
                    .unwrap_or_default();
                lines.push(mid_line(format!("Connected{identity}"), palette));
                if let Some(observed_at) = status.observed_at {
                    lines.push(dim_line(
                        format!("binding observed {observed_at:.0}"),
                        palette,
                    ));
                }
            }
            Some(false) => {
                lines.push(dim_line(
                    "Not connected · run estelle github connect.".to_string(),
                    palette,
                ));
                lines.push(dim_line(
                    "Proposed PRs are not read without a measured App binding.".to_string(),
                    palette,
                ));
            }
            None => {
                let reason = status
                    .absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason");
                lines.push(dim_line(format!("Connection unknown · {reason}"), palette));
                lines.push(dim_line(
                    "Proposed PR state is not inferred.".to_string(),
                    palette,
                ));
            }
        }
    } else {
        lines.push(rail_row(
            marks::Mark::Queued,
            "connection",
            "no read yet · GET /github/status",
            palette,
            width,
            palette.dim,
        ));
    }

    if connected != Some(true) {
        return lines;
    }
    if let Some(error) = &app.prod_proposed_prs_error {
        lines.push(alert_line(error.clone(), palette));
        return lines;
    }
    let Some(response) = &app.prod_proposed_prs else {
        lines.push(dim_line(
            "GET /prs has not returned yet.".to_string(),
            palette,
        ));
        return lines;
    };
    if response.prs.is_empty() {
        lines.push(dim_line(
            "No open Estelle-proposed PRs returned.".to_string(),
            palette,
        ));
    }
    for pr in response.prs.iter().take(3) {
        let title = if pr.title.trim().is_empty() {
            "untitled PR"
        } else {
            pr.title.as_str()
        };
        lines.push(mid_line(format!("#{} · {title}", pr.number), palette));
        lines.push(dim_line(format!("       {}", pr.url), palette));
        if let Some(gate) = &pr.gate {
            let confirmed = if gate.verified { " · verified" } else { "" };
            lines.push(dim_line(
                format!(
                    "       gate · {} · {} · {} blocker(s){confirmed}",
                    gate.state, gate.verdict, gate.blockers
                ),
                palette,
            ));
        } else {
            let reason = pr
                .gate_absent_reason
                .as_deref()
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or("server returned no reason");
            lines.push(dim_line(format!("       gate absent · {reason}"), palette));
        }
        if !pr.updated_at.trim().is_empty() {
            lines.push(dim_line(
                format!("       updated {}", pr.updated_at),
                palette,
            ));
        }
    }
    if response.has_more {
        lines.push(dim_line(
            "More open proposed PRs exist than this page shows.".to_string(),
            palette,
        ));
    }
    lines
}

pub(super) fn issue_title(issue: &estelle_client::MonitorIssue) -> &str {
    issue.display_title()
}

pub(super) fn error_count_sparkline(buckets: &[estelle_client::MonitorErrorBucket]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = buckets
        .iter()
        .map(|bucket| bucket.errors)
        .max()
        .unwrap_or(0);
    buckets
        .iter()
        .map(|bucket| {
            let index = bucket
                .errors
                .saturating_mul(7)
                .checked_div(max)
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(0);
            BARS[index.min(7)]
        })
        .collect()
}

pub(super) fn render_prod_panel(frame: &mut Frame<'_>, area: Rect, app: &App, now: Instant) {
    let palette = app.theme.screen_palette();
    // The rail's own rule takes the first row; every line under it is sized to what is left, so
    // the section rules inside the rail end on the same column as the rail's heading.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    let lines = if let Some(graph) = &app.prod_graph {
        production_hud::lines(
            graph,
            &palette,
            usize::from(rows[1].width),
            pulse_tick(app, now),
            true,
        )
    } else {
        let mut lines = production_workspace_lines(app, usize::from(rows[1].width));
        if app.prod_graph_in_flight {
            lines.push(Line::styled(
                "Reading blast_radius · chokepoints · subsystems · core_files...",
                Style::default().fg(app.theme.ghost()),
            ));
        } else if let Some(error) = &app.prod_graph_error {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(app.theme.alert()),
            ));
        }
        lines
    };
    let has_unresolved = app.prod_issues.as_ref().is_some_and(|response| {
        response
            .issues
            .iter()
            .any(|issue| issue.status != "resolved")
    });
    frame.render_widget(
        Paragraph::new(session_view::production_rule(
            app.repo.as_str(),
            usize::from(rows[0].width),
            &palette,
        ))
        .style(Style::default().fg(if has_unresolved {
            app.theme.alert()
        } else {
            app.theme.ghost()
        })),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().border_style(Style::default().fg(
                if app.focus == FocusSurface::Auxiliary {
                    app.theme.primary()
                } else {
                    app.theme.ghost()
                },
            )))
            .wrap(Wrap { trim: false }),
        rows[1],
    );
}

pub(super) fn render_diff_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let palette = app.theme.screen_palette();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    let mut lines = vec![Line::styled(
        "read-only · /apply submits this exact patch",
        Style::default().fg(palette.dim),
    )];
    if let Some(diff) = app.last_diff.as_deref() {
        lines.extend(github_diff_lines(diff, usize::from(rows[1].width), app));
    }
    frame.render_widget(
        Paragraph::new(session_view::title_rule(
            "work draft · /work · read only",
            usize::from(rows[0].width),
            &palette,
            if app.focus == FocusSurface::Auxiliary {
                palette.warn
            } else {
                palette.dim
            },
        )),
        rows[0],
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[1]);
}

pub(super) fn hunk_line_numbers(header: &str) -> Option<(usize, usize)> {
    let mut old = None;
    let mut new = None;
    for token in header.split_whitespace() {
        if let Some(range) = token.strip_prefix('-') {
            old = range.split(',').next().and_then(|value| value.parse().ok());
        } else if let Some(range) = token.strip_prefix('+') {
            new = range.split(',').next().and_then(|value| value.parse().ok());
        }
    }
    old.zip(new)
}

pub(super) fn github_diff_lines(diff: &str, width: usize, app: &App) -> Vec<Line<'static>> {
    let palette = app.theme.screen_palette();
    let mut lines = Vec::new();
    let mut old_line = 0_usize;
    let mut new_line = 0_usize;
    let number_width = 3_usize;
    let content_width = width.saturating_sub(number_width * 2 + 5);

    let (add_line_bg, add_gutter_bg, del_line_bg, del_gutter_bg) = match app.theme {
        Theme::Dark => (
            Color::from_u32(0x21_3A_2B),
            Color::from_u32(0x16_2E_20),
            Color::from_u32(0x4A_22_1D),
            Color::from_u32(0x36_17_14),
        ),
        Theme::CreamInk => (
            Color::from_u32(0xDA_FB_E1),
            Color::from_u32(0xAC_EE_BB),
            Color::from_u32(0xFF_EB_E9),
            Color::from_u32(0xFF_CE_CB),
        ),
    };

    for source in diff.lines() {
        if let Some(path) = source.strip_prefix("diff --git a/") {
            let path = path.split(" b/").next().unwrap_or(path);
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::styled(
                path.to_string(),
                Style::default()
                    .fg(app.theme.primary())
                    .add_modifier(Modifier::BOLD),
            ));
            old_line = 0;
            new_line = 0;
            continue;
        }
        if source.starts_with("---")
            || source.starts_with("+++")
            || source.starts_with("index ")
            || source.starts_with("new file mode ")
            || source.starts_with("deleted file mode ")
        {
            continue;
        }
        if source.starts_with("@@") {
            if let Some((old, new)) = hunk_line_numbers(source) {
                old_line = old;
                new_line = new;
            }
            lines.push(Line::styled(
                truncate_display(source, width),
                Style::default().fg(palette.cite),
            ));
            continue;
        }

        let (old, new, sign, content, line_bg, gutter_bg, foreground) =
            if let Some(content) = source.strip_prefix('+') {
                let row = (
                    None,
                    Some(new_line),
                    '+',
                    content,
                    add_line_bg,
                    add_gutter_bg,
                    if app.theme == Theme::CreamInk {
                        FATE_INK
                    } else {
                        palette.green
                    },
                );
                new_line = new_line.saturating_add(1);
                row
            } else if let Some(content) = source.strip_prefix('-') {
                let row = (
                    Some(old_line),
                    None,
                    '-',
                    content,
                    del_line_bg,
                    del_gutter_bg,
                    if app.theme == Theme::CreamInk {
                        FATE_INK
                    } else {
                        FATE_BG
                    },
                );
                old_line = old_line.saturating_add(1);
                row
            } else if let Some(content) = source.strip_prefix(' ') {
                let row = (
                    Some(old_line),
                    Some(new_line),
                    ' ',
                    content,
                    app.theme.background(),
                    app.theme.background(),
                    app.theme.ghost(),
                );
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                row
            } else {
                lines.push(Line::styled(
                    truncate_display(source, width),
                    Style::default().fg(palette.dim),
                ));
                continue;
            };

        let old = old.map_or_else(String::new, |value| value.to_string());
        let new = new.map_or_else(String::new, |value| value.to_string());
        let content = truncate_display(content, content_width);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{old:>number_width$} {new:>number_width$} {sign} "),
                Style::default().fg(app.theme.ghost()).bg(gutter_bg),
            ),
            Span::styled(
                format!("{content:<content_width$}"),
                Style::default().fg(foreground).bg(line_bg),
            ),
        ]));
    }
    lines
}

/// Take back every row the composer drew BELOW its prompt, and put the demo's hint row there.
///
/// The composer owns its own chrome: one blank row of padding, then the prompt, then more blank
/// rows, then `? for shortcuts`. The demo has the hint line IMMEDIATELY under the prompt and no
/// second hint at all. Rather than fight the widget's height - which the slash palette and the
/// command popup legitimately need - the frame finds the prompt row in the rendered buffer and
/// overwrites what follows it.
///
/// A popup is drawn ABOVE the prompt, so nothing here can clip one. When no prompt is on screen
/// (a popup owns the whole area) this is a no-op rather than a guess.
fn collapse_composer_tail(
    frame: &mut Frame<'_>,
    area: Rect,
    palette: &theme::Palette,
    background: Color,
) {
    let Some((prompt_row, prompt_col)) = (area.y..area.bottom()).find_map(|y| {
        (area.x..area.right())
            .find(|x| {
                let symbol = frame.buffer_mut()[(*x, y)].symbol();
                symbol == COMPOSER_PROMPT_GLYPH || symbol == PROMPT_GLYPH
            })
            .map(|x| (y, x))
    }) else {
        return;
    };
    // The composer widget draws U+203A, a small angle quote. The demo's prompt is U+3009, the
    // tall bracket - at terminal size they are not the same character. Repainted HERE rather than
    // in the composer, because 80 lib snapshots carry that glyph and another lane is editing those
    // same files; the prompt is one cell, and taking it is cheaper than taking their diff.
    //
    // U+3009 is East Asian WIDE, so it needs two columns. `LIVE_PREFIX_COLS` is 2 and the second
    // is the reserved space before the text, so it fits the gutter exactly and cannot clip what
    // the user has typed. The blank keeps the buffer honest about the cell the glyph now covers.
    frame.buffer_mut()[(prompt_col, prompt_row)].set_symbol(PROMPT_GLYPH);
    if prompt_col.saturating_add(1) < area.right() {
        frame.buffer_mut()[(prompt_col + 1, prompt_row)].set_symbol("");
    }
    // Clear only what is the composer's OWN chrome: blank padding rows and its `? for shortcuts`
    // footer. A row with anything else on it belongs to the slash palette or the command popup,
    // which are drawn below the prompt and must survive - a hint row is not worth eating a menu.
    for y in prompt_row.saturating_add(1)..area.bottom() {
        let row = (area.x..area.right())
            .map(|x| frame.buffer_mut()[(x, y)].symbol().to_string())
            .collect::<String>();
        if !row.trim().is_empty() && !row.contains("for shortcuts") {
            break;
        }
        for x in area.x..area.right() {
            frame.buffer_mut()[(x, y)]
                .set_symbol(" ")
                .set_style(Style::default().bg(background));
        }
    }
    if prompt_row.saturating_add(1) < area.bottom() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("  {}", crate::ask_hints_line()),
                Style::default().fg(palette.dim),
            )),
            Rect {
                y: prompt_row.saturating_add(1),
                height: 1,
                ..area
            },
        );
    }
}

/// The demo's prompt: U+3009, the tall right angle bracket. NOT U+203A, the small angle quote the
/// composer used to draw - at terminal size they read as different characters entirely.
pub(super) const PROMPT_GLYPH: &str = "\u{3009}";

/// What the composer widget draws before the frame repaints it.
const COMPOSER_PROMPT_GLYPH: &str = "\u{203a}";

pub(super) fn render_frame(frame: &mut Frame<'_>, app: &App, now: Instant) {
    app.tool_click_targets.borrow_mut().clear();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(
            Style::default()
                .fg(app.theme.primary())
                .bg(app.theme.background()),
        ),
        area,
    );
    let content_area = area;
    // `bottom_pane_desired_height` includes the composer's OWN chrome - its hint row and the
    // padding around it - which is what left a blank row between the ask rule and the prompt
    // and pushed the hints two rows down. The frame draws that chrome itself now, so the
    // composer is given exactly the rows its text needs and not one more.
    // The text area gets exactly the rows the TYPED TEXT needs. `desired_height` bundles the
    // composer's own footer into its answer, and that footer is `? for shortcuts` - a second
    // hint line competing with the demo's. Sized to the text, the chrome has nowhere to draw.
    // The composer keeps its own height, because the slash palette and the command popup are
    // drawn INSIDE it and shrinking the area silently truncates them. What the frame takes back
    // is the rows BELOW the prompt - see `collapse_composer_tail`.
    let composer_height = app
        .composer
        .bottom_pane_desired_height(content_area.width)
        .clamp(5, 14);
    let modal_owns_input =
        app.picker.is_some() || app.resume_picker.is_some() || app.gate_modal.is_some();
    let composer_height = if modal_owns_input { 0 } else { composer_height };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            // status row + blank + ask rule + the text area + the hint row.
            Constraint::Length(if composer_height == 0 {
                0
            } else {
                composer_height.saturating_add(4)
            }),
            Constraint::Length(1),
        ])
        .split(content_area);

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            header_line(app, area.width),
            session_tabs_line(app),
        ])),
        rows[0],
    );
    let surface_rows = [rows[1]];
    let palette = app.theme.screen_palette();

    // 🔴 THE COLUMN ENGINE DECIDES THE LIVE FRAME. Until this call the live TUI referenced
    // `cols` zero times while the whole redesign was built on it, so the customer saw a
    // different design language from the catalog. `split`/`split_areas`/`divider` all read the
    // SAME `[Col; 3]`, which is why the glyph cannot drift away from the split.
    let design_columns = session_view::split(surface_rows[0].width);
    let design_split = session_view::split_areas(surface_rows[0]);
    let modal_open =
        app.gate_modal.is_some() || app.picker.is_some() || app.resume_picker.is_some();

    let diff_as_rail = app.diff_panel_visible && design_split.is_some();
    let show_diff_panel = app.diff_panel_visible && !diff_as_rail;
    let show_context_panel =
        !app.diff_panel_visible && app.context_panel_visible && design_split.is_some();
    let show_citation_pane = !app.diff_panel_visible
        && !show_context_panel
        && !app.citations.is_empty()
        && design_split.is_some();
    // The design gives production a PERMANENT home on the right, not a `/prod` toggle: a rail
    // you have to remember to open is, from the user's seat, a rail that is not there.
    let prod_as_rail = !app.diff_panel_visible
        && !show_context_panel
        && !show_citation_pane
        && !modal_open
        && design_split.is_some();
    let show_prod_panel =
        app.prod_panel_visible && !app.diff_panel_visible && design_split.is_none();
    let show_auxiliary_pane =
        diff_as_rail || prod_as_rail || show_context_panel || show_citation_pane;
    let main_areas = match (show_auxiliary_pane, design_split, design_columns.as_ref()) {
        (true, Some((session, _, rail)), Some(columns)) => {
            let divider = session_view::divider(columns, &palette);
            frame.render_widget(
                Paragraph::new(vec![divider; usize::from(surface_rows[0].height)]),
                surface_rows[0],
            );
            vec![session, rail]
        }
        _ => vec![surface_rows[0]],
    };

    if show_prod_panel {
        render_prod_panel(frame, surface_rows[0], app, now);
    } else if show_diff_panel {
        render_diff_panel(frame, surface_rows[0], app);
    } else {
        // "┌ CONVERSATION ─┐" was the old language. The design opens the left column on a
        // rule that names the repo, which is what the founder's demo shows and what every
        // screen in the catalog does.
        let session_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(main_areas[0]);
        frame.render_widget(
            Paragraph::new(session_view::session_rule(
                app.repo.as_str(),
                usize::from(session_rows[0].width),
                &palette,
            )),
            session_rows[0],
        );
        // The demo frame carries repo AND branch on the line BENEATH the rule, dim and indented -
        // it is not part of the rule. An unread branch prints the repo alone; the line never
        // invents a name for a detached HEAD or a directory git does not own.
        frame.render_widget(
            Paragraph::new(Line::styled(
                match app.branch.as_deref() {
                    Some(branch) => format!("   {} · {branch}", app.repo.as_str()),
                    None => format!("   {}", app.repo.as_str()),
                },
                Style::default().fg(palette.dim),
            )),
            session_rows[1],
        );
        let primary_area = session_rows[2];

        // 🔴 THE ORCHESTRA BAND IS THE DESIGN'S WORKER TABLE NOW. It was a five-across grid of
        // plain strings re-coloured by searching each line for `✓`/`×`/`◷` — so the colour was a
        // guess about text rather than a fact about a worker's state, and every width was
        // `usize::from(width) / 5`. `orchestra_view` draws the catalog's table off `cols` and the
        // palette, and `screens.rs` draws the SAME function.
        let transcript_band = if let Some(fleet) = &app.fleet {
            let lines = orchestra_view::lines(
                fleet,
                &palette,
                usize::from(primary_area.width),
                epoch_seconds(),
            );
            let wanted = u16::try_from(lines.len()).unwrap_or(u16::MAX);
            let fleet_height = wanted.min(primary_area.height.saturating_sub(1));
            let fleet_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(fleet_height), Constraint::Min(1)])
                .split(primary_area);
            frame.render_widget(Paragraph::new(lines), fleet_rows[0]);
            fleet_rows[1]
        } else {
            primary_area
        };
        let transcript_band = if app.todo_visible {
            if let Some(todo) = &app.todo {
                let lines = commands::todo_view_lines(todo, app.todo_expanded);
                let wanted = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                let height = wanted.min(transcript_band.height.saturating_sub(1));
                let bands = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(height), Constraint::Min(1)])
                    .split(transcript_band);
                let rendered = lines
                    .into_iter()
                    .map(|line| {
                        let style = if line.starts_with("✓ ") {
                            Style::default()
                                .fg(palette.green)
                                .add_modifier(Modifier::CROSSED_OUT)
                        } else if line.starts_with("● ") {
                            Style::default()
                                .fg(palette.cite)
                                .add_modifier(Modifier::BOLD)
                        } else if line == "Todo" {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(palette.mid)
                        };
                        Line::styled(line, style)
                    })
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(rendered), bands[0]);
                bands[1]
            } else {
                transcript_band
            }
        } else {
            transcript_band
        };
        let transcript_root = if let Some(progress) = &app.work_progress {
            let plan_lines = progress
                .plan
                .as_ref()
                .map(|plan| work_plan::lines_at(plan, &palette, usize::from(transcript_band.width)))
                .unwrap_or_default();
            let plan_height = u16::try_from(plan_lines.len()).unwrap_or(u16::MAX);
            let work_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(plan_height),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(transcript_band);
            if !plan_lines.is_empty() {
                frame.render_widget(Paragraph::new(plan_lines), work_rows[0]);
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "work  ",
                        Style::default()
                            .fg(app.theme.primary())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(progress.line(now), Style::default().fg(palette.mid)),
                ])),
                work_rows[1],
            );
            frame.render_widget(
                Paragraph::new(progress.phase_track())
                    .style(Style::default().fg(app.theme.ghost())),
                work_rows[2],
            );
            work_rows[3]
        } else if let Some(progress) = &app.sweep_progress {
            let sweep_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ])
                .split(transcript_band);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "sweep  ",
                        Style::default()
                            .fg(app.theme.primary())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(progress.line(), Style::default().fg(palette.mid)),
                ])),
                sweep_rows[0],
            );
            frame.render_widget(
                Gauge::default()
                    .gauge_style(
                        Style::default()
                            .fg(app.theme.primary())
                            .bg(app.theme.ghost()),
                    )
                    .ratio((progress.percent / 100.0).clamp(0.0, 1.0))
                    .label(format!("{:.0}%", progress.percent)),
                sweep_rows[1],
            );
            sweep_rows[2]
        } else {
            transcript_band
        };
        let transcript = render_transcript_with_citations(
            &app.transcript,
            !show_citation_pane,
            app.theme,
            transcript_root.width,
        );
        let paragraph = Paragraph::new(transcript.text).wrap(Wrap { trim: false });
        let line_count = paragraph.line_count(transcript_root.width);
        let visible = usize::from(transcript_root.height);
        let bottom_scroll = line_count.saturating_sub(visible);
        let scroll =
            u16::try_from(bottom_scroll.saturating_sub(app.transcript_scroll.min(bottom_scroll)))
                .unwrap_or(u16::MAX);
        let targets =
            transcript::visible_tool_targets(transcript.interactive_rows, transcript_root, scroll);
        *app.tool_click_targets.borrow_mut() = targets;
        let show_ground = !app.has_submitted_question
            && app.transcript.is_empty()
            && app.sweep_progress.is_none()
            && app.work_progress.is_none()
            && app.gate_modal.is_none()
            && app.fleet.is_none()
            && !app.todo_visible
            && app.picker.is_none()
            && app.resume_picker.is_none()
            // The production rail is permanent now, so it can no longer be a reason to drop
            // the empty-state ground: the art lives in the session column, which is still
            // empty. A rail the user asked for (diff, context, evidence) still displaces it.
            && (!show_auxiliary_pane || prod_as_rail);
        if show_ground {
            if let Some(flourish) = flourish_area(transcript_root) {
                render_symbol_ground(frame, flourish, app);
            }
        }
        frame.render_widget(paragraph.scroll((scroll, 0)), transcript_root);
        if show_ground {
            render_empty_state(frame, transcript_root, app);
        }
    }
    if let Some(citation_area) = main_areas.get(1).copied() {
        if diff_as_rail {
            render_diff_panel(frame, citation_area, app);
        } else if prod_as_rail {
            render_prod_panel(frame, citation_area, app, now);
        } else if show_context_panel {
            render_context_panel(frame, citation_area, app);
        } else {
            let lines = app
                .citations
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:>2}  ", index + 1),
                            Style::default().fg(palette.dim),
                        ),
                        Span::styled(
                            source_label(source),
                            Style::default().fg(app.theme.primary()),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            let cited_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(citation_area);
            frame.render_widget(
                Paragraph::new(session_view::cited_rule(
                    app.repo.as_str(),
                    usize::from(cited_rows[0].width),
                    &palette,
                )),
                cited_rows[0],
            );
            frame.render_widget(
                Paragraph::new(lines)
                    .style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            palette.dim
                        }),
                    )
                    .wrap(Wrap { trim: false }),
                cited_rows[1],
            );
        }
    }
    // THE INPUT BAR, ROW FOR ROW, FROM THE DEMO FRAME. It was seven rows and every one of them
    // was wrong: no status row at all, the wrong prompt glyph, an "Ask Estelle" placeholder the
    // demo does not have, two blank rows the demo does not have, a "? for shortcuts" line the
    // demo does not have, and a hint line naming different keys. It is five rows now: the status
    // row, ONE blank, the ask rule, the bare prompt, and the hint line under it.
    let composer_area = if modal_owns_input {
        Rect::default()
    } else {
        let ask_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(rows[2]);
        frame.render_widget(
            Paragraph::new(status_bar_line(app, now, usize::from(ask_rows[0].width))),
            ask_rows[0],
        );
        // Row 2 is deliberately blank. The demo clumps the status row against the rule, and that
        // is the one place the founder is deliberately improving on the demo.
        frame.render_widget(
            Paragraph::new(session_view::ask_rule(
                app.repo.as_str(),
                usize::from(ask_rows[2].width),
                &palette,
            )),
            ask_rows[2],
        );
        app.composer.render_ref_with_background(
            ask_rows[3],
            frame.buffer_mut(),
            app.theme.background(),
        );
        collapse_composer_tail(frame, ask_rows[3], &palette, app.theme.background());
        ask_rows[3]
    };
    if let Some(picker) = &app.resume_picker {
        render_resume_picker(frame, picker, rows[1], app);
    } else if let Some(picker) = &app.picker {
        render_picker(frame, picker, rows[1], app);
    } else if let Some(modal) = &app.gate_modal {
        render_gate_modal(frame, modal, rows[1], app, now);
    } else if !app.boot_active(now)
        && app.focus == FocusSurface::Composer
        && let Some(position) = app.composer.cursor_pos(composer_area)
    {
        frame.set_cursor_position(position);
    }
    if let Some(boot) = &app.boot {
        let elapsed_ms = app.boot_elapsed_ms(now);
        if !boot.phase(elapsed_ms).is_finished() {
            boot.render(
                area,
                frame.buffer_mut(),
                elapsed_ms,
                app.theme.boot_palette(),
            );
        }
    }
}

pub(super) fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> io::Result<()> {
    let now = Instant::now();
    terminal.draw(|frame| render_frame(frame, app, now))?;
    Ok(())
}
