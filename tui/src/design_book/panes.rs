//! Screens 25, 35 and 38 — the three book screens that put more than one thing in one terminal.
//!
//! 🔴 **THE FOUNDER'S MOCK DRAWS `┬`, `┼` AND `┴`. THIS DOES NOT.** The two-pane split is a plain
//! `│` divider column with `──` rules that STOP either side of it, so a rule can never close into
//! a panel. The divider's column is decided by [`PANE`] — a [`crate::cols`] spec — which is why
//! `the_pane_divider_lands_on_one_column_on_every_row` can be written at all: a hand-counted
//! layout has no owner to assert against.
//!
//! ⚠️ **WHERE A TABLE HAS TO LIVE INSIDE A PANE CELL, IT IS STILL A `cols` TABLE.** A `Cell` holds
//! a `&str`, so the production rows, the fleet header and the fleet rows are each built by
//! [`crate::cols::row`] and then FLATTENED into the pane's cell ([`flatten`]). The alignment is
//! still computed; only the per-cell colour is lost and the row takes its state's colour instead —
//! the same trade `screens.rs` screen 9 already makes for its right rail.
//!
//! 🔴 **THE FLEET TABLE NAMES ITS COLUMNS IN WORDS, AND SAYS WHICH ONES THE WIRE DOES NOT CARRY.**
//! The founder's Orchestra note was two things: show *model · task · state · tokens · price*, and
//! *"I don't know what age ahead means, so that needs to be explained as well."* So `age` is gone
//! — the column is `last seen` — and `clock ahead` is spelled out under the table. `model`,
//! `tokens` and `price` hold FIXTURE values, and the footnote quotes [`MISSING_PER_WORKER_SPEND`]
//! verbatim rather than paraphrasing it, so this screen cannot drift from the live table's own
//! disclosure of the same absence.

use ratatui::style::{Color, Style};
use ratatui::text::Line;

use crate::cols::{Cell, Col, RULE, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::marks::{Mark, StepMark, headline};
use crate::orchestra_view::MISSING_PER_WORKER_SPEND;
use crate::theme::Palette;

// ── shared primitives ────────────────────────────────────────────────────

/// The palette roles these screens use, named so every fixture row below can stay a `const`.
/// A fixture that has to be built at runtime to pick a colour is a fixture nobody can read.
#[derive(Clone, Copy)]
enum Tone {
    Dim,
    Mid,
    Cite,
    Green,
    Warn,
}

impl Tone {
    fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Dim => palette.dim,
            Self::Mid => palette.mid,
            Self::Cite => palette.cite,
            Self::Green => palette.green,
            Self::Warn => palette.warn,
        }
    }
}

/// The rendered text of a `cols`-built line, so an aligned table can be placed INSIDE a pane cell.
fn flatten(line: &Line<'_>) -> String {
    line.spans.iter().map(|span| span.content.as_ref()).collect()
}

/// A rule's SHAPE without its colours, for the same reason. The texture still has one owner
/// ([`crate::cols::RULE`]); only the three accents are dropped.
fn rule_text(label: &str, mode: &str, width: usize) -> String {
    let bare = Color::Reset;
    flatten(&rule(label, mode, width, bare, bare, bare))
}

/// Text indented by `cols`, never by a hand-typed run of spaces.
fn indented(text: &str, indent: usize, width: usize) -> String {
    flatten(&row(&[Col::l(width)], &[Cell(text, Color::Reset)], indent))
}

/// Lift ONE already-aligned cell onto `palette.tint`. The geometry still comes from `cols`; this
/// only repaints the span `cols` produced, so a highlight can never move a column.
fn lift(line: Line<'static>, cell: &str, palette: &Palette) -> Line<'static> {
    let spans = line.spans.into_iter().map(|mut span| {
        if span.content.as_ref() == cell {
            span.style = span.style.bg(palette.tint);
        }
        span
    });
    Line::from(spans.collect::<Vec<_>>())
}

// ── 25 · several agents in one terminal ──────────────────────────────────

const PANE_LEFT: usize = 58;
const PANE_RIGHT: usize = 92;
/// The two-pane split. `Col::l` carries `gap: 2`, so the divider lands at `PANE_LEFT + 2` on every
/// row that has one — the property the test asserts rather than the one a reader hopes for.
const PANE: [Col; 3] = [Col::l(PANE_LEFT), Col::l(1), Col::l(PANE_RIGHT)];
/// A production line in the right rail: mark, service, detail.
const SERVICE: [Col; 3] = [Col::l(2), Col::l(12), Col::l(40)];
/// 🔴 The founder's seven columns, spelled out. `age` is not among them by design.
const FLEET_HEAD: [&str; 8] =
    ["", "wkr", "model", "task", "state", "tokens", "price", "last seen"];
const FLEET: [Col; 8] = [
    Col::l(2), Col::l(3), Col::l(11), Col::l(24), Col::l(10), Col::r(7), Col::r(7), Col::r(9),
];
/// ⚠️ `✓ ◐ ·` is `orchestra_view`'s status vocabulary, not `marks::Mark`'s. A worker row in the
/// book must read the same as a worker row in the terminal, and those are the terminal's glyphs.
/// ⚠️ `model`, `tokens` and `price` are FIXTURE — see [`MISSING_PER_WORKER_SPEND`], quoted on the
/// frame. The `tokens` and `price` cells add up to the summary row, because a fabricated number
/// that does not even reconcile is the one this interface exists to refuse.
#[rustfmt::skip]
const WORKERS: [([&str; 8], Tone); 3] = [
    (["✓", "w1", "haiku-4.5", "scope the retry path", "Completed", "4,120", "$0.001", "41s ago"], Tone::Green),
    (["◐", "w2", "sonnet-4.6", "patch src/client.rs", "Working", "27,480", "$0.061", "27s ago"], Tone::Cite),
    (["·", "w3", "opus-4.6", "review the diff", "Queued", "—", "—", "—"], Tone::Dim),
];
/// mark · service · detail · tone. The last row is the error sparkline, which has no mark.
const SERVICES: [([&str; 3], Tone); 4] = [
    ([Mark::Landed.glyph(), "api", "071b1aa6 · 16/246"], Tone::Green),
    ([Mark::Landed.glyph(), "postgres", "17/60 · 47% disk"], Tone::Green),
    ([Mark::Blocked.glyph(), "postgrest", "restarting · 4m"], Tone::Warn),
    (["", "errors 1h", "▁▁▂█▂  47"], Tone::Dim),
];
/// text · tone · indent. The indent is an argument to `cols::row`, never a padded string.
const CONVERSATION: [(&str, Tone, usize); 11] = [
    ("❯ add retry to the http client", Tone::Cite, 0),
    ("", Tone::Dim, 0),
    ("• Three attempts, exponential backoff, jitter.", Tone::Mid, 0),
    ("• The 429 path keeps the Retry-After header.", Tone::Mid, 0),
    ("", Tone::Dim, 0),
    ("⏺ Edit(src/client.rs)", Tone::Green, 0),
    ("⎿  +34 −6 · gate clean", Tone::Dim, 3),
    ("", Tone::Dim, 0),
    ("◐ Working · 31s · 3 of 4 landed", Tone::Warn, 0),
    ("", Tone::Dim, 0),
    ("", Tone::Dim, 0),
];
/// The tab strip. Each chip carries its own padding INSIDE the cell so the tint band reads as a
/// chip rather than stopping at the last letter; `cols` still owns where the next chip starts.
const TABS: [Col; 6] =
    [Col::l(8), Col::l(14), Col::l(17), Col::l(14), Col::l(12), Col::l(7)];
const AGENT_TABS: [(&str, Mark); 4] = [
    ("scope", Mark::Landed),
    ("implement", Mark::InFlight),
    ("review", Mark::Queued),
    ("gate", Mark::Queued),
];
/// Which tab is live. One index, so the strip cannot show two actives or none.
const ACTIVE_AGENT: usize = 1;

/// A strip of chips: a leading label, one chip per entry, and `+ new`. The active chip is lifted
/// onto `palette.tint`; every other carries only its mark's colour.
fn strip(columns: &[Col], label: &str, tabs: &[(&str, Mark)], active: usize, palette: &Palette)
-> Line<'static> {
    assert!(active < tabs.len(), "the active tab must exist");
    assert_eq!(
        columns.len(),
        tabs.len() + 2,
        "a chip strip is one label column, one column per chip, and `+ new`"
    );
    let chips = tabs
        .iter()
        .map(|(name, mark)| format!(" {} {name} ", mark.glyph()))
        .collect::<Vec<_>>();
    let mut cells = vec![Cell(label, palette.dim)];
    cells.extend(
        chips
            .iter()
            .zip(tabs)
            .map(|(chip, (_, mark))| Cell(chip.as_str(), mark.colour(palette))),
    );
    cells.push(Cell("+ new", palette.dim));
    lift(owned(row(columns, &cells, 0)), &chips[active], palette)
}

/// One row of the split: left content, the divider column, right content — all placed by [`PANE`].
fn split_row(palette: &Palette, left: (&str, Color), right: (&str, Color)) -> Line<'static> {
    owned(row(
        &PANE,
        &[
            Cell(left.0, left.1),
            Cell("│", palette.dim),
            Cell(right.0, right.1),
        ],
        0,
    ))
}

/// The right pane: production over orchestra, every row aligned by `cols` before it is flattened.
fn rails(palette: &Palette) -> Vec<(String, Color)> {
    let mut rows = SERVICES
        .iter()
        .map(|(cells, tone)| {
            let cells = cells.map(|text| Cell(text, Color::Reset));
            (flatten(&row(&SERVICE, &cells, 0)), tone.colour(palette))
        })
        .collect::<Vec<_>>();
    rows.push((String::new(), palette.dim));
    rows.push((
        rule_text("orchestra", "fleet 3 · rev 7", PANE_RIGHT),
        palette.dim,
    ));
    rows.push((
        flatten(&head(&FLEET, &FLEET_HEAD, Color::Reset, 0)),
        palette.dim,
    ));
    rows.extend(WORKERS.iter().map(|(cells, tone)| {
        let cells = cells.map(|text| Cell(text, Color::Reset));
        (flatten(&row(&FLEET, &cells, 0)), tone.colour(palette))
    }));
    rows.push((
        "fleet 31,600 tokens · $0.062 this run".to_string(),
        palette.dim,
    ));
    rows
}

pub(crate) fn panels(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let right = rails(palette);
    assert_eq!(
        CONVERSATION.len(),
        right.len(),
        "the two panes must have equal row counts or one of them silently loses content"
    );
    assert_eq!(PANE[1].w, 1, "the divider is exactly one column wide");

    let mut out = vec![
        strip(&TABS, "agents", &AGENT_TABS, ACTIVE_AGENT, palette),
        blank(),
        split_row(
            palette,
            (&rule_text("agents", "4 in this terminal", PANE_LEFT), palette.dim),
            (&rule_text("production", "fernpost/checkout-api", PANE_RIGHT), palette.dim),
        ),
    ];
    for ((text, tone, indent), (right_text, right_colour)) in CONVERSATION.iter().zip(right.iter()) {
        let left = indented(text, *indent, PANE_LEFT - indent);
        out.push(split_row(
            palette,
            (&left, tone.colour(palette)),
            (right_text, *right_colour),
        ));
    }
    out.push(split_row(
        palette,
        (&RULE.repeat(PANE_LEFT), palette.dim),
        (&RULE.repeat(PANE_RIGHT), palette.dim),
    ));
    out.push(blank());
    let alert = "postgrest has been restarting for 4m";
    out.push(headline(Mark::Blocked, alert, "the monitor opened a repair", palette, tick, pulse));
    out.push(blank());
    out.push(note(palette, &format!(
        "orchestra · {MISSING_PER_WORKER_SPEND} — the model, tokens and price cells above are FIXTURE"
    )));
    for line in [
        "last seen · when the worker last reported state, not how long it has run; a row dated ahead of this clock reads \"clock ahead\", never 0s",
        "tab strip · click a tab · drag the divider · ctrl+\\ splits · ctrl+w closes a VIEW, never the agent",
    ] {
        out.push(note(palette, line));
    }
    out
}

// ── 35 · several sessions in one terminal ────────────────────────────────

/// ⚠️ The SHIPPED strip (`live_renderer::session_tabs_line`) marks the active tab with `+` and
/// every other with `·`. The book draws the product's own mark vocabulary instead; the key hint
/// below is copied from that function verbatim so the two cannot disagree about the binding.
const SESSION_TABS: [Col; 6] =
    [Col::l(10), Col::l(18), Col::l(21), Col::l(21), Col::l(17), Col::l(7)];
const SESSION_ROWS: [Col; 5] = [Col::l(18), Col::l(26), Col::l(12), Col::l(16), Col::r(8)];
const SESSION_HEAD: [&str; 5] = ["session", "repo", "state", "last activity", "spend"];
/// 🔴 ONE OWNER FOR THE SESSION LIST. The chip strip and the table below are both derived from
/// this, so a session cannot appear on the strip and be missing from the table.
/// session · repo · state · last activity · spend · mark
const SESSIONS: [(&str, &str, &str, &str, &str, Mark); 4] = [
    ("checkout-api", "fernpost/checkout-api", "landed", "2m ago", "$0.41", Mark::Landed),
    ("payments-worker", "fernpost/payments", "in flight", "4s ago", "$1.07", Mark::InFlight),
    ("infra-terraform", "fernpost/infra", "queued", "18m ago", "$0.00", Mark::Queued),
    ("design-book", "uqeu/estelle", "queued", "1h ago", "$0.12", Mark::Queued),
];
const ACTIVE_SESSION: usize = 1;

pub(crate) fn session_tabs(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    assert_eq!(SESSIONS.len(), 4, "the strip and the table share one list");
    assert!(ACTIVE_SESSION < SESSIONS.len(), "the active session exists");
    let tabs = SESSIONS.map(|(name, _, _, _, _, mark)| (name, mark));
    let mut out = vec![
        strip(&SESSION_TABS, "sessions", &tabs, ACTIVE_SESSION, palette),
        blank(),
        owned(rule("sessions", "4 open · 1 in flight", 138, palette.dim, palette.mid, palette.cite)),
        owned(head(&SESSION_ROWS, &SESSION_HEAD, palette.dim, 2)),
    ];
    for (index, (session, repo, state, seen, spend, mark)) in SESSIONS.iter().enumerate() {
        let line = owned(row(
            &SESSION_ROWS,
            &[
                Cell(session, palette.bright),
                Cell(repo, palette.mid),
                Cell(state, mark.colour(palette)),
                Cell(seen, palette.dim),
                Cell(spend, palette.dim),
            ],
            2,
        ));
        out.push(if index == ACTIVE_SESSION {
            line.style(Style::default().bg(palette.tint))
        } else {
            line
        });
    }
    out.push(blank());
    let repair = "payments-worker is mid-repair";
    out.push(headline(Mark::InFlight, repair, "round 2 of 3 · $1.07 so far", palette, tick, pulse));
    out.push(blank());
    // ⚠️ Verbatim from `live_renderer::session_tabs_line`. If that string moves, this one is wrong.
    for line in [
        "Alt+Left/Right switch · Ctrl+W close view",
        "closing a view leaves the session running on the server; /resume reopens it where it stopped",
        "4 sessions · $1.60 this terminal · spend is counted per session, never per view",
    ] {
        out.push(note(palette, line));
    }
    out
}

// ── 38 · the sweep, and what the capacity check answered ─────────────────

/// The five states are `top_level::sweep_with_progress`'s own strings and its own percentages.
/// ⚠️ 35 then 30 is not a typo and not a regression: they are two transports and exactly one of
/// them runs. The screen says so out loud rather than quietly sorting the ladder.
const SWEEP_STEPS: [(&str, u16, &str); 5] = [
    ("files collected safely", 10, "1,993 files · 24.1 MB · 61 skipped"),
    ("checking account capacity", 20, "POST /sweep/estimate · its answer is below"),
    ("sending source set", 35, "sync transport · under 400 files"),
    ("starting background ingest", 30, "background transport · 400 files or more"),
    ("repo swept", 100, "the graph is current for this commit"),
];
const ACTIVE_STEP: usize = 1;
const STEPS: [Col; 4] = [Col::l(2), Col::l(28), Col::r(5), Col::l(44)];
const CAPACITY: [Col; 3] = [Col::l(20), Col::r(8), Col::l(44)];
const CAPACITY_HEAD: [&str; 3] = ["field", "value", "what the number means"];
/// The step this frame is caught on, and the book's needle for screen 38.
const ACTIVE: &str = "checking account capacity";
const SWEEPING: &str = "Sweeping fernpost/checkout-api";
/// 🔴 The estimate's OWN field names and OWN values — `estimated_tokens`, `held_tokens`, `cap`,
/// `remaining_tokens`. Nothing here is derived, so nothing here can be derived wrongly.
const CAPACITY_ROWS: [(&str, &str, &str, Tone); 4] = [
    ("estimated_tokens", "11.5M", "what this sweep would add", Tone::Mid),
    ("held_tokens", "103M", "already held across 6 repos", Tone::Mid),
    ("cap", "250M", "the plan's ceiling on tokens held", Tone::Mid),
    ("remaining_tokens", "147M", "free after this sweep lands", Tone::Green),
];

pub(crate) fn sweep_running(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    assert_eq!(SWEEP_STEPS.len(), 5, "the sweep has five named states");
    assert_eq!(
        SWEEP_STEPS[ACTIVE_STEP].0, ACTIVE,
        "the active step is the estimate call whose answer this screen exists to show"
    );

    let mut out = vec![
        headline(Mark::InFlight, SWEEPING, "step 2 of 5 · 20%", palette, tick, pulse),
        blank(),
        owned(rule("sweep", ACTIVE, 118, palette.dim, palette.mid, palette.cite)),
    ];
    for (index, (state, percent, detail)) in SWEEP_STEPS.iter().enumerate() {
        let mark = match index.cmp(&ACTIVE_STEP) {
            std::cmp::Ordering::Less => StepMark::Done,
            std::cmp::Ordering::Equal => StepMark::Active,
            std::cmp::Ordering::Greater => StepMark::NotStarted,
        };
        let percent = format!("{percent}%");
        let line = owned(row(
            &STEPS,
            &[
                Cell(mark.glyph(), mark.colour(palette)),
                Cell(state, mark.colour(palette)),
                Cell(&percent, palette.dim),
                Cell(detail, palette.dim),
            ],
            2,
        ));
        out.push(match mark.row_background(palette) {
            Some(background) => line.style(Style::default().bg(background)),
            None => line,
        });
    }
    out.push(blank());
    let estimate = "POST /sweep/estimate";
    out.push(owned(rule("account capacity", estimate, 118, palette.dim, palette.mid, palette.green)));
    out.push(note(palette, "1,993 files · 11.5M tokens · 103M of 250M held · 147M free"));
    out.push(owned(head(&CAPACITY, &CAPACITY_HEAD, palette.dim, 2)));
    for (field, value, meaning, tone) in CAPACITY_ROWS {
        out.push(owned(row(
            &CAPACITY,
            &[
                Cell(field, palette.dim),
                Cell(value, tone.colour(palette)),
                Cell(meaning, palette.dim),
            ],
            2,
        )));
    }
    out.push(blank());
    for line in [
        "35% then 30% is not a regression: they are two transports and only one of them runs",
        "under 400 files the whole set is sent; at 400 or more an ingest starts and continues if this terminal closes",
        "cost and budget stay on screen: every figure above is a field the estimate returned, not a derived one",
    ] {
        out.push(note(palette, line));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    const CORNERS: [char; 9] = ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];

    fn screen_text(lines: &[Line<'static>]) -> String {
        lines.iter().map(flatten).collect::<Vec<_>>().join("\n")
    }

    /// 🔴 THE PROPERTY `cols` EXISTS FOR, AND THE ONE A HAND-COUNTED SPLIT GETS WRONG.
    ///
    /// Two clauses, because either alone passes on a broken screen: the divider must appear on
    /// MANY rows (a screen that lost its split has no divider to misplace), and every appearance
    /// must sit at the column [`PANE`] puts it at — measured in CHARS, since `❯`, `●` and `─` are
    /// all multi-byte and a byte index would drift row by row.
    #[test]
    fn the_pane_divider_lands_on_one_column_on_every_row() {
        let palette = ScreenTheme::Dark.palette();
        let mut columns = std::collections::BTreeSet::new();
        let mut rows_with_a_divider = 0;
        for line in panels(&palette, 0, true) {
            for (index, character) in flatten(&line).chars().enumerate() {
                if character == '│' {
                    columns.insert(index);
                    rows_with_a_divider += 1;
                }
            }
        }
        assert!(
            rows_with_a_divider >= 12,
            "only {rows_with_a_divider} rows carried a divider — did the split survive?"
        );
        assert_eq!(
            columns.iter().copied().collect::<Vec<_>>(),
            vec![PANE[0].w + PANE[0].gap],
            "the divider wandered off its column"
        );
    }

    /// 🔴 NO CORNERS — AND THE SPLIT STILL THERE.
    ///
    /// The second half keeps this honest: a screen that drew no divider and no rule would pass a
    /// corner check trivially, so `panels` must contain BOTH `│` and `─`.
    #[test]
    fn no_pane_joins_a_rule_into_a_corner() {
        let palette = ScreenTheme::Dark.palette();
        let panels_text = screen_text(&panels(&palette, 7, true));
        for (name, text) in [
            ("panels", panels_text.clone()),
            ("session_tabs", screen_text(&session_tabs(&palette, 7, true))),
            ("sweep_running", screen_text(&sweep_running(&palette, 7, true))),
        ] {
            for corner in CORNERS {
                assert!(!text.contains(corner), "{name} drew a box corner {corner:?}");
            }
        }
        assert!(panels_text.contains('│'), "the divider vanished");
        assert!(panels_text.contains('─'), "the rules vanished");
    }

    /// 🔴 THE FOUNDER COULD NOT READ `age`, SO THE COLUMN IS `last seen` AND EVERY OTHER COLUMN IS
    /// SPELLED OUT TOO. The negative clause is the half that can regress: re-introducing `age`
    /// would still satisfy every positive assertion above it.
    #[test]
    fn every_orchestra_column_is_labelled_in_words() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = panels(&palette, 0, true);
        let header = rendered
            .iter()
            .map(flatten)
            .find(|text| text.contains("wkr"))
            .expect("the fleet header row");
        for label in ["wkr", "model", "task", "state", "tokens", "price", "last seen"] {
            assert!(header.contains(label), "{label:?} missing from {header:?}");
        }
        assert!(
            !header.contains("age"),
            "the unreadable column came back: {header:?}"
        );
        // And the fixture cells are disclosed rather than passed off as wire data.
        let text = screen_text(&rendered);
        assert!(text.contains(MISSING_PER_WORKER_SPEND), "{text}");
        assert!(text.contains("clock ahead"), "{text}");
    }

    /// The step name alone would be a screen that says a capacity check happened and never says
    /// what it answered. Held, cap and free are three different numbers and all three must land.
    #[test]
    fn the_sweep_shows_what_the_capacity_check_returned() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = sweep_running(&palette, 0, true);
        let text = screen_text(&rendered);
        assert!(text.contains("checking account capacity"), "{text}");
        for figure in ["1,993 files", "11.5M", "103M of 250M held", "147M free", "250M"] {
            assert!(text.contains(figure), "{figure:?} missing from {text}");
        }
        assert!(text.contains("remaining_tokens"), "{text}");
        // Exactly one step is lifted, and it is the one the headline names.
        let lifted = rendered
            .iter()
            .filter(|line| line.style.bg == Some(palette.tint))
            .count();
        assert_eq!(lifted, 1, "exactly one sweep step is active");
    }

    /// A row wider than its frame is clipped by the gallery and reads as a typo. Checked at the
    /// widths `design_book::SCREENS` gives these three, in both palettes.
    #[test]
    fn no_row_overruns_the_frame_the_book_gives_it() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for (name, width, lines) in [
                ("panels", 180usize, panels(&palette, 0, true)),
                ("session_tabs", 140, session_tabs(&palette, 0, true)),
                ("sweep_running", 120, sweep_running(&palette, 0, true)),
            ] {
                for line in &lines {
                    let rendered = flatten(line).chars().count();
                    assert!(
                        rendered <= width,
                        "{name} rendered a {rendered}-column row into a {width}-column frame"
                    );
                }
            }
        }
    }

    /// The fleet summary must be the sum of its own rows. A fabricated number that does not even
    /// reconcile is the failure this interface exists to refuse.
    #[test]
    fn the_fleet_summary_is_the_sum_of_the_rows_above_it() {
        let palette = ScreenTheme::Dark.palette();
        let text = screen_text(&panels(&palette, 0, true));
        assert!(text.contains("4,120") && text.contains("27,480"), "{text}");
        assert!(text.contains("31,600 tokens"), "4,120 + 27,480 = 31,600");
        assert!(text.contains("$0.001") && text.contains("$0.061"), "{text}");
        assert!(text.contains("$0.062 this run"), "0.001 + 0.061 = 0.062");
    }

    /// Exactly one chip is active on each strip, and it is lifted onto the `tint` role rather than
    /// given a colour of its own — the same role `StepMark::row_background` uses.
    #[test]
    fn one_chip_is_lifted_onto_the_tint_role_on_each_strip() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for (name, mut screen) in [
                ("panels", panels(&palette, 0, true)),
                ("session_tabs", session_tabs(&palette, 0, true)),
            ] {
                let lifted = screen
                    .remove(0)
                    .spans
                    .iter()
                    .filter(|span| span.style.bg == Some(palette.tint))
                    .count();
                assert_eq!(lifted, 1, "{name} lifted {lifted} chips");
            }
        }
    }

    /// 🔴 THE VACUITY GUARD, PINNED TO THE BOOK ENTRY RATHER THAN RETYPED.
    ///
    /// The needle is read out of `SCREENS` and the renderer is called THROUGH the table's own
    /// function pointer, so this cannot pass on a screen the book no longer points at — and a
    /// needle edited in `mod.rs` without the screen following it goes red here rather than in a
    /// gallery run nobody watches.
    #[test]
    fn each_pane_screen_renders_the_needle_its_book_entry_promises() {
        let palette = ScreenTheme::Dark.palette();
        for name in ["25-panels-one-terminal", "35-session-tabs", "38-sweep-running"] {
            let screen = crate::design_book::SCREENS
                .iter()
                .find(|screen| screen.name == name)
                .unwrap_or_else(|| panic!("{name} is no longer in the book"));
            let text = screen_text(&(screen.render)(&palette, 0, true));
            assert!(
                text.contains(screen.needle),
                "{name} lost its needle {:?}",
                screen.needle
            );
        }
    }
}

