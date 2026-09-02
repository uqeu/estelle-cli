//! `estelle demo --session N` — the REAL app, with scripted hands on it.
//!
//! 🔴 **THE FIRST VERSION OF THIS FILE WAS A SECOND RENDERER, AND THAT WAS THE WHOLE DEFECT.**
//! It composed its own `Vec<Line>`, drew its own ask bar, read `terminal.size()?.height` and
//! **never read the width**. Seven of the founder's complaints fell out of that one choice: the
//! frame stopped at column 100 in a wide terminal, the production rail was missing entirely, there
//! was no boot, the user-turn band never appeared because the real composer was bypassed, and the
//! spacing was laid out for a frame nobody has. Hand-drawn chrome is exactly what he rejected.
//!
//! So this module now draws **nothing**. It constructs a real [`App`], puts scripted text into the
//! real composer, pushes real [`TranscriptEntry`] values into the real transcript, and calls
//! [`live_renderer::render_frame`] — the same function the live binary calls, on `frame.area()`,
//! which is the entire terminal. Everything the founder asked for comes back for free and, more
//! importantly, **cannot drift**: the film gets the two-pane split, the production rail, the boot
//! scene, the user-turn tint band and the ask bar because it is the product, not a picture of it.
//!
//! ## What this module still owns
//!
//! Only TIME. [`cue_sheet`] turns a [`Film`] into a list of (millisecond, mutation), the run loop
//! advances a clock and applies them, and the runtime is therefore known before the first frame is
//! drawn — which is what a founder recording to a script needs. Three consequences:
//!
//! * **The frame never resets.** Nothing here clears the transcript; `apply` only ever pushes to
//!   it or grows its last entry. `the_transcript_only_ever_grows` presses every cue to prove it.
//! * **The bound is real.** [`session::MAX_FILM_MS`], checked by a test, not hoped for at runtime.
//! * **It exits by itself.** Keys ABORT, never advance. He had to Ctrl-C out of an earlier attempt.
//!
//! ⚠️ **THE ANSWER STREAMS BECAUSE HE SAID IT "LOADED IN ALL AT ONCE".** A reply is not one cue —
//! it is an `Open` followed by many small `Grow`s, so prose arrives in word-sized chunks and a tool
//! receipt arrives a line at a time. That is the difference between watching a model work and
//! watching a slide land, and it is why [`chunks`] exists.

use std::io;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::text::Line;

use crate::design_book::script;
use crate::design_book::session::{self, Beat, Film, GateFixture, Key, Say};
use crate::gate_refusal::{Blocker, Refusal};
use crate::transcript::TranscriptEntry;
use crate::{App, Args, Theme, live_renderer};

/// How often the screen is repainted. The boot scene animates off this, so it is fine enough that
/// its motion is smooth and coarse enough that a recording is not fighting the renderer for CPU.
const TICK: Duration = Duration::from_millis(40);

// ── the hands ────────────────────────────────────────────────────────────────────────────────
//
// 🔴 **THESE WERE 96/52/145 AND HE ASKED TWICE FOR THEM TO COME DOWN.** That was ~96 WPM on paper
// and read slower, because the word gap lands on every space and the eye reads the PAUSES, not the
// mean. At 62/38/95 it is ~140 WPM: a fast engineer, still legible on camera.
//
// ⚠️ **THE UNEVENNESS IS THE POINT AND IT SURVIVES THE SPEED-UP.** A person who produced a
// keystroke every N ms at any N reads as a machine, so the rate is still assembled from a base,
// plus a gap at every space, plus the jitter, plus whatever the script asks for — never from one
// interval. The typo-and-backspace beats are untouched: they are what make it read as a person.
//
// ⛔ **IF A FILM DROPS UNDER TWO MINUTES, THE FIX IS MORE CONTENT, NEVER SLOWER TYPING.**

const CHAR_MS: u32 = 62;
const BURST_CHAR_MS: u32 = 38;
const WORD_GAP_MS: u32 = 95;
/// How long a typo sits before the hand notices. Shorter reads as scripted, longer as hesitation.
const NOTICE_MS: u32 = 340;
const BACKSPACE_MS: u32 = 78;
const SUBMIT_MS: u32 = 260;
/// The last frame holds this long so a recording has a tail to cut on, not a black frame.
const TAIL_MS: u32 = 2_600;
/// The gap between two streamed chunks of one reply. Roughly a fast reading pace.
const CHUNK_MS: u32 = 55;
/// Words per streamed chunk. Two is the value that reads as generation rather than as a typewriter.
const CHUNK_WORDS: usize = 2;
/// 🔴 **THE HARD CEILING ON WAITING FOR THE BOOT.** The boot scene ends on its own clock, but a
/// loop that waits for someone else's condition with no bound is the shape Power of Ten #2 exists
/// to forbid. If boot has not finished by this point the film starts anyway.
const BOOT_CEILING: Duration = Duration::from_secs(12);
/// What the transcript spends on its own left indent before a command receipt's output starts.
/// Measured off the rendered buffer by `a_wrapped_cell_never_starts_left_of_its_column`.
const GUTTER: usize = 6;

/// Deterministic jitter in `[-38, +38]` ms, keyed on the keystroke's index.
///
/// ⚠️ **DETERMINISTIC ON PURPOSE — A RECORDING IS REHEARSED.** An RNG would make every take
/// different, so a fluff at 0:42 could not be re-shot against the same footage, and the runtime
/// this module reports would be a distribution rather than a number. What a viewer reads as human
/// is non-uniformity, not unpredictability.
const fn jitter(index: usize) -> i32 {
    let mixed = (index as u64)
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((mixed >> 33) % 77) as i32 - 38
}

// ── the cue sheet ────────────────────────────────────────────────────────────────────────────

/// One mutation of the real [`App`], at one moment.
///
/// ⚠️ Every variant here maps onto something the PRODUCT already does. There is no cue that draws
/// a line, because this module does not draw.
enum Cue {
    /// A character arrived in the real composer, or a backspace took one away.
    Compose(String),
    /// Enter. The turn joins the real transcript as [`TranscriptEntry::User`] — which is what
    /// earns the tint band, the one visual device separating his words from Estelle's.
    Submit(String),
    /// Estelle is working, or has stopped.
    Working(Option<&'static str>),
    /// Start a prose answer. It begins EMPTY and is grown by [`Cue::Grow`].
    OpenAnswer { grounded: bool },
    /// Append a chunk to the answer in flight.
    Grow(&'static str),
    /// Start a command receipt — `● /gate` — with no output yet.
    ///
    /// ⚠️ **`Command`, NOT `Tool`.** A tool receipt renders `· 23 lines` beside its label
    /// (`history_transcript.rs:167`). That is a debug metric, and it went out on screen in a film
    /// for an investor next to a gallery frame id.
    OpenCommand(&'static str),
    /// Append one output line to the command receipt in flight.
    CommandLine(String),
    /// A system note.
    System(&'static str),
    /// The product's own three-line refusal banner.
    Failure([&'static str; 3]),
    /// Open a block one of the product's own modules draws, styled. Nothing is revealed yet.
    ///
    /// ⚠️ **`name` IS `None` FOR A BLOCK THAT OPENS ITSELF.** `gate_refusal` leads on its own
    /// `── gate · refused ──` rule; a `● /gate` receipt above it draws a GREEN LANDED mark
    /// directly over a red refusal, which is the one pair of marks that must never stack.
    OpenPainted {
        name: Option<&'static str>,
        source: Paint,
    },
    /// Reveal one more row of the painted block in flight, at the app's own palette.
    PaintRow,
}

struct Step {
    at_ms: u32,
    cue: Cue,
}

/// Plan a whole film: every cue, at the millisecond it fires, at `--speed 1`.
///
/// 🔴 **BOUNDED BEFORE IT IS BUILT.** A film that is too long is a script defect, and the cheapest
/// place to find one is before the terminal is claimed.
fn cue_sheet(film: &'static Film, fixtures: bool, pane: usize) -> Vec<Step> {
    assert!(
        film.beats.len() <= session::MAX_BEATS,
        "film {} carries {} beats, over the {} bound",
        film.number,
        film.beats.len(),
        session::MAX_BEATS
    );
    let mut steps = Vec::new();
    let mut clock = 0u32;
    // 🔴 **THE PARKED LINE OUTLIVES THE BEAT THAT PARKED IT, AND IT HAS TO.** It was a local in
    // `plan_typing`, so it reset on every beat and `Key::Restore` handed back an empty string —
    // the film would have played the whole interrupt and then restored nothing, which is the one
    // detail the founder said sells it. The draft belongs to the SESSION, not to a beat.
    let mut parked = String::new();
    for beat in film.beats {
        clock = plan_beat(&mut steps, beat, clock, fixtures, pane, &mut parked);
    }
    steps
}

/// Plan one beat: typing, the wait, the reply arriving in pieces, then the silence he reads in.
fn plan_beat(
    steps: &mut Vec<Step>,
    beat: &'static Beat,
    start: u32,
    fixtures: bool,
    pane: usize,
    parked: &mut String,
) -> u32 {
    let (mut clock, typed) = plan_typing(steps, beat, start, pane, parked);

    clock += SUBMIT_MS;
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Submit(typed),
    });
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Working(Some("thinking")),
    });

    // Estelle takes its time. This silence is where the founder is talking, and it is the field
    // most likely to be trimmed by someone trying to fit more in.
    clock += beat.think_ms;
    steps.push(Step {
        at_ms: clock,
        cue: Cue::Working(None),
    });

    // 🔴 **THE FIXTURE GATE, ON THE SAME TIMELINE.** With the gate shut the beat still takes
    // exactly as long — every `Wait` is walked — so the runtime he rehearses against does not
    // move. Only the CONTENT changes.
    let reply: &'static [Say] = if fixtures { beat.reply } else { script::SHUT };
    for say in reply {
        clock = plan_say(steps, say, clock, pane);
    }
    clock + beat.read_ms
}

/// The keystrokes, and the two scripted stumbles that make the rate lumpy.
fn plan_typing(
    steps: &mut Vec<Step>,
    beat: &'static Beat,
    start: u32,
    pane: usize,
    parked: &mut String,
) -> (u32, String) {
    let mut clock = start;
    let mut typed = String::new();
    let mut strokes = 0usize;
    let press = |clock: &mut u32,
                 steps: &mut Vec<Step>,
                 typed: &mut String,
                 base: u32,
                 character: char,
                 strokes: &mut usize| {
        *clock = clock.saturating_add_signed(gap(base, character, *strokes));
        *strokes += 1;
        typed.push(character);
        steps.push(Step {
            at_ms: *clock,
            cue: Cue::Compose(typed.clone()),
        });
    };
    for key in beat.typed {
        match key {
            Key::Pause(ms) => clock += ms,
            // 🔴 THE CURSOR STAYS IN HIS LINE. Nothing here touches `typed`, so the composer keeps
            // whatever he had half-written while the transcript grows underneath it.
            Key::Interrupt(says) => {
                for say in *says {
                    clock = plan_say(steps, say, clock, pane);
                }
            }
            Key::Park => {
                *parked = std::mem::take(&mut typed);
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::Compose(String::new()),
                });
            }
            Key::Restore => {
                typed = std::mem::take(parked);
                clock += NOTICE_MS;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::Compose(typed.clone()),
                });
            }
            Key::Type(text) => {
                for character in text.chars() {
                    press(
                        &mut clock,
                        steps,
                        &mut typed,
                        CHAR_MS,
                        character,
                        &mut strokes,
                    );
                }
            }
            Key::Burst(text) => {
                for character in text.chars() {
                    press(
                        &mut clock,
                        steps,
                        &mut typed,
                        BURST_CHAR_MS,
                        character,
                        &mut strokes,
                    );
                }
            }
            Key::Oops(text) => {
                for character in text.chars() {
                    press(
                        &mut clock,
                        steps,
                        &mut typed,
                        CHAR_MS,
                        character,
                        &mut strokes,
                    );
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
    (clock, typed)
}

// ── the blocks the product draws for itself ──────────────────────────────────────────────────
//
// 🔴 **THE FILM USED TO CALL THE REAL RENDERER AND THEN THROW ITS STYLING AWAY.** The old
// `gate_lines` did `line.spans.iter().map(|span| span.content).collect::<String>()` — it kept the
// characters and dropped every `Style`. So `Gate refused` was computed in `#c52416`, BOLD, by
// `marks::headline`, and reached the screen as body text. The founder, watching film 3: *"you got
// rid of all the colours, you got rid of all the styling … you neutered Estelle in the demo."* He
// was reading exactly that `collect::<String>()`.
//
// ⚠️ **AND IT COULD NOT HAVE PULSED EITHER.** The old call passed `tick: 0, pulse_enabled: false`,
// which is the only honest thing a function that renders ONCE can pass. A pulse is a function of
// the frame clock, so a block that is painted at plan time is a block that cannot move. Hence
// [`Paint`]: the film records WHICH block is there, and repaints it from the app's own theme on
// every frame — which is also what lets a worker table's progress advance while it sits on screen.

/// A block whose own module renders it, and which the player repaints every frame.
///
/// ⚠️ **THE PANE TRAVELS WITH THE SOURCE.** It is measured once off the real terminal and every
/// repaint has to use the SAME width, or a block would re-wrap under its own reader.
#[derive(Clone, Copy)]
enum Paint {
    /// The gate refusing, drawn by [`crate::gate_refusal`] — the one renderer the live modal and
    /// the catalog screen also call.
    Gate {
        fixture: &'static GateFixture,
        pane: usize,
    },
    /// The Orchestra worker table, drawn by [`crate::orchestra_view`] — the renderer behind
    /// catalog screen 20 and the live session's own fleet panel.
    ///
    /// ⚠️ **THIS ONE READS THE CLOCK FOR CONTENT, NOT JUST FOR A PULSE.** The gate's rows are
    /// fixed and only its mark moves; a fleet's rows are a function of time, which is the founder's
    /// *"it actually shows each model going"*. Both go through the same [`repaint`], because
    /// "re-render this block at the current frame" is one job.
    Fleet {
        fixture: &'static session::FleetFixture,
        pane: usize,
    },
}

/// Where a repainted block lives in the real transcript, and when it got there.
///
/// 🔴 **`opened_tick` IS WHY THE FLEET IS NOT ALREADY FINISHED WHEN YOU FIRST SEE IT.**
/// [`repaint`] runs off `live_renderer::pulse_tick`, which counts from the APP's boot. Feeding that
/// straight to a worker table meant a block appearing forty seconds into a film opened on a fleet
/// that had been running forty seconds — every worker complete, the bar full, nothing to watch. A
/// block's animation clock has to start when the BLOCK does.
///
/// ⚠️ Harmless for the gate, whose pulse is periodic: a phase shift is invisible. It is load-bearing
/// for anything whose CONTENT is a function of time.
struct Painting {
    entry: usize,
    source: Paint,
    opened_tick: u64,
}

/// Draw one block, at this frame's clock, in the caller's palette.
///
/// 🔴 **ONE OWNER, CALLED FROM TWO PLACES AND NEVER COPIED.** [`apply`] calls it to reveal a row
/// as the block streams in, and [`repaint`] calls it to move what is already on screen. Both get
/// the same bytes for the same `tick`, which is the property `a_painted_row_is_the_renderers_own`
/// asserts — a second implementation of "what the gate looks like" is the defect
/// `gate_refusal.rs`'s own header was written to end.
fn paint(source: Paint, palette: &crate::theme::Palette, tick: u64) -> Vec<Line<'static>> {
    match source {
        Paint::Gate { fixture, pane } => {
            let blockers = fixture
                .blockers
                .iter()
                .map(|(claim, finding)| Blocker {
                    claim,
                    finding: Some(finding),
                })
                .collect::<Vec<_>>();
            let files = fixture
                .files
                .iter()
                .map(|(path, lines)| ((*path).to_string(), *lines))
                .collect::<Vec<_>>();
            crate::gate_refusal::lines(
                &Refusal {
                    detail: fixture.detail,
                    note: Some(fixture.note),
                    blockers: &blockers,
                    files: &files,
                },
                palette,
                pane,
                tick,
                // 🔴 **THE PULSE IS ON.** `marks::headline` can only apply it to the MARK — the
                // words stay steady red and bold — so this is the design's own answer to the
                // founder's "slow pulsing red", not a licence to blink a sentence at a reader.
                true,
            )
        }
        // 🔴 **THE CLOCK IS THE PULSE CLOCK, DELIBERATELY.** `pulse_tick` counts 50 ms steps from
        // the app's boot, so `tick * 50` is a millisecond reading of the SAME clock the gate's
        // mark moves on. One clock for everything that animates means a film cannot have two
        // notions of "now", which is how the rail and the session drifted apart once already.
        Paint::Fleet { fixture, pane } => {
            let elapsed_ms = u32::try_from(tick.saturating_mul(50)).unwrap_or(u32::MAX);
            crate::orchestra_view::lines(
                &session::fleet_snapshot(fixture, elapsed_ms),
                palette,
                pane,
                session::fleet_now(fixture, elapsed_ms),
            )
        }
    }
}

/// How many rows a block has. Needed at PLAN time, to lay one `PaintRow` cue per row on the clock.
///
/// ⚠️ **THE COUNT IS PALETTE-INDEPENDENT AND THE CONTENT IS NOT**, which is why planning may ask
/// for a count with any palette while [`apply`] paints with the app's real one. Every branch of
/// [`paint`] emits the same number of rows for every palette — colour changes a `Style`, never a
/// row — and `every_painted_block_has_the_same_height_in_both_themes` presses both themes to
/// prove it rather than leaving it as a comment.
fn paint_height(source: Paint) -> usize {
    paint(source, &crate::theme::ScreenTheme::Dark.palette(), 0).len()
}

/// Move every block already on screen to this frame's clock.
///
/// ⚠️ **ONLY THE ROWS ALREADY REVEALED.** `zip` stops at the shorter side, so a block still
/// streaming in keeps its partial height and does not jump to full size on the first repaint.
fn repaint(app: &mut App, paintings: &[Painting], tick: u64) {
    let palette = app.theme.screen_palette();
    for painting in paintings {
        let fresh = paint(
            painting.source,
            &palette,
            tick.saturating_sub(painting.opened_tick),
        );
        let Some(TranscriptEntry::Painted { lines, .. }) = app.transcript.get_mut(painting.entry)
        else {
            continue;
        };
        for (row, replacement) in lines.iter_mut().zip(fresh) {
            *row = replacement;
        }
    }
}

/// Plan one unit of a reply, streamed rather than dropped in whole.
fn plan_say(steps: &mut Vec<Step>, say: &'static Say, start: u32, pane: usize) -> u32 {
    let mut clock = start;
    match say {
        Say::Wait(ms) => clock += ms,
        Say::System(text) => {
            clock += CHUNK_MS * 4;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::System(text),
            });
        }
        Say::Failure(banner) => {
            clock += CHUNK_MS * 4;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::Failure(*banner),
            });
        }
        Say::Answer { text, grounded, .. } => {
            steps.push(Step {
                at_ms: clock,
                cue: Cue::OpenAnswer {
                    grounded: *grounded,
                },
            });
            for chunk in chunks(text) {
                clock += CHUNK_MS;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::Grow(chunk),
                });
            }
        }
        Say::Command { name, lines } => {
            clock += CHUNK_MS * 2;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::OpenCommand(name),
            });
            for line in *lines {
                clock += CHUNK_MS * 2;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::CommandLine((*line).to_string()),
                });
            }
        }
        Say::Table {
            name,
            columns,
            rows,
        } => {
            clock += CHUNK_MS * 2;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::OpenCommand(name),
            });
            // 🔴 LAID OUT AGAINST THE REAL PANE. This is the one call that stops a cell wrapping
            // to column 0 — `table_lines` wraps inside the column and `cols` positions every
            // continuation, so nothing here is ever wider than the pane it lands in.
            for line in session::table_lines(columns, rows, pane) {
                clock += CHUNK_MS * 2;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::CommandLine(line),
                });
            }
        }
        Say::LocalFleet => {
            clock += CHUNK_MS * 2;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::OpenCommand("models"),
            });
            // Measured on the machine the film is recorded on, at plan time, once.
            for line in session::local_fleet_lines(pane) {
                clock += CHUNK_MS * 2;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::CommandLine(line),
                });
            }
        }
        Say::Orchestra(fixture) => {
            let source = Paint::Fleet { fixture, pane };
            clock += CHUNK_MS * 2;
            steps.push(Step {
                at_ms: clock,
                // ⚠️ **NO `● /orchestra` RECEIPT.** `orchestra_view` opens on its own
                // `● Task(batch · N workers)`; a receipt above it draws two filled marks for one
                // event, which is the same doubling the gate block would have had.
                cue: Cue::OpenPainted { name: None, source },
            });
            for _ in 0..paint_height(source) {
                clock += CHUNK_MS * 2;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::PaintRow,
                });
            }
        }
        Say::Gate(fixture) => {
            let source = Paint::Gate { fixture, pane };
            clock += CHUNK_MS * 2;
            steps.push(Step {
                at_ms: clock,
                cue: Cue::OpenPainted { name: None, source },
            });
            // 🔴 **THE ROWS ARE COUNTED HERE AND PAINTED LATER.** Baking the styled row into the
            // cue would freeze it at the plan-time clock, and the mark could never pulse. What the
            // plan owns is TIME — one cue per row — which is all it ever owned.
            for _ in 0..paint_height(source) {
                clock += CHUNK_MS * 2;
                steps.push(Step {
                    at_ms: clock,
                    cue: Cue::PaintRow,
                });
            }
        }
    }
    clock
}

/// Split prose into word-sized chunks, so an answer GENERATES instead of appearing.
///
/// ⚠️ The chunks borrow from the source, which is `&'static str`, so streaming costs no allocation
/// per chunk — it is the same string, revealed a slice at a time.
fn chunks(text: &'static str) -> Vec<&'static str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut words = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' {
            words += 1;
            if words >= CHUNK_WORDS {
                out.push(&text[start..=index]);
                start = index + 1;
                words = 0;
            }
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The gap before one keystroke: the base rate, the word gap at a space, and the jitter.
fn gap(base: u32, character: char, index: usize) -> i32 {
    let word_gap = if character == ' ' { WORD_GAP_MS } else { 0 };
    let raw = i64::from(base) + i64::from(word_gap) + i64::from(jitter(index));
    raw.clamp(24, i64::from(u32::MAX >> 1)) as i32
}

/// The film's whole length at `--speed 1`, including the tail. Known before anything is drawn.
///
/// ⚠️ It does NOT include the boot scene, which runs on its own clock before the first cue. The
/// shot list says so rather than quietly folding an estimate in.
fn runtime_ms(steps: &[Step]) -> u32 {
    steps.last().map_or(0, |step| step.at_ms) + TAIL_MS
}

// ── the run ──────────────────────────────────────────────────────────────────────────────────

/// The session column's usable width, from the terminal's.
///
/// `session_view::split` is the one owner of where the divider falls; below its threshold there is
/// no rail and the session has the whole frame. [`GUTTER`] is what the transcript itself spends on
/// a command receipt's indent — measured from the rendered buffer, not guessed.
fn pane_width(terminal_width: u16) -> usize {
    let session = crate::session_view::split(terminal_width)
        .map_or(usize::from(terminal_width), |columns| columns[0].w);
    session
        .saturating_sub(GUTTER)
        .max(session::FALLBACK_PANE.min(session))
}

/// Play one film in the real terminal, unattended, and exit on its own.
pub(crate) async fn run(
    number: u8,
    speed: f32,
    fixtures: bool,
    theme: Theme,
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
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // 🔴 **THE PANE IS MEASURED, ONCE, FROM THE REAL TERMINAL.** Every table in the film is laid
    // out against this. It is the SESSION column — what is left after the production rail — not the
    // terminal width, because a table sized to the terminal overflows the pane it lands in, which
    // is exactly how six beats ended up wrapping to column 0.
    let pane = pane_width(terminal.size()?.width);
    let steps = cue_sheet(film, fixtures, pane);
    let ceiling = Duration::from_millis(u64::from(
        (runtime_ms(&steps) as f32 / speed).min(session::MAX_FILM_MS as f32 * 4.0) as u32,
    ));

    // 🔴 THE REAL APP. `App::new` touches the filesystem and nothing else — no client, no session,
    // no network — so a film is a local, offline render of the product's own frame.
    let mut app = App::new(Args {
        command: None,
        repo: Some(film.repo.to_string()),
    });
    app.branch = Some(film.branch.to_string());
    app.theme = theme;
    script::dress(&mut app, film, fixtures);

    // Which repainted blocks are on screen, and where. Grows with the transcript and is never
    // cleared, for the same reason the transcript is never cleared.
    let mut paintings: Vec<Painting> = Vec::new();
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    let opened = Instant::now();
    let mut film_started: Option<Instant> = None;
    let mut next = 0usize;

    loop {
        let now = Instant::now();
        // 🔴 BOOT FIRST, THEN THE SESSION. He called the boot "so fire" and asked that it not be
        // skippable, so the film waits for the product's own boot clock rather than racing it —
        // under a named ceiling, because waiting on someone else's condition unbounded is the
        // shape rule 2 forbids.
        if film_started.is_none() && (!app.boot_active(now) || opened.elapsed() >= BOOT_CEILING) {
            film_started = Some(now);
        }
        if let Some(start) = film_started {
            let elapsed_ms = (start.elapsed().as_millis() as f32 * speed) as u32;
            while next < steps.len() && steps[next].at_ms <= elapsed_ms {
                apply(&steps[next].cue, &mut app, now, &mut paintings);
                next += 1;
            }
        }

        // 🔴 **THE BLOCKS THE PRODUCT DRAWS FOR ITSELF ARE ALIVE, NOT PHOTOGRAPHED.** One call,
        // every frame, on the SAME clock the live app's own gate modal pulses off
        // (`live_renderer::pulse_tick`) — so the `■` beside `Gate refused` keeps the design's
        // 1.4 s cycle in a film exactly as it does in a session.
        let pulse = live_renderer::pulse_tick(&app, now);
        repaint(&mut app, &paintings, pulse);

        // 🔴 THE RAIL MOVES. One call, every frame: latency jitters, counters climb, timestamps
        // advance, and film 3's outage ramps while he is typing about something else.
        if let Some(start) = film_started {
            let elapsed_ms = (start.elapsed().as_millis() as f32 * speed) as u32;
            crate::design_book::rail::tick(&mut app, film, elapsed_ms, fixtures);
        }

        // THE WHOLE TERMINAL, WIDTH INCLUDED. `render_frame` lays out against `frame.area()`, so
        // the two-pane split, the production rail and the composer are the product's own.
        terminal.draw(|frame| live_renderer::render_frame(frame, &app, now))?;

        let done =
            next >= steps.len() && film_started.is_some_and(|start| start.elapsed() >= ceiling);
        if done || opened.elapsed() >= ceiling + BOOT_CEILING + Duration::from_secs(30) {
            break;
        }

        tokio::select! {
            _ = ticker.tick() => {}
            event = events.next() => match event {
                // Keys ABORT. Nothing advances the film — it plays unattended, which is the whole
                // point, and he should not have to touch the keyboard on camera.
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

/// Apply one cue to the real app.
///
/// 🔴 **THE ONLY FUNCTION THAT TOUCHES THE TRANSCRIPT, AND IT ONLY EVER PUSHES OR GROWS.** There is
/// no `clear`, no `truncate`, no assignment. That is the property separating a session from the
/// gallery he rejected, and `the_transcript_only_ever_grows` presses every cue to prove it.
fn apply(cue: &Cue, app: &mut App, now: Instant, paintings: &mut Vec<Painting>) {
    match cue {
        Cue::OpenPainted { name, source } => {
            app.transcript.push(TranscriptEntry::Painted {
                name: name.map(ToString::to_string),
                lines: Vec::new(),
            });
            paintings.push(Painting {
                entry: app.transcript.len() - 1,
                source: *source,
                opened_tick: live_renderer::pulse_tick(app, now),
            });
        }
        // 🔴 **PAINTED FROM THE APP'S OWN THEME, NOT FROM A PALETTE THE PLANNER CHOSE.** A film
        // recorded with `--theme cream-ink` draws its gate in cream ink's red, because the only
        // palette in this path is the one the frame is about to be drawn with.
        Cue::PaintRow => {
            let Some(painting) = paintings.last() else {
                return;
            };
            let rows = paint(painting.source, &app.theme.screen_palette(), 0);
            if let Some(TranscriptEntry::Painted { lines, .. }) = app.transcript.last_mut()
                && let Some(next) = rows.into_iter().nth(lines.len())
            {
                lines.push(next);
            }
        }
        Cue::Compose(text) => app.composer.set_text(text),
        Cue::Submit(text) => {
            app.transcript.push(TranscriptEntry::User(text.clone()));
            app.composer.set_text("");
            app.has_submitted_question = true;
        }
        Cue::Working(label) => {
            app.active = label.map(|label| crate::ActiveRequest {
                id: 1,
                label: label.to_string(),
                started: now,
                cancel: tokio_util::sync::CancellationToken::new(),
            });
        }
        Cue::OpenAnswer { grounded } => app.transcript.push(TranscriptEntry::Answer {
            text: String::new(),
            grounded: Some(*grounded),
            degraded: false,
            sources: Vec::new(),
        }),
        Cue::Grow(chunk) => {
            if let Some(TranscriptEntry::Answer { text, .. }) = app.transcript.last_mut() {
                text.push_str(chunk);
            }
        }
        Cue::OpenCommand(name) => app.transcript.push(TranscriptEntry::Command {
            name: (*name).to_string(),
            lines: Vec::new(),
        }),
        Cue::CommandLine(line) => {
            if let Some(TranscriptEntry::Command { lines, .. }) = app.transcript.last_mut() {
                lines.push(line.clone());
            }
        }
        Cue::System(text) => app
            .transcript
            .push(TranscriptEntry::System((*text).to_string())),
        Cue::Failure(banner) => app.transcript.push(TranscriptEntry::Failure([
            banner[0].to_string(),
            banner[1].to_string(),
            banner[2].to_string(),
        ])),
    }
}

/// `estelle demo --session 0`: the films and their real runtimes, as plain rows.
pub(crate) fn listing() -> String {
    let mut out = vec!["film  repo                     beats   runtime".to_string()];
    for film in script::FILMS {
        let seconds =
            f64::from(runtime_ms(&cue_sheet(film, true, session::FALLBACK_PANE))) / 1000.0;
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
        "runtimes are at --speed 1 and EXCLUDE the boot scene, which runs on its own clock first."
            .to_string(),
    );
    out.push("`--speed 0.75` plays at three quarters pace and runs longer.".to_string());
    out.join("\n")
}

/// `estelle demo --session N --list` — the shot list, with timecodes.
///
/// 🔴 **HE IS RECORDING TO THIS.** A voiceover is written against beat boundaries, and he needs
/// them before he presses record, not by scrubbing the footage afterwards. The numbers come out of
/// the same [`cue_sheet`] the player walks, so a beat he re-times in `script.rs` moves here in the
/// same edit — there is no second place that says how long a film is.
pub(crate) fn timeline(film: &'static Film) -> String {
    let mut out = vec![format!(
        "film {} \u{b7} {} \u{b7} {} beats",
        film.number,
        film.repo,
        film.beats.len()
    )];
    out.push(String::new());
    out.push("  in      out     typed".to_string());
    let mut clock = 0u32;
    let mut parked = String::new();
    for beat in film.beats {
        let mut steps = Vec::new();
        let start = clock;
        clock = plan_beat(
            &mut steps,
            beat,
            clock,
            true,
            session::FALLBACK_PANE,
            &mut parked,
        );
        // 🔴 THE SHOT LIST READS THE SUBMIT CUE, NOT A SECOND SIMULATION OF THE KEYBOARD.
        // `typed_text` used to re-walk `beat.typed` with its own rules, and the moment `Park` and
        // `Restore` arrived it disagreed with the composer about what he actually sent. One owner:
        // whatever `plan_typing` puts in the `Submit` cue IS the line.
        let typed = steps
            .iter()
            .find_map(|step| match &step.cue {
                Cue::Submit(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        out.push(format!("  {}  {}  {typed}", stamp(start), stamp(clock)));
    }
    out.push(String::new());
    out.push(format!(
        "  total {} after boot, at --speed 1, including a {:.1}s tail",
        stamp(clock + TAIL_MS),
        f64::from(TAIL_MS) / 1000.0
    ));
    out.join("\n")
}

/// `m:ss.s` from milliseconds.
fn stamp(ms: u32) -> String {
    format!("{}:{:04.1}", ms / 60_000, f64::from(ms % 60_000) / 1000.0)
}

/// The player's guards. `#[path]` rather than a sibling `mod` in `main.rs`: these assert on private
/// machinery, and the alternative was widening `Cue` to `pub(crate)` — which would put the
/// transcript within reach of code that is not allowed to touch it.
#[cfg(test)]
#[path = "demo_session_tests.rs"]
mod tests;
