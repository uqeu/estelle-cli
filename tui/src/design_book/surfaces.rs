//! The seven SURFACE screens: login, no-repository, the command audit, shell mode, provider keys,
//! a failing doctor row, and the frecency resume list. Columns come from [`crate::cols`], colours
//! from [`crate::theme::Palette`], and no row is framed — `design_book/mod.rs` holds the contract.
//!
//! 🔴 **THE COMMAND AUDIT IS MEASURED, NOT DRAWN.** `logout` is advertised in `SESSION_HELP`
//! (`commands.rs:62`) and dropped by the resolver (`DROPPED_COMMANDS`, `commands.rs:285`), so
//! typing it prints `no command`; its handler at `main.rs:2755` is reachable only from the dispatch
//! arm at `main.rs:2472` that the resolver never produces. `SESSION_HELP` (48) + `GRAFT_HELP` (15)
//! = 63 advertised, against a popup cap of `MAX_POPUP_ROWS` (`bottom_pane/popup_consts.rs:13`) = 8.
//!
//! ⚠️ **THE ONE THING THE SPEC ASKED FOR THAT THIS FILE DOES NOT DRAW.** The doctor spec writes the
//! failing row as `✗`. `marks.rs` owns a five-glyph vocabulary and maps a sixth state onto the
//! meaning it has rather than inventing a glyph, so the failing check is [`Mark::Refused`] — `■`,
//! red. The glyph differs from the spec; the meaning and the colour do not.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Cell, Col, head, row, rule};
use crate::design_book::{blank, note, owned};
use crate::marks::Mark::{Blocked, InFlight, Landed, Queued, Refused};
use crate::marks::{Mark, StepMark, headline};
use crate::theme::Palette;

// ── what every screen below is built out of ──────────────────────────────────────────────────

/// Split one `|`-delimited catalog row into exactly `N` trimmed cells. Rows are text rather than
/// tuples because a four-field tuple with an eighty-column description is six source lines once
/// `rustfmt` has had it, and this file holds sixty of them. ⚠️ The arity is asserted in BOTH
/// directions: a row that quietly lost a field renders its note under `state` and still looks
/// like a perfectly good table.
fn fields<'a, const N: usize>(source: &'a str) -> [&'a str; N] {
    let mut cells = [""; N];
    let mut count = 0usize;
    for part in source.split('|') {
        assert!(count < N, "{source:?} carries more than {N} fields");
        cells[count] = part.trim();
        count += 1;
    }
    assert_eq!(count, N, "{source:?} carries the wrong number of fields");
    cells
}

/// Render a `|`-delimited catalog into aligned rows. `paint` is the only thing that differs
/// between the twelve tables here: it turns a row's `N` source fields into the `M` drawn cells,
/// their colours, and whether the row is the selected one. The split, the arity check, the column
/// layout, the `'static` re-own and the tint band happen here once, so no screen gets one wrong.
fn table<const N: usize, const M: usize>(
    palette: &Palette,
    spec: &[Col],
    rows: &[&'static str],
    paint: impl Fn(&Palette, usize, [&'static str; N]) -> ([&'static str; M], [Color; M], bool),
) -> Vec<Line<'static>> {
    rows.iter()
        .enumerate()
        .map(|(index, source)| {
            let (texts, inks, highlight) = paint(palette, index, fields::<N>(*source));
            let cells = texts
                .into_iter()
                .zip(inks)
                .map(|(text, ink)| Cell(text, ink))
                .collect::<Vec<_>>();
            let line = owned(row(spec, &cells, 2));
            if highlight {
                line.style(Style::default().bg(palette.tint))
            } else {
                line
            }
        })
        .collect()
}

/// `── label · mode ───…`. No corners; a rule cannot close into a panel.
fn bar(palette: &Palette, label: &str, mode: &str, wide: usize, ink: Color) -> Line<'static> {
    owned(rule(label, mode, wide, palette.dim, palette.mid, ink))
}

fn prose(palette: &Palette, text: &[&str]) -> Vec<Line<'static>> {
    text.iter().map(|line| note(palette, line)).collect()
}

/// The founder's own status line, transcribed from his 2026-08-24 capture:
/// `~/estelle · main · $0.104 · ◐ affinity`. Global rule 2 — cost and budget are always visible.
fn status(palette: &Palette, left: &str, mark: Mark, state: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {left} · "), Style::default().fg(palette.dim)),
        Span::styled(
            mark.glyph().to_string(),
            Style::default().fg(mark.colour(palette)),
        ),
        Span::styled(format!(" {state}"), Style::default().fg(palette.dim)),
    ])
}

/// The rail vocabulary by name, so a catalog row carries its own status: `(glyph, colour)`.
fn rail(palette: &Palette, name: &str) -> (&'static str, Color) {
    let mark = match name {
        "landed" => Landed,
        "blocked" => Blocked,
        "inflight" => InFlight,
        "refused" => Refused,
        _ => Queued,
    };
    (mark.glyph(), mark.colour(palette))
}

/// A plan step's `(glyph, colour, is-the-active-one)`. Only the ACTIVE step lifts its row.
fn step_of(palette: &Palette, name: &str) -> (&'static str, Color, bool) {
    let mark = StepMark::from_status(name);
    let lifted = mark.row_background(palette).is_some();
    (mark.glyph(), mark.colour(palette), lifted)
}

/// The command audit's colour law: `(glyph, colour)`. 🔴 `refused` is BLUE, not red, on the
/// founder's instruction: red would read as "this failed", and nothing failed — the name was
/// advertised and never wired.
fn verdict(palette: &Palette, kind: &str) -> (&'static str, Color) {
    match kind {
        "refused" => (Mark::Refused.glyph(), palette.cite),
        "inert" | "near-miss" => (Mark::Blocked.glyph(), palette.warn),
        "duplicate" => (Mark::Queued.glyph(), palette.dim),
        _ => (Mark::Landed.glyph(), palette.green),
    }
}

// ── 02 · login ───────────────────────────────────────────────────────────────────────────────

/// `step | stage | the question it answers | where it stands`
const LADDER: &[&str] = &[
    "done | stage 1 | who you are | khai@fatelabs.ca · device code accepted 12s ago",
    "active | stage 2 | who pays for model tokens | pick one now; the other four can wait",
];

/// `field | value | what it means`
const IDENTITY: &[&str] = &[
    "email | khai@fatelabs.ca | confirmed · your Estelle identity, never a model login",
    "device code | BDXR-4417 | accepted on this machine · expires in 9m · headless-safe",
    "Estelle plan | Free · 10M tokens | grounding only · none of it can reach a model",
];

/// `number | label | what it is` — the five real options, transcribed from the founder's own live
/// capture (`Screenshot 2026-08-24 at 5.08.18 PM.png`). These are not invented.
const OPTIONS: &[&str] = &[
    "1 | Estelle account | buys grounding: memory, code graph, recall and gate; never pays for model tokens",
    "2 | Claude subscription | imports the credential Claude Code stored on this machine · Pro, Max or Team",
    "3 | ChatGPT plan | the engine: your plan generates the answer · device code · headless-safe",
    "4 | Provider API key | Anthropic · OpenAI · Gemini · OpenRouter · DeepSeek · masked input",
    "5 | Local model | This machine · 128.0 GB RAM (111.7 GB available) · 18 CPU cores · Apple M5 Max",
];

/// `step | what you get | which stage buys it | why`
const UNLOCKS: &[&str] = &[
    "done | memory · code graph · recall · the gate | stage 1 | already working, on Free, with no model",
    "todo | grounded answers, with citations | stage 2 | needs a model credential to write the words",
    "todo | work: propose, verify, repair, PR | stage 2 | needs a model credential and a swept repo",
];

const WHY_TWO_STAGES: &[&str] = &[
    "an Estelle plan buys grounding: memory, the code graph, recall and the gate. it never pays for model tokens.",
    "the model credential stays yours — your Claude plan, your ChatGPT plan, your API key, or this machine.",
    "you can finish stage 2 later. grounding works without a model; answering does not.",
];

/// Two-stage account creation, filling all 38 rows. 🔴 The founder's note on screen 2 was *"must
/// fill the entire screen — today it is cut off halfway"*, so the vertical room is the requirement
/// rather than decoration: the ladder, the reason the two stages are separate, and what stage 2
/// unlocks all exist because the screen has room to say them and a half-height screen did not.
pub(crate) fn login(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(9), Col::l(28), Col::l(72)];
    const WHERE: &str = "stage 2 of 2 · nothing has been charged to anything yet";

    let mut lines = vec![
        bar(palette, "login", "two stages", W, palette.cite),
        blank(),
        headline(
            InFlight,
            "creating your account",
            WHERE,
            palette,
            tick,
            pulse,
        ),
        blank(),
    ];
    let rows = table::<4, 4>(palette, C, LADDER, |p, _, [step, stage, ask, at]| {
        let (glyph, ink, active) = step_of(p, step);
        (
            [glyph, stage, ask, at],
            [ink, p.dim, p.bright, p.dim],
            active,
        )
    });
    lines.extend(rows);
    lines.push(blank());
    lines.extend(login_stage_one(palette));
    lines.push(blank());
    lines.extend(login_stage_two(palette));
    lines.push(blank());
    lines.extend(login_why(palette));
    let foot = status(palette, "~/estelle · main · $0.000", Queued, "no model yet");
    lines.push(foot);
    lines
}

fn login_stage_one(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(14), Col::l(26), Col::l(68)];
    let mut lines = vec![
        bar(palette, "stage 1", "who you are", W, palette.green),
        blank(),
    ];
    let rows = table::<3, 4>(palette, C, IDENTITY, |p, _, [field, value, means]| {
        let done = StepMark::Done.glyph();
        (
            [done, field, value, means],
            [p.green, p.mid, p.bright, p.dim],
            false,
        )
    });
    lines.extend(rows);
    lines
}

fn login_stage_two(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(1), Col::l(20).gap(3), Col::l(82)];
    let head = bar(
        palette,
        "stage 2",
        "who pays for model tokens",
        W,
        palette.bright,
    );
    let mut lines = vec![head, blank()];
    let rows = table::<3, 3>(palette, C, OPTIONS, |p, index, [number, label, what]| {
        ([number, label, what], [p.bright, p.mid, p.dim], index == 0)
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(note(
        palette,
        "↑↓ navigate · 1-9 or Enter select · Esc close",
    ));
    lines
}

fn login_why(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(45), Col::l(9), Col::l(50)];
    let mut lines = vec![bar(palette, "why two stages", "", W, palette.cite), blank()];
    lines.extend(prose(palette, WHY_TWO_STAGES));
    lines.push(blank());
    lines.push(owned(head(
        C,
        &["", "what you get", "needs", "why"],
        palette.dim,
        2,
    )));
    let rows = table::<4, 4>(palette, C, UNLOCKS, |p, _, [step, gain, stage, why]| {
        let (glyph, ink, _) = step_of(p, step);
        ([glyph, gain, stage, why], [ink, p.mid, p.dim, p.dim], false)
    });
    lines.extend(rows);
    lines.push(blank());
    lines
}

// ── 06 · no repository ───────────────────────────────────────────────────────────────────────

/// `step | what you do | how`
const NEXT: &[&str] = &[
    "active | cd into a repo | cd ~/Desktop/estelle, then estelle — the graph follows the tree",
    "todo | or sweep a path | /sweep ~/Desktop/estelle indexes it and brings you back here",
    "todo | or ask account-wide | /memory and /sessions answer from your team's memory, uncited",
];

/// `mark | surfaces | what they do with no repository under them`
const DARK: &[&str] = &[
    "landed | /login /me /keys /usage | account surfaces — no repository needed",
    "landed | /sessions /resume | your history is account-scoped, so it travels with you",
    "queued | /graph /entities /verify | dark until a repository is swept — nothing to read",
    "refused | /gate /work /review | refused — no tree here to ground a diff against",
];

/// `key | value`
const FACTS: &[&str] = &[
    "cwd | ~/Downloads",
    "git | no .git in this directory or any parent",
    "swept | 0 repositories reachable from here · 3 on this account",
];

const NO_REPO_WHY: &[&str] = &[
    "memory and the code graph are PER REPOSITORY. there is no tree here to ground against,",
    "so every answer would be a guess — which is the one thing this CLI will not do.",
];

/// The SHIPPED renderer state for a cwd with no git repo — and, per global rule 5, not a dead end.
pub(crate) fn no_repository(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 116;
    const STEP: &[Col] = &[Col::l(2), Col::l(22), Col::l(76)];
    const SURF: &[Col] = &[Col::l(2), Col::l(26), Col::l(76)];
    const FACT: &[Col] = &[Col::l(7), Col::l(96)];
    const GIT: &str = "no .git in this directory or any parent";

    let mut lines = vec![
        bar(palette, "estelle", "~/Downloads", W, palette.warn),
        blank(),
        headline(Blocked, "not a git repository", GIT, palette, tick, pulse),
        blank(),
    ];
    lines.extend(prose(palette, NO_REPO_WHY));
    lines.push(blank());
    lines.push(bar(palette, "what you do next", "", W, palette.cite));
    lines.push(blank());
    let rows = table::<3, 3>(palette, STEP, NEXT, |p, _, [step, does, how]| {
        let (glyph, ink, active) = step_of(p, step);
        ([glyph, does, how], [ink, p.bright, p.dim], active)
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(bar(palette, "what still works here", "", W, palette.cite));
    lines.push(blank());
    let rows = table::<3, 3>(palette, SURF, DARK, |p, _, [name, surfaces, does]| {
        let (glyph, ink) = rail(p, name);
        ([glyph, surfaces, does], [ink, ink, p.dim], false)
    });
    lines.extend(rows);
    lines.push(blank());
    let rows = table::<2, 2>(palette, FACT, FACTS, |p, _, [key, value]| {
        ([key, value], [p.dim, p.mid], false)
    });
    lines.extend(rows);
    lines.push(blank());
    let foot = status(palette, "~/Downloads · $0.000", Queued, "no repo here");
    lines.push(foot);
    lines
}

// ── 18 · every command ───────────────────────────────────────────────────────────────────────

/// `verdict | command | what it does | state` — the ten rows the popup shows. Ten, not sixty-three,
/// and not the shipped cap of eight.
const COMMANDS: &[&str] = &[
    "live | /help | what you can do here | live",
    "live | /login | connect grounding, and the plan or key that pays for model tokens | live",
    "refused | /logout | remove local Estelle and plan credentials | advertised and refused",
    "live | /doctor | why a provider login cannot generate an answer | live",
    "inert | /keymap | composer keymap status | inert · prints a stub",
    "inert | /task | view server orchestra work | inert · prints a stub",
    "near-miss | /fork | not a command; one edit from work, and the matcher takes it | near-miss",
    "live | /tools | list every MCP tool Estelle exposes | duplicate of mcp",
    "duplicate | /mcp | byte-identical to the tools listing above | duplicate of tools",
    "duplicate | /resume | re-queues the sessions picker, nothing more | duplicate of sessions",
];

/// `verdict | label | what the colour means`
const LEGEND: &[&str] = &[
    "live | live | resolves, and does the thing its own description names",
    "refused | advertised and refused | in SESSION_HELP at commands.rs:62, dropped at commands.rs:285",
    "inert | inert · near-miss | resolves and prints a stub, or resolves to a DIFFERENT command",
    "duplicate | duplicate | a second name for a surface that already answers to one",
];

/// `verdict | names | why it is a candidate`
const CUTS: &[&str] = &[
    "refused | logout | delete the name or restore the handler — it cannot stay both",
    "inert | keymap · task | ship them or drop them; a stub is a promise the CLI does not keep",
    "duplicate | mcp · plan · permissions · resume | four names over surfaces that already have one",
];

const AUDIT_COUNTS: &[&str] = &[
    "63 advertised · 60 resolve · 2 inert · 1 advertised and refused · 4 of the 60 are a second name",
    "logout is advertised in help and dropped by the resolver; its handler at main.rs:2755 never runs.",
];

/// 🔴 THE FOUNDER'S AUDIT SCREEN. *"Does every command actually work? I don't think we need that
/// many commands."* The gap between the name and its description is a `Col` width plus a declared
/// gap, never a `{:<12}` — the shipped `/help` pads to twelve and `leaderboard`, `automations`,
/// `marketplace` and `permissions` are each exactly twelve, so those four render with ZERO space.
pub(crate) fn every_command(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(14).gap(3), Col::l(70), Col::l(22)];
    const HEADS: &[&str] = &["", "command", "what it does", "state"];
    const SCROLL: &str = "1 of 63 · ↑↓ scrolls · enter runs · esc closes · the popup ships an 8-row cap; this shows ten";

    let mut lines = vec![
        bar(palette, "commands", "63 advertised", W, palette.cite),
        blank(),
        owned(head(C, HEADS, palette.dim, 2)),
    ];
    let rows = table::<4, 4>(palette, C, COMMANDS, |p, _, [kind, name, does, state]| {
        let (_, ink) = verdict(p, kind);
        let here = kind == "refused";
        (
            [if here { "›" } else { "" }, name, does, state],
            [ink, ink, p.dim, ink],
            here,
        )
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(note(palette, SCROLL));
    lines.push(blank());
    lines.extend(command_legend(palette));
    lines.push(blank());
    lines.extend(command_cut(palette));
    let foot = status(palette, "~/estelle · main · $0.104", InFlight, "affinity");
    lines.push(foot);
    lines
}

fn command_legend(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(24), Col::l(88)];
    let mut lines = vec![bar(palette, "legend", "", W, palette.cite), blank()];
    let rows = table::<3, 3>(palette, C, LEGEND, |p, _, [kind, label, means]| {
        let (glyph, ink) = verdict(p, kind);
        ([glyph, label, means], [ink, ink, p.dim], false)
    });
    lines.extend(rows);
    lines.push(blank());
    lines.extend(prose(palette, AUDIT_COUNTS));
    lines
}

fn command_cut(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 126;
    const C: &[Col] = &[Col::l(2), Col::l(34), Col::l(80)];
    const WHY: &str = "nothing here is cut yet. this screen IS the audit, and an audit is what a cut needs first.";

    let mut lines = vec![
        bar(palette, "proposed cut", "63 to 56", W, palette.warn),
        blank(),
    ];
    let rows = table::<3, 3>(palette, C, CUTS, |p, _, [kind, names, reason]| {
        let (glyph, ink) = verdict(p, kind);
        ([glyph, names, reason], [ink, ink, p.dim], false)
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(note(palette, WHY));
    lines.push(blank());
    lines
}

// ── 19 · shell mode ──────────────────────────────────────────────────────────────────────────

/// What a shell run owes you when it finishes: `key | value | what it means`.
const SHELL_FACTS: &[&str] = &[
    "shell | /bin/zsh -lc | inherited from $SHELL, not a shell Estelle chose",
    "cwd | ~/Desktop/estelle/cli-rs | the command's cwd, not the session's repository root",
    "timeout | 120s | overridable per command: !!timeout 600 cargo test --release",
    "captured | 14 lines · 1.2 KB | held locally; /shell explains what it does not send",
];

const SHELL_STDOUT: &[&str] = &[
    "",
    "   Compiling estelle-tui v0.2.31 (/Users/khai/Desktop/estelle/cli-rs/tui)",
    "    Finished `test` profile [unoptimized + debuginfo] target(s) in 41.02s",
    "     Running unittests src/main.rs (target/debug/deps/estelle-3f0a1c9d)",
    "",
    "running 219 tests",
    "......................................................................",
    "test result: ok. 219 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out",
    "",
    "exit 0 · 47.8s elapsed · timeout 120s, never reached",
];

const SHELL_WHY: &[&str] = &[
    "this ran in your shell, not Estelle. no model call, no grounding, no memory write.",
    "Estelle did not read the output. ask it to, and that becomes a turn you can see and price.",
];

/// `!cargo test`, run locally. *"Must not look like Estelle's own output."* The distinguishing
/// mark is a `!` gutter in [`Palette::warn`] on every row — deliberately unlike the `⏺` and `›`
/// gutters Estelle uses for its own turns — with no frame anywhere near it.
pub(crate) fn shell_mode(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 116;
    const C: &[Col] = &[Col::l(10), Col::l(26), Col::l(72)];
    let gutter = || Span::styled("  ! ".to_string(), Style::default().fg(palette.warn));
    let echo = Style::default()
        .fg(palette.bright)
        .add_modifier(Modifier::BOLD);

    let typed = Span::styled("cargo test -p estelle-tui --bin estelle".to_string(), echo);
    let top = bar(palette, "shell", "your shell, not Estelle", W, palette.warn);
    let mut lines = vec![top, blank(), Line::from(vec![gutter(), typed])];
    for text in SHELL_STDOUT {
        let body = Span::styled((*text).to_string(), Style::default().fg(palette.mid));
        lines.push(Line::from(vec![gutter(), body]));
    }
    lines.push(blank());
    lines.extend(prose(palette, SHELL_WHY));
    lines.push(blank());
    let rows = table::<3, 3>(palette, C, SHELL_FACTS, |p, _, [key, value, means]| {
        ([key, value, means], [p.dim, p.bright, p.dim], false)
    });
    lines.extend(rows);
    lines.push(blank());
    let foot = status(palette, "~/cli-rs · $0.000", Landed, "exit 0 · not billed");
    lines.push(foot);
    lines
}

// ── 30 · provider keys ───────────────────────────────────────────────────────────────────────

/// `provider | how it authenticates | state | in | out | note`
const PROVIDERS: &[&str] = &[
    "Anthropic | api key | on file | $5.00 | $25.00 | sk-ant-…4f2c · added 12 Aug",
    "Claude subscription | subscription import | on file | plan | plan | imported from Claude Code",
    "OpenAI API | api key | on file | $5.00 | $25.00 | sk-…9d1a · added 3 Aug",
    "ChatGPT plan | oauth device | on file | plan | plan | device code · headless-safe",
    "Google Gemini | api key | no key | $1.50 | $6.00 | login gemini adds it",
    "OpenRouter | api key | on file | varies | varies | 200+ models, priced per model",
    "DeepSeek | api key | no key | $0.95 | $3.80 |",
    "GitHub Copilot | oauth device | no key | — | — | device code · no token price",
    "Azure OpenAI | api key | no key | — | — | needs an API base as well",
    "Ollama | local, none | on file | $0.00 | $0.00 | localhost:11434 · this machine",
];

/// `mark | model | in | out | this session | what it went on`
const COSTS: &[&str] = &[
    "landed | claude-opus-5 | $5.00 | $25.00 | $0.104 | this session's plan and solve turns",
    "landed | claude-opus-4-8 | $5.00 | $25.00 | $0.000 | pinned by you, not chosen today",
    "queued | claude-haiku-4-5 | $1.00 | $5.00 | $0.000 | affinity has not picked it here",
];

const KEY_NOTES: &[&str] = &[
    "↑↓ navigate · enter edits · d removes · esc closes · in/out are $ per million tokens",
    "keys live in your OS keychain. Estelle prints a prefix and a state, never a value.",
];

/// Ten providers, each naming **how it authenticates** and what the key costs to use. 🔴 The
/// founder's notes on the shipped capture were *"corners are wrong"* and *"the costing panel is
/// missing and I miss it most"*. Both are answered: the selection is a tint band rather than a
/// frame, and the price the key unlocks sits in the table beside the key's state.
pub(crate) fn provider_keys(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 116;
    const C: &[Col] = &[
        Col::l(2),
        Col::l(20),
        Col::l(20),
        Col::l(7),
        Col::r(7),
        Col::r(7),
        Col::l(30),
    ];
    const LABELS: &[&str] = &[
        "",
        "provider",
        "how it authenticates",
        "state",
        "in",
        "out",
        "note",
    ];

    let mut lines = vec![
        bar(palette, "providers", "10 · one pool", W, palette.cite),
        blank(),
        owned(head(C, LABELS, palette.dim, 2)),
    ];
    let rows = table::<6, 7>(palette, C, PROVIDERS, |p, index, cells| {
        let [name, auth, state, input, output, detail] = cells;
        let held = if state == "on file" { p.green } else { p.dim };
        let price = if input.starts_with('$') { p.mid } else { p.dim };
        (
            [
                if index == 0 { "›" } else { "" },
                name,
                auth,
                state,
                input,
                output,
                detail,
            ],
            [p.cite, p.mid, p.cite, held, price, price, p.dim],
            index == 0,
        )
    });
    lines.extend(rows);
    lines.push(blank());
    lines.extend(prose(palette, KEY_NOTES));
    lines.push(blank());
    lines.extend(provider_cost(palette));
    let foot = status(palette, "~/estelle · main · $0.104", InFlight, "affinity");
    lines.push(foot);
    lines
}

fn provider_cost(palette: &Palette) -> Vec<Line<'static>> {
    const W: usize = 116;
    const C: &[Col] = &[
        Col::l(2),
        Col::l(20),
        Col::r(8),
        Col::r(8),
        Col::r(12),
        Col::l(40),
    ];
    const LABELS: &[&str] = &["", "model", "in", "out", "this session", "spent on"];
    const BUDGET: &str = "plan  Free · 10M tokens · 6.1M used · 3.9M remaining     run  $0.104 this session, none on the plan";

    let mut lines = vec![
        bar(palette, "what this key costs", "anthropic", W, palette.cite),
        blank(),
        owned(head(C, LABELS, palette.dim, 2)),
    ];
    let rows = table::<6, 6>(palette, C, COSTS, |p, _, cells| {
        let [name, model, input, output, spend, went] = cells;
        let ((glyph, ink), m) = (rail(p, name), p.mid);
        (
            [glyph, model, input, output, spend, went],
            [ink, m, m, m, p.bright, p.dim],
            false,
        )
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(note(palette, BUDGET));
    lines.push(blank());
    lines
}

// ── 36 · doctor, failing ─────────────────────────────────────────────────────────────────────

/// `mark | check | surface | result`
const CHECKS: &[&str] = &[
    "landed | endpoint | api.fatelabs.ca | 200 · 41 ms · build 1f5cc7a4, build_verified true",
    "landed | account | khai@fatelabs.ca | Free · 10M tokens · plan resolved from /plans",
    "landed | repository | uqeu/estelle · main | swept at 6ff03b18 · 5,608 symbols indexed",
    "landed | provider key | anthropic | on file · sk-ant-…4f2c · a generation returned 200",
    "landed | grounding gate | POST /gate | refused a fabricated import — the gate doing its job",
    "queued | local model | none | not configured — optional, and not why anything failed",
    "refused | MCP initialize | POST /mcp | 500 · the handshake was accepted, then failed inside",
];

const DOCTOR_WHY: &[&str] = &[
    "the server accepted the handshake and then raised inside its own initialize handler.",
    "the durable error log holds the traceback; /doctor --verbose prints it here.",
];

/// `/doctor` with one failing row — and the clause the spec is built around: the last line says
/// what the failure is **NOT**. ⚠️ The passing rows are here for contrast on purpose; a screen that
/// shows only the failure makes every reader ask which of the other checks even ran.
pub(crate) fn doctor_failing(palette: &Palette, tick: u64, pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 116;
    const C: &[Col] = &[Col::l(2), Col::l(16), Col::l(22), Col::l(64)];
    const ROUND: &str = "round 1 of 3 · retrying with a fresh session id";
    const NOT: &str = "the key generated a 200 two rows up and the handshake reached the handler.";

    let mut lines = vec![
        bar(palette, "doctor", "1 of 7 failed", W, palette.red),
        blank(),
        owned(head(C, &["", "check", "surface", "result"], palette.dim, 2)),
    ];
    let rows = table::<4, 4>(palette, C, CHECKS, |p, _, [name, check, at, result]| {
        let (glyph, ink) = rail(p, name);
        (
            [glyph, check, at, result],
            [ink, p.mid, p.dim, p.dim],
            name == "refused",
        )
    });
    lines.extend(rows);
    lines.push(blank());
    const FAIL: &str = "MCP initialize returned 500";
    lines.push(headline(Refused, FAIL, ROUND, palette, tick, pulse));
    lines.push(blank());
    lines.extend(prose(palette, DOCTOR_WHY));
    lines.push(blank());
    let bold = Style::default()
        .fg(palette.warn)
        .add_modifier(Modifier::BOLD);
    let clause = Span::styled("  what this is NOT: ".to_string(), bold);
    let rest = "not your key, not your network, not your plan.".to_string();
    lines.push(Line::from(vec![
        clause,
        Span::styled(rest, Style::default().fg(palette.mid)),
    ]));
    lines.push(note(palette, NOT));
    lines.push(blank());
    let foot = status(palette, "~/estelle · main · $0.000", Blocked, "1 failing");
    lines.push(foot);
    lines
}

// ── 37 · resume session ──────────────────────────────────────────────────────────────────────

/// `rank | session | repository | when | how it ended | turns | spend`
const SESSIONS: &[&str] = &[
    "1 | the gate timeout | uqeu/estelle | 2h ago | answered | 41 | $1.82",
    "2 | memory chat, stale index | uqeu/estelle | yesterday | refused | 9 | $0.31",
    "3 | the design book in Rust | uqeu/cli-rs | yesterday | still running | 17 | $0.94",
    "4 | purge left secrets behind | uqeu/estelle | 2 days ago | answered | 63 | $2.40",
    "5 | railway variable rebuild | uqeu/estelle | 3 days ago | you closed it | 4 | $0.08",
    "6 | the affinity ledger | uqeu/estelle | 5 days ago | answered | 22 | $0.77",
    "7 | LoCoMo provenance | uqeu/estelle-bench | 12 days ago | refused | 6 | $0.19",
];

const FRECENCY: &[&str] = &[
    "ranked by FRECENCY, not recency: a session you opened six times last week outranks one you",
    "opened once an hour ago. frequency times recency, the way zoxide ranks directories.",
    "\"how it ended\" is the LAST turn's verdict, not a summary. a refused session resumes AT the",
    "refusal, with the reason still on screen, because a refusal is a step in a loop.",
];

/// A frecency-ranked session list, zoxide-style, with **how it ended** as a first-class column.
pub(crate) fn resume_session(palette: &Palette, _tick: u64, _pulse: bool) -> Vec<Line<'static>> {
    const W: usize = 116;
    const C: &[Col] = &[
        Col::l(2),
        Col::r(2),
        Col::l(28),
        Col::l(20),
        Col::l(11),
        Col::l(15),
        Col::r(5),
        Col::r(7),
    ];
    const LABELS: &[&str] = &[
        "",
        "#",
        "session",
        "repository",
        "when",
        "how it ended",
        "turns",
        "spend",
    ];

    let mut lines = vec![
        bar(palette, "sessions", "ranked by frecency", W, palette.cite),
        blank(),
        owned(head(C, LABELS, palette.dim, 2)),
    ];
    let rows = table::<7, 8>(palette, C, SESSIONS, |p, index, cells| {
        let [rank, name, repo, when, ended, turns, spend] = cells;
        let verdict = match ended {
            "answered" => p.green,
            "refused" => p.red,
            "still running" => p.cite,
            _ => p.dim,
        };
        (
            [
                if index == 0 { "›" } else { "" },
                rank,
                name,
                repo,
                when,
                ended,
                turns,
                spend,
            ],
            [p.cite, p.dim, p.mid, p.dim, p.dim, verdict, p.dim, p.mid],
            index == 0,
        )
    });
    lines.extend(rows);
    lines.push(blank());
    lines.push(note(
        palette,
        "↑↓ navigate · enter resumes · d forgets · / filters · esc closes",
    ));
    lines.push(blank());
    lines.extend(prose(palette, FRECENCY));
    lines.push(blank());
    let foot = status(palette, "~/estelle · $6.51 over 7", InFlight, "1 running");
    lines.push(foot);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_book::{BookScreen, SCREENS};
    use crate::theme::ScreenTheme;

    type Render = fn(&Palette, u64, bool) -> Vec<Line<'static>>;

    const SEVEN: &[(&str, Render)] = &[
        ("02-login-two-stage", login),
        ("06-no-repository-here", no_repository),
        ("18-every-command", every_command),
        ("19-shell-mode", shell_mode),
        ("30-provider-keys", provider_keys),
        ("36-doctor-failing", doctor_failing),
        ("37-resume-session", resume_session),
    ];

    fn text_of(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn frame_of(render: Render, palette: &Palette) -> String {
        let lines = render(palette, 0, true);
        lines.iter().map(text_of).collect::<Vec<_>>().join("\n")
    }

    fn book_entry(name: &str) -> &'static BookScreen {
        SCREENS
            .iter()
            .find(|screen| screen.name == name)
            .unwrap_or_else(|| panic!("{name} is not registered in design_book::SCREENS"))
    }

    /// 🔴 THE REPORTED DEFECT, MADE CHECKABLE. Fifteen rows in a 38-row frame satisfies every
    /// other assertion in this file, so the FLOOR is the assertion.
    #[test]
    fn the_login_screen_fills_its_frame() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let rows = login(&theme.palette(), 0, true).len();
            assert!(rows >= 30, "login filled only {rows} of 38 rows");
            assert!(rows <= 38, "login overflowed 38 rows with {rows}");
        }
    }

    /// 🔴 NO BOXES — and the positive half nobody writes. Asserting the corners are absent passes
    /// on a file that draws nothing at all, so the second half asserts the REPLACEMENT is present:
    /// at least two of the seven lift a row onto `palette.tint`. Deleting the highlight to "fix" a
    /// box fails this — proven by mutation, both ways.
    #[test]
    fn no_surface_frames_a_list() {
        const CORNERS: [char; 9] = ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];
        let palette = ScreenTheme::Dark.palette();
        let mut tinted = 0usize;
        for (name, render) in SEVEN {
            let mut has_tint = false;
            for line in render(&palette, 0, true) {
                let text = text_of(&line);
                for corner in CORNERS {
                    assert!(!text.contains(corner), "{name} drew {corner:?} in {text:?}");
                }
                has_tint |= line.style.bg == Some(palette.tint);
            }
            tinted += usize::from(has_tint);
        }
        assert!(tinted >= 2, "only {tinted} of the seven highlight a row");
    }

    /// 🔴 `/logout` IS ADVERTISED AND REFUSED, AND THE FOUNDER WANTS IT BLUE, NOT RED. Red would
    /// read as "this failed"; nothing failed, the name was never wired. The legend phrase is
    /// asserted too, because a blue row with no legend is a colour nobody can decode.
    #[test]
    fn logout_is_marked_refused_and_rendered_blue() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            let lines = every_command(&palette, 0, true);
            let spans = || lines.iter().flat_map(|line| line.spans.iter());

            let (cite, red) = (Some(palette.cite), Some(palette.red));
            let blue = spans().any(|s| s.content.contains("logout") && s.style.fg == cite);
            assert!(blue, "no span carries 'logout' in palette.cite");
            let wrong = spans().any(|s| s.content.contains("/logout") && s.style.fg == red);
            assert!(!wrong, "/logout is red, and the founder said blue");
            let frame = frame_of(every_command, &palette);
            assert!(
                frame.contains("advertised and refused"),
                "the blue has no legend"
            );
        }
    }

    /// 🔴 THE FOUNDER'S "KEEP A REAL GAP", MADE CHECKABLE. The shipped `/help` writes
    /// `format!("{surface:<12}{description}")`, and `/leaderboard`, `/automations`, `/marketplace`
    /// and `/permissions` are each exactly twelve characters, so those four render with ZERO space.
    /// The row COUNT is asserted too: a row that stopped rendering would pass by being absent.
    #[test]
    fn the_command_and_its_description_never_touch() {
        let palette = ScreenTheme::Dark.palette();
        let mut checked = 0usize;
        for line in &every_command(&palette, 0, true) {
            let text = text_of(line);
            for source in COMMANDS {
                let [_, surface, description, _] = fields::<4>(source);
                let Some(start) = text.find(surface) else {
                    continue;
                };
                let tail = &text[start + surface.len()..];
                let gap = tail.chars().take_while(|c| *c == ' ').count();
                assert!(gap >= 2, "{surface} touches {description:?} ({gap})");
                checked += 1;
            }
        }
        assert_eq!(checked, COMMANDS.len(), "not every row was checked");
    }

    /// The needle in `mod.rs` and the words on the screen are two owners of one fact.
    #[test]
    fn every_surface_renders_the_needle_its_book_entry_promises() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for (name, render) in SEVEN {
                let needle = book_entry(name).needle;
                let frame = frame_of(*render, &palette);
                assert!(frame.contains(needle), "{name} lost its needle {needle:?}");
            }
        }
    }

    /// A row wider than its frame is silently clipped by the renderer, so it fails nowhere.
    #[test]
    fn no_surface_row_overflows_its_frame_width() {
        let palette = ScreenTheme::Dark.palette();
        for (name, render) in SEVEN {
            let cap = usize::from(book_entry(name).width);
            for line in render(&palette, 0, true) {
                let drawn = text_of(&line).chars().count();
                assert!(drawn <= cap, "{name} drew {drawn} columns into {cap}");
            }
        }
    }

    /// Every catalog row carries the arity its own doc comment promises: a row that lost a `|`
    /// renders its note under `state` and still looks like a table.
    #[test]
    fn every_catalog_row_has_the_arity_its_columns_expect() {
        for source in FACTS {
            let _ = fields::<2>(source);
        }
        let threes = IDENTITY
            .iter()
            .chain(OPTIONS)
            .chain(NEXT)
            .chain(DARK)
            .chain(LEGEND)
            .chain(CUTS)
            .chain(SHELL_FACTS);
        for source in threes {
            let _ = fields::<3>(source);
        }
        for source in LADDER.iter().chain(UNLOCKS).chain(COMMANDS).chain(CHECKS) {
            let _ = fields::<4>(source);
        }
        for source in PROVIDERS.iter().chain(COSTS) {
            let _ = fields::<6>(source);
        }
        for source in SESSIONS {
            let _ = fields::<7>(source);
        }
    }
}
