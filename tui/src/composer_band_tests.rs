//! The user turn's highlight band, asserted on the RENDERED BUFFER.
//!
//! 🔴 **THE SPEC SAID ONE BAND AND THE BUFFER DREW THREE.** The founder photographed session-home
//! with `What changed while I was away?` on it: a lit strip above his line and another below it.
//! Reading `transcript.rs` would not have found it — the entry it builds carries ONE background
//! and no padding. The extra rows come from [`crate::history_cell`]'s own cell, which frames every
//! user turn with a blank row at each end, and from the re-band in
//! `public_widgets::history_transcript` lifting those blanks too. **Two files each doing something
//! reasonable; a band nobody wrote.** That is why this asserts on cells, exactly as the tab-strip
//! gutters had to be measured rather than read off the six hand-typed widths that produced them.

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

/// Every maximal run of consecutive rows carrying `fill` on at least one cell.
fn bands(buffer: &ratatui::buffer::Buffer, fill: ratatui::style::Color) -> Vec<(u16, u16)> {
    let mut bands: Vec<(u16, u16)> = Vec::new();
    for y in 0..buffer.area.height {
        let lit = (0..buffer.area.width).any(|x| buffer[(x, y)].bg == fill);
        if !lit {
            continue;
        }
        match bands.last_mut() {
            Some((_, end)) if *end + 1 == y => *end = y,
            _ => bands.push((y, y)),
        }
    }
    bands
}

fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// What the band itself carries on row `y` — the cells that are LIT, and nothing else.
///
/// ⚠️ **THE WHOLE ROW IS THE WRONG SUBJECT.** The band is the width of the session column; the
/// production rail draws its own text on the same terminal rows. Reading the whole row therefore
/// finds `Live Monitor unavailable.` on a row the band lit blank, and an assertion that a lit row
/// "carries text" then passes on the very rows the founder photographed as empty strips.
fn lit_text(buffer: &ratatui::buffer::Buffer, y: u16, fill: ratatui::style::Color) -> String {
    (0..buffer.area.width)
        .filter(|x| buffer[(*x, y)].bg == fill)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

/// The state the founder screenshotted: one question asked, nothing back yet.
fn asked() -> App {
    let mut app = test_app();
    app.auth_resolved = true;
    let (tx, _rx) = mpsc::unbounded_channel();
    app.submit("What changed while I was away?".to_string(), &tx);
    app
}

/// 🔴 ONE BAND, AND EVERY ROW OF IT CARRIES TEXT.
///
/// ⚠️ The second clause is the one that catches this defect. "Exactly one band" passes on the
/// broken frame too — the three lit rows are consecutive, so they group as one run. What the
/// founder saw is a band whose FIRST and LAST rows are blank, and only a check on the rows'
/// CONTENT can tell a three-row band around one line from a one-row band on it.
#[test]
fn the_user_turn_is_one_band_and_no_blank_row_is_lit() {
    let app = asked();
    let tint = app.theme.screen_palette().tint;
    let buffer = buffer_at(&app, 160, 38);

    let bands = bands(&buffer, tint);
    assert_eq!(
        bands.len(),
        1,
        "expected one lifted band, found {bands:?}\n{}",
        (0..buffer.area.height)
            .map(|y| format!("{y:>2} {}\n", row_text(&buffer, y)))
            .collect::<String>()
    );

    let (top, bottom) = bands[0];
    for y in top..=bottom {
        assert!(
            !lit_text(&buffer, y, tint).trim().is_empty(),
            "row {y} is lit and blank — the band runs past the message ({top}..={bottom})\n{}",
            (0..buffer.area.height)
                .map(|row| format!("{row:>2} {}\n", row_text(&buffer, row)))
                .collect::<String>()
        );
    }
    assert!(
        lit_text(&buffer, top, tint).contains("What changed while I was away?"),
        "the lit band is not the message: {:?}",
        lit_text(&buffer, top, tint)
    );
}

/// The negative control. Without it the assertion above passes on a frame that lifts NOTHING —
/// `bands.len() == 1` would fail, but a reader could weaken it to `<= 1` and never notice that the
/// highlight the founder asked for had been deleted rather than trimmed.
#[test]
fn a_frame_with_no_user_turn_lifts_no_row_at_all() {
    let mut app = test_app();
    app.auth_resolved = true;
    let tint = app.theme.screen_palette().tint;
    assert_eq!(bands(&buffer_at(&app, 160, 38), tint), Vec::new());
}
