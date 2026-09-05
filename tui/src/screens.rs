//! Every page the CLI renders. One function per screen, each returning Lines.
//! Nothing here positions by hand: the column spec does it.

use crate::cols::{Cell, Col, head, row, rule};
use crate::production_hud::ProductionGraph;
use crate::theme::Palette;
use crate::theme::ScreenTheme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// One row of the routed-model table: marker, model, provider, score, n, in, out, ctx, up30d, notes.
///
/// Named because clippy is right that a bare ten-tuple is unreadable at the call site — at that width
/// a reader cannot tell column 7 from column 8 without counting, and neither can a reviewer.
type RoutedModelRow<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

/// One row of the local-model table: marker, model, param, tok/s, quant, mode, mem, fit.
type LocalModelRow<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

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

pub fn render(idx: usize, p: &Palette, tick: u64, pulse_on: bool) -> Vec<Line<'static>> {
    match idx {
        0 => models(p),
        1 => local(p),
        2 => compare(p),
        3 => usage(p),
        4 => tools(p),
        5 => graph(p),
        6 => memory(p),
        7 => skill(p),
        8 => everything(p, tick, pulse_on),
        9 => broken(p, tick, pulse_on),
        10 => monitor(p, tick, pulse_on),
        11 => production_hud(p, tick, pulse_on),
        _ => work_plan(p),
    }
}

/// Render the production screen catalog through the same Ratatui backend used by the
/// headless contract tests. The catalog is explicitly labelled as fixture data; live
/// commands continue to render server replies rather than these design examples.
pub fn dump(
    screen: Option<usize>,
    screen_theme: ScreenTheme,
    pulse_enabled: bool,
) -> Result<Vec<String>, String> {
    let indices = match screen {
        Some(number @ 1..=13) => vec![number - 1],
        Some(number) => return Err(format!("screen must be between 1 and 13, got {number}")),
        None => (0..SCREENS.len()).collect(),
    };
    let palette = screen_theme.palette();
    let mut output = Vec::new();
    for index in indices {
        output.push(format!(
            "DESIGN FIXTURE · NOT LIVE DATA · {} ({}/{})",
            SCREENS[index],
            index + 1,
            SCREENS.len()
        ));
        let mut terminal = Terminal::new(TestBackend::new(100, 30))
            .map_err(|error| format!("create screen renderer: {error}"))?;
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(render(index, &palette, 0, pulse_enabled))
                        .style(Style::default().bg(palette.ground)),
                    frame.area(),
                );
            })
            .map_err(|error| format!("render screen {}: {error}", index + 1))?;
        let buffer = terminal.backend().buffer();
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
    Ok(output)
}

fn production_hud(palette: &Palette, tick: u64, pulse_enabled: bool) -> Vec<Line<'static>> {
    crate::production_hud::lines(
        &ProductionGraph {
            issue_key: "design-fixture".to_string(),
            failing_symbol: "charge_card".to_string(),
            failing_file: "billing.py:88".to_string(),
            healthy_subsystems: vec![
                "auth".to_string(),
                "search".to_string(),
                "memory".to_string(),
            ],
            blast_radius: vec!["checkout.py".to_string(), "receipts.py".to_string()],
            chokepoints: vec!["api.py".to_string()],
            withheld: None,
            core_files: vec!["models.py".to_string()],
            drill_down: false,
        },
        palette,
        // The catalog page is 100 columns and every other screen rules at 82.
        82,
        tick,
        pulse_enabled,
    )
}

fn work_plan(palette: &Palette) -> Vec<Line<'static>> {
    crate::work_plan::lines(
        &estelle_client::WorkPlan {
            revision: 4,
            steps: vec![
                estelle_client::WorkPlanStep {
                    id: "inspect".to_string(),
                    step: "Inspect the existing work progress seam".to_string(),
                    status: "complete".to_string(),
                    evidence: "work_progress.py:WorkProgressRecorder".to_string(),
                },
                estelle_client::WorkPlanStep {
                    id: "wire".to_string(),
                    step: "Stream the structured architect plan".to_string(),
                    status: "active".to_string(),
                    evidence: "api_work.py:handle_work".to_string(),
                },
                estelle_client::WorkPlanStep {
                    id: "prove".to_string(),
                    step: "Prove the negative control".to_string(),
                    status: "pending".to_string(),
                    evidence: "".to_string(),
                },
                estelle_client::WorkPlanStep {
                    id: "deploy".to_string(),
                    step: "Deploy the verified build".to_string(),
                    status: "protected".to_string(),
                    evidence: "scripts/deploy.sh".to_string(),
                },
            ],
        },
        palette,
    )
}

fn blank() -> Line<'static> {
    Line::from("")
}
fn dim(p: &Palette, s: &str) -> Line<'static> {
    Line::from(Span::styled(s.to_string(), Style::default().fg(p.dim)))
}

// ── 1 ────────────────────────────────────────────────────────────────────
fn models(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[
        Col::l(2),
        Col::l(17),
        Col::l(10),
        Col::r(6),
        Col::r(3),
        Col::r(6),
        Col::r(7),
        Col::r(5),
        Col::r(6),
        Col::l(20),
    ];
    let mut v = vec![
        rule("models", "hosted", 78, p.dim, p.mid, p.cite),
        blank(),
        head(
            C,
            &[
                "", "model", "provider", "score", "n", "in", "out", "ctx", "up30d", "notes",
            ],
            p.dim,
            0,
        ),
    ];
    let rows: &[RoutedModelRow] = &[
        (
            "●",
            "claude-opus-5",
            "anthropic",
            "—",
            "0",
            "$5.00",
            "$25.00",
            "1M",
            "99.98",
            "",
        ),
        (
            "●",
            "claude-opus-4-8",
            "anthropic",
            "—",
            "4",
            "$5.00",
            "$25.00",
            "1M",
            "99.97",
            "pinned by you",
        ),
        (
            "●",
            "gpt-5.6-luna-pro",
            "openai",
            "—",
            "0",
            "$5.00",
            "$25.00",
            "400k",
            "99.95",
            "",
        ),
        (
            "●",
            "gpt-5.6-sol",
            "openai",
            "—",
            "3",
            "$5.00",
            "$25.00",
            "400k",
            "99.95",
            "empty under 16k out",
        ),
        (
            "●",
            "gemini-3.7-flash",
            "google",
            "—",
            "0",
            "$1.50",
            "$6.00",
            "1M",
            "97.10",
            "503s today, 0 of 3",
        ),
        (
            "●",
            "gemini-3.5-flash",
            "google",
            "—",
            "1",
            "$1.50",
            "$6.00",
            "1M",
            "99.99",
            "vendor says legacy",
        ),
        (
            "●",
            "deepseek-v4-pro",
            "deepseek",
            "—",
            "4",
            "$0.95",
            "$3.80",
            "128k",
            "99.90",
            "",
        ),
        (
            "○",
            "kimi-k3",
            "moonshot",
            "—",
            "0",
            "—",
            "—",
            "256k",
            "—",
            "no key on file",
        ),
    ];
    for (i, r) in rows.iter().enumerate() {
        let up = if r.8.starts_with("97") {
            p.red
        } else if r.8 == "—" {
            p.dim
        } else {
            p.green
        };
        let note = if r.9.contains("503") {
            p.red
        } else if r.9.contains("empty") || r.9.contains("legacy") {
            p.warn
        } else if r.9.contains("pinned") {
            p.green
        } else {
            p.dim
        };
        let mark = if r.0 == "●" { p.green } else { p.dim };
        let name = if r.0 == "○" { p.dim } else { p.mid };
        let mut line = row(
            C,
            &[
                Cell(r.0, mark),
                Cell(r.1, name),
                Cell(r.2, p.dim),
                Cell(r.3, p.dim),
                Cell(r.4, p.dim),
                Cell(r.5, p.mid),
                Cell(r.6, p.mid),
                Cell(r.7, p.dim),
                Cell(r.8, up),
                Cell(r.9, note),
            ],
            0,
        );
        if i == 0 {
            line = line.style(Style::default().bg(p.tint));
        }
        v.push(line);
    }
    v.push(blank());
    v.push(Line::from(Span::styled(
        "score is empty because score is OURS. 12 outcomes, p=0.63.".to_string(),
        Style::default().fg(p.warn),
    )));
    v.push(Line::from(vec![
        Span::styled(
            "estelle bench --models 7 --tasks 100".to_string(),
            Style::default().fg(p.cite),
        ),
        Span::styled(" fills it".to_string(), Style::default().fg(p.dim)),
    ]));
    v
}

// ── 2 ────────────────────────────────────────────────────────────────────
fn local(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[
        Col::l(2),
        Col::l(21),
        Col::r(6),
        Col::r(6),
        Col::l(7),
        Col::l(5),
        Col::r(5),
        Col::l(10),
    ];
    let mut v = vec![
        rule("models", "local", 74, p.dim, p.mid, p.plan),
        dim(p, "  M5 Max · 128 GB unified · 40-core GPU"),
        blank(),
        head(
            C,
            &["", "model", "param", "tok/s", "quant", "mode", "mem", "fit"],
            p.dim,
            0,
        ),
    ];
    let rows: &[LocalModelRow] = &[
        (
            "L",
            "Qwen3-Coder-Next-80B",
            "80B",
            "79.8",
            "Q4_K_M",
            "MoE",
            "44%",
            "perfect",
        ),
        (
            "",
            "Qwen3-Coder-30B-A3B",
            "30.5B",
            "71.7",
            "Q4_K_M",
            "MoE",
            "17%",
            "perfect",
        ),
        (
            "",
            "Qwen3-Coder-Next-80B",
            "80B",
            "31.2",
            "Q8_0",
            "MoE",
            "78%",
            "tight",
        ),
        (
            "",
            "Qwen2.5-Coder-32B",
            "32B",
            "68.4",
            "Q4_K_M",
            "GPU",
            "18%",
            "perfect",
        ),
        (
            "",
            "Qwen3-Coder-480B",
            "480B",
            "—",
            "Q4_K_M",
            "MoE",
            "263%",
            "too large",
        ),
        (
            "OL",
            "DeepSeek-Coder-V3-16B",
            "16B",
            "99.7",
            "Q4_K_M",
            "MoE",
            "9%",
            "perfect",
        ),
    ];
    for (i, r) in rows.iter().enumerate() {
        let fit = match r.7 {
            "perfect" => p.green,
            "tight" => p.warn,
            _ => p.red,
        };
        let mem = if r.6 == "263%" {
            p.red
        } else if r.6 == "78%" {
            p.warn
        } else {
            p.green
        };
        let mut line = row(
            C,
            &[
                Cell(r.0, p.green),
                Cell(r.1, p.mid),
                Cell(r.2, p.dim),
                Cell(r.3, p.mid),
                Cell(r.4, p.dim),
                Cell(r.5, p.cite),
                Cell(r.6, mem),
                Cell(r.7, fit),
            ],
            0,
        );
        if i == 0 {
            line = line.style(Style::default().bg(p.tint));
        }
        v.push(line);
    }
    v.push(blank());
    v.push(dim(
        p,
        "L installed · OL ollama · d download · r run · b bench",
    ));
    v
}

// ── 3 ────────────────────────────────────────────────────────────────────
fn compare(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[Col::l(20), Col::r(9), Col::r(9), Col::r(8), Col::r(9)];
    let mut v = vec![
        rule("compare", "4 marked", 62, p.dim, p.mid, p.cite),
        blank(),
    ];
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        ("", "opus-4-8", "luna-pro", "v4-pro", "Qwen80B"),
        ("where", "hosted", "hosted", "hosted", "local"),
        ("score (ours)", "91.2", "78.9", "84.1", "—"),
        ("outcomes", "40", "40", "40", "0"),
        ("$/Mtok in", "$5.00", "$5.00", "$0.95", "$0.00"),
        ("$/Mtok out", "$25.00", "$25.00", "$3.80", "$0.00"),
        ("context", "1M", "400k", "128k", "262k"),
        ("tok/s", "—", "—", "—", "79.8"),
        ("uptime 30d", "99.97", "99.95", "99.90", "100"),
        ("your spend 30d", "$61.40", "$2.10", "$0.88", "$0.00"),
    ];
    for r in rows {
        let last = if r.0 == "where" || r.0.contains("spend") || r.0.contains("$/") {
            p.green
        } else {
            p.mid
        };
        v.push(row(
            C,
            &[
                Cell(r.0, p.dim),
                Cell(r.1, p.mid),
                Cell(r.2, p.mid),
                Cell(r.3, p.mid),
                Cell(r.4, last),
            ],
            0,
        ));
    }
    v.push(blank());
    v.push(row(
        C,
        &[
            Cell("cost per landed change", p.cite),
            Cell("$1.53", p.mid),
            Cell("$3.44", p.mid),
            Cell("$0.26", p.green),
            Cell("—", p.dim),
        ],
        0,
    ));
    v
}

// ── 4 ────────────────────────────────────────────────────────────────────
fn bar(pct: usize, w: usize) -> String {
    let n = (w * pct + 50) / 100;
    format!("{}{}", "█".repeat(n), "░".repeat(w - n))
}
fn usage(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[Col::l(11), Col::l(22), Col::r(5), Col::l(24)];
    let mut v = vec![
        rule("usage", "Ultra 250M", 70, p.dim, p.mid, p.cite),
        blank(),
    ];
    v.push(row(
        C,
        &[
            Cell("Estelle", p.mid),
            Cell("", p.dim),
            Cell("", p.dim),
            Cell("memory-tokens this period", p.dim),
        ],
        2,
    ));
    let b41 = Box::leak(bar(41, 22).into_boxed_str());
    v.push(row(
        C,
        &[
            Cell("", p.dim),
            Cell(b41, p.green),
            Cell("41%", p.mid),
            Cell("103M of 250M", p.dim),
        ],
        2,
    ));
    v.push(row(
        C,
        &[
            Cell("", p.dim),
            Cell("", p.dim),
            Cell("", p.dim),
            Cell("resets 1 Sep", p.dim),
        ],
        2,
    ));
    v.push(blank());
    v.push(row(
        C,
        &[
            Cell("Your plan", p.mid),
            Cell("", p.dim),
            Cell("", p.dim),
            Cell("anthropic oauth", p.dim),
        ],
        2,
    ));
    for (lbl, pct, col, reset) in [
        ("5-hour", 18usize, p.green, "resets 4h 03m"),
        ("weekly", 69, p.warn, "resets Mon 6pm"),
        ("opus", 14, p.green, "resets Mon 6pm"),
    ] {
        let b = Box::leak(bar(pct, 22).into_boxed_str());
        let pc = Box::leak(format!("{pct}%").into_boxed_str());
        v.push(row(
            C,
            &[
                Cell(lbl, p.dim),
                Cell(b, col),
                Cell(pc, p.mid),
                Cell(reset, p.dim),
            ],
            2,
        ));
    }
    v.push(blank());
    v.push(dim(p, "  6 repos held · 2 jobs running"));
    v.push(dim(
        p,
        "  a provider that does not publish a limit shows a dash",
    ));
    v
}

// ── 5 ────────────────────────────────────────────────────────────────────
fn tools(p: &Palette) -> Vec<Line<'static>> {
    const D: &[Col] = &[Col::r(6), Col::l(2), Col::l(50)];
    let mut v = vec![
        Line::from(vec![
            Span::styled("⏺ ".to_string(), Style::default().fg(p.green)),
            Span::styled("Bash".to_string(), Style::default().fg(p.mid)),
            Span::styled(
                "(cargo test --workspace)".to_string(),
                Style::default().fg(p.dim),
            ),
        ]),
        dim(p, "  ⎿  Compiling estelle-tui v0.3.0"),
        dim(p, "      Finished test profile in 8.1s"),
        Line::from(vec![
            Span::styled(
                "     test client::retry::bounded ... ".to_string(),
                Style::default().fg(p.dim),
            ),
            Span::styled("ok".to_string(), Style::default().fg(p.green)),
        ]),
        Line::from(vec![
            Span::styled("     … ".to_string(), Style::default().fg(p.dim)),
            Span::styled("+394 lines".to_string(), Style::default().fg(p.cite)),
        ]),
        blank(),
        Line::from(vec![
            Span::styled("⏺ ".to_string(), Style::default().fg(p.green)),
            Span::styled("Edit".to_string(), Style::default().fg(p.mid)),
            Span::styled("(src/client.rs)".to_string(), Style::default().fg(p.dim)),
        ]),
        Line::from(vec![
            Span::styled(
                "  ⎿  Updated src/client.rs with ".to_string(),
                Style::default().fg(p.dim),
            ),
            Span::styled("12 additions".to_string(), Style::default().fg(p.green)),
            Span::styled(" and ".to_string(), Style::default().fg(p.dim)),
            Span::styled("3 removals".to_string(), Style::default().fg(p.red)),
        ]),
    ];
    // the tinted diff bands, exactly like codex
    let diff: &[(&str, &str, &str, bool, bool)] = &[
        ("86", "", "    let mut attempt = 0;", false, false),
        ("87", "+", "    const MAX_ATTEMPTS: u32 = 3;", true, false),
        (
            "88",
            "+",
            "    let mut backoff = Duration::from_millis(50);",
            true,
            false,
        ),
        ("89", "-", "    loop {", false, true),
        ("89", "+", "    while attempt < MAX_ATTEMPTS {", true, false),
        (
            "90",
            "",
            "        match self.inner.send(&req).await {",
            false,
            false,
        ),
    ];
    for (n, sign, text, add, del) in diff {
        let fg = if *add {
            p.green
        } else if *del {
            p.red
        } else {
            p.dim
        };
        let mut l = row(D, &[Cell(n, p.dim), Cell(sign, fg), Cell(text, fg)], 4);
        if *add {
            l = l.style(Style::default().bg(p.diff_add));
        }
        if *del {
            l = l.style(Style::default().bg(p.diff_del));
        }
        v.push(l);
    }
    v.push(Line::from(vec![
        Span::styled("     … ".to_string(), Style::default().fg(p.dim)),
        Span::styled("+10 lines".to_string(), Style::default().fg(p.cite)),
    ]));
    v.push(blank());
    v.push(dim(p, "ctrl+o expands every step · c copies · r re-runs"));
    v
}

// ── 6 ────────────────────────────────────────────────────────────────────
fn graph(p: &Palette) -> Vec<Line<'static>> {
    const K: &[Col] = &[Col::l(12), Col::l(46)];
    const E: &[Col] = &[Col::l(4), Col::l(24), Col::l(13), Col::l(12)];
    const M: &[Col] = &[Col::l(2), Col::l(24), Col::r(6), Col::l(22)];
    let mut v = vec![
        rule("graph", "estelle/estelle", 74, p.dim, p.mid, p.cite),
        blank(),
        Line::from(vec![
            Span::styled("› ".to_string(), Style::default().fg(p.cite)),
            Span::styled(
                "explain deep_review_gate".to_string(),
                Style::default().fg(p.mid),
            ),
        ]),
        blank(),
    ];
    for (k, val, c) in [
        ("source", "serve/deep_review_gate.py L357", p.cite),
        ("community", "3 · the gate cluster", p.dim),
        ("degree", "11 · 4 in, 7 out", p.mid),
        ("blast radius", "touching it moves 41 files", p.warn),
    ] {
        v.push(row(K, &[Cell(k, p.dim), Cell(val, c)], 2));
    }
    v.push(blank());
    v.push(dim(p, "  connections (11)"));
    for (dir, node, kind, conf) in [
        ("──▶", "post_images", "[calls]", "[EXTRACTED]"),
        ("──▶", "review_ruleset", "[calls]", "[EXTRACTED]"),
        ("◀──", "api_dev", "[imports]", "[EXTRACTED]"),
        ("──▶", "_REVIEW_TUNING", "[reads]", "[INFERRED]"),
    ] {
        let cc = if conf.contains("EXTRACTED") {
            p.green
        } else {
            p.warn
        };
        v.push(row(
            E,
            &[
                Cell(dir, p.dim),
                Cell(node, p.mid),
                Cell(kind, p.dim),
                Cell(conf, cc),
            ],
            2,
        ));
    }
    v.push(blank());
    v.push(Line::from(vec![
        Span::styled("› ".to_string(), Style::default().fg(p.cite)),
        Span::styled("communities".to_string(), Style::default().fg(p.mid)),
    ]));
    v.push(blank());
    for (node, n, top, c) in [
        ("the gate cluster", "96", "deep_review_gate", p.red),
        ("memory + retrieval", "71", "memory_pgvector", p.cite),
        ("routing + models", "56", "model_router", p.green),
        ("orchestra", "48", "orchestra_live", p.warn),
        ("billing", "40", "plans", p.skill),
    ] {
        v.push(row(
            M,
            &[
                Cell("◆", c),
                Cell(node, p.mid),
                Cell(n, p.mid),
                Cell(top, p.dim),
            ],
            2,
        ));
    }
    v.push(blank());
    v.push(dim(
        p,
        "  EXTRACTED is in the source. INFERRED was resolved, and says so.",
    ));
    v.push(dim(
        p,
        "  space filters · O opens the force-directed view in a browser",
    ));
    v
}

// ── 7 ────────────────────────────────────────────────────────────────────
fn memory(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[Col::l(2), Col::l(38), Col::l(9), Col::r(7), Col::l(14)];
    let mut v = vec![
        rule("memory", "estelle/estelle", 76, p.dim, p.mid, p.cite),
        blank(),
        head(C, &["", "", "kind", "added", "cited by"], p.dim, 0),
    ];
    let rows: &[(&str, &str, &str, &str)] = &[
        (
            "the gate is never bypassed by auto-mode",
            "decision",
            "14 Aug",
            "3 citations",
        ),
        (
            "we ship propose-only by default",
            "decision",
            "12 Aug",
            "1 citation",
        ),
        (
            "Rows exists so an adapter cannot tell",
            "lesson",
            "11 Aug",
            "2 citations",
        ),
        (
            "cream is #E9E6DC, not #F1EFE9",
            "fact",
            "09 Aug",
            "1 citation",
        ),
        (
            "opus-4-8 is pinned, do not upgrade",
            "decision",
            "25 Aug",
            "you, just now",
        ),
    ];
    for (i, r) in rows.iter().enumerate() {
        let kind = match r.1 {
            "decision" => p.cite,
            "lesson" => p.skill,
            _ => p.green,
        };
        let cite = if r.3.starts_with("you") {
            p.warn
        } else {
            p.dim
        };
        let mut l = row(
            C,
            &[
                Cell(if i == 0 { "›" } else { "" }, p.cite),
                Cell(r.0, p.mid),
                Cell(r.1, kind),
                Cell(r.2, p.dim),
                Cell(r.3, cite),
            ],
            0,
        );
        if i == 0 {
            l = l.style(Style::default().bg(p.tint));
        }
        v.push(l);
    }
    v.push(blank());
    v.push(dim(
        p,
        "enter reads · e edits · d retracts · c shows citations",
    ));
    v.push(dim(
        p,
        "an edit SUPERSEDES, it does not overwrite. retracted is not deleted.",
    ));
    v
}

// ── 8 ────────────────────────────────────────────────────────────────────
fn skill(p: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[Col::l(3), Col::l(20), Col::r(9), Col::l(32)];
    let mut v = vec![
        rule("guardian", "skill match", 70, p.dim, p.mid, p.skill),
        blank(),
        head(C, &["", "stage", "cost", "decides"], p.dim, 0),
    ];
    for (n, stage, cost, what, c) in [
        (
            "1",
            "symbol overlap",
            "free",
            "prompt verbs × swept surface",
            p.green,
        ),
        (
            "2",
            "embedding match",
            "$0.0001",
            "238 pre-embedded descriptions",
            p.green,
        ),
        (
            "3",
            "cheap model",
            "$0.0007",
            "only when 1 and 2 disagree",
            p.warn,
        ),
    ] {
        v.push(row(
            C,
            &[
                Cell(n, p.cite),
                Cell(stage, p.mid),
                Cell(cost, c),
                Cell(what, p.dim),
            ],
            0,
        ));
    }
    v.push(blank());
    v.push(Line::from(Span::styled(
        "the tiebreak is a CHOICE BETWEEN TWO, never an open question.".to_string(),
        Style::default().fg(p.warn),
    )));
    v.push(dim(
        p,
        "a model that can only return 1 or 2 cannot invent a skill.",
    ));
    v.push(blank());
    v.push(Line::from(vec![
        Span::styled("» ".to_string(), Style::default().fg(p.skill)),
        Span::styled("matches ".to_string(), Style::default().fg(p.mid)),
        Span::styled(
            "improve-codebase-architecture".to_string(),
            Style::default().fg(p.skill),
        ),
        Span::styled(".".to_string(), Style::default().fg(p.mid)),
    ]));
    v.push(dim(p, "  tab to use it · enter to answer normally"));
    v
}

/// The catalog's four workers, drawn by the SHARED renderer and flattened to text so screen 9's
/// two-column mockup can carry them in its left column.
///
/// ⚠️ The clock is fixed so the page is deterministic: the rows are observed 41, 28, 31 and 0
/// seconds before `NOW`, which is where the design's own `41s` column came from.
fn orchestra_fixture(palette: &Palette) -> Vec<String> {
    const OBSERVED: f64 = 1_000.0;
    const NOW: f64 = OBSERVED + 41.0;
    let agent = |index: u64, status: &str, action: &str, ago: f64| estelle_client::FleetAgent {
        index,
        status: serde_json::from_value(serde_json::Value::String(status.to_string()))
            .unwrap_or_default(),
        state_observed_at: NOW - ago,
        current_action: (!action.is_empty()).then(|| action.to_string()),
        ..Default::default()
    };
    let fleet = estelle_client::FleetSnapshot {
        batch: "gate cluster".to_string(),
        models: vec!["opus-4-8".to_string(), "sonnet-5".to_string()],
        state: "running".to_string(),
        observed_at: NOW,
        completed: Some(1),
        total: Some(4),
        agents: vec![
            agent(1, "completed", "the rewrite", 41.0),
            agent(2, "running", "12 call sites", 28.0),
            agent(3, "running", "the regression suite", 31.0),
            agent(4, "queued", "", 0.0),
        ],
        ..Default::default()
    };
    crate::orchestra_view::lines(&fleet, palette, 46, NOW)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

// ── 9 ────────────────────────────────────────────────────────────────────
fn everything(p: &Palette, tick: u64, on: bool) -> Vec<Line<'static>> {
    const T: &[Col] = &[Col::l(46), Col::l(1), Col::l(30)];
    let mut v = Vec::new();
    let mut two = |l: &str, lc, r: &str, rc| {
        v.push(row(
            T,
            &[
                Cell(Box::leak(l.to_string().into_boxed_str()), lc),
                Cell("│", p.dim),
                Cell(Box::leak(r.to_string().into_boxed_str()), rc),
            ],
            0,
        ));
    };
    two(
        "~/estelle · lane-monitor · $2.41",
        p.dim,
        "production",
        p.mid,
    );
    two("", p.dim, "● api       0e4f21a · 18/246", p.dim);
    two(
        "❯ take the four reds in the gate cluster",
        p.cite,
        "● postgres  22/60 · 51% disk",
        p.dim,
    );
    two("", p.dim, "◐ postgrest restarting", p.warn);
    // 🔴 THE WORKER ROWS ARE NOT WRITTEN HERE ANY MORE. They were four fixture strings
    // (`⎿ ✓ w1 opus-4-8 41s $0.212`) while the live session drew a five-across grid of plain
    // text — two presentations of one fact, and only the one nobody ships was designed. Both
    // callers now render `orchestra_view::lines`, so this screen cannot drift from the terminal.
    // The right column keeps its own fixture text: this screen is a mockup of the WHOLE frame.
    let right = [
        "errors 1h ▁▁▂█▃▁▁ 12",
        "────────────────────────────",
        "orchestra · rev 12",
        "◐ 3 running · 1 queued",
        "$0.281 this fleet",
    ];
    for (index, left) in orchestra_fixture(p).into_iter().enumerate() {
        let right = right.get(index).copied().unwrap_or("");
        two(&left, p.dim, right, p.dim);
    }
    two("", p.dim, "────────────────────────────", p.dim);
    two(
        "⏺ Bash(pytest tests/ -x)",
        p.green,
        "memory · estelle/estelle",
        p.mid,
    );
    two(
        "  ⎿  412 passed, 0 failed",
        p.dim,
        "412 held · 6 repos",
        p.dim,
    );
    two("      … +38 lines", p.cite, "103M of 250M · 41%", p.dim);
    two(
        "◐ Working · 2m 14s · 3 of 4 landed",
        p.warn,
        "skills · deepen-arch",
        p.skill,
    );
    v.push(blank());
    v.push(crate::marks::headline(
        crate::marks::Mark::Blocked,
        "postgrest has been restarting for 4m",
        "monitor opened a repair",
        p,
        tick,
        on,
    ));
    v.push(blank());
    // ⚠️ SECOND OWNER OF THE HINT ROW, AND IT KEPT THE DEAD CHORD FOR A WHILE AFTER
    // `ASK_HINTS` DROPPED IT. `ctrl+m` is carriage return in this binary and can never be
    // bound; a catalog screen may print an UNBUILT binding, but not an IMPOSSIBLE one — that
    // is a promise no future commit can keep. Fixing the live row and leaving this behind is
    // the "guard on the path you remembered" defect in its cheapest form.
    v.push(dim(p, "❯   tab repo · ctrl+s spend · ctrl+g context"));
    v
}

// ── 10 ───────────────────────────────────────────────────────────────────
fn broken(p: &Palette, tick: u64, on: bool) -> Vec<Line<'static>> {
    // 🔴 The refusal block itself is NOT drawn here any more. It is `gate_refusal::lines`, the
    // single renderer the live `render_gate_modal` also calls — this screen and the customer's
    // terminal now show the same block by construction rather than by two people agreeing.
    let mut v = crate::gate_refusal::lines(
        &crate::gate_refusal::Refusal {
            detail: "repairing  ·  round 1 of 3",
            note: None,
            blockers: &[
                crate::gate_refusal::Blocker {
                    claim: "reqwest::Client::retry()",
                    finding: Some("does not exist"),
                },
                crate::gate_refusal::Blocker {
                    claim: "src/client.rs:88",
                    finding: Some("graph: 0 definition sites"),
                },
            ],
            files: &[],
        },
        p,
        66,
        tick,
        on,
    );
    v.push(blank());
    v.push(crate::marks::headline(
        crate::marks::Mark::Blocked,
        "compact BLOCKED",
        "latest_turn_exceeds_usable_window",
        p,
        tick,
        on,
    ));
    v.push(dim(
        p,
        "   your last message alone is larger than the model's window.",
    ));
    v.push(dim(
        p,
        "   context was left UNCHANGED. split it, or /model something bigger.",
    ));
    v.push(blank());
    v.push(Line::from(Span::styled(
        "the severity is the GLYPH and the COLOUR. the pulse is only emphasis,".to_string(),
        Style::default().fg(p.dim),
    )));
    v.push(Line::from(Span::styled(
        "so --no-pulse loses nothing. every frame stays readable.".to_string(),
        Style::default().fg(p.dim),
    )));
    v.push(blank());
    v.push(Line::from(Span::styled(
        "/diff to read it · /apply to open a PR · /undo".to_string(),
        Style::default().fg(p.dim).add_modifier(Modifier::empty()),
    )));
    v
}

// ── 11 · production monitoring, a live feed you can filter ───────────────
//
// The left rail is every event the agent has SEEN, ranked by volume: that is
// `transaction` on MonitorIssue, which is already the grouping key the store
// uses. The right pane is the live stream for whatever is selected. Filtering
// is the whole point: 2,000 `agent.search` a day is noise until you can ask for
// only the ones that failed.
fn monitor(p: &Palette, tick: u64, on: bool) -> Vec<Line<'static>> {
    const RAIL: &[Col] = &[Col::l(2), Col::l(18), Col::r(7), Col::r(6)];
    const FEED: &[Col] = &[Col::l(9), Col::l(4), Col::l(30), Col::r(6), Col::l(14)];

    let mut v = vec![
        rule("monitor", "live", 82, p.dim, p.mid, p.green),
        blank(),
        Line::from(vec![
            Span::styled("/ ".to_string(), Style::default().fg(p.cite)),
            Span::styled("agent.search".to_string(), Style::default().fg(p.mid)),
            Span::styled("▏".to_string(), Style::default().fg(p.warn)),
            Span::styled(
                "            level: all · last 24h · following".to_string(),
                Style::default().fg(p.dim),
            ),
        ]),
        blank(),
        head(RAIL, &["", "event", "24h", "err"], p.dim, 0),
    ];

    // the rail: every event seen, ranked by volume
    let rail: &[(&str, &str, &str, &str, bool)] = &[
        ("›", "agent.search", "2,014", "31", true),
        ("", "agent.read_file", "1,882", "0", false),
        ("", "agent.edit", "944", "12", false),
        ("", "gate.verdict", "612", "0", false),
        ("", "agent.bash", "410", "7", false),
        ("", "memory.recall", "388", "0", false),
        ("", "orchestra.worker", "96", "2", false),
    ];
    for (mark, name, n, err, sel) in rail {
        let ec = if *err == "0" { p.dim } else { p.warn };
        let mut l = row(
            RAIL,
            &[
                Cell(mark, p.cite),
                Cell(name, if *sel { p.bright } else { p.mid }),
                Cell(n, p.mid),
                Cell(err, ec),
            ],
            0,
        );
        if *sel {
            l = l.style(Style::default().bg(p.tint));
        }
        v.push(l);
    }

    v.push(blank());
    v.push(Line::from(vec![
        Span::styled("agent.search".to_string(), Style::default().fg(p.bright)),
        Span::styled(
            "  ·  2,014 in 24h  ·  ".to_string(),
            Style::default().fg(p.dim),
        ),
        Span::styled("31 failed".to_string(), Style::default().fg(p.warn)),
        Span::styled("  ·  p95 340ms".to_string(), Style::default().fg(p.dim)),
    ]));
    v.push(blank());
    v.push(head(
        FEED,
        &["time", "lvl", "transaction", "ms", "culprit"],
        p.dim,
        0,
    ));

    let feed: &[(&str, &str, &str, &str, &str)] = &[
        (
            "09:14:02",
            "ok",
            "agent.search q=\"retry policy\"",
            "180",
            "search.py:88",
        ),
        (
            "09:14:02",
            "ok",
            "agent.search q=\"backoff\"",
            "204",
            "search.py:88",
        ),
        (
            "09:13:58",
            "ERR",
            "agent.search q=\"\"",
            "12",
            "search.py:41",
        ),
        (
            "09:13:51",
            "ok",
            "agent.search q=\"Rows iterable\"",
            "331",
            "search.py:88",
        ),
        (
            "09:13:44",
            "WARN",
            "agent.search q=… 8k chars",
            "1,902",
            "search.py:88",
        ),
        (
            "09:13:40",
            "ok",
            "agent.search q=\"gate_certified\"",
            "156",
            "search.py:88",
        ),
    ];
    for (t, lvl, txn, ms, culprit) in feed {
        let lc = match *lvl {
            "ERR" => p.red,
            "WARN" => p.warn,
            _ => p.green,
        };
        let mc = if ms.contains(',') { p.warn } else { p.dim };
        v.push(row(
            FEED,
            &[
                Cell(t, p.dim),
                Cell(lvl, lc),
                Cell(txn, p.mid),
                Cell(ms, mc),
                Cell(culprit, p.cite),
            ],
            0,
        ));
    }

    v.push(blank());
    v.push(crate::marks::headline(
        crate::marks::Mark::Blocked,
        "31 of 2,014 returned empty",
        "all of them q=\"\"  ·  search.py:41",
        p,
        tick,
        on,
    ));
    v.push(Line::from(vec![
        Span::styled(
            "  monitor opened a repair  ·  ".to_string(),
            Style::default().fg(p.dim),
        ),
        Span::styled(
            "estelle/estelle#412".to_string(),
            Style::default().fg(p.cite),
        ),
    ]));
    v.push(blank());
    v.push(dim(
        p,
        "/ filter · e errors only · f follow · enter opens a trace · o issue",
    ));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Paragraph;

    #[test]
    fn all_eleven_screens_render_headless_with_their_product_identity() {
        let expected = [
            "models · hosted",
            "models · local",
            "compare · 4 marked",
            "usage · Ultra 250M",
            "Bash",
            "graph · estelle/estelle",
            "memory · estelle/estelle",
            "guardian · skill match",
            "production",
            "Gate refused",
            "monitor · live",
        ];
        let palette = ScreenTheme::Dark.palette();

        for (index, needle) in expected.into_iter().enumerate() {
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        Paragraph::new(render(index, &palette, 0, true)),
                        frame.area(),
                    );
                })
                .expect("render screen");
            let frame = format!("{}", terminal.backend());
            assert!(
                frame.contains(needle),
                "screen {} ({}) did not render {needle:?}\n{frame}",
                index + 1,
                SCREENS[index]
            );
        }
    }

    #[test]
    fn twelfth_screen_renders_the_production_hud_through_the_binary_catalog() {
        let output = dump(Some(12), ScreenTheme::Dark, true)
            .expect("production HUD fixture")
            .join("\n");
        assert!(output.contains("production HUD (12/13)"));
        assert!(output.contains("charge_card"));
        assert!(output.contains("Enter opens event → symbol → diff"));
    }

    #[test]
    fn thirteenth_screen_renders_the_live_plan_through_the_binary_catalog() {
        let output = dump(Some(13), ScreenTheme::Dark, true)
            .expect("plan fixture")
            .join("\n");
        assert!(output.contains("THE PLAN (13/13)"));
        assert!(output.contains("— unevidenced"));
        assert!(output.contains("▲"));
    }
}
