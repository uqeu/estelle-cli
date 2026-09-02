use super::*;

use crossterm::event::MouseButton;
use crossterm::event::MouseEventKind;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_json;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn test_app() -> App {
    let mut app = App::new(Args {
        command: None,
        repo: Some("uqeu/estelle".to_string()),
    });
    app.boot = None;
    app
}

fn rendered_frame_at_size(app: &App, now: Instant, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_frame(frame, app, now))
        .expect("render frame");
    format!("{}", terminal.backend())
}

/// 🔴 **CLICK STILL TOGGLES THE EXACT ROW, AND EXPANDING NOW COUNTS WHAT IT DID NOT DRAW.**
///
/// This test's body was one line long, so it could not tell "expanded" from "the whole output
/// fits". It is twenty lines now: the first is the one the tail must NOT show, the last is the one
/// it must, and the count between them is the founder's rule — *a bash step printing 400 lines
/// shows the last 12 and a COUNT.*
#[test]
fn tool_output_stays_collapsed_until_its_exact_row_is_clicked() {
    let mut app = test_app();
    let mut body = vec!["hidden tool body".to_string()];
    body.extend((1..20).map(|index| format!("line {index:02} of the run")));
    app.transcript.push(TranscriptEntry::Tool {
        label: "!cargo test".to_string(),
        lines: body,
        expanded: false,
    });

    // ⚠️ TALL ENOUGH THAT THE EXPANDED BODY DOES NOT PUSH THE HEADER OFF THE TOP. At 40 rows the
    // 20-line body scrolled `⏺ !cargo test` out of view and the assertion below failed on a
    // correct render - a test that measures the terminal size instead of the behaviour.
    let collapsed = rendered_frame_at_size(&app, Instant::now(), 100, 60);
    assert!(collapsed.contains("⏺ !cargo test"));
    assert!(
        !collapsed.contains("hidden tool body"),
        "the first line is 29 rows above the tail and must not be shown:\n{collapsed}"
    );
    assert!(
        !collapsed.contains("line 19 of the run"),
        "a collapsed call is one row and must draw no output:\n{collapsed}"
    );
    let target = app.tool_click_targets.borrow()[0];

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert!(matches!(
        app.transcript.first(),
        Some(TranscriptEntry::Tool {
            expanded: false,
            ..
        })
    ));

    handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.area.x,
            row: target.area.y,
            modifiers: KeyModifiers::NONE,
        },
    );
    let expanded = rendered_frame_at_size(&app, Instant::now(), 100, 60);
    assert!(expanded.contains("⏺ !cargo test"));
    assert!(
        !expanded.contains("hidden tool body"),
        "the first of twenty lines is above the tail and must not be drawn:\n{expanded}"
    );
    assert!(
        expanded.contains("line 19 of the run"),
        "the expanded row must show the END of the output:\n{expanded}"
    );
    assert!(
        expanded.contains("8 lines hidden"),
        "what is hidden must be counted:\n{expanded}"
    );
}

/// 🔴 THIS TEST USED TO PIN THE DEFECT.
///
/// It asserted `Theme::Dark.semantic() == #65A8FF` — a value whose own comment called it
/// *"Claude-like semantic blue"*, in no palette this product ships, painting the file paths that
/// lead back to the user's own code. A test that pins a hardcoded colour is a test that makes the
/// colour permanent; the gallery counted it at 17 cells on `01b-waiting-answer` and nothing was
/// going to go red about it, because this was green.
///
/// It asserts the RELATIONSHIP now: whatever `cite` is, `semantic` is that. The next person to
/// retune the citation blue changes one value and this stays green, which is what a test about an
/// owner should do.
#[test]
fn the_semantic_role_is_the_palettes_own_cite_token() {
    for theme in [Theme::Dark, Theme::CreamInk] {
        let expected = theme.screen_palette().cite;
        assert_eq!(theme.semantic(), expected);
        let rendered = render_transcript_with_citations(
            &[TranscriptEntry::System(
                "open file src/main.rs and run /verify".to_string(),
            )],
            true,
            theme,
            100,
        );
        let semantic_spans = rendered
            .text
            .lines
            .iter()
            .flat_map(|line| &line.spans)
            .filter(|span| span.content.contains("src/main.rs") || span.content.contains("/verify"))
            .collect::<Vec<_>>();
        assert_eq!(semantic_spans.len(), 2);
        assert!(
            semantic_spans
                .iter()
                .all(|span| span.style.fg == Some(expected))
        );
    }
}

#[test]
fn normal_session_frame_has_no_context_percentage_bar() {
    let frame = rendered_frame_at_size(&test_app(), Instant::now(), 100, 24);
    assert!(!frame.contains("context left"));
    assert!(!frame.contains("% context"));
}

#[tokio::test]
async fn compact_blocker_crosses_the_http_client_renderer_seam_without_replacing_history() {
    let server = MockServer::start().await;
    let source = json!([{"role": "user", "content": "keep this turn"}]);
    Mock::given(method("POST"))
        .and(path("/govern"))
        .and(body_json(json!({
            "messages": source,
            "session_id": "main",
            "generation": 2,
            "task": "",
            "model": "",
            "compact": true,
            "force": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "governed": source,
            "compaction": {
                "status": "blocked",
                "reason": "latest_turn_exceeds_usable_window",
                "generation_before": 2,
                "generation_after": 2
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut app = test_app();
    app.client = Some(
        Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("client"),
    );
    app.auth_resolved = true;
    app.compaction_generations.insert("main".to_string(), 2);
    app.transcript
        .push(TranscriptEntry::User("keep this turn".to_string()));
    let (tx, mut rx) = mpsc::unbounded_channel();

    app.submit("/compact".to_string(), &tx);
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("govern response deadline")
        .expect("govern response");
    app.handle_ui_event(event, &tx);

    assert_eq!(app.compaction_generations.get("main"), Some(&2));
    let rendered = format!("{:?}", render_transcript(&app.transcript));
    assert!(rendered.contains("compact BLOCKED  latest_turn_exceeds_usable_window"));
    assert!(rendered.contains("keep this turn"));
}

#[tokio::test]
async fn compact_refusal_that_mutates_source_goes_red_and_does_not_advance_generation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/govern"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "governed": [{"role": "user", "content": "changed by refusal"}],
            "compaction": {
                "status": "blocked",
                "reason": "latest_turn_exceeds_usable_window",
                "generation_before": 2,
                "generation_after": 2
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut app = test_app();
    app.client = Some(
        Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("client"),
    );
    app.auth_resolved = true;
    app.compaction_generations.insert("main".to_string(), 2);
    app.transcript
        .push(TranscriptEntry::User("keep this turn".to_string()));
    let (tx, mut rx) = mpsc::unbounded_channel();

    app.submit("/compact".to_string(), &tx);
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("govern response deadline")
        .expect("govern response");
    app.handle_ui_event(event, &tx);

    assert_eq!(app.compaction_generations.get("main"), Some(&2));
    let rendered = format!("{:?}", render_transcript(&app.transcript));
    assert!(rendered.contains("Compaction refusal changed the active transcript"));
    assert!(!rendered.contains("compact BLOCKED  latest_turn_exceeds_usable_window"));
}

/// 🔴 **SCREEN 34's GAP WAS NAMED WRONG IN ONE OF ITS TWO HALVES, AND THIS IS THE HALF THAT SHIPS.**
///
/// `design_book::SCREENS` said `34-answer-table-diagram` needs *"no markdown table or diagram
/// renderer in this client"*. The table half is false and has been since the renderer was vendored:
/// `markdown_render.rs` computes column widths by content-priority class, honours `:---` / `---:` /
/// `:---:`, wraps rather than truncates an over-wide cell, transposes to key/value records when the
/// grid stops being scannable, and separates the header with `━` (`markdown_render.rs:87-88`,
/// `:1241`, `:1500-1503`). The answer path reaches it: `TranscriptEntry::Answer` →
/// `HistoryTranscriptItem::Markdown` → `AgentMarkdownCell` →
/// `markdown::render_markdown_agent_with_links_cwd_and_visualizations`.
///
/// ⚠️ **A GAP STATEMENT IS A CLAIM, AND A CLAIM NEEDS AN INSTRUMENT.** Nobody had ever asserted the
/// table renders, which is exactly why the line could say it does not for as long as it did. This
/// is that instrument: break the pipeline anywhere along it and this goes red.
#[test]
fn an_answer_that_carries_a_markdown_table_renders_a_table() {
    let mut app = test_app();
    app.transcript.push(TranscriptEntry::Answer {
        text: "| call site | file:line | retries |\n| --- | --- | ---: |\n| charge_card | billing/charge.rs:82 | 3 |\n| receipt_writer | billing/receipt.rs:17 | 0 |".to_string(),
        grounded: Some(true),
        degraded: false,
        sources: Vec::new(),
    });
    let frame = rendered_frame_at_size(&app, Instant::now(), 100, 24);
    assert!(frame.contains("call site"), "header missing:\n{frame}");
    assert!(
        frame.contains('━'),
        "the header separator is the table's own texture:\n{frame}"
    );
    // The pipe syntax must be GONE — its survival is what "printed, not rendered" looks like.
    assert!(
        !frame.contains("| --- |"),
        "the delimiter row was printed rather than rendered:\n{frame}"
    );
    // A right-aligned column is the half a naive renderer drops. `3` and `0` sit under the `s`
    // of `retries`, not at the left edge of the cell.
    let retries = frame
        .lines()
        .find(|line| line.contains("retries"))
        .and_then(|line| line.find("retries"))
        .expect("the retries header");
    for row in ["charge_card", "receipt_writer"] {
        let line = frame
            .lines()
            .find(|line| line.contains(row))
            .unwrap_or_else(|| panic!("row {row} missing:\n{frame}"));
        let digit = line
            .rfind(['0', '3'])
            .unwrap_or_else(|| panic!("no count on {row}:\n{frame}"));
        assert!(
            digit >= retries,
            "{row}'s count is left-aligned under a `---:` column:\n{frame}"
        );
    }
}

/// 🔴 **AND THIS IS THE HALF THAT DOES NOT SHIP — ASSERTED, SO THE GAP CANNOT QUIETLY CLOSE OR
/// QUIETLY WIDEN.**
///
/// There is no diagram renderer in this client. Measured 2026-09-02: no `ratatui-image`, no
/// resvg/usvg, no kitty/sixel emission anywhere in the crate. The two rivals that solve it
/// disagree, and BOTH answers are refused here for a stated reason:
///
/// - **jcode** parses and lays out the diagram, rasterises to PNG and emits it over a real terminal
///   image protocol (`jcode-tui-mermaid/src/lib.rs:1-5`), explicitly refusing a half-block fallback
///   because *"source is more useful than a degraded diagram"* (`mermaid_runtime.rs:417-427`). The
///   layout half is a 58k-line dependency.
/// - **oh-my-pi** draws it as character art
///   (`packages/utils/src/vendor/mermaid-ascii/`) — and its canvas is built from `┌ ┐ └ ┘ ├ ┤`
///   node boxes (`ascii/draw.ts:171-176`) that MERGE into `┼` where edges cross
///   (`ascii/canvas.ts:217-226`). **Every one of those glyphs is on this repo's no-box list**, which
///   the founder has stated five times and `BOX_CORNERS` enforces over every gallery frame. Porting
///   it is not a cost trade-off; it is a house rule.
///
/// ▶ **SO THE FENCE PRINTS ITS SOURCE, WHICH IS THE ANSWER BOTH RIVALS AGREE IS CORRECT WHEN THE
/// PICTURE CANNOT BE DRAWN HONESTLY.** That is a decision for the founder to overturn, not for this
/// file to guess at, and it is asserted here so nobody mistakes it for an oversight.
#[test]
fn a_fenced_diagram_prints_its_source_because_nothing_draws_one() {
    let mut app = test_app();
    app.transcript.push(TranscriptEntry::Answer {
        text: "```mermaid\nflowchart LR\n  charge_card --> retry_gate\n```".to_string(),
        grounded: Some(true),
        degraded: false,
        sources: Vec::new(),
    });
    let frame = rendered_frame_at_size(&app, Instant::now(), 100, 24);
    assert!(
        frame.contains("charge_card --> retry_gate"),
        "the source is what a reader gets, and it must survive verbatim:\n{frame}"
    );
}
