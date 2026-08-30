//! Deterministic snapshots of the live Estelle TUI.
//!
//! The command supplies bounded sample state, then enters the same `render_frame` function used by
//! the interactive binary. It deliberately owns no widgets or layout: a screenshot that can drift
//! away from the customer renderer is worse than no screenshot.

use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::App;
use crate::Args;
use crate::Theme;
use crate::live_renderer::render_frame;
use crate::theme::ScreenTheme;
use crate::transcript::TranscriptEntry;

pub const SCREENS: &[&str] = &[
    "models · hosted",
    "models · local",
    "compare",
    "usage",
    "tool calls",
    "graph",
    "memory",
    "skill match",
    "everything at once",
    "when it breaks",
    "monitor · live",
    "production HUD",
    "THE PLAN",
];

pub fn dump(
    screen: Option<usize>,
    screen_theme: ScreenTheme,
    _pulse_enabled: bool,
) -> Result<Vec<String>, String> {
    let indices = match screen {
        Some(number @ 1..=13) => vec![number - 1],
        Some(number) => return Err(format!("screen must be between 1 and 13, got {number}")),
        None => (0..SCREENS.len()).collect(),
    };
    let mut output = Vec::new();
    for index in indices {
        let app = snapshot_app(index, screen_theme);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))
            .map_err(|error| format!("create live screen renderer: {error}"))?;
        terminal
            .draw(|frame| render_frame(frame, &app, Instant::now()))
            .map_err(|error| format!("render live screen {}: {error}", index + 1))?;
        output.push(format!(
            "LIVE RENDERER SNAPSHOT · BOUNDED SAMPLE STATE · {} ({}/{})",
            SCREENS[index],
            index + 1,
            SCREENS.len()
        ));
        append_buffer(&mut output, terminal.backend().buffer());
    }
    Ok(output)
}

fn snapshot_app(index: usize, screen_theme: ScreenTheme) -> App {
    let mut app = App::new(Args {
        command: None,
        repo: Some("fatelabs/estelle".to_string()),
    });
    app.boot = None;
    app.auth_resolved = true;
    app.prod_panel_visible = false;
    app.theme = match screen_theme {
        ScreenTheme::Dark => Theme::Dark,
        ScreenTheme::Cream => Theme::CreamInk,
    };

    let title = SCREENS[index];
    app.transcript.push(TranscriptEntry::System(format!(
        "snapshot {}/{} · {title} · bounded sample state",
        index + 1,
        SCREENS.len()
    )));
    match index {
        0 => command(
            &mut app,
            "models",
            &["hosted models", "openai/gpt-5.5  frontier  selected"],
        ),
        1 => command(
            &mut app,
            "models local",
            &["local models", "lmstudio/qwen  reachable"],
        ),
        2 => command(
            &mut app,
            "compare",
            &["4 models marked", "same prompt · separate receipts"],
        ),
        3 => command(
            &mut app,
            "usage",
            &["measured model spend", "unknown cost stays unknown"],
        ),
        4 => app.transcript.push(TranscriptEntry::Tool {
            label: "Bash · cargo test -p estelle-tui".to_string(),
            lines: vec!["320 passed".to_string(), "0 failed".to_string()],
            expanded: false,
        }),
        5 => command(
            &mut app,
            "graph",
            &["fatelabs/estelle", "symbol → callers → affected tests"],
        ),
        6 => command(
            &mut app,
            "memory",
            &["repository memory", "scope · evidence · observed_at"],
        ),
        7 => command(
            &mut app,
            "skills",
            &[
                "guardian · skill match",
                "matched because the request names a probe",
            ],
        ),
        8 => {
            command(&mut app, "orchestra", &["production · review · repair"]);
            app.transcript.push(TranscriptEntry::Tool {
                label: "Read tui/src/live_renderer.rs".to_string(),
                lines: vec!["one renderer owns the frame".to_string()],
                expanded: false,
            });
        }
        9 => app.transcript.push(TranscriptEntry::Failure([
            "Gate refused the proposed change.".to_string(),
            "The named control did not fail.".to_string(),
            "Restore the control before retrying.".to_string(),
        ])),
        10 => command(
            &mut app,
            "monitor",
            &[
                "monitor · live",
                "healthy checks remain distinct from incidents",
            ],
        ),
        11 => command(
            &mut app,
            "production",
            &[
                "production HUD",
                "Enter opens event → symbol → diff",
                "request denominator unavailable",
            ],
        ),
        _ => command(
            &mut app,
            "work",
            &[
                "THE PLAN",
                "✓ Inspect the live renderer",
                "● Replace the parallel catalogue  — unevidenced",
                "▲ Deploy remains protected",
            ],
        ),
    }
    app
}

fn command(app: &mut App, name: &str, lines: &[&str]) {
    app.transcript.push(TranscriptEntry::Command {
        name: name.to_string(),
        lines: lines.iter().map(|line| (*line).to_string()).collect(),
    });
}

fn append_buffer(output: &mut Vec<String>, buffer: &ratatui::buffer::Buffer) {
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        let line = line.trim_end();
        if !line.is_empty() {
            output.push(line.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_entry_snapshots_the_live_renderer() {
        for number in 1..=SCREENS.len() {
            let output = dump(Some(number), ScreenTheme::Dark, true)
                .expect("live renderer snapshot")
                .join("\n");
            assert!(
                output.contains("LIVE RENDERER SNAPSHOT · BOUNDED SAMPLE STATE"),
                "{output}"
            );
            assert!(
                !output.contains("DESIGN FIXTURE · NOT LIVE DATA"),
                "{output}"
            );
            assert!(output.contains("ESTELLE"), "{output}");
            assert!(output.contains("ASK ESTELLE"), "{output}");
            assert!(output.contains(SCREENS[number - 1]), "{output}");
        }
    }

    #[test]
    fn invalid_screen_cannot_fall_through_to_a_different_snapshot() {
        assert_eq!(
            dump(Some(14), ScreenTheme::Dark, true),
            Err("screen must be between 1 and 13, got 14".to_string())
        );
    }
}
