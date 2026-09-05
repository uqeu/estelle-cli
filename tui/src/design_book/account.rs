//! The three ACCOUNT screens: the provider-key picker (30), a failing doctor (36) and the frecency
//! resume list (37). Split out of `surfaces.rs` when that file passed the 800-line hard limit.
//!
//! 🔴 **NOTHING IN HERE RENDERS A CREDENTIAL, OR A FRAGMENT OF ONE.** Screen 30 shipped
//! `sk-ant-…4f2c` and `sk-…9d1a`, and screen 36 carried the same prefix in a doctor row, under a
//! footnote claiming *"Estelle prints a prefix and a state, never a value"* — as though a prefix
//! were not part of the value. The founder: *"you probably shouldn\'t dox the API key, it should
//! probably actually be hidden."* A key prefix names the vendor, the account and the key
//! generation; it is the half of a credential that is useful to somebody sorting a leak.
//!
//! ⚠️ **THE REPO ALREADY OWNED THIS RULE AND THESE TWO SCREENS WERE OUTSIDE IT.**
//! `top_level.rs::deletion_receipts_never_render_even_a_server_redacted_key_prefix` refuses
//! `estelle_live_0b95827…` — a prefix the SERVER had already elided. One surface refused what the
//! server had hidden while the surface beside it printed one in full.
//! `design_book::tests::no_book_screen_renders_a_credential_or_a_fragment_of_one` now covers every
//! screen rather than one of them.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cols::{Col, head};
use crate::design_book::kit::{bar, prose, rail, status, table};
use crate::design_book::{blank, note, owned};
use crate::marks::Mark::{Blocked, InFlight, Refused};
use crate::marks::headline;
use crate::theme::Palette;

/// `provider | how it authenticates | state | in | out | note`
///
/// 🔴 **NO CELL HERE CARRIES A FRAGMENT OF A CREDENTIAL.** The first two rows read
/// `sk-ant-…4f2c` and `sk-…9d1a`, and the footnote under them claimed *"Estelle prints a prefix
/// and a state, never a value"* — as if a prefix were not part of the value. The founder's note
/// was one sentence: *"you probably shouldn't dox the API key, it should probably actually be
/// hidden."* A key prefix identifies the account, the vendor and the key generation; it is the
/// half of a credential that is useful to somebody sorting through a leak.
///
/// ⚠️ **THIS REPO ALREADY HELD THE RULE AND THIS SCREEN CONTRADICTED IT.**
/// `top_level.rs`'s `deletion_receipts_never_render_even_a_server_redacted_key_prefix` asserts
/// that a receipt must not show `estelle_live_0b95827…` — a prefix the SERVER had already
/// redacted. One surface refused a prefix the server had elided while the surface next to it
/// printed one in full. What a reader needs is the STATE and the DATE, and both survive.
pub(crate) const PROVIDERS: &[&str] = &[
    "Anthropic | api key | on file | $5.00 | $25.00 | added 12 Aug · verified today",
    "Claude subscription | subscription import | on file | plan | plan | imported from Claude Code",
    "OpenAI API | api key | on file | $5.00 | $25.00 | added 3 Aug · verified today",
    "ChatGPT plan | oauth device | on file | plan | plan | device code · headless-safe",
    "Google Gemini | api key | no key | $1.50 | $6.00 | login gemini adds it",
    "OpenRouter | api key | on file | varies | varies | 200+ models, priced per model",
    "DeepSeek | api key | no key | $0.95 | $3.80 |",
    "GitHub Copilot | oauth device | no key | — | — | device code · no token price",
    "Azure OpenAI | api key | no key | — | — | needs an API base as well",
    "Ollama | local, none | on file | $0.00 | $0.00 | localhost:11434 · this machine",
];

/// `mark | model | in | out | this session | what it went on`
pub(crate) const COSTS: &[&str] = &[
    "landed | claude-opus-5 | $5.00 | $25.00 | $0.104 | this session's plan and solve turns",
    "landed | claude-opus-4-8 | $5.00 | $25.00 | $0.000 | pinned by you, not chosen today",
    "queued | claude-haiku-4-5 | $1.00 | $5.00 | $0.000 | affinity has not picked it here",
];

/// ⚠️ **THE SECOND LINE IS PART OF THE DEFECT, NOT A CAPTION ON IT.** It used to read *"Estelle
/// prints a prefix and a state, never a value"* — a sentence that described the old behaviour and
/// made it sound like a safeguard. A footnote left standing over a fixed screen is the cheapest
/// way to reintroduce the thing you just removed.
pub(crate) const KEY_NOTES: &[&str] = &[
    "↑↓ navigate · enter replaces · d removes · esc closes · in/out are $ per million tokens",
    "keys live in the OS keychain. Estelle prints a state and a date. No prefix, no suffix, no value.",
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
pub(crate) const CHECKS: &[&str] = &[
    "landed | endpoint | api.fatelabs.ca | 200 · 41 ms · build 1f5cc7a4, build_verified true",
    "landed | account | khai@fatelabs.ca | Free · 10M tokens · plan resolved from /plans",
    "landed | repository | uqeu/estelle · main | swept at 6ff03b18 · 5,608 symbols indexed",
    "landed | provider key | anthropic | on file · a generation returned 200 · no value printed",
    "landed | grounding gate | POST /gate | refused a fabricated import — the gate doing its job",
    "queued | local model | none | not configured — optional, and not why anything failed",
    "refused | MCP initialize | POST /mcp | 500 · the handshake was accepted, then failed inside",
];

pub(crate) const DOCTOR_WHY: &[&str] = &[
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
pub(crate) const SESSIONS: &[&str] = &[
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
