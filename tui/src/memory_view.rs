//! Held memory, and the one key that changes it — design-book screen 41.
//!
//! 🔴 **THE FOUNDER'S NOTE ON THIS SCREEN: *a memory the customer cannot correct is the failure
//! that section exists to close.*** `GET /memories` has listed held memory since it was written
//! and nothing in the CLI could act on a row.
//!
//! ## What the wire carries, measured against production on 2026-09-02
//!
//! ```text
//! GET https://api.fatelabs.ca/memories?repo=uqeu/estelle
//!   -> {"memories": [{source, kind, chunks, trust, may_ground, externally_authored}],
//!       "count", "limit", "truncated", "file_pointers", "pointer_count", "repo", "scope"}
//! ```
//!
//! ⚠️ **`kind` AND `trust` ARE ON THE WIRE, AND AN EARLIER LANE'S NOTE SAYING OTHERWISE IS STALE.**
//! They arrive for asserted facts too — a `POST /fact` row comes back as
//! `{"source": "key:…", "kind": "fact", "trust": "acquired", "may_ground": false}`. So this screen
//! draws them, and it draws the SERVER's vocabulary: `grounded` / `acquired`, not the book
//! fixture's `measured` / `observed` / `asserted`, which no endpoint produces.
//!
//! 🔴 **AND TWO OF THE BOOK'S COLUMNS HAVE NO WIRE AT ALL.** `added` (a date) and `cited by` (a
//! citation count) are in the screen-41 fixture and in NO field of the reply above. They are not
//! drawn, and the pane says why rather than leaving a reader to wonder whether their memory has
//! never been cited.
//!
//! ## Why `x` is the only key here that changes anything
//!
//! `POST /retract` is the one correction verb this client can drive end to end. `edit_memory` is
//! an MCP tool taking `{key, value}` — it needs a VALUE typed by the user and a `key:`-prefixed
//! row, which is an input surface this pane does not have; it is named as missing rather than
//! half-built. `c` (citations) has no field on the wire.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use serde_json::Value;

use crate::cols::{Cell, Col, head, row};
use crate::theme::Palette;
use estelle_client::MemoryItem;

/// The keys the list binds.
pub(crate) const KEYS: &[(&str, &str)] = &[
    ("\u{2191}\u{2193}", "walk"),
    ("enter", "open"),
    ("/", "filter"),
    ("x", "retract"),
    ("esc", "close"),
];

/// The keys while the filter line is open.
pub(crate) const FILTER_KEYS: &[(&str, &str)] = &[
    ("type", "narrow the list"),
    ("backspace", "undo a letter"),
    ("enter", "keep it and walk"),
    ("esc", "clear it"),
];

/// The keys on the confirmation.
///
/// 🔴 **`enter` MEANS SOMETHING DIFFERENT HERE AND THE PANE IS A DIFFERENT PANE.** The confirm
/// state takes the WHOLE surface — no rows, no list, nothing under it — so there is no state in
/// which a reader believes they are walking a list and `enter` retracts. A confirmation drawn as a
/// footnote under a live list is a confirmation somebody hits by momentum.
pub(crate) const CONFIRM_KEYS: &[(&str, &str)] = &[
    ("enter", "retract it"),
    ("esc", "leave it alone"),
];

/// What the pane needs the app to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Handled,
    /// Not the pane's key — hand it back to the app (every chord).
    Passthrough,
    Close,
    /// The user confirmed a retraction of this subject.
    Retract(String),
}

/// The pane.
pub(crate) struct Held {
    repo: String,
    rows: Vec<MemoryItem>,
    /// Rows in THIS response, and the cap, straight from the reply.
    ///
    /// ⚠️ **A CAPPED READ REPORTS ITS CAP, NEVER A TOTAL.** `handle_memories`'s own docstring
    /// records this being wrong on prod: `count: 200` — exactly the cap — on an account holding
    /// far more, which reads as "Estelle knows 200 things about you".
    count: usize,
    limit: Option<u64>,
    truncated: bool,
    filter: String,
    filtering: bool,
    cursor: usize,
    matched: Vec<usize>,
    opened: bool,
    /// The subject awaiting a confirmed retraction.
    confirm: Option<String>,
    /// The last retraction's outcome, in the words this module derives from the receipt.
    receipt: Option<Vec<String>>,
    /// A retraction is in flight for this subject.
    pending: Option<String>,
}

impl Held {
    /// Build the pane from a `GET /memories` reply.
    pub(crate) fn new(repo: String, reply: &Value) -> Self {
        let rows = reply
            .get("memories")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value::<MemoryItem>(item.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut held = Self {
            repo,
            count: rows.len(),
            rows,
            limit: reply.get("limit").and_then(Value::as_u64),
            truncated: reply.get("truncated") == Some(&Value::Bool(true)),
            filter: String::new(),
            filtering: false,
            cursor: 0,
            matched: Vec::new(),
            opened: false,
            confirm: None,
            receipt: None,
            pending: None,
        };
        held.refilter();
        held
    }

    fn selected(&self) -> Option<&MemoryItem> {
        self.matched
            .get(self.cursor)
            .and_then(|index| self.rows.get(*index))
    }

    pub(crate) fn selected_source(&self) -> Option<&str> {
        self.selected().and_then(|item| item.source.as_deref())
    }

    /// Fold a `POST /retract` receipt back in, as sentences.
    pub(crate) fn retracted(&mut self, subject: &str, reply: &Value) {
        self.pending = None;
        self.receipt = Some(receipt_lines(subject, reply));
        // The row stays on screen. A retraction is not a delete, and a list that dropped the row
        // would be showing the customer a deletion the server did not perform.
    }

    /// The retraction did not happen, in whoever's words explained why.
    pub(crate) fn refused(&mut self, subject: &str, reason: &str) {
        self.pending = None;
        self.receipt = Some(vec![
            format!("{subject} was NOT retracted."),
            estelle_client::mask_secret(reason),
            "Nothing changed. Estelle still answers with this memory.".to_string(),
        ]);
    }

    pub(crate) fn key(&mut self, key: crossterm::event::KeyEvent) -> Action {
        use crossterm::event::{KeyCode, KeyModifiers};
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Action::Passthrough;
        }
        if let Some(subject) = self.confirm.clone() {
            return match key.code {
                KeyCode::Enter => {
                    self.confirm = None;
                    self.pending = Some(subject.clone());
                    self.receipt = None;
                    Action::Retract(subject)
                }
                // ⚠️ ANYTHING THAT IS NOT `enter` CANCELS, not just `esc`. A destructive
                // confirmation that ignores unknown keys leaves the user holding a primed pane
                // they think they dismissed.
                _ => {
                    self.confirm = None;
                    Action::Handled
                }
            };
        }
        // A receipt describes the last action. Any key moves past it.
        self.receipt = None;
        if self.filtering {
            return self.filter_key(key);
        }
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Up => {
                self.opened = false;
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Down => {
                self.opened = false;
                self.cursor = (self.cursor + 1).min(self.matched.len().saturating_sub(1));
            }
            KeyCode::Enter => self.opened = !self.opened,
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Char('x') => {
                self.confirm = self.selected_source().map(str::to_string);
            }
            _ => {}
        }
        Action::Handled
    }

    fn filter_key(&mut self, key: crossterm::event::KeyEvent) -> Action {
        use crossterm::event::{KeyCode, KeyModifiers};
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
            _ => {}
        }
        Action::Handled
    }

    fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.matched = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                needle.is_empty()
                    || item
                        .source
                        .as_deref()
                        .is_some_and(|source| source.to_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect();
        self.cursor = self.cursor.min(self.matched.len().saturating_sub(1));
        self.opened = false;
    }

    /// The pane, windowed to `height`.
    ///
    /// 🔴 **`height` IS NOT DECORATION — WITHOUT IT THIS LIST WAS A PICTURE AGAIN.** Measured
    /// against production on 2026-09-02: `/memories` returned 200 rows into a 50-line terminal and
    /// every row past the fold was unreachable by any keypress, while `↓` moved a band the reader
    /// could no longer see. The window follows the band; what is off the top and the bottom is
    /// COUNTED, never silently dropped.
    pub(crate) fn lines(
        &self,
        palette: &Palette,
        width: usize,
        height: usize,
    ) -> Vec<Line<'static>> {
        if let Some(subject) = &self.confirm {
            return confirm_lines(subject, &self.repo, palette, width);
        }
        let visible = self.visible_rows(height);
        let mut output = vec![
            crate::cols::owned(crate::cols::rule(
                "memory",
                &self.repo,
                width,
                palette.dim,
                palette.mid,
                palette.cite,
            )),
            blank(),
            Line::from(vec![
                Span::styled("  / ".to_string(), Style::default().fg(palette.cite)),
                Span::styled(
                    if self.filter.is_empty() {
                        "everything".to_string()
                    } else {
                        self.filter.clone()
                    },
                    Style::default().fg(palette.bright),
                ),
                Span::styled(self.scope_line(), Style::default().fg(palette.dim)),
            ]),
            blank(),
            head(
                &columns(width),
                &["", "source", "kind", "trust", "chunks"],
                palette.mid,
                INDENT,
            ),
        ];

        let (first, count) = crate::cols::window(self.matched.len(), self.cursor, visible);
        if first > 0 {
            output.push(note(palette, &format!("\u{2191} {first} more above")));
        }
        for (position, index) in self
            .matched
            .iter()
            .enumerate()
            .skip(first)
            .take(count)
        {
            let Some(item) = self.rows.get(*index) else {
                continue;
            };
            let trust = item.trust.as_deref().unwrap_or(UNKNOWN);
            let ink = match trust {
                "grounded" => palette.green,
                "acquired" => palette.warn,
                _ => palette.dim,
            };
            let chunks = item
                .chunks
                .map_or_else(|| UNKNOWN.to_string(), |count| count.to_string());
            let mut line = row(
                &columns(width),
                &[
                    Cell(
                        if item.externally_authored == Some(true) {
                            "\u{25b2}"
                        } else {
                            "\u{25cf}"
                        },
                        ink,
                    ),
                    Cell(item.source.as_deref().unwrap_or(UNKNOWN), palette.mid),
                    Cell(item.kind.as_deref().unwrap_or(UNKNOWN), palette.cite),
                    Cell(trust, ink),
                    Cell(&chunks, palette.dim),
                ],
                INDENT,
            );
            if position == self.cursor {
                line = line.style(Style::default().bg(palette.tint));
            }
            output.push(crate::cols::owned(line));
        }

        let below = self.matched.len().saturating_sub(first + count);
        if below > 0 {
            output.push(note(palette, &format!("\u{2193} {below} more below")));
        }
        if let Some(item) = self.opened.then(|| self.selected()).flatten() {
            output.extend(opened_lines(item, palette, width));
        }
        if let Some(subject) = &self.pending {
            output.push(blank());
            output.push(note(
                palette,
                &format!("retracting {subject} \u{2026} waiting for the receipt."),
            ));
        }
        if let Some(receipt) = &self.receipt {
            output.push(blank());
            for line in receipt {
                output.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(line.clone(), Style::default().fg(palette.bright)),
                ]));
            }
        }

        output.push(blank());
        // 🔴 **THE TWO COLUMNS THE BOOK DRAWS AND THE WIRE DOES NOT CARRY, NAMED.** Absent is not
        // zero: a blank `cited by` reads as "never cited", which is a claim about the memory
        // rather than about the endpoint.
        for footnote in [
            "trust is the server's own word: grounded is your swept code, acquired is everything else.",
            "no date and no citation count: GET /memories returns neither, so neither is drawn.",
            "retract withdraws the claim. the record that Estelle believed it survives, by design.",
        ] {
            output.push(note(palette, footnote));
        }
        output.push(note(
            palette,
            &KEYS
                .iter()
                .map(|(key, label)| format!("{key} {label}"))
                .collect::<Vec<_>>()
                .join(" \u{b7} "),
        ));
        if self.filtering {
            output.push(note(
                palette,
                &FILTER_KEYS
                    .iter()
                    .map(|(key, label)| format!("{key} {label}"))
                    .collect::<Vec<_>>()
                    .join(" \u{b7} "),
            ));
        }
        output
    }

    /// How many rows fit under the chrome.
    ///
    /// ⚠️ **THE RESERVE IS NAMED AND IT GROWS WITH THE OPENED ROW.** A fixed reserve would push
    /// the last rows off screen exactly when `enter` added six lines under them — the state a
    /// reader is in when they most want to see both.
    fn visible_rows(&self, height: usize) -> usize {
        const CHROME: usize = 13;
        const OPENED_ROW: usize = 7;
        let reserved = CHROME
            + usize::from(self.opened) * OPENED_ROW
            + usize::from(self.filtering)
            + self.receipt.as_ref().map_or(0, |lines| lines.len() + 1);
        height.saturating_sub(reserved).max(1)
    }

    /// `12 of 200 shown · capped at 200, more is held`.
    fn scope_line(&self) -> String {
        let cap = match self.limit {
            None => String::new(),
            Some(limit) if self.truncated => {
                format!(" \u{b7} capped at {limit}, more is held than this")
            }
            Some(limit) => format!(" \u{b7} cap {limit}, not reached"),
        };
        format!("   {} of {} shown{cap}", self.matched.len(), self.count)
    }
}

/// The confirmation. It owns the whole pane on purpose.
fn confirm_lines(subject: &str, repo: &str, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let mut output = vec![
        crate::cols::owned(crate::cols::rule(
            "retract",
            repo,
            width,
            palette.dim,
            palette.mid,
            palette.warn,
        )),
        blank(),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                estelle_client::mask_secret(subject),
                Style::default().fg(palette.bright),
            ),
        ]),
        blank(),
    ];
    // 🔴 **WHAT IT DOES AND WHAT IT DOES NOT DO, BOTH, BEFORE THE KEY.** Warnings come first and
    // are written as commands. The second sentence is the one a user needs in order to press the
    // key without fear, and the third is the one that stops them expecting a delete.
    for sentence in [
        "Estelle stops recalling this and stops answering it as the current belief.",
        "The record that Estelle believed it survives. This is not a delete.",
        "It applies across every namespace you own unless the scope below says otherwise.",
    ] {
        output.push(note(palette, sentence));
    }
    output.push(blank());
    output.push(note(palette, &format!("scope  {repo}")));
    output.push(blank());
    output.push(note(
        palette,
        &CONFIRM_KEYS
            .iter()
            .map(|(key, label)| format!("{key} {label}"))
            .collect::<Vec<_>>()
            .join("  \u{b7}  "),
    ));
    output.push(note(
        palette,
        "any other key leaves it alone, so a stray press cannot retract.",
    ));
    output
}

/// The opened row: every field the reply carried for it, in words.
fn opened_lines(item: &MemoryItem, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let ground = match item.may_ground {
        None => "the server did not say whether this may certify an answer".to_string(),
        Some(true) => "may certify an answer: this is your swept code".to_string(),
        Some(false) => "may NOT certify an answer: it is context, never proof".to_string(),
    };
    let authored = match item.externally_authored {
        None => "the server did not say who wrote this".to_string(),
        Some(true) => "written OUTSIDE your repo, over a channel a stranger can reach".to_string(),
        Some(false) => "not externally authored".to_string(),
    };
    vec![
        blank(),
        crate::cols::owned(crate::cols::rule(
            "open",
            item.source.as_deref().unwrap_or(UNKNOWN),
            width,
            palette.dim,
            palette.mid,
            palette.cite,
        )),
        blank(),
        note(palette, &ground),
        note(palette, &authored),
        note(
            palette,
            &format!(
                "kind {}  \u{b7}  {} chunk(s) hold it",
                item.kind.as_deref().unwrap_or(UNKNOWN),
                item.chunks
                    .map_or_else(|| UNKNOWN.to_string(), |count| count.to_string())
            ),
        ),
    ]
}

/// A `POST /retract` receipt, as sentences a customer can act on.
///
/// 🔴 **THIS FUNCTION IS THE WHOLE REASON THE KEY IS SAFE TO SHIP.** Until 2026-09-01 a secret
/// survived `/purge` in FOUR stores while every guard stayed green, because the guards could not
/// reach the stores and nothing read a receipt back. `/retract` was built with that lesson in it:
/// it returns `claim_closed` and `recall_cleared` **read back from their own stores**, plus
/// `partial` and a `warning` when either could not be confirmed. A client that reported success
/// from the absence of an HTTP error would be throwing away the only evidence there is.
///
/// ⚠️ **ABSENT IS NOT FALSE.** A missing `claim_closed` means the server did not say, and it gets
/// its own sentence. Rendering it as "not closed" would invent a failure; rendering it as closed
/// would invent a success, and that is the worse of the two.
pub(crate) fn receipt_lines(subject: &str, reply: &Value) -> Vec<String> {
    let flag = |name: &str| reply.get(name).and_then(Value::as_bool);
    let purged = reply.get("purged").and_then(Value::as_u64);
    let scope = reply
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("not reported");
    let namespaces = reply
        .get("namespaces")
        .and_then(Value::as_array)
        .map(Vec::len);

    let mut lines = Vec::new();
    // The server's own warning LEADS when it gave one. It is the actionable half and it names
    // what may still be served.
    if flag("partial") == Some(true) {
        lines.push("PARTIAL. The retraction was not confirmed in every store.".to_string());
        if let Some(warning) = reply.get("warning").and_then(Value::as_str) {
            lines.push(estelle_client::mask_secret(warning));
        }
    }

    lines.push(match flag("claim_closed") {
        Some(true) => format!("{subject} is no longer answered as the current belief."),
        Some(false) => format!("{subject} IS STILL answered as the current belief."),
        None => format!("The server did not report whether {subject}'s claim was closed."),
    });
    lines.push(match flag("recall_cleared") {
        Some(true) => "It is no longer live for search or the memory listing.".to_string(),
        Some(false) => "IT IS STILL LIVE for search and the memory listing.".to_string(),
        None => "The server did not report whether recall was cleared.".to_string(),
    });

    // ⚠️ `purged: 0` IS A TRUTHFUL ANSWER, NOT A FAILURE — the server's own docstring says so, and
    // reporting it as an error pushes a caller to retry a no-op.
    lines.push(match purged {
        None => "The server did not report how many rows it purged.".to_string(),
        Some(0) => "Nothing under this subject was live in any of your namespaces.".to_string(),
        Some(count) => format!(
            "{count} row(s) purged across {} namespace(s), scope {scope}.",
            namespaces.map_or_else(|| "an unreported number of".to_string(), |n| n.to_string())
        ),
    });
    lines.push("The record that Estelle believed it survives, and is receipted.".to_string());
    lines
}

const UNKNOWN: &str = "\u{2014}";
const MARK: usize = 2;
const KIND: usize = 12;
const TRUST: usize = 10;
const CHUNKS: usize = 7;
const GAP: usize = 2;
const INDENT: usize = 2;
const FIXED: usize = INDENT + MARK + KIND + TRUST + CHUNKS + GAP * 4;
const MIN_SOURCE: usize = 18;
const MAX_SOURCE: usize = 56;

fn columns(width: usize) -> [Col; 5] {
    let source = width
        .saturating_sub(FIXED)
        .clamp(MIN_SOURCE, MAX_SOURCE.max(MIN_SOURCE));
    [
        Col::l(MARK),
        Col::l(source),
        Col::l(KIND),
        Col::l(TRUST),
        Col::r(CHUNKS),
    ]
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    /// The shape `GET /memories?repo=uqeu/estelle` returned from production on 2026-09-02, with
    /// the sources replaced. Every key here was read off the live reply, not invented.
    fn reply() -> Value {
        json!({
            "memories": [
                {"source": "key:cream-is-e9e6dc", "kind": "fact", "trust": "acquired",
                 "may_ground": false, "externally_authored": false, "chunks": 1},
                {"source": "serve/api.py", "kind": "code", "trust": "grounded",
                 "may_ground": true, "externally_authored": false, "chunks": 42},
                {"source": "https://example.invalid/post", "kind": "doc", "trust": "acquired",
                 "may_ground": false, "externally_authored": true, "chunks": 7}
            ],
            "count": 3, "limit": 200, "truncated": false,
            "repo": "uqeu/estelle", "scope": "uqeu/estelle"
        })
    }

    fn held() -> Held {
        Held::new("uqeu/estelle".to_string(), &reply())
    }

    fn press(held: &mut Held, code: KeyCode) -> Action {
        held.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn frame(held: &Held) -> String {
        held.lines(&ScreenTheme::Dark.palette(), 130, 50)
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
    fn the_screen_draws_the_servers_vocabulary_and_omits_the_columns_it_has_no_field_for() {
        let drawn = frame(&held());
        // The server's words, not the book fixture's `measured`/`observed`/`asserted`.
        assert!(drawn.contains("grounded"), "{drawn}");
        assert!(drawn.contains("acquired"), "{drawn}");
        for invented in ["measured", "observed", "asserted"] {
            assert!(
                !drawn.contains(invented),
                "the pane drew `{invented}`, a trust tier no endpoint produces:\n{drawn}"
            );
        }
        // The two columns with no wire are absent AND explained.
        assert!(!drawn.contains("cited by"), "{drawn}");
        assert!(
            drawn.contains("no date and no citation count"),
            "an absent column must say it is absent:\n{drawn}"
        );
        // A capped read reports its cap.
        assert!(drawn.contains("3 of 3 shown"), "{drawn}");
    }

    /// 🔴 EVERY ADVERTISED KEY IS PRESSED, AND THE FRAME IS COMPARED.
    #[test]
    fn the_footer_advertises_exactly_the_keys_that_move_the_panel() {
        let mut inert = held();
        let before = frame(&inert);
        assert_eq!(press(&mut inert, KeyCode::Char('q')), Action::Handled);
        assert_eq!(frame(&inert), before, "an unbound key redrew the pane");

        for (key, label) in KEYS {
            let mut pane = held();
            let before = frame(&pane);
            match *key {
                "\u{2191}\u{2193}" => {
                    assert_eq!(press(&mut pane, KeyCode::Down), Action::Handled);
                    assert_ne!(frame(&pane), before, "{key} {label} moved nothing");
                }
                "enter" => {
                    assert_eq!(press(&mut pane, KeyCode::Enter), Action::Handled);
                    let opened = frame(&pane);
                    assert_ne!(opened, before, "{key} {label} opened nothing");
                    assert!(opened.contains("may NOT certify an answer"), "{opened}");
                }
                "/" => {
                    assert_eq!(press(&mut pane, KeyCode::Char('/')), Action::Handled);
                    assert!(frame(&pane).contains("narrow the list"), "{key} {label}");
                }
                "x" => {
                    assert_eq!(press(&mut pane, KeyCode::Char('x')), Action::Handled);
                    let confirm = frame(&pane);
                    assert!(confirm.contains("retract"), "{confirm}");
                    assert!(
                        confirm.contains("key:cream-is-e9e6dc"),
                        "the confirmation must name the exact subject:\n{confirm}"
                    );
                }
                "esc" => assert_eq!(press(&mut pane, KeyCode::Esc), Action::Close),
                other => panic!("{other} is advertised with no test that presses it"),
            }
        }
    }

    /// 🔴 **`x` NEVER RETRACTS ON ITS OWN, AND THE CONFIRMATION OWNS THE WHOLE PANE.**
    ///
    /// A confirmation drawn under a live list is one a user hits by momentum. There must be no
    /// state in which a reader believes they are walking rows and `enter` destroys something.
    #[test]
    fn a_retraction_needs_a_second_deliberate_key_and_any_other_key_cancels() {
        let mut pane = held();
        assert_eq!(press(&mut pane, KeyCode::Char('x')), Action::Handled);
        let confirm = frame(&pane);
        // No list under the confirmation: no row, no table head.
        assert!(!confirm.contains("serve/api.py"), "{confirm}");
        assert!(!confirm.contains("chunks"), "{confirm}");
        assert!(confirm.contains("This is not a delete."), "{confirm}");

        // Any key that is not enter cancels, and cancelling changes nothing.
        assert_eq!(press(&mut pane, KeyCode::Char('j')), Action::Handled);
        assert!(frame(&pane).contains("serve/api.py"), "the list did not come back");

        // And the deliberate second key is the one that acts.
        assert_eq!(press(&mut pane, KeyCode::Char('x')), Action::Handled);
        assert_eq!(
            press(&mut pane, KeyCode::Enter),
            Action::Retract("key:cream-is-e9e6dc".to_string())
        );
    }

    /// 🔴 **THE RECEIPT IS READ BACK; SUCCESS IS NEVER INFERRED FROM THE ABSENCE OF AN ERROR.**
    #[test]
    fn a_partial_retraction_leads_with_the_warning_and_never_reads_as_done() {
        let full = receipt_lines(
            "key:x",
            &json!({"retracted": true, "purged": 2, "namespaces": ["a", "b"],
                    "scope": "uqeu/estelle", "claim_closed": true, "recall_cleared": true}),
        );
        assert!(full.iter().any(|line| line.contains("no longer answered")));
        assert!(full.iter().any(|line| line.contains("no longer live")));
        assert!(full.iter().any(|line| line.contains("2 row(s) purged")));
        assert!(!full.iter().any(|line| line.contains("PARTIAL")));

        let partial = receipt_lines(
            "key:x",
            &json!({"retracted": true, "purged": 1, "namespaces": ["a"], "scope": "all-namespaces",
                    "claim_closed": true, "recall_cleared": false, "partial": true,
                    "warning": "recall could not be confirmed; /search may still return it"}),
        );
        assert!(partial[0].starts_with("PARTIAL"), "{partial:?}");
        assert!(
            partial[1].contains("/search may still return it"),
            "the server's warning must be carried verbatim: {partial:?}"
        );
        assert!(
            partial.iter().any(|line| line.contains("IT IS STILL LIVE")),
            "a false recall_cleared must be shouted, not softened: {partial:?}"
        );

        // 🔴 ABSENT IS NOT FALSE, AND IT IS NOT TRUE EITHER.
        let silent = receipt_lines("key:x", &json!({"retracted": true, "purged": 1}));
        assert!(
            silent
                .iter()
                .any(|line| line.contains("did not report whether key:x's claim was closed")),
            "{silent:?}"
        );
        assert!(
            silent
                .iter()
                .any(|line| line.contains("did not report whether recall was cleared")),
            "{silent:?}"
        );
        assert!(
            !silent.iter().any(|line| line.contains("no longer answered")),
            "an unreported store was rendered as a success: {silent:?}"
        );

        // `purged: 0` is truthful, not a failure.
        let nothing = receipt_lines(
            "key:x",
            &json!({"retracted": true, "purged": 0, "claim_closed": true,
                    "recall_cleared": true, "namespaces": []}),
        );
        assert!(
            nothing
                .iter()
                .any(|line| line.contains("Nothing under this subject was live")),
            "{nothing:?}"
        );
    }

    /// A refusal changes nothing and says so, in the server's words.
    #[test]
    fn a_refused_retraction_says_nothing_changed() {
        let mut pane = held();
        pane.refused("key:x", "Estelle returned HTTP 403: manage_team required");
        let drawn = frame(&pane);
        assert!(drawn.contains("was NOT retracted"), "{drawn}");
        assert!(drawn.contains("manage_team required"), "{drawn}");
        assert!(
            drawn.contains("Estelle still answers with this memory"),
            "a refusal must say the memory is still in force:\n{drawn}"
        );
    }

    /// The row stays on screen after a retraction, because a retraction is not a delete.
    #[test]
    fn a_retracted_row_is_not_removed_from_the_listing() {
        let mut pane = held();
        pane.retracted(
            "key:cream-is-e9e6dc",
            &json!({"retracted": true, "purged": 1, "claim_closed": true,
                    "recall_cleared": true, "namespaces": ["a"], "scope": "uqeu/estelle"}),
        );
        let drawn = frame(&pane);
        // 🔴 **THE ASSERTION IS ON THE ROW, NOT ON THE FRAME.** The RECEIPT names the subject too
        // ("key:… is no longer answered as the current belief"), so `frame.contains(subject)`
        // passed with the row deleted — the needle was in the wrong place, which is the inert
        // guard this repo pays for most. A row is the line that also carries its cells.
        let row = drawn
            .lines()
            .find(|line| line.contains("key:cream-is-e9e6dc") && line.contains("acquired"))
            .unwrap_or_default();
        assert!(
            row.contains("fact"),
            "the row vanished, which claims a delete the server did not do:\n{drawn}"
        );
        assert!(drawn.contains("no longer answered"), "{drawn}");
    }

    /// 🔴 **THE DEFECT A LIVE WALK FOUND AND EVERY UNIT TEST MISSED.**
    ///
    /// Against production on 2026-09-02 this pane drew 200 rows into a 50-line terminal. `↓`
    /// moved the band correctly and the band was off the screen, so every row past the fold was
    /// unreachable by any keypress — a walkable list that is a picture from row 30 down. The
    /// fixture here is the same shape: 200 rows, a short pane, and the band walked to the end.
    #[test]
    fn a_long_list_windows_to_the_pane_and_the_band_is_always_drawn() {
        let rows = (0..200)
            .map(|index| {
                json!({"source": format!("src/file{index:03}.rs"), "kind": "code",
                       "trust": "grounded", "may_ground": true,
                       "externally_authored": false, "chunks": 1})
            })
            .collect::<Vec<_>>();
        let mut pane = Held::new(
            "uqeu/estelle".to_string(),
            &json!({"memories": rows, "count": 200, "limit": 200, "truncated": true}),
        );

        let drawn = |pane: &Held| {
            pane.lines(&ScreenTheme::Dark.palette(), 130, 30)
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

        // The pane fits: it draws fewer rows than the list holds, and says how many it left out.
        let top = drawn(&pane);
        let file_rows = top.iter().filter(|line| line.contains("src/file")).count();
        assert!(file_rows < 200, "the pane drew {file_rows} rows into 30 lines");
        assert!(top.len() <= 30, "the pane rendered {} lines into 30", top.len());
        assert!(
            top.iter().any(|line| line.contains("more below")),
            "the rows below the fold were dropped without a count:\n{}",
            top.join("\n")
        );

        // 🔴 THE BAND IS ON SCREEN AT EVERY STEP, INCLUDING THE LAST ROW.
        for step in 0..220 {
            press(&mut pane, KeyCode::Down);
            let frame = drawn(&pane);
            let banded = frame
                .iter()
                .filter(|line| line.starts_with('\u{203a}'))
                .count();
            assert_eq!(banded, 1, "step {step}: {banded} banded rows:\n{}", frame.join("\n"));
        }
        let last = drawn(&pane);
        assert!(
            last.iter().any(|line| line.contains("src/file199.rs")),
            "walking to the end did not bring the last row into view:\n{}",
            last.join("\n")
        );
        assert!(
            last.iter().any(|line| line.contains("more above")),
            "what scrolled off the top was dropped without a count:\n{}",
            last.join("\n")
        );
        // And the count line still reports the FILTER's answer, not the window's.
        assert!(
            last.iter().any(|line| line.contains("200 of 200 shown")),
            "the scrolled pane reported the window as the match count:\n{}",
            last.join("\n")
        );
    }

    #[test]
    fn no_box_corner_reaches_this_pane() {
        let mut pane = held();
        press(&mut pane, KeyCode::Enter);
        let listed = frame(&pane);
        press(&mut pane, KeyCode::Enter);
        press(&mut pane, KeyCode::Char('x'));
        let confirming = frame(&pane);
        for drawn in [listed, confirming] {
            for corner in [
                '\u{250c}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{251c}', '\u{2524}', '\u{252c}',
                '\u{2534}', '\u{253c}',
            ] {
                assert!(!drawn.contains(corner), "{corner} in:\n{drawn}");
            }
            // POSITIVE CONTROL: the detector fires on text that carries one.
            assert!(format!("{drawn}\u{250c}").contains('\u{250c}'));
        }
    }
}
