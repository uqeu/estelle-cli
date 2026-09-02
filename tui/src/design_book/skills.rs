//! Screens 12–14: the skill surface, from `cli-reference-2026-08-24/skill.png`, `skill 2.png` and
//! the 2026-09-02 review's *"Skills — the biggest set of notes"*.
//!
//! 🔴 **THE THREE DECISIONS THAT ARE NOT NEGOTIABLE HERE.**
//! 1. **The dropdown is not a box.** *"The box shouldn't be a box at all."* The mock drew
//!    `┌ skills ────` with a `│` gutter. A rule plus a `palette.tint` band carry everything the frame
//!    carried, and the band also says *which* row — which the frame never did.
//! 2. **The offer fires BEFORE the message is sent.** *"The moment they send that message it should
//!    go into that first answer thing — hey, do you want to send this, or send this with the skill
//!    involved."* [`offered`] is drawn at the instant `enter` is pressed and nothing has left the
//!    machine: the draft is still yours, `esc` returns it untouched.
//! 3. **It offers, never auto-runs.** Printed on the founder's own mock, the same rule as
//!    propose-only auto-repair applied to skills; `the_offer_never_claims_to_have_run` pins it.
//!
//! ⚠️ **What this module does NOT claim.** These are design frames, not the live picker (that is
//! `bottom_pane/skills_toggle_view.rs`), and the counts are the founder's own figures off
//! `skill 2.png` — illustrative sample data, not a measurement of anybody's installed set.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::theme::Palette;

/// One owned span in one palette colour, so no call site repeats `Style::default().fg(..)`.
fn tinted(body: &str, colour: Color) -> Span<'static> {
    Span::styled(body.to_string(), Style::default().fg(colour))
}

/// The rule length every screen here shares, so the three read as one surface.
const RULE_WIDTH: usize = 78;

/// The skill under the cursor, and its cost. **One owner**: the dropdown row, the composer binding
/// and the offer sentence all read these two, so they cannot drift apart.
const MATCHED: &str = "improve-codebase-architecture";
const MATCHED_COST: &str = "~640";

/// The founder's figures off `skill 2.png`. Named, not inlined: the header, the footnote and the
/// test all want them, and a second literal is a second owner.
const SKILLS_TOTAL: usize = 238;
const SKILLS_ON: usize = 189;
const MAX_COMPOSE: usize = 3;
const COMPOSE_BUDGET: &str = "1,800";

/// The row the cursor sits on. Index, not a flag, so "which row is banded" has one answer.
const SELECTED: usize = 0;

/// `state`, `name`, `cost` — the browse table's three data columns.
type SkillRow = (&'static str, &'static str, &'static str);

/// The dropdown's rows: what `/` + tab offers you, in match order.
const TYPED_ROWS: &[(&str, &str)] = &[
    (MATCHED, MATCHED_COST),
    ("refactor-cleaner", "~410"),
    ("systematic-debugging", "~520"),
    ("verification-before-completion", "~380"),
    ("whitepaper-draft", "~641"),
    ("bug-hunt", "~310"),
];

/// The browse rows. `off` is a PERMISSION, not a filter — an off skill stays listed and Estelle may
/// neither recommend nor auto-use it, which is why `off` rows are drawn rather than hidden.
const BROWSE_ROWS: &[SkillRow] = &[
    ("on", MATCHED, MATCHED_COST),
    ("on", "systematic-debugging", "~520"),
    ("off", "whitepaper-draft", "~641"),
    ("on", "bug-hunt", "~310"),
    ("on", "verification-before-completion", "~380"),
    ("off", "refactor-cleaner", "~410"),
    ("on", "test-driven-development", "~470"),
    ("off", "orca-port-review", "~590"),
];

/// Gutter marker, name, right-aligned token cost — and the same with an on/off cell for browse.
const TYPED_COLS: &[Col] = &[Col::l(1).gap(1), Col::l(38), Col::r(6)];
const BROWSE_COLS: &[Col] = &[Col::l(1).gap(1), Col::l(3), Col::l(32), Col::r(6)];

/// `enter` / `tab` / `esc` against what each does. ⚠️ 49 is the longest line, measured: at 48 `cols`
/// truncated the `esc` row to *"…as you wrote …"*, which is the class of defect a hand-counted space
/// would have shipped silently. `no_screen_truncates_its_own_copy` keeps it.
const CHOICE_COLS: &[Col] = &[Col::l(5), Col::l(49)];

/// Indented two, the way every catalog screen indents.
const INDENT: usize = 2;

/// The width `BROWSE_COLS` + [`INDENT`] must produce. Written out, not derived: a column edit has to
/// change this number too, in the open.
#[cfg(test)]
const BROWSE_ROW_WIDTH: usize = 49;

/// Screen 12 — **skills, typed.** You pressed `/` then tab; the dropdown docked above the composer.
/// `tick`/`pulse` drive the caret only: a list that animates while you read it cannot be read.
pub(crate) fn typed(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    let mut lines = vec![
        owned(rule(
            "skills",
            "type to filter",
            RULE_WIDTH,
            palette.dim,
            palette.mid,
            palette.skill,
        )),
        blank(),
    ];

    for (index, (name, cost)) in TYPED_ROWS.iter().enumerate() {
        let selected = index == SELECTED;
        let marker = if selected { "›" } else { "" };
        let name_colour = if selected { palette.skill } else { palette.mid };
        let line = owned(row(
            TYPED_COLS,
            &[
                Cell(marker, palette.skill),
                Cell(name, name_colour),
                Cell(cost, palette.dim),
            ],
            INDENT,
        ));
        lines.push(if selected {
            line.style(Style::default().bg(palette.tint))
        } else {
            line
        });
    }

    lines.push(blank());
    let caret = crate::theme::pulse(palette.bright, tick, pulse);
    lines.push(Line::from(vec![
        tinted("  » ", palette.skill),
        tinted("refactor the auth module", palette.bright),
        Span::styled("▏".to_string(), caret),
    ]));
    lines.push(Line::from(vec![
        tinted("  estelle · skill: ", palette.dim),
        tinted(MATCHED, palette.skill),
    ]));
    lines.push(blank());
    for footnote in [
        "enter preloads the skill · you never type a slash",
        "esc closes the dropdown · it docks above the composer, it is not a page you leave",
        "no frame, on purpose: the band says WHICH row, which a border never did",
    ] {
        lines.push(note(palette, footnote));
    }
    lines
}

/// Screen 13 — **skills, offered.** You pressed enter and **nothing has been sent**. The
/// interception point, and the whole point: the choice happens before the message leaves.
pub(crate) fn offered(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    let draft = owned(row(
        &[Col::l(1).gap(1), Col::l(60)],
        &[
            Cell("❯", palette.skill),
            Cell("i want to organise this codebase better", palette.bright),
        ],
        INDENT,
    ))
    .style(Style::default().bg(palette.tint));

    let mut lines = vec![
        owned(rule(
            "estelle",
            "nothing sent yet",
            RULE_WIDTH,
            palette.dim,
            palette.mid,
            palette.skill,
        )),
        blank(),
        draft,
        blank(),
        Line::from(vec![
            tinted("  » ", palette.skill),
            tinted("This looks like ", palette.mid),
            tinted(MATCHED, palette.skill),
            tinted(".", palette.mid),
        ]),
        note(
            palette,
            &format!(
                "  matched on symbol overlap, then {SKILLS_TOTAL} pre-embedded descriptions · {MATCHED_COST} tok if you take it"
            ),
        ),
        blank(),
    ];

    for (key, what, key_colour) in [
        ("enter", "send as typed", palette.dim),
        ("tab", "send with the skill", palette.bright),
        (
            "esc",
            "dismiss · the draft stays exactly as you wrote it",
            palette.dim,
        ),
    ] {
        lines.push(owned(row(
            CHOICE_COLS,
            &[Cell(key, key_colour), Cell(what, palette.dim)],
            INDENT,
        )));
    }

    lines.push(blank());
    lines.push(note(
        palette,
        "It offers, never auto-runs. A skill that fires without you asking is an agent choosing for you.",
    ));
    lines.push(blank());
    for footnote in [
        "the offer fires on send, before the message leaves · not after the answer comes back",
        "a skill that is off in browse is never offered here · the toggle is the permission",
    ] {
        lines.push(note(palette, footnote));
    }
    lines
}

/// Screen 14 — **skills, browse and toggle.** The density the shipped screen lacks: a real total,
/// an on-count, a per-skill token cost, and the compose budget those costs are spent against.
pub(crate) fn browse(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    let header = format!("{SKILLS_TOTAL} · {SKILLS_ON} on");
    let budget = format!("{MAX_COMPOSE} max compose · {COMPOSE_BUDGET} tok budget");

    let mut lines = vec![
        owned(rule(
            "skills",
            &header,
            RULE_WIDTH,
            palette.dim,
            palette.mid,
            palette.skill,
        )),
        note(palette, "/ search · space toggle · enter run"),
        blank(),
        owned(head(
            BROWSE_COLS,
            &["", "", "skill", "tok"],
            palette.dim,
            INDENT,
        )),
    ];

    for (index, (state, name, cost)) in BROWSE_ROWS.iter().enumerate() {
        let selected = index == SELECTED;
        let enabled = *state == "on";
        let marker = if selected { "›" } else { "" };
        let state_colour = if enabled { palette.green } else { palette.dim };
        let name_colour = match (selected, enabled) {
            (true, _) => palette.skill,
            (false, true) => palette.mid,
            (false, false) => palette.dim,
        };
        let line = owned(row(
            BROWSE_COLS,
            &[
                Cell(marker, palette.skill),
                Cell(state, state_colour),
                Cell(name, name_colour),
                Cell(cost, palette.dim),
            ],
            INDENT,
        ));
        lines.push(if selected {
            line.style(Style::default().bg(palette.tint))
        } else {
            line
        });
    }

    lines.push(blank());
    lines.push(Line::from(tinted(&format!("  {budget}"), palette.mid)));
    lines.push(blank());
    for footnote in [
        "off is a permission, not a filter: a skill that is off, Estelle may neither recommend nor auto-use.",
        "space changes that permission · enter fires the row under the cursor · the two are different keys",
        "the cost column is what the skill body spends before you have asked a single question",
    ] {
        lines.push(note(palette, footnote));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    const CORNERS: [char; 9] = ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];

    fn text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn width(line: &Line<'_>) -> usize {
        text(line).chars().count()
    }

    fn join(lines: Vec<Line<'static>>) -> String {
        lines.iter().map(text).collect::<Vec<_>>().join("\n")
    }

    /// 🔴 THE FOUNDER SAID "NO BOX" THREE TIMES, AND SAID WHAT REPLACES IT. Both halves asserted:
    /// dropping the frame WITHOUT the band leaves a list with no cursor, and that is silent, not red.
    #[test]
    fn the_selected_row_is_highlighted_not_framed() {
        let palette = ScreenTheme::Dark.palette();
        let lines = browse(&palette, 0, true);

        let banded = lines
            .iter()
            .filter(|line| line.style.bg == Some(palette.tint))
            .count();
        assert_eq!(
            banded, 1,
            "browse must band exactly one row, banded {banded}"
        );

        let band = lines
            .iter()
            .find(|line| line.style.bg == Some(palette.tint))
            .expect("one banded row");
        let banded_text = text(band);
        assert!(
            banded_text.contains('›') && banded_text.contains(MATCHED),
            "the band must carry the marker and the whole row, got {banded_text:?}"
        );
        assert!(
            banded_text.contains(MATCHED_COST),
            "the band must reach the cost column, got {banded_text:?}"
        );

        for line in &lines {
            let rendered = text(line);
            for corner in CORNERS {
                assert!(
                    !rendered.contains(corner),
                    "browse drew a box corner {corner:?} in {rendered:?}"
                );
            }
        }
    }

    /// 🔴 ON THIS SCREEN NO SKILL HAS RUN, AND IT MAY NOT IMPLY ONE HAS. Drawn before anything is
    /// sent, so past-tense wording ("running", "applied") would describe a system that auto-fires —
    /// the exact behaviour the founder's own mock forbids in print.
    #[test]
    fn the_offer_never_claims_to_have_run() {
        let palette = ScreenTheme::Dark.palette();
        let screen = join(offered(&palette, 0, true));
        assert!(
            screen.contains("offers, never auto-runs"),
            "the principle line is missing:\n{screen}"
        );
        assert!(
            screen.contains("send with the skill"),
            "the gallery needle is missing:\n{screen}"
        );
        assert!(
            screen.contains("nothing sent yet"),
            "the screen must say the message has not left:\n{screen}"
        );
        for banned in ["running", "applied"] {
            assert!(
                !screen.contains(banned),
                "the offer screen said {banned:?}, which claims a skill fired:\n{screen}"
            );
        }
    }

    /// The property `cols` exists to guarantee. ⚠️ Compared against a WRITTEN-OUT number as well as
    /// row-to-row: two rows from one `Col` slice are equal by construction, so that half alone is a
    /// tautology. The pinned total fails on a widened column hand-padded back.
    #[test]
    fn every_skill_row_is_the_same_width() {
        let palette = ScreenTheme::Dark.palette();
        let rows = browse(&palette, 0, true)
            .into_iter()
            .filter(|line| text(line).contains('~'))
            .collect::<Vec<_>>();

        assert_eq!(
            rows.len(),
            BROWSE_ROWS.len(),
            "every skill row must carry a token cost"
        );

        let first = &rows[0];
        let last = &rows[rows.len() - 1];
        assert_ne!(
            text(first),
            text(last),
            "the two rows compared must be different rows"
        );
        assert_eq!(
            width(first),
            width(last),
            "{:?} and {:?} are different widths",
            text(first),
            text(last)
        );
        assert_eq!(width(first), BROWSE_ROW_WIDTH);

        // 🔴 THE HALF THAT IS NOT TRUE BY CONSTRUCTION. Row 0 is `› on  <name>`, the last is
        // `  off <name>` — a marker against none, a two-char state against three. Only `Col` padding
        // puts both names on the same screen column; hand-counted spaces would be two apart.
        let name_column = |line: &Line<'_>, name: &str| {
            let rendered = text(line);
            rendered
                .find(name)
                .map(|byte| rendered[..byte].chars().count())
                .expect("the row names its skill")
        };
        assert_eq!(
            name_column(first, BROWSE_ROWS[0].1),
            name_column(last, BROWSE_ROWS[BROWSE_ROWS.len() - 1].1),
            "the name column drifted between a selected `on` row and an unselected `off` row"
        );
    }

    /// The three screens are one surface: the skill under the cursor is the one the composer binds
    /// and the one the offer names. Drift is "two owners that agree until they don't".
    #[test]
    fn one_skill_name_owns_all_three_screens() {
        let palette = ScreenTheme::Cream.palette();
        let typed_text = join(typed(&palette, 0, true));
        assert!(typed_text.contains("skill: "), "{typed_text}");
        assert!(typed_text.contains(MATCHED), "{typed_text}");
        assert!(join(offered(&palette, 0, true)).contains(MATCHED));
        assert!(join(browse(&palette, 0, true)).contains("max compose"));
    }

    /// The gallery's contract, on every `cargo test`. ⚠️ **A NEEDLE IN A `Line` IS NOT A NEEDLE IN
    /// THE FRAME** — the renderer clips an over-wide row and reports nothing, so both halves are
    /// checked, and the registration is LOOKED UP rather than repeated here.
    #[test]
    fn each_skill_screen_renders_the_needle_its_book_entry_promises() {
        let screens: [(&str, fn(&Palette, u64, bool) -> Vec<Line<'static>>); 3] = [
            ("12-skills-typed", typed),
            ("13-skills-offered", offered),
            ("14-skills-browse", browse),
        ];
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for (name, render) in screens {
                let entry = crate::design_book::SCREENS
                    .iter()
                    .find(|screen| screen.name == name)
                    .unwrap_or_else(|| panic!("{name} is not registered in design_book::SCREENS"));
                let lines = render(&palette, 0, true);
                let frame = join(lines.clone());
                assert!(
                    frame.contains(entry.needle),
                    "{name} never renders its needle {:?}:\n{frame}",
                    entry.needle
                );
                for line in &lines {
                    let width = text(line).chars().count();
                    assert!(
                        width <= usize::from(entry.width),
                        "{name} rendered a {width}-column row into a {}-column frame",
                        entry.width
                    );
                }
                assert!(
                    lines.len() <= usize::from(entry.height),
                    "{name} rendered {} rows into a {}-row frame",
                    lines.len(),
                    entry.height
                );
            }
        }
    }

    /// 🔴 A COLUMN ONE TOO NARROW EATS THE END OF A SENTENCE AND NOTHING GOES RED. This test FIRED:
    /// `CHOICE_COLS` was 48 against a 49-character line and shipped *"…as you wrote …"*. Truncating
    /// suits a live row; on a DESIGN frame a `…` is a defect, and only an assertion tells them apart.
    #[test]
    fn no_screen_truncates_its_own_copy() {
        let palette = ScreenTheme::Dark.palette();
        for (name, lines) in [
            ("typed", typed(&palette, 0, true)),
            ("offered", offered(&palette, 0, true)),
            ("browse", browse(&palette, 0, true)),
        ] {
            for line in &lines {
                let rendered = text(line);
                assert!(
                    !rendered.contains('…'),
                    "{name} truncated a cell: {rendered:?}"
                );
            }
        }
    }
}
