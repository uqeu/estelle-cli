//! The walkable code graph: one renderer, shared by the live pane and design-book screen 40.
//!
//! 🔴 **WHAT THE WIRE ACTUALLY CARRIES, MEASURED 2026-09-02 — AND WHY THIS TABLE IS FILES.**
//!
//! Screen 40 was drawn with a SYMBOL per row and a `fan-in`/`fan-out` pair beside it. Every graph
//! tool on the wire is FILE-level and none of them returns a degree:
//!
//! | tool | returns | `agent/graph_tools.py` |
//! |---|---|---|
//! | `chokepoints` | `path  (score)` — betweenness centrality | `:185-191` |
//! | `core_files` | `path  (score)` — PageRank | `:177-183` |
//! | `subsystems` | comma-joined file lists, one per component | `:193-199` |
//! | `import_cycles` | `a -> b -> c` | `:169-175` |
//! | `blast_radius(file)` | the files that transitively depend on it | `:150-154` |
//!
//! `fan_out` does not exist anywhere in the server. `fan_in` exists exactly once — as
//! `len(graph.importers(path))` inside `serve/improve.py:453`, file-level, and it never reaches a
//! tool reply. So the columns were not "not wired yet"; they were **a granularity the graph does
//! not have**, and a table that printed a number there would be inventing one.
//!
//! ⚠️ **THE ROLE COLUMN'S CLAIM SURVIVED THE MEASUREMENT, WHICH IS WHY THE SCREEN IS STILL THE
//! SCREEN.** *"chokepoint · touching this moves 47 files"* is `blast_radius`'s line count — a real
//! number off the real graph. That claim is the row's whole point (a badge nobody can check versus
//! a count anybody can), and it is the one thing here that needed no change at all.
//!
//! ## The three states, and why there are three
//!
//! [`Surface::Withheld`] is not an error and not an empty list. `serve/mcp/__init__.py:1174`
//! returns the currency refusal as ORDINARY text on the success path, so a client that models this
//! as two states draws the refusal as rows — which is what [`crate::production_hud`] did until the
//! same measurement caught it. Absent, empty and refused are three different facts and they get
//! three different pictures.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row};
use crate::theme::Palette;

/// What the graph says a file IS. Derived from set membership across the four tool replies, never
/// typed by a caller — a role and a mark that could disagree is the defect the marks exist to stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    /// In `chokepoints`. The strongest claim on the pane, and the one carrying a file count.
    Chokepoint,
    /// In `core_files` but not `chokepoints` — load-bearing by PageRank, not by betweenness.
    Core,
    /// On an `import_cycles` chain. A cycle is a structural fact, not a ranking.
    Cycle,
    /// In a subsystem and in none of the above.
    Plain,
}

impl Role {
    /// The mark. `●` is measured-and-hot, `◆` is structural, `○` is quiet.
    ///
    /// ⚠️ These are the catalog's three node glyphs, and none of them is `⏺` — that one opens a
    /// TOOL CALL in the transcript and one meaning per name is the rule this repo pays for most.
    pub(crate) fn mark(self) -> &'static str {
        match self {
            Self::Chokepoint => "●",
            Self::Core | Self::Cycle => "◆",
            Self::Plain => "○",
        }
    }

    /// The word. Short by construction, so the cell cannot truncate and the claim cannot be
    /// half-read — see [`columns`] for why the count is not in here with it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Chokepoint => "chokepoint",
            Self::Core => "core",
            Self::Cycle => "in an import cycle",
            Self::Plain => "leaf",
        }
    }

    fn ink(self, palette: &Palette, hot: ratatui::style::Color) -> ratatui::style::Color {
        match self {
            Self::Chokepoint => hot,
            Self::Core => palette.mid,
            Self::Cycle => palette.warn,
            Self::Plain => palette.dim,
        }
    }
}

/// One row of the walk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub path: String,
    /// The centrality figure the tool printed, VERBATIM, parentheses and all.
    ///
    /// 🔴 **NOT RE-FORMATTED, AND NOT ROUNDED.** `graph_tools.py:182` builds `f"{path}  ({score})"`
    /// out of a float the server chose the precision of. Re-rendering it here would make this the
    /// second owner of how a centrality score reads, and the two would drift the first time the
    /// server changed a format string. `None` when the file came from `subsystems` or
    /// `import_cycles`, which carry no score — and `None` draws `—`, never `0.00`.
    pub score: Option<String>,
    /// How many files `blast_radius` said touching this one moves.
    ///
    /// ⚠️ `None` means NOT ASKED, which is the normal state: the walk fetches a blast radius for
    /// the SELECTED row only, so the pane costs four requests plus one per selection rather than
    /// one per file. `Some(0)` means asked and nothing depends on it. Absent and zero are
    /// different bytes here on purpose.
    pub moves: Option<u64>,
    pub role: Role,
}

impl Node {
    /// One row from a tool line that carries a centrality figure.
    ///
    /// 🔴 **SPLIT ON THE LAST `"  ("`, AND THE SCORE IS PASSED ON VERBATIM.**
    /// `agent/graph_tools.py:182` builds each line as `f"{path}  ({score})"` from a float whose
    /// precision the SERVER chose. Splitting on the FIRST `  (` would cut a path that contains two
    /// spaces before a bracket; re-formatting the number would make this the second owner of how a
    /// centrality score reads, and the two would drift the first time a format string moved.
    ///
    /// A line with no score is not an error — `subsystems` and `import_cycles` carry none — so the
    /// whole line is the path and [`Node::score`] is `None`, which draws `—`.
    pub(crate) fn from_tool_line(line: &str, role: Role) -> Self {
        let (path, score) = line
            .rsplit_once("  (")
            .map_or((line, None), |(path, score)| {
                (path.trim(), Some(format!("({score}")))
            });
        Self {
            path: path.to_string(),
            score,
            moves: None,
            role,
        }
    }
}

/// The pane's whole state.
pub(crate) enum Surface<'a> {
    /// The graph answered.
    Walk {
        repo: &'a str,
        /// What the user has typed to narrow the list, and the two counts that make the narrowing
        /// legible. `matched` is rows AFTER the filter; `total` is every file the four tools named.
        filter: &'a str,
        matched: usize,
        total: usize,
        nodes: &'a [Node],
        /// Which row carries the `palette.tint` band, when anything does.
        ///
        /// 🔴 **`Option`, NOT A SENTINEL INDEX.** Nothing in this binary binds a key on this pane,
        /// so "no row is selected" is the ONLY state it has today — and `usize::MAX` would have
        /// been a second meaning for a number. An absent selection and row `MAX` must not share
        /// bytes, for the same reason an absent measurement and `0` must not.
        selected: Option<usize>,
    },
    /// The server declined to answer, in its own words.
    Withheld { repo: &'a str, reason: &'a str },
}

/// mark · file · centrality · moves · role.
///
/// 🔴 **THE COUNT LIVES IN ITS OWN COLUMN, AND A GUARD FORCED THAT.**
///
/// The first version of this table wrote the role as the founder's sentence —
/// *"chokepoint · touching this moves 47 files"* — into a fixed 40-wide cell. That is 41
/// characters, so `cols` clipped it, and `no_book_screen_truncates_its_own_copy` went red on the
/// first full run. **The right fix was not a wider column.** The sentence carried a number that the
/// `moves` cell two columns to its left already carried: one derived fact, two owners, and the
/// wider owner was the one that could not be aligned, sorted or compared between rows.
///
/// So the role column names the ROLE and `moves` carries the EVIDENCE. The founder's claim is
/// intact — *a chokepoint is not a label; the count is what the graph says touching it moves* — it
/// is in the column with the other numbers now, where two rows can be compared without reading two
/// sentences. And a short label cannot truncate, so the guard that caught this cannot fire here
/// again for the same reason.
const MARK: usize = 2;
const SCORE: usize = 10;
const MOVES: usize = 6;
const ROLE: usize = 20;
const GAP: usize = 2;
const INDENT: usize = 2;
/// Everything but the file column: the indent, four gaps, and the three fixed cells.
const FIXED: usize = INDENT + MARK + SCORE + MOVES + ROLE + GAP * 4;
/// Below this a path is a hash rather than a name, so the file column stops shrinking.
const MIN_FILE: usize = 16;
/// Above this it stops growing.
///
/// ⚠️ **A FLEX COLUMN WITH NO CEILING IS A TABLE THAT SPRAWLS.** At 128 columns the file cell took
/// 84 and the rows read as four numbers marooned on the right-hand edge — technically aligned,
/// unscannable. 46 holds `serve/estelle/conveyor/code_graph.py` whole, and a longer path truncates
/// with the `…` `cols` puts there, which is the recoverable failure: the row still names a role and
/// a count while the reader widens the pane.
const MAX_FILE: usize = 46;

/// The narrowest pane worth drawing this table into at all.
pub(crate) const MIN_WIDTH: usize = FIXED + MIN_FILE;

/// The file column takes whatever the pane has left.
///
/// ⚠️ A truncated PATH is recoverable — widen the pane, and the row still names a role and a count
/// meanwhile. A truncated NUMBER is a different number and a truncated ROLE is a different claim,
/// so those three are fixed and the path is the one that flexes.
fn columns(width: usize) -> [Col; 5] {
    let file = width
        .saturating_sub(FIXED)
        .clamp(MIN_FILE, MAX_FILE.max(MIN_FILE));
    [
        Col::l(MARK),
        Col::l(file),
        Col::r(SCORE),
        Col::r(MOVES),
        Col::l(ROLE),
    ]
}

pub(crate) fn lines(
    surface: &Surface<'_>,
    palette: &Palette,
    width: usize,
    tick: u64,
    pulse: bool,
) -> Vec<Line<'static>> {
    match surface {
        Surface::Withheld { repo, reason } => withheld(repo, reason, palette, width, tick, pulse),
        Surface::Walk {
            repo,
            filter,
            matched,
            total,
            nodes,
            selected,
        } => walk(
            repo, filter, *matched, *total, nodes, *selected, palette, width, tick, pulse,
        ),
    }
}

/// 🔴 **THE REFUSAL GETS THE WHOLE PANE.** Not a footnote under an empty table: an empty table
/// under a heading that says `graph` reads as "your repo has no chokepoints", which is the
/// opposite of what the server said.
fn withheld(
    repo: &str,
    reason: &str,
    palette: &Palette,
    width: usize,
    tick: u64,
    pulse: bool,
) -> Vec<Line<'static>> {
    let mut output = vec![
        crate::cols::owned(crate::cols::rule(
            "graph",
            repo,
            width,
            palette.dim,
            palette.mid,
            palette.warn,
        )),
        blank(),
        crate::marks::headline(
            crate::marks::Mark::Blocked,
            "no walk from here",
            "the graph cannot be dated",
            palette,
            tick,
            pulse,
        ),
        blank(),
    ];
    output.extend(
        crate::gate_refusal::wrapped(
            &estelle_client::mask_secret(reason),
            width.saturating_sub(2).max(MIN_WIDTH / 2),
        )
        .into_iter()
        .map(|chunk| {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(chunk, Style::default().fg(palette.mid)),
            ])
        }),
    );
    output.push(blank());
    output.push(note(
        palette,
        "no rows are drawn: an empty risk map reads as no risk, and the server said neither.",
    ));
    output
}

#[allow(
    clippy::too_many_arguments,
    reason = "one renderer, one call site each"
)]
fn walk(
    repo: &str,
    filter: &str,
    matched: usize,
    total: usize,
    nodes: &[Node],
    selected: Option<usize>,
    palette: &Palette,
    width: usize,
    tick: u64,
    pulse: bool,
) -> Vec<Line<'static>> {
    let hot = crate::theme::pulse(palette.warn, tick, pulse)
        .fg
        .unwrap_or(palette.warn);
    let mut output = vec![
        crate::cols::owned(crate::cols::rule(
            "graph",
            repo,
            width,
            palette.dim,
            palette.mid,
            palette.cite,
        )),
        blank(),
        Line::from(vec![
            Span::styled("  / ".to_string(), Style::default().fg(palette.cite)),
            Span::styled(
                if filter.is_empty() {
                    "everything".to_string()
                } else {
                    filter.to_string()
                },
                Style::default().fg(palette.bright),
            ),
            Span::styled(
                format!("   {matched} of {} files match", thousands(total)),
                Style::default().fg(palette.dim),
            ),
        ]),
        blank(),
        head(
            &columns(width),
            &["", "file", "centrality", "moves", "role"],
            palette.mid,
            INDENT,
        ),
    ];

    for (index, node) in nodes.iter().enumerate() {
        // 🔴 `—` FOR UNKNOWN. `moves` is `None` until this row is selected and the blast radius is
        // fetched, and `0` is a real answer meaning nothing depends on it. Printing `0` for both
        // would put "safe to change" on a file nobody has measured.
        let moves = node
            .moves
            .map_or_else(|| "—".to_string(), |count| count.to_string());
        let score = node.score.clone().unwrap_or_else(|| "—".to_string());
        let ink = node.role.ink(palette, hot);
        let mut line = row(
            &columns(width),
            &[
                Cell(node.role.mark(), ink),
                Cell(&node.path, palette.mid),
                Cell(&score, palette.cite),
                Cell(
                    &moves,
                    if node.moves.is_some() {
                        ink
                    } else {
                        palette.dim
                    },
                ),
                Cell(node.role.label(), ink),
            ],
            INDENT,
        );
        if selected == Some(index) {
            line = line.style(Style::default().bg(palette.tint));
        }
        output.push(crate::cols::owned(line));
    }

    output.push(blank());
    // 🔴 **NO FOOTNOTE ADVERTISES A KEY THAT IS NOT BOUND.** The screen this renderer replaced
    // promised `enter opens the symbol · space filters · b shows the blast radius · d exports dot`,
    // and not one of those four is bound in this binary. A hint the binding cannot keep is the
    // defect `ASK_HINTS` in `main.rs` is still carrying, and repeating it here would be choosing to
    // pay for it twice. What the pane can do it says; the walk is named as missing, out loud.
    for footnote in [
        "moves is the blast radius: how many files the graph says touching this one moves.",
        "centrality is the server's own figure, printed verbatim · — means not measured, not zero",
        "not walkable yet: nothing binds a key here, so no row can be selected or drilled into.",
    ] {
        output.push(note(palette, footnote));
    }
    output
}

/// `5608` -> `5,608`.
///
/// ⚠️ A four-digit count with no separator is the kind of number a reader mis-reads by an order of
/// magnitude at a glance, and this one sits beside a single-digit `matched`. Bounded by
/// construction: one pass over the digits.
fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn blank() -> Line<'static> {
    Line::from("")
}

fn note(palette: &Palette, text: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(text.to_string(), Style::default().fg(palette.dim)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    fn text(lines: &[Line<'static>]) -> String {
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
    fn a_count_is_grouped_at_every_boundary_and_nowhere_else() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(812), "812");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(5_608), "5,608");
        assert_eq!(thousands(12_345), "12,345");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    /// The file column is bounded at BOTH ends, and a wide pane does not strand the numbers.
    #[test]
    fn the_file_column_neither_sprawls_nor_collapses() {
        let wide = columns(400)[1].w;
        let narrow = columns(10)[1].w;
        assert_eq!(wide, MAX_FILE, "a 400-column pane gave the path {wide}");
        assert_eq!(narrow, MIN_FILE, "a 10-column pane gave the path {narrow}");
        assert!(MIN_FILE < MAX_FILE);
    }

    #[test]
    fn the_score_is_split_off_the_last_bracket_and_never_reformatted() {
        let node = Node::from_tool_line("serve/api.py  (0.8137)", Role::Chokepoint);
        assert_eq!(node.path, "serve/api.py");
        assert_eq!(node.score.as_deref(), Some("(0.8137)"));
        assert_eq!(
            node.moves, None,
            "nothing has been measured for this row yet"
        );

        // A path with two spaces and a bracket in it: the LAST separator is the boundary.
        let awkward = Node::from_tool_line("src/a  (b)/c.py  (0.5)", Role::Core);
        assert_eq!(awkward.path, "src/a  (b)/c.py");
        assert_eq!(awkward.score.as_deref(), Some("(0.5)"));

        // `subsystems` and `import_cycles` carry no score. That is not an error.
        let bare = Node::from_tool_line("agent/graph_tools.py", Role::Plain);
        assert_eq!(bare.path, "agent/graph_tools.py");
        assert_eq!(bare.score, None);
    }

    /// 🔴 **THE `—` IS ASSERTED ON THE ROW, NOT ON THE FRAME — AND THE FIRST VERSION OF THIS TEST
    /// WAS INERT FOR EXACTLY THAT REASON.**
    ///
    /// It asserted `frame.contains('—')` over a node whose SCORE was also `None`. So it passed
    /// while `moves` printed `0`: the em dash it found was the score's. A guard that fires on a
    /// cell it was not looking at is a guard on nothing, and it survived one mutation round before
    /// `guard_mutants` caught it. The node below carries a real score, so the only `—` a row can
    /// contain is the one this test is about, and the footnote row (which also carries an em dash)
    /// is excluded by matching the row BY PATH.
    #[test]
    fn an_unmeasured_blast_radius_draws_an_em_dash_and_a_measured_zero_draws_a_sentence() {
        let palette = ScreenTheme::Dark.palette();
        let unmeasured = Node {
            path: "serve/leaf.py".to_string(),
            score: Some("(0.42)".to_string()),
            moves: None,
            role: Role::Plain,
        };
        let measured = Node {
            moves: Some(0),
            ..unmeasured.clone()
        };
        let row_for = |node: &Node| -> String {
            let drawn = text(&lines(
                &Surface::Walk {
                    repo: "uqeu/estelle",
                    filter: "",
                    matched: 1,
                    total: 1,
                    nodes: std::slice::from_ref(node),
                    selected: None,
                },
                &palette,
                120,
                0,
                false,
            ));
            drawn
                .lines()
                .find(|line| line.contains("serve/leaf.py"))
                .unwrap_or_else(|| panic!("no row for the node:\n{drawn}"))
                .to_string()
        };

        let unknown = row_for(&unmeasured);
        assert!(
            unknown.contains("(0.42)"),
            "the score is present, so a stray em dash cannot be its:\n{unknown}"
        );
        assert_eq!(
            unknown.matches('—').count(),
            1,
            "exactly one unknown cell, and it is `moves`:\n{unknown}"
        );
        assert!(
            !unknown.contains('0') || unknown.contains("(0.42)"),
            "{unknown}"
        );
        assert!(
            unknown.contains("leaf"),
            "the role is named whether or not the count has been measured:\n{unknown}"
        );

        let known = row_for(&measured);
        assert_eq!(
            known.matches('—').count(),
            0,
            "a MEASURED zero fills the cell, so nothing is unknown on this row:\n{known}"
        );
        assert!(
            known.split_whitespace().any(|cell| cell == "0"),
            "a measured zero is an answer and fills the cell:\n{known}"
        );
    }

    /// The production refusal, drawn. The actionable half must survive to the last word.
    #[test]
    fn the_withheld_pane_draws_the_servers_own_sentence_and_no_rows() {
        let palette = ScreenTheme::Dark.palette();
        let reason = "uqeu/estelle: currency UNKNOWN — this repo has never been swept, so there is no graph to date. Sweep this repo first — nothing has been indexed for it yet.";
        let drawn = text(&lines(
            &Surface::Withheld {
                repo: "uqeu/estelle",
                reason,
            },
            &palette,
            110,
            0,
            false,
        ));
        assert!(drawn.contains("no walk from here"), "{drawn}");
        // 🔴 REASSEMBLED, NOT SUBSTRING-MATCHED ON THE RENDERED TEXT. The refusal WRAPS, so
        // `contains("Sweep this repo first")` fails on a correctly-wrapped pane and passes on a
        // TRUNCATED one that happens to keep the phrase — it would have been an assertion that
        // fires on the right behaviour and not on the wrong one. Joining the rows back into one
        // sentence asserts the thing that actually matters: nothing was dropped.
        let rejoined = drawn
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let expected = reason.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            rejoined.contains(&expected),
            "the refusal lost characters between the wire and the pane:\n{drawn}"
        );
        assert!(!drawn.contains("centrality"), "no table header: {drawn}");
    }

    #[test]
    fn no_box_corner_reaches_either_state() {
        let palette = ScreenTheme::Dark.palette();
        let nodes = [
            Node::from_tool_line("serve/api.py  (0.81)", Role::Chokepoint),
            Node::from_tool_line("conveyor/code_graph.py  (0.31)", Role::Core),
            Node::from_tool_line("serve/b.py", Role::Cycle),
        ];
        for drawn in [
            text(&lines(
                &Surface::Walk {
                    repo: "uqeu/estelle",
                    filter: "serve",
                    matched: nodes.len(),
                    total: nodes.len(),
                    nodes: &nodes,
                    selected: Some(1),
                },
                &palette,
                130,
                0,
                true,
            )),
            text(&lines(
                &Surface::Withheld {
                    repo: "uqeu/estelle",
                    reason: "currency UNKNOWN",
                },
                &palette,
                130,
                0,
                true,
            )),
        ] {
            for corner in ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'] {
                assert!(!drawn.contains(corner), "{corner} in:\n{drawn}");
            }
        }
    }
}
