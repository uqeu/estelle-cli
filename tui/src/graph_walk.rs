//! The walk: the state and the keys behind design-book screen 40.
//!
//! 🔴 **THE READ PATH WAS ALREADY REAL. THIS IS THE HALF THAT MOVES.**
//! [`crate::graph_view`] has drawn a `filter`, a `matched of total` count and a `selected` band
//! since it was written, and every one of those was a constant: nothing in the binary could change
//! them, so the pane's own last footnote said *"not walkable yet: nothing binds a key here"*. This
//! module is what that footnote was waiting for — the same renderer, with a keymap behind it.
//!
//! ## What each key is allowed to mean, and why it is not more
//!
//! | key | what it does | what it costs |
//! |---|---|---|
//! | `↑` `↓` | move the band | nothing |
//! | `enter` | open the row | **nothing** — the detail is composed from replies already in hand |
//! | `space` | narrow to the selected row's role | nothing |
//! | `b` | take that file's blast radius | **one** `blast_radius` call |
//! | `/` | open the filter line | nothing |
//! | `x` | write the graph as `.dot` | one local file |
//! | `esc` | leave the filter line, else close the walk | nothing |
//!
//! ⚠️ **`enter` AND `b` ARE DIFFERENT KEYS BECAUSE THEY HAVE DIFFERENT PRICES.** Opening a row is
//! free and a user will do it on every row; measuring a blast radius is a request and they will
//! not. Folding the request into `enter` would make walking the list cost one call per arrow key.
//!
//! 🔴 **AND THE COLUMN THE ORIGINAL SCREEN ASKED FOR IS STILL NOT DRAWN.** `fan_out` does not
//! exist anywhere in the server and `fan_in` never reaches a tool reply — measured, and recorded
//! at the head of [`crate::graph_view`]. Binding keys to a pane does not make a missing
//! measurement appear, and no key here produces one.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::graph_view::{Detail, Node, Role};

/// The keys the walk binds, in the order the footer prints them.
///
/// 🔴 **ONE OWNER FOR "WHICH KEYS THIS PANE HAS".** [`crate::graph_view`] prints this slice and
/// [`Walk::key`] dispatches on it, so a chord can be advertised only if the handler takes it.
/// `the_footer_advertises_exactly_the_keys_that_move_the_panel` presses every entry and asserts
/// the pane changed — a hint whose key does nothing goes red on the assertion, not on a grep.
pub(crate) const KEYS: &[(&str, &str)] = &[
    ("\u{2191}\u{2193}", "walk"),
    ("enter", "open"),
    ("space", "role"),
    ("b", "blast radius"),
    ("/", "filter"),
    ("x", "export dot"),
    ("esc", "close"),
];

/// The keys while the filter line is open.
///
/// 🔴 **A SEPARATE ROW BECAUSE `esc` MEANS SOMETHING ELSE HERE, AND ONE WORD MAY NOT MEAN TWO
/// THINGS.** With the filter line open `esc` clears the filter; with it shut `esc` closes the
/// walk. Printing one row for both states would be a footer that is wrong in whichever state the
/// reader is actually in.
pub(crate) const FILTER_KEYS: &[(&str, &str)] = &[
    ("type", "narrow the walk"),
    ("backspace", "undo a letter"),
    ("enter", "keep it and walk"),
    ("esc", "clear it"),
];

/// What the walk needs the app to do, because the walk owns no client and no filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// The key was consumed and nothing outside this struct has to happen.
    Handled,
    /// The key is NOT the walk's — hand it back to the app unchanged.
    ///
    /// 🔴 **DISTINCT FROM [`Self::Handled`] BECAUSE THE DIFFERENCE IS WHETHER THE APP STILL SEES
    /// THE KEY.** The walk refuses every chord; if refusing meant `Handled`, a walk left open
    /// would quietly take `ctrl+t`, `ctrl+g` and `ctrl+o` away from the product. One value for
    /// two meanings is the defect this codebase names most often, and this is that shape exactly.
    Passthrough,
    /// Close the pane.
    Close,
    /// Fetch `blast_radius` for this path and hand it back through [`Walk::measured`].
    Blast(String),
    /// Write [`Walk::dot`] somewhere and report where.
    Export,
}

/// The four replies the walk is built from, before they become rows.
///
/// ⚠️ **A REFUSAL IS CARRIED, NOT DROPPED.** All four tools read one graph through one currency
/// guard, so a refusal from any of them is a refusal about the whole pane — the reasoning
/// [`crate::production_hud::fetch`] already applies, and the reason this struct has a `withheld`
/// arm rather than four optional lists.
#[derive(Clone, Debug, Default)]
pub(crate) struct Fetched {
    pub repo: String,
    pub chokepoints: Vec<String>,
    pub core_files: Vec<String>,
    /// One line per import cycle, verbatim: `a -> b -> c`.
    pub cycles: Vec<String>,
    /// One line per component, comma-joined file lists, verbatim.
    pub subsystems: Vec<String>,
    pub withheld: Option<String>,
}

/// Rows the opened detail block takes, so the window shrinks by exactly that when `enter` is on.
const DETAIL_CHROME: usize = 22;

/// The whole pane.
pub(crate) struct Walk {
    repo: String,
    /// Every file the four replies named, deduplicated, in first-seen order. NEVER re-sorted by a
    /// filter: the filter narrows what is shown, and a list that also re-orders makes a user who
    /// clears the filter lose their place.
    nodes: Vec<Node>,
    /// The components `subsystems` reported, split into paths.
    subsystems: Vec<Vec<String>>,
    /// The chains `import_cycles` reported, verbatim.
    cycles: Vec<String>,
    /// Measured blast radii, by path. A key that is absent has NOT been asked; a key present with
    /// an empty vector was asked and nothing depends on it.
    dependents: BTreeMap<String, Vec<String>>,
    filter: String,
    filtering: bool,
    role: Option<Role>,
    /// Index into [`Self::matched`], never into [`Self::nodes`]. Every key that can shrink the
    /// match list re-clamps it, so the band cannot point past the end of a filtered pane.
    cursor: usize,
    matched: Vec<usize>,
    opened: bool,
    /// The opened row's subsystem peers and cycle chains, resolved once by [`Self::open`].
    open_subsystem: Vec<String>,
    open_cycle: Vec<String>,
    /// The last thing the pane did that the reader would not otherwise see — an export path, a
    /// blast-radius failure. Cleared by the next key so it cannot go stale on screen.
    notice: Option<String>,
    withheld: Option<String>,
}

impl Walk {
    pub(crate) fn new(fetched: Fetched) -> Self {
        let subsystems = fetched
            .subsystems
            .iter()
            .map(|line| {
                line.split(',')
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let nodes = assemble(
            &fetched.chokepoints,
            &fetched.core_files,
            &fetched.cycles,
            &subsystems,
        );
        let mut walk = Self {
            repo: fetched.repo,
            nodes,
            subsystems,
            cycles: fetched.cycles,
            dependents: BTreeMap::new(),
            filter: String::new(),
            filtering: false,
            role: None,
            cursor: 0,
            matched: Vec::new(),
            opened: false,
            open_subsystem: Vec::new(),
            open_cycle: Vec::new(),
            notice: None,
            withheld: fetched.withheld,
        };
        walk.refilter();
        walk
    }

    /// The pane as the renderer wants it, plus the borrowed detail when a row is open.
    ///
    /// ⚠️ Returned as a closure argument rather than a `Surface` because `Detail` borrows out of
    /// `self` and a `Surface` holding that borrow cannot escape this method.
    /// The pane, windowed to `height`.
    ///
    /// 🔴 **`height` IS NOT DECORATION.** A repo's graph is hundreds of files; without a window
    /// every row past the fold is unreachable by any keypress while `↓` moves a band the reader
    /// can no longer see. Found on the sibling pane by a live walk against production, and fixed
    /// in both: the same defect `skills_filtered` recorded one screen over.
    pub(crate) fn lines(
        &self,
        palette: &crate::theme::Palette,
        width: usize,
        height: usize,
        tick: u64,
        pulse: bool,
    ) -> Vec<ratatui::text::Line<'static>> {
        use crate::graph_view::Surface;
        if let Some(reason) = &self.withheld {
            return crate::graph_view::lines(
                &Surface::Withheld {
                    repo: &self.repo,
                    reason,
                },
                palette,
                width,
                tick,
                pulse,
            );
        }
        // The window follows the band, and what it leaves out is counted by `graph_view`'s
        // `matched of total` row rather than dropped.
        const CHROME: usize = 12;
        let visible = height
            .saturating_sub(CHROME + usize::from(self.opened) * DETAIL_CHROME)
            .max(1);
        let (first, count) = crate::cols::window(self.matched.len(), self.cursor, visible);
        let rows = self
            .matched
            .iter()
            .skip(first)
            .take(count)
            .filter_map(|index| self.nodes.get(*index))
            .cloned()
            .map(|node| Node {
                moves: self
                    .dependents
                    .get(&node.path)
                    .map(|files| files.len() as u64),
                ..node
            })
            .collect::<Vec<_>>();
        let detail = self.detail();
        let mut lines = crate::graph_view::lines(
            &Surface::Walk {
                repo: &self.repo,
                filter: &self.filter,
                // ⚠️ `matched` IS THE FILTER'S ANSWER, NOT THE WINDOW'S. Reporting the drawn row
                // count here would make a scrolled pane claim the filter matched twelve files.
                matched: self.matched.len(),
                total: self.nodes.len(),
                nodes: &rows,
                selected: (!rows.is_empty()).then(|| self.cursor.saturating_sub(first)),
                hints: if self.filtering { FILTER_KEYS } else { KEYS },
                detail: detail.as_ref(),
            },
            palette,
            width,
            tick,
            pulse,
        );
        if let Some(notice) = &self.notice {
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled(
                    notice.clone(),
                    ratatui::style::Style::default().fg(palette.cite),
                ),
            ]));
        }
        lines
    }

    /// The row `enter` opened, composed from replies already in hand.
    ///
    /// 🔴 **THE TWO LISTS ARE COMPUTED WHEN THE ROW OPENS, NOT WHEN THE FRAME DRAWS.** [`Detail`]
    /// borrows slices, and a frame-time computation would have to leak them to hand out a
    /// `&[String]` — once per redraw, forever, on a pane that redraws on a timer. [`Self::open`]
    /// fills the two owned fields once per `enter`; this method only borrows them.
    fn detail(&self) -> Option<Detail<'_>> {
        if !self.opened {
            return None;
        }
        let node = self.selected()?;
        Some(Detail {
            node,
            subsystem: &self.open_subsystem,
            cycle: &self.open_cycle,
            dependents: self.dependents.get(&node.path).map(Vec::as_slice),
        })
    }

    /// Open the row under the band: resolve its subsystem peers and its cycle chains, once.
    fn open(&mut self) {
        let Some(path) = self.selected_path().map(str::to_string) else {
            self.opened = false;
            return;
        };
        self.open_subsystem = self
            .subsystems
            .iter()
            .find(|component| component.contains(&path))
            .map(|component| {
                component
                    .iter()
                    .filter(|member| **member != path)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.open_cycle = self
            .cycles
            .iter()
            .filter(|chain| chain.split("->").any(|step| step.trim() == path))
            .cloned()
            .collect();
        self.opened = true;
    }

    /// Shut the opened row and drop what it was showing, so a stale neighbourhood cannot survive
    /// a move to a different file.
    fn close_row(&mut self) {
        self.opened = false;
        self.open_subsystem.clear();
        self.open_cycle.clear();
    }

    fn selected(&self) -> Option<&Node> {
        self.matched
            .get(self.cursor)
            .and_then(|index| self.nodes.get(*index))
    }

    pub(crate) fn selected_path(&self) -> Option<&str> {
        self.selected().map(|node| node.path.as_str())
    }

    /// Fold a measured blast radius back in. `files` may be empty — that is an answer.
    pub(crate) fn measured(&mut self, path: String, files: Vec<String>) {
        let count = files.len();
        self.dependents.insert(path.clone(), files);
        self.notice = Some(format!("{path}: {count} files depend on it, measured just now."));
    }

    /// The blast radius could not be taken, in the server's own words.
    pub(crate) fn refused(&mut self, path: &str, reason: String) {
        self.notice = Some(format!("{path}: no blast radius. {reason}"));
    }

    pub(crate) fn note(&mut self, notice: String) {
        self.notice = Some(notice);
    }

    /// One key. Returns what the app still has to do.
    ///
    /// 🔴 **EVERY ARM EITHER CHANGES STATE OR RETURNS AN ACTION.** A key that falls through to
    /// `Handled` without doing either is a key the footer must not advertise, and the footer is
    /// [`KEYS`], which this function is tested against by pressing it.
    pub(crate) fn key(&mut self, key: KeyEvent) -> Action {
        // A notice describes the LAST key. Any new key makes it stale, so it dies before the arm
        // that might set a new one runs.
        self.notice = None;
        if self.filtering {
            return self.filter_key(key);
        }
        // Chords belong to the app, not to the pane: ctrl+c must still cancel and ctrl+o must
        // still toggle selection while a walk is open.
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Action::Passthrough;
        }
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Up => {
                self.close_row();
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Down => {
                self.close_row();
                self.cursor = (self.cursor + 1).min(self.matched.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                if self.opened {
                    self.close_row();
                } else {
                    self.open();
                }
            }
            KeyCode::Char(' ') => {
                // Toggle, and toggle to the SELECTED row's role rather than cycling a list: the
                // effect is legible because the role is a column the reader can already see on
                // the row they are standing on.
                self.role = match (self.role, self.selected().map(|node| node.role)) {
                    (Some(_), _) => None,
                    (None, role) => role,
                };
                self.refilter();
            }
            KeyCode::Char('b') => {
                return self
                    .selected_path()
                    .map(|path| Action::Blast(path.to_string()))
                    .unwrap_or(Action::Handled);
            }
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Char('x') => return Action::Export,
            _ => {}
        }
        Action::Handled
    }

    fn filter_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filtering = false;
                self.refilter();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.refilter();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter.push(c);
                self.refilter();
            }
            // ⚠️ A CHORD IS NOT A LETTER, EVEN INSIDE A TEXT FIELD. `ctrl+t` typed into the filter
            // line is still the app's task ledger; only unmodified characters narrow the walk.
            _ if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                return Action::Passthrough;
            }
            _ => {}
        }
        Action::Handled
    }

    /// Recompute the match list and re-clamp the band.
    ///
    /// ⚠️ **THE BAND IS CLAMPED HERE AND NOWHERE ELSE.** Every key that can shrink the list calls
    /// this, so there is one place a cursor past the end can be created and one place it is fixed.
    fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.matched = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| self.role.is_none_or(|role| node.role == role))
            .filter(|(_, node)| needle.is_empty() || node.path.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        self.cursor = self.cursor.min(self.matched.len().saturating_sub(1));
        self.close_row();
    }

    /// The graph as Graphviz `dot`.
    ///
    /// 🔴 **IT CARRIES ONLY THE EDGES THE SERVER ACTUALLY RETURNED, AND SAYS SO IN THE FILE.**
    /// There are exactly two edge sources on the wire: an `import_cycles` chain, and the
    /// dependents `blast_radius` reports for a file somebody pressed `b` on. There is no general
    /// import-edge tool, so this file contains no general import edges — an export that inferred
    /// them from subsystem membership would be a picture of a graph the server never drew, handed
    /// to a reader in a format that reads as authoritative.
    ///
    /// ⚠️ The blast-radius edges are **transitive** and are labelled so. Drawing them as direct
    /// imports would overstate what was measured.
    pub(crate) fn dot(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("// estelle code graph \u{b7} {}\n", self.repo));
        out.push_str("// NODES: every file the four graph tools named (chokepoints, core_files, subsystems, import_cycles).\n");
        out.push_str("// EDGES: only the two kinds the server returns -\n");
        out.push_str("//   source=\"import_cycles\"  a reported cycle chain, direction as printed;\n");
        out.push_str("//   source=\"blast_radius\"   TRANSITIVE dependents, and only for files a walk pressed b on.\n");
        out.push_str("// NOT HERE: a general import edge list. The server exposes no tool for one, so none is invented.\n");
        out.push_str("digraph estelle {\n");
        for node in &self.nodes {
            out.push_str(&format!(
                "  {} [role={}, centrality={}];\n",
                quote(&node.path),
                quote(node.role.label()),
                quote(node.score.as_deref().unwrap_or("not measured")),
            ));
        }
        for chain in &self.cycles {
            let steps = chain
                .split("->")
                .map(str::trim)
                .filter(|step| !step.is_empty())
                .collect::<Vec<_>>();
            for pair in steps.windows(2) {
                out.push_str(&format!(
                    "  {} -> {} [source=\"import_cycles\"];\n",
                    quote(pair[0]),
                    quote(pair[1])
                ));
            }
        }
        for (path, files) in &self.dependents {
            for file in files {
                out.push_str(&format!(
                    "  {} -> {} [source=\"blast_radius\", transitive=true];\n",
                    quote(file),
                    quote(path)
                ));
            }
        }
        out.push_str("}\n");
        out
    }

    pub(crate) fn repo(&self) -> &str {
        &self.repo
    }
}

/// Escape a path for `dot`. Bounded: one pass, and the only two characters `dot` cannot take raw
/// inside a quoted id are the quote and the backslash.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// The four replies, as rows.
///
/// 🔴 **FIRST ROLE WINS, AND THE ORDER IS THE ROLE LADDER.** `Role::Core`'s own docstring says
/// *"in `core_files` but not `chokepoints`"* — that is a claim about set membership, and it is
/// true only if chokepoints are inserted first. Reversing these four loops would silently
/// re-label every file that is both.
fn assemble(
    chokepoints: &[String],
    core_files: &[String],
    cycles: &[String],
    subsystems: &[Vec<String>],
) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::new();
    let push = |line: &str, role: Role, nodes: &mut Vec<Node>| {
        let node = Node::from_tool_line(line, role);
        if node.path.is_empty() || nodes.iter().any(|seen| seen.path == node.path) {
            return;
        }
        nodes.push(node);
    };
    for line in chokepoints {
        push(line, Role::Chokepoint, &mut nodes);
    }
    for line in core_files {
        push(line, Role::Core, &mut nodes);
    }
    for chain in cycles {
        for step in chain.split("->").map(str::trim) {
            push(step, Role::Cycle, &mut nodes);
        }
    }
    for component in subsystems {
        for path in component {
            push(path, Role::Plain, &mut nodes);
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    fn fixture() -> Walk {
        Walk::new(Fetched {
            repo: "uqeu/estelle".to_string(),
            chokepoints: vec![
                "serve/api.py  (0.81)".to_string(),
                "serve/mcp.py  (0.64)".to_string(),
            ],
            core_files: vec![
                "serve/api.py  (0.81)".to_string(),
                "agent/graph_tools.py  (0.31)".to_string(),
            ],
            cycles: vec!["serve/a.py -> serve/b.py -> serve/a.py".to_string()],
            subsystems: vec![
                "serve/api.py, serve/mcp.py, serve/plans.py".to_string(),
                "agent/graph_tools.py, agent/nav.py".to_string(),
            ],
            withheld: None,
        })
    }

    fn press(walk: &mut Walk, code: KeyCode) -> Action {
        walk.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// The pane as the reader sees it, INCLUDING which row carries the band.
    ///
    /// 🔴 **THE BAND IS A BACKGROUND STYLE AND CARRIES NO TEXT, SO A TEXT-ONLY COMPARISON CANNOT
    /// SEE `↑` OR `↓` AT ALL.** The first version of this helper collected span contents and
    /// nothing else; `the_footer_advertises_exactly_the_keys_that_move_the_panel` then went red on
    /// two frames that were byte-identical as text and DIFFERENT on screen. That is the right
    /// failure for the wrong reason — the product moved and the instrument could not see it — and
    /// a helper that cannot observe the thing under test turns every assertion built on it into
    /// decoration. The marker below is the band, made legible.
    fn frame(walk: &Walk) -> String {
        walk.lines(&ScreenTheme::Dark.palette(), 130, 50, 0, false)
            .iter()
            .map(|line| {
                let band = if line.style.bg.is_some() { "\u{203a}" } else { " " };
                let text = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                format!("{band}{text}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_file_named_by_two_tools_keeps_the_stronger_role_and_appears_once() {
        let walk = fixture();
        let api = walk
            .nodes
            .iter()
            .filter(|node| node.path == "serve/api.py")
            .collect::<Vec<_>>();
        assert_eq!(api.len(), 1, "serve/api.py was listed by two tools");
        assert_eq!(
            api[0].role,
            Role::Chokepoint,
            "core_files must not demote a chokepoint - Role::Core means NOT a chokepoint"
        );
        assert_eq!(
            api[0].score.as_deref(),
            Some("(0.81)"),
            "the server's own figure, verbatim"
        );
        // Cycle members and subsystem-only members are both present, each once.
        assert!(walk.nodes.iter().any(|node| node.path == "serve/b.py"));
        assert!(walk.nodes.iter().any(|node| node.path == "agent/nav.py"));
    }

    /// 🔴 THE TEST THE BRIEF ASKS FOR: PRESS EVERY ADVERTISED KEY AND ASSERT THE PANEL MOVED.
    ///
    /// Not "a handler exists" — a handler that returns `Handled` and changes nothing satisfies
    /// any check that only looks for an arm. Each key below is pressed against a live pane and the
    /// RENDERED FRAME is compared before and after.
    #[test]
    fn the_footer_advertises_exactly_the_keys_that_move_the_panel() {
        // NEGATIVE CONTROL. A key nobody bound must leave the frame byte-identical, or every
        // assertion below passes on a pane that redraws itself differently every call.
        let mut inert = fixture();
        let before = frame(&inert);
        assert_eq!(
            press(&mut inert, KeyCode::Char('q')),
            Action::Handled,
            "an unmodified letter belongs to the pane even when the pane does nothing with it"
        );
        assert_eq!(
            frame(&inert),
            before,
            "an unbound key changed the pane: the before/after comparison proves nothing"
        );

        for (key, label) in KEYS {
            let mut walk = fixture();
            let before = frame(&walk);
            match *key {
                "\u{2191}\u{2193}" => {
                    assert_eq!(press(&mut walk, KeyCode::Down), Action::Handled);
                    assert_ne!(frame(&walk), before, "{key} {label} moved nothing");
                    let down = frame(&walk);
                    assert_eq!(press(&mut walk, KeyCode::Up), Action::Handled);
                    assert_ne!(frame(&walk), down, "up did not undo down");
                }
                "enter" => {
                    assert_eq!(press(&mut walk, KeyCode::Enter), Action::Handled);
                    let opened = frame(&walk);
                    assert_ne!(opened, before, "{key} {label} opened nothing");
                    assert!(
                        opened.contains("in the same subsystem"),
                        "enter must open the row's neighbourhood:\n{opened}"
                    );
                }
                "space" => {
                    assert_eq!(press(&mut walk, KeyCode::Char(' ')), Action::Handled);
                    assert_ne!(frame(&walk), before, "{key} {label} narrowed nothing");
                }
                "b" => {
                    assert_eq!(
                        press(&mut walk, KeyCode::Char('b')),
                        Action::Blast("serve/api.py".to_string()),
                        "b must ask for the SELECTED row's blast radius"
                    );
                }
                "/" => {
                    assert_eq!(press(&mut walk, KeyCode::Char('/')), Action::Handled);
                    let filtering = frame(&walk);
                    assert_ne!(filtering, before, "{key} {label} opened no filter line");
                    assert!(
                        filtering.contains("narrow the walk"),
                        "the filter line must say what it takes:\n{filtering}"
                    );
                }
                "x" => assert_eq!(press(&mut walk, KeyCode::Char('x')), Action::Export),
                "esc" => assert_eq!(press(&mut walk, KeyCode::Esc), Action::Close),
                other => panic!("{other} is advertised in KEYS with no test that presses it"),
            }
        }
    }

    /// `space` narrows to the role under the band, and pressing it again restores the whole walk.
    #[test]
    fn space_narrows_to_the_selected_rows_role_and_toggles_back() {
        let mut walk = fixture();
        let total = walk.matched.len();
        assert!(total > 2, "the fixture must have more than one role");
        press(&mut walk, KeyCode::Char(' '));
        assert!(
            walk.matched
                .iter()
                .all(|index| walk.nodes[*index].role == Role::Chokepoint),
            "space narrowed to something other than the selected row's role"
        );
        assert!(walk.matched.len() < total);
        press(&mut walk, KeyCode::Char(' '));
        assert_eq!(walk.matched.len(), total, "the second press did not restore");
    }

    /// The band can never point past the end of a filtered list.
    #[test]
    fn narrowing_the_walk_reclamps_the_band_instead_of_stranding_it() {
        let mut walk = fixture();
        for _ in 0..20 {
            press(&mut walk, KeyCode::Down);
        }
        assert_eq!(walk.cursor, walk.matched.len() - 1, "down ran off the end");
        press(&mut walk, KeyCode::Char('/'));
        for c in "graph_tools".chars() {
            press(&mut walk, KeyCode::Char(c));
        }
        assert_eq!(walk.matched.len(), 1, "the filter matched the wrong rows");
        assert_eq!(walk.cursor, 0, "the band was left past the end of the list");
        assert!(walk.selected_path() == Some("agent/graph_tools.py"));
    }

    /// While the filter line is open, `b` and `x` are letters, not commands.
    ///
    /// 🔴 **THE FAILURE THIS PREVENTS IS SILENT AND DESTRUCTIVE-ADJACENT**: typing `blast` into the
    /// filter would otherwise fire a request on the `b`, narrow on nothing, and write a file on
    /// the `x` in `export`. A modal pane that leaks its commands into its text field is the same
    /// defect as a hint that names an unbound key, in the other direction.
    #[test]
    fn the_filter_line_takes_letters_rather_than_firing_the_walk_keys() {
        let mut walk = fixture();
        press(&mut walk, KeyCode::Char('/'));
        for c in "bx".chars() {
            assert_eq!(
                press(&mut walk, KeyCode::Char(c)),
                Action::Handled,
                "{c} fired a command from inside the filter line"
            );
        }
        assert_eq!(walk.filter, "bx");
        // esc clears the filter and leaves the line; it does NOT close the pane.
        assert_eq!(press(&mut walk, KeyCode::Esc), Action::Handled);
        assert!(walk.filter.is_empty());
        assert!(!walk.filtering);
        // and NOW esc closes.
        assert_eq!(press(&mut walk, KeyCode::Esc), Action::Close);
    }

    /// A refusal owns the whole pane, and no row is drawn beside it.
    #[test]
    fn a_withheld_graph_draws_the_refusal_and_no_rows() {
        let walk = Walk::new(Fetched {
            repo: "uqeu/estelle".to_string(),
            withheld: Some(
                "uqeu/estelle: currency UNKNOWN - this repo has never been swept.".to_string(),
            ),
            ..Fetched::default()
        });
        let drawn = frame(&walk);
        assert!(drawn.contains("no walk from here"), "{drawn}");
        assert!(!drawn.contains("centrality"), "a table was drawn: {drawn}");
    }

    /// 🔴 THE EXPORT CARRIES NO EDGE THE SERVER DID NOT SEND.
    #[test]
    fn the_dot_export_holds_only_measured_edges_and_labels_the_transitive_ones() {
        let mut walk = fixture();
        let before = walk.dot();
        assert!(before.contains("digraph estelle"));
        assert!(
            before.contains("\"serve/a.py\" -> \"serve/b.py\" [source=\"import_cycles\"]"),
            "the reported cycle chain is missing:\n{before}"
        );
        // ⚠️ THE NEEDLE IS THE EDGE, NOT THE WORD. The file's own legend names both edge kinds at
        // the top, so `contains("blast_radius")` fires on the header of a correct export — an
        // assertion that cannot pass on the right answer is not a weaker guard, it is a wrong one.
        assert!(
            !before.contains("[source=\"blast_radius\""),
            "nothing was measured yet, so no blast-radius edge may exist:\n{before}"
        );
        // A node the graph named but for which no edge exists is still a node.
        assert!(before.contains("\"agent/nav.py\" [role=\"leaf\""));
        // 🔴 SUBSYSTEM MEMBERSHIP IS NOT AN EDGE, AND THE ASSERTION IS ON THE EDGE SET RATHER THAN
        // ON A WORD. Counting `->` lines says what is IN the file; grepping for "subsystem" says
        // only that a word is absent, and that word is in the legend of a correct export.
        let edges = before
            .lines()
            .filter(|line| line.contains(" -> "))
            .collect::<Vec<_>>();
        assert_eq!(
            edges.len(),
            2,
            "the fixture reports one 3-step cycle and nothing else, so the file holds two edges \
             and no subsystem or centrality-derived edge:\n{before}"
        );
        assert!(
            edges
                .iter()
                .all(|line| line.contains("[source=\"import_cycles\"]")),
            "an edge came from somewhere the server never reported:\n{before}"
        );

        walk.measured(
            "serve/api.py".to_string(),
            vec!["serve/mcp.py".to_string()],
        );
        let after = walk.dot();
        assert!(
            after.contains(
                "\"serve/mcp.py\" -> \"serve/api.py\" [source=\"blast_radius\", transitive=true]"
            ),
            "a measured dependent is missing or is not labelled transitive:\n{after}"
        );
    }

    /// A measured zero and an unasked row are different bytes all the way to the pane.
    #[test]
    fn an_unasked_row_and_a_measured_empty_radius_draw_different_things() {
        let mut walk = fixture();
        let unasked = frame(&walk);
        // 🔴 **MATCHED BY ROW, AND ON A ROW WHOSE SCORE IS PRESENT.** `frame.contains('—')` passed
        // over a `moves` cell printing `0`, because the fixture's cycle rows have no score and
        // their em dash is the score's. A mutant that replaced the dash with `0` survived that
        // assertion — the same hole the sibling guard in `graph_view` records having had.
        let row = unasked
            .lines()
            .find(|line| line.contains("serve/api.py"))
            .expect("the fixture's chokepoint row is missing")
            .to_string();
        assert!(row.contains("(0.81)"), "the score must be present:\n{row}");
        assert_eq!(
            row.matches('\u{2014}').count(),
            1,
            "exactly one unknown cell on this row, and it is moves:\n{row}"
        );
        walk.measured("serve/api.py".to_string(), Vec::new());
        press(&mut walk, KeyCode::Enter);
        let measured = frame(&walk);
        assert!(
            measured.contains("measured: nothing in the graph depends on this file."),
            "a measured empty radius must say it was measured:\n{measured}"
        );
        assert!(
            !measured.contains("press b to take"),
            "a measured row still advertised the key that measures it:\n{measured}"
        );
    }

    /// The same windowing property, on the graph. Found on the sibling pane by a live walk.
    #[test]
    fn a_long_walk_windows_to_the_pane_and_the_band_is_always_drawn() {
        let mut walk = Walk::new(Fetched {
            repo: "uqeu/estelle".to_string(),
            chokepoints: (0..200)
                .map(|index| format!("src/file{index:03}.rs  (0.{index:03})"))
                .collect(),
            ..Fetched::default()
        });
        let drawn = |walk: &Walk| {
            walk.lines(&ScreenTheme::Dark.palette(), 130, 30, 0, false)
                .iter()
                .map(|line| {
                    let band = if line.style.bg.is_some() { "\u{203a}" } else { " " };
                    format!(
                        "{band}{}",
                        line.spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
        };
        let top = drawn(&walk);
        assert!(top.len() <= 30, "the pane rendered {} lines into 30", top.len());
        assert!(
            top.iter().any(|line| line.contains("200 of 200 files match")),
            "the count row must report the FILTER's answer, not the window's:\n{}",
            top.join("\n")
        );
        for step in 0..220 {
            press(&mut walk, KeyCode::Down);
            let frame = drawn(&walk);
            let banded = frame
                .iter()
                .filter(|line| line.starts_with('\u{203a}'))
                .count();
            assert_eq!(banded, 1, "step {step}: {banded} banded rows");
        }
        assert!(
            drawn(&walk)
                .iter()
                .any(|line| line.contains("src/file199.rs")),
            "walking to the end did not bring the last row into view"
        );
    }

    /// The no-box rule, on this pane, in both of its states.
    #[test]
    fn no_box_corner_reaches_the_walk() {
        let mut walk = fixture();
        press(&mut walk, KeyCode::Enter);
        walk.measured("serve/api.py".to_string(), vec!["serve/mcp.py".to_string()]);
        let drawn = frame(&walk);
        for corner in ['\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{251c}', '\u{2524}',
                       '\u{252c}', '\u{2534}', '\u{253c}'] {
            assert!(!drawn.contains(corner), "{corner} in:\n{drawn}");
        }
        // POSITIVE CONTROL: the detector fires on text that really does carry one.
        assert!(format!("{drawn}\u{250c}").contains('\u{250c}'));
    }
}
