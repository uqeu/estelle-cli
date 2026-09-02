//! Screen 42 — where the run's TIME and TOKENS actually went.
//!
//! 🔴 **THIS IS THE COSTING PANEL'S MISSING HALF.** Screens 32, 33 and 33b answer *what did it
//! cost*. The founder saw pi's stats view and asked for the other question: *what did the time and
//! the tokens go TO*. He has asked for a costing panel three times and been disappointed twice, and
//! both times the thing he was missing was a breakdown, not a total.
//!
//! 🔴 **THE ONE NUMBER ON HERE THAT CHANGES A DECISION IS THE CACHE SPLIT.** 24.7M prompt tokens
//! read from cache against 768k computed is a **32×** difference in what a BYOK user pays for the
//! same conversation, and nothing this CLI renders today shows it — even though
//! [`crate::token_usage::TokenUsage`] has carried `cached_input_tokens` all along and
//! `non_cached_input` is the field the blended total is already built from. The number was on the
//! wire and no surface spent it.
//!
//! ⚠️ **WHAT IS MEASURED AND WHAT IS NOT, PER ROW, ON THE FRAME.** The cache split is a real field.
//! The per-phase clock (prefill · first token · generation · reasoning · tools · compaction) and
//! the per-tool activity counts are **not instrumented in this client**, and the fleet's per-worker
//! numbers are not on the wire at all — [`crate::orchestra_view::MISSING_PER_WORKER_SPEND`] is
//! quoted verbatim rather than paraphrased so this screen cannot drift from that disclosure.
//! A stats panel that cannot say which of its own numbers were counted is worse than no panel.
//!
//! ⚠️ **NO BOXES AND NO CHART LIBRARY.** The timeline is a density strip of block glyphs on a
//! [`crate::cols`] row; the lap ruler is a `│` column, which is a divider and not a corner. The
//! duration column is right-aligned through `Col::r`, which is what those four `cols` tests exist
//! for — a hand-padded `3h02m` beside a `4.1s` is exactly the misalignment this module closes.

use ratatui::text::Line;

use crate::cols::{Cell, Col, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::marks::{Mark, headline};
use crate::orchestra_view::MISSING_PER_WORKER_SPEND;
use crate::theme::Palette;

/// The density ramp, faintest to fullest. Nine steps so an empty bucket is a SPACE rather than a
/// `▁` — a bucket where nothing happened must look like nothing, not like a little of something.
const RAMP: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
/// How many buckets one timeline row is drawn in. Named because it is also the lap ruler's width,
/// and two hand-typed copies of one width is how a ruler stops lining up with the thing it rules.
const BUCKETS: usize = 60;
/// Six laps across [`BUCKETS`]. One owner, so the ruler and the `avg/lap` figure cannot disagree.
const LAPS: usize = 6;

fn bar(palette: &Palette, label: &str, mode: &str, wide: usize) -> Line<'static> {
    owned(rule(
        label,
        mode,
        wide,
        palette.dim,
        palette.mid,
        palette.cite,
    ))
}

/// A run of block glyphs, one per bucket, from a per-bucket intensity in `0..=8`.
///
/// ⚠️ Built from an `&[u8]` rather than typed as a string literal per row, because a hand-typed
/// strip is a hand-counted layout: the row could be 58 glyphs long against a 60-column ruler and
/// nothing would say so. The length is asserted.
fn strip(levels: &[u8]) -> String {
    assert_eq!(
        levels.len(),
        BUCKETS,
        "a timeline row must be exactly one bucket wide per ruler column"
    );
    levels
        .iter()
        .map(|level| RAMP[usize::from(*level).min(RAMP.len() - 1)])
        .collect()
}

// ── rates ────────────────────────────────────────────────────────────────────────────────────

/// `figure | what it counts`. Spelled out, because `tg/s` and `pp/s` are pi's abbreviations and
/// the founder's standing note on the fleet table was that a column he cannot read is a column
/// that must be labelled in words.
const RATES: [(&str, &str, &str); 4] = [
    (
        "32.1",
        "tg/s",
        "tokens generated per second, over the whole run",
    ),
    ("1,204", "pp/s", "prompt tokens processed per second"),
    ("6", "laps", "turns that reached a tool call and came back"),
    (
        "30m20s",
        "avg/lap",
        "wall clock divided by laps, not a per-lap median",
    ),
];

// ── time ─────────────────────────────────────────────────────────────────────────────────────

/// `phase | duration | share of wall clock | bucket levels`.
///
/// ⚠️ **THE SHARES DO NOT SUM TO 100% AND THE FRAME SAYS SO.** Reasoning happens INSIDE generation
/// and is counted in both; prefill overlaps neither. A stacked bar here would be a lie with a
/// picture on it, so each row is drawn against wall clock independently and the overlap is stated
/// in a sentence under the table.
#[rustfmt::skip]
const PHASES: [(&str, &str, &str, u8); 8] = [
    ("wall clock",  "3h02m",  "100%",   8),
    ("generation",  "1h57m",  "64.3%",  6),
    ("reasoning",   "1h00m",  "33.0%",  4),
    ("prefill",     "21m02s", "11.5%",  2),
    ("tools",       "1m18s",  "0.7%",   1),
    ("first token", "4.1s",   "0.0%",   0),
    ("startup",     "0.9s",   "0.0%",   0),
    ("compaction",  "0.0s",   "not run",0),
];

// ── tokens ───────────────────────────────────────────────────────────────────────────────────

/// `kind | tokens | share | what it is priced at`.
///
/// 🔴 The two rows that matter sit next to each other on purpose: `prompt cached` at `0.10×` and
/// `prompt computed` at `1.00×`. That ratio is the whole reason this table exists.
#[rustfmt::skip]
const TOKENS: [(&str, &str, &str, &str); 5] = [
    ("prompt cached",   "24.7M", "96.1%", "0.10x input · read back, not recomputed"),
    ("prompt computed", "768k",  "3.0%",  "1.00x input"),
    ("completion",      "202k",  "0.8%",  "output"),
    ("reasoning",       "67k",   "0.3%",  "output · billed as completion"),
    ("tool results",    "64k",   "0.2%",  "1.00x input · fed back next lap"),
];

// ── activity ─────────────────────────────────────────────────────────────────────────────────

/// `row | count | per-bucket intensity`. Three assistant rows and six tool rows, the shape of the
/// panel the founder pointed at.
#[rustfmt::skip]
const ACTIVITY: [(&str, &str, [u8; 12]); 9] = [
    ("assistant · prefill",    "6",   [8,2,1,1,2,1,1,1,2,1,1,0]),
    ("assistant · reasoning",  "6",   [3,6,7,5,6,7,6,5,4,6,3,1]),
    ("assistant · generation", "6",   [2,5,8,6,7,8,7,6,5,7,4,2]),
    ("bash",                   "143", [1,4,6,3,5,7,4,6,3,5,2,1]),
    ("jina_search_web",        "34",  [0,2,3,1,0,4,2,1,3,2,0,0]),
    ("edit",                   "25",  [0,0,2,1,3,1,2,3,1,2,4,1]),
    ("read",                   "12",  [2,1,0,1,0,1,0,2,1,0,1,0]),
    ("write",                  "1",   [0,0,0,0,0,0,0,0,0,0,1,0]),
    ("compaction",             "0",   [0,0,0,0,0,0,0,0,0,0,0,0]),
];

/// Which numbers on this screen were counted, and which were staged. One row per claim.
const SOURCES: [&str; 4] = [
    "measured   the cache split — token_usage::TokenUsage::cached_input_tokens, on every reply",
    "NOT measured   the per-phase clock: no timer wraps prefill, generation, reasoning or tools",
    "NOT measured   the per-tool counts: tool calls are rendered, never tallied for a run",
    "NOT on the wire   per-worker model and cost — see the line above, quoted from orchestra_view",
];

/// Expand a 12-point shape to [`BUCKETS`] by repeating each point. Written out rather than typed as
/// 60 digits per row: sixty hand-typed digits is sixty chances to be one off, and the repeat factor
/// is asserted against the ruler.
fn widen(shape: &[u8; 12]) -> Vec<u8> {
    const REPEAT: usize = BUCKETS / 12;
    assert_eq!(
        REPEAT * 12,
        BUCKETS,
        "the shape must tile the ruler exactly"
    );
    shape
        .iter()
        .flat_map(|level| std::iter::repeat_n(*level, REPEAT))
        .collect()
}

/// The lap ruler: a `│` every `BUCKETS / LAPS` columns, so a reader can see which lap a burst is in.
///
/// ⚠️ `│` is a DIVIDER, not a corner. The no-box guard counts `┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`; a vertical rule
/// that never meets a horizontal one cannot close into a panel, which is the property the rule is
/// actually about.
fn lap_ruler() -> String {
    let stride = BUCKETS / LAPS;
    (0..BUCKETS)
        .map(|column| if column % stride == 0 { '│' } else { ' ' })
        .collect()
}

pub(crate) fn stats_activity(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 126;
    let mut out = vec![bar(palette, "run", "3h02m · 6 laps · $4.71", W), blank()];
    out.extend(rates(palette));
    out.push(blank());
    out.extend(time(palette));
    out.push(blank());
    out.extend(tokens(palette));
    out.push(blank());
    out.extend(activity(palette));
    out.push(blank());
    out.push(headline(
        Mark::Landed,
        "24.7M of 25.5M prompt tokens were read from cache",
        "computing them again would have cost 32x",
        palette,
        tick,
        pulse,
    ));
    out.push(blank());
    out.push(note(
        palette,
        &format!("orchestra · {MISSING_PER_WORKER_SPEND}"),
    ));
    for line in SOURCES {
        out.push(note(palette, line));
    }
    out
}

fn rates(palette: &Palette) -> Vec<Line<'static>> {
    const C: &[Col] = &[Col::r(8), Col::l(9), Col::l(60)];
    let mut out = vec![owned(head(
        C,
        &["", "rate", "what it counts"],
        palette.dim,
        2,
    ))];
    for (figure, name, meaning) in RATES {
        out.push(owned(row(
            C,
            &[
                Cell(figure, palette.bright),
                Cell(name, palette.mid),
                Cell(meaning, palette.dim),
            ],
            2,
        )));
    }
    out
}

fn time(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(13), Col::r(8), Col::r(8), Col::l(64)];
    let mut out = vec![
        bar(palette, "time", "where the 3h02m went", W),
        owned(head(
            C,
            &["phase", "duration", "of wall", "share of the run"],
            palette.dim,
            2,
        )),
    ];
    for (phase, duration, share, level) in PHASES {
        // The share bar is the same ramp the timeline uses, at one intensity across its own width,
        // so two different pictures on one screen cannot mean two different things by `█`.
        let width = usize::from(level) * 5;
        let drawn = RAMP[usize::from(level).min(RAMP.len() - 1)].repeat(width);
        let ink = if phase == "compaction" {
            palette.dim
        } else {
            palette.cite
        };
        out.push(owned(row(
            C,
            &[
                Cell(phase, palette.mid),
                Cell(duration, palette.bright),
                Cell(share, palette.dim),
                Cell(&drawn, ink),
            ],
            2,
        )));
    }
    out.push(note(
        palette,
        "reasoning is INSIDE generation and is counted in both. wall clock is the only total; the \
         rows do not sum to it.",
    ));
    out
}

fn tokens(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(16), Col::r(8), Col::r(8), Col::l(48)];
    let mut out = vec![
        bar(palette, "tokens", "25.7M · what each kind costs", W),
        owned(head(
            C,
            &["kind", "tokens", "share", "priced at"],
            palette.dim,
            2,
        )),
    ];
    for (kind, count, share, priced) in TOKENS {
        let ink = if kind == "prompt cached" {
            palette.green
        } else {
            palette.mid
        };
        out.push(owned(row(
            C,
            &[
                Cell(kind, ink),
                Cell(count, palette.bright),
                Cell(share, palette.dim),
                Cell(priced, palette.dim),
            ],
            2,
        )));
    }
    out
}

fn activity(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(22), Col::r(5), Col::l(BUCKETS)];
    let mut out = vec![
        bar(palette, "activity", "one row per tool, across the run", W),
        owned(head(C, &["", "n", "wall clock"], palette.dim, 2)),
        owned(row(
            C,
            &[
                Cell("lap", palette.dim),
                Cell("6", palette.dim),
                Cell(&lap_ruler(), palette.dim),
            ],
            2,
        )),
    ];
    for (name, count, shape) in ACTIVITY {
        let drawn = strip(&widen(&shape));
        let ink = if name.starts_with("assistant") {
            palette.cite
        } else if count == "0" {
            palette.dim
        } else {
            palette.green
        };
        out.push(owned(row(
            C,
            &[
                Cell(name, palette.mid),
                Cell(count, palette.bright),
                Cell(&drawn, ink),
            ],
            2,
        )));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    fn text(palette: &Palette) -> Vec<String> {
        stats_activity(palette, 0, true)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    /// 🔴 **THE RULER AND EVERY ROW IT RULES ARE THE SAME WIDTH.**
    ///
    /// A density strip under a lap ruler is only readable if a column means the same instant on
    /// both, and "they look about right" is exactly the hand-counted claim this module exists to
    /// stop. `strip` asserts its own input length; this asserts the RENDERED result, because a
    /// `Col::l` narrower than its cell would truncate a correct strip into a wrong one.
    #[test]
    fn every_timeline_row_is_exactly_as_wide_as_the_lap_ruler() {
        let palette = ScreenTheme::Dark.palette();
        let rows = text(&palette);
        let ruler = rows
            .iter()
            .find(|line| line.contains('│'))
            .expect("the lap ruler");
        let ruler_width = ruler.trim_end().chars().count();
        assert_eq!(lap_ruler().chars().count(), BUCKETS);
        for (name, _, shape) in ACTIVITY {
            let drawn = strip(&widen(&shape));
            assert_eq!(drawn.chars().count(), BUCKETS, "{name} drew a short strip");
        }
        // The ruler starts with `│` in its first bucket, so its rendered width is the indent plus
        // the two columns before it plus BUCKETS — asserted as a floor rather than pinned, because
        // the trailing spaces of an all-blank tail are trimmed.
        assert!(
            ruler_width > BUCKETS,
            "the ruler was truncated: {ruler_width}"
        );
    }

    /// The screen states which of its own numbers were counted. Both halves, by name.
    ///
    /// ⚠️ Asserting only that the words "NOT measured" appear would pass on a screen that said it
    /// about the one number that IS measured. The measured claim and the unmeasured claims are
    /// checked separately, and the cache split is named as the measured one.
    #[test]
    fn the_screen_says_which_of_its_numbers_were_counted() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&palette).join("\n");
        assert!(
            rendered.contains("measured   the cache split"),
            "{rendered}"
        );
        assert_eq!(
            rendered.matches("NOT measured").count(),
            2,
            "the two uninstrumented families must each say so: {rendered}"
        );
        assert!(rendered.contains(MISSING_PER_WORKER_SPEND), "{rendered}");
    }

    /// 🔴 THE CACHE SPLIT IS THE POINT, AND IT IS LEGIBLE AS A RATIO, NOT AS TWO NUMBERS.
    ///
    /// A reader who has to divide 24.7M by 768k in their head has not been told anything. The
    /// multiple is on the frame in words.
    #[test]
    fn the_cache_split_is_stated_as_a_multiple() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&palette).join("\n");
        assert!(rendered.contains("prompt cached"), "{rendered}");
        assert!(rendered.contains("prompt computed"), "{rendered}");
        assert!(
            rendered.contains("32x"),
            "the ratio is not stated: {rendered}"
        );
    }

    /// The time table refuses to imply its rows sum to the wall clock.
    #[test]
    fn the_time_table_declares_its_own_overlap() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&palette).join("\n");
        assert!(
            rendered.contains("reasoning is INSIDE generation"),
            "{rendered}"
        );
    }

    /// A zero row reads as zero: `compaction` ran zero times and its strip is blank, not `▁`.
    ///
    /// ⚠️ This is the ramp's first step and it is a SPACE on purpose. `▁` for "nothing happened"
    /// is the density-plot version of rendering an absent field as `0`.
    #[test]
    fn a_bucket_where_nothing_happened_is_blank() {
        assert_eq!(RAMP[0], " ");
        let empty = strip(&widen(&[0; 12]));
        assert!(empty.chars().all(|glyph| glyph == ' '), "{empty:?}");
    }
}
