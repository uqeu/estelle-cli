//! The four ANSWER surfaces: screens 34, 39, 40 and 41.
//!
//! What an answer looks like when it carries a table and a diagram (34), what a tool call looks
//! like collapsed and expanded (39), what the code graph looks like when you walk it (40), and
//! what memory looks like when you *correct* it (41).
//!
//! 🔴 **THE FOUNDER'S DECISION ON 34.** *"Tables and diagrams render as MERMAID. Estelle should be
//! able to draw in mermaid."* A terminal cannot lay out every mermaid dialect, so the screen names
//! the ones it draws and the ones that fall back to the fenced source. Claiming the rest would be
//! the same defect as claiming a benchmark nobody ran.
//!
//! ⚠️ **Nothing here is positioned by hand** — every aligned row goes through [`crate::cols`],
//! including the markdown rule under the table head and every line of the drawn diagram. The
//! `#[rustfmt::skip]` on each data table is deliberate: a row of the SOURCE reads as a row of the
//! SCREEN, which is how a reviewer sees a wrong cell without rendering it.

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::cols::Cell;
use crate::cols::Col;
use crate::cols::RULE;
use crate::cols::head;
use crate::cols::row;
use crate::cols::rule;
use crate::design_book::blank;
use crate::design_book::note;
use crate::design_book::owned;
use crate::theme::Palette;

/// One line of prose in a chosen colour, indented two — [`crate::design_book::note`]'s sibling for
/// the lines that are not dim. Prose only: anything that lines up with a neighbour is a
/// [`crate::cols::row`].
fn said(color: Color, text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(color),
    ))
}

/// The colour the pulse is showing this tick, with the dampening left to its one owner.
///
/// ⚠️ Re-deriving the dampened value here would give the pulse a second owner, and the last time a
/// derived fact had two owners they disagreed for four days.
fn pulsed(base: Color, tick: u64, enabled: bool) -> Color {
    crate::theme::pulse(base, tick, enabled).fg.unwrap_or(base)
}

/// A section rule in the product's colours: a screen names a label, a mode and an accent, because
/// the dim and mid halves are the same on every screen in the book.
fn band(
    p: &Palette,
    label: &'static str,
    mode: &'static str,
    width: usize,
    accent: Color,
) -> Line<'static> {
    rule(label, mode, width, p.dim, p.mid, accent)
}

// ── 34 · the answer that carries a table and a diagram ───────────────────────────────────────

/// The rendered markdown table. Four columns, because a right-aligned count between two
/// left-aligned strings is exactly where hand-counted spaces come apart.
const TABLE: &[Col] = &[Col::l(22), Col::l(22), Col::r(7), Col::l(18)];

/// The drawn mermaid flow: wide node columns, narrow edge columns, one shared spec so the branch
/// row's arrow lands under the arrow above it.
const NODE: Col = Col::l(14);
const EDGE: Col = Col::l(5);
const FLOW: &[Col] = &[Col::l(2), NODE, EDGE, NODE, EDGE, NODE, EDGE, Col::l(16)];

pub(crate) fn table_and_diagram(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    let mut lines = vec![
        band(palette, "answer", "grounded", 84, palette.cite),
        blank(),
        Line::from(vec![
            Span::styled("⏺ ".to_string(), Style::default().fg(palette.green)),
            Span::styled(
                "Five call sites touch a charge. Only one writes the receipt before it settles."
                    .to_string(),
                Style::default().fg(palette.bright),
            ),
        ]),
        said(
            palette.mid,
            "It is reached from the retry branch as well as the settled branch, so a charge that",
        ),
        said(palette.mid, "gave up still gets a receipt."),
        blank(),
        note(palette, "markdown · table"),
        blank(),
        head(
            TABLE,
            &["call site", "file:line", "retries", "writes receipt"],
            palette.mid,
            2,
        ),
    ];

    // The markdown separator, built through `cols` from the rule texture's single owner so it is
    // exactly as wide as the head it sits under — and it is a RULE, never a ruled grid.
    let dashes: Vec<String> = TABLE.iter().map(|col| RULE.repeat(col.w)).collect();
    let cells: Vec<Cell<'_>> = dashes.iter().map(|d| Cell(d, palette.dim)).collect();
    lines.push(owned(row(TABLE, &cells, 2)));

    // 🔴 A CITATION STAYS CLICKABLE INSIDE THE TABLE. A `file:line` that stops being a citation the
    // moment it enters a cell costs the reader the one thing the answer was for.
    #[rustfmt::skip]
    let rows: &[(&str, &str, &str, &str)] = &[
        ("charge_card",    "billing/charge.rs:82",  "3", "after settle"),
        ("retry_gate",     "billing/retry.rs:41",   "3", "never"),
        ("settle_charge",  "billing/settle.rs:113", "0", "never"),
        ("receipt_writer", "billing/receipt.rs:17", "0", "before settle"),
        ("refund_card",    "billing/refund.rs:64",  "2", "after settle"),
    ];
    for (symbol, cite, retries, receipt) in rows {
        let receipt_ink = if *receipt == "before settle" {
            palette.red
        } else {
            palette.dim
        };
        lines.push(row(
            TABLE,
            &[
                Cell(symbol, palette.mid),
                Cell(cite, palette.cite),
                Cell(retries, palette.mid),
                Cell(receipt, receipt_ink),
            ],
            2,
        ));
    }

    lines.push(blank());
    lines.push(note(palette, "mermaid · flowchart LR"));
    lines.push(blank());
    let ink = |cell: &str| {
        if cell.contains("receipt_writer") {
            palette.red
        } else if cell == "●" {
            palette.green
        } else if cell.starts_with('─') || cell.starts_with('│') || cell.starts_with('╰') {
            palette.dim
        } else {
            palette.mid
        }
    };
    #[rustfmt::skip]
    let flow: [[&str; 8]; 3] = [
        ["●", "charge_card", "──▸", "retry_gate",   "──▸", "settled",      "──▸", "receipt_writer"],
        ["",  "",            "",    "│",            "",    "",             "",    ""],
        ["",  "",            "",    "╰──▸ gave_up", "──▸", "alert_oncall", "──▸", "receipt_writer"],
    ];
    for cells in flow {
        lines.push(row(FLOW, &cells.map(|cell| Cell(cell, ink(cell))), 2));
    }

    lines.push(blank());
    lines.push(said(
        palette.red,
        "✗ the second edge is the bug: a receipt for a charge that never settled.",
    ));
    lines.push(blank());
    // Every footnote here is a LIMIT stated out loud, which is the half a reader has to be told.
    let mut foot = |text: &str| lines.push(note(palette, text));
    foot("mermaid · flowchart, sequenceDiagram and stateDiagram are DRAWN here.");
    foot("classDiagram, gantt, erDiagram and pie print their fenced source unchanged —");
    foot("a terminal cannot lay those out honestly, and a wrong picture beats no source.");
    foot("the table is markdown, not mermaid. every file:line in it clicks like one in prose.");
    lines
}

// ── 39 · tool calls, collapsed and expanded ──────────────────────────────────────────────────

/// One collapsed tool call: mark · tool · argument · elapsed · result.
///
/// ⚠️ `⏺` is three bytes and one terminal column, which is the exact bug
/// `cols::multibyte_glyphs_count_as_one_column` exists for. It goes through a `Col`, never a
/// byte-counted pad.
const CALLS: &[Col] = &[Col::l(2), Col::l(6), Col::l(46), Col::r(7), Col::l(24)];

/// The expanded call's output: gutter · line · status.
const OUTPUT: &[Col] = &[Col::l(2), Col::l(56), Col::l(9)];

pub(crate) fn tool_calls(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let running = pulsed(palette.warn, tick, pulse);
    let mut lines = vec![
        band(palette, "tools", "this turn", 84, palette.cite),
        blank(),
        head(
            CALLS,
            &["", "tool", "argument", "elapsed", "result"],
            palette.mid,
            2,
        ),
    ];

    // The mark and the colour are DERIVED from the result, so a row cannot say one thing in its
    // glyph and another in its words.
    #[rustfmt::skip]
    let calls: &[(&str, &str, &str, &str)] = &[
        ("Bash", "cargo test -p estelle-tui",   "8.1s", "214 passed · 0 failed"),
        ("Read", "billing/receipt.rs",          "0.1s", "82 lines"),
        ("Grep", "settle( · billing/",          "0.3s", "6 matches in 4 files"),
        ("Edit", "billing/receipt.rs · 1 hunk", "2.4s", "running"),
        ("Gate", "the proposed diff",           "1.2s", "refused · 1 finding"),
    ];
    for (tool, argument, elapsed, result) in calls {
        let (mark, ink) = match *result {
            "running" => ("◐", running),
            done if done.starts_with("refused") => ("⏺", palette.red),
            _ => ("⏺", palette.green),
        };
        lines.push(row(
            CALLS,
            &[
                Cell(mark, ink),
                Cell(tool, palette.mid),
                Cell(argument, palette.dim),
                Cell(elapsed, palette.dim),
                Cell(result, ink),
            ],
            2,
        ));
    }

    lines.push(blank());
    lines.push(Line::from(vec![
        Span::styled("  ▾ ".to_string(), Style::default().fg(palette.cite)),
        Span::styled("Bash".to_string(), Style::default().fg(palette.mid)),
        Span::styled(
            " · cargo test -p estelle-tui   expanded, tail first".to_string(),
            Style::default().fg(palette.dim),
        ),
    ]));

    // 🔴 THE COUNT OF WHAT IS NOT SHOWN, IN THE FIRST OUTPUT LINE. A capped read means "cannot
    // answer", never "that's all there is" — so the hidden lines are counted before the tail that
    // survived, and the key that shows them sits on the same line.
    lines.push(row(
        OUTPUT,
        &[
            Cell("⎿", palette.dim),
            Cell("212 lines hidden · ctrl+r expands", palette.warn),
            Cell("", palette.dim),
        ],
        2,
    ));

    #[rustfmt::skip]
    let tail: &[(&str, &str)] = &[
        ("test billing::charge::retries_three_times ...",    "ok"),
        ("test billing::charge::gives_up_after_three ...",   "ok"),
        ("test billing::retry::backoff_is_bounded ...",      "ok"),
        ("test billing::retry::gate_refuses_unsettled ...",  "ok"),
        ("test billing::receipt::writes_after_settle ...",   "ok"),
        ("test billing::receipt::one_receipt_per_charge ...", "ok"),
        ("test billing::refund::refunds_are_idempotent ...", "ok"),
        ("test billing::settle::settles_once ...",           "ok"),
        ("test cols::multibyte_glyphs_count_as_one ...",     "ok"),
        ("test theme::pulse_never_uses_rapid_blink ...",     "ok"),
        ("test result: ok. 214 passed; 0 failed; 3 ignored", ""),
        ("Finished test profile in 8.14s",                   ""),
    ];
    for (text, status) in tail {
        let status_ink = if status.is_empty() {
            palette.dim
        } else {
            palette.green
        };
        lines.push(row(
            OUTPUT,
            &[
                Cell("", palette.dim),
                Cell(text, palette.dim),
                Cell(status, status_ink),
            ],
            2,
        ));
    }

    lines.push(blank());
    let mut foot = |text: &str| lines.push(note(palette, text));
    foot("ctrl+r expands the selected call · ctrl+o expands every call · c copies this output");
    foot("a collapsed call is one row. nothing is dropped silently: what is hidden is counted.");
    lines
}

// ── 40 · the code graph, walkable ────────────────────────────────────────────────────────────

/// mark · symbol · file:line · fan-in · fan-out · role. The role column is 44 wide because a
/// chokepoint's role is a SENTENCE with a file count in it, and a truncated blast radius would be
/// the same lie as no blast radius.
#[rustfmt::skip]
const NODES: &[Col] = &[Col::l(2), Col::l(22), Col::l(24), Col::r(4), Col::r(4), Col::l(44)];

pub(crate) fn code_graph(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let hot = pulsed(palette.warn, tick, pulse);
    let mut lines = vec![
        band(palette, "graph", "uqeu/estelle", 92, palette.cite),
        blank(),
        Line::from(vec![
            Span::styled("  / ".to_string(), Style::default().fg(palette.cite)),
            Span::styled("settle".to_string(), Style::default().fg(palette.bright)),
            Span::styled(
                "   6 of 5,608 symbols match · 4 files".to_string(),
                Style::default().fg(palette.dim),
            ),
        ]),
        blank(),
        head(
            NODES,
            &["", "symbol", "file:line", "in", "out", "role"],
            palette.mid,
            2,
        ),
    ];

    // 🔴 A CHOKEPOINT IS MARKED BY WHAT TOUCHING IT MOVES, NOT BY A BADGE. The file count is the
    // claim; `chokepoint` on its own is a label nobody can check. The marker glyph is DERIVED from
    // the role, so the glyph and the words cannot disagree.
    #[rustfmt::skip]
    let nodes: &[(&str, &str, &str, &str, &str)] = &[
        ("charge_card",    "billing/charge.rs:82",  "12", "4", "chokepoint · touching this moves 47 files"),
        ("settle_charge",  "billing/settle.rs:113",  "9", "6", "chokepoint · touching this moves 31 files"),
        ("retry_gate",     "billing/retry.rs:41",    "7", "3", "gate · every charge passes through it"),
        ("card_client",    "billing/client.rs:29",   "5", "9", "hub · 9 outbound edges, 1 provider"),
        ("receipt_writer", "billing/receipt.rs:17",  "2", "1", "leaf · nothing imports it"),
        ("refund_card",    "billing/refund.rs:64",   "3", "2", "leaf · nothing imports it"),
    ];
    for (index, (symbol, cite, fan_in, fan_out, role)) in nodes.iter().enumerate() {
        let choke = role.starts_with("chokepoint");
        let mark = if choke {
            "●"
        } else if role.starts_with("leaf") {
            "○"
        } else {
            "◆"
        };
        let mut line = row(
            NODES,
            &[
                Cell(mark, if choke { hot } else { palette.mid }),
                Cell(symbol, palette.mid),
                Cell(cite, palette.cite),
                Cell(fan_in, palette.mid),
                Cell(fan_out, palette.mid),
                Cell(role, if choke { hot } else { palette.dim }),
            ],
            2,
        );
        if index == 0 {
            line = line.style(Style::default().bg(palette.tint));
        }
        lines.push(line);
    }

    lines.push(blank());
    let mut foot = |text: &str| lines.push(note(palette, text));
    foot("enter opens the symbol · space filters · b shows the blast radius · d exports dot");
    foot("fan-in and fan-out come off the swept graph at 1f5cc7a4, not inferred from imports.");
    foot("a chokepoint is not a label: the file count is what the graph says touching it moves.");
    lines
}

// ── 41 · memory, and the half that corrects it ───────────────────────────────────────────────

/// mark · claim · kind · trust · added · cited by.
///
/// The trust column carries the same evidence vocabulary as the rest of the product — `measured`,
/// `observed`, `asserted` — because a memory whose provenance is invisible is one the reader has
/// to take on faith, which is the failure this product exists to prevent.
#[rustfmt::skip]
const HELD: &[Col] = &[Col::l(2), Col::l(42), Col::l(9), Col::l(9), Col::r(7), Col::l(14)];

/// who · the claim · where it came from. The two correction rows share it so the replacement and
/// the thing it replaced line up and read as a pair.
const EDIT: &[Col] = &[Col::l(4), Col::l(42), Col::l(24)];

/// The claim being corrected. Named once: the held row selects on it and the `was` line repeats
/// it — two places that must not drift apart.
const SUPERSEDED: &str = "cream is #F1EFE9";

pub(crate) fn memory_correct(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    let mut lines = vec![
        band(palette, "memory", "uqeu/estelle", 92, palette.cite),
        blank(),
        head(
            HELD,
            &["", "", "kind", "trust", "added", "cited by"],
            palette.mid,
            2,
        ),
    ];

    #[rustfmt::skip]
    let held: &[(&str, &str, &str, &str, &str)] = &[
        ("the gate is never bypassed by auto-mode", "decision", "measured", "14 Aug", "3 citations"),
        ("we ship propose-only by default",         "decision", "observed", "12 Aug", "1 citation"),
        ("Rows exists so an adapter cannot tell",   "lesson",   "measured", "11 Aug", "2 citations"),
        (SUPERSEDED,                                "fact",     "asserted", "09 Aug", "1 citation"),
        ("opus-4-8 is pinned, do not upgrade",      "decision", "asserted", "25 Aug", "you, just now"),
    ];
    for (claim, kind, trust, added, cited) in held {
        let selected = *claim == SUPERSEDED;
        let kind_ink = match *kind {
            "decision" => palette.cite,
            "lesson" => palette.skill,
            _ => palette.green,
        };
        let trust_ink = match *trust {
            "measured" => palette.green,
            "observed" => palette.cite,
            _ => palette.warn,
        };
        let mut line = row(
            HELD,
            &[
                Cell(if selected { "›" } else { "" }, palette.cite),
                Cell(claim, palette.mid),
                Cell(kind, kind_ink),
                Cell(trust, trust_ink),
                Cell(added, palette.dim),
                Cell(cited, palette.dim),
            ],
            2,
        );
        if selected {
            line = line.style(Style::default().bg(palette.tint));
        }
        lines.push(line);
    }

    lines.push(blank());
    let correcting = band(palette, "correcting", "that is wrong", 92, palette.warn);
    lines.push(correcting);
    lines.push(blank());
    lines.push(row(
        EDIT,
        &[
            Cell("you", palette.warn),
            Cell("cream is #E9E6DC", palette.bright),
            Cell("globals.css:18-32", palette.cite),
        ],
        2,
    ));
    // 🔴 THE SUPERSEDED ORIGINAL STAYS ON THE SCREEN. A correction that erases what it corrected
    // leaves no audit trail, and no way to tell a fix from a rewrite of history.
    lines.push(row(
        EDIT,
        &[
            Cell("was", palette.dim),
            Cell(SUPERSEDED, palette.dim),
            Cell("asserted · 09 Aug", palette.dim),
        ],
        2,
    ));
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::styled("  ⏺ ".to_string(), Style::default().fg(palette.green)),
        Span::styled(
            "the new claim supersedes the old one, and stops being served.".to_string(),
            Style::default().fg(palette.bright),
        ),
    ]));
    let mut foot = |t: &str| {
        lines.push(if t.is_empty() {
            blank()
        } else {
            note(palette, t)
        })
    };
    foot("an edit SUPERSEDES, it does not overwrite. retracted is not deleted.");
    foot(
        "the superseded claim stays readable, dated and cited — what was believed when, on the record.",
    );
    foot("");
    foot("enter reads · e edits · d retracts · c shows citations");
    foot("measured came off an instrument · observed was seen once · asserted is somebody's word.");
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    fn palette() -> Palette {
        ScreenTheme::Dark.palette()
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn width(line: &Line<'_>) -> usize {
        line.spans.iter().map(|s| s.content.chars().count()).sum()
    }

    fn joined(lines: &[Line<'_>]) -> String {
        lines.iter().map(text).collect::<Vec<_>>().join("\n")
    }

    /// The table head and the row directly under it, as rendered text.
    fn head_and_rule(lines: &[Line<'static>]) -> (String, String) {
        let at = lines
            .iter()
            .position(|line| text(line).contains("file:line"))
            .expect("the table head is missing");
        (text(&lines[at]), text(&lines[at + 1]))
    }

    /// A row made of the rule texture and nothing else. Owned here so the two tests that need it
    /// cannot drift apart on what counts as a rule.
    fn is_rule(row: &str) -> bool {
        let dash = RULE.chars().next().expect("the rule texture is empty");
        row.contains(dash) && row.chars().all(|glyph| glyph == ' ' || glyph == dash)
    }

    /// 🔴 THE PROPERTY `cols` EXISTS FOR. Two data rows whose cells differ in every character still
    /// end on the same column, and the markdown separator is no narrower than the head above it. A
    /// row padded by hand fails this the first time a cell changes length.
    #[test]
    fn the_rendered_table_columns_line_up() {
        let lines = table_and_diagram(&palette(), 0, true);
        let (header, separator) = head_and_rule(&lines);
        assert!(is_rule(&separator), "not a rule: {separator:?}");
        assert!(
            separator.chars().count() >= header.chars().count(),
            "the rule is narrower than the head above it: {separator:?}"
        );

        let find = |needle: &str| {
            lines
                .iter()
                .find(|line| text(line).contains(needle))
                .unwrap_or_else(|| panic!("no row for {needle}"))
        };
        let first = find("billing/charge.rs:82");
        let second = find("billing/receipt.rs:17");
        assert_ne!(text(first), text(second), "the two rows are the same row");
        assert_eq!(
            width(first),
            width(second),
            "two data rows end on different columns:\n{:?}\n{:?}",
            text(first),
            text(second)
        );
        assert!(
            first.spans.iter().any(|span| {
                span.content.contains("billing/charge.rs:82")
                    && span.style.fg == Some(palette().cite)
            }),
            "the file:line in the table stopped being a citation"
        );
    }

    /// 🔴 A CAPPED READ MEANS "CANNOT ANSWER", NEVER "THAT'S ALL THERE IS". The hidden count is a
    /// NUMBER attached to the words, and no line trails off into an ellipsis with no count on it.
    #[test]
    fn a_truncated_tool_output_says_how_much_it_hid() {
        let texts: Vec<String> = tool_calls(&palette(), 0, true).iter().map(text).collect();
        let hidden = texts
            .iter()
            .find(|line| line.contains("lines hidden"))
            .expect("the expanded call never says what it hid");
        let at = hidden.find(" lines hidden").expect("checked by the find");
        assert!(
            hidden[..at]
                .trim_end()
                .chars()
                .last()
                .is_some_and(|glyph| glyph.is_ascii_digit()),
            "the hidden lines are not counted: {hidden:?}"
        );

        for line in &texts {
            let trimmed = line.trim_end();
            if trimmed.ends_with('…') {
                assert!(
                    trimmed.chars().any(|glyph| glyph.is_ascii_digit()),
                    "a line trails off with no count of what it dropped: {line:?}"
                );
            }
        }
    }

    /// 🔴 NO SCREEN DRAWS A GRID — AND THE TABLE'S OWN RULE PROVES THE CHECK IS NOT VACUOUS.
    ///
    /// ⚠️ The corners are spelled as escapes for the reason `box_glyphs` spells them that way: the
    /// source-level guard searches for the raw bytes, and a fixture written the readable way would
    /// need an exemption — which is where the next box hides.
    #[test]
    fn no_answer_screen_draws_a_grid() {
        const CORNERS: [char; 9] = [
            '\u{250C}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{251C}', '\u{2524}', '\u{252C}',
            '\u{2534}', '\u{253C}',
        ];
        let palette = palette();
        let screens: [(&str, Vec<Line<'static>>); 4] = [
            ("34", table_and_diagram(&palette, 0, true)),
            ("39", tool_calls(&palette, 0, true)),
            ("40", code_graph(&palette, 0, true)),
            ("41", memory_correct(&palette, 0, true)),
        ];
        for (name, lines) in &screens {
            let all = joined(lines);
            for corner in CORNERS {
                assert!(!all.contains(corner), "{name} drew {corner:?}");
            }
        }

        // 🔴 THE CONTROL. A screen that drew no table would pass the loop above for the wrong
        // reason, and "a `─` appears somewhere" would not catch that either — the header rule
        // satisfies it with the table deleted. So: the row DIRECTLY UNDER the table head is a rule
        // and nothing else. That is the glyph the no-box rule put in a grid's place.
        let (_, separator) = head_and_rule(&screens[0].1);
        assert!(is_rule(&separator), "the rule is gone: {separator:?}");
    }

    /// 🔴 A CORRECTION SUPERSEDES; IT DOES NOT OVERWRITE. Both the replacement and the thing it
    /// replaced are on the screen, the superseded one dim. A screen that overwrote the original
    /// fails on the second assertion.
    #[test]
    fn a_correction_keeps_the_thing_it_corrected() {
        let palette = palette();
        let lines = memory_correct(&palette, 0, true);
        let all = joined(&lines);
        assert!(all.contains("cream is #E9E6DC"), "no replacement");
        assert!(
            all.contains(SUPERSEDED),
            "the superseded claim was overwritten instead of superseded"
        );
        assert!(all.contains("supersedes"), "the rule is not on the screen");

        let superseded = lines
            .iter()
            .find(|line| text(line).trim_start().starts_with("was "))
            .expect("the superseded row is missing");
        assert!(
            text(superseded).contains(SUPERSEDED),
            "the was-row is empty"
        );
        assert!(
            superseded
                .spans
                .iter()
                .filter(|span| !span.content.trim().is_empty())
                .all(|span| span.style.fg == Some(palette.dim)),
            "the superseded claim is not dimmed: {:?}",
            text(superseded)
        );
    }

    /// The vacuity guard, local to this file: `mod.rs` never asserts the needles — only the gallery
    /// does — so a screen that stopped saying the thing it exists to say would go red nowhere until
    /// someone regenerated the book. The chokepoint clause is stronger than the gallery's, because
    /// the word on its own is a badge and the row has to carry the file count.
    #[test]
    fn every_screen_still_says_the_thing_its_frame_asserts() {
        let palette = palette();
        for (needle, lines) in [
            ("mermaid", table_and_diagram(&palette, 0, true)),
            ("lines hidden", tool_calls(&palette, 0, true)),
            ("chokepoint", code_graph(&palette, 0, true)),
            ("supersedes", memory_correct(&palette, 0, true)),
        ] {
            assert!(joined(&lines).contains(needle), "{needle:?} lost");
        }

        let choke = code_graph(&palette, 0, true)
            .iter()
            .map(text)
            .find(|line| line.contains("chokepoint"))
            .expect("no chokepoint row");
        assert!(
            choke.contains("touching this moves")
                && choke.chars().any(|glyph| glyph.is_ascii_digit()),
            "the chokepoint is a bare label with no blast radius: {choke:?}"
        );
    }
}
