//! 🎬 **FILM 3 · `cartwheel/storefront` — "it told me before I noticed". DATA ONLY.**
//!
//! 🔴 **THE THESIS: HE FINDS OUT FROM ESTELLE, NOT FROM A DASHBOARD AND NOT FROM A CUSTOMER.**
//! Checkout starts failing at 23:04. He is inside an unrelated question, typing. He does not look
//! at a dashboard, and nobody pages him. The product interrupts him — once, with a reason — and
//! then keeps talking, and then fixes it while he watches.
//!
//! ## The four things that make this film work
//!
//! 1. **The wait.** The rail climbs from 38 seconds and he does not react to it. Nothing is said
//!    for twenty seconds. **The silence is the beat** — it is what makes the interrupt land as a
//!    thing the product noticed rather than a thing the film cut to.
//! 2. **The interrupt is MID-SENTENCE.** Not after enter. The cursor is still inside his line
//!    ([`Key::Interrupt`]), and the trust line is his own wording: *"I would not normally
//!    interrupt."*
//! 3. 🔴 **AND THEN IT SENDS THE NEXT MESSAGE ITSELF.** The founder's note, verbatim: *"I want
//!    Estelle to autonomously just send its next message instead of waiting for him to type a
//!    message, if it finds an unresolved error."* So beat 3 carries **four** Estelle turns and
//!    **zero** prompts: the banner, then what a customer sees, then the cause to the line, then
//!    the plan it has already started on. **He never types "fix it" — nobody asks it to.** The two
//!    beats that used to carry *"what is actually failing"* and *"why"* are gone as questions and
//!    survive as things it volunteered, which is the whole argument in one structural change.
//! 4. 🔴 **HIS LINE COMES BACK.** [`Key::Park`] lifts the half-written question out, and
//!    [`Key::Restore`] puts it back at the end, to the character. **An interrupt that costs you
//!    your sentence is an interruption; one that gives the sentence back is an assistant.**
//!
//! ## He keeps interrupting IT, and the work does not stop
//!
//! The founder again: *"while Estelle works autonomously the guy's asking questions, but it's
//! still trying to work autonomously and the guy keeps interrupting Estelle."* So in beats 4 and 5
//! the worker table **opens inside his half-typed sentence** — a `Key::Interrupt` carrying
//! [`Say::Orchestra`] — and then advances on its own clock underneath the answer to his question.
//! That is not a picture of overlap; the rows genuinely move while he types, because
//! [`crate::orchestra_view`] is repainted every frame from `fleet_snapshot`.
//!
//! ⛔ **NO `/work` RECEIPT, AND NO PER-WORKER MODEL CELL.** This film used to draw its fleet as a
//! hand-typed `Say::Command { name: "work" }` whose rows read `checkout   serve/checkout.py ·
//! claude-opus-4-8` — **a per-worker model column, which is the one cell the product refuses to
//! draw**, because `FleetAgent` carries neither a model nor a cost. The model is still on screen
//! on the line where it is true: `models · …`, the fleet's real roster field.
//!
//! ## The team beat is `/presence`, and it is shipped
//!
//! The founder asked for *"three members have been alerted. You're the only one here"* →
//! *"we're gonna fix it together."* ✅ `/presence` answers the second half exactly, and is real on
//! both sides: `GET /presence` at `serve/api.py:1908`, `Endpoint::Presence` in the client,
//! `commands.rs:1799` for the rendering, with `presence_reply_renders_active_overnight_files_and_handoffs`
//! pressing the shape. ⚠️ The first draft of this beat drew a `/team` roster instead and had
//! Estelle say *"nothing here measures who is awake"* — **a refusal of a capability the product
//! has.** Understating the system is the same defect as overclaiming it, and it hides better.
//!
//! ⛔ **BUT "ALERTED" IS CUT, AND FILM 2'S LANE IS WHY.** They measured it: there is no DM, no
//! per-member email and no push anywhere in the product. The only human-facing push is a Slack
//! **channel** post, which needs a connected app and a per-channel opt-in and is fired by monitor
//! signals. An outage IS a monitor signal, so the post is defensible — **being read is not.** So
//! the line says the mechanism and its limit in one sentence: *"which is a channel post and not a
//! page. Nobody was woken."* The founder's beat survives; the claim it implied does not.
//!
//! ## And the gate refuses
//!
//! The repair it drafts under pressure is **wrong**, and the gate says so before anything reaches
//! production — **while he is asking about something else**. **A gate that only ever passes is a
//! gate nobody believes.** Film 1's refusal frame is the shape; the content here is this outage's.
//!
//! ⛔ **NAME THE USER-VISIBLE FACT, NOT THE METRIC.** Not `error rate 34%`. *"142 checkouts failed
//! since 23:04. 38 retried and failed again."* Those are customers who could not buy something.
//!
//! ## ⚠️ THE INCIDENT WINDOW IS OWNED BY `rail.rs`, NOT BY THIS FILE
//!
//! `rail::profile` gives film 3 `incident: Some((20, 118))`: the outage ramps from 20 s, the three
//! services drop when severity passes 0.55 (**t ≈ 32 s**), and they come back when it falls under
//! it again (**t ≈ 126 s**). Those two numbers pin two beats of this script to the clock:
//!
//! * the interrupt must land **after ~34 s**, or Estelle announces an outage over a `3/3 up` rail;
//! * `apply it` must submit at **~116 s**, so the recovery on the rail and the `recovered 23:15`
//!   table arrive together rather than forty seconds apart.
//!
//! 🔴 **THAT IS WHY THE INCIDENT IS DENSE AND THE TAIL IS NOT.** Everything between 34 s and 116 s
//! is competing for eighty-two seconds that this file cannot extend, so the local-model beat lives
//! **after** the recovery, where the clock is free — and it lives there honestly, on the small
//! change he originally asked about, rather than being squeezed into the fire.

use crate::cols::Col;
use crate::design_book::session::{Beat, FleetFixture, FleetWorker, GateFixture, Key, Say};

static CITE: &[Col] = &[Col::l(30), Col::l(42)];
static FAILING: &[Col] = &[Col::l(26), Col::r(9), Col::l(34)];
static CAUSE: &[Col] = &[Col::l(24), Col::l(22), Col::l(28)];
static STEP: &[Col] = &[Col::l(2), Col::l(28), Col::l(40)];
static SPEND: &[Col] = &[Col::l(24), Col::r(10), Col::l(32)];
/// The plan it streams back while it is already working — the founder's *"start sending back the
/// plan of what it's doing"*. ⚠️ The last row is the propose-only promise, in the plan itself.
static PLAN: &[Col] = &[Col::l(2), Col::l(30), Col::l(38)];

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

/// 🔴 **THE FLEET IS THE PRODUCT'S OWN RENDERER, AND IT OPENS MID-FLIGHT.**
///
/// `opens_at_s` is why the table is not sitting at `0/9` when he first sees it: the batch has been
/// running for six seconds already, because **Estelle started it without being asked**. A fleet
/// that opens at zero reads as *nothing is working*, which is the founder's complaint about the
/// local models word for word, and it would have been true of this one too.
static REPAIR: FleetFixture = FleetFixture {
    batch: "Move three call sites off the removed Stripe fields",
    // ⚠️ ONE MODEL, NAMED ONCE, ON THE LINE WHERE IT IS TRUE. `orchestra_view` renders this as
    // `models · claude-opus-4-8` on the frame's second row. There is no per-worker model cell and
    // there never has been — the film used to invent ten of them.
    models: &["claude-opus-4-8"],
    narrator: "4 workers on 9 assignments across checkout and webhooks",
    total: 9,
    opens_at_s: 6,
    killed_at_s: None,
    workers: &[
        FleetWorker {
            action: Some("serve/checkout.py:88, the removed payment field"),
            steps: 3,
            starts_s: 0,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("serve/checkout.py:140, the source field"),
            steps: 2,
            starts_s: 1,
            ends_s: Some(15),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("serve/webhooks.py:57, the renamed event"),
            steps: 2,
            starts_s: 2,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("reading what stripe 12.4.0 does define"),
            steps: 2,
            starts_s: 0,
            ends_s: Some(12),
            unknown_reason: None,
        },
    ],
};

/// The same four, on the repair that survives the gate. ⚠️ A **new batch**, not the old one with a
/// tick on it: the first repair was refused, so its assignments are not partly reusable and the
/// table must not pretend they are.
static ROUND_TWO: FleetFixture = FleetFixture {
    batch: "Second repair \u{b7} automatic_payment_methods",
    models: &["claude-opus-4-8"],
    narrator: "4 workers on 9 assignments, restarted after the gate refused",
    total: 9,
    opens_at_s: 5,
    killed_at_s: None,
    workers: &[
        FleetWorker {
            action: Some("automatic_payment_methods on the session"),
            steps: 3,
            starts_s: 0,
            ends_s: Some(14),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("payment_method in place of source"),
            steps: 2,
            starts_s: 0,
            ends_s: Some(11),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("the webhook event, under its new name"),
            steps: 2,
            starts_s: 1,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("1,204 tests, in a clone of the repo"),
            steps: 2,
            starts_s: 2,
            ends_s: None,
            unknown_reason: None,
        },
    ],
};

/// 🔴 **THE LOCAL MODELS MOVE, AND THEY MOVE ON A JOB THAT SUITS THEM.**
///
/// The founder asked for two things here: the choice *"should we use a local model to fix it?"*
/// made out loud, and then **local models showing real progress rather than standing still**. The
/// choice is made during the fire, in beat 4, and the answer is *no* — a down checkout is the
/// wrong place to trade latency for hardware. The progress is here, after the recovery, on the
/// two-line sweep change he asked about at 23:07 and never got to.
///
/// ⚠️ **NAMES FROM THE BUNDLED CATALOGUE, VERBATIM.** These are two of the rows in
/// [`crate::design_book::session::LOCAL_FLEET`], which `named_model` resolves exactly and refuses
/// to fuzzy-match. A film that invents a model name is inventing a capability.
static LOCAL_SWEEP: FleetFixture = FleetFixture {
    batch: "Split the sweep timeout into an upload budget and a poll budget",
    models: &[
        "Qwen/Qwen2.5-Coder-32B-Instruct",
        "Qwen/Qwen2.5-Coder-14B-Instruct",
    ],
    narrator: "3 workers on 6 assignments, every one of them on this machine",
    total: 6,
    opens_at_s: 4,
    killed_at_s: None,
    workers: &[
        FleetWorker {
            action: Some("serve/sweep.py:112, two constants in place of one"),
            steps: 2,
            starts_s: 0,
            ends_s: Some(13),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("serve/sweep.py:203, the poll reads its own"),
            steps: 2,
            starts_s: 1,
            ends_s: None,
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("tests for the poll budget"),
            steps: 2,
            starts_s: 2,
            ends_s: None,
            unknown_reason: None,
        },
    ],
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
    //
    //       ⚠️ **THIS BEAT ENDS AT ~27.8 s AND THAT NUMBER IS LOAD-BEARING.** `rail::profile`
    //       ramps the outage from 20 s and drops the services at ~32 s. Shortening beats 1 and 2
    //       moves the interrupt below that line, and Estelle then announces a dead checkout over
    //       a rail that still reads `3/3 up`.
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
    // ── 3 · 🔴 THE TURN, AND THE FOUNDER'S CENTRAL NOTE. The rail has been climbing since 38
    //       seconds and he has not looked at it.
    //
    //       THE INTERRUPT ARRIVES MID-WORD. He is inside `bat` of `batch` when it lands, the
    //       cursor never leaves his line, and the first thing it says is a fact about CUSTOMERS.
    //
    //       🔴 **AND THEN IT DOES NOT WAIT.** Three more turns arrive with nothing typed between
    //       them: what a customer sees, the cause to the line, and the plan it has already begun.
    //       He types nothing at all for the first three of them. **The beats that used to be his
    //       questions — "what is actually failing" and "why" — are the same content, volunteered.**
    Beat {
        typed: &[
            Key::Type("whats the cleanest way to bound the upload "),
            Key::Pause(2_400),
            Key::Type("and the poll "),
            Key::Pause(1_900),
            Key::Type("separately without breaking the retry bat"),
            // ── turn 1. The cursor is still in his sentence. Nothing below touches it.
            Key::Interrupt(&[Say::Failure([
                "142 checkouts failed since 23:04. 38 retried and failed again.",
                "Checkout, catalog and webhooks are all returning 503. The whole line is down.",
                // ⚠️ This used to end *"You are here, so I will ask."* and then it never asked —
                // it started. A trust line that describes a turn the film does not take is worse
                // than no trust line, so the second half now says what actually happens.
                "I would not normally interrupt. Production is down.",
            ])]),
            // He reads it. His hands are off the keyboard, and nobody presses enter.
            Key::Pause(2_800),
            // ── turn 2. NOBODY ASKED. This is what the old beat 4 asked for.
            Key::Interrupt(&[
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
                Say::Wait(1_500),
                Say::Answer {
                    text: "Every checkout since 23:04 failed. Nobody bought anything for seven \
                           minutes.",
                    grounded: true,
                },
            ]),
            Key::Pause(2_200),
            // ── turn 3. Still nobody asked. This is what the old beat 5 asked for.
            Key::Interrupt(&[
                Say::Answer {
                    text: "Stripe shipped API version 2026-09-01 four hours ago. Three call sites \
                           in this repo read fields that version removed.",
                    grounded: true,
                },
                Say::Wait(900),
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
                Say::Wait(1_100),
                Say::System(
                    "Your lockfile pins stripe 12.4.0, and the account is on the new version. The library did not move; the account did.",
                ),
            ]),
            Key::Pause(1_900),
            // His half-written question lifts out of the composer, and it is held.
            Key::Park,
            // ── turn 4. 🔴 IT STREAMS BACK THE PLAN IT IS ALREADY WORKING FROM, and the last row
            //    of that plan is the promise it will not break.
            Key::Interrupt(&[
                Say::Answer {
                    text: "I kept your question about the sweep budget and I will put it back \
                           when this is over. I start on checkout now.",
                    grounded: true,
                },
                Say::Wait(800),
                Say::Table {
                    name: "plan",
                    columns: PLAN,
                    rows: &[
                        "  | step | where",
                        "\u{2713} | read the failed requests | 271 of them, eleven minutes",
                        "\u{2713} | find what moved | the Stripe account, not the library",
                        "\u{25b6} | rewrite three call sites | serve/checkout.py, serve/webhooks.py",
                        "\u{25a1} | gate every diff | this repo's symbol graph",
                        "\u{25a1} | run the suite in a clone | never in your tree",
                        "\u{25a1} | stop, and wait for you | nothing merges without you",
                    ],
                },
            ]),
            Key::Pause(1_600),
            // ── and only NOW does he type, and it is a question rather than an order.
            Key::Type("who else is awake"),
        ],
        think_ms: 2_400,
        reply: &[
            // ✅ **THIS IS A SHIPPED SURFACE AND IT WAS ALMOST FAKED.** The first draft of this
            // beat drew a `/team` roster and had Estelle say *"nothing here measures who is
            // awake"* — a refusal of a capability the product HAS. `/presence` is
            // `GET /presence` (`serve/api.py:1908`, handler `api_console.py:100`),
            // `Endpoint::Presence` in the client, and `commands.rs:1799` renders exactly these
            // rows: the active count, one line per active member with the files they hold, an
            // `overnight` line per member, then files in flight.
            //
            // 🔴 **UNDERSTATING THE SYSTEM IS THE SAME DEFECT AS OVERCLAIMING IT**, and it is the
            // more dangerous one, because a false modesty reads as caution and nobody audits
            // caution. The check that caught it is the one to repeat: grep the COMMAND TABLE
            // before writing a sentence that says the product cannot do something.
            Say::Command {
                name: "presence",
                lines: &[
                    "1 active  |  3 overnight",
                    "you@cartwheel.shop  |  since 22:41  |  serve/sweep.py",
                    "overnight  priya@cartwheel.shop  |  at 02:10",
                    "overnight  devon@cartwheel.shop  |  at 01:55",
                    "overnight  marcus@cartwheel.shop  |  at 03:20",
                    "files in flight  serve/sweep.py",
                ],
            },
            Say::Wait(1_300),
            Say::Answer {
                text: "One session is open on this repo and it is yours. The outage went to #eng \
                       at 23:05, which is a channel post and not a page. Nobody was woken. You \
                       are the one here. We fix it together.",
                grounded: true,
            },
        ],
        read_ms: 7_400,
    },
    // ── 4 · 🔴 HE INTERRUPTS IT, AND THE WORK DOES NOT STOP.
    //
    //       The founder: *"the guy keeps interrupting Estelle, but it still is working overall."*
    //       So the worker table opens **inside his half-typed sentence** and then advances under
    //       the answer to his question — the rows are a function of the frame clock, so they are
    //       genuinely moving while he types, not a still picture of a fleet.
    //
    //       🔴 **AND HIS QUESTION IS THE FOUNDER'S OWN: should we use a local model?** The answer
    //       is no, with a reason and a promise. A product that says yes to every question is not
    //       making a decision; it is agreeing.
    //
    //       🔴 **THE GATE REFUSES IN THE SAME BREATH, UNASKED.** He is asking about hardware; the
    //       repair Estelle drafted under pressure is wrong, and it says so rather than finishing
    //       the conversation first.
    Beat {
        typed: &[
            Key::Type("should we use a local model for "),
            // The batch has been running six seconds already. It started without him.
            Key::Interrupt(&[Say::Orchestra(&REPAIR)]),
            Key::Type("this"),
        ],
        think_ms: 3_000,
        reply: &[
            Say::Answer {
                text: "Not for this one. Checkout is down, and the shortest path to a correct fix \
                       wins. Your machine takes the sweep change afterwards, where nothing is on \
                       fire.",
                grounded: true,
            },
            Say::Wait(2_000),
            Say::Gate(&BAD_REPAIR),
            Say::Wait(2_600),
            Say::Answer {
                text: "That repair was wrong, and nothing left the sandbox. The method it reached \
                       for reads like it should exist and does not.",
                grounded: true,
            },
        ],
        read_ms: 9_200,
    },
    // ── 5 · the second attempt, and he interrupts it again. The loop closes.
    Beat {
        typed: &[
            Key::Type("is checkout still "),
            Key::Interrupt(&[Say::Orchestra(&ROUND_TWO)]),
            Key::Type("down"),
        ],
        think_ms: 2_800,
        reply: &[
            Say::Answer {
                text: "Yes. 61 more failed while we talked. Four workers finished the second \
                       repair, and the gate has nothing on it.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Table {
                name: "gate",
                columns: STEP,
                rows: &[
                    "\u{2713} | symbols | every name resolves in stripe 12.4.0",
                    "\u{2713} | arity | 6 calls checked against their definitions",
                    "\u{2713} | dependencies | no known advisory",
                    "\u{2713} | suite | 1,204 tests in a clone, never your tree",
                ],
            },
            Say::Wait(1_400),
            // 🔴 **THE ENDING HE ALREADY LIKED. DO NOT SOFTEN IT.** This is propose-only on
            // screen, and it is the honest version of the product.
            Say::Answer {
                text: "Ready to apply. It is your call, and I will not merge it for you.",
                grounded: true,
            },
        ],
        read_ms: 9_000,
    },
    // ── 6 · 🔴 IT COMES BACK, ON CAMERA. A film that shows an outage and never shows it end has
    //       told half a story, and the half it left out is the one we sell.
    //
    //       ⚠️ **THIS BEAT'S SUBMIT MUST LAND AT ~116 s.** `rail::profile` returns the three
    //       services to `up` at t ≈ 126 s, which is where the `recovered 23:15` table below sits.
    //       A beat added in front of this one moves the fix later than the recovery it causes.
    Beat {
        typed: &[Key::Type("apply it")],
        think_ms: 3_400,
        reply: &[
            Say::Command {
                name: "apply",
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
    // ── 7 · 🔴 HIS SENTENCE COMES BACK. This is the beat that sells the interrupt, and it is
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
        think_ms: 3_200,
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
        read_ms: 6_600,
    },
    // ── 8 · 🔴 THE PROMISE FROM BEAT 4 IS PAID, AND THE LOCAL MODELS MOVE.
    //
    //       `Say::LocalFleet` is **the one block in any film that is not a fixture**: every row is
    //       measured by `estelle_machine` on the laptop the film is recorded on, and the estimate
    //       notice under the table is the library's own sentence rather than our paraphrase.
    //
    //       Then three local workers take the change, in the product's own renderer, advancing.
    //       ⛔ **NO COMPARATIVE CLAIM.** Not "as good as", not a percentage. The honest line is
    //       that the check does not change, and that is the only claim on screen.
    Beat {
        typed: &[Key::Type("do the sweep change on my machine then")],
        think_ms: 3_000,
        reply: &[
            Say::LocalFleet,
            Say::Wait(1_800),
            Say::Orchestra(&LOCAL_SWEEP),
            Say::Wait(2_600),
            Say::Table {
                name: "gate",
                columns: STEP,
                rows: &[
                    "\u{2713} | symbols | UPLOAD_TIMEOUT and POLL_TIMEOUT both resolve",
                    "\u{2713} | arity | 2 call sites checked against their definitions",
                    "\u{2713} | suite | the sweep tests, in a clone",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "Two budgets, split, and the same gate ran on it. A model on your machine is \
                       not a better model. It is here, and nothing about the check changes.",
                grounded: true,
            },
        ],
        read_ms: 7_000,
    },
    // ── 9 · the close.
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
                    "the sweep change, local | 0.09M | $0.00",
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
