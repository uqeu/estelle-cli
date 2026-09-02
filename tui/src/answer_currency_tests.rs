//! The staleness verdict, asserted on the RENDERED FRAME.
//!
//! Screen 10 of the design book declared `no CLI surface reads the staleness verdict yet`. The
//! server had been producing one: `serve/answer_currency.py` attaches a `code_currency` block to
//! `/memory/chat` when the index is behind the tree AND the answer leans on the code. These tests
//! are what makes that claim checkable from this side.

use super::*;

use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn test_app() -> App {
    let mut app = App::new(Args {
        command: None,
        repo: Some("uqeu/estelle".to_string()),
    });
    app.boot = None;
    app
}

fn buffer_at(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let now = Instant::now();
    terminal
        .draw(|frame| render_frame(frame, app, now))
        .expect("render frame");
    terminal.backend().buffer().clone()
}

fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// A decertified answer, exactly as `serve/answer_currency.py` composes one.
fn decertified() -> estelle_client::CodeCurrency {
    estelle_client::CodeCurrency {
        status: "stale".to_string(),
        indexed_head: "6ff03b1857ab4c0d9e21".to_string(),
        current_head: "75557c7f11ab2e0044aa".to_string(),
        depends_on_code: "certified_code_claim".to_string(),
        cited_paths: 1,
        detail: "uqeu/estelle: STALE — indexed at 6ff03b1857ab, repo is now 75557c7f11ab. \
                 Real code added since then reads as invented. Re-sweep/reindex this repo to \
                 advance the marker, then retry."
            .to_string(),
    }
}

fn asked_and_answered(currency: Option<estelle_client::CodeCurrency>) -> App {
    let mut app = test_app();
    app.auth_resolved = true;
    app.has_submitted_question = true;
    app.transcript
        .push(TranscriptEntry::User("where is charge_card?".to_string()));
    if let Some(currency) = currency {
        app.transcript.push(TranscriptEntry::Stale(currency));
    }
    app.transcript.push(TranscriptEntry::Answer {
        text: "charge_card lives in billing/charge.rs.".to_string(),
        grounded: Some(false),
        degraded: false,
        sources: vec![Source {
            file: "billing/charge.rs".to_string(),
            line: Some(82),
            extra: serde_json::Map::new(),
        }],
    });
    app
}

fn frame_text(app: &App, width: u16, height: u16) -> String {
    let buffer = buffer_at(app, width, height);
    (0..buffer.area.height)
        .map(|y| format!("{}\n", row_text(&buffer, y)))
        .collect()
}

/// 🔴 SCREEN 10 IS WIRED: THE STALENESS VERDICT REACHES A CLI SURFACE.
///
/// Both SHAs, in the server's own vocabulary, ABOVE the answer they are about. Asserted on the
/// rendered frame rather than on the entry, because an entry that never reaches a cell is a
/// verdict nobody reads.
#[test]
fn the_staleness_verdict_leads_the_answer_it_decertifies() {
    let app = asked_and_answered(Some(decertified()));
    let rendered = frame_text(&app, 160, 40);

    assert!(
        rendered.contains("Index is behind your tree"),
        "no staleness headline:\n{rendered}"
    );
    assert!(
        rendered.contains("STALE — indexed at 6ff03b1857ab, repo is now 75557c7f11ab"),
        "the two heads are not both on the wire:\n{rendered}"
    );
    assert!(
        rendered.contains("Re-sweep/reindex this repo"),
        "the server's remedy did not reach the screen:\n{rendered}"
    );

    let verdict = rendered
        .lines()
        .position(|line| line.contains("Index is behind your tree"))
        .expect("the headline is on screen");
    let answer = rendered
        .lines()
        .position(|line| line.contains("charge_card lives in"))
        .expect("the answer is on screen");
    assert!(
        verdict < answer,
        "the disclosure must lead the answer, got verdict at {verdict} and answer at {answer}"
    );
}

/// The negative control, and the one that keeps the wiring honest.
///
/// The server sends the block ONLY when it has withdrawn certification. A CLI that drew a currency
/// row on every answer would be reporting a verdict nobody reached — the same defect as a `0` that
/// means "not measured". The healthy frame must be free of every word the band uses.
#[test]
fn a_current_index_draws_no_currency_row_at_all() {
    let rendered = frame_text(&asked_and_answered(None), 160, 40);
    for forbidden in ["Index is behind", "STALE", "indexed at", "Re-sweep"] {
        assert!(
            !rendered.contains(forbidden),
            "a healthy answer drew {forbidden:?}:\n{rendered}"
        );
    }
    assert!(rendered.contains("charge_card lives in"), "{rendered}");
}

/// 🔴 THE HOP THE RENDER TEST ABOVE CANNOT SEE.
///
/// `the_staleness_verdict_leads_the_answer_it_decertifies` builds the transcript entry by hand, so
/// it would stay green if `push_answer_reply` dropped the block on the floor — the outer layer
/// reporting success over an inner layer that never ran. This drives the real reply path.
///
/// ⚠️ **THE LIMIT, SAID OUT LOUD:** the hop from `deep_search`'s response into `AnswerReply` is one
/// field copy in `answer_research_question`, and nothing here exercises it. What covers that half
/// is `estelle-client`'s parse test plus a reading of the assignment; a mock-server round trip
/// would close it and has not been written.
#[test]
fn the_reply_path_carries_the_currency_block_into_the_transcript() {
    let mut app = test_app();
    app.auth_resolved = true;
    app.has_submitted_question = true;
    app.transcript
        .push(TranscriptEntry::User("where is charge_card?".to_string()));
    app.push_answer_reply(AnswerReply {
        text: "charge_card lives in billing/charge.rs.".to_string(),
        grounded: Some(false),
        degraded: false,
        sources: Vec::new(),
        working_paths: Vec::new(),
        code_currency: Some(decertified()),
    });

    let rendered = frame_text(&app, 160, 40);
    assert!(
        rendered.contains("STALE — indexed at 6ff03b1857ab, repo is now 75557c7f11ab"),
        "the reply path dropped the block:\n{rendered}"
    );
}
