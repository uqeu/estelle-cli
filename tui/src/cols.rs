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

pub fn head<'a>(cols: &[Col], labels: &[&'a str], dim: Color, indent: usize) -> Line<'a> {
    let cells = labels
        .iter()
        .map(|label| Cell(label, dim))
        .collect::<Vec<_>>();
    row(cols, &cells, indent)
}

pub fn rule<'a>(
    label: &'a str,
    mode: &'a str,
    width: usize,
    dim: Color,
    mid: Color,
    accent: Color,
) -> Line<'a> {
    let used = 3 + label.chars().count() + 3 + mode.chars().count() + 1;
    let dashes = width.saturating_sub(used).max(4);
    Line::from(vec![
        Span::styled("╌╌ ", Style::default().fg(dim)),
        Span::styled(label, Style::default().fg(mid)),
        Span::styled(" · ", Style::default().fg(dim)),
        Span::styled(mode, Style::default().fg(accent)),
        Span::raw(" "),
        Span::styled("╌".repeat(dashes), Style::default().fg(dim)),
    ])
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
