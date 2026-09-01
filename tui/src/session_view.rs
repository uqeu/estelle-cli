//! The live session frame, in the design language the catalog has carried since `45495f9d8`.
//!
//! 🔴 **THIS MODULE EXISTS BECAUSE `cols` WAS THE ONE PIECE THE REDESIGN NEVER WIRED.**
//! `screens.rs` renders thirteen pages built on [`crate::cols`]; until this module the live
//! TUI referenced `cols` **zero times**, so the catalog shipped one design language and the
//! customer's terminal drew another — boxed `CONVERSATION` panels instead of the two-column
//! `session │ production` frame. Measured and written up in
//! `docs/lanes/2026-08-29-r9/FINDING-the-redesign-shipped-as-a-catalog.md`.
//!
//! Every width in the live frame is decided **here**, by [`Col`], and the divider the customer
//! sees is emitted by [`crate::cols::row`] — so the geometry has one owner. If the split moves,
//! the divider moves with it; they cannot disagree.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, row, rule};
use crate::theme::Palette;

/// The gap `cols` puts either side of the divider column. Screen 9 of the catalog lays the
/// session view out as `[Col::l(46), Col::l(1), Col::l(30)]`, and `Col::l` carries `gap: 2`.
pub(crate) const DIVIDER_GAP: usize = 2;

/// The production rail's width in the catalog's screen 9 (`Col::l(30)`), and its floor here.
pub(crate) const RAIL_WIDTH: usize = 30;

/// The narrowest session column the design tolerates, from the same spec (`Col::l(46)`).
/// Below this the rail is dropped rather than squeezed — a two-column frame that cannot fit
/// two columns is worse than one column.
pub(crate) const MIN_SESSION_WIDTH: usize = 46;

/// The footer hints, in the design's form: dim, `key label` pairs separated by `·`.
///
/// 🔴 **THE CATALOG'S OWN STRING IS NOT USED HERE, AND THAT IS DELIBERATE.** Screen 9 prints
/// `tab repo · ctrl+s spend · ctrl+m models`, and **none of those three bindings exists in the
/// live TUI**: `Tab` moves focus (`main.rs`), there is no `ctrl+s`, and there is no `ctrl+m`
/// (`alt+m` toggles the context panel). A catalog screen stamped `DESIGN FIXTURE · NOT LIVE
/// DATA` may advertise an unbuilt binding; **the live footer may not** — a hint the key does
/// not honour is a hallucinated affordance, in the interface whose whole job is refusing those.
///
/// So the design contributes the FORM and the live keymap contributes the CONTENT. Every pair
/// below is pinned to its handler by
/// `every_advertised_key_is_a_binding_the_live_tui_actually_handles`.
pub(crate) const KEY_HINTS_PAIRS: &[(&str, &str)] = &[
    ("tab", "focus"),
    ("shift+tab", "autonomy"),
    ("ctrl+t", "tasks"),
    ("alt+m", "context"),
    ("/", "commands"),
];

/// Every pair, for the guard and for any frame wide enough to carry them all.
pub(crate) const KEY_HINTS: &str =
    "tab focus · shift+tab autonomy · ctrl+t tasks · alt+m context · / commands";

/// As many hints as fit in `budget` columns, in order — a footer that overruns its row pushes
/// the live status off the screen, which is a worse failure than a missing hint.
pub(crate) fn key_hints(budget: usize) -> String {
    if KEY_HINTS.chars().count() <= budget {
        return KEY_HINTS.to_string();
    }
    let mut rendered = String::new();
    for (key, label) in KEY_HINTS_PAIRS {
        let pair = format!("{key} {label}");
        let addition = if rendered.is_empty() {
            pair.chars().count()
        } else {
            pair.chars().count() + 3
        };
        if rendered.chars().count() + addition > budget {
            break;
        }
        if !rendered.is_empty() {
            rendered.push_str(" · ");
        }
        rendered.push_str(&pair);
    }
    rendered
}

/// The frame screen 9 is drawn at: `46 + 2 + 1 + 2 + 30`. The rail's share of the design is
/// this ratio, not the absolute 30 — live production strings are longer than the fixture's, and
/// a rail pinned to 30 wraps `caught · TimeoutError in charge_card` onto two rows.
pub(crate) const DESIGN_WIDTH: usize =
    MIN_SESSION_WIDTH + DIVIDER_GAP + 1 + DIVIDER_GAP + RAIL_WIDTH;

/// The rail's width at `width` columns: the design's share, never below the design's own 30,
/// never so wide that the session column drops under the design's 46.
fn rail_width(width: usize) -> Option<usize> {
    let fixed = DIVIDER_GAP.checked_add(1)?.checked_add(DIVIDER_GAP)?;
    let ceiling = width.checked_sub(fixed)?.checked_sub(MIN_SESSION_WIDTH)?;
    if ceiling < RAIL_WIDTH {
        return None;
    }
    let share = width.checked_mul(RAIL_WIDTH)? / DESIGN_WIDTH;
    Some(share.clamp(RAIL_WIDTH, ceiling))
}

/// The column spec for a frame `width` columns wide, or `None` when the design refuses to
/// split — the rail is dropped, never squeezed.
pub(crate) fn split(width: u16) -> Option<[Col; 3]> {
    let width = usize::from(width);
    let rail = rail_width(width)?;
    let fixed = DIVIDER_GAP.checked_add(1)?.checked_add(DIVIDER_GAP)?;
    let session = width.checked_sub(fixed)?.checked_sub(rail)?;
    if session < MIN_SESSION_WIDTH {
        return None;
    }
    let columns = [
        Col::l(session).gap(DIVIDER_GAP),
        Col::l(1).gap(DIVIDER_GAP),
        Col::l(rail),
    ];
    debug_assert_eq!(
        columns[0].w + columns[0].gap + columns[1].w + columns[1].gap + columns[2].w,
        width,
        "the split must consume the frame exactly, or the divider lands off the rail"
    );
    debug_assert!(
        columns[0].w >= MIN_SESSION_WIDTH,
        "a session column narrower than the design's minimum must not be returned"
    );
    Some(columns)
}

/// The session area, the one-column divider, and the production rail — derived from the same
/// [`Col`] spec the divider line is drawn from, so there is one owner for "where the split is".
pub(crate) fn split_areas(area: Rect) -> Option<(Rect, Rect, Rect)> {
    let columns = split(area.width)?;
    let session = u16::try_from(columns[0].w).ok()?;
    let lead_gap = u16::try_from(columns[0].gap).ok()?;
    let divider = u16::try_from(columns[1].w).ok()?;
    let trail_gap = u16::try_from(columns[1].gap).ok()?;
    let rail = u16::try_from(columns[2].w).ok()?;

    let left = Rect {
        width: session,
        ..area
    };
    let middle = Rect {
        x: area.x.checked_add(session)?.checked_add(lead_gap)?,
        width: divider,
        ..area
    };
    let right = Rect {
        x: middle.x.checked_add(divider)?.checked_add(trail_gap)?,
        width: rail,
        ..area
    };
    debug_assert!(
        right.x.saturating_add(rail) <= area.x.saturating_add(area.width),
        "the production rail must not run past the frame"
    );
    debug_assert!(
        left.x + left.width < middle.x && middle.x < right.x,
        "session, divider and rail must stay in that order"
    );
    Some((left, middle, right))
}

/// The `│` the customer sees between the two columns, positioned by [`crate::cols::row`].
///
/// The full-width line is rendered *under* both panes: the panes overwrite their own cells and
/// only the divider column survives, which is why the glyph cannot drift from the split.
pub(crate) fn divider(columns: &[Col; 3], palette: &Palette) -> Line<'static> {
    owned(row(
        columns,
        &[
            Cell("", palette.dim),
            Cell("│", palette.dim),
            Cell("", palette.dim),
        ],
        0,
    ))
}

/// `── session · <repo> ───…` — the design's replacement for `┌ CONVERSATION ─┐`.
pub(crate) fn session_rule(repo: &str, width: usize, palette: &Palette) -> Line<'static> {
    design_rule("session", repo, width, palette, palette.cite)
}

/// `── production · <repo> ───…` — the right rail's heading.
pub(crate) fn production_rule(repo: &str, width: usize, palette: &Palette) -> Line<'static> {
    design_rule("production", repo, width, palette, palette.green)
}

/// `── ask · <repo> ───…` — the design's replacement for `┌ ASK ESTELLE ─┐`.
pub(crate) fn ask_rule(repo: &str, width: usize, palette: &Palette) -> Line<'static> {
    design_rule("ask", repo, width, palette, palette.cite)
}

/// `── cited · <repo> ───…` — the evidence rail's heading.
pub(crate) fn cited_rule(repo: &str, width: usize, palette: &Palette) -> Line<'static> {
    design_rule("cited", repo, width, palette, palette.warn)
}

/// `── <label> · <mode> ───…` for a section INSIDE a pane — the production rail's `app`,
/// `services`, `agents`, `queue` and `github` bands.
///
/// 🔴 **THESE WERE BOLD ALL-CAPS HEADINGS (`APP HEALTH`, `AGENT HEALTH`, `GITHUB`) UNTIL NOW.**
/// The design has one heading vocabulary and it is the dashed rule; a pane that opens on a rule
/// and then switches to shouted headings is two design languages inside one column. Every rule the
/// frame draws — outer and inner — now comes from [`crate::cols::rule`].
pub(crate) fn section_rule(
    label: &str,
    mode: &str,
    width: usize,
    palette: &Palette,
    accent: Color,
) -> Line<'static> {
    design_rule(label, mode, width, palette, accent)
}

/// A pane's heading, built from a human title like `Model pool · account-wide`.
///
/// 🔴 **THIS IS WHAT REPLACES `Block::default().borders(Borders::ALL).title(" MODEL POOL ")`.**
/// Eight of the live frame's eighteen surfaces still drew a box while the catalog drew none, and
/// one rendered row carried both languages at once:
/// `── session · uqeu/estelle ───  │  ┌ CONTEXT  Alt+M · /context ────┐`.
///
/// The title's first ` · ` splits label from mode, and BOTH are lowercased: the shouted
/// `┌ SETTINGS ┐` heading is the old language, and `── settings ──` is this one.
pub(crate) fn title_rule(
    title: &str,
    width: usize,
    palette: &Palette,
    accent: Color,
) -> Line<'static> {
    let title = title.trim().to_lowercase();
    let (label, mode) = title
        .split_once(" · ")
        .map(|(label, mode)| (label.trim(), mode.trim()))
        .unwrap_or((title.as_str(), ""));
    design_rule(label, mode, width, palette, accent)
}

/// The dashes `crate::cols::rule` keeps even when the label and mode have used the whole row.
const MIN_RULE_DASHES: usize = 4;

fn design_rule(
    label: &str,
    mode: &str,
    width: usize,
    palette: &Palette,
    accent: Color,
) -> Line<'static> {
    // ⚠️ `cols::rule` does not shorten its mode: given a long one it emits a line WIDER than the
    // frame, which the rail then wraps onto a second row and the rule stops looking like a rule.
    // A repo slug alone is long enough to do it at the design's own 30-column rail. So the mode is
    // trimmed here, where the frame's width is known, rather than inside the shared primitive
    // whose other callers pass a page width they have already sized for.
    let fixed = 3 + label.chars().count() + 3 + 1 + MIN_RULE_DASHES;
    let budget = width.saturating_sub(fixed);
    let mode = if mode.chars().count() <= budget {
        mode.to_string()
    } else if budget == 0 {
        String::new()
    } else {
        mode.chars()
            .take(budget.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    };
    owned(rule(label, &mode, width, palette.dim, palette.mid, accent))
}

/// A rule borrows its label; the live frame needs it to outlive the borrow of `app`.
fn owned(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn the_split_consumes_the_frame_exactly_at_every_width_it_accepts() {
        for width in 0u16..400 {
            let Some(columns) = split(width) else {
                continue;
            };
            let total =
                columns[0].w + columns[0].gap + columns[1].w + columns[1].gap + columns[2].w;
            assert_eq!(total, usize::from(width), "width {width} did not tile");
            assert!(
                columns[2].w >= RAIL_WIDTH,
                "width {width} squeezed the rail under the design's own 30"
            );
            assert!(
                columns[0].w >= MIN_SESSION_WIDTH,
                "width {width} squeezed the session column under the design's own 46"
            );
        }
    }

    #[test]
    fn the_design_refuses_to_split_a_frame_that_cannot_hold_both_columns() {
        let narrowest = MIN_SESSION_WIDTH + DIVIDER_GAP + 1 + DIVIDER_GAP + RAIL_WIDTH;
        assert!(split(u16::try_from(narrowest).expect("fits u16")).is_some());
        assert!(split(u16::try_from(narrowest - 1).expect("fits u16")).is_none());
        assert!(split(0).is_none());
    }

    #[test]
    fn the_divider_sits_at_the_column_the_split_puts_it_at() {
        let columns = split(140).expect("140 columns splits");
        let palette = ScreenTheme::Dark.palette();
        let line = text(&divider(&columns, &palette));

        let at = line
            .char_indices()
            .filter(|(_, ch)| *ch == '│')
            .map(|(index, _)| line[..index].chars().count())
            .collect::<Vec<_>>();

        assert_eq!(at, vec![columns[0].w + columns[0].gap]);
    }

    #[test]
    fn the_areas_and_the_divider_agree_about_where_the_split_is() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 140,
            height: 40,
        };
        let columns = split(area.width).expect("140 columns splits");
        let (left, middle, right) = split_areas(area).expect("140 columns splits");

        assert_eq!(usize::from(middle.x), columns[0].w + columns[0].gap);
        assert_eq!(left.width, u16::try_from(columns[0].w).expect("fits u16"));
        assert_eq!(right.width, u16::try_from(columns[2].w).expect("fits u16"));
        assert_eq!(right.x + right.width, area.width);
    }

    /// ⚠️ TWO OWNERS THAT AGREE ARE INDISTINGUISHABLE FROM ONE — UNTIL THEY DRIFT.
    /// `KEY_HINTS` is the full string and `KEY_HINTS_PAIRS` is what the narrow path joins.
    /// Editing one without the other would silently give the wide and narrow footers different
    /// wording, and the binding guard only reads `KEY_HINTS`.
    #[test]
    fn the_hint_string_and_the_hint_pairs_cannot_drift_apart() {
        let joined = KEY_HINTS_PAIRS
            .iter()
            .map(|(key, label)| format!("{key} {label}"))
            .collect::<Vec<_>>()
            .join(" · ");
        assert_eq!(joined, KEY_HINTS);
    }

    #[test]
    fn the_footer_drops_hints_rather_than_pushing_the_status_off_the_row() {
        assert_eq!(key_hints(KEY_HINTS.chars().count()), KEY_HINTS);
        assert_eq!(key_hints(0), "");
        assert_eq!(key_hints(9), "tab focus");
        assert_eq!(key_hints(8), "");
        for budget in 0..200usize {
            assert!(
                key_hints(budget).chars().count() <= budget,
                "budget {budget} overran"
            );
        }
    }

    #[test]
    fn the_rules_name_the_repo_in_the_designs_wording() {
        let palette = ScreenTheme::Dark.palette();
        assert!(
            text(&session_rule("fernpost/checkout-api", 60, &palette))
                .starts_with("── session · fernpost/checkout-api ─")
        );
        assert!(
            text(&production_rule("fernpost", 40, &palette))
                .starts_with("── production · fernpost ─")
        );
        assert!(text(&ask_rule("fernpost", 40, &palette)).starts_with("── ask · fernpost ─"));
    }

    /// 🔴 A RULE WIDER THAN ITS FRAME IS NOT A RULE — IT IS TWO WRAPPED ROWS.
    /// The design's own 30-column rail is narrower than a repo slug plus a label, which is exactly
    /// where this used to break, so the sweep starts there and asserts the RENDERED width.
    ///
    /// ⚠️ **THE LIMIT, SAID OUT LOUD:** the sweep starts at [`RAIL_WIDTH`] because that is the
    /// narrowest surface the frame ever draws a rule on — [`split`] refuses to open a rail below
    /// it. Below that a label like `production` cannot fit at all and nothing here would help; the
    /// guard is a guarantee about the frame's real widths, not about every integer.
    #[test]
    fn no_rule_is_ever_wider_than_the_frame_it_is_drawn_in() {
        let palette = ScreenTheme::Dark.palette();
        for width in RAIL_WIDTH..160 {
            for line in [
                session_rule("fernpost/checkout-api-and-then-some", width, &palette),
                production_rule("fernpost/checkout-api", width, &palette),
                section_rule("services", "12/12 up", width, &palette, palette.green),
                section_rule("estelle", "3 unresolved", width, &palette, palette.red),
            ] {
                let rendered = text(&line).chars().count();
                assert!(
                    rendered <= width.max(MIN_RULE_DASHES),
                    "width {width} produced a {rendered}-column rule"
                );
            }
        }
    }
}
