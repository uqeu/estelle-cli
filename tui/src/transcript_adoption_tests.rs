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

#[test]
fn tool_output_stays_collapsed_until_its_exact_row_is_clicked() {
    let mut app = test_app();
    app.transcript.push(TranscriptEntry::Tool {
        label: "!cargo test".to_string(),
        lines: vec!["hidden tool body".to_string()],
        expanded: false,
    });

    let collapsed = rendered_frame_at_size(&app, Instant::now(), 100, 24);
    assert!(collapsed.contains("⏺ !cargo test"));
    assert!(!collapsed.contains("hidden tool body"));
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
    let expanded = rendered_frame_at_size(&app, Instant::now(), 100, 24);
    assert!(expanded.contains("⏺ !cargo test"));
    assert!(expanded.contains("hidden tool body"));
}

#[test]
fn semantic_blue_is_applied_in_both_supported_themes() {
    for (theme, expected) in [
        (Theme::Dark, Color::from_u32(0x65_A8_FF)),
        (Theme::CreamInk, Color::from_u32(0x1F_5A_A6)),
    ] {
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
