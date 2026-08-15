// This test-only composition target specifies Estelle's named inks exactly. The
// product renderer uses terminal-safe colours; the fixture must remain stable.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Gauge;
use ratatui::widgets::Paragraph;

// The reference terminal is intentionally wide: five swarm columns remain readable beside
// the persistent context pane instead of collapsing into status-only cells.
const WIDTH: u16 = 200;
const HEIGHT: u16 = 42;

const BG: Color = Color::Rgb(16, 16, 16);
const CREAM: Color = Color::Rgb(233, 230, 220);
const GHOST: Color = Color::Rgb(112, 116, 120);
const BLUE: Color = Color::Rgb(101, 168, 255);
const CYAN: Color = Color::Rgb(112, 198, 204);
const GREEN: Color = Color::Rgb(103, 211, 145);
const RED: Color = Color::Rgb(226, 91, 85);
const GOLD: Color = Color::Rgb(228, 188, 93);

const REQUIRED_FRAMES: [(&str, &str); 10] = [
    ("01-startup-home", "Ask Estelle"),
    ("02-orchestra-active", "Estelle Orchestra"),
    ("03-orchestra-completed", "completed"),
    ("04-production-issues", "What Estelle caught"),
    ("05-proposed-diff", "Proposed repair"),
    ("06-slash-palette", "/context"),
    ("07-settings", "Settings"),
    ("08-model-picker", "Select a model"),
    ("09-todo-expanded", "Ctrl+T collapse"),
    ("10-todo-collapsed", "Ctrl+T expand"),
];

#[derive(Clone, Copy, Debug)]
enum Scene {
    Startup,
    OrchestraActive,
    OrchestraCompleted,
    ProductionIssues,
    ProposedDiff,
    SlashPalette,
    Settings,
    ModelPicker,
    TodoExpanded,
    TodoCollapsed,
}

#[derive(Debug)]
struct GalleryFrame {
    text: String,
    svg: String,
}

fn gallery_frames() -> BTreeMap<&'static str, GalleryFrame> {
    [
        ("01-startup-home", Scene::Startup),
        ("02-orchestra-active", Scene::OrchestraActive),
        ("03-orchestra-completed", Scene::OrchestraCompleted),
        ("04-production-issues", Scene::ProductionIssues),
        ("05-proposed-diff", Scene::ProposedDiff),
        ("06-slash-palette", Scene::SlashPalette),
        ("07-settings", Scene::Settings),
        ("08-model-picker", Scene::ModelPicker),
        ("09-todo-expanded", Scene::TodoExpanded),
        ("10-todo-collapsed", Scene::TodoCollapsed),
    ]
    .into_iter()
    .map(|(name, scene)| (name, render_gallery_frame(scene)))
    .collect()
}

fn render_gallery_frame(scene: Scene) -> GalleryFrame {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("gallery terminal");
    terminal
        .draw(|frame| render_scene(frame, scene))
        .expect("gallery render");
    let buffer = terminal.backend().buffer();
    GalleryFrame {
        text: buffer_text(buffer),
        svg: buffer_svg(buffer),
    }
}

fn render_scene(frame: &mut Frame<'_>, scene: Scene) {
    frame.render_widget(
        Block::new().style(Style::default().bg(BG).fg(CREAM)),
        frame.area(),
    );
    if matches!(
        scene,
        Scene::Startup | Scene::OrchestraActive | Scene::OrchestraCompleted
    ) {
        render_dither(frame, frame.area());
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(30),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(frame.area());
    render_header(frame, rows[0], scene);
    match scene {
        Scene::Startup => render_startup(frame, rows[1]),
        Scene::OrchestraActive => render_orchestra(frame, rows[1], false),
        Scene::OrchestraCompleted => render_orchestra(frame, rows[1], true),
        Scene::ProductionIssues => render_production_issues(frame, rows[1]),
        Scene::ProposedDiff => render_diff(frame, rows[1]),
        Scene::SlashPalette => render_slash_palette(frame, rows[1]),
        Scene::Settings => render_settings(frame, rows[1]),
        Scene::ModelPicker => render_model_picker(frame, rows[1]),
        Scene::TodoExpanded => render_todo(frame, rows[1], false),
        Scene::TodoCollapsed => render_todo(frame, rows[1], true),
    }
    render_composer(frame, rows[2], scene);
    render_status(frame, rows[3], scene);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, scene: Scene) {
    let mode = match scene {
        Scene::OrchestraActive | Scene::OrchestraCompleted => "ORCHESTRA",
        Scene::ProductionIssues => "PRODUCTION",
        Scene::ProposedDiff => "REVIEW",
        _ => "SESSION",
    };
    let line = Line::from(vec![
        Span::styled(" ESTELLE ", Style::default().fg(BG).bg(CREAM).bold()),
        Span::styled(format!("  {mode}  "), Style::default().fg(BLUE).bold()),
        Span::styled("fatelabs/estelle", Style::default().fg(CREAM)),
        Span::styled(
            "  ·  main  ·  repo graph current",
            Style::default().fg(GHOST),
        ),
    ]);
    let fixture = Line::from(Span::styled(
        " DESIGN FIXTURE · NOT LIVE DATA · deterministic Ratatui TestBackend render",
        Style::default().fg(GOLD),
    ));
    frame.render_widget(
        Paragraph::new(Text::from(vec![line, fixture])).style(Style::default().bg(BG)),
        area,
    );
}

fn render_dither(frame: &mut Frame<'_>, area: Rect) {
    let marks = ["0", "1", "·", "·", "·", "×", "·", "·"];
    // A terminal cannot truly z-index glyphs. Keep the field in unused ground cells so it
    // remains texture behind the working surface and never replaces whitespace in prose.
    for y in area.y.saturating_add(23)..area.height.saturating_sub(5) {
        for x in 0..area.width {
            let sample = usize::from(x.wrapping_mul(5) + y.wrapping_mul(3)) % 47;
            if sample == 0 {
                let symbol = marks[usize::from((x + y) % marks.len() as u16)];
                frame.buffer_mut().set_string(
                    x,
                    y,
                    symbol,
                    Style::default().fg(Color::Rgb(36, 38, 38)).bg(BG),
                );
            }
        }
    }
}

fn render_startup(frame: &mut Frame<'_>, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);

    let content = Text::from(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Ask Estelle",
            Style::default().fg(CREAM).bold(),
        )),
        Line::from(Span::styled(
            "Grounded answers from the code you have, including work not pushed yet.",
            Style::default().fg(GHOST),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("1,993", Style::default().fg(CREAM).bold()),
            Span::styled(" files   ", Style::default().fg(GHOST)),
            Span::styled("13,757", Style::default().fg(CREAM).bold()),
            Span::styled(" chunks   ", Style::default().fg(GHOST)),
            Span::styled("30,748", Style::default().fg(CREAM).bold()),
            Span::styled(" memories", Style::default().fg(GHOST)),
        ]),
        Line::from(""),
        action_line("›", "Ask about the indexed repo", "Where is auth enforced?"),
        action_line("›", "Review current changes", "/review"),
        action_line("›", "Sweep another repository", "/sweep"),
        action_line("›", "See every command", "/help"),
        Line::from(""),
        Line::from(Span::styled(
            "Working memory is session-private · repo graph is shared with your team",
            Style::default().fg(GHOST),
        )),
    ]);
    frame.render_widget(Paragraph::new(content), inset(columns[0], 3, 1));

    let side = Text::from(vec![
        section_title("Context"),
        kv("Repository", "fatelabs/estelle"),
        kv("Branch", "main"),
        kv("Files in play", "none yet"),
        Line::from(""),
        section_title("Production"),
        Line::from(Span::styled("● calm", Style::default().fg(GREEN))),
        Line::from(Span::styled(
            "No live gate refusals",
            Style::default().fg(GHOST),
        )),
        Line::from(""),
        section_title("Connections"),
        Line::from(Span::styled(
            "GitHub  connected",
            Style::default().fg(CREAM),
        )),
        Line::from(Span::styled("MCP     11 tools", Style::default().fg(CREAM))),
    ]);
    frame.render_widget(
        panel("SESSION CONTEXT").style(Style::default().fg(GHOST)),
        columns[1],
    );
    frame.render_widget(Paragraph::new(side), inset(columns[1], 2, 2));
}

fn render_orchestra(frame: &mut Frame<'_>, area: Rect, completed: bool) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(76), Constraint::Percentage(24)])
        .split(area);
    let main = inset(columns[0], 2, 1);
    let status = if completed {
        "8 participants finished · 8 completed · 0 failed · $14.62 measured"
    } else {
        "8 participant slots selected · batch purpose grounded · estimated ceiling $180"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "● ",
                Style::default().fg(if completed { GREEN } else { CREAM }),
            ),
            Span::styled(status, Style::default().fg(CREAM)),
        ])),
        Rect::new(main.x, main.y, main.width, 1),
    );

    let title = if completed {
        "Estelle Orchestra — Ground checkout failures — completed x8"
    } else {
        "Estelle Orchestra — Ground checkout failures and prepare review evidence x8"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("— ", Style::default().fg(BLUE)),
            Span::styled(title, Style::default().fg(CYAN).bold()),
            Span::styled(" ".repeat(8), Style::default()),
            Span::styled("────────────────────────", Style::default().fg(BLUE)),
        ])),
        Rect::new(main.x, main.y + 2, main.width, 1),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Participants  ", Style::default().fg(GHOST)),
            Span::styled("GPT-5 ×2", Style::default().fg(CREAM)),
            Span::styled(" · ", Style::default().fg(GHOST)),
            Span::styled("Claude Opus 4.1 ×2", Style::default().fg(CREAM)),
            Span::styled(" · ", Style::default().fg(GHOST)),
            Span::styled("Gemini 2.5 Pro ×2", Style::default().fg(CREAM)),
            Span::styled(" · ", Style::default().fg(GHOST)),
            Span::styled("Grok 4 ×2", Style::default().fg(CREAM)),
        ])),
        Rect::new(main.x, main.y + 3, main.width, 1),
    );

    let actions = [
        "[GPT-5] ground checkout",
        "[Opus] trace billing",
        "[Gemini] inspect gate",
        "[Grok] review repair",
        "[GPT-5] compare memory",
        "[Opus] check GitHub",
        "[Gemini] verify sandbox",
        "[Grok] summarize sources",
    ];
    let terminal = [
        "[GPT-5] completed · 4 files",
        "[Opus] completed · charge.ts:52",
        "[Gemini] completed · abstained",
        "[Grok] completed · tests green",
        "[GPT-5] completed · disclosed",
        "[Opus] completed · PR #184",
        "[Gemini] completed · passed",
        "[Grok] completed · 11 citations",
    ];
    let grid = Rect::new(main.x, main.y + 5, main.width, 7);
    let cell_width = grid.width / 5;
    for index in 0..8_u16 {
        let row = index / 5;
        let col = index % 5;
        let cell = Rect::new(
            grid.x + col * cell_width,
            grid.y + row * 2,
            cell_width.saturating_sub(1),
            1,
        );
        render_agent_cell(
            frame,
            cell,
            usize::from(index),
            if completed {
                terminal[usize::from(index)]
            } else {
                actions[usize::from(index)]
            },
            completed,
        );
    }

    let ratio = if completed { 1.0 } else { 0.38 };
    let gauge = Gauge::default()
        .label(if completed {
            "completed 8/8"
        } else {
            "◐ Working…  3/8 measured"
        })
        .ratio(ratio)
        .gauge_style(
            Style::default()
                .fg(if completed { GREEN } else { BLUE })
                .bg(Color::Rgb(45, 47, 48)),
        );
    frame.render_widget(gauge, Rect::new(main.x, main.y + 10, main.width, 1));

    let log_lines = if completed {
        vec![
            section_title("Human summary"),
            Line::from(Span::styled(
                "✓ 8 grounded outcomes",
                Style::default().fg(GREEN),
            )),
            Line::from(Span::styled(
                "No result inferred from elapsed time.",
                Style::default().fg(GHOST),
            )),
            Line::from(""),
            section_title("Next action"),
            Line::from("Open PR #184 for review"),
        ]
    } else {
        vec![
            section_title("Live narrator"),
            Line::from("Agents 001–008 are reading the selected account slices."),
            Line::from(Span::styled("Observed 12s ago", Style::default().fg(GHOST))),
            Line::from(""),
            section_title("Todo"),
            Line::from(Span::styled(
                "✓ Select grounded tasks",
                Style::default().fg(GREEN),
            )),
            Line::from(Span::styled(
                "● Run Estelle Orchestra",
                Style::default().fg(BLUE),
            )),
            Line::from(Span::styled("○ Score evidence", Style::default().fg(GHOST))),
        ]
    };
    frame.render_widget(
        Paragraph::new(log_lines),
        Rect::new(main.x, main.y + 14, main.width, 10),
    );

    render_context_panel(frame, columns[1], completed);
}

fn render_agent_cell(frame: &mut Frame<'_>, area: Rect, index: usize, text: &str, completed: bool) {
    let filled = if completed { 8 } else { (index % 5 + 1).min(8) };
    let bar = format!("{}{}", "∷".repeat(filled), "·".repeat(8 - filled));
    let glyph = if completed { "✓" } else { "" };
    let line = Line::from(vec![
        Span::styled(format!("{:03} ", index + 1), Style::default().fg(BLUE)),
        Span::styled(
            format!("[{bar}] "),
            Style::default().fg(if completed { GREEN } else { GHOST }),
        ),
        Span::styled(
            format!("{glyph} {text}"),
            Style::default().fg(if completed { GREEN } else { GHOST }),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_context_panel(frame: &mut Frame<'_>, area: Rect, completed: bool) {
    frame.render_widget(panel("CONTEXT  Alt+M · /context"), area);
    let body = Text::from(vec![
        section_title("Grounding"),
        kv("Files", "7"),
        kv("Symbols", "12 resolved"),
        kv("Working memory", "3 local notes"),
        Line::from(""),
        section_title("Fleet"),
        kv("Lifecycle", if completed { "terminal" } else { "running" }),
        kv("Observed", "12s ago"),
        kv("Source", "/orchestra/run"),
        Line::from(""),
        section_title("Cost"),
        kv("Spent", if completed { "$14.62" } else { "$6.08" }),
        kv("Ceiling", "$180.00"),
        Line::from(""),
        section_title("Memory"),
        Line::from(Span::styled(
            "LOCAL differs from team graph",
            Style::default().fg(GOLD),
        )),
    ]);
    frame.render_widget(Paragraph::new(body), inset(area, 2, 2));
}

fn render_production_issues(frame: &mut Frame<'_>, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(area);
    let rows = vec![
        section_title("Production health"),
        Line::from(vec![
            Span::styled("● calm  ", Style::default().fg(GREEN)),
            Span::raw("0 error counts · 14,029 requests · 99.98% uptime"),
        ]),
        Line::from(""),
        section_title("What Estelle caught"),
        issue_line("E-104", "checkout failures", "api/charge.ts:52", "FLAGGED"),
        issue_line("E-103", "invoice retries", "billing/retry.ts:88", "PASSED"),
        issue_line("E-099", "login latency", "symbol unbound", "ABSTAINED"),
        Line::from(""),
        section_title("What Estelle did about it"),
        Line::from(vec![
            Span::styled("PR #184  ", Style::default().fg(BLUE)),
            Span::raw("Drafted repair awaiting human review"),
        ]),
        Line::from(vec![
            Span::styled("Gate      ", Style::default().fg(GHOST)),
            Span::raw("propose-only · no auto-merge claimed"),
        ]),
        Line::from(vec![
            Span::styled("Sandbox   ", Style::default().fg(GHOST)),
            Span::raw("passed · a clone, never production"),
        ]),
        Line::from(""),
        section_title("GitHub"),
        Line::from("Connected · 2 pull requests awaiting review"),
    ];
    frame.render_widget(Paragraph::new(rows), inset(columns[0], 3, 1));

    frame.render_widget(panel("ISSUE E-104"), columns[1]);
    let detail = Text::from(vec![
        section_title("Signal"),
        Line::from("17 failures / 2,091 requests"),
        Line::from(Span::styled(
            "Measured rate 0.81%",
            Style::default().fg(RED),
        )),
        Line::from(""),
        section_title("Bound symbol"),
        Line::from(Span::styled("api/charge.ts:52", Style::default().fg(CYAN))),
        kv("Bind status", "verified"),
        Line::from(""),
        section_title("Repair"),
        kv("State", "awaiting review"),
        kv("PR", "#184"),
        Line::from(""),
        section_title("Gate"),
        Line::from(Span::styled("FLAGGED", Style::default().fg(RED).bold())),
        Line::from("Ungrounded fallback removed"),
    ]);
    frame.render_widget(Paragraph::new(detail), inset(columns[1], 2, 2));
}

fn render_diff(frame: &mut Frame<'_>, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(area);
    let diff = Text::from(vec![
        section_title("Proposed repair  ·  api/charge.ts  ·  PR #184"),
        Line::from(Span::styled(
            "@@ chargeCustomer(request) @@",
            Style::default().fg(CYAN),
        )),
        Line::from("  const account = request.account"),
        Line::from(Span::styled(
            "- return gateway.charge(account)",
            Style::default().fg(RED),
        )),
        Line::from(Span::styled(
            "+ if (!account.billingEnabled) {",
            Style::default().fg(GREEN),
        )),
        Line::from(Span::styled(
            "+   return { status: 'declined', reason: 'billing disabled' }",
            Style::default().fg(GREEN),
        )),
        Line::from(Span::styled("+ }", Style::default().fg(GREEN))),
        Line::from(Span::styled(
            "+ return gateway.charge(account)",
            Style::default().fg(GREEN),
        )),
        Line::from(""),
        section_title("Tests"),
        Line::from(vec![
            Span::styled("✓ ", Style::default().fg(GREEN)),
            Span::raw("declines when billing is disabled"),
        ]),
        Line::from(vec![
            Span::styled("✓ ", Style::default().fg(GREEN)),
            Span::raw("retains grounded repo on scoped call"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Red before green: assertion failed at charge.test.ts:117",
            Style::default().fg(GHOST),
        )),
    ]);
    frame.render_widget(Paragraph::new(diff), inset(columns[0], 3, 1));
    frame.render_widget(panel("REVIEW"), columns[1]);
    let review = Text::from(vec![
        section_title("Evidence"),
        kv("Signal", "E-104"),
        kv("Symbol", "api/charge.ts:52"),
        kv("Sandbox", "passed"),
        kv("Gate", "FLAGGED"),
        Line::from(""),
        section_title("Decision"),
        Line::from(Span::styled("[a] Approve", Style::default().fg(GREEN))),
        Line::from(Span::styled("[e] Edit", Style::default().fg(CREAM))),
        Line::from(Span::styled("[r] Reject", Style::default().fg(RED))),
        Line::from(""),
        Line::from(Span::styled(
            "No automatic merge",
            Style::default().fg(GHOST),
        )),
    ]);
    frame.render_widget(Paragraph::new(review), inset(columns[1], 2, 2));
}

fn render_slash_palette(frame: &mut Frame<'_>, area: Rect) {
    render_dim_transcript(frame, area);
    let popup = Rect::new(area.x + 3, area.y + 9, area.width - 6, 20);
    frame.render_widget(
        Block::new()
            .style(Style::default().bg(BG))
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(BLUE)),
        popup,
    );
    let commands = Text::from(vec![
        Line::from(vec![
            Span::styled("Slash commands", Style::default().fg(BLUE).bold()),
            Span::styled("  type to filter", Style::default().fg(GHOST)),
        ]),
        Line::from(Span::styled(
            "↑↓ navigate  ·  Enter select  ·  Esc cancel",
            Style::default().fg(GHOST),
        )),
        Line::from(""),
        selected("/context", "Open the grounding side panel"),
        command("/orchestra", "Inspect server-owned agent runs"),
        command("/issues", "Open production health and caught issues"),
        command("/review", "Review Estelle's proposed repair"),
        command("/model", "Show account routing and available models"),
        command("/settings", "Open terminal preferences"),
        command("/skills", "Browse the server skill catalogue"),
        command("/help", "Show every command and key binding"),
    ]);
    frame.render_widget(
        Paragraph::new(commands).style(Style::default().bg(BG)),
        inset(popup, 2, 1),
    );
}

fn render_settings(frame: &mut Frame<'_>, area: Rect) {
    render_dim_transcript(frame, area);
    let popup = Rect::new(area.x + 3, area.y + 6, area.width - 6, 23);
    frame.render_widget(panel("Settings"), popup);
    let content = Text::from(vec![
        Line::from(Span::styled(
            "↑↓ navigate  ·  Enter select  ·  Esc cancel",
            Style::default().fg(GHOST),
        )),
        Line::from(""),
        setting("Model", "Automatic · strongest available", false),
        setting("Permission", "Ask before tools change customer state", true),
        setting("Theme", "Estelle dark · cream ink", false),
        setting("Editor", "$EDITOR", false),
        setting("Effects", "Subtle · reduced motion respected", false),
        setting("Automatic updates", "On", false),
        setting("Usage", "Session tokens and context window", false),
        setting("Providers", "3 connected · BYOK", false),
        setting("MCP", "11 tools discovered", false),
        setting("ACP", "protocol-v1 · session/new available", false),
    ]);
    frame.render_widget(
        Paragraph::new(content).style(Style::default().bg(BG)),
        inset(popup, 2, 2),
    );
}

fn render_model_picker(frame: &mut Frame<'_>, area: Rect) {
    render_dim_transcript(frame, area);
    let popup = Rect::new(area.x + 3, area.y + 7, area.width - 6, 22);
    frame.render_widget(panel("Select a model  ·  type to search"), popup);
    let content = Text::from(vec![
        Line::from(Span::styled(
            "Tab providers  ·  ↑↓ navigate  ·  Enter select  ·  Alt+S session-only",
            Style::default().fg(GHOST),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("All", Style::default().fg(BG).bg(BLUE).bold()),
            Span::raw("   Anthropic   Google   OpenAI   xAI"),
        ]),
        Line::from(""),
        model("Claude Opus 4.1", "Anthropic", "strongest", false),
        model("Gemini 2.5 Pro", "Google", "1M context", false),
        model("GPT-5", "OpenAI", "current route", true),
        model("Grok 4", "xAI", "fast", false),
        Line::from(""),
        section_title("Routing"),
        Line::from("Plan mode uses the strongest model in your BYOK pool."),
        Line::from(Span::styled(
            "Session pin unavailable · account route remains active",
            Style::default().fg(GOLD),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(content).style(Style::default().bg(BG)),
        inset(popup, 2, 2),
    );
}

fn render_todo(frame: &mut Frame<'_>, area: Rect, collapsed: bool) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(74), Constraint::Percentage(26)])
        .split(area);
    let command = if collapsed {
        "Todo · Ctrl+T expand"
    } else {
        "Todo · Ctrl+T collapse"
    };
    frame.render_widget(panel(command), columns[0]);

    let content = if collapsed {
        Text::from(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("✓ 2 completed", Style::default().fg(GREEN)),
                Span::styled("  ·  ", Style::default().fg(GHOST)),
                Span::styled("● 1 in progress", Style::default().fg(BLUE)),
                Span::styled("  ·  ", Style::default().fg(GHOST)),
                Span::styled("○ 2 queued", Style::default().fg(GHOST)),
            ]),
            Line::from(""),
            section_title("Completed results retained"),
            Line::from(vec![
                Span::styled("✓ Ground checkout failures", Style::default().fg(GREEN)),
                Span::styled(
                    "  result: 7 files · 12 symbols resolved",
                    Style::default().fg(CREAM),
                ),
            ]),
            Line::from(vec![
                Span::styled("✓ Compare working memory", Style::default().fg(GREEN)),
                Span::styled(
                    "  result: local disagreement disclosed",
                    Style::default().fg(CREAM),
                ),
            ]),
            Line::from(""),
            section_title("Current"),
            Line::from(vec![
                Span::styled("● Run Estelle Orchestra", Style::default().fg(BLUE)),
                Span::styled(
                    "  3/8 measured · observed 12s ago",
                    Style::default().fg(CREAM),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Ctrl+T expands queued work without discarding completed evidence.",
                Style::default().fg(GHOST),
            )),
        ])
    } else {
        Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "5 items · 2 completed · completed results remain readable",
                Style::default().fg(GHOST),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "✓ Ground checkout failures",
                Style::default().fg(GREEN),
            )),
            Line::from(Span::styled(
                "  result: 7 files grounded · 12 symbols resolved · api/charge.ts:52 bound",
                Style::default().fg(CREAM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "✓ Compare working memory with repo graph",
                Style::default().fg(GREEN),
            )),
            Line::from(Span::styled(
                "  result: local charge.ts differs from the team copy · disagreement disclosed",
                Style::default().fg(CREAM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "● Run Estelle Orchestra",
                Style::default().fg(BLUE),
            )),
            Line::from(Span::styled(
                "  Ground checkout failures and prepare review evidence · 3/8 measured",
                Style::default().fg(CREAM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "○ Score sourced evidence against the gate",
                Style::default().fg(GHOST),
            )),
            Line::from(Span::styled(
                "○ Prepare PR #184 for human review",
                Style::default().fg(GHOST),
            )),
        ])
    };
    frame.render_widget(Paragraph::new(content), inset(columns[0], 3, 2));

    frame.render_widget(panel("TODO CONTRACT"), columns[1]);
    let contract = Text::from(vec![
        section_title("Binding"),
        kv("Toggle", "Ctrl+T"),
        kv("State", if collapsed { "collapsed" } else { "expanded" }),
        Line::from(""),
        section_title("States"),
        Line::from(Span::styled("✓ completed", Style::default().fg(GREEN))),
        Line::from(Span::styled("● in progress", Style::default().fg(BLUE))),
        Line::from(Span::styled("○ not started", Style::default().fg(GHOST))),
        Line::from(""),
        section_title("Invariant"),
        Line::from("Collapse changes density,"),
        Line::from("never recorded results."),
        Line::from(""),
        section_title("Source"),
        Line::from(Span::styled(
            "Session task state",
            Style::default().fg(CREAM),
        )),
        Line::from(Span::styled(
            "Fixture values only",
            Style::default().fg(GOLD),
        )),
    ]);
    frame.render_widget(Paragraph::new(contract), inset(columns[1], 2, 2));
}

fn render_dim_transcript(frame: &mut Frame<'_>, area: Rect) {
    let text = Text::from(vec![
        Line::from("You  Trace the checkout failures to the code that can fix them."),
        Line::from(""),
        Line::from("Estelle  The production signal binds to api/charge.ts:52."),
        Line::from(
            "         The proposed repair is ready for review, with two citations retained.",
        ),
    ]);
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Rgb(58, 60, 61)).bg(BG)),
        inset(area, 3, 1),
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, scene: Scene) {
    let prompt = match scene {
        Scene::SlashPalette => "/con",
        Scene::Settings => "",
        Scene::ModelPicker => "/model",
        _ => "Ask Estelle…",
    };
    let style = if prompt == "Ask Estelle…" {
        Style::default().fg(GHOST)
    } else {
        Style::default().fg(CREAM)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", Style::default().fg(CYAN).bold()),
            Span::styled(prompt, style),
        ]))
        .block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(51, 53, 54))),
        ),
        area,
    );
}

fn render_status(frame: &mut Frame<'_>, area: Rect, scene: Scene) {
    let mode = match scene {
        Scene::OrchestraActive | Scene::OrchestraCompleted => "orchestra",
        Scene::ProductionIssues => "production",
        Scene::ProposedDiff => "review",
        Scene::TodoExpanded | Scene::TodoCollapsed => "todo",
        _ => "session",
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {mode} "), Style::default().fg(CYAN).bold()),
            Span::styled("GPT-5 · auto route", Style::default().fg(CREAM)),
            Span::styled("   ~/Desktop/estelle   main", Style::default().fg(GHOST)),
            Span::styled(
                "                                                   ",
                Style::default(),
            ),
            Span::styled("context: 18%", Style::default().fg(CREAM)),
        ]))
        .style(Style::default().bg(Color::Rgb(18, 18, 18))),
        area,
    );
}

fn action_line<'a>(glyph: &'a str, label: &'a str, command: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{glyph} "), Style::default().fg(CYAN).bold()),
        Span::styled(format!("{label:<30}"), Style::default().fg(CREAM)),
        Span::styled(command, Style::default().fg(BLUE)),
    ])
}

fn section_title(title: &str) -> Line<'_> {
    Line::from(Span::styled(title, Style::default().fg(CYAN).bold()))
}

fn kv<'a>(key: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<17}"), Style::default().fg(GHOST)),
        Span::styled(value, Style::default().fg(CREAM)),
    ])
}

fn issue_line<'a>(id: &'a str, signal: &'a str, symbol: &'a str, verdict: &'a str) -> Line<'a> {
    let verdict_color = match verdict {
        "FLAGGED" => RED,
        "PASSED" => GREEN,
        _ => GOLD,
    };
    Line::from(vec![
        Span::styled(format!("{id:<8}"), Style::default().fg(BLUE)),
        Span::styled(format!("{signal:<24}"), Style::default().fg(CREAM)),
        Span::styled(format!("{symbol:<29}"), Style::default().fg(CYAN)),
        Span::styled(verdict, Style::default().fg(verdict_color).bold()),
    ])
}

fn command<'a>(command: &'a str, description: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {command:<18}"), Style::default().fg(CREAM)),
        Span::styled(description, Style::default().fg(GHOST)),
    ])
}

fn selected<'a>(command: &'a str, description: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("› {command:<18}"),
            Style::default().fg(BG).bg(BLUE).bold(),
        ),
        Span::styled(format!(" {description}"), Style::default().fg(CREAM)),
    ])
}

fn setting<'a>(name: &'a str, value: &'a str, selected: bool) -> Line<'a> {
    let glyph = if selected { "›" } else { " " };
    Line::from(vec![
        Span::styled(
            format!("{glyph} {name:<24}"),
            Style::default()
                .fg(if selected { BLUE } else { CREAM })
                .bold(),
        ),
        Span::styled(
            value,
            Style::default().fg(if selected { CREAM } else { GHOST }),
        ),
    ])
}

fn model<'a>(name: &'a str, provider: &'a str, note: &'a str, selected: bool) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{} {name:<28}", if selected { "›" } else { " " }),
            Style::default()
                .fg(if selected { BLUE } else { CREAM })
                .bold(),
        ),
        Span::styled(format!("{provider:<16}"), Style::default().fg(GHOST)),
        Span::styled(
            note,
            Style::default().fg(if selected { GREEN } else { GHOST }),
        ),
    ])
}

fn panel(title: &str) -> Block<'_> {
    Block::new()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(CYAN).bold(),
        ))
        .borders(Borders::LEFT | Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(52, 55, 56)))
        .style(Style::default().bg(BG))
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(horizontal),
        area.y.saturating_add(vertical),
        area.width.saturating_sub(horizontal.saturating_mul(2)),
        area.height.saturating_sub(vertical.saturating_mul(2)),
    )
}

fn buffer_text(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        output.push_str(row.trim_end());
        output.push('\n');
    }
    output
}

fn buffer_svg(buffer: &Buffer) -> String {
    const CELL_WIDTH: u16 = 9;
    const CELL_HEIGHT: u16 = 18;
    let width = buffer.area.width * CELL_WIDTH + 32;
    let height = buffer.area.height * CELL_HEIGHT + 32;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"#101010\"/>\n\
         <g font-family=\"SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"14\" xml:space=\"preserve\">\n"
    );
    for y in 0..buffer.area.height {
        let baseline = 24 + y * CELL_HEIGHT;
        let mut x = 0;
        while x < buffer.area.width {
            let cell = &buffer[(x, y)];
            let fg = cell.fg;
            let bg = cell.bg;
            let modifier = cell.modifier;
            let start = x;
            let mut content = String::new();
            while x < buffer.area.width {
                let next = &buffer[(x, y)];
                if next.fg != fg || next.bg != bg || next.modifier != modifier {
                    break;
                }
                content.push_str(next.symbol());
                x += 1;
            }
            let pixel_x = 16 + start * CELL_WIDTH;
            if bg != Color::Reset && bg != BG {
                let segment_width = (x - start) * CELL_WIDTH;
                let pixel_y = baseline.saturating_sub(14);
                let _ = writeln!(
                    svg,
                    "<rect x=\"{pixel_x}\" y=\"{pixel_y}\" width=\"{segment_width}\" height=\"{CELL_HEIGHT}\" fill=\"{}\"/>",
                    color_hex(bg),
                );
            }
            if !content.trim().is_empty() {
                let weight = if modifier.contains(Modifier::BOLD) {
                    "700"
                } else {
                    "400"
                };
                let _ = writeln!(
                    svg,
                    "<text x=\"{pixel_x}\" y=\"{baseline}\" fill=\"{}\" font-weight=\"{weight}\">{}</text>",
                    color_hex(fg),
                    xml_escape(&content),
                );
            }
        }
    }
    svg.push_str("</g>\n</svg>\n");
    svg
}

fn color_hex(color: Color) -> String {
    match color {
        Color::Reset => "#E9E6DC".into(),
        Color::Black => "#101010".into(),
        Color::Red | Color::LightRed => "#E25B55".into(),
        Color::Green | Color::LightGreen => "#67D391".into(),
        Color::Yellow | Color::LightYellow => "#E4BC5D".into(),
        Color::Blue | Color::LightBlue => "#65A8FF".into(),
        Color::Magenta | Color::LightMagenta => "#C28AC9".into(),
        Color::Cyan | Color::LightCyan => "#70C6CC".into(),
        Color::Gray | Color::White => "#E9E6DC".into(),
        Color::DarkGray => "#707478".into(),
        Color::Rgb(r, g, b) => format!("#{r:02X}{g:02X}{b:02X}"),
        Color::Indexed(index) => format!("rgb({index},{index},{index})"),
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_gallery(
    output: &Path,
    frames: &BTreeMap<&str, GalleryFrame>,
) -> Result<(), std::io::Error> {
    fs::create_dir_all(output)?;
    let mut index = String::from(
        "<!doctype html><meta charset=\"utf-8\"><title>Estelle TUI visual gallery</title>\
         <style>body{background:#080808;color:#E9E6DC;font-family:system-ui;margin:32px}\
         h1{font-size:20px}h2{font-size:15px;color:#70C6CC;margin-top:36px}\
         img{display:block;width:min(100%,1328px);border:1px solid #292b2c;background:#101010}</style>\
         <h1>Estelle TUI · deterministic fixture gallery</h1>",
    );
    for (name, frame) in frames {
        fs::write(output.join(format!("{name}.txt")), &frame.text)?;
        fs::write(output.join(format!("{name}.svg")), &frame.svg)?;
        let _ = write!(
            index,
            "<h2>{name}</h2><img src=\"{name}.svg\" alt=\"{name}\">"
        );
    }
    fs::write(output.join("index.html"), index)
}

#[test]
fn gallery_covers_the_requested_surfaces() -> Result<(), std::io::Error> {
    let frames = gallery_frames();

    assert_eq!(frames.len(), REQUIRED_FRAMES.len());
    for (name, proof) in REQUIRED_FRAMES {
        let frame = frames.get(name).unwrap_or_else(|| panic!("missing {name}"));
        assert!(frame.text.contains(proof), "{name} did not show {proof:?}");
        assert!(frame.svg.starts_with("<svg"), "{name} was not an SVG");
        assert!(frame.svg.contains("#E9E6DC"), "{name} lost cream ink");
        assert!(
            frame.text.contains("DESIGN FIXTURE · NOT LIVE DATA"),
            "{name} could be mistaken for actual-code proof"
        );
        for retired in ["Agent Swarm", "Rationed", "K3"] {
            assert!(
                !frame.text.contains(retired),
                "{name} retained retired vocabulary {retired:?}"
            );
        }
    }

    if let Some(output) = std::env::var_os("ESTELLE_VISUAL_GALLERY_DIR") {
        write_gallery(Path::new(&output), &frames)?;
    }
    Ok(())
}
