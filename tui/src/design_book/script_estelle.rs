//! 🎬 **FILM 3 · `uqeu/estelle` — "capability at ease". DATA ONLY.**
//!
//! One person on Ultra. No outage, no teammate, no crisis. Just the thing working, which is the
//! hardest of the three to make interesting and the one an engineer in the room will believe.
//!
//! 🔴 **SET IN THIS REPO, DELIBERATELY.** Fourteen of the book's twenty-four screens carry
//! `uqeu/estelle`, `~/estelle` or `khai@fatelabs.ca` in their fixtures. In a film about a fictional
//! company those read as a continuity error; here they read as the truth, because it IS the repo we
//! dogfood on. The founder is not playing a customer in this one.
//!
//! ⚠️ **THE REFUSAL IN BEAT 4 IS THE POINT OF THE WHOLE FILM.** A stale index does not fail loudly —
//! it answers fluently with a citation into code that has moved, which is exactly the failure this
//! company exists to prevent. So the calm film is the one that shows the product declining to
//! answer, then fixing itself, then answering. Capability at ease includes knowing when to stop.

use crate::cols::Col;
use crate::design_book::session::{Beat, Key, Say};

static SINCE: &[Col] = &[Col::l(20), Col::l(52)];
static CITE: &[Col] = &[Col::l(30), Col::l(42)];
static GRAPH: &[Col] = &[Col::l(30), Col::l(10), Col::l(32)];
static SWEEP: &[Col] = &[Col::l(18), Col::l(54)];
static MEM: &[Col] = &[Col::l(22), Col::r(12), Col::l(34)];
static SKILL: &[Col] = &[Col::l(22), Col::l(50)];

pub(crate) const ESTELLE_REPO: &[Beat] = &[
    // ── 1 · the first thing anyone does after a night away.
    Beat {
        typed: &[Key::Type("catch me up")],
        think_ms: 3_000,
        reply: &[
            Say::Table {
                name: "resume",
                columns: SINCE,
                rows: &[
                    "since 23:40 | 8h 12m ago",
                    "merged | #418 checkout hotfix \u{b7} by Priya, 22:14 \u{b7} green",
                    "decided | status infrastructure must not depend on Redis",
                    "touched | serve/checkout.py \u{b7} serve/webhooks.py \u{b7} docs/adr/0011",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "The Redis decision came out of Marcus's revert at 23:02, not out of a meeting. I will show this to anyone who proposes a Redis-backed status cache, with the commit.",
            ),
        ],
        read_ms: 6_200,
    },
    // ── 2 · a grounded answer about our own code, cited to the line.
    Beat {
        typed: &[
            Key::Type("where does the gate decide "),
            Key::Oops("wether"),
            Key::Type("whether to merge?"),
        ],
        think_ms: 3_600,
        reply: &[
            Say::Answer {
                text: "One function decides it, and nothing else may. `decide_auto_merge` returns \
                       a PR unless every guard passes.",
                grounded: true,
            },
            Say::Wait(800),
            Say::Table {
                name: "find_definition",
                columns: CITE,
                rows: &[
                    "where | what it does",
                    "serve/autonomy.py:214 | decide_auto_merge \u{b7} the only caller of the ceiling",
                    "serve/autonomy.py:96 | the fail-closed ceiling itself",
                    "serve/gate.py:41 | the grounding verdict it reads",
                ],
            },
        ],
        read_ms: 9_600,
    },
    // ── 3 · the code graph, which is the thing a grep cannot answer.
    Beat {
        typed: &[
            Key::Burst("whats the blast radius "),
            Key::Type("if i change Rows.fetchone"),
        ],
        think_ms: 4_000,
        reply: &[
            Say::Table {
                name: "blast_radius",
                columns: GRAPH,
                rows: &[
                    "caller | depth | why it matters",
                    "pg_pool.Rows | 0 | the shape itself",
                    "estelle_server.py:9518 | 1 | ledger.may_serve, on every request",
                    "api_shared.py:181 | 1 | caller resolution, three times a request",
                    "endpoint_runs.py:112 | 2 | the admin read path",
                ],
            },
            Say::Wait(1_200),
            Say::System(
                "17 call sites, 4 of them on the request hot path. Its docstring says it exists because its absence was the same bug twice.",
            ),
        ],
        read_ms: 6_400,
    },
    // ── 4 · 🔴 IT REFUSES. The index is behind the tree, so an answer would be fluent and wrong.
    Beat {
        typed: &[Key::Type("what calls find_secret_shape now?")],
        think_ms: 2_200,
        reply: &[
            Say::Failure([
                "STALE \u{2014} indexed at 6ff03b18, the repo is now 75557c7f.",
                "214 files changed since the sweep. I give no citation.",
                "A stale index does not fail loudly; it answers with a plausible wrong line.",
            ]),
            Say::Wait(1_600),
            Say::Table {
                name: "sweep",
                columns: SWEEP,
                rows: &[
                    "reading | 214 changed files",
                    "chunking | 1,284 files \u{b7} 61%",
                    "embedding | queued",
                ],
            },
            Say::Wait(2_200),
            Say::System("Swept to 75557c7f. Asking again, at the tree you actually have."),
        ],
        read_ms: 7_400,
    },
    // ── 5 · the same question, now answerable. The loop closing is what makes the refusal useful.
    Beat {
        typed: &[Key::Type("try again")],
        think_ms: 2_800,
        reply: &[
            Say::Answer {
                text: "Two callers. Both arrived after that sweep, so I refused the first answer \
                       instead of a guess.",
                grounded: true,
            },
            Say::Wait(700),
            Say::Table {
                name: "find_references",
                columns: CITE,
                rows: &[
                    "where | when it arrived",
                    "tui/src/top_level.rs:515 | the context hook's pre-network fence",
                    "estelle-client/src/auth.rs:55 | the shared shape engine",
                ],
            },
        ],
        read_ms: 5_600,
    },
    // ── 6 · what the plan actually buys, in the only unit Estelle meters.
    Beat {
        typed: &[
            Key::Type("how much memory do i have "),
            Key::Pause(600),
            Key::Type("left"),
        ],
        think_ms: 2_400,
        reply: &[
            Say::Table {
                name: "usage",
                columns: MEM,
                rows: &[
                    "this repo | 11.5M | uqeu/estelle, whole",
                    "used | 103M | across 6 repos",
                    "your plan | 250M | Ultra \u{b7} $85 a month",
                    "left | 147M | no token meter, ever",
                ],
            },
            Say::Wait(1_100),
            Say::System(
                "Your bill counts memory. Whoever you bring bills the model tokens. Here that is nobody: the last answer ran on this machine.",
            ),
        ],
        read_ms: 6_000,
    },
    // ── 7 · skills, which are the team's written rules rather than prompts.
    Beat {
        typed: &[Key::Burst("/skills "), Key::Type("retry")],
        think_ms: 2_000,
        reply: &[
            Say::Table {
                name: "skills",
                columns: SKILL,
                rows: &[
                    "skill | what it encodes",
                    "upstream-budget | third-party rate limits, written by Priya",
                    "no-redis-status | Marcus's revert, 23:02 last night",
                    "gate-before-pr | never open a PR the gate has not seen",
                ],
            },
            Say::Wait(1_100),
            Say::System(
                "Loaded upstream-budget. It is a rule your team wrote, not a prompt we did.",
            ),
        ],
        read_ms: 5_600,
    },
    // ── 8 · the closing frame. One number, and the limit said out loud beside it.
    Beat {
        typed: &[Key::Type("how good is the gate, really")],
        think_ms: 3_200,
        reply: &[
            Say::Answer {
                text: "On 41,934 labelled snippets: 100.0% of invented repository APIs caught, \
                       0.0% false positives on real symbols. It runs on this laptop, offline, with \
                       no key.",
                grounded: true,
            },
            Say::Wait(1_600),
            // ⛔ THE LIMIT IS ON THE SCREEN, NOT IN THE VOICEOVER. A number this good is the one a
            // hostile reader attacks first, and stating its boundary is what makes it survive.
            Say::System(
                "The limit: that measures INVENTED REPOSITORY APIs, in Python. It is not a correctness claim, and 12 of 23 languages block.",
            ),
        ],
        read_ms: 6_000,
    },
];
