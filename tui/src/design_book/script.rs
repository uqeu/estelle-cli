//! 🎬 **THE FILMS. THIS FILE IS DATA — REORDER IT WITHOUT TOUCHING ANY OTHER.**
//!
//! Every beat is a `const`: what gets typed, how long Estelle takes, what comes back, and how long
//! it sits there. No branch of [`crate::demo_session`] reads a beat's CONTENT to decide what to do
//! with it, so moving a beat, deleting one, or rewriting a line is an edit here and nowhere else.
//!
//! ## Film 1 has to work with the sound off
//!
//! The founder's bar: *"With these three demos I shouldn't need to speak — but I will speak. James
//! should understand what Estelle does by the end of even the FIRST demo."* So film 1 is ordered to
//! say seven things in seven beats, each visible without narration:
//!
//! | beat | what a silent viewer learns |
//! |------|------------------------------|
//! | 1 | it answers questions about YOUR code, with `file:line`, and it found a disagreement |
//! | 2 | it REFUSES a change it cannot ground, names why, then repairs and passes |
//! | 3 | it blocks a credential **before it leaves the machine**, instantly |
//! | 4 | the model provider dies and the plan survives |
//! | 5 | it says which provider is unhealthy rather than retrying into the dark |
//! | 6 | one line swaps the model and the run **resumes from the existing plan** |
//! | 7 | it knows what the team decided three weeks ago and says the code contradicts it |
//! | 8 | someone else is already in this file; it opens a PR and merges nothing |
//! | 9 | what it cost, in two numbers, and what the cache did to the bill |
//!
//! ## Why film 1 is set at a company that is not us
//!
//! ⚠️ **AND WHY FILM 3 WILL NOT BE.** Fourteen of the book's twenty-four screens carry `uqeu/estelle`,
//! `~/estelle` or `khai@fatelabs.ca` in their fixtures. Dropping those into a session at a fictional
//! company reads as jumping around even though the frame never cuts — the story says one thing and
//! the data says another. So film 1 uses only the screens whose fixtures are repo-AGNOSTIC
//! (`09`, `30`, `33b`, `36`, `42`) and writes Sable's own spine in [`Say`]; the screens that name
//! this repo belong in the film that is SET in this repo. ⚠️ Two of the five still carry
//! `~/estelle · main · $0.104` on their status row — one dim line, called out here rather than
//! quietly accepted.

use crate::cols::Col;
use crate::design_book::session::{Beat, Film, Grid, Ink, Key, Say};
use crate::marks::Mark::{Blocked, Landed, Refused};
use crate::marks::StepMark;
use crate::marks::StepMark::{Active, Done, NotStarted};

/// The ask bar's hint row, in the founder's own wording. One owner: the player reads this, and so
/// does the live frame's `ASK_HINTS`. ⚠️ They are two owners of one row today and the honest note
/// is here — `main.rs` builds it from pairs and this is the flattened copy the session draws.
pub(crate) const HINTS: &str =
    "enter send \u{b7} tab repo \u{b7} ctrl+s spend \u{b7} ctrl+m models \u{b7} esc stop";

/// Strings that exist ONLY in a fixture, used by the gate test in both directions.
///
/// 🔴 The positive half is why this is a list and not an `assert!(!contains)`: an absence check
/// passes identically over a player that drew nothing at all.
pub(crate) const FIXTURE_NEEDLES: &[&str] = &[
    "claims/upstream.py:141",
    "0009-upstream-retry-budget.md:31",
    "moonshotai/kimi-k2.7-code",
];

// ── the tables film 1 lays out against ───────────────────────────────────────────────────────
//
// One `Grid` per table, declared beside the rows it serves. Widths are `cols::Col`; inks are
// palette ROLES. A script cannot name a colour and cannot pad with spaces.

const CITE: Grid = Grid::new(
    &[Col::l(26), Col::l(38), Col::l(22)],
    &[Ink::Cite, Ink::Mid, Ink::Bright],
);

const DECIDE: Grid = Grid::new(
    &[Col::l(8), Col::l(10), Col::l(62)],
    &[Ink::Dim, Ink::Skill, Ink::Bright],
);

const CHOICE: Grid = Grid::new(&[Col::l(3), Col::l(70)], &[Ink::Cite, Ink::Mid]);

const SPEND: Grid = Grid::new(
    &[Col::l(24), Col::r(10), Col::l(44)],
    &[Ink::Mid, Ink::Bright, Ink::Dim],
);

const ROLE: Grid = Grid::new(
    &[Col::l(12), Col::l(30), Col::l(38)],
    &[Ink::Mid, Ink::Bright, Ink::Dim],
);

/// 🔴 **THE FIXTURE CREDENTIAL, AND EVERY WORD OF THIS CHOICE WAS MEASURED.**
///
/// It has to be two things at once and they pull against each other: **inert by construction**, so
/// a viewer who pauses the video sees a string nobody could mistake for a leak, and **actually
/// refused by the shipped fence**, so the beat is real rather than a drawing of a refusal.
///
/// ⚠️ **THE TWO REPO FENCES DISAGREE ABOUT THIS EXACT CASE, AND THE DIFFERENCE IS THE WHOLE
/// REASON THIS CONSTANT HAS A DOCSTRING.** `estelle_session_hooks.py` exempts any match containing
/// one of its `EXAMPLE_MARKERS`, so this string is invisible to it — measured, not assumed: every
/// example-marked candidate returned `None` from `find_secret`. The Rust fence the CLI actually
/// runs, `estelle_client::find_secret_shape`, does NOT use that list; it exempts only values an
/// upstream allowlist names as published examples (`AKIAIOSFODNN7EXAMPLE` is exempt; this is not).
/// Measured 2026-09-02: this value returns `Some(("an sk- API key", 1))`.
///
/// So it carries TWO of the Python scanner's markers — `notarealkey` and `0000000000` — which makes
/// it inert to every source scanner in the parent repo, and it is still blocked by the fence in the
/// binary this film is recorded from. ⛔ Do not "tidy" it into something shorter without re-running
/// that measurement; the shape it has is the only reason the beat is true.
const FIXTURE_KEY: &str = "sk-ant-api03-notarealkey-demo-fixture-0000000000";

// ── FILM 1 · SABLE · the day the cloud goes down ─────────────────────────────────────────────

/// A two-person team shipping AI agents that read insurance claims. Mid-afternoon, Team plan,
/// real work already in progress.
const SABLE: &[Beat] = &[
    // ── 1 · ordinary work. Establish that this is just how he works, and that the answer is cited.
    Beat {
        typed: &[
            Key::Type("where do we retry the claims "),
            // The stumble is scripted here and NOT anywhere else in this beat. A person does not
            // misspell every fourth word; uniform imperfection reads as a machine imitating one.
            Key::Oops("fecth"),
            Key::Type("fetch?"),
        ],
        think_ms: 3_400,
        line_ms: 300,
        reply: &[
            Say::Rule("grounded", "sable/claims-agent"),
            Say::Blank,
            Say::Head(
                Landed,
                "Two places, and they disagree",
                "3 citations \u{b7} no model call on the lookup",
            ),
            Say::Blank,
            Say::Cols(CITE, "where|what it does|retries"),
            Say::Row(
                CITE,
                "claims/fetcher.py:88|urllib3 Retry on the session|3, backoff 0.5",
            ),
            Say::Row(
                CITE,
                "claims/upstream.py:141|a hand-rolled while loop|5, no backoff",
            ),
            Say::Wait(1_100),
            Say::Blank,
            Say::Note("both paths hit the same insurer endpoint."),
        ],
        read_ms: 4_400,
    },
    // ── 2 · THE BEAT THE FILM EXISTS FOR. The gate refuses, names the package and the registry,
    //       and then the loop CONTINUES — a refusal is a step, not a stop.
    Beat {
        typed: &[
            Key::Type("ok add exponential backoff with jitter to the "),
            Key::Pause(700),
            Key::Type("second one"),
        ],
        think_ms: 5_200,
        line_ms: 260,
        reply: &[
            Say::Screen("09-gate-refused"),
            Say::Wait(2_800),
            Say::Blank,
            Say::Head(
                Landed,
                "Repaired and re-gated",
                "round 2 of 3 \u{b7} 0 findings",
            ),
            Say::Text(
                Ink::Green,
                "merge:true   0 invented symbols \u{b7} 0 arity mismatches \u{b7} 0 vulnerable deps",
            ),
        ],
        read_ms: 5_200,
    },
    // ── 3 · THE CREDENTIAL BLOCK. Instant, on purpose: every other beat in this film takes
    //       seconds, and the contrast is the point. `think_ms` is ZERO and `line_ms` is a tenth of
    //       a beat because this refusal happens before the network, not after a round trip.
    Beat {
        typed: &[
            Key::Burst("here use this key for the sandbox "),
            Key::Type(FIXTURE_KEY),
        ],
        think_ms: 0,
        line_ms: 90,
        reply: &[
            Say::Blank,
            Say::Head(Refused, "Blocked that", "you were about to paste a key"),
            // ⛔ THE SHAPE, NEVER THE VALUE. The repo's standing rule is `file:line + type`; the
            // string he typed is on his own composer row and is never echoed back by Estelle.
            Say::Text(Ink::Warn, "something shaped like an sk- API key, line 1"),
            Say::Note("it did not leave this machine. nothing was sent."),
            Say::Note("find_secret_shape \u{b7} top_level.rs:515 \u{b7} before the network call"),
        ],
        read_ms: 4_000,
    },
    // ── 4 · the provider dies mid-implementation, and the plan does not.
    Beat {
        typed: &[
            Key::Type("now do the same for the "),
            Key::Pause(900),
            // The second and last scripted stumble in this film. Two in nine beats is what a
            // person does; one per beat is a machine imitating one.
            Key::Oops("webook"),
            Key::Type("webhook handler"),
        ],
        think_ms: 4_200,
        line_ms: 280,
        reply: &[
            Say::Rule("implement", "claude-opus-4-8"),
            Say::Blank,
            Say::Step(Active, "implementing", "claims/webhooks.py \u{b7} 2 hunks"),
            Say::Wait(2_600),
            Say::Blank,
            Say::Head(
                Blocked,
                "Provider returned 529",
                "overloaded \u{b7} attempt 2 of 3",
            ),
            Say::Wait(1_700),
            Say::Blank,
            Say::Step(Done, "plan", "complete \u{b7} claude-opus-4-8"),
            Say::Step(
                StepMark::Blocked,
                "implement",
                "paused \u{b7} no healthy provider for this role",
            ),
            Say::Step(NotStarted, "review", "waiting"),
            Say::Blank,
            Say::Note("the plan is held. nothing is lost."),
        ],
        read_ms: 4_800,
    },
    // ── 5 · it says WHICH provider is unhealthy rather than retrying into the dark.
    Beat {
        typed: &[Key::Type("whats up with anthropic")],
        think_ms: 2_600,
        line_ms: 220,
        reply: &[Say::Screen("36-doctor-failing")],
        read_ms: 5_000,
    },
    // ── 6 · one line swaps the model, INCLUDING a local one, and the run resumes from the plan.
    Beat {
        typed: &[
            Key::Burst("use kimi for implement "),
            Key::Type("and keep opus on review"),
        ],
        think_ms: 3_200,
        line_ms: 200,
        reply: &[
            Say::Screen("30-provider-keys"),
            Say::Wait(1_900),
            Say::Blank,
            Say::Cols(ROLE, "role|model|why"),
            Say::Row(ROLE, "plan|claude-opus-4-8|complete, untouched"),
            Say::Lift(
                ROLE,
                "implement|moonshotai/kimi-k2.7-code|pinned \u{b7} healthy \u{b7} 256K",
            ),
            Say::Row(ROLE, "review|claude-opus-4-8|cross-model on purpose"),
            Say::Blank,
            Say::Step(
                Active,
                "implement",
                "resumed from the existing plan, not from scratch",
            ),
            Say::Note("plan, context and the two files already changed are unchanged."),
        ],
        read_ms: 4_600,
    },
    // ── 7 · THE PEAK. The code compiles either way; the team decided otherwise three weeks ago.
    Beat {
        typed: &[
            Key::Burst("push it "),
            Key::Pause(1_100),
            Key::Type("and open the PR"),
        ],
        think_ms: 4_800,
        line_ms: 300,
        reply: &[
            Say::Blank,
            Say::Head(
                Blocked,
                "Your team decided otherwise",
                "this is not a merge conflict \u{b7} the code compiles either way",
            ),
            Say::Blank,
            Say::Text(
                Ink::Bright,
                "you are adding backoff with jitter, max 5 attempts.",
            ),
            Say::Blank,
            Say::Cols(DECIDE, "when|who|what was decided"),
            Say::Row(
                DECIDE,
                "14 Aug|Priya|claims fetcher retries are capped at 2",
            ),
            Say::Text(Ink::Cite, "docs/adr/0009-upstream-retry-budget.md:31"),
            Say::Note("the insurer rate-limits per minute. a 5-deep backoff crosses the window."),
            Say::Wait(1_800),
            Say::Blank,
            Say::Lift(CHOICE, "1|follow the recorded decision \u{2014} cap 2"),
            Say::Row(CHOICE, "2|keep 5, and record why the decision changed"),
            Say::Row(CHOICE, "3|ask Priya"),
        ],
        read_ms: 6_400,
    },
    // ── 8 · someone else is already in the file, and the PR opens without merging.
    Beat {
        typed: &[Key::Type("1")],
        think_ms: 3_000,
        line_ms: 260,
        reply: &[
            Say::Blank,
            Say::Step(Done, "capped", "2 attempts \u{b7} claims/upstream.py:141"),
            Say::Step(Done, "re-gated", "0 findings"),
            Say::Step(
                Done,
                "reviewed",
                "claude-opus-4-8 \u{b7} the implementer was kimi",
            ),
            Say::Wait(1_500),
            Say::Blank,
            Say::Head(
                Blocked,
                "Devon has been in this file since 09:20",
                "4 commits on feat/retry-budget",
            ),
            Say::Note("you would be the second person on it."),
            Say::Wait(1_500),
            Say::Blank,
            Say::Text(Ink::Green, "PR #412 opened for a human. nothing merged."),
            Say::Text(Ink::Cite, "posted to #eng \u{b7} Devon has the branch"),
        ],
        read_ms: 5_200,
    },
    // ── 9 · what it cost. Two owners, named — and the cache split, which is the honest number.
    //       ⛔ The word "saved" does not appear: a saving counts only against a measured
    //       counterfactual, and no counterfactual was run here.
    Beat {
        typed: &[Key::Type("what did that cost")],
        think_ms: 2_400,
        line_ms: 240,
        reply: &[
            Say::Screen("33b-model-cost"),
            Say::Wait(1_700),
            Say::Blank,
            Say::Cols(SPEND, "prompt tokens|count|how it is billed"),
            Say::Row(SPEND, "read from cache|24.7M|at the cache rate"),
            Say::Row(SPEND, "computed|768k|in full"),
            Say::Text(
                Ink::Green,
                "32\u{d7} of this turn's prompt tokens came off cache.",
            ),
            Say::Blank,
            Say::Row(SPEND, "vendor list price|$0.0550|what the provider charges"),
            Say::Row(
                SPEND,
                "billed by Estelle|$0.0000|your key \u{b7} zero per token",
            ),
            Say::Blank,
            Say::Note("Team \u{b7} 100M memory \u{b7} $99 per seat."),
        ],
        read_ms: 5_800,
    },
];

/// 🔴 **WHAT A BEAT SAYS WHEN NOBODY ASKED FOR FIXTURES.**
///
/// Every number in these films is invented, and that includes the ones written in [`Say`] rather
/// than borrowed from a book screen. `Say::Screen` already goes through `design_book::render` and
/// is gated; the DSL rows were not, and the gate test caught them leaking `claims/upstream.py:141`
/// on a default run. **A second door into the fixtures is exactly the defect the book's one-owner
/// rule exists to prevent**, so the player swaps a beat's whole reply for this — the timing is
/// unchanged, so the runtime he rehearses against is the same either way.
pub(crate) const SHUT: &[Say] = &[
    Say::Blank,
    Say::Note("this beat draws design-fixture data, and fixtures are off."),
    Say::Note("the LAYOUT is production. play the film with  estelle demo --session 1 --demo"),
];

/// Every film, in the order `--session N` selects them.
pub(crate) const FILMS: &[Film] = &[Film {
    number: 1,
    repo: "sable/claims-agent",
    branch: "feat/retry-budget",
    beats: SABLE,
}];

/// The film `--session N` names, or `None`.
pub(crate) fn film(number: u8) -> Option<&'static Film> {
    FILMS.iter().find(|film| film.number == number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 **THE FIXTURE CREDENTIAL IS STILL REFUSED BY THE SHIPPED FENCE.**
    ///
    /// This is the assertion that keeps beat 3 honest. If someone shortens the string, or the
    /// engine's allowlist grows to cover it, the beat quietly becomes a DRAWING of a refusal that
    /// the product would not actually make — and nothing else in the suite would notice, because
    /// the frame renders identically either way.
    #[test]
    fn the_fixture_credential_is_really_refused_by_the_shipped_fence() {
        let (shape, line) = estelle_client::find_secret_shape(FIXTURE_KEY)
            .expect("the film's fixture key must be refused by the fence the CLI runs");
        assert_eq!(shape, "an sk- API key");
        assert_eq!(line, 1);
        // And the beat's rendered wording must name the SAME shape the fence reports, or the
        // screen is saying something the product would not say.
        let spoken: String = SABLE[2]
            .reply
            .iter()
            .filter_map(|say| match say {
                Say::Text(_, text) => Some(*text),
                _ => None,
            })
            .collect();
        assert!(
            spoken.contains(shape),
            "beat 3 names a different shape than the fence: {spoken:?}"
        );
    }

    /// ⛔ The fixture credential is inert to the parent repo's own source scanners, which exempt
    /// any match carrying an example marker. Two markers, so losing one is not losing the property.
    #[test]
    fn the_fixture_credential_carries_two_example_markers() {
        let lowered = FIXTURE_KEY.to_ascii_lowercase();
        let markers = ["notarealkey", "0000000000"];
        for marker in markers {
            assert!(
                lowered.contains(marker),
                "the fixture key lost its {marker:?} marker and is no longer inert by construction"
            );
        }
    }

    /// ⛔ **NO AI-SPEAK.** No beat may narrate its own helpfulness, hedge, apologise or greet.
    /// The founder's north star is a cool older sibling: they say the thing and move on.
    #[test]
    fn nothing_estelle_says_is_ai_speak() {
        const BANNED: &[&str] = &[
            "I noticed",
            "I've taken",
            "I have taken",
            "let me",
            "Let me",
            "I'd be happy",
            "As you can see",
            "as you can see",
            "Great!",
            "Sorry",
            "sorry",
            "apolog",
            "I hope",
            "feel free",
            "Certainly",
            "successfully",
        ];
        for film in FILMS {
            for beat in film.beats {
                for say in beat.reply {
                    let text = match say {
                        Say::Head(_, a, b) => format!("{a} {b}"),
                        Say::Note(a) | Say::Text(_, a) => (*a).to_string(),
                        Say::Step(_, a, b) | Say::Rule(a, b) => format!("{a} {b}"),
                        _ => String::new(),
                    };
                    for banned in BANNED {
                        assert!(
                            !text.contains(banned),
                            "film {} says {banned:?} in {text:?}",
                            film.number
                        );
                    }
                }
            }
        }
    }

    /// ⛔ **NEVER "SAVED".** A saving is only a fact against a measured counterfactual, and no
    /// film runs one. The honest frame is the cache split, which is a real field.
    #[test]
    fn no_film_claims_a_saving() {
        for film in FILMS {
            for beat in film.beats {
                for say in beat.reply {
                    let text = match say {
                        Say::Note(a) | Say::Text(_, a) => (*a).to_string(),
                        Say::Row(_, a) | Say::Lift(_, a) | Say::Cols(_, a) => (*a).to_string(),
                        Say::Head(_, a, b) => format!("{a} {b}"),
                        _ => String::new(),
                    };
                    let lowered = text.to_ascii_lowercase();
                    for claim in ["saved", "savings", "you save", "cheaper than"] {
                        assert!(
                            !lowered.contains(claim),
                            "film {} claims a saving: {text:?}",
                            film.number
                        );
                    }
                }
            }
        }
    }

    /// Film 1 must carry all seven statements a silent viewer has to leave with. Written as
    /// needles rather than as a comment, so removing a beat to fit the runtime is a test failure
    /// rather than a quiet loss of the film's whole argument.
    #[test]
    fn film_one_stands_alone_with_the_sound_off() {
        let spoken: String = SABLE
            .iter()
            .flat_map(|beat| beat.reply.iter())
            .map(|say| match say {
                Say::Head(_, a, b) | Say::Step(_, a, b) | Say::Rule(a, b) => format!("{a} {b} "),
                Say::Note(a) | Say::Text(_, a) => format!("{a} "),
                Say::Row(_, a) | Say::Lift(_, a) | Say::Cols(_, a) => format!("{a} "),
                Say::Screen(a) => format!("{a} "),
                _ => String::new(),
            })
            .collect();
        for statement in [
            "claims/upstream.py:141",         // 1 · cited answers about your own code
            "09-gate-refused",                // 2 · it refuses what it cannot ground
            "Blocked that",                   // 3 · the credential fence
            "529",                            // 4 · the provider dies
            "36-doctor-failing",              // 5 · it says which provider
            "resumed from the existing plan", // 6 · the work survives the swap
            "Your team decided otherwise",    // 7 · team memory contradicts him
            "nothing merged",                 // 8 · propose-only
            "billed by Estelle",              // 9 · what it cost
        ] {
            assert!(
                spoken.contains(statement),
                "film 1 lost the beat carrying {statement:?} — it no longer stands alone"
            );
        }
    }
}
