//! What every design-book screen is built out of: the catalog splitter, the table renderer, the
//! rule, the footnote, the status line and the three colour laws.
//!
//! 🔴 **THIS FILE EXISTS BECAUSE `surfaces.rs` REACHED 961 LINES.** The house rule is 200–400
//! typical and **800 hard**, and `cargo fmt --check` is a CI gate that only ever adds rows. A file
//! that is over the limit does not get read; it gets skimmed, and a skimmed file is where a
//! hand-counted layout survives a review. Splitting it is cheaper than the alternative, which is
//! nobody noticing the next `sk-ant-…4f2c`.
//!
//! ⚠️ Every helper here was already shared by more than one screen before the move. Nothing was
//! generalised on the way out — a helper invented during a split is a second owner nobody asked
//! for.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, row, rule};
use crate::design_book::note;
use crate::design_book::owned;
use crate::marks::Mark::{Blocked, InFlight, Landed, Queued, Refused};
use crate::marks::{Mark, StepMark};
use crate::theme::Palette;
/// Split one `|`-delimited catalog row into exactly `N` trimmed cells. Rows are text rather than
/// tuples because a four-field tuple with an eighty-column description is six source lines once
/// `rustfmt` has had it, and this file holds sixty of them. ⚠️ The arity is asserted in BOTH
/// directions: a row that quietly lost a field renders its note under `state` and still looks
/// like a perfectly good table.
pub(crate) fn fields<const N: usize>(source: &str) -> [&str; N] {
    let mut cells = [""; N];
    let mut count = 0usize;
    for part in source.split('|') {
        assert!(count < N, "{source:?} carries more than {N} fields");
        cells[count] = part.trim();
        count += 1;
    }
    assert_eq!(count, N, "{source:?} carries the wrong number of fields");
    cells
}

/// Render a `|`-delimited catalog into aligned rows. `paint` is the only thing that differs
/// between the twelve tables here: it turns a row's `N` source fields into the `M` drawn cells,
/// their colours, and whether the row is the selected one. The split, the arity check, the column
/// layout, the `'static` re-own and the tint band happen here once, so no screen gets one wrong.
pub(crate) fn table<const N: usize, const M: usize>(
    palette: &Palette,
    spec: &[Col],
    rows: &[&'static str],
    paint: impl Fn(&Palette, usize, [&'static str; N]) -> ([&'static str; M], [Color; M], bool),
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(index, source)| {
            let (texts, inks, highlight) = paint(palette, index, fields::<N>(source));
            let cells = texts
                .into_iter()
                .zip(inks)
                .map(|(text, ink)| Cell(text, ink))
                .collect::<Vec<_>>();
            let line = owned(row(spec, &cells, 2));
            if highlight {
                line.style(Style::default().bg(palette.tint))
            } else {
                line
            }
        })
        .collect()
}

/// `── label · mode ───…`. No corners; a rule cannot close into a panel.
pub(crate) fn bar(
    palette: &Palette,
    label: &str,
    mode: &str,
    wide: usize,
    ink: Color,
) -> Line<'static> {
    owned(rule(label, mode, wide, palette.dim, palette.mid, ink))
}

pub(crate) fn prose(palette: &Palette, text: &[&str]) -> Vec<Line<'static>> {
    text.iter().map(|line| note(palette, line)).collect()
}

/// The founder's own status line, transcribed from his 2026-08-24 capture:
/// `~/estelle · main · $0.104 · ◐ affinity`. Global rule 2 — cost and budget are always visible.
pub(crate) fn status(palette: &Palette, left: &str, mark: Mark, state: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {left} · "), Style::default().fg(palette.dim)),
        Span::styled(
            mark.glyph().to_string(),
            Style::default().fg(mark.colour(palette)),
        ),
        Span::styled(format!(" {state}"), Style::default().fg(palette.dim)),
    ])
}

/// The rail vocabulary by name, so a catalog row carries its own status: `(glyph, colour)`.
pub(crate) fn rail(palette: &Palette, name: &str) -> (&'static str, Color) {
    let mark = match name {
        "landed" => Landed,
        "blocked" => Blocked,
        "inflight" => InFlight,
        "refused" => Refused,
        _ => Queued,
    };
    (mark.glyph(), mark.colour(palette))
}

/// A plan step's `(glyph, colour, is-the-active-one)`. Only the ACTIVE step lifts its row.
pub(crate) fn step_of(palette: &Palette, name: &str) -> (&'static str, Color, bool) {
    let mark = StepMark::from_status(name);
    let lifted = mark.row_background(palette).is_some();
    (mark.glyph(), mark.colour(palette), lifted)
}

/// The command audit's colour law: `(glyph, colour)`. 🔴 `refused` is BLUE, not red, on the
/// founder's instruction: red would read as "this failed", and nothing failed — the name was
/// advertised and never wired.
pub(crate) fn verdict(palette: &Palette, kind: &str) -> (&'static str, Color) {
    match kind {
        "refused" => (Mark::Refused.glyph(), palette.cite),
        "inert" | "near-miss" => (Mark::Blocked.glyph(), palette.warn),
        "duplicate" => (Mark::Queued.glyph(), palette.dim),
        _ => (Mark::Landed.glyph(), palette.green),
    }
}
