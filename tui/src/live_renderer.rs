//! The live Estelle terminal renderer.
//!
//! This module is the single owner of the customer-visible frame. Snapshot commands and tests
//! must enter through `render_frame`; a second hand-built representation is forbidden.

use super::*;
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
            Style::default().fg(Color::Gray),
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
            Style::default().fg(Color::Gray),
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
            Style::default().fg(Color::Gray),
        ));
    }
    Line::from(spans)
}

pub(super) fn session_tabs_line(app: &App) -> Line<'static> {
    if app.session_tabs.is_empty() {
        return Line::default();
    }
    let mut spans = vec![Span::styled(
        "SESSIONS  ",
        Style::default()
            .fg(app.theme.ghost())
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

pub(super) fn value_style(resolved: bool) -> Style {
    Style::default().fg(if resolved {
        Color::Gray
    } else {
        Color::DarkGray
    })
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

pub(super) fn status_line(app: &App, now: Instant) -> Line<'static> {
    if let Some(active) = &app.active {
        let elapsed = now.saturating_duration_since(active.started).as_secs();
        let local_shell = active.label.starts_with("local shell");
        let label = if elapsed >= 30 && active.label.starts_with("/gate ·") {
            format!("{} · still waiting for Estelle", active.label)
        } else if elapsed >= 30 && !local_shell {
            "still waiting for Estelle".to_string()
        } else {
            active.label.clone()
        };
        let mut spans = vec![
            Span::styled(label, Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "  {}  |  Esc cancels",
                estelle_tui::fmt_elapsed_compact(elapsed)
            )),
        ];
        if elapsed >= 30 {
            spans.push(Span::raw(if local_shell {
                "  |  local command has not exited"
            } else {
                "  |  no response received yet"
            }));
        }
        return Line::from(spans);
    }
    if !app.queue.is_empty() {
        return Line::styled(
            format!("{} queued", app.queue.len()),
            Style::default().fg(Color::Gray),
        );
    }
    let mode = commands::mode_name(commands::effective_mode(
        &app.local_mode,
        app.server_mode.as_deref(),
    ));
    let (model, model_resolved) = app.active_model.as_ref().map_or_else(
        || ("routing auto".to_string(), false),
        |model| {
            let freshness = if app
                .active_model_observed_at
                .is_some_and(|observed| now.saturating_duration_since(observed).as_secs() <= 300)
            {
                "observed"
            } else {
                "stale"
            };
            (format!("model {model} · {freshness}"), true)
        },
    );
    let mut spans = vec![
        Span::styled(mode.to_string(), Style::default().fg(Color::Gray)),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(model, value_style(model_resolved)),
    ];
    if let Some(count) = app.header.memories {
        spans.extend([
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("memory {}", commas(count)), value_style(true)),
        ]);
    }
    if app.header.connected {
        spans.extend([
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled("connected", value_style(true)),
        ]);
    }
    Line::from(spans)
}

/// The footer carries the design's key hints ahead of the live status.
///
/// ⚠️ `KEY_HINTS` is the catalog's screen-9 wording verbatim. The demo mockup shows
/// `enter send` and `esc stop` beside them; neither exists in the restored design code, so
/// neither is printed here.
pub(super) fn footer_line(app: &App, now: Instant, width: u16) -> Line<'static> {
    let status = status_line(app, now);
    let status_width = status
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum::<usize>();
    let budget = usize::from(width)
        .saturating_sub(status_width)
        .saturating_sub(5);
    let hints = session_view::key_hints(budget);
    let mut spans = Vec::new();
    if !hints.is_empty() {
        spans.extend([
            Span::styled(hints, Style::default().fg(app.theme.ghost())),
            Span::styled("  |  ", Style::default().fg(app.theme.ghost())),
        ]);
    }
    spans.extend(status.spans);
    Line::from(spans)
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
                            Style::default().fg(Color::Gray)
                        },
                    ),
                    Span::styled(
                        truncate_display(&row.detail, detail_width),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .chain(std::iter::once(Line::styled(
                "↑↓ navigate · 1-9 or Enter select · Esc close",
                Style::default().fg(Color::DarkGray),
            ))),
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(
                Style::default()
                    .fg(app.theme.primary())
                    .bg(app.theme.background()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(app.theme.primary()))
                    .title(format!(" {} ", picker.title.to_ascii_uppercase())),
            ),
        modal,
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
    frame.render_widget(
        Paragraph::new(lines)
            .style(
                Style::default()
                    .fg(app.theme.primary())
                    .bg(app.theme.background()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(app.theme.primary()))
                    .title(" RESUME A PREVIOUS SESSION "),
            ),
        modal,
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

pub(super) fn render_symbol_ground(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
                        Color::Black
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
                .map(|line| Line::styled(line.clone(), Style::default().fg(Color::Gray))),
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
        lines.push(Line::styled(identity, Style::default().fg(Color::Gray)));
        if let Some(team) = &account.team {
            let name = team.name.as_deref().unwrap_or(&team.id);
            let role = team.role.as_deref().unwrap_or("role not returned");
            lines.push(Line::styled(
                format!("Team · {name} · {role}"),
                Style::default().fg(Color::Gray),
            ));
        }
    }
    lines.extend([
        Line::default(),
        Line::from(vec![
            Span::styled("/review  ", Style::default().fg(app.theme.primary())),
            Span::styled("Read current changes", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("/sweep   ", Style::default().fg(app.theme.primary())),
            Span::styled(sweep, Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("?        ", Style::default().fg(app.theme.primary())),
            Span::styled("Show shortcuts", Style::default().fg(Color::Gray)),
        ]),
    ]);
    lines.truncate(usize::from(area.height));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(super) fn render_gate_modal(
    frame: &mut Frame<'_>,
    modal: &GateModal,
    content_area: Rect,
    app: &App,
) {
    let width = content_area.width.saturating_sub(4).min(86);
    let height = content_area.height.saturating_sub(2).min(18);
    let area = centered_rect(width, height, content_area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FATE_RED))
        .title(" gate · deterministic · no model ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 10 || inner.width < 48 {
        let total_lines = modal
            .files
            .iter()
            .map(|file| file.changed_lines)
            .sum::<u64>();
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "EDIT REFUSED",
                    Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD),
                ),
                Line::raw("Gate protected this repository. Nothing was written."),
                Line::raw(format!("Verdict  {}", modal.verdict)),
                Line::raw(format!(
                    "blast radius  {} files · {total_lines} changed lines",
                    modal.files.len()
                )),
                Line::raw(modal.reasons.join(" | ")),
                Line::styled(
                    "Enter or Esc closes · Ask Estelle",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new("EDIT REFUSED")
            .style(Style::default().fg(FATE_RED).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("Gate protected this repository. Nothing was written.")
            .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!("Verdict  {}", modal.verdict))
            .style(Style::default().fg(Color::Gray)),
        rows[2],
    );

    let total_lines = modal
        .files
        .iter()
        .map(|file| file.changed_lines)
        .sum::<u64>();
    let points = modal
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| (index as f64, file.changed_lines as f64))
        .collect::<Vec<_>>();
    let max_lines = modal
        .files
        .iter()
        .map(|file| file.changed_lines)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let x_max = modal.files.len().saturating_sub(1).max(1) as f64;
    let dataset = Dataset::default()
        .name("changed lines")
        .marker(Marker::Braille)
        .graph_type(GraphType::Scatter)
        .style(Style::default().fg(app.theme.primary()))
        .data(&points);
    frame.render_widget(
        Chart::new(vec![dataset])
            .block(Block::default().title(format!(
                " blast radius · {} files · {total_lines} changed lines ",
                modal.files.len()
            )))
            .x_axis(Axis::default().bounds([0.0, x_max]))
            .y_axis(Axis::default().bounds([0.0, max_lines])),
        rows[3],
    );

    let mut details = modal
        .files
        .iter()
        .map(|file| Line::from(format!("{:>6}  {}", file.changed_lines, file.path)))
        .collect::<Vec<_>>();
    details.extend(
        modal
            .reasons
            .iter()
            .map(|reason| Line::from(format!("blocked  {reason}"))),
    );
    frame.render_widget(
        Paragraph::new(details)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: false }),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new("Enter or Esc closes · Ask Estelle")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center),
        rows[5],
    );
}

pub(super) fn render_context_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
            Style::default().fg(Color::DarkGray),
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
                Style::default().fg(Color::DarkGray),
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
        Style::default().fg(Color::DarkGray),
    ));
    lines.push(Line::styled(
        "Not added to the team's Repo graph.",
        Style::default().fg(Color::DarkGray),
    ));
    if app.working_memory_paths.is_empty() {
        lines.push(Line::styled(
            "No eligible local files were attached to the last question.",
            Style::default().fg(Color::DarkGray),
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
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            app.theme.ghost()
                        }),
                    )
                    .title(" CONTEXT  Alt+M · /context "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
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

pub(super) fn production_workspace_lines(app: &App) -> Vec<Line<'static>> {
    let heading = |text: String| {
        Line::styled(
            text,
            Style::default()
                .fg(app.theme.primary())
                .add_modifier(Modifier::BOLD),
        )
    };
    let dim = |text: String| Line::styled(text, Style::default().fg(app.theme.ghost()));
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
        (Some(app_name), Some(org)) => format!("APP HEALTH · {org}/{app_name}"),
        (Some(app_name), None) => format!("APP HEALTH · {app_name}"),
        (None, _) => format!("APP HEALTH · repo {repo}"),
    };
    let mut lines = vec![heading(identity)];

    if !app.auth_resolved {
        lines.push(dim("Connecting to Estelle...".to_string()));
    } else if app.client.is_none() {
        lines.push(dim("Live Monitor unavailable.".to_string()));
        lines.push(dim("Run /login here.".to_string()));
    } else if let Some(error) = &app.prod_issue_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
        lines.push(dim("The client will retry in the background.".to_string()));
    } else if let Some(overview) = &app.prod_overview {
        let buckets = overview.error_buckets();
        if buckets.is_empty() {
            lines.push(dim("No measured error window was returned.".to_string()));
        } else {
            let errors = buckets.iter().map(|bucket| bucket.errors).sum::<u64>();
            let requests = buckets
                .iter()
                .filter_map(|bucket| bucket.requests)
                .sum::<u64>();
            let has_denominator = overview.requests_source() != Some("unavailable")
                && buckets.iter().all(|bucket| bucket.requests.is_some());
            lines.push(Line::from(format!(
                "error counts · {}  {errors}",
                error_count_sparkline(&buckets)
            )));
            if has_denominator {
                lines.push(Line::from(format!(
                    "measured · {errors}/{requests} requests"
                )));
            } else {
                lines.push(dim("request denominator unavailable".to_string()));
            }
        }
        if overview.uptime.checks == 0 {
            lines.push(dim(
                "No uptime checks · add one with POST /monitor/uptime.".to_string()
            ));
        } else {
            lines.push(Line::from(format!(
                "uptime checks · {}/{} up",
                overview.uptime.up, overview.uptime.checks
            )));
            if overview.uptime.down > 0 {
                lines.push(Line::styled(
                    format!("{} uptime check(s) down", overview.uptime.down),
                    Style::default().fg(app.theme.alert()),
                ));
            }
        }
    } else {
        lines.push(dim("Loading a real Monitor window...".to_string()));
    }

    lines.push(Line::from(""));
    lines.push(heading("AGENT HEALTH".to_string()));
    if let Some(error) = &app.prod_agent_health_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
        lines.push(dim("The client will retry in the background.".to_string()));
    } else if let Some(health) = &app.prod_agent_health {
        match health.enabled {
            Some(false) => lines.push(dim(
                "Agent telemetry not enabled · send POST /agent/events after enabling it."
                    .to_string(),
            )),
            None => lines.push(dim(format!(
                "Agent health unknown · {}",
                health
                    .enabled_absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason")
            ))),
            Some(true) => {
                if let Some(counts) = &health.counts {
                    let count = |value: Option<u64>, label: &str| match value {
                        Some(value) => format!("{value} {label}"),
                        None => format!("{label} unknown"),
                    };
                    lines.push(Line::from(format!(
                        "{} · {} · {}",
                        count(counts.reporting, "reporting"),
                        count(counts.degraded, "degraded"),
                        count(counts.silent, "silent")
                    )));
                } else {
                    lines.push(dim(
                        "Agent counts unavailable · server returned no measurement.".to_string(),
                    ));
                }
                match (health.observed_at, health.stale_after_s) {
                    (Some(observed_at), Some(stale_after_s)) => lines.push(dim(format!(
                        "observed {observed_at:.0} · stale threshold {stale_after_s}s"
                    ))),
                    _ => lines.push(dim("Snapshot freshness unavailable.".to_string())),
                }
                for agent in health.agents.iter().take(3) {
                    let state = match agent.state {
                        estelle_client::AgentHealthState::Healthy => "healthy",
                        estelle_client::AgentHealthState::Degraded => "degraded",
                        estelle_client::AgentHealthState::Silent => "silent",
                        estelle_client::AgentHealthState::Disabled => "disabled",
                        estelle_client::AgentHealthState::Unknown => "unknown",
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
                    lines.push(Line::from(format!(
                        "{state} {} · {events} · {signal}",
                        agent.id
                    )));
                    if let Some(last_seen) = agent.last_seen {
                        lines.push(dim(format!("       last seen {last_seen:.0}")));
                    }
                }
                if health.agents.len() > 3 {
                    lines.push(dim(format!("+{} more agents", health.agents.len() - 3)));
                }
            }
        }
    } else {
        lines.push(dim(
            "State unavailable · no read contract · send POST /agent/events.".to_string(),
        ));
    }

    lines.push(Line::from(""));
    lines.push(heading("ESTELLE STATUS".to_string()));
    match app.prod_issues.as_ref() {
        Some(response) => {
            let unresolved = response
                .issues
                .iter()
                .filter(|issue| issue.status != "resolved")
                .collect::<Vec<_>>();
            if !unresolved.is_empty() {
                for issue in unresolved.iter().take(2) {
                    let events = issue.event_count();
                    let location = issue
                        .bound_location()
                        .map(|(file, line)| format!("{file}:{line}"))
                        .unwrap_or_else(|| "unbound · reason not recorded".to_string());
                    lines.push(Line::from(format!("caught · {}", issue_title(issue))));
                    lines.push(dim(format!(
                        "grouped · {events} event(s) · traced to · {location}"
                    )));
                    if let Some(range) = &issue.symbol_range
                        && range.line_end > range.line_start
                    {
                        lines.push(dim(format!(
                            "       range {}:{}-{}",
                            range.file, range.line_start, range.line_end
                        )));
                    }
                }
                if unresolved.len() > 2 {
                    lines.push(dim(format!(
                        "+{} more · open /monitor issues",
                        unresolved.len() - 2
                    )));
                }
            } else {
                lines.push(dim("No errors have reached Estelle yet.".to_string()));
                lines.push(dim(
                    "Point OTLP or Sentry at api.fatelabs.ca/monitor/ingest.".to_string(),
                ));
            }
        }
        None => lines.push(dim("Waiting for the live issue feed...".to_string())),
    }

    lines.push(Line::from(""));
    lines.push(heading("ESTELLE QUEUE".to_string()));
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
    if queued.is_empty() {
        lines.push(dim("Queue empty · no repair work is reported.".to_string()));
        lines.push(dim("Issue selection: /monitor issues".to_string()));
    } else {
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
            lines.push(Line::from(format!(
                "{label} · {} · {destination}",
                issue_title(issue)
            )));
            if let Some(verdict) = issue.effective_gate_verdict() {
                lines.push(dim(format!("       gate · {verdict}")));
            } else if let Some(reason) = issue
                .gate_absent_reason
                .as_deref()
                .filter(|reason| !reason.trim().is_empty())
            {
                lines.push(dim(format!("       gate absent · {reason}")));
            }
            if let Some(patch) = issue.effective_repair_patch() {
                let short_sha = patch.base_sha.chars().take(12).collect::<String>();
                lines.push(dim(format!(
                    "       patch · {} · base {short_sha}",
                    patch.format
                )));
                lines.extend(github_diff_lines(&patch.text, 96, app));
            } else {
                let reason = issue
                    .effective_patch_absent_reason()
                    .unwrap_or("unavailable");
                lines.push(dim(format!("       diff unavailable - {reason}")));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(heading("GITHUB".to_string()));
    if let Some(error) = &app.prod_github_status_error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(app.theme.alert()),
        ));
    } else if let Some(status) = &app.prod_github_status {
        match status.connected {
            Some(true) => {
                let identity = status
                    .login
                    .as_deref()
                    .filter(|login| !login.trim().is_empty())
                    .map(|login| format!(" · @{login}"))
                    .unwrap_or_default();
                lines.push(Line::from(format!("Connected{identity}")));
                if let Some(observed_at) = status.observed_at {
                    lines.push(dim(format!("binding observed {observed_at:.0}")));
                }
            }
            Some(false) => {
                lines.push(dim(
                    "Not connected · run estelle github connect.".to_string()
                ));
                lines.push(dim(
                    "Proposed PRs are not read without a measured App binding.".to_string(),
                ));
            }
            None => {
                let reason = status
                    .absent_reason
                    .as_deref()
                    .filter(|reason| !reason.trim().is_empty())
                    .unwrap_or("server returned no reason");
                lines.push(dim(format!("Connection unknown · {reason}")));
                lines.push(dim("Proposed PR state is not inferred.".to_string()));
            }
        }
    } else {
        lines.push(dim(
            "Waiting for measured GitHub connection state...".to_string()
        ));
    }

    if app
        .prod_github_status
        .as_ref()
        .and_then(|status| status.connected)
        == Some(true)
    {
        if let Some(error) = &app.prod_proposed_prs_error {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(app.theme.alert()),
            ));
        } else if let Some(response) = &app.prod_proposed_prs {
            if response.prs.is_empty() {
                lines.push(dim("No open Estelle-proposed PRs returned.".to_string()));
            }
            for pr in response.prs.iter().take(3) {
                let title = if pr.title.trim().is_empty() {
                    "untitled PR"
                } else {
                    pr.title.as_str()
                };
                lines.push(Line::from(format!("#{} · {title}", pr.number)));
                lines.push(dim(format!("       {}", pr.url)));
                if let Some(gate) = &pr.gate {
                    let verified = if gate.verified { " · verified" } else { "" };
                    lines.push(dim(format!(
                        "       gate · {} · {} · {} blocker(s){verified}",
                        gate.state, gate.verdict, gate.blockers
                    )));
                } else {
                    let reason = pr
                        .gate_absent_reason
                        .as_deref()
                        .filter(|reason| !reason.trim().is_empty())
                        .unwrap_or("server returned no reason");
                    lines.push(dim(format!("       gate absent · {reason}")));
                }
                if !pr.updated_at.trim().is_empty() {
                    lines.push(dim(format!("       updated {}", pr.updated_at)));
                }
            }
            if response.has_more {
                lines.push(dim(
                    "More open proposed PRs exist than this page shows.".to_string()
                ));
            }
        } else {
            lines.push(dim("Waiting for the proposed-PR feed...".to_string()));
        }
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
    let lines = if let Some(graph) = &app.prod_graph {
        let palette = match app.theme {
            Theme::Dark => theme::ScreenTheme::Dark.palette(),
            Theme::CreamInk => theme::ScreenTheme::Cream.palette(),
        };
        let tick = now
            .saturating_duration_since(app.boot_started)
            .as_millis()
            .checked_div(50)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        production_hud::lines(graph, &palette, tick, true)
    } else {
        let mut lines = production_workspace_lines(app);
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
    let palette = app.theme.screen_palette();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
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
    let mut lines = vec![Line::styled(
        "read-only · /apply submits this exact patch",
        Style::default().fg(Color::DarkGray),
    )];
    if let Some(diff) = app.last_diff.as_deref() {
        lines.extend(github_diff_lines(
            diff,
            usize::from(area.width.saturating_sub(2)),
            app,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(if app.focus == FocusSurface::Auxiliary {
                            app.theme.primary()
                        } else {
                            app.theme.ghost()
                        }),
                    )
                    .title(" WORK DRAFT · /work · READ ONLY "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
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
                Style::default().fg(Color::Cyan),
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
                        Color::Green
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
                    Style::default().fg(Color::DarkGray),
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
            Constraint::Length(composer_height),
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
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(main_areas[0]);
        frame.render_widget(
            Paragraph::new(session_view::session_rule(
                app.repo.as_str(),
                usize::from(session_rows[0].width),
                &palette,
            )),
            session_rows[0],
        );
        let primary_area = session_rows[1];

        let transcript_band = if let Some(fleet) = &app.fleet {
            let raw_lines = commands::fleet_view_lines(fleet, primary_area.width);
            let wanted = u16::try_from(raw_lines.len()).unwrap_or(u16::MAX);
            let fleet_height = wanted.min(primary_area.height.saturating_sub(1));
            let fleet_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(fleet_height), Constraint::Min(1)])
                .split(primary_area);
            let last = raw_lines.len().saturating_sub(1);
            let lines = raw_lines
                .into_iter()
                .enumerate()
                .map(|(index, line)| {
                    if index == last {
                        styled_fleet_progress_line(line)
                    } else {
                        styled_fleet_agent_line(line)
                    }
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().fg(Color::Gray)),
                fleet_rows[0],
            );
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
                                .fg(Color::Green)
                                .add_modifier(Modifier::CROSSED_OUT)
                        } else if line.starts_with("● ") {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else if line == "Todo" {
                            Style::default()
                                .fg(app.theme.primary())
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
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
                    Span::styled(progress.line(now), Style::default().fg(Color::Gray)),
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
                    Span::styled(progress.line(), Style::default().fg(Color::Gray)),
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
            render_symbol_ground(frame, transcript_root, app);
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
                            Style::default().fg(Color::DarkGray),
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
                            Color::DarkGray
                        }),
                    )
                    .wrap(Wrap { trim: false }),
                cited_rows[1],
            );
        }
    }
    // "┌ ASK ESTELLE ─┐" becomes the design's `╌╌ ask · <repo> ╌╌` rule with a bare prompt
    // under it. The composer keeps its own behaviour; only its framing changes.
    let composer_area = if modal_owns_input {
        Rect::default()
    } else {
        let ask_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(rows[2]);
        frame.render_widget(
            Paragraph::new(session_view::ask_rule(
                app.repo.as_str(),
                usize::from(ask_rows[0].width),
                &palette,
            )),
            ask_rows[0],
        );
        app.composer.render_ref_with_background(
            ask_rows[1],
            frame.buffer_mut(),
            app.theme.background(),
        );
        ask_rows[1]
    };
    frame.render_widget(
        Paragraph::new(footer_line(app, now, rows[3].width)),
        rows[3],
    );
    if let Some(picker) = &app.resume_picker {
        render_resume_picker(frame, picker, rows[1], app);
    } else if let Some(picker) = &app.picker {
        render_picker(frame, picker, rows[1], app);
    } else if let Some(modal) = &app.gate_modal {
        render_gate_modal(frame, modal, rows[1], app);
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

pub(super) fn styled_fleet_progress_line(line: String) -> Line<'static> {
    let Some(open) = line.find('[') else {
        return Line::from(line);
    };
    let Some(relative_close) = line[open..].find(']') else {
        return Line::from(line);
    };
    let close = open + relative_close;
    let prefix = line[..open].to_string();
    let bar = &line[open + 1..close];
    let boundary = bar.find('─').unwrap_or(bar.len());
    let completed = bar[..boundary].to_string();
    let remaining = bar[boundary..].to_string();
    let suffix = line[close + 1..].to_string();
    Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Cyan)),
        Span::styled("[", Style::default().fg(Color::Gray)),
        Span::styled(completed, Style::default().fg(Color::Green)),
        Span::styled(remaining, Style::default().fg(Color::Blue)),
        Span::styled(format!("]{suffix}"), Style::default().fg(Color::Gray)),
    ])
}

pub(super) fn styled_fleet_agent_line(line: String) -> Line<'static> {
    let markers = [
        ("✓ ", Color::Green),
        ("× ", Color::Red),
        ("◷ ", Color::Yellow),
        ("■ ", Color::Magenta),
        ("? ", Color::Cyan),
    ];
    let mut spans = Vec::new();
    let mut remaining = line.as_str();
    while let Some((offset, marker, colour)) = markers
        .iter()
        .filter_map(|(marker, colour)| {
            remaining
                .find(marker)
                .map(|offset| (offset, *marker, *colour))
        })
        .min_by_key(|(offset, _, _)| *offset)
    {
        if offset > 0 {
            spans.push(Span::styled(
                remaining[..offset].to_string(),
                Style::default().fg(Color::Gray),
            ));
        }
        spans.push(Span::styled(
            marker.to_string(),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ));
        remaining = &remaining[offset + marker.len()..];
    }
    if !remaining.is_empty() {
        spans.push(Span::styled(
            remaining.to_string(),
            Style::default().fg(Color::Gray),
        ));
    }
    Line::from(spans)
}

pub(super) fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> io::Result<()> {
    let now = Instant::now();
    terminal.draw(|frame| render_frame(frame, app, now))?;
    Ok(())
}
