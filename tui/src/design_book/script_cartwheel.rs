//! 🎬 **FILM 2 · `cartwheel/storefront` — "the repo moved without me". DATA ONLY.**
//!
//! 🔴 **THE THESIS: TEAM CONTEXTUAL MEMORY, AND IT IS THE ONE NOBODY ELSE CAN BUILD.** He comes back
//! to a repository that moved while he was away and does not know what happened. Estelle volunteers
//! the team's context, again and again, without being asked for it — a decision with its ADR line,
//! a teammate mid-flight in the same file, a proposal already parked in Slack.
//!
//! ## The frame that has no competitor
//!
//! Beat 5. Not *"Devon committed to this file"* — every tool can read git. This:
//!
//! > **Devon asked about this same retry path at 09:40 and got an answer. He is working from it.**
//!
//! What a teammate **asked their agent**, and **what it told them**. That is a fact about the team's
//! reasoning rather than about their commits, it exists only where every session lands in one
//! memory, and it is the reason two people stop building the same thing twice.
//!
//! ## What this film is NOT
//!
//! It was a production fire, which made it film 3's twin. The fire moved out. ⛔ There is no outage
//! here, and the rail is **alive but calm** — traffic moves, a PR opens at forty seconds, and
//! nothing ever goes red. Movement without alarm.
//!
//! ⚠️ **THE `/choose` BEAT MOVED HERE FROM FILM 1**, where a team decision never belonged: film 1 is
//! one developer alone. Priya's retry budget is a team fact and this is the team film.

use crate::cols::Col;
use crate::design_book::session::{Beat, Key, Say};

static SINCE: &[Col] = &[Col::l(10), Col::l(20), Col::l(42)];
static DECIDED: &[Col] = &[Col::l(9), Col::l(9), Col::l(54)];
static INFLIGHT: &[Col] = &[Col::l(12), Col::l(24), Col::l(36)];
static ASKED: &[Col] = &[Col::l(9), Col::l(9), Col::l(54)];
static CHOICE: &[Col] = &[Col::l(4), Col::l(62)];
static HANDOFF: &[Col] = &[Col::l(12), Col::l(60)];
static PARKED: &[Col] = &[Col::l(10), Col::l(62)];

pub(crate) const CARTWHEEL: &[Beat] = &[
    // ── 1 · he has been away. The repo has not.
    Beat {
        typed: &[
            Key::Type("what happened while i was "),
            Key::Oops("of"),
            Key::Type("off"),
        ],
        think_ms: 3_400,
        reply: &[
            Say::Table {
                name: "resume",
                columns: SINCE,
                rows: &[
                    "when | who | what",
                    "Fri 17:40 | Priya | merged #401, retry budget in the fetcher",
                    "Mon 09:12 | Devon | opened feat/checkout-idempotency",
                    "Mon 10:30 | Marcus | reverted the Redis status cache",
                    "Mon 11:05 | Sam | wrote the runbook for the new webhook",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Four days moved this repo. Two of those changes touch the file you opened \
                       this morning, so I will bring them up as you go.",
                grounded: true,
            },
        ],
        read_ms: 7_000,
    },
    // ── 2 · he starts work, and the first team fact arrives WITHOUT him asking for it.
    Beat {
        typed: &[Key::Type(
            "i want to add a retry around the payment capture",
        )],
        think_ms: 3_800,
        reply: &[
            Say::Failure([
                "Your team decided this on 14 August, and the code compiles either way.",
                "Priya capped the fetcher retries at 2. The insurer rate-limits per minute.",
                "docs/adr/0009-upstream-retry-budget.md:31",
            ]),
            Say::Wait(1_900),
            Say::Table {
                name: "memory",
                columns: DECIDED,
                rows: &[
                    "when | who | what the team decided",
                    "14 Aug | Priya | cap upstream retries at 2, no exceptions",
                    "14 Aug | Priya | a 5-deep backoff crosses the rate window",
                    "02 Sep | Marcus | status infrastructure must not depend on Redis",
                ],
            },
            Say::Wait(1_500),
            Say::Answer {
                text: "I raise this before you write it, not in review. A decision you learn about \
                       after the diff costs you the afternoon.",
                grounded: true,
            },
        ],
        read_ms: 8_200,
    },
    // ── 3 · he pushes back, which is the honest thing a person does. The choice is his.
    Beat {
        typed: &[
            Key::Type("the capture path is different though, "),
            Key::Pause(900),
            Key::Type("stripe retries it for us"),
        ],
        think_ms: 4_200,
        reply: &[
            Say::Answer {
                text: "You may be right. Priya wrote that decision about the insurer fetcher, and \
                       the capture path talks to Stripe, which retries for three days on its own.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Table {
                name: "choose",
                columns: CHOICE,
                rows: &[
                    "1 | follow the recorded decision \u{2014} cap at 2",
                    "2 | keep your retry, and record why this path differs",
                    "3 | ask Priya",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "Option 2 writes a decision of your own. The next person who reads this file finds your reasoning, not just your code.",
            ),
        ],
        read_ms: 7_600,
    },
    // -- 3b - 🔴 A PROPOSAL ALREADY PARKED IN SLACK. He is about to suggest a thing his own
    //          teammate suggested on Thursday, to silence. Nobody reads four days of #eng, and
    //          this is the cost of that: two people design the same thing a week apart.
    Beat {
        typed: &[Key::Type(
            "we should probably just centralise all the retry config",
        )],
        think_ms: 4_000,
        reply: &[
            Say::Table {
                name: "memory",
                columns: PARKED,
                rows: &[
                    "when | what is already sitting there",
                    "Thu 16:20 | Sam proposed one retry policy module in #eng",
                    "Thu 16:20 | three reactions, no replies, no ticket",
                    "Fri 09:05 | Priya asked her agent whether that would break the cap",
                    "Fri 09:05 | told: a shared module keeps the cap if the fetcher opts in",
                ],
            },
            Say::Wait(2_200),
            Say::Answer {
                text: "Sam wrote this proposal on Thursday and nobody answered him. Priya then \
                       checked it against the retry cap and got a yes. Your design already has two people behind it and no ticket.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::System(
                "I can open the ticket and reply in Sam's thread, so his proposal gets an answer four days late instead of never.",
            ),
        ],
        read_ms: 8_600,
    },
    // ── 4 · someone else is inside the same file, right now.
    Beat {
        typed: &[Key::Type("2")],
        think_ms: 3_000,
        reply: &[
            Say::Command {
                name: "memory",
                lines: &[
                    "recorded   capture retries differ from fetcher retries \u{b7} Stripe retries for 3 days",
                    "linked     docs/adr/0009-upstream-retry-budget.md:31",
                    "author     you \u{b7} 2 September",
                ],
            },
            Say::Wait(2_000),
            Say::Failure([
                "Devon is inside billing/capture.py right now.",
                "He opened feat/checkout-idempotency this morning and has 3 commits on it.",
                "His last commit touches the function you are about to change.",
            ]),
        ],
        read_ms: 7_400,
    },
    // ── 5 · 🔴 THE FRAME NOBODY ELSE CAN BUILD.
    //
    //       Every tool can read a colleague's COMMITS. This reads what he ASKED HIS AGENT and what
    //       it TOLD HIM — a fact about the team's reasoning rather than about their git history. It
    //       exists only where every session lands in one memory, and it is the difference between
    //       "Devon touched this file" and "Devon already has an answer, and here it is."
    Beat {
        typed: &[Key::Type("what is devon actually doing")],
        think_ms: 4_600,
        reply: &[
            Say::Table {
                name: "memory",
                columns: ASKED,
                rows: &[
                    "when | who | what they asked, and what they were told",
                    "09:40 | Devon | asked where the capture retries are bounded",
                    "09:40 | \u{2014} | told: the budget is capped at 2, ADR 0009",
                    "09:52 | Devon | asked whether Stripe retries on its own",
                    "09:52 | \u{2014} | told: yes, for 3 days, with the same idempotency key",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "Devon asked about this same retry path at 09:40 and got an answer. He works \
                       from it. His branch adds the idempotency key that makes your change safe, so \
                       you and Devon build halves of one fix.",
                grounded: true,
            },
            Say::Wait(1_800),
            Say::System(
                "Nobody told me that. Devon's session and yours land in the same memory, so I read his question and the answer he acted on.",
            ),
        ],
        read_ms: 9_400,
    },
    // -- 5b - THE SECOND AGENT-CONVERSATION FRAME, AND IT CONTRADICTS SOMEONE. Marcus was told
    //          one thing on Monday; the repo says another today. A team memory that only ever
    //          agrees with people is a memory nobody needs.
    Beat {
        typed: &[Key::Type("does anyone disagree with the postgres plan")],
        think_ms: 4_200,
        reply: &[
            Say::Table {
                name: "memory",
                columns: ASKED,
                rows: &[
                    "when | who | what they asked, and what they were told",
                    "Mon 10:22 | Marcus | asked what the status cache should use",
                    "Mon 10:22 | \u{2014} | told: Postgres, and he reverted Redis an hour later",
                    "Mon 14:10 | Sam | asked whether Postgres adds latency to capture",
                    "Mon 14:10 | \u{2014} | told: 4ms at your volume, measured on Friday",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "Nobody disagrees. Marcus reached the same conclusion on Monday and acted on \
                       it, and Sam already checked the latency question you would ask next.",
                grounded: true,
            },
        ],
        read_ms: 7_800,
    },
    // ── 6 · THE MEMORY CATCH. Film 1's near-miss shape, with a team fact instead of a solo one:
    //       the code compiles, the gate passes it, and only the record of what this TEAM decided
    //       catches it.
    Beat {
        typed: &[Key::Type("ok write my half")],
        think_ms: 4_000,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "capture    billing/capture.py \u{b7} claude-opus-4-8",
                    "capture    a bounded retry around the capture call \u{b7} written",
                    "gate       0 findings",
                ],
            },
            Say::Wait(2_200),
            Say::Failure([
                "One more thing, and the gate cannot see it.",
                "You store the idempotency key in Redis. Marcus reverted a Redis cache yesterday.",
                "His note: payment infrastructure must not depend on the thing that goes down.",
            ]),
            Say::Wait(2_000),
            Say::Answer {
                text: "The gate passed that code. Every symbol exists and the arity is right. Only \
                       the record of what this team already tried catches a change that compiles.",
                grounded: true,
            },
        ],
        read_ms: 8_600,
    },
    // ── 7 · he fixes it, and the two halves meet.
    Beat {
        typed: &[Key::Type("put it in postgres then")],
        think_ms: 3_600,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "capture    idempotency key on the payments table \u{b7} written",
                    "gate       0 findings",
                    "review     claude-opus-4-8 \u{b7} no objection",
                    "tests      41 cases \u{b7} passed",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Your half is ready. It uses the same key Devon's branch writes, so the two \
                       branches merge without a conflict and without a second design.",
                grounded: true,
            },
        ],
        read_ms: 6_400,
    },
    // ── 8 · DELEGATION OUT. The work leaves the terminal and reaches the people it concerns.
    //       ⛔ Every one of these is a proposal. Nothing merges and nothing closes.
    Beat {
        typed: &[Key::Burst("tell devon "), Key::Type("and log it properly")],
        think_ms: 4_400,
        reply: &[
            Say::Table {
                name: "handoff",
                columns: HANDOFF,
                rows: &[
                    "slack | posted to #eng, replying in Devon's own thread",
                    "linear | CRT-218 updated \u{b7} the two branches now say they pair",
                    "github | PR #421 opened, and linked to Devon's #419",
                    "memory | your capture decision, readable by everyone",
                ],
            },
            Say::Wait(2_200),
            Say::Command {
                name: "slack",
                lines: &[
                    "#eng \u{b7} 11:48 \u{b7} in Devon's thread from 09:41",
                    "\"On behalf of Ravi: he took the capture retry, you have the idempotency",
                    " key. Same key, both halves. His PR #421 links yours. Postgres, not",
                    " Redis \u{2014} Marcus's revert from yesterday.\"",
                    "https://cartwheel.slack.com/archives/C0192/p1756\u{2026}",
                ],
            },
        ],
        read_ms: 8_800,
    },
    // ── 9 · the close. He knows what the team knows, and none of it came from a meeting.
    Beat {
        typed: &[Key::Type("anything else i missed")],
        think_ms: 3_800,
        reply: &[
            Say::Table {
                name: "memory",
                columns: INFLIGHT,
                rows: &[
                    "who | where | what they are on",
                    "Priya | feat/rate-limits | 6 commits, waiting on your review",
                    "Devon | feat/checkout-idempotency | pairs with yours now",
                    "Marcus | main | reverted Redis, wrote the reason down",
                    "Sam | docs | runbook for the new webhook, ready to read",
                ],
            },
            Say::Wait(2_000),
            Say::Answer {
                text: "One thing waits on you: Priya's rate-limit branch has been open since \
                       Friday. Everything else moves without you.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::System(
                "You caught up on four days in nine minutes, and nobody wrote you a summary.",
            ),
        ],
        read_ms: 8_000,
    },
];
