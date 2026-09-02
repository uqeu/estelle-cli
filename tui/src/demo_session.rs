//! `estelle demo --session N` — one continuous working session, played unattended.
//!
//! 🔴 **THE FRAME NEVER RESETS, AND THAT IS NOT A SETTING.** The founder ran `estelle demo --demo`
//! and rejected it in one sentence: *"Is it a natural sequence? It should be a natural sequence
//! where it's like someone's actually using the CLI — not 'okay this is the next page, and as you
//! can see here, this is what it did.'"* The gallery's defect is structural — every screen is a
//! full-frame render and advancing is a keypress and a hard cut — so the fix is structural too.
//! This module owns **one `Vec<Line>` that is only ever pushed to**. There is no `clear`, no
//! `truncate`, no assignment to `transcript` after it is built, and
//! `the_transcript_only_ever_grows` presses every cue of every film to prove it. A page turn is
//! unrepresentable rather than merely avoided.
//!
//! ## The whole run is planned before the first frame is drawn
//!
//! [`cue_sheet`] turns a [`Film`] into a `Vec<Step>` — a time and a thing to do — and the run loop
//! does nothing but advance a clock and apply the steps whose moment has come. Three things fall
//! out of that, and all three are why it is built this way:
//!
//! * **The runtime is known without running it.** The founder is recording to a script and needs to
//!   know a film is two and a half minutes before he presses record, not after.
//! * **The bound is real.** Power of Ten #2: the loop's bound is [`session::MAX_FILM_MS`], a named
//!   constant, checked against every film by a test rather than hoped for at runtime.
//! * **It is testable without a terminal.** A cue sheet is data; the assertions below read it.
//!
//! ⚠️ **IT EXITS BY ITSELF AND THAT IS A FIXED BUG, NOT A FEATURE.** The founder had to Ctrl-C
//! repeatedly out of an earlier attempt at this. The loop ends when the sheet does; keys are an
//! ABORT, never an advance, and nothing waits on one.

use std::io;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::cols::rule;
use crate::design_book::script;
use crate::design_book::session::{self, Beat, Film, Key, Say};
use crate::theme::Palette;

/// How often the screen is repainted. Fine enough that a character appearing mid-tick is not
/// visible as a stall, coarse enough that a recording is not fighting the renderer for CPU.
const TICK: Duration = Duration::from_millis(40);

// ── the hands ────────────────────────────────────────────────────────────────────────────────
//
// 🔴 **THESE FIVE CONSTANTS ARE THE 50-WORDS-A-MINUTE THE FOUNDER ASKED FOR, AND THE POINT IS
// THAT THEY ARE UNEVEN.** Fifty words a minute is 240 ms per keystroke *on average*, and a person
// who actually produced a keystroke every 240 ms would read as a machine. A real typist bursts
// inside a word and stops between them, so the mean is assembled from a fast base rate plus a gap
// at every space plus whatever the script asks for — never from one interval.

/// The base gap between two characters inside a word.
const CHAR_MS: u32 = 96;
/// The gap a familiar phrase is typed at. A burst next to a slow patch is what makes a rate lumpy.
const BURST_CHAR_MS: u32 = 52;
/// Added at every space: the hand pausing between words.
const WORD_GAP_MS: u32 = 145;
/// How long a typo sits on screen before the hand notices it. Shorter and the correction reads as
/// scripted; longer and it reads as hesitation.
const NOTICE_MS: u32 = 340;
/// A backspace is faster than a keystroke — the finger is already on the key.
const BACKSPACE_MS: u32 = 78;
/// The reach for enter once the sentence is finished.
const SUBMIT_MS: u32 = 260;
/// How long the last frame of a film holds before the process exits, so a recording has a tail to
/// cut on rather than a black frame on the final line.
const TAIL_MS: u32 = 2_600;

/// Deterministic jitter in `[-38, +38]` ms, keyed on the keystroke's index.
///
/// ⚠️ **DETERMINISTIC ON PURPOSE — A RECORDING IS REHEARSED.** An RNG would make every take
/// different, so a fluff at 0:42 could not be re-shot against the same footage, and the runtime
/// this module reports would be a distribution rather than a number. The unevenness a viewer reads
/// as human does not require unpredictability; it requires non-uniformity, which this has.
const fn jitter(index: usize) -> i32 {
    // A tiny LCG, evaluated at compile time where the caller allows it. Two-in-three of the values
    // land off centre, which is the property that matters.
    let mixed = (index as u64)
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((mixed >> 33) % 77) as i32 - 38
}

// ── the cue sheet ────────────────────────────────────────────────────────────────────────────

/// One thing that happens at one moment.
enum Cue {
    /// The composer's contents changed — a character arrived, or a backspace took one away.
    Compose(String),
    /// Enter. The prompt joins the transcript and the composer empties.
    Submit(String),
    /// One unit of a reply lands in the transcript.
    Speak(&'static Say),
    /// Estelle is working, or has stopped. `None` clears the status row.
    Status(Option<&'static str>),
}

struct Step {
    at_ms: u32,
    cue: Cue,
}

/// Plan a whole film: every cue, at the millisecond it fires, at `--speed 1`.
///
/// 🔴 **BOUNDED BEFORE IT IS BUILT.** `MAX_BEATS` is asserted here rather than checked in the run
/// loop, because a film that is too long is a script defect and the cheapest place to find a script
/// defect is before the terminal is claimed.
fn cue_sheet(film: &'static Film, fixtures: bool) -> Vec<Step> {
    assert!(
        film.beats.len() <= session::MAX_BEATS,
        "film {} carries {} beats, over the {} bound",
        film.number,
        film.beats.len(),
        session::MAX_BEATS
    );
    let mut steps = Vec::new();
    let mut clock = 0u32;
    for beat in film.beats {
        clock = plan_beat(&mut steps, beat, clock, fixtures);
    }
    steps
}

/// Plan one beat and return the clock after it. Typing, then the wait, then the reply arriving a
/// line at a time, then the silence the founder reads in.
fn plan_beat(steps: &mut Vec<Step>, beat: &'static Beat, start: u32, fixtures: bool) -> u32 {
    let mut clock = start;
    let mut typed = String::new();
    let mut strokes = 0usize;

    for key in beat.typed {
        match key {
            Key::Pause(ms) => clock += ms,
            Key::Type(text) | Key::Burst(text) => {
                let base = if matches!(key, Key::Burst(_)) {
                    BURST_CHAR_MS
                } else {
                    CHAR_MS
                };
                for character in text.chars() {
                    clock = clock.saturating_add_signed(gap(base, character, strokes));
                    strokes += 1;
                    typed.push(character);
                    steps.push(Step {
                        at_ms: clock,
                        cue: Cue::Compose(typed.clone()),
                    });
                }
            }
            Key::Oops(text) => {
                for character in text.chars() {
                    clock = clock.saturating_add_signed(gap(CHAR_MS, character, strokes));
                    strokes += 1;
                    typed.push(character);
                    steps.push(Step {
                        at_ms: clock,
                        cue: Cue::Compose(typed.clone()),
                    });
                }
                // The hand notices, then takes it back one character at a time. The backspaces are
                // the tell that a person is at the keyboard rather than a script printing a string.
                clock += NOTICE_MS;
                for _ in text.chars() {
                    clock += BACKSPACE_MS;
                    typed.pop();
                    steps.push(Step {
                        at_ms: clock,
                        cue: Cue::Compose(typed.clone()),
                    });
                }
            }
        }
    }

    clock += SUBMIT_MS;
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Submit(typed),
    });
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Status(Some("working")),
    });

    // Estelle takes its time. This silence is where the founder is talking, and it is the field
    // most likely to be trimmed by someone trying to fit more in.
    clock += beat.think_ms;
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Status(None),
    });

    // 🔴 **THE FIXTURE GATE, ON THE SAME TIMELINE.** With the gate shut the beat still takes
    // exactly as long — every `Wait` and every `line_ms` is walked — so the runtime the founder
    // rehearses against does not move. Only the CONTENT changes: the first content-bearing unit
    // becomes the honest block naming what is missing, and the rest of the reply draws nothing.
    let mut said = false;
    for say in beat.reply {
        if let Say::Wait(ms) = say {
            clock += ms;
            continue;
        }
        clock += beat.line_ms;
        if fixtures {
            steps.push(Step {
                at_ms: clock,
                cue: Cue::Speak(say),
            });
        } else if !said {
            said = true;
            for shut in script::SHUT {
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::Speak(shut),
                });
            }
        }
    }

    clock + beat.read_ms
}

/// The gap before one keystroke: the base rate, the word gap at a space, and the jitter.
fn gap(base: u32, character: char, index: usize) -> i32 {
    let word_gap = if character == ' ' { WORD_GAP_MS } else { 0 };
    let raw = i64::from(base) + i64::from(word_gap) + i64::from(jitter(index));
    // A keystroke never takes negative time, and never less than a physical key can bounce.
    raw.clamp(24, i64::from(u32::MAX >> 1)) as i32
}

/// The film's whole length at `--speed 1`, including the tail. Known before anything is drawn.
fn runtime_ms(steps: &[Step]) -> u32 {
    steps.last().map_or(0, |step| step.at_ms) + TAIL_MS
}

// ── the frame ────────────────────────────────────────────────────────────────────────────────

/// What is on screen right now. Everything except `transcript` is replaced every frame;
/// `transcript` is only ever appended to.
struct Screen {
    transcript: Vec<Line<'static>>,
    composer: String,
    status: Option<&'static str>,
}

/// Rows the chrome owns above and below the transcript, so the viewport can be sized from the
/// terminal rather than from a number somebody counted.
const CHROME_ROWS: u16 = 9;

/// Draw one frame. The transcript scrolls; nothing else moves.
fn compose(
    film: &Film,
    screen: &Screen,
    palette: &Palette,
    tick: u64,
    fixtures: bool,
    height: u16,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        owned_rule("session", film.repo, palette),
        disclosure(palette, fixtures),
        Line::from(""),
    ];

    // 🔴 THE SCROLL. The transcript is never trimmed — only the WINDOW onto it moves, so a beat
    // that scrolls past is still in the buffer and the frame has not reset.
    let viewport = usize::from(height.saturating_sub(CHROME_ROWS)).max(1);
    let first = screen.transcript.len().saturating_sub(viewport);
    lines.extend(screen.transcript[first..].iter().cloned());
    let drawn = screen.transcript.len() - first;
    for _ in drawn..viewport {
        lines.push(Line::from(""));
    }

    lines.push(status_row(screen.status, palette, tick));
    lines.push(Line::from(""));
    lines.push(owned_rule("ask", film.branch, palette));
    lines.push(prompt_row(&screen.composer, palette, tick));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {}", script::HINTS),
        Style::default().fg(palette.dim),
    )));
    lines
}

fn owned_rule(label: &str, mode: &str, palette: &Palette) -> Line<'static> {
    crate::design_book::owned(rule(
        label,
        mode,
        session::WIDTH,
        palette.dim,
        palette.mid,
        palette.cite,
    ))
}

/// 🔴 **THE FIXTURE DISCLOSURE, IN THE SAME WORDS THE GALLERY USES, AND IT DOES NOT COME OFF.**
/// The founder can hand this to an investor and answer *"is that real?"* in one sentence because
/// the row is on every frame of every film. It is the same instinct that makes the refusal beat
/// land, and it costs the footage one dim line.
fn disclosure(palette: &Palette, fixtures: bool) -> Line<'static> {
    let (text, ink) = if fixtures {
        (
            "  design fixture · the numbers on this screen were NOT measured",
            palette.warn,
        )
    } else {
        (
            "  fixtures off · this session renders each screen's empty state — add --demo",
            palette.dim,
        )
    };
    Line::from(Span::styled(text.to_string(), Style::default().fg(ink)))
}

/// The status row. Empty while idle — the founder asked for `● Ready` gone — and a pulsing mark
/// with a steady word while Estelle is working.
fn status_row(status: Option<&str>, palette: &Palette, tick: u64) -> Line<'static> {
    match status {
        None => Line::from(""),
        Some(word) => Line::from(vec![
            Span::raw("  "),
            crate::marks::Mark::InFlight.span(palette, tick, true),
            Span::styled(word.to_string(), Style::default().fg(palette.dim)),
        ]),
    }
}

/// `❯ what is being typed▏`. The caret blinks; nothing else on the row moves.
fn prompt_row(composer: &str, palette: &Palette, tick: u64) -> Line<'static> {
    let caret = if (tick / 12) % 2 == 0 {
        "\u{258f}"
    } else {
        " "
    };
    Line::from(vec![
        Span::styled(
            format!("{} ", crate::live_renderer::PROMPT_GLYPH),
            Style::default().fg(palette.cite),
        ),
        Span::styled(
            composer.to_string(),
            Style::default()
                .fg(palette.bright)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(caret.to_string(), Style::default().fg(palette.cite)),
    ])
}

/// The prompt as it joins the transcript, so the scrollback reads as a conversation.
fn echoed_prompt(text: &str, palette: &Palette) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("{} ", crate::live_renderer::PROMPT_GLYPH),
                Style::default().fg(palette.cite),
            ),
            Span::styled(
                text.to_string(),
                Style::default()
                    .fg(palette.bright)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ]
}

// ── the run ──────────────────────────────────────────────────────────────────────────────────

/// Play one film in the real terminal, unattended, and exit on its own.
pub(crate) async fn run(
    number: u8,
    speed: f32,
    fixtures: bool,
    palette: Palette,
    background: ratatui::style::Color,
) -> io::Result<ExitCode> {
    let Some(film) = script::film(number) else {
        return Ok(ExitCode::FAILURE);
    };
    // A speed of zero, or a negative one, is a run that never finishes. Validated at the boundary.
    let speed = if speed.is_finite() && speed > 0.05 {
        speed
    } else {
        1.0
    };
    let steps = cue_sheet(film, fixtures);
    let ceiling = Duration::from_millis(u64::from(
        (runtime_ms(&steps) as f32 / speed).min(session::MAX_FILM_MS as f32 * 4.0) as u32,
    ));

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    let started = Instant::now();
    let mut screen = Screen {
        transcript: Vec::new(),
        composer: String::new(),
        status: None,
    };
    let mut next = 0usize;
    let mut tick = 0u64;

    loop {
        let elapsed_ms = (started.elapsed().as_millis() as f32 * speed) as u32;
        while next < steps.len() && steps[next].at_ms <= elapsed_ms {
            apply(&steps[next].cue, &mut screen, &palette, tick, fixtures);
            next += 1;
        }

        let height = terminal.size()?.height;
        let lines = compose(film, &screen, &palette, tick, fixtures, height);
        terminal.draw(|frame| {
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(background)),
                frame.area(),
            );
        })?;

        // 🔴 TWO INDEPENDENT TERMINATION CONDITIONS, AND THE SECOND IS THE ONE THAT MATTERS. The
        // sheet running out is the ordinary exit; the wall-clock ceiling is what makes a hung
        // recording impossible even if a cue were ever planned past the end of time.
        if next >= steps.len() && started.elapsed() >= ceiling {
            break;
        }
        if started.elapsed() >= ceiling + Duration::from_secs(30) {
            break;
        }

        tokio::select! {
            _ = ticker.tick() => tick = tick.wrapping_add(1),
            event = events.next() => match event {
                // Keys ABORT. Nothing advances the film — it plays unattended, which is the
                // whole point, and the founder should not have to touch the keyboard on camera.
                Some(Ok(Event::Key(key))) if key.kind != KeyEventKind::Release => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        _ => {}
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Apply one cue. **The only function that touches `transcript`, and it only ever pushes.**
fn apply(cue: &Cue, screen: &mut Screen, palette: &Palette, tick: u64, fixtures: bool) {
    match cue {
        Cue::Compose(text) => screen.composer = text.clone(),
        Cue::Submit(text) => {
            screen.transcript.extend(echoed_prompt(text, palette));
            screen.composer.clear();
        }
        Cue::Status(status) => screen.status = *status,
        Cue::Speak(say) => screen
            .transcript
            .extend(say.lines(palette, tick, true, fixtures)),
    }
}

/// `estelle demo --session --list`: the films and their real runtimes, as plain rows.
pub(crate) fn listing() -> String {
    let mut out = vec!["film  repo                     beats   runtime".to_string()];
    for film in script::FILMS {
        let seconds = f64::from(runtime_ms(&cue_sheet(film, true))) / 1000.0;
        out.push(format!(
            "{:<5} {:<24} {:>5}   {:>4.0}s",
            film.number,
            film.repo,
            film.beats.len(),
            seconds
        ));
    }
    out.push(String::new());
    out.push(
        "runtimes are at --speed 1. `--speed 0.75` plays at three quarters pace and runs longer."
            .to_string(),
    );
    out.join("\n")
}

/// `estelle demo --session N --list` — the shot list, with timecodes.
///
/// 🔴 **HE IS RECORDING TO THIS.** A voiceover is written against beat boundaries, and the founder
/// needs them before he presses record, not by scrubbing the footage afterwards. The numbers come
/// out of the same [`cue_sheet`] the player walks, so a beat he re-times in `script.rs` moves here
/// in the same edit — there is no second place that says how long a film is.
pub(crate) fn timeline(film: &'static Film) -> String {
    let mut out = vec![format!(
        "film {} \u{b7} {} \u{b7} {} beats",
        film.number,
        film.repo,
        film.beats.len()
    )];
    out.push(String::new());
    out.push("  in     out    typed".to_string());
    let mut clock = 0u32;
    for beat in film.beats {
        let mut steps = Vec::new();
        let start = clock;
        clock = plan_beat(&mut steps, beat, clock, true);
        let typed: String = beat
            .typed
            .iter()
            .map(|key| match key {
                Key::Type(text) | Key::Burst(text) => (*text).to_string(),
                Key::Oops(text) => format!("[{text}<<]"),
                Key::Pause(_) => "\u{2026}".to_string(),
            })
            .collect();
        out.push(format!("  {}  {}  {typed}", stamp(start), stamp(clock)));
    }
    out.push(String::new());
    out.push(format!(
        "  total {} at --speed 1, including a {:.1}s tail",
        stamp(clock + TAIL_MS),
        f64::from(TAIL_MS) / 1000.0
    ));
    out.join("\n")
}

/// `m:ss` from milliseconds.
fn stamp(ms: u32) -> String {
    format!("{}:{:04.1}", ms / 60_000, f64::from(ms % 60_000) / 1000.0)
}

/// The player's guards. `#[path]` rather than a sibling `mod` in `main.rs`: these assert on
/// private machinery, and the alternative was widening `Screen` and `Cue` to `pub(crate)` — which
/// would put the transcript within reach of code that is not allowed to touch it.
#[cfg(test)]
#[path = "demo_session_tests.rs"]
mod tests;
