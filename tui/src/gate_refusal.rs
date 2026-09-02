//! The `Gate refused` block, in the catalog's design language, for BOTH callers.
//!
//! 🔴 **THE REFUSAL IS THE PRODUCT'S LOUDEST MOMENT AND IT WAS DRAWN TWICE.** Screen 10 of the
//! catalog (`screens.rs::broken`) drew the design — a `── gate · refused ──` rule, a pulsing
//! `⏹ Gate refused`, and one row per blocker naming the CLAIM and the FINDING against it. The live
//! session drew a bordered `┌ gate · deterministic · no model ┐` modal with a scatter chart and
//! `{:>6}  {path}` hand-positioned text. Only the second one ever reached a customer. This module
//! is the single renderer; `screens.rs` and `live_renderer::render_gate_modal` both call it.
//!
//! ⚠️ **A REFUSAL MAY NOT BE TRUNCATED.** `cols` truncates an overlong cell with `…`, which is
//! correct for a model name and catastrophic for the sentence explaining why an edit was refused.
//! So this module wraps blocker text across continuation rows instead, and
//! `a_long_blocker_survives_a_narrow_modal_intact` reassembles the rendered rows and asserts the
//! original sentence is still there, character for character.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, row, rule};
use crate::theme::Palette;

/// One reason the gate refused: the CLAIM the change made, and the FINDING against it.
///
/// `finding` is `None` when the server sent one undivided sentence — the live `/gate` reply's
/// `blockers` are flat strings. An absent finding widens the claim across both columns rather
/// than printing an empty second column, because a blank cell reads as "checked, nothing found".
pub(crate) struct Blocker<'a> {
    pub claim: &'a str,
    pub finding: Option<&'a str>,
}

/// Everything the block draws. A struct rather than seven positional arguments, because at that
/// width a reader cannot tell `detail` from `note` at the call site, and neither can a reviewer.
pub(crate) struct Refusal<'a> {
    /// The caller's context for the refusal, on the headline: the catalog's
    /// `repairing · round 1 of 3`, the live modal's verdict.
    pub detail: &'a str,
    /// One sentence under the headline, for a caller that must say what happened to the user's
    /// action — the live modal says nothing was written. `None` on a screen that has no action.
    pub note: Option<&'a str>,
    pub blockers: &'a [Blocker<'a>],
    /// The blast radius as `(path, changed_lines)`. Empty means the section is not drawn at all,
    /// rather than a `0 files` line that reads as a measurement.
    pub files: &'a [(String, u64)],
}

/// The design's blocker table: marker, claim, finding — `[Col::l(3), Col::l(30), Col::l(34)]` on
/// the catalog's 66-column page. The ratio is what survives a narrower surface.
const MARKER: usize = 3;
const CLAIM: usize = 30;
const FINDING: usize = 34;
const GAP: usize = 2;
/// The narrowest text cell worth wrapping into; below it the block stops splitting columns.
const MIN_TEXT: usize = 12;

fn columns(width: usize) -> [Col; 3] {
    let text = width.saturating_sub(MARKER + GAP + GAP).max(MIN_TEXT * 2);
    let claim = (text * CLAIM / (CLAIM + FINDING)).max(MIN_TEXT);
    let finding = text.saturating_sub(claim).max(MIN_TEXT);
    [Col::l(MARKER), Col::l(claim), Col::l(finding)]
}

/// The claim column widened over the finding column, for a blocker the server did not split.
fn wide_columns(width: usize) -> [Col; 2] {
    let split = columns(width);
    [Col::l(MARKER), Col::l(split[1].w + GAP + split[2].w)]
}

fn owned(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
}

/// `text` broken into chunks that fit `width` columns, on word boundaries where one exists.
///
/// Bounded by construction: every iteration consumes at least one character, so the loop cannot
/// run longer than the input, and an empty input yields exactly one empty chunk.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut chunks = Vec::new();
    let mut rest = text.trim();
    if rest.is_empty() {
        return vec![String::new()];
    }
    while !rest.is_empty() {
        if rest.chars().count() <= width {
            chunks.push(rest.to_string());
            break;
        }
        let hard = rest
            .char_indices()
            .nth(width)
            .map(|(index, _)| index)
            .unwrap_or(rest.len());
        let cut = rest[..hard].rfind(' ').filter(|at| *at > 0).unwrap_or(hard);
        chunks.push(rest[..cut].trim_end().to_string());
        rest = rest[cut..].trim_start();
    }
    chunks
}

/// The refusal block: rule, headline, one or more rows per blocker, then the blast radius.
pub(crate) fn lines(
    refusal: &Refusal<'_>,
    palette: &Palette,
    width: usize,
    tick: u64,
    pulse_enabled: bool,
) -> Vec<Line<'static>> {
    let split = columns(width);
    let wide = wide_columns(width);
    let mut output = vec![
        owned(rule(
            "gate",
            "refused",
            width,
            palette.dim,
            palette.mid,
            palette.red,
        )),
        Line::from(""),
        // 🔴 `⏹` became `■` (the founder's set 2A) and the WORDS STOPPED PULSING. This row used
        // to apply `pulse(...)` to both spans, so the whole headline dimmed and brightened on a
        // 1.4s cycle — the exact thing the spec forbids. `marks::headline` can only pulse the
        // mark, which is why the fix is a call and not a correction.
        crate::marks::headline(
            crate::marks::Mark::Refused,
            "Gate refused",
            refusal.detail,
            palette,
            tick,
            pulse_enabled,
        ),
    ];
    if let Some(note) = refusal.note.map(str::trim).filter(|note| !note.is_empty()) {
        output.push(Line::styled(
            note.to_string(),
            Style::default().fg(palette.mid),
        ));
    }
    output.push(Line::from(""));

    for blocker in refusal.blockers {
        match blocker.finding {
            Some(finding) => {
                let claims = wrapped(blocker.claim, split[1].w);
                let findings = wrapped(finding, split[2].w);
                for index in 0..claims.len().max(findings.len()) {
                    output.push(owned(row(
                        &split,
                        &[
                            Cell(if index == 0 { "│" } else { "" }, palette.red),
                            Cell(claims.get(index).map_or("", String::as_str), palette.mid),
                            Cell(findings.get(index).map_or("", String::as_str), palette.dim),
                        ],
                        0,
                    )));
                }
            }
            None => {
                for (index, chunk) in wrapped(blocker.claim, wide[1].w).iter().enumerate() {
                    output.push(owned(row(
                        &wide,
                        &[
                            Cell(if index == 0 { "│" } else { "" }, palette.red),
                            Cell(chunk, palette.mid),
                        ],
                        0,
                    )));
                }
            }
        }
    }
    if refusal.blockers.is_empty() {
        output.push(Line::styled(
            "The server returned no blocker detail.",
            Style::default().fg(palette.dim),
        ));
    }

    if refusal.files.is_empty() {
        return output;
    }
    output.push(Line::from(""));
    let changed = refusal.files.iter().map(|(_, lines)| *lines).sum::<u64>();
    output.push(Line::styled(
        format!(
            "blast radius · {} files · {changed} changed lines",
            refusal.files.len()
        ),
        Style::default().fg(palette.warn),
    ));
    for (path, changed_lines) in refusal.files {
        output.push(owned(row(
            &split,
            &[
                Cell("", palette.dim),
                Cell(path, palette.mid),
                Cell(&format!("{changed_lines} changed lines"), palette.dim),
            ],
            0,
        )));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

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

    #[test]
    fn the_block_opens_on_the_designs_rule_and_headline_not_a_box() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&lines(
            &Refusal {
                detail: "repairing · round 1 of 3",
                note: None,
                blockers: &[Blocker {
                    claim: "reqwest::Client::retry()",
                    finding: Some("does not exist"),
                }],
                files: &[],
            },
            &palette,
            66,
            0,
            true,
        ));

        assert!(rendered.starts_with("── gate · refused ─"), "{rendered}");
        assert!(
            rendered.contains("■ Gate refused  ·  repairing · round 1 of 3"),
            "{rendered}"
        );
        assert!(rendered.contains("reqwest::Client::retry()"), "{rendered}");
        assert!(rendered.contains("does not exist"), "{rendered}");
        // No BOXED panel: the corners and the solid rule are the old language. `│` stays — it is
        // the design's own blocker marker, not a border.
        // ⚠️ `─` is NOT in this list any more: since the founder picked the solid rule it IS
        // the rule texture. Corners and tees are what make a box, and they stay forbidden.
        for boxed in ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'] {
            assert!(
                !rendered.contains(boxed),
                "a box glyph {boxed:?} survived\n{rendered}"
            );
        }
    }

    /// 🔴 THE ASSERTION THIS MODULE EXISTS FOR. `cols` truncates; a refusal reason may not be
    /// truncated. Rebuild the sentence out of the rendered rows and compare it to the input.
    #[test]
    fn a_long_blocker_survives_a_narrow_modal_intact() {
        let palette = ScreenTheme::Dark.palette();
        let claim = "reqwest::Client::retry() is called at src/client.rs:88 and the repo graph \
                     holds zero definition sites for it in any dependency version this lockfile \
                     resolves";
        for width in [40usize, 52, 66, 86] {
            let rendered = text(&lines(
                &Refusal {
                    detail: "verdict blocked",
                    note: None,
                    blockers: &[Blocker {
                        claim,
                        finding: None,
                    }],
                    files: &[],
                },
                &palette,
                width,
                0,
                false,
            ));
            assert!(
                !rendered.contains('…'),
                "width {width} truncated the refusal\n{rendered}"
            );
            let rebuilt = rendered
                .lines()
                .skip(4)
                .take_while(|line| !line.trim().is_empty())
                .map(|line| line.trim_start_matches(['│', ' ']).trim_end())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(
                rebuilt.split_whitespace().collect::<Vec<_>>(),
                claim.split_whitespace().collect::<Vec<_>>(),
                "width {width} lost part of the refusal\n{rendered}"
            );
        }
    }

    #[test]
    fn the_blast_radius_totals_the_measured_changed_lines() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&lines(
            &Refusal {
                detail: "verdict blocked",
                note: Some("Gate protected this repository. Nothing was written."),
                blockers: &[],
                files: &[("a.rs".to_string(), 6), ("b.rs".to_string(), 0)],
            },
            &palette,
            80,
            0,
            false,
        ));
        assert!(
            rendered.contains("blast radius · 2 files · 6 changed lines"),
            "{rendered}"
        );
        assert!(
            rendered.contains("The server returned no blocker detail."),
            "{rendered}"
        );

        // No inspected files means NO blast-radius section — not a `0 files` line that reads as
        // a measurement of an empty change.
        let none = text(&lines(
            &Refusal {
                detail: "verdict blocked",
                note: None,
                blockers: &[],
                files: &[],
            },
            &palette,
            80,
            0,
            false,
        ));
        assert!(!none.contains("blast radius"), "{none}");
    }

    #[test]
    fn every_blocker_row_tiles_the_frame_exactly() {
        for width in 30usize..140 {
            let split = columns(width);
            let tiled = split[0].w + GAP + split[1].w + GAP + split[2].w;
            let wide = wide_columns(width);
            assert_eq!(
                wide[0].w + GAP + wide[1].w,
                tiled,
                "width {width}: the wide row and the split row disagree"
            );
        }
    }

    /// ⚠️ The guarantee is on the CHARACTERS, not on the words: below the longest word's width
    /// there is nowhere to break except mid-word, and the loop must still terminate and still
    /// carry every character. Asserting on words would have quietly excused a dropped one.
    #[test]
    fn wrapping_is_bounded_and_never_drops_a_character() {
        let sentence = "alpha beta gamma delta epsilon";
        let dense = sentence.replace(' ', "");
        for width in 1usize..40 {
            let chunks = wrapped(sentence, width);
            assert!(
                chunks.len() <= sentence.chars().count(),
                "width {width} produced {} chunks for {} characters",
                chunks.len(),
                sentence.chars().count()
            );
            assert_eq!(
                chunks.concat().replace(' ', ""),
                dense,
                "width {width} dropped or reordered characters"
            );
        }
        assert_eq!(wrapped("   ", 10), vec![String::new()]);
    }
}
