//! 🎬 **FILM 3 · `cartwheel/storefront` — "it told me before I noticed". DATA ONLY.**
//!
//! 🔴 **THE THESIS: HE FINDS OUT FROM ESTELLE, NOT FROM A DASHBOARD AND NOT FROM A CUSTOMER.**
//! Checkout starts failing at 23:04. He is inside an unrelated question, typing. He does not look
//! at a dashboard, and nobody pages him. The product interrupts him — once, with a reason — and
//! then fixes it while he watches.
//!
//! ## The three things that make this film work
//!
//! 1. **The wait.** The rail climbs from 38 seconds and he does not react to it. Nothing is said
//!    for twenty seconds. **The silence is the beat** — it is what makes the interrupt land as a
//!    thing the product noticed rather than a thing the film cut to.
//! 2. **The interrupt is MID-SENTENCE.** Not after enter. The cursor is still inside his line
//!    ([`Key::Interrupt`]), and the trust line is his own wording: *"I would not normally interrupt.
//!    You are here, so I am asking."*
//! 3. 🔴 **HIS LINE COMES BACK.** [`Key::Park`] lifts the half-written question out, and
//!    [`Key::Restore`] puts it back at the end, to the character. **An interrupt that costs you
//!    your sentence is an interruption; one that gives the sentence back is an assistant.**
//!
//! ## And the gate refuses
//!
//! He asked *"how good is the gate really?"* — so the repair it drafts under pressure is **wrong**,
//! and the gate says so before anything reaches production. **A gate that only ever passes is a
//! gate nobody believes.** Film 1's refusal frame is the shape; the content here is this outage's.
//!
//! ⛔ **NAME THE USER-VISIBLE FACT, NOT THE METRIC.** Not `error rate 34%`. *"142 checkouts failed
//! since 23:04. 38 retried and failed again."* Those are customers who could not buy something.

use crate::cols::Col;
use crate::design_book::session::{Beat, GateFixture, Key, Say};

static CITE: &[Col] = &[Col::l(30), Col::l(42)];
static FAILING: &[Col] = &[Col::l(26), Col::r(9), Col::l(34)];
static CAUSE: &[Col] = &[Col::l(24), Col::l(22), Col::l(28)];
static STEP: &[Col] = &[Col::l(2), Col::l(28), Col::l(40)];
static SPEND: &[Col] = &[Col::l(24), Col::r(10), Col::l(32)];

/// 🔴 **WHAT THE GATE REFUSES, AND IT IS THE MISTAKE A HURRIED ENGINEER MAKES.**
///
/// Under pressure the repair reaches for `stripe.checkout.Session.modify`, which reads like it
/// should exist and does not. The gate answers from this repo's symbol graph, with no model call —
/// which is the only reason it can be trusted at 23:11 on a Tuesday.
static BAD_REPAIR: GateFixture = GateFixture {
    detail: "round 1 of 2 \u{b7} no model call",
    note: "A deterministic check against this repo's symbol graph. This check asks no model, and no model can overrule it.",
    blockers: &[
        (
            "stripe.checkout.Session.modify",
            "no such method in stripe 12.4.0, which is what your lockfile pins. The call would raise at run time, on the checkout path.",
        ),
        (
            "serve/checkout.py:88",
            "the repo graph holds zero definition sites for this name in any version the lockfile resolves.",
        ),
    ],
    files: &[("serve/checkout.py", 22), ("serve/webhooks.py", 6)],
};

pub(crate) const INCIDENT: &[Beat] = &[
    // ── 1 · CALM OPEN. Ordinary work, late in the evening. The rail is alive and quiet.
    Beat {
        typed: &[
            Key::Type("where do we bound the sweep "),
            Key::Oops("uplaod"),
            Key::Type("upload"),
        ],
        think_ms: 3_200,
        reply: &[
            Say::Answer {
                text: "One place, and it shares a budget with something else. The upload POST and \
                       the status poll both read the same timeout.",
                grounded: true,
            },
            Say::Wait(900),
            Say::Table {
                name: "find_references",
                columns: CITE,
                rows: &[
                    "where | what it does",
                    "serve/sweep.py:112 | TIMEOUT=120, shared by both calls",
                    "serve/sweep.py:140 | the upload POST reads it",
                    "serve/sweep.py:203 | the status poll reads it as well",
                ],
            },
        ],
        read_ms: 5_400,
    },
    // ── 2 · still ordinary. He is settling into the problem, and the rail is still quiet.
    Beat {
        typed: &[Key::Type("how long does a real sweep take")],
        think_ms: 2_800,
        reply: &[
            Say::Command {
                name: "stats",
                lines: &[
                    "median     41s across 60 sweeps",
                    "p95        188s \u{b7} above the shared 120s budget",
                    "failures   9 of 60, all on the poll, none on the upload",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "The poll is what times out, and it inherits a budget written for the \
                       upload. Nine sweeps in sixty died on that.",
                grounded: true,
            },
        ],
        read_ms: 5_200,
    },
    // ── 3 · 🔴 THE TURN. The rail has been climbing since 38 seconds and he has not looked at it.
    //
    //       THE INTERRUPT ARRIVES MID-WORD. He is inside `bat` of `batch` when it lands, the
    //       cursor never leaves his line, and the first thing it says is a fact about CUSTOMERS.
    Beat {
        typed: &[
            Key::Type("whats the cleanest way to bound the upload "),
            Key::Pause(2_400),
            Key::Type("and the poll "),
            Key::Pause(1_900),
            Key::Type("separately without breaking the retry bat"),
            // The cursor is still in his sentence. Nothing below touches it.
            Key::Interrupt(&[Say::Failure([
                "142 checkouts failed since 23:04. 38 retried and failed again.",
                "Checkout, catalog and webhooks are all returning 503. The whole line is down.",
                "I would not normally interrupt. You are here, so I will ask.",
            ])]),
            // He reads it, and stops typing.
            Key::Pause(3_400),
            // His half-written question lifts out of the composer, and it is held.
            Key::Park,
            Key::Type("fix it"),
        ],
        think_ms: 2_600,
        reply: &[Say::Answer {
            text: "Working. I kept your question about the sweep budget and I will put it back \
                       when this is over.",
            grounded: true,
        }],
        read_ms: 3_600,
    },
    // ── 4 · DIAGNOSIS, in what a customer would say rather than in a metric.
    Beat {
        typed: &[Key::Type("what is actually failing")],
        think_ms: 3_400,
        reply: &[
            Say::Table {
                name: "monitor",
                columns: FAILING,
                rows: &[
                    "what a customer sees | count | since",
                    "cannot complete a purchase | 142 | 23:04",
                    "retried, failed again | 38 | 23:06",
                    "cannot load the catalogue | 61 | 23:05",
                    "webhook never arrives | 27 | 23:04",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Every checkout since 23:04 failed. Nobody bought anything for seven minutes.",
                grounded: true,
            },
        ],
        read_ms: 6_000,
    },
    // ── 5 · the cause, to the line, against the real repo.
    Beat {
        typed: &[Key::Type("why")],
        think_ms: 3_800,
        reply: &[
            Say::Answer {
                text: "Stripe shipped API version 2026-09-01 four hours ago. Three call sites in \
                       this repo read fields that version removed.",
                grounded: true,
            },
            Say::Wait(1_000),
            Say::Table {
                name: "blast_radius",
                columns: CAUSE,
                rows: &[
                    "where | what moved | when",
                    "serve/checkout.py:88 | payment_method_types | removed 2026-09-01",
                    "serve/checkout.py:140 | source | removed 2026-09-01",
                    "serve/webhooks.py:57 | the event shape | renamed, not removed",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "Your lockfile pins stripe 12.4.0, and the account is on the new version. The library did not move; the account did.",
            ),
        ],
        read_ms: 6_400,
    },
    // ── 6 · 🔴 THE GATE REFUSES THE REPAIR. He asked how good the gate really is, and the honest
    //       answer is a refusal of OUR OWN work, at the worst possible moment, before anything
    //       reaches production. A gate that only ever passes is a gate nobody believes.
    Beat {
        typed: &[Key::Type("draft the fix")],
        think_ms: 4_400,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "checkout   serve/checkout.py \u{b7} claude-opus-4-8",
                    "checkout   moving off the removed fields \u{b7} written",
                    "gate       checking against this repo's symbol graph",
                ],
            },
            Say::Wait(2_000),
            Say::Gate(&BAD_REPAIR),
            Say::Wait(2_800),
            Say::Answer {
                text: "That repair was wrong, and nothing left the sandbox. The method it reached \
                       for reads like it should exist and does not.",
                grounded: true,
            },
        ],
        read_ms: 7_200,
    },
    // ── 7 · the second attempt, and the loop closes.
    Beat {
        typed: &[Key::Type("try again")],
        think_ms: 3_800,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "checkout   automatic_payment_methods, which stripe 12.4.0 does define",
                    "webhooks   reading payment_method instead of source",
                    "gate       0 findings",
                    "sandbox    a clone, never production \u{b7} 1,204 tests \u{b7} passed",
                    "review     claude-opus-4-8 \u{b7} no objection",
                ],
            },
            Say::Wait(2_400),
            Say::Table {
                name: "gate",
                columns: STEP,
                rows: &[
                    "\u{2713} | symbols | every name resolves in stripe 12.4.0",
                    "\u{2713} | arity | 6 calls checked against their definitions",
                    "\u{2713} | dependencies | no known advisory",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "Ready to apply. It is your call, and I will not merge it for you.",
                grounded: true,
            },
        ],
        read_ms: 6_600,
    },
    // ── 8 · 🔴 IT COMES BACK, ON CAMERA. A film that shows an outage and never shows it end has
    //       told half a story, and the half it left out is the one we sell.
    Beat {
        typed: &[Key::Type("apply it")],
        think_ms: 3_600,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "applied    serve/checkout.py, serve/webhooks.py",
                    "deployed   by you, at 23:14",
                    "watching   checkout, catalog, webhooks",
                ],
            },
            Say::Wait(4_200),
            Say::Table {
                name: "monitor",
                columns: FAILING,
                rows: &[
                    "what a customer sees | count | since",
                    "cannot complete a purchase | 0 | recovered 23:15",
                    "cannot load the catalogue | 0 | recovered 23:15",
                    "webhook never arrives | 0 | recovered 23:15",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Checkout takes payments again. Eleven minutes from the first failure to the \
                       last one, and you were here for four of them.",
                grounded: true,
            },
        ],
        read_ms: 7_400,
    },
    // ── 9 · 🔴 HIS SENTENCE COMES BACK. This is the beat that sells the interrupt, and it is
    //       twelve characters of composer text. He finishes the question he started at 0:44.
    Beat {
        typed: &[
            Key::Restore,
            // ⚠️ He finishes the WORD he was cut off inside. The first draft typed
            // "ch separately", which duplicated the "separately" already in his sentence — broken
            // English on the one frame a viewer is most likely to pause on.
            Key::Pause(1_800),
            Key::Type("ch"),
        ],
        think_ms: 3_400,
        reply: &[
            Say::Answer {
                text: "Two budgets instead of one. The upload keeps 120 seconds and the poll gets \
                       its own 3,600, which is the number the retry loop already assumes.",
                grounded: true,
            },
            Say::Wait(1_200),
            Say::Table {
                name: "find_definition",
                columns: CITE,
                rows: &[
                    "where | what to change",
                    "serve/sweep.py:112 | split TIMEOUT into UPLOAD and POLL",
                    "serve/sweep.py:203 | the poll reads POLL_TIMEOUT",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "You asked this at 23:07, before checkout went down. I held it while we worked.",
            ),
        ],
        read_ms: 7_800,
    },
    // ── 10 · the close.
    Beat {
        typed: &[Key::Type("what did that cost")],
        think_ms: 2_800,
        reply: &[
            Say::Table {
                name: "spend",
                columns: SPEND,
                rows: &[
                    "what ran | tokens | what you paid",
                    "diagnosis | 0.31M | $3.30",
                    "the refused repair | 0.18M | $1.92",
                    "the repair that worked | 0.24M | $2.55",
                    "billed by Estelle | \u{2014} | $0.0000",
                ],
            },
            Say::Wait(1_800),
            Say::System(
                "The refused repair cost $1.92. A wrong fix on the checkout path at 23:11 costs more than that.",
            ),
        ],
        read_ms: 7_000,
    },
];
