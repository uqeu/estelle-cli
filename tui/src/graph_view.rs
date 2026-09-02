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

/// What `enter` opens on the selected row.
///
/// 🔴 **EVERY FIELD IS A REPLY ALREADY IN HAND, AND THAT IS WHY OPENING A ROW COSTS NO REQUEST.**
/// `subsystems` and `import_cycles` are fetched once for the whole walk, so the peers and the
/// cycle chain for any row are a lookup rather than a call. [`Self::dependents`] is the one field
/// that is not — it is `None` until `b` measures it, and `None` here means NOT ASKED, never
/// "nothing depends on this". An empty slice means asked and empty. Three states, three bytes.
pub(crate) struct Detail<'a> {
    pub node: &'a Node,
    /// The other files `subsystems` put in the same component. Empty is a real answer: a file can
    /// be the only member of its own component.
    pub subsystem: &'a [String],
    /// The `import_cycles` chain this file sits on, verbatim, arrows and all. Empty = on none.
    pub cycle: &'a [String],
    /// What `blast_radius` said depends on this file. `None` until `b` is pressed.
    pub dependents: Option<&'a [String]>,
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
        /// 🔴 **`Option`, NOT A SENTINEL INDEX.** `usize::MAX` would have been a second meaning
        /// for a number. An absent selection and row `MAX` must not share bytes, for the same
        /// reason an absent measurement and `0` must not — and a pane with no rows at all still
        /// has to say "nothing is selected" rather than "row 18446744073709551615 is".
        selected: Option<usize>,
        /// The keys THIS pane binds, in the order the footer prints them.
        ///
        /// 🔴 **A FIELD, NOT A CONSTANT, BECAUSE TWO PANES DRAW THIS TABLE AND ONLY ONE OF THEM
        /// TAKES KEYS.** [`crate::production_hud`] renders the same rows inside a read-only rail;
        /// [`crate::graph_walk`] renders them under a keymap. Baking one footer into the renderer
        /// would advertise a binding on the pane that does not have it, which is the exact defect
        /// the previous version of this footer was written to avoid. An EMPTY slice draws the
        /// read-only sentence instead — absent keys and no keys are the same fact here, and it is
        /// spelled out rather than left as a blank row.
        ///
        /// ⚠️ The walk passes [`crate::graph_walk::KEYS`], which is the same slice its key handler
        /// dispatches on, so the hint row cannot advertise a chord the handler drops.
        hints: &'a [(&'a str, &'a str)],
        /// The row the user opened with `enter`, when one is open.
        ///
        /// ⚠️ Composed only from replies already in hand — see [`Detail`]. Opening a row costs no
        /// request, which is why `enter` and `b` are different keys.
        detail: Option<&'a Detail<'a>>,
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
            hints,
            detail,
        } => walk(
            repo, filter, *matched, *total, nodes, *selected, hints, *detail, palette, width, tick,
            pulse,
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
    hints: &[(&str, &str)],
    detail: Option<&Detail<'_>>,
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

    if let Some(detail) = detail {
        output.extend(detail_lines(detail, palette, width));
    }

    output.push(blank());
    // 🔴 **NO FOOTNOTE ADVERTISES A KEY THAT IS NOT BOUND.** The screen this renderer replaced
    // promised `enter opens the symbol · space filters · b shows the blast radius · d exports dot`,
    // and not one of those four was bound in this binary. The row below is no longer a sentence
    // written here: it is `hints`, which the walk fills from the same slice its key handler
    // dispatches on. A chord can therefore be advertised here only if the handler takes it, and a
    // pane that binds nothing says so instead of printing an empty row.
    for footnote in [
        "moves is the blast radius: how many files the graph says touching this one moves.",
        "centrality is the server's own figure, printed verbatim · — means not measured, not zero",
    ] {
        output.push(note(palette, footnote));
    }
    output.push(hint_row(hints, palette));
    output
}

/// The key row, or the sentence a pane with no keys owes the reader.
///
/// ⚠️ **THE EMPTY CASE IS COPY, NOT A BLANK.** A footer that simply disappears when nothing is
/// bound reads as "there are no keys worth mentioning", which is what a reader concludes right
/// before pressing enter and getting nothing.
fn hint_row(hints: &[(&str, &str)], palette: &Palette) -> Line<'static> {
    if hints.is_empty() {
        // ⚠️ **"NO ROW IS SELECTABLE", NOT "NO KEY IS BOUND".** The production rail that draws this
        // table with no hints DOES bind `enter` — it drills into the mermaid path — so the wider
        // sentence would have been false on the only pane that prints it.
        return note(
            palette,
            "no row is selectable here: this table is read-only. /graph walk is the walkable one.",
        );
    }
    note(
        palette,
        &hints
            .iter()
            .map(|(key, label)| format!("{key} {label}"))
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
    )
}

/// The opened row: what the four replies already say about one file.
///
/// 🔴 **THREE SECTIONS, AND EACH ONE PRINTS A SENTENCE WHEN IT IS EMPTY.** A blank under
/// `subsystem` is indistinguishable from a subsystem that was never fetched, and this pane exists
/// because that distinction is the product. `dependents` carries the third state — `None` is *not
/// asked*, and it names the key that would ask.
fn detail_lines(detail: &Detail<'_>, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let mut output = vec![
        blank(),
        crate::cols::owned(crate::cols::rule(
            "open",
            &detail.node.path,
            width,
            palette.dim,
            palette.mid,
            palette.cite,
        )),
        blank(),
    ];
    output.push(note(
        palette,
        &format!(
            "role  {}   centrality  {}",
            detail.node.role.label(),
            detail.node.score.as_deref().unwrap_or("\u{2014}")
        ),
    ));
    output.push(blank());
    output.push(section(palette, "in the same subsystem"));
    if detail.subsystem.is_empty() {
        output.push(note(
            palette,
            "  nothing: the graph put no other file in this component.",
        ));
    } else {
        // Bounded read: the pane shows a window and says how wide the whole thing is, because a
        // capped list that reports its cap as a total is the claim nobody checked.
        for peer in detail.subsystem.iter().take(DETAIL_ROWS) {
            output.push(note(palette, &format!("  {peer}")));
        }
        if detail.subsystem.len() > DETAIL_ROWS {
            output.push(note(
                palette,
                &format!(
                    "  \u{2026} {} more in this subsystem, not shown",
                    detail.subsystem.len() - DETAIL_ROWS
                ),
            ));
        }
    }
    output.push(blank());
    output.push(section(palette, "on an import cycle"));
    if detail.cycle.is_empty() {
        output.push(note(palette, "  no: this file is on none of the reported cycles."));
    } else {
        for chain in detail.cycle.iter().take(DETAIL_ROWS) {
            output.push(note(palette, &format!("  {chain}")));
        }
    }
    output.push(blank());
    output.push(section(palette, "what depends on it"));
    match detail.dependents {
        // 🔴 NOT ASKED IS ITS OWN SENTENCE AND IT NAMES THE KEY. Printing "0 files" here would be
        // "safe to change" over a file nobody measured — the same defect the `\u{2014}` in the
        // `moves` column exists to prevent, one screen deeper.
        None => output.push(note(
            palette,
            "  not measured. press b to take this file's blast radius.",
        )),
        Some([]) => output.push(note(
            palette,
            "  measured: nothing in the graph depends on this file.",
        )),
        Some(files) => {
            for file in files.iter().take(DETAIL_ROWS) {
                output.push(note(palette, &format!("  {file}")));
            }
            if files.len() > DETAIL_ROWS {
                output.push(note(
                    palette,
                    &format!("  \u{2026} {} more, not shown", files.len() - DETAIL_ROWS),
                ));
            }
        }
    }
    output
}

/// How many rows of a list the opened row shows before it says how many it is not showing.
const DETAIL_ROWS: usize = 6;

fn section(palette: &Palette, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(label.to_string(), Style::default().fg(palette.mid)),
    ])
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
                    hints: &[],
                    detail: None,
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

    /// 🔴 **A PANE THAT BINDS NOTHING MAY NOT PRINT A CHORD, AND ONE THAT BINDS KEYS MUST PRINT
    /// THEM.** Both halves, because a footer that is always silent passes the first assertion and
    /// a footer that always prints the walk's row passes the second. The read-only side is the one
    /// that ships inside the production rail, where a `b blast radius` hint would name a key that
    /// pane has never had.
    #[test]
    fn the_footer_names_a_chord_only_where_a_chord_is_bound() {
        let palette = ScreenTheme::Dark.palette();
        let nodes = [Node::from_tool_line("serve/api.py  (0.81)", Role::Chokepoint)];
        let drawn = |hints: &[(&str, &str)]| {
            text(&lines(
                &Surface::Walk {
                    repo: "uqeu/estelle",
                    filter: "",
                    matched: 1,
                    total: 1,
                    nodes: &nodes,
                    selected: None,
                    hints,
                    detail: None,
                },
                &palette,
                130,
                0,
                false,
            ))
        };

        let read_only = drawn(&[]);
        assert!(
            read_only.contains("no row is selectable here"),
            "a keyless pane must say so rather than print an empty footer:\n{read_only}"
        );
        for (key, label) in crate::graph_walk::KEYS {
            assert!(
                !read_only.contains(&format!("{key} {label}")),
                "the read-only table advertised `{key} {label}`, which reaches nothing here:\n{read_only}"
            );
        }

        let walkable = drawn(crate::graph_walk::KEYS);
        for (key, label) in crate::graph_walk::KEYS {
            assert!(
                walkable.contains(&format!("{key} {label}")),
                "the walkable pane dropped `{key} {label}` from its footer:\n{walkable}"
            );
        }
        assert!(
            !walkable.contains("no row is selectable here"),
            "the walkable pane called itself read-only:\n{walkable}"
        );
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
                    hints: &[("enter", "open")],
                    detail: None,
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
