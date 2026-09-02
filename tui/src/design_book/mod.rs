//! The design book's screens, rendered by the REAL renderer.
//!
//! 🔴 **WHY THIS MODULE EXISTS.** The founder reviewed `CLI-DESIGN-BOOK.html` screen by screen and
//! asked one thing of the next pass: *"Is this rendered in Rust or JavaScript? I want you to render
//! all of this now in Rust, so that it's easier for you to port these over."* Twenty-five of the
//! forty-one screens already came out of the production renderer. The other sixteen — and seven
//! SHIPPED renderer states that had no gallery frame — were HTML drawn by hand, which means their
//! columns were spaces somebody counted rather than a layout anything computed.
//!
//! ⚠️ **A HAND-PLACED SPACE IS A LAYOUT CLAIM NOBODY CAN FALSIFY.** That is the whole defect this
//! module closes. Every screen here builds its rows through [`crate::cols`] — the module whose four
//! tests exist because of four real alignment bugs, including `⏺` being three bytes and one column
//! — so a row that does not line up is a test failure rather than a thing a reader notices in a
//! screenshot six weeks later.
//!
//! ## The contract every screen in here keeps
//!
//! 1. **Columns come from [`crate::cols`].** `Col::l`/`Col::r`/`row`/`head`/`rule`. Indentation is
//!    the `indent` argument, never a padded string.
//! 2. **Colours come from [`crate::theme::Palette`].** No `Color::Rgb`, no ANSI `Color::Blue` —
//!    the gallery's SVG maps bare ANSI to `#65A8FF`/`#70C6CC`, two values that are in no token.
//! 3. 🔴 **NO BOXES.** Not one of `┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`. The selected row is highlighted with
//!    `palette.tint`; a list is never framed. The gallery asserts this over every frame, and the
//!    founder said it three times in one review.
//! 4. **Every screen declares a `needle`** — text the rendered buffer must contain. A frame that
//!    renders blank is otherwise a passing frame.

use ratatui::text::Line;

use crate::theme::Palette;

pub(crate) mod account;
pub(crate) mod answers;
pub(crate) mod costing;
pub(crate) mod kit;
pub(crate) mod loops;
pub(crate) mod panel;
pub(crate) mod panes;
/// The three films, as data. Reorder a beat here and nowhere else.
/// The production rail, moving.
pub(crate) mod rail;
pub(crate) mod script;
/// Film 2 · Cartwheel, as data.
pub(crate) mod script_cartwheel;
/// Film 3 · this repo, as data.
pub(crate) mod script_estelle;
/// Film 1 · a solo developer and his own machine, as data.
pub(crate) mod script_solo;
/// The vocabulary a scripted session is written in.
pub(crate) mod session;
pub(crate) mod skills;
pub(crate) mod stats;
pub(crate) mod surfaces;

/// One book screen: what to call it, how big it is, and the string that proves it rendered.
pub(crate) struct BookScreen {
    /// The gallery frame name. Prefixed with the book's screen number so the book and the gallery
    /// sort together and a reader can find one from the other without a lookup table.
    pub name: &'static str,
    /// 🔴 **WHAT THIS SCREEN NEEDS BEFORE IT CAN RENDER FROM REALITY.** Written down per screen so
    /// the honest empty state can NAME the gap instead of saying "no data", and so the list of
    /// un-wired contracts is derivable from the code rather than from somebody's memory. `shipped ·
    /// …` means live state already exists and the fixture only stages it on demand.
    pub contract: &'static str,
    pub width: u16,
    pub height: u16,
    /// 🔴 THE VACUITY GUARD. `write_frame` will happily write an empty buffer, and an empty frame
    /// reads exactly like a rendered one in a directory listing. Asserting a needle is the cheapest
    /// thing that cannot pass on a screen that drew nothing.
    #[cfg_attr(not(test), allow(dead_code, reason = "the gallery's vacuity guard"))]
    pub needle: &'static str,
    pub render: fn(&Palette, u64, bool) -> Vec<Line<'static>>,
}

/// 🔴 **THE ONE OWNER OF "MAY FIXTURE DATA BE DRAWN".**
///
/// The founder's instruction was *"you fake the tool call and all that stuff in the demo, because
/// we just have to send this to them"* — and the constraint that comes with it is not negotiable:
/// **a demo frame that silently shows invented numbers as if measured is the exact failure this
/// company exists to prevent.** So the fixtures live behind one switch, off unless somebody set it
/// for this process, and every path that could draw them asks THIS function.
///
/// ⚠️ It is a function rather than a `bool` threaded through the call graph because a threaded
/// flag acquires a second reader with a different default within a week, and then there are two
/// answers to "is this real". One owner per derived fact.
pub(crate) const FIXTURE_ENV: &str = "ESTELLE_DEMO_FIXTURES";

/// `true` only when the operator asked for fixtures — the env var set to `1`, or `--demo` passed.
///
/// ⚠️ **`is_some` WOULD HAVE BEEN WRONG.** An env var exported as `0` or left empty by a shell
/// script is a variable that is SET, and a gate that opens on presence rather than on value opens
/// on `ESTELLE_DEMO_FIXTURES=0`. The value is compared.
pub(crate) fn fixtures_allowed(flag: bool) -> bool {
    flag || std::env::var_os(FIXTURE_ENV).is_some_and(|value| value == "1")
}

/// Render one screen, or — with the gate shut — the honest empty state that names what is missing.
///
/// 🔴 **THIS IS THE ONLY FUNCTION THE PRODUCT CALLS.** `screen.render` draws fixture data
/// unconditionally and always will; keeping it private to this module means a caller cannot reach
/// invented numbers by forgetting a flag, only by asking for them by name.
pub(crate) fn render(
    screen: &BookScreen,
    palette: &Palette,
    tick: u64,
    pulse: bool,
    fixtures: bool,
) -> Vec<Line<'static>> {
    if fixtures {
        return (screen.render)(palette, tick, pulse);
    }
    empty_state(screen, palette)
}

/// What a fixture screen shows when nobody asked for fixtures: no numbers, and the reason.
///
/// ⚠️ It states the CONTRACT, not "no data". *"Nothing measured"* over a screen whose data source
/// does not exist yet reads as a transient failure a reader will retry; naming the missing endpoint
/// is the difference between an empty state and an honest one.
fn empty_state(screen: &BookScreen, palette: &Palette) -> Vec<Line<'static>> {
    use ratatui::style::Style;
    use ratatui::text::Span;

    let heading = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(screen.name.to_string(), Style::default().fg(palette.bright)),
        Span::styled(
            "  ·  nothing measured".to_string(),
            Style::default().fg(palette.dim),
        ),
    ]);
    vec![
        heading,
        blank(),
        note(palette, "no live data reaches this screen yet."),
        note(
            palette,
            &format!("needs   {contract}", contract = screen.contract),
        ),
        blank(),
        note(
            palette,
            &format!(
                "draw it from the design fixture with  {FIXTURE_ENV}=1 estelle demo {name}",
                name = screen.name
            ),
        ),
        note(
            palette,
            "the LAYOUT is production. the DATA under that flag is not measured.",
        ),
    ]
}

/// Every screen the live app cannot produce from its own state, in book order.
pub(crate) const SCREENS: &[BookScreen] = &[
    BookScreen {
        name: "02-login-two-stage",
        contract: "shipped · login::run stages these one at a time",
        width: 130,
        height: 38,
        needle: "who pays for model tokens",
        render: surfaces::login,
    },
    BookScreen {
        name: "06-no-repository-here",
        contract: "shipped · live_renderer.rs:997",
        width: 120,
        height: 30,
        needle: "not a git repository",
        render: surfaces::no_repository,
    },
    BookScreen {
        name: "09-gate-refused",
        contract: "shipped · gate_refusal::render_gate_modal",
        width: 120,
        height: 30,
        needle: "Gate refused",
        render: loops::gate_refused,
    },
    BookScreen {
        name: "10-navigation-stale",
        contract: "shipped · transcript.rs renders memory_chat's code_currency block",
        width: 120,
        height: 28,
        needle: "indexed at",
        render: loops::navigation_stale,
    },
    BookScreen {
        name: "11-compaction-refused",
        contract: "refusal read by compaction_view · no split plan or per-part tokens",
        width: 120,
        height: 26,
        needle: "one message",
        render: loops::compaction_refused,
    },
    BookScreen {
        name: "12-skills-typed",
        contract: "shipped · the popup does not preload on enter yet",
        width: 120,
        height: 30,
        needle: "skill:",
        render: skills::typed,
    },
    BookScreen {
        name: "13-skills-offered",
        contract: "the offer does not fire on send yet",
        width: 120,
        height: 30,
        needle: "send with the skill",
        render: skills::offered,
    },
    BookScreen {
        name: "14-skills-browse",
        contract: "no per-skill token cost on the wire",
        width: 120,
        height: 34,
        needle: "max compose",
        render: skills::browse,
    },
    BookScreen {
        name: "18-every-command",
        contract: "shipped · help_lines()",
        width: 130,
        height: 40,
        needle: "advertised and refused",
        render: surfaces::every_command,
    },
    BookScreen {
        name: "19-shell-mode",
        contract: "shipped · !cmd",
        width: 120,
        height: 30,
        needle: "your shell, not Estelle",
        render: surfaces::shell_mode,
    },
    BookScreen {
        name: "25-panels-one-terminal",
        contract: crate::orchestra_view::MISSING_PER_WORKER_SPEND,
        width: 180,
        height: 34,
        needle: "tab strip",
        render: panes::panels,
    },
    BookScreen {
        name: "30-provider-keys",
        contract: "shipped · provider_catalog",
        width: 120,
        height: 34,
        needle: "how it authenticates",
        render: account::provider_keys,
    },
    BookScreen {
        name: "32-memory-remaining",
        contract: "shipped · sweep_estimate::estimate_panel, drawn by the ctrl+s pane",
        width: 130,
        height: 36,
        needle: "largest paths",
        render: costing::memory_remaining,
    },
    BookScreen {
        name: "33-usage-spend",
        contract: "no per-session spend total on the wire",
        width: 130,
        height: 34,
        needle: "this session you spent",
        render: costing::usage_spend,
    },
    BookScreen {
        name: "33b-model-cost",
        contract: "no per-model cost breakdown on the wire",
        width: 130,
        height: 34,
        needle: "run spend",
        render: panel::model_cost,
    },
    BookScreen {
        name: "34-answer-table-diagram",
        contract: "markdown tables render · no diagram renderer, the fence prints source",
        width: 120,
        height: 36,
        needle: "mermaid",
        render: answers::table_and_diagram,
    },
    BookScreen {
        name: "35-session-tabs",
        contract: "shipped · live_renderer.rs:114",
        width: 140,
        height: 30,
        needle: "sessions",
        render: panes::session_tabs,
    },
    BookScreen {
        name: "36-doctor-failing",
        contract: "shipped · doctor::lines_with_binding",
        width: 120,
        height: 30,
        needle: "what this is NOT",
        render: account::doctor_failing,
    },
    BookScreen {
        name: "37-resume-session",
        contract: "shipped · resume_picker",
        width: 120,
        height: 30,
        needle: "how it ended",
        render: account::resume_session,
    },
    BookScreen {
        name: "38-sweep-running",
        contract: "shipped · top_level::sweep_with_progress",
        width: 120,
        height: 28,
        needle: "checking account capacity",
        render: panes::sweep_running,
    },
    BookScreen {
        name: "39-tool-calls",
        contract: "collapsed/expanded exists (transcript.rs:44); only shell fills it",
        width: 120,
        height: 34,
        needle: "lines hidden",
        render: answers::tool_calls,
    },
    BookScreen {
        name: "40-code-graph",
        contract: "shipped · graph_view · files not symbols, no keys to walk it",
        width: 130,
        height: 32,
        needle: "chokepoint",
        render: answers::code_graph,
    },
    BookScreen {
        name: "42-stats-activity",
        contract: "no per-phase clock and no per-tool tally in this client",
        width: 130,
        height: 48,
        needle: "where the 3h02m went",
        render: stats::stats_activity,
    },
    BookScreen {
        name: "41-memory-correct",
        contract: "edit_memory + POST /facts exist; no CLI surface, and no kind/trust",
        width: 130,
        height: 32,
        needle: "supersedes",
        render: answers::memory_correct,
    },
];

/// Re-own a `Line` whose spans borrow local `String`s.
///
/// ⚠️ **MOVED TO [`crate::cols::owned`], BESIDE THE FUNCTION THAT CREATES THE BORROW.** It was
/// written here, which meant a PRODUCTION renderer computing its own cells had to depend on the
/// DESIGN BOOK to escape a lifetime `cols` had introduced. This alias keeps the book's call sites
/// reading as they did; the implementation has one owner and it is not this module.
pub(crate) fn owned(line: Line<'_>) -> Line<'static> {
    crate::cols::owned(line)
}

/// A blank row. Named so a screen never reaches for `Line::from("")` and loses its palette.
pub(crate) fn blank() -> Line<'static> {
    Line::from("")
}

/// One dim line of prose, indented two, the way every catalog screen writes a footnote.
pub(crate) fn note(palette: &Palette, text: &str) -> Line<'static> {
    Line::from(ratatui::text::Span::styled(
        format!("  {text}"),
        ratatui::style::Style::default().fg(palette.dim),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;

    /// 🔴 **FIXTURE DATA CANNOT REACH A DEFAULT-CONFIGURATION RUN.**
    ///
    /// This is the whole of the founder's one hard constraint, expressed as an assertion. Every
    /// screen declares a `needle` — a string that only exists in its FIXTURE — and with the gate
    /// shut none of those needles appears in what [`render`] returns. A screen that started
    /// leaking its fixture through a new code path fires here by name.
    ///
    /// ⚠️ **THE POSITIVE CONTROL IS THE SECOND HALF, AND IT IS NOT DECORATION.** An assertion that
    /// a set of needles is absent passes identically over a renderer that returns nothing at all —
    /// the vacuity shape this repo has paid for repeatedly. So the same needles are asserted
    /// PRESENT with the gate open, in the same loop, over the same screens.
    #[test]
    fn fixture_data_cannot_reach_a_default_configuration_run() {
        let palette = ScreenTheme::Dark.palette();
        for screen in SCREENS {
            let shut: String = render(screen, &palette, 0, true, false)
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
                .collect();
            assert!(
                !shut.contains(screen.needle),
                "{} leaked its fixture with the gate shut: {shut:?}",
                screen.name
            );
            assert!(
                shut.contains(screen.contract),
                "{} shut but did not name what it needs: {shut:?}",
                screen.name
            );

            let open: String = render(screen, &palette, 0, true, true)
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
                .collect();
            assert!(
                open.contains(screen.needle),
                "{} drew nothing with the gate OPEN — the check above proves nothing",
                screen.name
            );
        }
    }

    /// 🔴 **THE GATE OPENS ON A VALUE, NEVER ON PRESENCE.**
    ///
    /// `var_os(..).is_some()` is the reflex, and it opens on `ESTELLE_DEMO_FIXTURES=0` and on the
    /// empty string a shell script leaves behind — a switch whose "off" position turns it on. The
    /// `--demo` flag is the second door and is asserted separately, because a gate with two inputs
    /// and one test is a gate with an untested input.
    ///
    /// ⚠️ Env vars are process-global and this test sets one. It is `#[serial]`-free because it
    /// restores what it found; if this file ever gains a second env-touching test they must be
    /// made mutually exclusive rather than both trusted.
    #[test]
    fn the_fixture_gate_opens_on_one_and_nothing_else() {
        let restore = std::env::var_os(FIXTURE_ENV);
        // SAFETY-ish: single-threaded within this test, and the original value is put back below.
        unsafe {
            std::env::remove_var(FIXTURE_ENV);
            assert!(!fixtures_allowed(false), "shut by default");
            assert!(fixtures_allowed(true), "--demo opens it");

            std::env::set_var(FIXTURE_ENV, "0");
            assert!(!fixtures_allowed(false), "\"0\" is off, not \"set\"");
            std::env::set_var(FIXTURE_ENV, "");
            assert!(!fixtures_allowed(false), "empty is off, not \"set\"");
            std::env::set_var(FIXTURE_ENV, "true");
            assert!(!fixtures_allowed(false), "only \"1\" opens it");
            std::env::set_var(FIXTURE_ENV, "1");
            assert!(fixtures_allowed(false), "\"1\" opens it");

            match restore {
                Some(value) => std::env::set_var(FIXTURE_ENV, value),
                None => std::env::remove_var(FIXTURE_ENV),
            }
        }
    }

    /// Every screen says what it still needs, and no two say it in a way that reads as measured.
    #[test]
    fn every_screen_names_the_contract_it_still_needs() {
        for screen in SCREENS {
            assert!(
                !screen.contract.is_empty(),
                "{} declares no contract",
                screen.name
            );
            // 🔴 THE GUARD ABOVE CAN BE DEFEATED BY WORDING, AND WAS, ON ITS FIRST RUN.
            // Screen 34's needle is "mermaid" and its contract said "…or mermaid yet", so the
            // EMPTY state contained the needle and the leak check fired on a screen that had not
            // leaked. A needle that also appears in the honest state is a needle that can no
            // longer tell the two apart in either direction.
            assert!(
                !screen.contract.contains(screen.needle),
                "{}'s contract quotes its own needle {:?} — the leak guard cannot see past it",
                screen.name,
                screen.needle
            );
            assert!(
                screen.contract.len() < 74,
                "{}'s contract does not fit the listing column: {:?}",
                screen.name,
                screen.contract
            );
        }
    }

    /// 🔴 A NAME COLLISION WOULD SILENTLY OVERWRITE A FRAME ON DISK.
    ///
    /// `write_frame` writes `{name}.txt`; two screens sharing a name means the book loses one and
    /// the gallery index lists a file whose content belongs to a different screen. Nothing else
    /// would go red.
    #[test]
    fn every_book_screen_has_a_unique_name() {
        let names = SCREENS
            .iter()
            .map(|screen| screen.name)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), SCREENS.len(), "two book screens share a name");
    }

    /// Every screen renders non-empty in BOTH palettes. A screen that only works on the dark
    /// ground is a screen the cream reader cannot use, and the founder reads cream.
    #[test]
    fn every_book_screen_renders_in_both_themes() {
        for theme in [ScreenTheme::Dark, ScreenTheme::Cream] {
            let palette = theme.palette();
            for screen in SCREENS {
                let lines = (screen.render)(&palette, 0, true);
                assert!(!lines.is_empty(), "{} rendered nothing", screen.name);
                assert!(
                    lines.len() <= usize::from(screen.height),
                    "{} rendered {} rows into a {}-row frame",
                    screen.name,
                    lines.len(),
                    screen.height
                );
            }
        }
    }

    /// 🔴 **A DESIGN FRAME MAY NOT TRUNCATE ITS OWN COPY.**
    ///
    /// `cols` ends an overlong cell with `…`, which is correct on a live screen — a model name has
    /// to fit and the reader can widen the terminal. On a BOOK frame it is a defect: the founder is
    /// reviewing the words, and a word he cannot read is a word he cannot rule on. The frame widths
    /// in [`SCREENS`] are ours to choose, so an ellipsis here means a column was sized wrong, not
    /// that the content was too long. Three cells were truncated when this was written —
    /// `affinity · cost p…`, `this machine, no …`, `$45 soft…` — all on the costing panel, all
    /// invisible until somebody read the generated book instead of the test output.
    ///
    /// ⚠️ **IT ASSERTS ON A SPAN ENDING IN `…`, NOT ON A LINE CONTAINING ONE, AND THAT DISTINCTION
    /// IS THE WHOLE TEST.** The first version searched the joined line and fired on
    /// `sk-ant-…4f2c` — a deliberately MASKED API key on the provider screen, where the ellipsis is
    /// the mask and eliding it is the point. `cols::truncate` builds `take(width - 1)` + `…`, so a
    /// truncated cell is always a span whose LAST character is the ellipsis; a mask never is. A
    /// guard that cannot tell those two apart gets suppressed within a week, and a suppressed guard
    /// is worth less than none.
    #[test]
    fn no_book_screen_truncates_its_own_copy() {
        let palette = ScreenTheme::Dark.palette();
        let mut checked = 0_usize;
        for screen in SCREENS {
            for line in (screen.render)(&palette, 0, true) {
                for span in &line.spans {
                    checked += 1;
                    assert!(
                        !span.content.ends_with('\u{2026}'),
                        "{} truncated a cell — widen the column, do not shorten the words: {:?}",
                        screen.name,
                        span.content
                    );
                }
            }
        }
        // ⚠️ A guard that iterated nothing would pass identically. The book is ~1,900 rows; a
        // hundred spans is a floor no real gallery can fall under by accident.
        assert!(checked > 100, "only {checked} spans were checked");
    }

    /// 🔴 **NO SCREEN RENDERS A FRAGMENT OF A CREDENTIAL, AND A PREFIX IS A FRAGMENT.**
    ///
    /// Screen 30 shipped `sk-ant-…4f2c` and `sk-…9d1a`, under a footnote that read *"Estelle
    /// prints a prefix and a state, never a value"* — a sentence that made the leak sound like the
    /// safeguard. The founder's note was *"you probably shouldn't dox the API key, it should
    /// probably actually be hidden."* Screen 36 carried the same prefix in a doctor row.
    ///
    /// ⚠️ **THIS REPO ALREADY OWNED THE RULE AND THIS MODULE WAS OUTSIDE IT.**
    /// `top_level.rs::deletion_receipts_never_render_even_a_server_redacted_key_prefix` refuses
    /// `estelle_live_0b95827…` — a prefix the SERVER had already elided — while the screen next
    /// door printed one in full. A rule enforced on one surface is a rule on that surface.
    ///
    /// ⚠️ It asserts on VENDOR PREFIXES rather than on "looks like a secret", because the shape of
    /// a secret is unbounded and the shape of the four we actually name is not. `Anthropic` the
    /// word must stay legible, so the needles carry their separator: `sk-` not `sk`.
    #[test]
    fn no_book_screen_renders_a_credential_or_a_fragment_of_one() {
        const VENDOR_PREFIXES: [&str; 6] = [
            "sk-",
            "sk_",
            "estelle_live_",
            "ghp_",
            "github_pat_",
            "xoxb-",
        ];
        let palette = ScreenTheme::Dark.palette();
        let mut screens = 0_usize;
        for screen in SCREENS {
            screens += 1;
            for line in (screen.render)(&palette, 0, true) {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                for needle in VENDOR_PREFIXES {
                    assert!(
                        !text.contains(needle),
                        "{} renders a credential fragment {needle:?}: {text:?}",
                        screen.name
                    );
                }
            }
        }
        assert!(screens > 20, "only {screens} screens were checked");
    }

    /// 🔴 THE NO-BOX RULE, ENFORCED AT THE SOURCE RATHER THAN ONLY AT THE FRAME.
    ///
    /// The gallery already greps the rendered buffer, but that check runs only when the gallery
    /// runs. This one runs on every `cargo test` and names the screen, so a corner never gets as
    /// far as a frame.
    #[test]
    fn no_book_screen_draws_a_box_corner() {
        const CORNERS: [char; 9] = ['┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];
        let palette = ScreenTheme::Dark.palette();
        for screen in SCREENS {
            for line in (screen.render)(&palette, 0, true) {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                for corner in CORNERS {
                    assert!(
                        !text.contains(corner),
                        "{} drew a box corner {corner:?} in {text:?}",
                        screen.name
                    );
                }
            }
        }
    }
}
