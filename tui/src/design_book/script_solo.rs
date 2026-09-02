//! 🎬 **FILM 1 · `saltbox/inkwell` — "the wall, and what I did next". DATA ONLY.**
//!
//! 🔴 **THE THESIS, AND IT IS THE MOST INVESTABLE CLAIM IN THE THREE FILMS.** A local model alone is
//! mediocre at a long agentic task. A local model with Estelle underneath it is competitive —
//! because research, grounding, the gate and cross-family review close the gap, and none of them
//! cost anything per token. The film attacks the cost structure of the whole industry: **you do not
//! need to rent a frontier model for the bulk of the work.**
//!
//! ## What changed from the first version of this film, and why
//!
//! It was Sable, a claims team, and the trigger was a 529. Both were wrong:
//! * **a usage cap, not an outage.** A 529 is bad luck. A cap happens to every heavy user, on
//!   purpose, by the design of the plans — which is more relatable and more damning.
//! * **a LONG agentic task, not a two-line retry fix.** Multi-file, multi-step, the kind you would
//!   never hand to a small model. He hands it to one anyway, and **the length of the film is the
//!   evidence**: one correct step proves nothing, thirty correct steps are the argument.
//! * **a solo developer, no team, no production.** The team story is film 2's alone now, and the
//!   `/choose` beat about a colleague's retry decision moved there — a team-memory beat sitting in
//!   this film was part of why all three felt the same.
//!
//! ## ⛔ THE HONESTY LINE, WHICH IS NOT NEGOTIABLE
//!
//! **No comparative NUMBER appears on screen.** No "as good as frontier", no percentage, no
//! benchmark. **We have not measured it** — that experiment is designed and has not run. A
//! fabricated benchmark is the one thing this repo never does, and it is the exact failure Estelle
//! exists to prevent. `/spend` is allowed because it is arithmetic on published rates, not a
//! capability claim. The quality argument lives in the founder's voiceover, where he can say what he
//! believes and own it.

use crate::cols::Col;
use crate::design_book::session::{Beat, FleetFixture, FleetWorker, GateFixture, Key, Say};

static PLAN: &[Col] = &[Col::l(2), Col::l(26), Col::l(24), Col::l(20)];
static STEP: &[Col] = &[Col::l(2), Col::l(30), Col::l(40)];
static PRICE: &[Col] = &[Col::l(26), Col::r(11), Col::l(34)];
static ROLES: &[Col] = &[Col::l(11), Col::l(28), Col::l(32)];
static LOGIN: &[Col] = &[Col::l(2), Col::l(8), Col::l(28), Col::l(32)];
static DOCS: &[Col] = &[Col::l(28), Col::l(44)];
static TESTS: &[Col] = &[Col::l(26), Col::r(8), Col::l(36)];
static FLEET: &[Col] = &[Col::l(4), Col::l(26), Col::l(24), Col::l(20)];
static QUEUE: &[Col] = &[Col::l(4), Col::l(30), Col::l(22), Col::l(22)];

/// 🔴 **THE FIXTURE CREDENTIAL, AND EVERY WORD OF THIS CHOICE WAS MEASURED.**
///
/// It has to be inert by construction — a viewer who pauses the video sees a string nobody could
/// mistake for a leak — and **actually refused by the shipped fence**, so the beat is real rather
/// than a drawing of a refusal.
///
/// ⚠️ **THE TWO REPO FENCES DISAGREE ABOUT THIS EXACT CASE.** `estelle_session_hooks.py` exempts any
/// match containing one of its `EXAMPLE_MARKERS`, so this string is invisible to it — measured, not
/// assumed. The Rust fence the CLI actually runs, `estelle_client::find_secret_shape`, does not use
/// that list; it exempts only values an upstream allowlist names as published examples. Measured
/// 2026-09-02: this value returns `Some(("an sk- API key", 1))`. It carries TWO of the Python
/// scanner's markers, so it is inert to every source scanner in the parent repo and still blocked by
/// the binary this film is recorded from. ⛔ Do not shorten it without re-running that measurement.
/// What the gate refuses in film 1: the local model reaches for a package that is not there.
static INVENTED_IMPORT: GateFixture = GateFixture {
    detail: "round 1 of 3 \u{b7} no model call",
    note: "A deterministic check against this repo's symbol graph. This check asks no model, and no model can overrule it.",
    blockers: &[
        (
            "import fastapi_turbo",
            "no such package on PyPI; nearest is fastapi (0.115.6). The import would fail at load, not at test time.",
        ),
        (
            "billing/portal.py:44",
            "the repo graph holds zero definition sites for this module in any version the lockfile resolves.",
        ),
    ],
    files: &[("billing/portal.py", 14), ("billing/hooks.py", 3)],
};

/// 🔴 **FILM 1'S TEN WORKERS, AS THE PRODUCT'S OWN RENDERER DRAWS THEM.**
///
/// This replaces two hand-typed `Say::Table` blocks that between them invented a per-worker MODEL
/// column — a cell `orchestra_view` refuses to draw, because `FleetAgent` carries neither a model
/// nor a cost. The founder read the difference off the screen without being told: *"in orchestra it
/// actually shows each model going… this doesn't really look like the CLI… it kind of seems like
/// you faked it."*
///
/// ⚠️ **THE MODEL IS STILL ON SCREEN, ON THE LINE WHERE IT IS TRUE.** `models · claude-opus-4-8`
/// is the fleet's roster, a real `FleetSnapshot` field, rendered on the frame's second row. Beat 2
/// still argues "ten workers, one model, all dead at once" — it just no longer fabricates ten
/// cells to say it.
///
/// ⚠️ **WORKER 7 REPORTS NOTHING, ON PURPOSE.** Screen 20 of the design book carries exactly this
/// row, and it is the most honest thing on the frame: the fleet does not pretend to know a state
/// the server never sent. It renders `? Unknown · worker state not reported` in `mid`, not a
/// warning colour, because unknown is the ABSENCE of a signal rather than a call for a human.
static TEN_WORKERS: FleetFixture = FleetFixture {
    killed_at_s: None,
    ..TEN_JOBS
};

/// 🔴 **THE NUMBERS IN THE FAILURE BANNER ARE DERIVED FROM THIS ROSTER, NOT ASSERTED OVER IT.**
/// The banner says *"Seven were mid-write"*, and at `killed_at_s` exactly seven workers have
/// started an assignment and not finished it — `the_stopped_fleet_reconciles_with_its_own_banner`
/// counts them off the rendered rows. A sentence and a table that disagree on camera is the kind of
/// detail a hostile viewer finds in one pause.
static TEN_STOPPED_AT: u32 = 9;

/// The same ten, after the usage cap. ⚠️ **`Killed`, NOT `Completed`** — `orchestra_view` draws a
/// red multiplication sign for a stopped process, and the contract it cites is explicit that a
/// stopped process is not a successful one. Ten green ticks would have inverted the whole beat.
static TEN_STOPPED: FleetFixture = FleetFixture {
    // 🔴 **BOTH FIELDS, AND THE FIRST DRAFT SET ONLY ONE.** `killed_at_s` is measured on the
    // fleet's OWN clock, which starts at `opens_at_s` — so a block that opens at 3 s and dies at
    // 9 s renders six seconds of a fleet that is still alive before it freezes. The stopped block
    // opens at the moment of death: it is a photograph of the wreck, not a replay of the crash.
    opens_at_s: TEN_STOPPED_AT,
    killed_at_s: Some(TEN_STOPPED_AT),
    ..TEN_JOBS
};

/// 🔴 **ONE ROSTER, TWO STATES.** Both blocks above spread this, so "which worker had which job"
/// cannot drift between the living table and the dead one — which is exactly what two hand-typed
/// tables could do, and did: the old pair disagreed, listing worker 7 as `error mapping` in both
/// while nothing else in the film ever mentioned error mapping.
const TEN_JOBS: FleetFixture = FleetFixture {
    batch: "Migrate billing off the removed Stripe fields",
    models: &["claude-opus-4-8"],
    narrator: "10 workers writing 24 assignments across the billing package",
    total: 24,
    // The batch has been running three seconds when the block appears, so the table opens
    // mid-flight rather than at zero — and two workers finish on camera during the beat.
    opens_at_s: 3,
    killed_at_s: None,
    workers: &[
        FleetWorker {
            action: Some("rewriting the webhook handler"),
            steps: 3,
            starts_s: 0,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("idempotency keys on capture"),
            steps: 3,
            starts_s: 0,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("reading the backfill script"),
            steps: 2,
            starts_s: 1,
            ends_s: Some(6),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("tests, billing/charge"),
            steps: 3,
            starts_s: 0,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("tests, billing/hooks"),
            steps: 2,
            starts_s: 1,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("reading the customer portal"),
            steps: 2,
            starts_s: 2,
            ends_s: Some(7),
            unknown_reason: None,
        },
        // The one the server never reported on. Screen 20 carries the same row.
        FleetWorker {
            action: None,
            steps: 2,
            starts_s: 0,
            ends_s: None,
            unknown_reason: Some("worker state not reported"),
        },
        FleetWorker {
            action: Some("reading the retry budget"),
            steps: 2,
            starts_s: 3,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("migration notes"),
            steps: 3,
            starts_s: 2,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("changelog"),
            steps: 2,
            starts_s: 9,
            ends_s: None,
            unknown_reason: None,
        },
    ],
};

pub(crate) const FIXTURE_KEY: &str = "sk-ant-api03-notarealkey-demo-fixture-0000000000";

pub(crate) const SOLO: &[Beat] = &[
    // ── 1 · OPEN INSIDE THE WORK. No greeting, no setup. A long task is already running.
    Beat {
        typed: &[
            Key::Type("keep going on the stripe "),
            // One of two scripted stumbles in this film. A person does not misspell every fourth
            // word; uniform imperfection reads as a machine imitating one.
            Key::Oops("migraton"),
            Key::Type("migration"),
        ],
        think_ms: 2_600,
        reply: &[
            Say::Table {
                name: "plan",
                columns: PLAN,
                rows: &[
                    "  | step | files | model",
                    "\u{2713} | read the old integration | billing/stripe.py, 4 more | claude-opus-4-8",
                    "\u{2713} | map removed fields | 11 call sites | claude-opus-4-8",
                    "\u{2713} | subscriptions | billing/subs.py | claude-opus-4-8",
                    "\u{2713} | customer portal | billing/portal.py | claude-opus-4-8",
                    "\u{25b6} | webhooks | billing/hooks.py | claude-opus-4-8",
                    "\u{25a1} | idempotency keys | billing/client.py | queued",
                    "\u{25a1} | backfill script | scripts/backfill.py | queued",
                    "\u{25a1} | tests | tests/test_billing.py | queued",
                    "\u{25a1} | gate | every diff, before your tree | deterministic",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "The run finished four of eleven steps. The webhook handler comes next, and it \
                       touches the removed fields in three places.",
                grounded: true,
            },
        ],
        read_ms: 6_400,
    },
    // ── 2 · THE WALL. Not an outage — a usage cap, which is worse because it is by design.
    Beat {
        typed: &[Key::Type("go")],
        think_ms: 3_800,
        reply: &[
            Say::Orchestra(&TEN_WORKERS),
            Say::Wait(3_400),
            // 🔴 THE CATASTROPHE. Not one worker: ALL TEN, at once, mid-write. Every engineer
            // watching has had this happen, and the beat is held rather than summarised.
            Say::Failure([
                "You reached your Anthropic usage limit. All ten workers stopped.",
                // 🔴 **THIS SENTENCE IS COUNTED OFF THE TABLE ABOVE IT, NOT TYPED.** It read
                // "Seven were mid-write. Four files are half changed." beside a hand-written table
                // that supported neither number. The worker rows are the product's renderer now,
                // so `the_stopped_fleet_reconciles_with_its_own_banner` counts the `[done/total]`
                // cells and fails if this line and those rows ever disagree. ⚠️ And the seventh
                // worker is not "mid-write" — its state never arrived, which is a DIFFERENT thing
                // and is the more interesting half: the fleet does not guess.
                "Six were mid-write. One never reported its state.",
                "Nothing merged. Your tree did not change.",
            ]),
            Say::Wait(2_600),
            Say::Orchestra(&TEN_STOPPED),
            Say::Wait(2_800),
            // ⚠️ THE CONTRAST IS THE PRODUCT, AND IT LANDS IN ONE CUT: the models died, the work
            // did not. Everything the ten were holding is held here instead.
            Say::Answer {
                text: "The models stopped. The work did not. I hold the plan, all eleven steps, the \
                       decisions from steps 1 to 4 and every diff that was in flight. Pick a \
                       different implementer and the ten jobs continue from where they stopped.",
                grounded: true,
            },
        ],
        read_ms: 6_600,
    },
    // ── 3 · COST IS THE REASON HE MOVES, AND IT IS ARITHMETIC, NOT A CLAIM.
    //       ⛔ Published rates only. No comparative quality number appears anywhere in this film.
    Beat {
        typed: &[
            Key::Type("what would it cost to finish this "),
            Key::Pause(600),
            Key::Oops("on teh"),
            Key::Type("on the api"),
        ],
        think_ms: 3_200,
        reply: &[
            Say::Table {
                name: "spend",
                columns: PRICE,
                rows: &[
                    "                        | tokens | on the API, at list price",
                    "steps 1-4, already run  | 1.84M  | $19.60",
                    "steps 5-11, remaining   | 3.10M  | $33.05 projected",
                    "this task, end to end   | 4.94M  | $52.65",
                ],
            },
            Say::Wait(1_600),
            Say::System(
                "Projected from the six steps already measured, at Anthropic's published rates. It is arithmetic, not an estimate of quality.",
            ),
        ],
        read_ms: 6_000,
    },
    // ── 4 · he signs into a plan he already pays for. Two stages, and the second is the one
    //       that matters: a plan is spent by THIS CLI, never by a server.
    Beat {
        typed: &[Key::Burst("/login "), Key::Type("codex")],
        think_ms: 2_400,
        reply: &[
            Say::Table {
                name: "login",
                columns: LOGIN,
                rows: &[
                    "\u{2713} | stage 1 | who you are | device code accepted",
                    "\u{25b6} | stage 2 | who pays for model tokens | Codex plan \u{b7} your subscription",
                ],
            },
            Say::Wait(1_400),
            Say::System(
                "This CLI spends a plan, on this machine. A server cannot spend one. That is the job of an API key.",
            ),
        ],
        read_ms: 4_400,
    },
    // ── 5 · THE PIVOT. Codex thinks, his own machine writes. The fleet numbers are MEASURED on
    //       the machine the film is recorded on — see `Say::LocalFleet`.
    Beat {
        typed: &[
            Key::Burst("plan and review on codex, "),
            Key::Type("implement on my machine"),
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
                    "review | codex \u{b7} your plan | this CLI",
                ],
            },
            Say::Wait(1_500),
            Say::LocalFleet,
            Say::Wait(1_600),
            Say::Answer {
                text: "Step 5 restarts with the plan and the four finished steps intact. The \
                       implementer changed; the work did not.",
                grounded: true,
            },
        ],
        read_ms: 6_400,
    },
    // -- 5b - THE PAYOFF. Ten local models take the same ten jobs. They RESUME; they do not restart.
    //
    //         🔴 **THE OBVIOUS OBJECTION IS ANSWERED ON SCREEN.** Everyone watching knows small
    //         models lose the plot on a long task. The answer is not a bigger model, it is the
    //         harness: each worker reads the SAME plan and the SAME decisions, so they do not
    //         repeat each other and they do not contradict each other.
    //
    //         ⛔ No comparative claim appears. Not "as good as", not a percentage. The viewer is
    //         shown ten models finishing a long task correctly and draws their own conclusion;
    //         the quality argument lives in the founder's voiceover, where he owns it.
    Beat {
        typed: &[Key::Type("give the ten jobs to my machine")],
        think_ms: 3_400,
        reply: &[
            Say::Table {
                name: "orchestra",
                columns: FLEET,
                rows: &[
                    "  | worker | job | state",
                    "1 | Qwen2.5-Coder-32B | webhooks | resuming at step 5",
                    "2 | Qwen2.5-Coder-32B | idempotency keys | resuming",
                    "3 | Qwen2.5-Coder-14B | backfill script | resuming",
                    "4 | Qwen2.5-Coder-14B | tests, billing | resuming",
                    "5 | Qwen2.5-Coder-14B | tests, hooks | resuming",
                    "6 | Qwen2.5-Coder-32B | customer portal | resuming",
                    "7 | Qwen2.5-Coder-7B | error mapping | resuming",
                    "8 | Qwen2.5-Coder-7B | retry budget | resuming",
                    "9 | Qwen2.5-Coder-7B | migration notes | resuming",
                    "10 | Qwen2.5-Coder-7B | changelog | resuming",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "Every worker reads the same plan and the same four decisions. Worker 2 knows \
                       what worker 1 wrote, so it does not write it again. None of them starts from \
                       an empty context.",
                grounded: true,
            },
        ],
        read_ms: 6_800,
    },
    // ── 6 · THE BODY BEGINS. This is most of the film and it is the argument: a small model
    //       doing many things correctly, with three independent nets around it.
    Beat {
        typed: &[Key::Type("carry on")],
        think_ms: 2_400,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "webhooks   billing/hooks.py \u{b7} Qwen2.5-Coder-32B \u{b7} on this machine",
                    "webhooks   checkout.session.completed \u{b7} written",
                    "webhooks   customer.subscription.updated \u{b7} written",
                ],
            },
            Say::Wait(2_600),
            Say::Failure([
                "Stopped on the third handler. `payment_intent.amount_capturable_updated` is not in this model's training data.",
                "Stripe added that event after this model shipped.",
                "The worker guessed nothing and wrote nothing.",
            ]),
        ],
        read_ms: 5_200,
    },
    // ── 7 · 🔴 THE BEAT THE FILM EXISTS FOR. A 32B model does not know this week's Stripe API.
    //       Estelle does, because it reads the documentation live and hands it over.
    Beat {
        typed: &[Key::Type("look it up")],
        think_ms: 4_200,
        reply: &[
            Say::Command {
                name: "research",
                lines: &[
                    "context7    stripe/stripe-python \u{b7} 2026-09-01",
                    "docs        stripe.com/docs/api/events/types",
                    "github      stripe/stripe-python \u{b7} CHANGELOG.md",
                ],
            },
            Say::Wait(2_400),
            Say::Table {
                name: "research",
                columns: DOCS,
                rows: &[
                    "field | what the current API says",
                    "amount_capturable | integer, minor units, present on the event",
                    "payment_intent.status | requires_capture while capturable",
                    "removed 2026-09-01 | payment_method_types, source",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "The handler needs the capturable amount and the status, and it must not read \
                       `source`. The model that writes this handler has that now.",
                grounded: true,
            },
        ],
        read_ms: 7_000,
    },
    // ── 8 · the local model writes it correctly against an API it was never trained on.
    Beat {
        typed: &[Key::Type("ok write it")],
        think_ms: 3_400,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "webhooks   payment_intent.amount_capturable_updated \u{b7} written",
                    "webhooks   billing/hooks.py \u{b7} 3 handlers \u{b7} 61 lines",
                    "gate       checking against this repo's symbol graph",
                ],
            },
            Say::Wait(2_200),
            Say::Table {
                name: "gate",
                columns: STEP,
                rows: &[
                    "\u{2713} | symbols | every name resolves in this repo",
                    "\u{2713} | arity | 3 calls checked against their definitions",
                    "\u{2713} | dependencies | stripe 12.4.0 \u{b7} no known advisory",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "The gate passed it. This check asks no model, and no model can overrule it.",
                grounded: true,
            },
        ],
        read_ms: 6_000,
    },
    // -- 8a - 🔴 THE NEAR-MISS. A capability is a claim; a model that WOULD have gone wrong and was
    //         pulled back is a proof. And the distinction in the last line is the one no rival can
    //         make: the gate checks whether a symbol EXISTS, memory checks whether this team
    //         already RULED IT OUT. Only one of those catches an idea that compiles.
    Beat {
        typed: &[Key::Type("whats worker 8 doing")],
        think_ms: 3_000,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "worker 8   retry budget \u{b7} billing/client.py",
                    "worker 8   proposed a second retry wrapper around the Stripe client",
                ],
            },
            Say::Wait(1_800),
            Say::Failure([
                "Stopped worker 8. You removed a retry wrapper from this file on 14 August.",
                "That wrapper double-charged two customers. Commit 4f2c1a9 carries the note on the revert.",
                "Worker 8 has that decision now. It uses the client's own idempotency instead.",
            ]),
            Say::Wait(2_000),
            Say::Answer {
                text: "The gate finds nothing wrong with that code. Every symbol in it exists, and the \
                       arity is right. Only the record of what this repo already tried can catch an \
                       idea that compiles.",
                grounded: true,
            },
        ],
        read_ms: 7_400,
    },
    // -- 8b - MORE RESEARCH, DIFFERENT SOURCES. He named Stripe, GitHub and Vercel by name: those
    //         are the APIs an engineer recognises instantly. Repeating the beat is the point:
    //         research is not a one-off trick, it is how the small model keeps working.
    Beat {
        typed: &[Key::Type("the portal redirect changed too, check it")],
        think_ms: 3_800,
        reply: &[
            Say::Command {
                name: "research",
                lines: &[
                    "context7    stripe/stripe-python \u{b7} billing_portal.Session",
                    "github      stripe/stripe-python \u{b7} 4 commits since your pinned version",
                    "vercel      vercel.com/docs/functions \u{b7} redirect limits on the edge runtime",
                ],
            },
            Say::Wait(2_400),
            Say::Table {
                name: "research",
                columns: DOCS,
                rows: &[
                    "what changed | what it means here",
                    "return_url is required | billing/portal.py:31 passes it already",
                    "flow_data added | optional, and you do not need it",
                    "edge runtime | a 307 is capped at 4KB of headers",
                ],
            },
            Say::Wait(1_600),
            Say::Answer {
                text: "Your portal code is already correct for the current API. One line in \
                       billing/portal.py:44 sets a cookie on the redirect that the edge runtime \
                       will drop. That is worth fixing while we are here.",
                grounded: true,
            },
        ],
        read_ms: 7_000,
    },
    // -- 8c - THE GATE CATCHES THE SMALL MODEL, AND THE LOOP CLOSES. A gate that only ever passes
    //         is a gate nobody believes, so it refuses something the local model actually wrote.
    Beat {
        typed: &[Key::Type("fix it")],
        think_ms: 3_400,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "portal     billing/portal.py \u{b7} Qwen2.5-Coder-32B",
                    "portal     moving the cookie onto the session \u{b7} written",
                ],
            },
            Say::Wait(2_000),
            Say::Gate(&INVENTED_IMPORT),
            Say::Wait(2_600),
            Say::Command {
                name: "work",
                lines: &[
                    "repair     using stripe.billing_portal.Session, which this repo already imports",
                    "gate       0 findings",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "The model reached for a helper that does not exist in this repo. The gate \
                       refused it before it touched your tree, and the second attempt used the one \
                       you already have at billing/portal.py:12.",
                grounded: true,
            },
        ],
        read_ms: 7_200,
    },
    // -- 8d - the graph keeps up with the work. A sweep is not a chore here: it is what makes the
    //         next gate check true.
    Beat {
        typed: &[Key::Type("is the graph still current")],
        think_ms: 2_600,
        reply: &[
            Say::Command {
                name: "sweep",
                lines: &[
                    "changed    7 files since the last sweep",
                    "chunking   billing/ \u{b7} 7 files",
                    "graph      current at your working tree",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Current. Every gate check from here reads the code you have now, not the \
                       code you had at the start of the task.",
                grounded: true,
            },
        ],
        read_ms: 4_800,
    },
    // ── 9 · the third net: a different model family argues with the code the local model wrote.
    Beat {
        typed: &[Key::Type("review it")],
        think_ms: 4_600,
        reply: &[
            Say::Command {
                name: "review",
                lines: &[
                    "reviewer   codex \u{b7} your plan \u{b7} a different family from the implementer",
                    "read       billing/hooks.py \u{b7} 61 lines",
                ],
            },
            Say::Wait(2_600),
            Say::Failure([
                "One objection. The handler is not idempotent.",
                "Stripe retries a webhook up to 3 days. This one would capture twice.",
                "billing/hooks.py:88 \u{b7} the handler records no event id before the capture.",
            ]),
            Say::Wait(1_600),
            Say::Answer {
                text: "Step 7 already plans the idempotency keys. I can pull it forward and the \
                       reviewer looks again, or you can leave it for step 7 as written.",
                grounded: true,
            },
        ],
        read_ms: 7_200,
    },
    // ── 10 · he decides, the small model fixes it, and the reviewer signs off.
    Beat {
        typed: &[Key::Type("pull it forward")],
        think_ms: 3_800,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "idempotency  billing/client.py \u{b7} Qwen2.5-Coder-32B",
                    "idempotency  event id recorded before capture \u{b7} written",
                    "webhooks     billing/hooks.py \u{b7} updated to use it",
                    "gate         0 findings",
                    "review       codex \u{b7} no objection",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "The fleet finished steps 5, 6 and 7. Four files changed. Nothing merged.",
                grounded: true,
            },
        ],
        read_ms: 5_600,
    },
    // ── 11 · the credential fence, in the middle of real work, instant.
    //         ⛔ THE SHAPE, NEVER THE VALUE. The string is on his own composer row and Estelle
    //         never echoes it back.
    Beat {
        typed: &[
            Key::Burst("use this for the sandbox "),
            Key::Type(FIXTURE_KEY),
        ],
        think_ms: 0,
        reply: &[
            // 🔴 The wording avoids the literal `sk-` prefix on purpose: `mask_secret`
            // (`estelle-client/src/auth.rs:282`) blanks a whole line that merely CONTAINS it, and
            // `transcript.rs:419` runs every Failure line through it. The first draft of this beat
            // quoted the fence's own shape name and rendered as `[credential hidden]` — the refusal
            // redacted its own reason.
            Say::Failure([
                "That prompt did not go out. It carries an Anthropic API key on line 1.",
                "Nothing left this machine, and I stored nothing.",
                "The sandbox reads the key from your environment. It does not need it in a message.",
            ]),
        ],
        read_ms: 4_600,
    },
    // ── 12 · the long task finishes. The length of this film is the evidence.
    Beat {
        typed: &[Key::Type("finish the rest")],
        think_ms: 3_000,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "backfill    scripts/backfill.py \u{b7} written \u{b7} gate 0 findings",
                    "tests       tests/test_billing.py \u{b7} 34 cases written",
                    "tests       running",
                ],
            },
            Say::Wait(3_200),
            Say::Table {
                name: "tests",
                columns: TESTS,
                rows: &[
                    "suite | cases | result",
                    "tests/test_billing.py | 34 | passed",
                    "tests/test_hooks.py | 19 | passed",
                    "the rest of the suite | 402 | passed",
                ],
            },
            Say::Wait(1_600),
            Say::Answer {
                text: "Eleven of eleven steps. Seven files changed, 455 tests green, and the gate \
                       saw every diff before your tree did. The PR is open. Nothing merged.",
                grounded: true,
            },
        ],
        read_ms: 7_400,
    },
    // -- 12b - 🔴 THE DELIGHT, AND IT IS ALSO THE COVERAGE ANSWER. The fleet finished and has
    //          capacity left. Nothing is rented, so idle capacity costs nothing — and it starts on
    //          the queue.
    //
    //          🔬 **EVERY SUITE APPEARS HERE BECAUSE THE WORK NEEDED IT, NOT BECAUSE WE WANTED TO
    //          SHOW IT.** That is the difference between a product tour and a product working, and
    //          it is the defect the founder has been naming all night in a different form. Each job
    //          is real work on THIS repo: a CVE in a package it actually uses, a doc that actually
    //          moved, a test that actually flakes.
    //
    //          ⛔ EVERY ONE IS PROPOSE-ONLY. Ten models filling a review queue is leverage; ten
    //          models merging is a liability, and the restraint is what makes it credible.
    Beat {
        typed: &[
            Key::Type("anything else you can pick up while "),
            Key::Pause(700),
            Key::Type("youre there"),
        ],
        think_ms: 4_000,
        reply: &[
            Say::Table {
                name: "orchestra",
                columns: QUEUE,
                rows: &[
                    "  | job | suite | state",
                    "1 | CVE in urllib3 2.2.1 | security | drafted, awaiting your review",
                    "2 | README shows the old flow | research | drafted, awaiting your review",
                    "3 | PR #204 has no review | review | drafted, awaiting your review",
                    "4 | test_refund flakes 1 in 12 | bench | repro found, awaiting review",
                    "5 | send_invoice has no caller | graph | reported, no change proposed",
                    "6 | 7 files ahead of the index | sweep | done",
                    "7 | what did we decide on refunds | memory | answered, 2 citations",
                    "8 | mailer latency doubled | monitor | watching, no fix proposed",
                    "9 | which worker for which job | orchestra | rebalanced",
                    "10 | every diff above | gate | 0 findings",
                ],
            },
            Say::Wait(2_600),
            Say::Answer {
                text: "Nine jobs wait for you. The fleet finished the tenth. Read the CVE first: your \
                       lockfile pins urllib3 2.2.1, and the advisory landed on Monday.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::System("Nothing here merges without you. The fleet drafts; you decide."),
        ],
        read_ms: 8_400,
    },
    // ── 13 · THE CLOSE. What he paid, against what the same work costs on the API.
    //         ⛔ Published rates and his own meter. No quality comparison, on screen or implied.
    Beat {
        typed: &[Key::Type("what did that cost")],
        think_ms: 2_800,
        reply: &[
            Say::Table {
                name: "spend",
                columns: PRICE,
                rows: &[
                    "where it ran | tokens | what you paid",
                    "steps 1-4, Anthropic | 1.84M | $19.60, before the cap",
                    "plan and review, Codex | 0.41M | included in your subscription",
                    "steps 5-11, this machine | 3.10M | no vendor, no meter",
                    "billed by Estelle | \u{2014} | $0.0000",
                ],
            },
            Say::Wait(2_000),
            Say::Table {
                name: "spend",
                columns: PRICE,
                rows: &[
                    "the same task on the API | 4.94M | $52.65 at list price",
                    "what you actually paid | \u{2014} | $19.60",
                ],
            },
            Say::Wait(1_800),
            Say::System(
                "Your bill counts memory, never tokens. Ultra \u{b7} 250M memory \u{b7} $85 a month.",
            ),
        ],
        read_ms: 8_000,
    },
];
