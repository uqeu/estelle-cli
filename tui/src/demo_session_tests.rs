//! The session player's guards, in their own file.
//!
//! ⚠️ **SPLIT OUT BECAUSE `demo_session.rs` WOULD OTHERWISE BE OVER THE 800-LINE HOUSE LIMIT.** It
//! is attached with `#[path]` as a CHILD of `demo_session` rather than declared as a sibling in
//! `main.rs`, which is the difference that matters: a sibling could only reach `pub(crate)` items,
//! and every guard here asserts on the player's PRIVATE machinery — the cue sheet, the cues, the
//! frame. Widening `Cue` to `pub(crate)` to make a test compile would put the transcript within
//! reach of code that is not allowed to touch it.
//!
//! 🔴 **THE FRAME TESTS RENDER THROUGH `live_renderer::render_frame`, NOT THROUGH A HELPER OF OUR
//! OWN.** That is the whole correction this rewrite makes. A test that asserted on a frame this
//! module composed would have passed on every one of the seven defects the founder reported, because
//! all seven were in the composing.

use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// The width the founder records at. Comfortably over `session_view::DESIGN_WIDTH` (81), which is
/// the threshold below which the two-pane split — and therefore the rail — is dropped.
const WIDE: u16 = 150;
/// The session pane the guards lay their tables out against — the same value `run` measures
/// off a 150-column terminal, so a test frame and a recorded frame wrap identically.
const PANE: usize = 88;
const TALL: u16 = 44;

/// Build the film's app exactly as [`run`] does, minus the terminal.
fn film_app(film: &'static Film) -> App {
    let mut app = App::new(Args {
        command: None,
        repo: Some(film.repo.to_string()),
    });
    app.branch = Some(film.branch.to_string());
    app.theme = Theme::Dark;
    script::dress(&mut app, film, true);
    // The boot scene owns the frame until its own clock finishes; these guards are about the
    // SESSION, so they start after it, exactly as the run loop does.
    app.boot = None;
    app
}

/// Play a film forward to `at_ms` and render the real frame at the default recording width.
fn frame_at(film: &'static Film, at_ms: u32, fixtures: bool) -> String {
    frame_at_width(film, at_ms, fixtures, WIDE).0
}

/// The rendered frame AND the buffer's own reported width.
///
/// 🔴 **THE WIDTH COMES BACK OUT OF THE BUFFER, NOT OUT OF THE ARGUMENT.** Asserting against the
/// constant you passed in proves the test called the renderer, never that the renderer used it —
/// the same discipline that caught the tab-strip gutters and the triple-banded composer.
fn frame_at_width(film: &'static Film, at_ms: u32, fixtures: bool, width: u16) -> (String, u16) {
    let mut app = film_app(film);
    if !fixtures {
        app = {
            let mut bare = App::new(Args {
                command: None,
                repo: Some(film.repo.to_string()),
            });
            bare.branch = Some(film.branch.to_string());
            script::dress(&mut bare, film, false);
            bare.boot = None;
            bare
        };
    }
    let now = Instant::now();
    let mut paintings = Vec::new();
    for step in cue_sheet(film, fixtures, PANE) {
        if step.at_ms > at_ms {
            break;
        }
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    let mut terminal = Terminal::new(TestBackend::new(width, TALL)).expect("test terminal");
    terminal
        .draw(|frame| live_renderer::render_frame(frame, &app, now))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let text = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (text, buffer.area.width)
}

/// The session column of a rendered row, cut at the DIVIDER'S COLUMN.
///
/// ⚠️ **SPLITTING ON THE `\u{2502}` CHARACTER WAS WRONG AND IT HID REAL ROWS.** The divider is that
/// glyph, but so is `gate_refusal`'s own blocker marker — so a `split('\u{2502}').next()` on a
/// blocker row returned the empty string before the marker, and every blocker line vanished from
/// the guards and from the frames I was reading. The divider is a POSITION, and
/// `session_view::split` is the one owner of it.
fn session_of(row: &str, width: u16) -> String {
    let cut = crate::session_view::split(width)
        .map_or(usize::from(width), |columns| columns[0].w + columns[0].gap);
    row.chars()
        .take(cut)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// 🔴 **THE ONE PROPERTY THAT SEPARATES A SESSION FROM THE GALLERY HE REJECTED.**
///
/// Every cue of every film is applied in order and the transcript's length is asserted to be
/// monotonic. If anyone ever reaches for `clear()` to "start the next beat cleanly", this is red
/// before the footage is shot.
///
/// ⚠️ The vacuity half is asserted too: a transcript that never grew would also never shrink, so
/// the final length is required to be substantial.
#[test]
fn the_transcript_only_ever_grows() {
    for film in script::FILMS {
        let mut app = film_app(film);
        let now = Instant::now();
        let mut paintings = Vec::new();
        let mut high_water = 0usize;
        for step in cue_sheet(film, true, PANE) {
            apply(&step.cue, &mut app, now, &mut paintings);
            assert!(
                app.transcript.len() >= high_water,
                "film {} reset its transcript at {} ms",
                film.number,
                step.at_ms
            );
            high_water = app.transcript.len();
        }
        assert!(
            high_water > 20,
            "film {} only ever wrote {high_water} entries — the growth check proves nothing",
            film.number
        );
    }
}

/// 🔴 **THE RIGHT-HAND SIDE IS THERE, AND IT HAS SOMETHING ON IT.**
///
/// His words were *"the right side is completely gone"*. The rail is permanent in the design and
/// needs only a wide enough terminal, so this asserts BOTH halves: the divider column exists (the
/// split happened) and the rail's own band rules are on the frame (the split has content). Asserting
/// only the divider would pass over a rail rendering five empty rules, which is what he saw.
#[test]
fn the_production_rail_is_on_every_frame_and_is_not_empty() {
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true, PANE));
    for fraction in [0, 2, 4, 6, 7] {
        let at = total * fraction / 8;
        let frame = frame_at(film, at, true);
        assert!(
            frame.contains('\u{2502}'),
            "no divider column at {at} ms — the two-pane split did not happen:\n{frame}"
        );
        for band in [
            "production",
            "app",
            "services",
            "agents",
            "estelle",
            "queue",
            "github",
        ] {
            assert!(
                frame.contains(band),
                "the rail has no {band} band at {at} ms:\n{frame}"
            );
        }
        // The louder failure: a rail that rendered before `dress` ran says this instead.
        assert!(
            !frame.contains("Live Monitor unavailable"),
            "the rail is undressed at {at} ms — `dress` did not set a client:\n{frame}"
        );
    }
}

/// 🔴 **THE FRAME FILLS WHATEVER TERMINAL HE FILMS IN, AND DEGRADES INSTEAD OF TRUNCATING.**
///
/// His first and loudest complaint: *"Why is it cut off on the right? The entire TUI doesn't even go
/// to the right side."* The old player read `terminal.size()?.height` and **never touched the
/// width**, so it laid out against a width it had assumed. This module now reads NEITHER dimension —
/// `render_frame` lays out against `frame.area()` — and that is exactly what makes the property
/// testable at more than one size.
///
/// Swept across the widths he could plausibly record at, plus the two below the design's threshold:
/// * **width ≥ 81** (`session_view::DESIGN_WIDTH`) — the two-pane split holds and the rail is there;
/// * **width < 81** — the rail is DROPPED and the session takes the whole frame, which is the
///   design's own degradation, not a truncation.
///
/// ⚠️ Every assertion reads the BUFFER's own `area.width`, never the constant passed in.
#[test]
fn the_frame_fills_every_width_and_degrades_below_the_split_threshold() {
    let film = script::film(1).expect("film 1");
    let at = runtime_ms(&cue_sheet(film, true, PANE)) / 2;
    // `session_view::DESIGN_WIDTH` is 81: the narrowest frame that can hold a 46-column session,
    // a divider with its two gaps, and a 30-column rail.
    for width in [200u16, 160, 150, 120, 100, 81, 80, 70] {
        let (frame, measured) = frame_at_width(film, at, true, width);
        assert_eq!(
            measured, width,
            "the backend did not render at the width it was given"
        );
        let rightmost = frame
            .lines()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        assert!(
            rightmost >= usize::from(measured) - 4,
            "at {measured} columns the widest row is {rightmost} — the frame is cut off on the right"
        );
        assert!(
            rightmost <= usize::from(measured),
            "at {measured} columns a row is {rightmost} wide — the frame overruns the terminal"
        );
        let split = frame.contains('\u{2502}');
        if width >= 81 {
            assert!(split, "no two-pane split at {measured} columns:\n{frame}");
            assert!(
                frame.contains("production"),
                "the rail is missing at {measured} columns:\n{frame}"
            );
        } else {
            // Below the threshold the design drops the rail rather than squeezing it. A frame that
            // still drew a divider here would be the truncation he complained about.
            assert!(
                !split,
                "at {measured} columns the rail should be dropped, not squeezed:\n{frame}"
            );
        }
    }
}

/// 🔴 **HIS WORDS AND ESTELLE'S ARE TOLD APART BY THE TINT BAND.**
///
/// *"It's impossible to tell who sent the message."* The band is the product's own device, fixed by
/// a sibling lane in `history_transcript::band_the_message_only`, and the film gets it by pushing a
/// real `TranscriptEntry::User` rather than by drawing a prompt row. This asserts on the BUFFER's
/// backgrounds, because the band is a colour and a text assertion cannot see it.
///
/// ⚠️ It reads the SESSION column only. The production rail draws its own text on the same terminal
/// rows, so a whole-row read finds rail content on a row the band lit blank — the same trap the
/// sibling lane documented when they fixed it.
#[test]
fn the_users_own_turn_is_the_only_thing_wearing_the_band() {
    let film = script::film(1).expect("film 1");
    let mut app = film_app(film);
    let now = Instant::now();
    let mut paintings = Vec::new();
    // Far enough in that a user turn and a reply are both on screen.
    for step in cue_sheet(film, true, PANE) {
        if step.at_ms > 20_000 {
            break;
        }
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    let mut terminal = Terminal::new(TestBackend::new(WIDE, TALL)).expect("test terminal");
    terminal
        .draw(|frame| live_renderer::render_frame(frame, &app, now))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();

    let tint = Theme::Dark.screen_palette().tint;
    let banded: Vec<u16> = (0..buffer.area.height)
        .filter(|y| {
            // Session column only — the rail starts well past column 60 on a 150-wide frame.
            (2..60).any(|x| buffer[(x, *y)].style().bg == Some(tint))
        })
        .collect();
    assert!(
        !banded.is_empty(),
        "no row wears the user band — his message is indistinguishable from Estelle's"
    );
    // Every banded row carries text. "Exactly one band" passes on the broken frame too, since the
    // three lit rows there were consecutive; what separates them is whether a lit row is blank.
    for y in &banded {
        let text: String = (2..60).map(|x| buffer[(x, *y)].symbol()).collect();
        assert!(
            !text.trim().is_empty(),
            "row {y} is lit but blank — the band is painted over its own padding again"
        );
    }
}

/// 🔴 **THE ANSWER STREAMS INSTEAD OF LANDING WHOLE.**
///
/// *"It just loaded in all at once... it's like an entire section at a time."* A reply is planned as
/// an open plus many small grows, so this asserts the answer's LENGTH takes many distinct values
/// over its beat. A block that appeared whole would take exactly two: empty, then final.
#[test]
fn a_reply_arrives_in_many_pieces_rather_than_all_at_once() {
    let film = script::film(1).expect("film 1");
    let mut app = film_app(film);
    let now = Instant::now();
    let mut paintings = Vec::new();
    let mut lengths = std::collections::BTreeSet::new();
    for step in cue_sheet(film, true, PANE) {
        apply(&step.cue, &mut app, now, &mut paintings);
        if let Some(TranscriptEntry::Answer { text, .. }) = app.transcript.last() {
            lengths.insert(text.len());
        }
    }
    assert!(
        lengths.len() > 12,
        "the last answer took only {} distinct lengths — it is landing as a block",
        lengths.len()
    );
}

/// Every film fits the bound, and none is so short it cannot be talked over.
#[test]
fn every_film_is_bounded_and_long_enough_to_talk_over() {
    for film in script::FILMS {
        let ms = runtime_ms(&cue_sheet(film, true, PANE));
        assert!(
            ms < session::MAX_FILM_MS,
            "film {} runs {ms} ms, over the {} ms bound",
            film.number,
            session::MAX_FILM_MS
        );
        // ⚠️ **A FLOOR, NEVER A TARGET.** Runtime is now decided by the arc; this only catches a
        // film that lost most of its beats to a bad edit. The founder's note that killed the
        // ceiling applies here too: long is not the goal, SUBSTANCE is, and a short film is a
        // symptom of missing content rather than a thing to pad.
        assert!(
            ms > 90_000,
            "film {} runs {ms} ms — it has lost most of its beats",
            film.number
        );
    }
}

/// The cue sheet is ordered in time. The run loop walks it with one index and never sorts, so an
/// out-of-order step would silently be applied late — or never.
#[test]
fn the_cue_sheet_is_monotonic_in_time() {
    for film in script::FILMS {
        for pair in cue_sheet(film, true, PANE).windows(2) {
            assert!(
                pair[0].at_ms <= pair[1].at_ms,
                "film {} plans a cue backwards",
                film.number
            );
        }
    }
}

/// 🔴 Typing is UNEVEN. A film whose keystroke gaps were all the same would read as a machine.
///
/// ⚠️ Asserted as a SPREAD, not as "jitter was called": a jitter function wired to a constant would
/// pass the latter and fail this.
#[test]
fn keystrokes_are_not_evenly_spaced() {
    let film = script::film(1).expect("film 1");
    let mut gaps = Vec::new();
    let mut previous: Option<u32> = None;
    for step in cue_sheet(film, true, PANE) {
        if matches!(step.cue, Cue::Compose(_)) {
            if let Some(previous) = previous {
                gaps.push(step.at_ms.saturating_sub(previous));
            }
            previous = Some(step.at_ms);
        } else {
            previous = None;
        }
    }
    assert!(
        gaps.len() > 80,
        "not enough keystrokes to measure: {}",
        gaps.len()
    );
    let distinct = gaps.iter().collect::<std::collections::BTreeSet<_>>().len();
    assert!(
        distinct > 25,
        "only {distinct} distinct keystroke gaps — the typing is uniform"
    );
}

/// The backspaces are in the footage. A scripted stumble that produced no shrinking composer would
/// be a stumble nobody can see.
#[test]
fn a_scripted_stumble_shows_the_composer_getting_shorter() {
    let film = script::film(1).expect("film 1");
    let mut shrinks = 0usize;
    let mut previous = 0usize;
    for step in cue_sheet(film, true, PANE) {
        if let Cue::Compose(text) = &step.cue {
            if text.chars().count() < previous {
                shrinks += 1;
            }
            previous = text.chars().count();
        } else {
            previous = 0;
        }
    }
    assert!(
        shrinks >= 10,
        "film 1 shows {shrinks} backspaces — a person at the keyboard makes more visible"
    );
}

/// What lands in the composer is the CORRECTED sentence — the stumble is typed and taken back, so
/// the submitted turn must not carry it.
#[test]
fn the_submitted_turn_does_not_carry_the_typo() {
    let film = script::film(1).expect("film 1");
    let submitted: Vec<String> = cue_sheet(film, true, PANE)
        .into_iter()
        .filter_map(|step| match step.cue {
            Cue::Submit(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(submitted.len(), film.beats.len());
    assert!(
        submitted[0].contains("stripe migration"),
        "{:?}",
        submitted[0]
    );
    assert!(!submitted[0].contains("migraton"), "the typo was submitted");
    assert!(!submitted[2].contains("teh"), "the typo was submitted");
}

/// Everything the film ever put in the transcript, as one string.
///
/// ⚠️ **THE GATE CHECK READS THIS, NOT A FRAME.** The first version rendered one frame at the end
/// of the film and looked for a needle from beat 1 — which had long scrolled out of the viewport,
/// so the POSITIVE CONTROL failed and would have been "fixed" by deleting it. A frame shows the
/// last screenful; the question "did fixture data reach the product at all" is a question about
/// everything that was ever written.
fn transcript_text(film: &'static Film, fixtures: bool) -> String {
    let mut app = film_app(film);
    if !fixtures {
        app = App::new(Args {
            command: None,
            repo: Some(film.repo.to_string()),
        });
        script::dress(&mut app, film, false);
    }
    let now = Instant::now();
    let mut paintings = Vec::new();
    for step in cue_sheet(film, fixtures, PANE) {
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    app.transcript
        .iter()
        .map(|entry| match entry {
            TranscriptEntry::User(text) | TranscriptEntry::System(text) => text.clone(),
            TranscriptEntry::Answer { text, .. } => text.clone(),

            TranscriptEntry::Failure(lines) => lines.join(" "),
            TranscriptEntry::Command { name, lines } => format!("{name} {}", lines.join(" ")),
            TranscriptEntry::SessionHandoff(lines) => lines.join(" "),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// With the fixture gate SHUT, no film shows a fixture number — and the session is still the real
/// app, with his own typed words on it.
///
/// ⚠️ Paired with its positive control in the same loop, because an absence assertion passes
/// identically over a player that drew nothing at all.
#[test]
fn a_film_cannot_draw_fixture_numbers_with_the_gate_shut() {
    let film = script::film(1).expect("film 1");
    let open = transcript_text(film, true);
    let shut = transcript_text(film, false);
    for needle in script::FIXTURE_NEEDLES {
        assert!(
            open.contains(needle),
            "film 1 drew nothing for {needle:?} with the gate OPEN — the check below proves nothing"
        );
        assert!(
            !shut.contains(needle),
            "film 1 leaked the fixture {needle:?} with the gate shut"
        );
    }
    // The frame is the product either way: his own typed words are still on it.
    assert!(shut.contains("stripe migration"), "{shut}");
}

/// 🔴 **NO FILM SAYS "SAVED", AND THIS READS THE RENDERED FRAMES RATHER THAN THE SCRIPT.**
///
/// `design_book/costing.rs` renders `saved $0.214 · affinity picked, you did not` against a
/// MODELLED all-opus baseline, on a surface `ESTELLE_DEMO_FIXTURES=1 estelle demo` shows customers.
/// The films run through the same fixture flag, so "does that string reach a film?" is a question
/// about OUTPUT, and the honest way to answer it is to look at the output.
///
/// ⚠️ **THE SCRIPT-LEVEL CHECK IN `script.rs` IS NOT ENOUGH ON ITS OWN.** It reads the words a film
/// writes; this reads the words a film DRAWS, which is a superset — anything a beat pulls in from
/// the book, from `gate_refusal`, or from a future `Say` variant lands here and nowhere else.
///
/// 🔴 The word is only allowed over a MEASURED counterfactual. A modelled baseline re-prices the
/// same tokens at another model's rate: a legitimate estimate and an illegitimate sentence, because
/// "saved" asserts a fact about money that never left his account.
#[test]
fn no_film_frame_ever_says_saved() {
    for film in script::FILMS {
        let total = runtime_ms(&cue_sheet(film, true, PANE));
        let mut inspected = 0usize;
        for fraction in 0..16 {
            let frame = frame_at(film, total * fraction / 16, true);
            inspected += frame.lines().count();
            for claim in ["saved", "savings", "you save"] {
                assert!(
                    !frame.to_ascii_lowercase().contains(claim),
                    "film {} draws {claim:?} \u{2014} only a MEASURED counterfactual may say it:\n{frame}",
                    film.number
                );
            }
        }
        assert!(
            inspected > 400,
            "only {inspected} rows inspected for film {} \u{2014} the guard proves nothing",
            film.number
        );
    }
}

/// 🔴 **ESTELLE SPEAKS WHILE THE CURSOR IS STILL IN HIS LINE.**
///
/// The founder asked for an interrupt MID-SENTENCE, not after enter. This walks film 3's cue sheet
/// and requires that at least one transcript cue fires at a moment when the composer holds a
/// NON-EMPTY, UNSUBMITTED line — which is exactly what "mid-sentence" means and what a cue firing
/// between beats would not satisfy.
#[test]
fn film_three_interrupts_while_he_is_still_typing() {
    let film = script::film(3).expect("film 3");
    let mut composer = String::new();
    let mut submitted_since_typing = true;
    let mut interrupted_mid_line = 0usize;
    for step in cue_sheet(film, true, PANE) {
        match &step.cue {
            Cue::Compose(text) => {
                composer = text.clone();
                submitted_since_typing = false;
            }
            Cue::Submit(_) => {
                composer.clear();
                submitted_since_typing = true;
            }
            // Anything that writes to the transcript while a half-typed line is on screen.
            Cue::Failure(_) | Cue::System(_) | Cue::OpenCommand(_) | Cue::OpenAnswer { .. } => {
                if !composer.is_empty() && !submitted_since_typing {
                    interrupted_mid_line += 1;
                }
            }
            _ => {}
        }
    }
    assert!(
        interrupted_mid_line > 0,
        "nothing reaches the transcript while he is mid-line \u{2014} the interrupt fires after enter"
    );
}

/// 🔴 **HIS SENTENCE COMES BACK, TO THE CHARACTER.**
///
/// ⚠️ **THIS IS THE HALF THAT SELLS THE INTERRUPT.** An interrupt that costs him his sentence is an
/// interruption; one that gives the sentence back is an assistant. So the guard is not "a Restore
/// cue exists" — it is that the text which reappears in the composer is BYTE-IDENTICAL to the text
/// that was parked, and that he then finishes it and sends the whole thing.
#[test]
fn the_parked_line_comes_back_exactly_as_he_left_it() {
    let film = script::film(3).expect("film 3");
    let mut composer = String::new();
    let mut parked: Option<String> = None;
    let mut restored: Option<String> = None;
    let mut last_submit = String::new();
    for step in cue_sheet(film, true, PANE) {
        match &step.cue {
            Cue::Compose(text) => {
                // The park empties the composer mid-sentence.
                if text.is_empty() && composer.chars().count() > 40 && parked.is_none() {
                    parked = Some(composer.clone());
                }
                // ⚠️ THE RESTORE IS THE COMPOSER BECOMING THE PARKED LINE AGAIN, IN ONE STEP.
                // My first version took the next non-empty Compose after the park, which is the
                // `f` of `fix it` — he types a REPLY before his sentence comes back. A guard that
                // matches the wrong event passes on a film that never restores anything.
                if let Some(parked) = parked.as_ref()
                    && restored.is_none()
                    && text == parked
                    && composer != *parked
                {
                    restored = Some(text.clone());
                }
                composer = text.clone();
            }
            Cue::Submit(text) => last_submit = text.clone(),
            _ => {}
        }
    }
    let parked = parked.expect("film 3 must park his half-typed line");
    let restored = restored.expect("film 3 must give the parked line back");
    assert!(
        parked.len() > 40,
        "the parked line is only {:?} \u{2014} too short to read as an interrupted sentence",
        parked
    );
    assert_eq!(
        restored, parked,
        "the line that came back is not the line he lost"
    );
    // And he finishes it: the last thing he sends starts with the sentence he began before the
    // outage, so a viewer sees the question completed rather than merely redisplayed.
    assert!(
        last_submit.starts_with(&parked) || last_submit.contains("what did that cost"),
        "he never finished the restored sentence: {last_submit:?}"
    );
}

/// 🔴 **THE WATERMARK IS GONE.**
///
/// He asked directly: *"why are you writing 'design fixture · the numbers on this screen were NOT
/// measured'?"* — a disclaimer stamped across every frame is not what a vision film does. The
/// `--demo` flag still gates the data, which is the real safety property; the banner does not appear.
#[test]
fn no_film_frame_carries_the_fixture_watermark() {
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true, PANE));
    for fraction in [0, 3, 6] {
        let frame = frame_at(film, total * fraction / 8, true);
        assert!(
            !frame.contains("NOT measured"),
            "the fixture watermark is back on the film:\n{frame}"
        );
        assert!(
            !frame.contains("design fixture"),
            "the fixture watermark is back on the film:\n{frame}"
        );
    }
}

/// 🔴 No box corner survives a whole film.
#[test]
fn no_film_frame_carries_a_box_corner() {
    /// The nine corners, escaped so this guard cannot match itself. `box_glyphs` owns the list and
    /// is compiled into the LIBRARY, which this binary does not declare — the same reason
    /// `main.rs:7866` carries its own copy.
    const BOX_CORNERS: [&str; 9] = [
        "\u{250C}", "\u{2510}", "\u{2514}", "\u{2518}", "\u{251C}", "\u{2524}", "\u{252C}",
        "\u{2534}", "\u{253C}",
    ];
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true, PANE));
    for fraction in [1, 4, 7] {
        let frame = frame_at(film, total * fraction / 8, true);
        for corner in BOX_CORNERS {
            assert!(
                !frame.contains(corner),
                "film 1 drew the box corner {corner:?}:\n{frame}"
            );
        }
    }
}

/// 🔴 **NO GALLERY FRAME ID AND NO DEBUG LINE COUNT REACHES A FILM.**
///
/// His frames carried `09-gate-refused · 23 lines`, `30-provider-keys · 27 lines`,
/// `33b-model-cost · 20 lines` and four more — a filename and an internal metric, on screen, in a
/// film for an investor. Two mechanisms produced all of them: `Say::Screen`, which labelled a beat
/// with the book's own frame name, and `TranscriptEntry::Tool`, whose receipt renders
/// `· N lines` beside its label (`history_transcript.rs:167`). Both are gone — a beat is a
/// `Command` now — and this asserts it on the RENDERED FRAME rather than on the script, because
/// the count was never in the script to begin with.
#[test]
fn no_internal_id_or_line_count_reaches_a_film_frame() {
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true, PANE));
    for fraction in 0..8 {
        let frame = frame_at(film, total * fraction / 8, true);
        // Every screen the book has ever named. If a beat reintroduces one, it fires by name.
        for screen in crate::design_book::SCREENS {
            assert!(
                !frame.contains(screen.name),
                "the gallery frame id {:?} is on screen at {}/8 through the film:\n{frame}",
                screen.name,
                fraction
            );
        }
        // ⚠️ **THE NEEDLE IS THE RECEIPT'S EXACT CHROME, NOT THE WORD "lines".** A bare
        // `contains(" lines")` fired on the gate's own `blast radius · 2 files · 17 changed
        // lines`, which is product content a viewer SHOULD see. What must not appear is the tool
        // receipt's `  ·  N line(s)` suffix — a count of rows, which is a debug metric.
        for row in frame.lines() {
            if let Some(at) = row.find("  \u{b7}  ") {
                let after = &row[at + 5..];
                let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
                assert!(
                    digits.is_empty() || !after[digits.len()..].starts_with(" line"),
                    "a tool receipt's line count is on screen at {fraction}/8: {row:?}"
                );
            }
        }
    }
}

/// 🔴 **THE DEFECT HE PHOTOGRAPHED: TEXT RENDERED VERTICALLY, ONE CHARACTER PER LINE.**
///
/// `claims/upstream.py:141` came out eleven rows tall because the beat put a 22-character path in
/// a 2-wide marker column. `session::MIN_WRAP` refuses that at the source, which is the strong
/// guard; this is the INDEPENDENT ORACLE over the rendered buffer. It checks the SYMPTOM rather
/// than the cause, so it still fires if some other path ever produces the same picture.
///
/// ⚠️ **THE FIRST VERSION OF THIS TEST ASSERTED THE WRONG INVARIANT** — that no line in a receipt
/// may start left of the first line's indent. That is false for a diff: its header row leaves the
/// marker column blank (indent 4) and every row after it fills the marker at column 0. It fired on
/// CORRECT output, which is how a guard teaches you to loosen it until it catches nothing. The
/// property that actually separates the defect from a diff is a RUN of near-empty lines.
///
/// ⚠️ Read on the SESSION column only: the production rail draws its own text on the same terminal
/// rows, and a whole-row read would find rail content on a row the session left blank.
#[test]
fn no_receipt_ever_renders_its_text_vertically() {
    // Three consecutive lines of one or two visible characters is not a layout, it is a column.
    const RUN: usize = 3;
    const THIN: usize = 2;
    for film in script::FILMS {
        let total = runtime_ms(&cue_sheet(film, true, PANE));
        let mut inspected = 0usize;
        for fraction in 1..16 {
            let frame = frame_at(film, total * fraction / 16, true);
            let mut consecutive = 0usize;
            for row in frame.lines() {
                let session = session_of(row, WIDE);
                let visible = session.trim().chars().count();
                inspected += 1;
                if visible > 0 && visible <= THIN {
                    consecutive += 1;
                    assert!(
                        consecutive < RUN,
                        "film {} rendered text vertically \u{2014} {consecutive} consecutive lines of {THIN} characters or fewer:\n{frame}",
                        film.number
                    );
                } else {
                    consecutive = 0;
                }
            }
        }
        assert!(
            inspected > 400,
            "only {inspected} rows inspected for film {} \u{2014} the guard proves nothing",
            film.number
        );
    }
}

// ── the flattening, and the guards that would have caught it ─────────────────────────────────
//
// 🔴 **THE FILM RENDERED THE REAL GATE AND THREW ITS STYLING AWAY, AND NOTHING WAS RED.** The
// founder, watching film 3: *"you kind of got rid of all the colours, you got rid of all the
// styling … you neutered Estelle in the demo."* And again, specifically: *"the gate refused in
// white for some reason, it should be red."*
//
// ⚠️ **EVERY GUARD IN THIS FILE COULD HAVE PASSED OVER IT, AND DID.** They assert on the frame's
// CHARACTERS — `frame_at` collects `buffer[(x, y)].symbol()` and drops the cell's style on the
// floor, which is the same `collect::<String>()` the defect was. A text assertion cannot see a
// colour, so the guards below read `fg` and `modifier` off the buffer instead. That is the whole
// lesson: **the instrument has to be able to see the thing it is certifying.**

/// The one colour the brand owns, from the one place that declares it (`theme.rs:61`).
fn brand_red() -> ratatui::style::Color {
    crate::theme::ScreenTheme::Dark.palette().red
}

/// When the last row of the film's gate block has arrived, in film-clock milliseconds.
///
/// ⚠️ **FOUND, NEVER INDEXED.** `script.rs` is data the founder reorders, so a guard that pins the
/// gate to a beat number breaks on exactly the edit it exists to survive — the same correction
/// `credential_lines` already carries next door.
fn gate_settles_at(film: &'static Film) -> u32 {
    let steps = cue_sheet(film, true, PANE);
    // 🔴 **BY SOURCE, NOT BY POSITION — AND THIS GUARD CAUGHT ITSELF GETTING THAT WRONG.**
    // The first version matched any `OpenPainted`, which was the gate right up until the orchestra
    // beat became a painted block too. It then measured the FLEET's settle time, the loop below
    // stopped before the gate had streamed in, and both gate guards failed with "`Gate refused` is
    // not on the frame at all" — the vacuity check firing exactly as designed. A guard that selects
    // the only member of a set is a guard that breaks when the set grows.
    let opened = steps
        .iter()
        .position(|step| {
            matches!(
                step.cue,
                Cue::OpenPainted {
                    source: Paint::Gate { .. },
                    ..
                }
            )
        })
        .expect("the film must still carry a painted gate block");
    steps[opened..]
        .iter()
        .take_while(|step| matches!(step.cue, Cue::OpenPainted { .. } | Cue::PaintRow))
        .map(|step| step.at_ms)
        .last()
        .expect("the gate block must reveal at least one row")
}

/// The painted block whose source is a gate, and the source it was painted from.
fn gate_block(film: &'static Film) -> (Vec<Line<'static>>, Paint) {
    let mut app = film_app(film);
    let now = Instant::now();
    let mut paintings = Vec::new();
    for step in cue_sheet(film, true, PANE) {
        if step.at_ms > gate_settles_at(film) {
            break;
        }
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    let painting = paintings
        .iter()
        .find(|painting| matches!(painting.source, Paint::Gate { .. }))
        .expect("a gate block must be registered for repainting");
    let lines = match &app.transcript[painting.entry] {
        TranscriptEntry::Painted { lines, .. } => lines.clone(),
        other => panic!(
            "the registered gate entry is not a painted block: {:?}",
            std::mem::discriminant(other)
        ),
    };
    (lines, painting.source)
}

/// Every cell of the rendered frame, with the style the renderer actually gave it.
fn styled_frame(film: &'static Film, at_ms: u32) -> Vec<Vec<(char, ratatui::buffer::Cell)>> {
    let mut app = film_app(film);
    let now = Instant::now();
    let mut paintings = Vec::new();
    for step in cue_sheet(film, true, PANE) {
        if step.at_ms > at_ms {
            break;
        }
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    let mut terminal = Terminal::new(TestBackend::new(WIDE, TALL)).expect("test terminal");
    terminal
        .draw(|frame| live_renderer::render_frame(frame, &app, now))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| {
                    let cell = buffer[(x, y)].clone();
                    (cell.symbol().chars().next().unwrap_or(' '), cell)
                })
                .collect()
        })
        .collect()
}

/// 🔴 **THE FOUNDER'S SENTENCE, AS AN ASSERTION ON THE PIXELS.**
///
/// *"The gate refused in white for some reason, it should be red."* This finds the row spelling
/// `Gate refused` on the REAL rendered frame and asserts every character of those two words is
/// `#c52416` and BOLD — the style `marks::headline` computes and the old `collect::<String>()`
/// discarded.
///
/// ⚠️ **THE NEGATIVE CONTROL IS IN THE SAME TEST.** An assertion that some cell somewhere is red
/// would pass on a frame that painted the whole session red, so the row's trailing detail —
/// `· round 1 of 3 · no model call` — is asserted to be DIM. Red where red belongs, and not
/// elsewhere on the same row.
#[test]
fn gate_refused_reaches_the_frame_in_the_brand_red_and_not_in_white() {
    let film = script::film(1).expect("film 1");
    let frame = styled_frame(film, gate_settles_at(film));
    let needle: Vec<char> = "Gate refused".chars().collect();

    let mut checked = 0usize;
    for row in &frame {
        let symbols: Vec<char> = row.iter().map(|(character, _)| *character).collect();
        let Some(at) = symbols
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
        else {
            continue;
        };
        checked += 1;
        for (offset, (character, cell)) in row[at..at + needle.len()].iter().enumerate() {
            assert_eq!(
                cell.fg,
                brand_red(),
                "column {} of `Gate refused` rendered {:?} rather than the brand red — \
                 the film is flattening the gate's styling again",
                at + offset,
                cell.fg
            );
            assert!(
                cell.modifier.contains(ratatui::style::Modifier::BOLD),
                "the character {character:?} in `Gate refused` lost its BOLD"
            );
        }
        // The negative control: the detail beside the headline is DIM, so "red" is a statement
        // about these twelve characters and not about the row.
        let detail = row[at + needle.len()..]
            .iter()
            .find(|(character, _)| character.is_ascii_alphanumeric());
        if let Some((_, cell)) = detail {
            assert_ne!(
                cell.fg,
                brand_red(),
                "the headline's detail is red too — the whole row is being painted, not the words"
            );
        }
    }
    assert!(
        checked > 0,
        "`Gate refused` is not on the frame at all — this guard proves nothing"
    );
}

/// 🔴 **THE MARK PULSES AND THE WORDS DO NOT — ON THE FILM'S OWN REPAINT PATH.**
///
/// `marks.rs` already proves this property of `headline`. What it could not prove is that the FILM
/// reaches it: the old player passed `tick: 0, pulse_enabled: false` and baked one frame forever,
/// so a correct `headline` was called once and photographed. This drives [`repaint`] at the two
/// ends of the 28-tick cycle and reads the result off the transcript.
#[test]
fn the_gates_mark_pulses_in_a_film_and_its_words_hold_still() {
    let film = script::film(1).expect("film 1");
    let mut app = film_app(film);
    let now = Instant::now();
    let mut paintings = Vec::new();
    for step in cue_sheet(film, true, PANE) {
        if step.at_ms > gate_settles_at(film) {
            break;
        }
        apply(&step.cue, &mut app, now, &mut paintings);
    }
    assert!(
        !paintings.is_empty(),
        "no painted block was registered — the repaint path is unreachable"
    );

    let headline_of = |app: &App| {
        app.transcript
            .iter()
            .filter_map(|entry| match entry {
                TranscriptEntry::Painted { lines, .. } => Some(lines),
                _ => None,
            })
            .flatten()
            .find(|line| {
                line.spans
                    .iter()
                    .any(|span| span.content.contains("Gate refused"))
            })
            .cloned()
            .expect("the painted gate must carry its headline")
    };

    repaint(&mut app, &paintings, 0);
    let hot = headline_of(&app);
    repaint(&mut app, &paintings, 14);
    let cool = headline_of(&app);

    // The mark is span 0. It MOVED.
    assert_ne!(
        hot.spans[0].style.fg, cool.spans[0].style.fg,
        "the `\u{25a0}` did not pulse across the cycle — the film is still painting one frame"
    );
    assert_eq!(
        hot.spans[0].content, cool.spans[0].content,
        "the mark changed GLYPH rather than intensity"
    );
    // The words are span 1. They did NOT move — the spec's whole point.
    assert_eq!(
        hot.spans[1].style.fg, cool.spans[1].style.fg,
        "the words pulsed, which the design spec forbids: only the mark may pulse"
    );
    assert_eq!(hot.spans[1].style.fg, Some(brand_red()));
}

/// 🔴 **ONE OWNER: WHAT THE FILM DRAWS IS BYTE-FOR-BYTE WHAT THE PRODUCT DRAWS.**
///
/// `gate_refusal.rs`'s own header states the contract — *"This module is the single renderer;
/// `screens.rs` and `live_renderer::render_gate_modal` both call it"* — and the film is now the
/// third caller. This asserts the film's rows ARE that function's output, spans and styles
/// included, so a second implementation of "what the gate looks like" cannot appear without going
/// red.
#[test]
fn a_painted_row_is_the_shipped_renderers_own_output() {
    let film = script::film(1).expect("film 1");
    let (painted, source) = gate_block(film);
    let expected = paint(source, &Theme::Dark.screen_palette(), 0);

    assert_eq!(
        painted.len(),
        expected.len(),
        "the film revealed {} rows for a {}-row block",
        painted.len(),
        expected.len()
    );
    for (index, (mine, theirs)) in painted.iter().zip(&expected).enumerate() {
        assert_eq!(
            mine.spans.len(),
            theirs.spans.len(),
            "row {index} has a different span count from the renderer's own"
        );
        for (a, b) in mine.spans.iter().zip(&theirs.spans) {
            assert_eq!(a.content, b.content, "row {index} lost text");
            assert_eq!(a.style, b.style, "row {index} lost its style");
        }
    }
    // The vacuity half: a block of one blank row would satisfy everything above.
    assert!(
        painted.len() > 6,
        "the painted block is only {} rows — this guard proves nothing",
        painted.len()
    );
}

/// The claim [`paint_height`]'s docstring makes, pressed rather than asserted in prose.
///
/// Planning counts a block's rows with one palette and [`apply`] paints it with another, which is
/// only sound while colour cannot change a row COUNT. If a theme ever adds or drops a row, the
/// cue sheet would lay down the wrong number of `PaintRow` cues and the block would arrive
/// truncated or padded on camera.
#[test]
fn every_painted_block_has_the_same_height_in_both_themes() {
    for film in script::FILMS {
        for step in cue_sheet(film, true, PANE) {
            let Cue::OpenPainted { source, .. } = step.cue else {
                continue;
            };
            let dark = paint(source, &Theme::Dark.screen_palette(), 0).len();
            let cream = paint(source, &Theme::CreamInk.screen_palette(), 0).len();
            assert_eq!(
                dark, cream,
                "film {} has a block that is {dark} rows dark and {cream} rows cream — \
                 the plan's row count would be wrong in one of them",
                film.number
            );
            assert_eq!(dark, paint_height(source), "paint_height disagrees");
        }
    }
}

/// 🔴 **THE WORKERS MOVE. THE FOUNDER'S WHOLE ORCHESTRA NOTE IS THIS ONE PROPERTY.**
///
/// His words on the local models, which apply to the fleet identically: *"they just stand there,
/// nothing is working, it doesn't show actual progress."* A table pushed into the transcript once
/// is a photograph, and that is exactly what the hand-typed `Say::Table` was.
///
/// ⚠️ **ASSERTED ON THE RENDERED ROWS, NOT ON THE SNAPSHOT.** `fleet_snapshot` returning different
/// structs proves the fixture is a function of time; it does not prove the FILM shows it. This
/// drives [`repaint`] at three clocks and counts the distinct rendered blocks — the same discipline
/// `rail.rs` uses on the production rail.
#[test]
fn the_orchestra_workers_advance_while_the_block_sits_on_screen() {
    let mut blocks = std::collections::BTreeSet::new();
    let mut fleets = 0usize;
    for film in script::FILMS {
        for step in cue_sheet(film, true, PANE) {
            let Cue::OpenPainted {
                source: source @ Paint::Fleet { .. },
                ..
            } = step.cue
            else {
                continue;
            };
            fleets += 1;
            for tick in [0u64, 60, 140, 260] {
                let rows = paint(source, &Theme::Dark.screen_palette(), tick);
                blocks.insert(
                    rows.iter()
                        .map(|line| {
                            line.spans
                                .iter()
                                .map(|span| span.content.as_ref())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
        }
    }
    assert!(
        fleets > 0,
        "no film renders a worker table — this guard proves nothing"
    );
    assert!(
        blocks.len() > 2,
        "the worker table drew {} distinct frames across the whole film — it is a photograph",
        blocks.len()
    );
}

/// 🔴 **A BLOCK THAT CHANGES HEIGHT WOULD ARRIVE TRUNCATED ON CAMERA.**
///
/// The plan lays down one `PaintRow` cue per row, counted ONCE with [`paint_height`]. If a block's
/// height moved with the clock, [`repaint`]'s `zip` would silently stop at the shorter side and the
/// fleet would lose its progress bar — the last row, and the one the beat is about. Pressed at four
/// clocks rather than argued in a comment.
#[test]
fn a_painted_blocks_height_never_moves_with_the_clock() {
    for film in script::FILMS {
        for step in cue_sheet(film, true, PANE) {
            let Cue::OpenPainted { source, .. } = step.cue else {
                continue;
            };
            let planned = paint_height(source);
            for tick in [0u64, 60, 140, 260, 4_000] {
                assert_eq!(
                    paint(source, &Theme::Dark.screen_palette(), tick).len(),
                    planned,
                    "film {} has a block that is {planned} rows at plan time and a different \
                     height at tick {tick} — the film would reveal the wrong number of rows",
                    film.number
                );
            }
        }
    }
}

/// 🔴 **THE FILM MAY NOT INVENT A PER-WORKER MODEL OR COST. THE PRODUCT REFUSES TO.**
///
/// `FleetAgent` carries neither field (`orchestra_view`'s own header says so), which is why every
/// cost cell is an em dash and the frame prints `per-worker model + cost · no server contract`
/// underneath. The old hand-typed table put `claude-opus-4-8` in ten per-worker cells — a
/// fabricated measurement in the interface whose entire job is refusing those.
///
/// ⚠️ **THE POSITIVE CONTROL IS THE ROSTER.** Asserting only "no model name appears" would pass
/// over a table that had lost the model entirely, so the fleet's own `models ·` line is asserted to
/// still name it. The claim is *the model is named ONCE, where it is true* — not *the model is
/// gone*.
#[test]
fn the_worker_table_never_prints_a_per_worker_model_or_price() {
    for film in script::FILMS {
        for step in cue_sheet(film, true, PANE) {
            let Cue::OpenPainted {
                source: source @ Paint::Fleet { fixture, .. },
                ..
            } = step.cue
            else {
                continue;
            };
            let rows = painted_text(source, 120);
            let roster = rows
                .iter()
                .position(|row| row.contains("models \u{b7}"))
                .expect("the fleet must still name its model roster");
            // 🔴 **THE CONTROL IS THE FIXTURE'S OWN ROSTER, NOT ONE FILM'S MODEL NAME.** This read
            // `contains("claude-opus-4-8")`, which is a fact about film 1 wearing the shape of a
            // property about every fleet — so film 3's local batch, whose roster is honestly two
            // Qwen models, failed a guard it satisfies perfectly. Asserting the fixture's OWN
            // names is strictly stronger: a lost roster still fails, and a film that changes
            // implementer no longer has to change this test.
            assert!(
                !fixture.models.is_empty(),
                "a fleet with an empty roster names no model anywhere"
            );
            for model in fixture.models {
                assert!(
                    rows[roster].contains(model),
                    "the roster line lost {model:?}: {:?}",
                    rows[roster]
                );
            }
            // Every WORKER row — the ones beginning with a `w<n>` cell — carries no model and no
            // price. `$` catches a price in any column.
            for row in worker_rows(&rows) {
                for model in fixture.models {
                    assert!(
                        !row.contains(model),
                        "a worker row names a model the server does not send: {row:?}"
                    );
                }
                assert!(
                    !row.contains("claude-opus"),
                    "a worker row names a model the server does not send: {row:?}"
                );
                assert!(
                    !row.contains('$'),
                    "a worker row prints a price the server does not send: {row:?}"
                );
            }
            assert!(
                rows.iter()
                    .any(|row| row.contains(crate::orchestra_view::MISSING_PER_WORKER_SPEND)),
                "the frame does not say which measurement is missing"
            );
        }
    }
}

/// The worker rows of a rendered fleet block: the ones whose second cell is `w<n>`.
///
/// ⚠️ **NOT `starts_with(glyph)` AND NOT `contains(" w")`.** The first missed every row whose glyph
/// was not the one the test happened to expect, and the second matches the narrator line
/// *"10 workers writing 24 assignments"*. Selecting on the `wkr` CELL is the only reading that
/// means "a worker row" regardless of what state the worker is in.
fn worker_rows(rows: &[String]) -> Vec<&String> {
    rows.iter()
        .filter(|row| {
            row.split_whitespace().nth(1).is_some_and(|cell| {
                cell.starts_with('w')
                    && cell[1..].chars().all(|c| c.is_ascii_digit())
                    && cell.len() > 1
            })
        })
        .collect()
}

/// One painted block, flattened to plain rows for reading.
fn painted_text(source: Paint, tick: u64) -> Vec<String> {
    paint(source, &Theme::Dark.screen_palette(), tick)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

/// 🔴 **THE FAILURE BANNER AND THE WORKER TABLE MUST AGREE, AND NOW ONE IS DERIVED FROM THE OTHER.**
///
/// Film 1's banner says *"You reached your Anthropic usage limit. All ten workers stopped. Seven
/// were mid-write."* Those were three numbers typed into a string beside ten rows typed into a
/// table, with nothing connecting them — the shape where a viewer who pauses the video finds a
/// contradiction the author never could. The rows are the renderer's now, so this counts the
/// claim off the RENDERED block: ten rows, and exactly seven of them started an assignment and did
/// not finish it.
///
/// ⚠️ **COUNTED FROM THE `[done/total]` CELL, WHICH IS WHAT A VIEWER READS.** Counting off the
/// fixture would only prove the fixture is self-consistent; the question is whether the thing on
/// screen supports the sentence next to it.
#[test]
fn the_stopped_fleet_reconciles_with_its_own_banner() {
    let film = script::film(1).expect("film 1");
    let stopped = cue_sheet(film, true, PANE)
        .into_iter()
        .filter_map(|step| match step.cue {
            Cue::OpenPainted {
                source: source @ Paint::Fleet { .. },
                ..
            } => Some(source),
            _ => None,
        })
        // The film opens the fleet twice: alive, then stopped. The banner describes the second.
        .next_back()
        .expect("film 1 must render a stopped fleet");

    let rows = painted_text(stopped, 0);
    let workers = worker_rows(&rows);
    assert_eq!(
        workers.len(),
        10,
        "the banner says ten workers and the table draws {}:\n{}",
        workers.len(),
        rows.join("\n")
    );

    // `[done/total]` — mid-write is `done > 0` and `done < total`.
    let mid_write = workers
        .iter()
        .filter(|row| {
            row.split_once('[')
                .and_then(|(_, rest)| rest.split_once(']'))
                .and_then(|(cell, _)| cell.split_once('/'))
                .and_then(|(done, total)| {
                    Some((
                        done.trim().parse::<u64>().ok()?,
                        total.trim().parse::<u64>().ok()?,
                    ))
                })
                .is_some_and(|(done, total)| done > 0 && done < total)
        })
        .count();

    // 🔴 **THE NUMBER COMES OUT OF THE BANNER THE FILM ACTUALLY SPEAKS.** Hard-coding `6` here
    // would let the sentence and the table drift apart again the moment somebody retimes a worker
    // — the two would simply both be wrong in the same test. The banner is FOUND, the way
    // `credential_lines` is found, because the script is data the founder reorders.
    let banner = script::FILMS
        .iter()
        .flat_map(|film| film.beats.iter())
        .flat_map(|beat| beat.reply.iter())
        .find_map(|say| match say {
            crate::design_book::session::Say::Failure(lines) if lines[1].contains("mid-write") => {
                Some(lines[1])
            }
            _ => None,
        })
        .expect("film 1 must still carry the usage-limit banner");
    let spelled = [
        ("One", 1usize),
        ("Two", 2),
        ("Three", 3),
        ("Four", 4),
        ("Five", 5),
        ("Six", 6),
        ("Seven", 7),
        ("Eight", 8),
        ("Nine", 9),
        ("Ten", 10),
    ];
    let claimed = spelled
        .iter()
        .find(|(word, _)| banner.starts_with(word))
        .map(|(_, value)| *value)
        .unwrap_or_else(|| panic!("the banner does not open on a spelled number: {banner:?}"));
    assert_eq!(
        mid_write,
        claimed,
        "the banner says {banner:?} and the table shows {mid_write} mid-write:\n{}",
        rows.join("\n")
    );

    // 🔴 **AND THE UNKNOWN WORKER IS NOT COUNTED AS MID-WRITE.** Its state never arrived, which is
    // a different fact and the more interesting one — the fleet declines to guess. The banner says
    // so in its own sentence, and exactly one row must back that.
    assert!(
        banner.contains("never reported"),
        "the banner stopped naming the unreported worker: {banner:?}"
    );
    assert_eq!(
        workers
            .iter()
            .filter(|row| row.contains("Unknown \u{b7}"))
            .count(),
        1,
        "the table does not show exactly one unreported worker:\n{}",
        rows.join("\n")
    );

    // 🔴 And every stopped worker is drawn with the STOPPED glyph, never a checkmark. The data
    // contract this renderer cites is explicit: a stopped process is not a successful process.
    assert!(
        !rows
            .iter()
            .any(|row| row.trim_start().starts_with('\u{2713}')),
        "a killed worker drew a checkmark:\n{}",
        rows.join("\n")
    );
}
