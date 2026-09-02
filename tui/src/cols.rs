//! Column layout primitives shared by Estelle's diagnostic screens.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    L,
    R,
}

#[derive(Clone, Copy)]
pub struct Col {
    pub w: usize,
    pub a: Align,
    pub gap: usize,
}

impl Col {
    pub const fn l(w: usize) -> Self {
        Col {
            w,
            a: Align::L,
            gap: 2,
        }
    }

    pub const fn r(w: usize) -> Self {
        Col {
            w,
            a: Align::R,
            gap: 2,
        }
    }

    #[allow(dead_code)]
    pub const fn gap(mut self, gap: usize) -> Self {
        self.gap = gap;
        self
    }
}

pub struct Cell<'a>(pub &'a str, pub Color);

pub fn row<'a>(cols: &[Col], cells: &[Cell<'a>], indent: usize) -> Line<'a> {
    let mut spans = Vec::with_capacity(cells.len() * 2 + 1);
    if indent > 0 {
        spans.push(Span::raw(" ".repeat(indent)));
    }
    for (index, (col, cell)) in cols.iter().zip(cells.iter()).enumerate() {
        let text = truncate(cell.0, col.w);
        let pad = col.w.saturating_sub(text.chars().count());
        match col.a {
            Align::R => {
                if pad > 0 {
                    spans.push(Span::raw(" ".repeat(pad)));
                }
                spans.push(Span::styled(text, Style::default().fg(cell.1)));
            }
            Align::L => {
                spans.push(Span::styled(text, Style::default().fg(cell.1)));
                if pad > 0 {
                    spans.push(Span::raw(" ".repeat(pad)));
                }
            }
        }
        if index + 1 < cols.len() {
            spans.push(Span::raw(" ".repeat(col.gap)));
        }
    }
    Line::from(spans)
}

/// Re-own a `Line` whose spans borrow local `String`s.
///
/// ⚠️ **THE REASON THIS EXISTS RATHER THAN `Box::leak`.** [`row`] borrows its cells, so a row built
/// from computed text is a `Line<'_>` tied to locals. `screens.rs` reached for `Box::leak` to get
/// `'static`, which leaks a string per call and is invisible at the call site. Copying the spans is
/// the same cost once and no cost forever after.
///
/// 🔴 **IT LIVES HERE, BESIDE THE FUNCTION THAT CREATES THE BORROW.** It was written inside
/// `design_book`, which meant a PRODUCTION renderer that computes its cells had to depend on the
/// design book to escape a lifetime `cols` had introduced — or write a second copy. One owner, and
/// in the module whose API made it necessary.
pub fn owned(line: Line<'_>) -> Line<'static> {
    let style = line.style;
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
    .style(style)
}

/// The slice of a long list to draw so the selected row is always on screen.
///
/// 🔴 **A WALKABLE LIST WITH NO WINDOW IS A PICTURE AGAIN, AND A LIVE WALK IS WHAT PROVED IT.**
/// `/memories` against production returned 200 rows into a 50-line terminal on 2026-09-02: every
/// row past the fold was unreachable by any keypress, and `↓` moved a band the reader could no
/// longer see. `skills_filtered`'s docstring records the same defect one screen over —
/// *"handing it 247 rows does not produce a long list, it produces a list whose tail cannot be
/// reached"*. The bound belongs at the point the surface is built.
///
/// Returns `(first, count)`. `count` is `0` only when there is nothing to draw or no room for it.
///
/// ⚠️ The band is kept CENTRED where it can be, and pinned at whichever end it has reached. A
/// window that only scrolled when the cursor left it makes the last row of a page jump the whole
/// page, which is the motion a reader loses their place in.
pub fn window(total: usize, cursor: usize, visible: usize) -> (usize, usize) {
    if total == 0 || visible == 0 {
        return (0, 0);
    }
    let count = visible.min(total);
    let first = cursor.saturating_sub(count / 2).min(total - count);
    debug_assert!(first + count <= total);
    debug_assert!(count == total || (first..first + count).contains(&cursor.min(total - 1)));
    (first, count)
}

pub fn head<'a>(cols: &[Col], labels: &[&'a str], dim: Color, indent: usize) -> Line<'a> {
    let cells = labels
        .iter()
        .map(|label| Cell(label, dim))
        .collect::<Vec<_>>();
    row(cols, &cells, indent)
}

/// The rule texture, and the single owner of it.
///
/// 🔴 **U+2500 LIGHT BOX RULE, SOLID, NO DASHES** — the founder picked variant D off the rendered
/// specimen sheet over the dense `╌`, the spaced `╌`, the ASCII `---` the demo video used, and the
/// finer `┄`. Every rule in the product reads this constant: the session, production, ask and
/// cited rules, every in-pane section rule and every panel title. There is no second place to
/// change the texture, which is the point — the last time a design token had two owners the
/// catalog and the terminal disagreed for four days.
///
/// ⚠️ It is NOT a box corner. `─` is a horizontal rule; `┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼` are what make a box and
/// the box guard counts those. A rule with no corners cannot close into a panel.
pub const RULE: &str = "─";

/// `── label · mode ───…`, or `── label ───…` when there is no mode.
///
/// ⚠️ An empty `mode` drops the separator with it. A pane that has no second half — `settings`,
/// `skills` — would otherwise render `╌╌ settings ·  ╌╌`, a separator pointing at nothing.
pub fn rule<'a>(
    label: &'a str,
    mode: &'a str,
    width: usize,
    dim: Color,
    mid: Color,
    accent: Color,
) -> Line<'a> {
    let separator = if mode.is_empty() { 0 } else { 3 };
    let used = 3 + label.chars().count() + separator + mode.chars().count() + 1;
    let dashes = width.saturating_sub(used).max(4);
    let mut spans = vec![
        Span::styled(format!("{RULE}{RULE} "), Style::default().fg(dim)),
        Span::styled(label, Style::default().fg(mid)),
    ];
    if !mode.is_empty() {
        spans.push(Span::styled(" · ", Style::default().fg(dim)));
        spans.push(Span::styled(mode, Style::default().fg(accent)));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(RULE.repeat(dashes), Style::default().fg(dim)));
    Line::from(spans)
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        value.to_string()
    } else {
        let mut output = value
            .chars()
            .take(width.saturating_sub(1))
            .collect::<String>();
        output.push('…');
        output
    }
}

#[cfg(test)]
mod tests {
    use super::window;

    /// The band is always inside the window, at both ends and in the middle.
    #[test]
    fn the_window_always_contains_the_band_and_never_runs_past_the_list() {
        for total in [0usize, 1, 3, 200] {
            for visible in [0usize, 1, 5, 20] {
                for cursor in 0..total.max(1) {
                    let (first, count) = window(total, cursor, visible);
                    assert!(first + count <= total, "{total}/{cursor}/{visible}");
                    if count > 0 && cursor < total {
                        assert!(
                            (first..first + count).contains(&cursor),
                            "the band fell outside the window: {total}/{cursor}/{visible}"
                        );
                    }
                }
            }
        }
        // A list shorter than the window is drawn whole, from the top.
        assert_eq!(window(3, 2, 20), (0, 3));
        // A cursor at the end pins the window to the end rather than scrolling past it.
        assert_eq!(window(200, 199, 10), (190, 10));
        // An empty list draws nothing rather than one blank row.
        assert_eq!(window(0, 0, 10), (0, 0));
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    fn width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum()
    }

    #[test]
    fn every_row_in_a_table_is_the_same_width() {
        let cols = [Col::l(2), Col::l(17), Col::r(6), Col::r(3)];
        let a = row(
            &cols,
            &[
                Cell("●", Color::Reset),
                Cell("claude-opus-5", Color::Reset),
                Cell("—", Color::Reset),
                Cell("0", Color::Reset),
            ],
            0,
        );
        let b = row(
            &cols,
            &[
                Cell("○", Color::Reset),
                Cell("kimi-k3", Color::Reset),
                Cell("91.2", Color::Reset),
                Cell("40", Color::Reset),
            ],
            0,
        );
        assert_eq!(width(&a), width(&b));
        assert_eq!(width(&a), 34);
    }

    #[test]
    fn multibyte_glyphs_count_as_one_column() {
        let cols = [Col::l(4), Col::l(6)];
        let plain = row(
            &cols,
            &[Cell("x", Color::Reset), Cell("y", Color::Reset)],
            0,
        );
        let fancy = row(
            &cols,
            &[Cell("⏺", Color::Reset), Cell("⎿", Color::Reset)],
            0,
        );
        assert_eq!(width(&plain), width(&fancy));
        assert_eq!(width(&fancy), 12);
    }

    #[test]
    fn an_overlong_cell_is_truncated_not_allowed_to_push_the_row() {
        let cols = [Col::l(8), Col::l(4)];
        let long = row(
            &cols,
            &[
                Cell("a-very-long-model-name", Color::Reset),
                Cell("ok", Color::Reset),
            ],
            0,
        );
        assert_eq!(width(&long), 14);
        let joined: String = long
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            joined.contains('…'),
            "truncation must be visible, got {joined:?}"
        );
    }

    #[test]
    fn a_rule_with_no_mode_drops_the_separator_with_it() {
        let text = |line: &Line<'_>| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };
        let with = rule(
            "gate",
            "refused",
            40,
            Color::Reset,
            Color::Reset,
            Color::Reset,
        );
        let without = rule("settings", "", 40, Color::Reset, Color::Reset, Color::Reset);

        assert!(
            text(&with).starts_with("── gate · refused ─"),
            "{}",
            text(&with)
        );
        assert!(
            text(&without).starts_with("── settings ─"),
            "{}",
            text(&without)
        );
        assert!(!text(&without).contains(" · "));
        // Both still fill the same row: dropping the separator lengthens the dashes, not the line.
        assert_eq!(width(&with), 40);
        assert_eq!(width(&without), 40);
    }

    #[test]
    fn right_aligned_numbers_end_on_the_same_column() {
        let cols = [Col::l(6), Col::r(8)];
        let a = row(
            &cols,
            &[Cell("in", Color::Reset), Cell("$5.00", Color::Reset)],
            0,
        );
        let b = row(
            &cols,
            &[Cell("out", Color::Reset), Cell("$25.00", Color::Reset)],
            0,
        );
        let a_text: String = a.spans.iter().map(|span| span.content.as_ref()).collect();
        let b_text: String = b.spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(a_text.find("$5.00").map(|start| start + 5), Some(16));
        assert_eq!(b_text.find("$25.00").map(|start| start + 6), Some(16));
    }
}
