//! The three screens where Estelle says NO — and then keeps going.
//!
//! 🔴 **A REFUSAL IS A STEP IN A LOOP, NOT A STOP.** The founder's review of 2026-09-02 gave this
//! module its two decisions in one breath: *"The refusal PULSES, and it gives the reason"*, and
//! *"If Estelle refuses a gate it goes back in, fixes it, takes that into account, remembers all
//! of that."* So no screen here may end on the refusal. Each closes on a [`loop_band`] — `✓` done,
//! `▶` in flight, `□` still to come — and `every_refusal_shows_the_loop_continuing` fails one that
//! stops.
//!
//! ⚠️ **THE PULSE IS ON THE MARK, NEVER ON THE WORDS.** Every headline goes through
//! [`crate::marks::headline`], which can style only one of the two. `marks.rs` proves that of the
//! FUNCTION; `only_the_mark_pulses_never_the_reason` proves it of the SCREEN, which is free to
//! reach for `pulse()` itself and which nothing in `marks.rs` would notice.
//!
//! ⚠️ **NOTHING IN A REFUSAL MAY BE TRUNCATED.** [`crate::cols::row`] ends an overlong cell in `…`,
//! right for a model name and catastrophic for the sentence saying why an edit was refused. Screen
//! 9 delegates to [`crate::gate_refusal`], which wraps instead, and the test reassembles the rows.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::gate_refusal::{Blocker, Refusal};
use crate::marks::StepMark::{Active, Done, NotStarted};
use crate::marks::{Mark, StepMark, headline};
use crate::theme::Palette;

/// The page width the three screens lay out against. All three frames are 120 columns; the rules
/// and the loop band stop short of the edge so the frame has a margin rather than a seam.
const WIDTH: usize = 108;

/// The loop band's columns: mark, verb, detail. Named, because the band's tint reaches the right
/// edge only if the three tile `WIDTH` exactly — which is asserted, not assumed.
const STEP_MARK: usize = 1;
const STEP_VERB: usize = 14;
const GAP: usize = 2;

/// One step of the loop a refusal opens: how far along it is, the verb, and what it is doing.
type Step = (StepMark, &'static str, &'static str);

fn step_columns() -> [Col; 3] {
    [
        Col::l(STEP_MARK),
        Col::l(STEP_VERB),
        Col::l(WIDTH - STEP_MARK - GAP - STEP_VERB - GAP),
    ]
}

/// `gate_refusal`'s blocker marker, on a table's first row only. A sub-line marker, not a border.
const fn marker(index: usize) -> &'static str {
    if index == 0 { "│" } else { "" }
}

/// 🔴 The ACTIVE step is a FULL-WIDTH BAND, not a brighter glyph. `cols::row` pads every cell, so
/// the line is exactly `WIDTH` columns and `palette.tint` reaches the right edge — a band stopping
/// at its own last word reads as a highlight on the text, not as "you are here".
fn step(mark: StepMark, verb: &str, detail: &str, palette: &Palette) -> Line<'static> {
    let verb_colour = match mark {
        Active => palette.bright,
        NotStarted => palette.dim,
        _ => palette.mid,
    };
    let line = owned(row(
        &step_columns(),
        &[
            Cell(mark.glyph(), mark.colour(palette)),
            Cell(verb, verb_colour),
            Cell(detail, palette.dim),
        ],
        0,
    ));
    match mark.row_background(palette) {
        Some(background) => line.style(Style::default().bg(background)),
        None => line,
    }
}

/// The half of every screen here that makes it a loop rather than a dead end. A function, not three
/// copies: a rule kept by everyone remembering it is kept only on the paths somebody remembered.
fn loop_band(
    label: &'static str,
    mode: &'static str,
    steps: &[Step],
    palette: &Palette,
) -> Vec<Line<'static>> {
    let mut output = vec![
        blank(),
        rule(label, mode, WIDTH, palette.dim, palette.mid, palette.plan),
        blank(),
    ];
    for &(mark, verb, detail) in steps {
        output.push(step(mark, verb, detail, palette));
    }
    output
}

/// A screen's opening rule, accented `warn` because its mode half always names the refusal.
fn opening(label: &'static str, mode: &'static str, palette: &Palette) -> Line<'static> {
    rule(label, mode, WIDTH, palette.dim, palette.mid, palette.warn)
}

/// One table row, each cell's colour chosen by the caller.
fn table_row(columns: &[Col], cells: &[(&str, Color)], indent: usize) -> Line<'static> {
    let cells: Vec<_> = cells.iter().map(|(t, c)| Cell(t, *c)).collect();
    owned(row(columns, &cells, indent))
}

// ── 09 · the deterministic gate, refusing a package that does not exist ─────────────────────────

const GATE_BLOCKERS: [(&str, &str); 2] = [
    (
        "import fastapi_turbo",
        "no such package on PyPI; nearest is fastapi (0.115.6). The import would fail at load, not \
         at test time.",
    ),
    (
        "src/api/routes.py:12",
        "the repo graph holds zero definition sites for this module in any version the lockfile \
         resolves.",
    ),
];

const GATE_STEPS: [Step; 4] = [
    (Done, "refused", "the import does not exist"),
    (Done, "remembered", "this repo has no fastapi_turbo"),
    (Active, "repairing", "round 2 of 3, rewriting the import"),
    (NotStarted, "re-gate", "the repaired diff goes back"),
];

/// 09 · the gate refusing a diff, and the turn continuing anyway.
///
/// The refusal block is [`crate::gate_refusal::lines`] — the single renderer the live modal also
/// calls — because a book screen that redrew the product's loudest moment would be a second owner
/// of it, and the last time a design token had two owners they disagreed for four days.
pub(crate) fn gate_refused(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let files = [
        ("src/api/routes.py".to_string(), 14u64),
        ("src/api/deps.py".to_string(), 3u64),
    ];
    let blockers = GATE_BLOCKERS
        .iter()
        .map(|(claim, finding)| Blocker {
            claim,
            finding: Some(finding),
        })
        .collect::<Vec<_>>();
    let mut lines = crate::gate_refusal::lines(
        &Refusal {
            detail: "round 1 of 3 · no model call",
            note: Some(
                "A deterministic check against this repo's symbol graph. No model was asked, and no model can overrule it.",
            ),
            blockers: &blockers,
            files: &files,
        },
        palette,
        WIDTH,
        tick,
        pulse,
    );
    lines.extend(loop_band("repair", "round 2 of 3", &GATE_STEPS, palette));
    lines.push(blank());
    lines.push(note(palette, "nothing was written to your tree."));
    lines
}

// ── 10 · PROPOSED · the index is behind the tree, so navigation refuses ─────────────────────────

const NAV_ROWS: [(&str, &str); 3] = [
    ("find_definition", "Rows.fetchone, written since the sweep"),
    ("the index", "6ff03b18 · swept 2026-09-01 06:14"),
    ("your working tree", "75557c7f · 214 files changed"),
];

const NAV_STEPS: [Step; 4] = [
    (Done, "refused", "no citation was invented"),
    (Done, "remembered", "the sweep is behind HEAD"),
    (Active, "sweeping", "214 files changed since 6ff03b18"),
    (NotStarted, "re-answer", "the same question, at 75557c7f"),
];

/// 10 · PROPOSED. The server's `STALE — indexed at <sha>, repo is now <sha>` verdict is real and
/// has no CLI screen; this is the one it should get.
///
/// ⚠️ **WHY THIS IS A REFUSAL AND NOT AN ERROR.** A stale index does not fail loudly. It answers,
/// fluently, with a citation into code that has moved — the exact failure Estelle exists to
/// prevent. So the screen says the refusal is CORRECT, rather than leaving the reader to assume
/// something broke.
pub(crate) fn navigation_stale(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let columns = [Col::l(1), Col::l(20), Col::l(38)];
    let labels = ["", "what was asked", "against which tree"];
    let mut lines = vec![
        opening("navigation", "stale index", palette),
        blank(),
        headline(
            Mark::Blocked,
            "Index is behind your tree",
            "find_definition · no answer given",
            palette,
            tick,
            pulse,
        ),
        blank(),
        // The verdict in the server's own words, built from spans rather than a column: a clipped
        // SHA is a wrong SHA, and `cols` would clip it without saying so.
        Line::from(vec![
            Span::raw("  "),
            Span::styled("STALE", Style::default().fg(palette.warn)),
            Span::styled(" — indexed at ", Style::default().fg(palette.dim)),
            Span::styled("6ff03b18", Style::default().fg(palette.warn)),
            Span::styled(", repo is now ", Style::default().fg(palette.dim)),
            Span::styled("75557c7f", Style::default().fg(palette.cite)),
        ]),
        blank(),
        note(
            palette,
            "a stale index does not fail loudly — it answers with a plausible citation into code that has moved.",
        ),
        note(
            palette,
            "refusing is the correct behaviour, and the only one that cannot be confidently wrong.",
        ),
        blank(),
        head(&columns, &labels, palette.dim, 2),
    ];
    lines.extend(NAV_ROWS.iter().enumerate().map(|(index, (asked, tree))| {
        let cells = [
            (marker(index), palette.warn),
            (*asked, palette.mid),
            (*tree, palette.dim),
        ];
        table_row(&columns, &cells, 2)
    }));
    lines.extend(loop_band("recovery", "sweeping", &NAV_STEPS, palette));
    lines.push(blank());
    lines.push(note(palette, "your question is held, not dropped."));
    lines
}

// ── 11 · SPEC · one message is bigger than the window, so compaction refuses ────────────────────

const SPLIT_ROWS: [(&str, &str, &str); 3] = [
    ("part 1 of 3", "src/api/routes.py, deps.py", "78,400"),
    ("part 2 of 3", "src/serve/gate.py, graph.py", "71,200"),
    ("part 3 of 3", "tests/test_gate.py, conftest.py", "64,400"),
];

const SPLIT_STEPS: [Step; 3] = [
    (Done, "measured", "nothing was dropped"),
    (Active, "split", "at the file boundary"),
    (NotStarted, "send", "part 1 of 3, then 2 and 3"),
];

/// 11 · SPEC. The last message alone is larger than the usable window.
///
/// 🔴 **ONE ACTIONABLE LINE, NOT A PERCENTAGE BAR.** The founder replaced the context meter with a
/// sentence on purpose: a bar at 107% tells a reader they are in trouble and nothing about what to
/// do. `the_screen_states_the_overflow_and_draws_no_meter` fails a meter back into existence.
pub(crate) fn compaction_refused(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let columns = [Col::l(1), Col::l(12), Col::l(34), Col::r(9)];
    let labels = ["", "part", "what it carries", "tokens"];
    let mut lines = vec![
        opening("context", "will not fit", palette),
        blank(),
        headline(
            Mark::Blocked,
            "This turn cannot be compacted",
            "latest_turn_exceeds_usable_window",
            palette,
            tick,
            pulse,
        ),
        blank(),
        // THE line. Spans, not a column — the sentence IS the screen, and a `…` in it would be the
        // screen failing at the one thing it does.
        Line::from(vec![
            Span::raw("  "),
            Span::styled("one message is ", Style::default().fg(palette.mid)),
            Span::styled("214,000 tokens", Style::default().fg(palette.warn)),
            Span::styled(" and the window holds ", Style::default().fg(palette.mid)),
            Span::styled("200,000", Style::default().fg(palette.cite)),
            Span::styled(
                " — it cannot be compacted, only split.",
                Style::default().fg(palette.mid),
            ),
        ]),
        blank(),
        note(
            palette,
            "compaction summarises turns. There is one turn here, so there is nothing to summarise against.",
        ),
        blank(),
        head(&columns, &labels, palette.dim, 2),
    ];
    lines.extend(SPLIT_ROWS.iter().enumerate().map(|(index, part)| {
        let cells = [
            (marker(index), palette.warn),
            (part.0, palette.mid),
            (part.1, palette.dim),
            (part.2, palette.cite),
        ];
        table_row(&columns, &cells, 2)
    }));
    lines.extend(loop_band("split", "3 parts", &SPLIT_STEPS, palette));
    lines.push(blank());
    lines.push(note(palette, "nothing was summarised away to make room."));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    /// The shape `design_book::BookScreen::render` stores. Spelled out because an inferred
    /// `fn(_, _, _) -> _` is not higher-ranked over the palette's lifetime and will not unify.
    type Render = for<'a> fn(&'a Palette, u64, bool) -> Vec<Line<'static>>;

    const SCREENS: [(&str, &str, Render); 3] = [
        ("gate_refused", "Gate refused", gate_refused),
        ("navigation_stale", "indexed at", navigation_stale),
        ("compaction_refused", "one message", compaction_refused),
    ];

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every span's text and the colour it is actually PAINTED in.
    ///
    /// ⚠️ **THE FIRST VERSION READ `span.style.fg` AND WAS BLIND.** A mutant that pulsed the
    /// refusal's sentence with `Line::styled(text, pulse(..))` PASSED, because that styles the LINE
    /// and leaves every span's `fg` at `None`. The painted colour is the line patched by the span.
    fn spans(lines: &[Line<'_>]) -> Vec<(String, Option<Color>)> {
        lines
            .iter()
            .flat_map(|line| {
                line.spans
                    .iter()
                    .map(|span| (span.content.to_string(), line.style.patch(span.style).fg))
            })
            .collect()
    }

    /// 🔴 THE PROPERTY `marks.rs` EXISTS TO PROTECT, ASSERTED OVER A WHOLE SCREEN.
    ///
    /// Both halves are asserted, because either alone is passable by a broken renderer: EXACTLY
    /// one span may move its colour across the cycle (a screen that stopped pulsing altogether
    /// fails that), and the words of the refusal must hold one colour at every tick.
    #[test]
    fn only_the_mark_pulses_never_the_reason() {
        let palette = ScreenTheme::Dark.palette();
        let hot = spans(&gate_refused(&palette, 0, true));
        let cool = spans(&gate_refused(&palette, 14, true));
        assert_eq!(hot.len(), cool.len(), "the two ticks rendered differently");

        let moved = hot
            .iter()
            .zip(cool.iter())
            .inspect(|(a, b)| assert_eq!(a.0, b.0, "the two ticks rendered different TEXT"))
            .filter(|(a, b)| a.1 != b.1)
            .map(|(a, _)| a.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            moved,
            vec![format!("{} ", Mark::Refused.glyph())],
            "something other than the mark changed colour between tick 0 and tick 14"
        );

        for tick in [0u64, 7, 14, 21] {
            let rendered = gate_refused(&palette, tick, true);
            let reason = rendered
                .iter()
                .flat_map(|line| line.spans.iter())
                .find(|span| span.content == "Gate refused")
                .expect("the headline lost its text span");
            assert_eq!(reason.style.fg, Some(palette.red), "tick {tick}");
        }
    }

    /// 🔴 A REFUSAL MAY NEVER BE TRUNCATED. A half-sentence explaining why an edit was refused is
    /// worse than none: the reader cannot tell which half is missing.
    ///
    /// ⚠️ The row break is normalised to one space before comparing — a wrapped column necessarily
    /// inserts padding, so a literal `contains` on the raw frame could only ever pass by not
    /// wrapping at all. Every non-space character is still compared, in order.
    #[test]
    fn a_refusal_sentence_survives_the_frame_intact() {
        let sentence = GATE_BLOCKERS[0]
            .1
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let rendered = text(&gate_refused(&theme.palette(), 0, false));
            assert!(
                !rendered.contains('…'),
                "the frame truncated something\n{rendered}"
            );
            let flat = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                flat.contains(&sentence),
                "the refusal reason did not survive the frame\n{rendered}"
            );
        }
    }

    /// 🔴 THE FOUNDER'S RULE AS A TEST: *"A refusal is a step in a loop, not a stop."* A screen
    /// that ends at the refusal renders no `▶` and no `□`, and fails here.
    #[test]
    fn every_refusal_shows_the_loop_continuing() {
        let palette = ScreenTheme::Dark.palette();
        for (name, _, render) in SCREENS {
            let rendered = text(&render(&palette, 0, true));
            assert!(
                rendered.contains(Active.glyph()),
                "{name} has no step in flight — it is a dead end\n{rendered}"
            );
            assert!(
                rendered.contains(NotStarted.glyph()),
                "{name} shows nothing still to come — it is a dead end\n{rendered}"
            );
            assert!(
                rendered.contains(Done.glyph()),
                "{name} does not say what it already did\n{rendered}"
            );
        }
    }

    /// The needles `design_book::mod` declares, asserted here so a blank screen names itself on
    /// `cargo test` rather than only when the gallery runs.
    #[test]
    fn each_screen_renders_the_needle_the_book_declares() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for (name, needle, render) in SCREENS {
                let rendered = text(&render(&palette, 0, true));
                assert!(
                    rendered.contains(needle),
                    "{name} lost {needle:?}\n{rendered}"
                );
            }
        }
    }

    /// 🔴 THE BAR THE FOUNDER DELETED MUST NOT COME BACK, and the sentence replacing it has to
    /// carry BOTH numbers — a line saying "too big" without saying by how much is the bar again,
    /// spelled out in words.
    #[test]
    fn the_screen_states_the_overflow_and_draws_no_meter() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&compaction_refused(&palette, 0, true));
        assert!(
            rendered.contains("one message is 214,000 tokens"),
            "{rendered}"
        );
        assert!(rendered.contains("the window holds 200,000"), "{rendered}");
        for meter in ['█', '▓', '▒', '░', '▁', '▇', '%'] {
            assert!(
                !rendered.contains(meter),
                "a meter {meter:?} came back\n{rendered}"
            );
        }
    }

    /// The active step's tint reaches the right edge only if the band tiles the page exactly. A
    /// column-arithmetic slip leaves a ragged highlight nobody notices in a screenshot.
    #[test]
    fn the_active_step_band_tiles_the_page_and_only_it_is_lifted() {
        let palette = ScreenTheme::Dark.palette();
        let columns = step_columns();
        assert_eq!(
            columns[0].w + GAP + columns[1].w + GAP + columns[2].w,
            WIDTH
        );

        let active = step(Active, "repairing", "round 2 of 3", &palette);
        let width: usize = active.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, WIDTH);
        assert_eq!(active.style.bg, Some(palette.tint));
        for mark in [Done, NotStarted, StepMark::Blocked] {
            assert_eq!(
                step(mark, "x", "y", &palette).style.bg,
                None,
                "{mark:?} lifted"
            );
        }
    }
}
