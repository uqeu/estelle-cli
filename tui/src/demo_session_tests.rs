//! The session player's guards, in their own file.
//!
//! ⚠️ **SPLIT OUT BECAUSE `demo_session.rs` REACHED 917 LINES** against a house limit of 800. It is
//! attached with `#[path]` as a CHILD of `demo_session` rather than declared as a sibling in
//! `main.rs`, which is the difference that matters: a sibling could only reach `pub(crate)` items,
//! and every guard here asserts on the player's PRIVATE machinery — the cue sheet, the transcript,
//! the frame. Widening `Cue` and `Screen` to `pub(crate)` to make a test compile would have made
//! the transcript reachable from outside the one module that is allowed to touch it, which is the
//! invariant this whole file exists to defend.

use super::*;
use crate::theme::ScreenTheme;

/// The nine corners that make a box, spelled as escapes.
///
/// ⚠️ **THE ESCAPES ARE LOAD-BEARING AND THIS IS THE THIRD COPY OF THIS LIST IN THE TREE.**
/// `box_glyphs::BOX_CORNERS` is the owner, and it is compiled into the LIBRARY; this file is in
/// the `estelle` BINARY, which does not declare that module — the same reason `main.rs` carries
/// its own copy at `main.rs:7866`. Written as raw glyphs it would also defeat the source guard
/// by matching itself. **One owner is the right shape and the binary cannot reach it today**;
/// the honest form of that is this note, not a silent fourth copy.
const BOX_CORNERS: [&str; 9] = [
    "\u{250C}", "\u{2510}", "\u{2514}", "\u{2518}", "\u{251C}", "\u{2524}", "\u{252C}", "\u{2534}",
    "\u{253C}",
];

/// 🔴 **THE ONE PROPERTY THAT SEPARATES A SESSION FROM THE GALLERY HE REJECTED.**
///
/// Every cue of every film is applied in order and the transcript's length is asserted to be
/// monotonic. If anyone ever reaches for `clear()` to "start the next beat cleanly", this is
/// red before the footage is shot.
///
/// ⚠️ The vacuity half is asserted too: a transcript that never grew would also never shrink,
/// so the final length is required to be substantial. An emptiness check that passes on
/// nothing is the shape this repo has paid for repeatedly.
#[test]
fn the_transcript_only_ever_grows() {
    let palette = ScreenTheme::Dark.palette();
    for film in script::FILMS {
        let mut screen = Screen {
            transcript: Vec::new(),
            composer: String::new(),
            status: None,
        };
        let mut high_water = 0usize;
        for step in cue_sheet(film, true) {
            apply(&step.cue, &mut screen, &palette, 0, true);
            assert!(
                screen.transcript.len() >= high_water,
                "film {} reset its transcript at {} ms",
                film.number,
                step.at_ms
            );
            high_water = screen.transcript.len();
        }
        assert!(
            high_water > 60,
            "film {} only ever wrote {high_water} rows — the growth check proves nothing",
            film.number
        );
    }
}

/// Every film fits the bound, and none of them is so short it cannot be what was asked for.
#[test]
fn every_film_is_bounded_and_long_enough_to_talk_over() {
    for film in script::FILMS {
        let steps = cue_sheet(film, true);
        let ms = runtime_ms(&steps);
        assert!(
            ms < session::MAX_FILM_MS,
            "film {} runs {ms} ms, over the {} ms bound",
            film.number,
            session::MAX_FILM_MS
        );
        // He asked for two and a half minutes so he can talk through it. A film that came in
        // at forty seconds would be the gallery's pace wearing a session's clothes.
        assert!(
            ms > 100_000,
            "film {} runs {ms} ms — too fast to narrate",
            film.number
        );
    }
}

/// The cue sheet is ordered in time. The run loop walks it with one index and never sorts, so
/// an out-of-order step would silently be applied late — or never.
#[test]
fn the_cue_sheet_is_monotonic_in_time() {
    for film in script::FILMS {
        let steps = cue_sheet(film, true);
        for pair in steps.windows(2) {
            assert!(
                pair[0].at_ms <= pair[1].at_ms,
                "film {} plans a cue backwards",
                film.number
            );
        }
    }
}

/// 🔴 Typing is UNEVEN. A film whose keystroke gaps were all the same would read as a machine,
/// and that is the defect the founder named twice.
///
/// ⚠️ Asserted as a SPREAD, not as "jitter was called": a jitter function wired to a constant
/// would pass the second and fail this.
#[test]
fn keystrokes_are_not_evenly_spaced() {
    let film = script::film(1).expect("film 1");
    let steps = cue_sheet(film, true);
    let mut gaps = Vec::new();
    let mut previous: Option<u32> = None;
    for step in &steps {
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

/// The backspaces are in the footage. A scripted stumble that produced no shrinking composer
/// would be a stumble nobody can see.
#[test]
fn a_scripted_stumble_shows_the_composer_getting_shorter() {
    let film = script::film(1).expect("film 1");
    let mut shrinks = 0usize;
    let mut previous = 0usize;
    for step in cue_sheet(film, true) {
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
        shrinks >= 6,
        "film 1 shows {shrinks} backspaces — a person at the keyboard makes more than that visible"
    );
}

/// Every screen a film names exists in the book. A mistyped name would render nothing and the
/// beat would play as silence — a hole in the footage that still reports a clean runtime.
#[test]
fn every_screen_a_film_names_exists() {
    for film in script::FILMS {
        for beat in film.beats {
            for say in beat.reply {
                if let Say::Screen(name) = say {
                    assert!(
                        crate::design_book::SCREENS
                            .iter()
                            .any(|screen| screen.name == *name),
                        "film {} names screen {name:?}, which the book does not have",
                        film.number
                    );
                }
            }
        }
    }
}

/// 🔴 No box corner survives a whole film, in either theme.
#[test]
fn no_film_frame_carries_a_box_corner() {
    for (theme, name) in [(ScreenTheme::Dark, "dark"), (ScreenTheme::Cream, "cream")] {
        let palette = theme.palette();
        for film in script::FILMS {
            let mut screen = Screen {
                transcript: Vec::new(),
                composer: String::new(),
                status: None,
            };
            for step in cue_sheet(film, true) {
                apply(&step.cue, &mut screen, &palette, 0, true);
            }
            let frame = compose(film, &screen, &palette, 0, true, 40);
            let text: String = frame
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
                .collect();
            for corner in BOX_CORNERS {
                assert!(
                    !text.contains(corner),
                    "film {} drew the box corner {corner:?} on {name}",
                    film.number
                );
            }
        }
    }
}

/// With the fixture gate SHUT, no film shows a fixture number — it shows the empty states.
///
/// ⚠️ Paired with its positive control in the same loop, because an absence assertion passes
/// identically over a player that drew nothing at all.
#[test]
fn a_film_cannot_draw_fixture_numbers_with_the_gate_shut() {
    let palette = ScreenTheme::Dark.palette();
    let render = |fixtures: bool| -> String {
        let mut screen = Screen {
            transcript: Vec::new(),
            composer: String::new(),
            status: None,
        };
        for step in cue_sheet(script::film(1).expect("film 1"), fixtures) {
            apply(&step.cue, &mut screen, &palette, 0, fixtures);
        }
        screen
            .transcript
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
            .collect()
    };
    let open = render(true);
    let shut = render(false);
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
}

/// Render the film's frame as the terminal would, at the moment `at_ms`.
fn frame_at(film: &'static Film, at_ms: u32, palette: &Palette) -> String {
    let mut screen = Screen {
        transcript: Vec::new(),
        composer: String::new(),
        status: None,
    };
    for step in cue_sheet(film, true) {
        if step.at_ms > at_ms {
            break;
        }
        apply(&step.cue, &mut screen, palette, 0, true);
    }
    let backend = ratatui::backend::TestBackend::new(132, 44);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|f| {
            f.render_widget(
                Paragraph::new(compose(film, &screen, palette, 0, true, 44)),
                f.area(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 🔴 **THE ASK BAR IS THE FOUNDER'S OWN FIVE ROWS, AND IT SURVIVES EVERY MOMENT OF THE FILM.**
///
/// The live frame's bar has drifted three times and he has called it out three times
/// (`the_input_bar_is_the_demo_frames_five_rows_and_nothing_else`). A session that redrew it
/// slightly differently would put a fourth variant on camera, so the same clauses are asserted
/// here — over the SESSION's frame, at four moments spread across the film, because a bar that
/// is only correct on the first frame is a bar that breaks the moment the transcript scrolls.
#[test]
fn the_ask_bar_holds_its_shape_at_every_moment_of_the_film() {
    let palette = ScreenTheme::Dark.palette();
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true));
    for fraction in [1, 3, 5, 7] {
        let at = total * fraction / 8;
        let frame = frame_at(film, at, &palette);
        let rows: Vec<&str> = frame.lines().collect();

        // The prompt glyph is the heavy angle ornament, never the CJK bracket Terminal.app
        // draws as a closing parenthesis.
        assert!(
            frame.contains(crate::live_renderer::PROMPT_GLYPH),
            "no prompt at {at} ms:\n{frame}"
        );
        assert!(!frame.contains('\u{3009}'), "the CJK bracket came back");
        assert!(
            !frame.contains('\u{203a}'),
            "the small angle quote came back"
        );
        // The hint row is the frame's LAST row, and the prompt is not adjacent to it — the
        // founder photographed a cursor sitting on the `e` of "enter send".
        let hint = rows.last().expect("a last row");
        assert!(
            hint.contains("enter send"),
            "the hint row is not last at {at} ms:\n{frame}"
        );
        let prompt_at = rows
            .iter()
            .position(|row| row.contains(crate::live_renderer::PROMPT_GLYPH))
            .expect("prompt row");
        assert!(
            rows.len() - 1 > prompt_at + 1,
            "no room to type at {at} ms:\n{frame}"
        );
        // The fixture disclosure is on the frame, always, in the second row.
        assert!(
            rows[1].contains("NOT measured"),
            "the disclosure left the frame at {at} ms:\n{frame}"
        );
        // The rule is solid; the dashed one the product shipped until recently is gone.
        assert!(!frame.contains('\u{254c}'), "a dashed rule came back");
        assert!(frame.contains('\u{2500}'), "the solid rule is missing");
    }
}

/// 🔴 **THE TRANSCRIPT SCROLLS RATHER THAN RESETTING, AND THIS IS THE PROOF ON THE RENDERED
/// FRAME** — the growth test above proves it of the BUFFER, which is the other half.
///
/// Late in the film the transcript is longer than the viewport, so the frame must be FULL of
/// content (not blank rows), and the first content row must have MOVED ON from what it showed
/// earlier. A frame that reset would show beat 9 at the top with blank rows beneath it.
#[test]
fn the_window_moves_over_the_transcript_instead_of_the_transcript_resetting() {
    let palette = ScreenTheme::Dark.palette();
    let film = script::film(1).expect("film 1");
    let total = runtime_ms(&cue_sheet(film, true));
    let early = frame_at(film, total / 4, &palette);
    let late = frame_at(film, total * 7 / 8, &palette);
    let body = |frame: &str| -> Vec<String> {
        frame
            .lines()
            .skip(3)
            .take(44 - usize::from(CHROME_ROWS))
            .map(str::to_string)
            .collect()
    };
    let late_body = body(&late);
    let filled = late_body
        .iter()
        .filter(|row| !row.trim().is_empty())
        .count();
    assert!(
        filled > late_body.len() / 2,
        "late in the film the viewport is half empty — the transcript reset:\n{late}"
    );
    assert_ne!(
        body(&early).first(),
        late_body.first(),
        "the top of the viewport never moved — nothing scrolled"
    );
}

/// The disclosure is on the frame in both states, and it says which one it is.
#[test]
fn the_fixture_disclosure_is_on_every_frame() {
    let palette = ScreenTheme::Dark.palette();
    let text = |fixtures: bool| -> String {
        disclosure(&palette, fixtures)
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect()
    };
    assert!(text(true).contains("NOT measured"));
    assert!(text(false).contains("fixtures off"));
}
