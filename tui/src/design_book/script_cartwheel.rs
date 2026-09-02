//! 🎬 **FILM 2 · CARTWHEEL — "production is on fire and it already knows". DATA ONLY.**
//!
//! An online storefront on the Team plan. Checkout has been failing for eleven minutes and the
//! founder is mid-feature, has not looked at a dashboard, and does not change windows.
//!
//! 🔴 **THE SECOND HALF IS THE FILM.** The outage is the setup, not the point — anything can page
//! you. What nothing else does is notice the SHAPE of your outages: checkout has gone down four
//! times in ninety days and the support inbox spikes half an hour later every time, because there
//! is no status page. So the fire is four tight beats and the recommendation, the teammate
//! collision and the walk-out get the room. `the_recommendation_is_not_cut_to_fit_the_outage`
//! asserts that split in seconds, because "trim a beat to fit the runtime" always trims the end.

use crate::cols::Col;
use crate::design_book::session::{Beat, Key, Say};

static SIGNAL: &[Col] = &[Col::l(30), Col::r(10), Col::l(28)];
static CAUSE: &[Col] = &[Col::l(24), Col::l(22), Col::l(30)];
static FLIGHT: &[Col] = &[Col::l(16), Col::l(56)];
static PATCH: &[Col] = &[Col::l(2), Col::l(64)];
static COST: &[Col] = &[Col::l(20), Col::l(30), Col::r(10)];
static PATTERN: &[Col] = &[Col::l(16), Col::l(56)];
static CHOICE: &[Col] = &[Col::l(4), Col::l(60)];

pub(crate) const CARTWHEEL: &[Beat] = &[
    // ── 1 · he asks in plain English, in the terminal he was already coding in.
    Beat {
        typed: &[
            Key::Type("whats going on in "),
            Key::Oops("prod"),
            Key::Type("production"),
        ],
        think_ms: 2_600,
        reply: &[
            Say::Table {
                name: "monitor",
                columns: SIGNAL,
                rows: &[
                    "signal | failures | since",
                    "checkout.session.create | 142 | 11 minutes ago",
                    "webhook /stripe/events | 38 | 11 minutes ago",
                    "everything else | 0 | nominal",
                ],
            },
            Say::Wait(1_400),
        ],
        read_ms: 5_000,
    },
    // ── 2 · the root cause, to the line. This is the half a dashboard cannot do.
    Beat {
        typed: &[Key::Type("why")],
        think_ms: 3_400,
        reply: &[
            Say::Answer {
                text: "Stripe shipped API version 2026-09-01 four hours ago. Three call sites in \
                       this repo use fields that version removed.",
                grounded: true,
            },
            Say::Wait(900),
            Say::Table {
                name: "blast_radius",
                columns: CAUSE,
                rows: &[
                    "where | what moved | since",
                    "serve/checkout.py:88 | payment_method_types | removed in 2026-09-01",
                    "serve/checkout.py:140 | source | removed in 2026-09-01",
                    "serve/webhooks.py:57 | event shape changed | renamed, not removed",
                ],
            },
        ],
        read_ms: 6_200,
    },
    // ── 3 · it is already working, and it has NOT touched anything.
    Beat {
        typed: &[Key::Type("what are you doing about it")],
        think_ms: 2_800,
        reply: &[
            Say::Table {
                name: "work",
                columns: FLIGHT,
                rows: &[
                    "sandboxes | 3 clones, never production",
                    "suite | 1,204 tests \u{b7} 61% complete",
                    "fix | drafted, NOT applied \u{b7} waiting on the suite",
                ],
            },
            Say::Wait(2_600),
            Say::System("1,204 passed. 0 failed."),
        ],
        read_ms: 5_000,
    },
    // ── 4 · the diff, red for what leaves and green for what arrives, still not applied.
    Beat {
        typed: &[Key::Type("show me the diff")],
        think_ms: 2_400,
        reply: &[
            Say::Table {
                name: "diff",
                columns: PATCH,
                rows: &[
                    " | serve/checkout.py:88",
                    "- | payment_method_types=[\"card\"],",
                    "+ | automatic_payment_methods={\"enabled\": True},",
                    " | ",
                    " | serve/webhooks.py:57",
                    "- | event[\"data\"][\"object\"][\"source\"]",
                    "+ | event[\"data\"][\"object\"][\"payment_method\"]",
                ],
            },
            Say::Wait(1_500),
            Say::Table {
                name: "spend",
                columns: COST,
                rows: &[
                    "role | model | cost",
                    "plan | claude-opus-4-8 | $0.0333",
                    "implement | moonshotai/kimi-k2.7-code | $0.0104",
                    "review | claude-opus-4-8 | $0.0113",
                    "vendor list price | what the provider charges | $0.0550",
                    "billed by Estelle | your key \u{b7} zero per token | $0.0000",
                ],
            },
        ],
        read_ms: 6_400,
    },
    // ── 5 · 🔴 THE BEAT THE FILM EXISTS FOR. Not fixing the bug — noticing the SHAPE of the bugs.
    Beat {
        typed: &[Key::Type("open the PR")],
        think_ms: 3_200,
        reply: &[
            Say::Answer {
                text: "PR #418 is open for a human. Nothing merged.",
                grounded: true,
            },
            Say::Wait(2_000),
            Say::Failure([
                "A pattern, not an incident.",
                "Checkout has gone down 4 times in 90 days.",
                "Each time, your support inbox spikes 30 to 50 minutes later.",
            ]),
            Say::Wait(1_600),
            Say::Table {
                name: "pattern",
                columns: PATTERN,
                rows: &[
                    "13 Jun | checkout down 22 min \u{b7} 31 tickets, first at +38 min",
                    "02 Jul | checkout down 9 min \u{b7} 12 tickets, first at +41 min",
                    "19 Aug | checkout down 34 min \u{b7} 55 tickets, first at +29 min",
                    "today | checkout down 11 min \u{b7} 7 tickets so far, first at +33 min",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "You have no status page. Your customers cannot tell your outage from their own broken card.",
            ),
            Say::Wait(2_200),
            Say::System("Want me to build one?  [y/N]"),
        ],
        read_ms: 10_400,
    },
    // ── 6 · someone is already on it. Concurrent work, before two people waste an afternoon.
    Beat {
        typed: &[Key::Type("y")],
        think_ms: 2_600,
        reply: &[
            Say::Failure([
                "Hold \u{2014} Priya opened `feat/status-page` 20 minutes ago.",
                "3 commits. You would be the second person on this.",
                "I created nothing.",
            ]),
            Say::Wait(1_300),
            Say::Table {
                name: "choose",
                columns: CHOICE,
                rows: &[
                    "1 | tell her you are taking it",
                    "2 | hand it to her, with the incident data",
                    "3 | pair \u{2014} you take the backend",
                ],
            },
        ],
        read_ms: 7_000,
    },
    // ── 7 · it shows the message it sent, not a claim that it sent one.
    Beat {
        typed: &[Key::Type("2")],
        think_ms: 2_400,
        reply: &[
            Say::Command {
                name: "slack",
                lines: &[
                    "posted to #eng \u{b7} 19:44",
                    "\"Estelle, on behalf of Ravi: taking the checkout hotfix (PR #418).",
                    " Status page is yours \u{2014} he saw your branch. The incident data is on",
                    " the PR if you want it.\"",
                    "https://cartwheel.slack.com/archives/C0192/p1756\u{2026}",
                ],
            },
            Say::Wait(1_200),
        ],
        read_ms: 6_800,
    },
    // ── 8 · the walk-out. Nothing merges while he is on the train, and that is the DEFAULT.
    Beat {
        typed: &[
            Key::Type("i have to head home, "),
            Key::Pause(800),
            Key::Type("can you wrap up"),
        ],
        think_ms: 2_800,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "PR #418     open \u{b7} suite green \u{b7} sandbox verified \u{b7} NOT merged",
                    "status page  handed to Priya",
                    "monitor      still watching checkout",
                ],
            },
            Say::Wait(1_300),
            Say::Answer {
                text: "Nothing will merge without you. That is not a setting you turned on \u{2014} it \
                       is the default, and turning it off is opt-in, tiered and proof-gated.",
                grounded: true,
            },
        ],
        read_ms: 9_200,
    },
];
