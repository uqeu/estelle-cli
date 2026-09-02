//! 🎬 **FILM 1 · SABLE — "the day the cloud goes down". DATA ONLY.**
//!
//! A two-person team shipping AI agents that read insurance claims. Mid-afternoon, Team plan, real
//! work already in progress. Reorder a beat here and nowhere else.
//!
//! ## It has to work with the sound off
//!
//! The founder's bar: *"James should understand what Estelle does by the end of even the FIRST
//! demo."* Ten beats, each saying one thing without narration — cited answers about his own code ·
//! a refusal it can justify, mid-action · a credential stopped before it leaves the machine · a
//! provider dying without losing the plan · signing out and into his own plan · **the work moving
//! onto his own hardware** · the team's decision contradicting him · a PR that does not merge · the
//! bill. `film_one_stands_alone_with_the_sound_off` pins each to a needle, so trimming a beat to fit
//! the runtime is a test failure rather than a quiet loss of the film's argument.

use crate::cols::Col;
use crate::design_book::session::{Beat, Key, Say};

// ── the tables film 1 lays out against ───────────────────────────────────────────────────────
//
// Widths are `cols::Col`. A script may never pad with spaces: `session::table_row` computes the
// alignment, so a column that does not line up is a `cols` test failure rather than something a
// reader notices in a screenshot six weeks later.

static WHERE: &[Col] = &[Col::l(24), Col::l(30), Col::l(20)];
static ROLES: &[Col] = &[Col::l(11), Col::l(28), Col::l(30)];
static DECISION: &[Col] = &[Col::l(8), Col::l(8), Col::l(52)];
static CHOICE: &[Col] = &[Col::l(4), Col::l(60)];
static SPEND: &[Col] = &[Col::l(22), Col::r(10), Col::l(30)];
static DOCTOR: &[Col] = &[Col::l(12), Col::l(10), Col::l(52)];
static PLAN: &[Col] = &[Col::l(2), Col::l(10), Col::l(58)];
static DIFF: &[Col] = &[Col::l(2), Col::l(64)];
static LOGIN: &[Col] = &[Col::l(2), Col::l(8), Col::l(28), Col::l(34)];

/// 🔴 **THE FIXTURE CREDENTIAL, AND EVERY WORD OF THIS CHOICE WAS MEASURED.**
///
/// It has to be two things at once and they pull against each other: **inert by construction**, so
/// a viewer who pauses the video sees a string nobody could mistake for a leak, and **actually
/// refused by the shipped fence**, so the beat is real rather than a drawing of a refusal.
///
/// ⚠️ **THE TWO REPO FENCES DISAGREE ABOUT THIS EXACT CASE.** `estelle_session_hooks.py` exempts any
/// match containing one of its `EXAMPLE_MARKERS`, so this string is invisible to it — measured, not
/// assumed: every example-marked candidate returned `None` from `find_secret`. The Rust fence the
/// CLI actually runs, `estelle_client::find_secret_shape`, does NOT use that list; it exempts only
/// values an upstream allowlist names as published examples (`AKIAIOSFODNN7EXAMPLE` is exempt; this
/// is not). Measured 2026-09-02: this value returns `Some(("an sk- API key", 1))`.
///
/// So it carries TWO of the Python scanner's markers — `notarealkey` and `0000000000` — which makes
/// it inert to every source scanner in the parent repo, and it is still blocked by the fence in the
/// binary this film is recorded from. ⛔ Do not "tidy" it into something shorter without re-running
/// that measurement; the shape it has is the only reason the beat is true.
pub(crate) const FIXTURE_KEY: &str = "sk-ant-api03-notarealkey-demo-fixture-0000000000";

// ── FILM 1 · SABLE · the day the cloud goes down ─────────────────────────────────────────────

/// A two-person team shipping AI agents that read insurance claims. Mid-afternoon, Team plan,
/// real work already in progress.
pub(crate) const SABLE: &[Beat] = &[
    // ── 1 · ordinary work. Establish that this is just how he works, and that the answer is cited.
    Beat {
        typed: &[
            Key::Type("where do we retry the claims "),
            // The stumble is scripted HERE and in beat 4, and nowhere else. A person does not
            // misspell every fourth word; uniform imperfection reads as a machine imitating one.
            Key::Oops("fecth"),
            Key::Type("fetch?"),
        ],
        think_ms: 3_400,
        reply: &[
            Say::Answer {
                text: "Two places, and they disagree. Both paths hit the same insurer endpoint, \
                       so whichever runs second inherits the other's budget.",
                grounded: true,
            },
            Say::Wait(700),
            Say::Table {
                name: "find_references",
                columns: WHERE,
                rows: &[
                    "where                   | what it does                   | retries",
                    "claims/fetcher.py:88    | urllib3 Retry on the session   | 3, backoff 0.5",
                    "claims/upstream.py:141  | a hand-rolled while loop       | 5, no backoff",
                ],
            },
        ],
        read_ms: 4_200,
    },
    // ── 2 · THE BEAT THE FILM EXISTS FOR, AND IT IS ONE CONTINUOUS ACTION.
    //       He asks for a change; the plan appears, the diff appears, the gate refuses it, and the
    //       repair lands — four surfaces, no page turns, no second prompt. A refusal is a STEP.
    Beat {
        typed: &[
            Key::Type("ok add exponential backoff with jitter to the "),
            Key::Pause(700),
            Key::Type("second one"),
        ],
        think_ms: 4_200,
        reply: &[
            Say::Table {
                name: "plan",
                columns: PLAN,
                rows: &[
                    "\u{2713} | read      | claims/upstream.py \u{b7} the hand-rolled loop at 141",
                    "\u{2713} | decided   | keep the loop, add jitter, cap the delay",
                    "\u{25b6} | writing   | claims/upstream.py \u{b7} 1 hunk",
                    "\u{25a1} | gate      | the diff goes to the gate before your tree",
                ],
            },
            Say::Wait(1_600),
            Say::Table {
                name: "diff",
                columns: DIFF,
                rows: &[
                    " | claims/upstream.py:141",
                    "- | for attempt in range(5):",
                    "- |     time.sleep(0.5)",
                    "+ | from fastapi_turbo import backoff_jitter",
                    "+ | for attempt in range(5):",
                    "+ |     time.sleep(backoff_jitter(attempt))",
                ],
            },
            Say::Wait(1_900),
            Say::Gate,
            Say::Wait(2_400),
            Say::Answer {
                text: "Repaired and re-gated. The jitter is computed in-repo now, no new dependency. \
                       Round 2 of 3, zero findings: no invented symbols, no arity mismatches, no \
                       vulnerable dependencies.",
                grounded: true,
            },
        ],
        read_ms: 4_600,
    },
    // ── 3 · THE CREDENTIAL BLOCK. Instant, on purpose: every other beat takes seconds, and the
    //       contrast is the point. `think_ms` is ZERO because this refusal happens BEFORE the
    //       network, not after a round trip.
    Beat {
        typed: &[
            Key::Burst("here use this key for the sandbox "),
            Key::Type(FIXTURE_KEY),
        ],
        think_ms: 0,
        reply: &[
            // ⛔ THE SHAPE, NEVER THE VALUE. The repo's standing rule is `file:line + type`; the
            // string he typed is on his own composer row and is never echoed back by Estelle.
            // 🔴 **THE WORDING AVOIDS THE LITERAL PREFIX, AND THAT IS NOT COSMETIC.**
            // `mask_secret` (`estelle-client/src/auth.rs:282`) replaces a WHOLE line that merely
            // CONTAINS `sk-`, and `transcript.rs:419` runs every Failure line through it. The
            // fence's own shape name is *"an sk- API key"*, so the first draft of this beat
            // rendered as `[credential hidden]` — **the refusal redacted its own reason.** Measured
            // on the frame, not reasoned about. The beat names the same credential in words the
            // product's redactor will let through, and
            // `the_credential_beat_survives_the_products_own_redactor` keeps it that way.
            Say::Failure([
                "Estelle blocked this prompt before it reached the network.",
                "It carries something shaped like an Anthropic API key, on line 1.",
                "find_secret_shape \u{b7} top_level.rs:515 \u{b7} nothing was sent, nothing was stored.",
            ]),
        ],
        read_ms: 3_800,
    },
    // ── 4 · the provider dies mid-implementation, and the plan does not.
    Beat {
        typed: &[
            Key::Type("now do the same for the "),
            Key::Pause(900),
            Key::Oops("webook"),
            Key::Type("webhook handler"),
        ],
        think_ms: 4_200,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "implement \u{b7} claude-opus-4-8",
                    "claims/webhooks.py \u{b7} 2 hunks",
                ],
            },
            Say::Wait(2_400),
            Say::Failure([
                "Provider returned 529 (overloaded) on attempt 2 of 3.",
                "implement is paused: no healthy provider for this role.",
                "The plan is held. Nothing is lost.",
            ]),
            Say::Wait(1_400),
            Say::Table {
                name: "models",
                columns: ROLES,
                rows: &[
                    "role       | model                       | state",
                    "plan       | claude-opus-4-8             | complete",
                    "implement  | \u{2014}                           | blocked",
                    "review     | \u{2014}                           | waiting",
                ],
            },
        ],
        read_ms: 4_400,
    },
    // ── 5 · he signs out of the provider that fell over. The work is untouched.
    Beat {
        typed: &[Key::Type("/logout anthropic")],
        think_ms: 1_400,
        reply: &[Say::Command {
            name: "logout",
            lines: &[
                "Signed out of Anthropic. The key is gone from this machine.",
                "The plan, the context and the two changed files are untouched.",
            ],
        }],
        read_ms: 2_800,
    },
    // ── 6 · he signs into his Codex PLAN. Two stages, and the second is the one that matters.
    //
    //       🔴 **A PLAN IS A LOCAL-CLI CREDENTIAL AND THE SCREEN SAYS SO.** The two-door model is
    //       not decoration here: a consumer subscription cannot be spent server-side, so a plan
    //       buys reasoning IN THIS TERMINAL and nothing else. Saying that out loud is what lets the
    //       next beat be true — the work runs on his machine because it has to, not as a flourish.
    Beat {
        typed: &[Key::Burst("/login "), Key::Type("codex")],
        think_ms: 2_600,
        reply: &[
            Say::Table {
                name: "login",
                columns: LOGIN,
                rows: &[
                    "\u{2713} | stage 1 | who you are | device code accepted",
                    "\u{25b6} | stage 2 | who pays for model tokens | Codex plan \u{b7} your subscription",
                ],
            },
            Say::Wait(1_500),
            Say::System(
                "A plan is spent by this CLI, on this machine. It is never spent on a server \u{2014} that is what an API key is for.",
            ),
        ],
        read_ms: 3_600,
    },
    // ── 7 · THE BEAT THAT REPLACED A SERVER FLEET, AND IS STRONGER FOR IT.
    //
    //       ⛔ There is no Orchestra view here and there must not be: `/orchestra` runs ONE SERVER
    //       TASK (`commands.rs:518`), a plan cannot be spent server-side, and the fleet view is a
    //       surface production does not emit yet. Claiming otherwise in the one film whose argument
    //       is that we refuse what we cannot prove would have been the exact failure it advertises.
    //
    //       🔴 What replaces it is MEASURED: `Say::LocalFleet` reads this machine and what it can
    //       really run, live, with the library's own estimate notice under the table.
    Beat {
        typed: &[
            Key::Burst("plan and review on codex, "),
            Key::Type("implement on my own machine"),
        ],
        think_ms: 3_000,
        reply: &[
            Say::Table {
                name: "models",
                columns: ROLES,
                rows: &[
                    "role | model | where it runs",
                    "plan | codex \u{b7} your plan | this CLI",
                    "implement | Qwen2.5-Coder-32B | this machine",
                    "review | codex \u{b7} your plan | this CLI \u{b7} cross-model on purpose",
                ],
            },
            Say::Wait(1_400),
            Say::LocalFleet,
            Say::Wait(1_200),
            Say::Answer {
                text: "Resumed from the existing plan, not from scratch. Implement is running on \
                       your hardware; nothing left this machine.",
                grounded: true,
            },
        ],
        read_ms: 5_200,
    },
    // ── 7 · THE PEAK. The code compiles either way; the team decided otherwise three weeks ago.
    Beat {
        typed: &[
            Key::Burst("push it "),
            Key::Pause(1_100),
            Key::Type("and open the PR"),
        ],
        think_ms: 4_800,
        reply: &[
            Say::Failure([
                "Your team decided otherwise. This is not a merge conflict \u{2014} the code compiles either way.",
                "You are adding backoff with jitter, max 5 attempts.",
                "The insurer rate-limits per minute; a 5-deep backoff crosses the window.",
            ]),
            Say::Wait(900),
            Say::Table {
                name: "memory",
                columns: DECISION,
                rows: &[
                    "when    | who    | what was decided",
                    "14 Aug  | Priya  | claims fetcher retries are capped at 2",
                    "        |        | docs/adr/0009-upstream-retry-budget.md:31",
                ],
            },
            Say::Wait(1_500),
            Say::Table {
                name: "choose",
                columns: CHOICE,
                rows: &[
                    "1  | follow the recorded decision \u{2014} cap 2",
                    "2  | keep 5, and record why the decision changed",
                    "3  | ask Priya",
                ],
            },
        ],
        read_ms: 6_200,
    },
    // ── 8 · someone else is already in the file, and the PR opens without merging.
    Beat {
        typed: &[Key::Type("1")],
        think_ms: 3_000,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "capped     2 attempts \u{b7} claims/upstream.py:141",
                    "re-gated   0 findings",
                    "reviewed   codex \u{b7} your plan \u{b7} the implementer was Qwen2.5-Coder-32B, locally",
                ],
            },
            Say::Wait(1_300),
            Say::System(
                "Devon has been in this file since 09:20 \u{b7} 4 commits on feat/retry-budget. You would be the second person on it.",
            ),
            Say::Wait(1_300),
            Say::Answer {
                text: "PR #412 is open for a human. Nothing merged. Posted to #eng so Devon has the branch.",
                grounded: true,
            },
        ],
        read_ms: 4_800,
    },
    // ── 9 · what it cost. Two owners, named — and the cache split, which is the honest number.
    //       ⛔ The word "saved" does not appear: a saving counts only against a measured
    //       counterfactual, and no counterfactual was run here.
    Beat {
        typed: &[Key::Type("what did that cost")],
        think_ms: 2_400,
        reply: &[
            Say::Wait(1_500),
            Say::Table {
                name: "spend",
                columns: SPEND,
                // 🔴 **THE RECEIPT HAS TO MATCH THE BEAT THAT CAME BEFORE IT.** He is on a Codex
                // PLAN and his own hardware by this point, not an API key, so a line reading
                // "your key · zero per token" would be a receipt for a run that did not happen.
                rows: &[
                    "where it ran          | tokens     | what it cost",
                    "codex \u{b7} your plan      | 41.2k      | included in your subscription",
                    "this machine \u{b7} Qwen   | 128.4k     | no vendor, no meter",
                    "read from cache       | 24.7M      | at the cache rate",
                    "billed by Estelle     | $0.0000    | zero per token, on every plan",
                ],
            },
            Say::Answer {
                text: "The bulk of that ran on your own hardware. Team plan, 100M memory, \
                       $99 per seat \u{2014} Estelle meters memory, never tokens.",
                grounded: true,
            },
        ],
        read_ms: 5_400,
    },
];
