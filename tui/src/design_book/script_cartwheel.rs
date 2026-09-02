//! 🎬 **FILM 2 · `cartwheel/storefront` — "the repo moved without me". DATA ONLY.**
//!
//! 🔴 **THE THESIS: TWO PEOPLE, TWO PLANS, ONE MEMORY.** Ravi comes back to a repository that moved
//! while he was away. Devon has been prompting his own agent on his own Claude plan for three hours,
//! inside the same file. Neither of them has spoken to the other. **Estelle reads what Devon TYPED**,
//! and that is the frame no competitor can build — every tool on the market reads his commits.
//!
//! ## 🔴 THE COVERAGE MARKERS, AND WHY THEY ARE IN THIS FILE
//!
//! Every beat below carries one of three markers. James will ask *"is that real?"*, and the answer
//! has to be in the room rather than in somebody's memory.
//!
//! * **✅ SHIPPED** — a surface a customer reaches from this terminal today.
//! * **🟡 BUILT, NOT SURFACED** — the server answers it today; the CLI has no door to ask.
//! * **⛔ NOT BUILT** — nothing answers it anywhere. **No beat in this film carries one.** Three
//!   did, and they were cut rather than shot: a numbered `/choose` menu (no option picker exists),
//!   a `● /slack` receipt (the terminal cannot post to Slack — the Slack bot is its own door), and
//!   a `who · branch · commits` table (`presence` carries no branch and no commit count).
//!
//! ### The one gap this film leans on, stated once
//!
//! Three beats draw `● /turns`. `POST /turns` is **shipped on the server** — Tier 1, no model on the
//! path, and for a team account `memory_routing.namespace_for` resolves the namespace to the TEAM
//! id, so a teammate's turns come back to any member's key. `Turn` carries `author`, `at` and the
//! text **exactly as typed**. What is missing is one line in `estelle-client/src/endpoint.rs`:
//! there is no `Turns` variant, so no CLI command can ask. That is the whole 🟡.
//!
//! ⚠️ **AND THE LOG HOLDS ONLY WHAT A PERSON TYPED.** `prepare_turn` is called once in production
//! (`scripts/estelle_server.py:4891`) with no `role`, so every row is `role="user"`. **The answers
//! Devon got are not in it.** The earlier version of beat 6 drew a table headed *"what they asked,
//! and what they were told"* — half of that table had no store behind it. Estelle now says the limit
//! out loud, on camera, in the film's centre beat. A product that refuses to tell you what it does
//! not know is the product; drawing the second half would have been the exact failure it sells against.
//!
//! ## What this film is NOT
//!
//! It was a production fire, which made it film 3's twin. The fire moved out. ⛔ There is no outage
//! here, and the rail is **alive but calm** — traffic moves, a PR opens at forty seconds, and
//! nothing ever goes red. Movement without alarm.

use crate::cols::Col;
use crate::design_book::session::{Beat, FleetFixture, FleetWorker, Key, Say};

/// `● /presence` — `member · since · files`, the exact fields `presence.py:80-87` reports.
static PRESENT: &[Col] = &[Col::l(10), Col::l(18), Col::l(44)];
/// `● /cards` — `title · folder · body`, the fields `commands.rs` prints for a knowledge card.
static CARD: &[Col] = &[Col::l(24), Col::l(12), Col::l(42)];
/// `● /memories` — `source · kind · chunks · trust`, the `MemoryItem` fields, in order.
static HELD: &[Col] = &[Col::l(20), Col::l(13), Col::r(9), Col::l(24)];
/// `● /turns` — `at · author · text`. Three of `Turn`'s five fields; `role` and `source` are
/// constant on every row this film draws, so printing them would be four wasted columns.
static TURN: &[Col] = &[Col::l(9), Col::l(8), Col::l(56)];
/// `● /presence` again, its `handoffs` half — `member · note · at`.
static NOTES: &[Col] = &[Col::l(9), Col::l(48), Col::l(14)];

/// 🔴 **HIS HALF OF THE FIX, RUNNING WHILE HE ASKS QUESTIONS AT IT.**
///
/// The founder's note on this film: *"while Estelle works autonomously the guy's asking questions,
/// but it's still trying to work autonomously and the guy keeps interrupting Estelle, but it still
/// is working overall."* Three snapshots of ONE roster at three clocks are how that is shown —
/// [`Key::Interrupt`] drops the middle one into the transcript **while his cursor is still in his
/// line**, so the work visibly does not wait for him.
///
/// ⚠️ **ONE ROSTER, SPREAD THREE TIMES.** Three hand-written tables could disagree about which
/// worker had which job, and film 1's pair already did once. Only `opens_at_s` differs here, so the
/// jobs cannot drift and the fleet can only move forward.
///
/// ⛔ **NOTHING IS KILLED.** `killed_at_s` is `None` on all three: this film has no outage, and a
/// red `×` in it would borrow film 3's beat.
const CAPTURE_HALF: FleetFixture = FleetFixture {
    batch: "A bounded retry on the capture path",
    // ⚠️ Named ONCE, on the roster line, where it is a real `FleetSnapshot` field. `orchestra_view`
    // has no per-worker model column and this film may not invent one.
    models: &["claude-opus-4-8"],
    narrator: "4 workers writing 9 assignments across billing/",
    total: 9,
    opens_at_s: 4,
    killed_at_s: None,
    workers: &[
        FleetWorker {
            action: Some("the retry wrapper on capture"),
            steps: 3,
            starts_s: 0,
            ends_s: Some(52),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("idempotency key on the payments table"),
            steps: 2,
            starts_s: 1,
            ends_s: Some(38),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("tests, billing/capture"),
            steps: 3,
            starts_s: 2,
            ends_s: Some(64),
            unknown_reason: None,
        },
        FleetWorker {
            action: Some("reading Devon's branch for the key name"),
            steps: 1,
            starts_s: 3,
            ends_s: Some(20),
            unknown_reason: None,
        },
    ],
};

/// The same four, forty seconds in — two finished while he was typing a question at them.
static FLEET_MID: FleetFixture = FleetFixture {
    opens_at_s: 40,
    ..CAPTURE_HALF
};

/// The same four, done. Every assignment closed, nothing killed.
static FLEET_DONE: FleetFixture = FleetFixture {
    opens_at_s: 70,
    ..CAPTURE_HALF
};

pub(crate) const CARTWHEEL: &[Beat] = &[
    // ── 1 · ✅ SHIPPED · `GET /presence` (`commands.rs:805`) + the grounded answer path.
    //
    //       He has been away. The repo has not. The table is `presence`'s own four keys — active
    //       rows carry `member · files · since`, overnight rows carry `member · at` and nothing
    //       else, which is why their file cells are honest em dashes rather than invented paths.
    Beat {
        typed: &[
            Key::Type("what happened while i was "),
            Key::Oops("of"),
            Key::Type("off"),
        ],
        think_ms: 3_400,
        reply: &[
            Say::Table {
                name: "presence",
                columns: PRESENT,
                rows: &[
                    "who | since | files in flight",
                    "Devon | 09:12 today | billing/capture.py, billing/keys.py",
                    "Priya | overnight 23:40 | \u{2014}",
                    "Marcus | overnight 22:10 | \u{2014}",
                    "Sam | overnight 22:55 | \u{2014}",
                ],
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Four days moved this repo. Priya merged #401 on Friday and Marcus reverted \
                       the Redis status cache on Monday. Both touch the file you opened this morning.",
                grounded: true,
            },
        ],
        read_ms: 7_000,
    },
    // ── 2 · ✅ SHIPPED · `GET /memory/cards` (`commands.rs:788`), which renders
    //       `title · folder · body` with provenance. He starts work, and the first team fact
    //       arrives WITHOUT him asking for it.
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
                name: "cards",
                columns: CARD,
                rows: &[
                    "card | folder | what it says",
                    "upstream retry budget | decisions | cap upstream retries at 2, no exceptions",
                    "the rate window | decisions | a 5-deep backoff crosses the insurer's minute",
                    "status cache | decisions | status must not depend on Redis \u{b7} Marcus, Monday",
                ],
            },
            Say::Wait(1_500),
            Say::Answer {
                text: "I raise this before you write it, not in review. A decision you meet after \
                       the diff costs you the afternoon.",
                grounded: true,
            },
        ],
        read_ms: 8_200,
    },
    // ── 3 · ✅ SHIPPED · the grounded answer path. He pushes back, which is the honest thing a
    //       person does, and the answer agrees with him.
    //
    //       ⛔ **THE `/choose` MENU IS GONE.** It drew three numbered options and a picker to
    //       select one. No numbered-option surface exists in this client — `commands.rs` has no
    //       `choose` and the composer has no option list — so the beat was a drawing of a feature.
    //       What replaced it is stronger and is real: Estelle asks him for the reason.
    Beat {
        typed: &[
            Key::Type("the capture path is different though, "),
            Key::Pause(900),
            Key::Type("stripe retries it for us"),
        ],
        think_ms: 4_200,
        reply: &[
            Say::Answer {
                text: "You may be right. Priya wrote that decision about the insurer fetcher. The \
                       capture path talks to Stripe, and Stripe retries for three days on its own.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Answer {
                text: "Tell me why the capture path differs. I keep your reason on this file, so \
                       the next person reads it instead of guessing.",
                grounded: true,
            },
        ],
        read_ms: 6_400,
    },
    // ── 4 · 🟡 BUILT, NOT SURFACED · `POST /turns` (`api_turns.py:18`) answers this today with no
    //       model on the path. `estelle-client/src/endpoint.rs` has no `Turns` variant, so no
    //       command can ask for it.
    //
    //       🔴 **THE MECHANISM IS DEMONSTRATED ON HIM BEFORE IT IS USED ON DEVON.** He watches his
    //       own sentence land, word for word, casing and all — then two beats later the same door
    //       hands him Devon's. A viewer who saw the first frame cannot call the second one a trick.
    //
    //       ⚠️ The text is NOT tidied. `turn_log.py:prepare_turn` strips the ends and stops:
    //       *"a store that tidies what was said cannot later prove what was said."*
    Beat {
        typed: &[Key::Type(
            "stripe retries capture for 3 days on the same idempotency key, so retrying here risks a double charge",
        )],
        think_ms: 3_600,
        reply: &[
            Say::Table {
                name: "turns",
                columns: TURN,
                rows: &[
                    "at | who | what was typed, word for word",
                    "11:26 | ravi | stripe retries capture for 3 days on the same idempotency key, so retrying here risks a double charge",
                ],
            },
            Say::Wait(1_700),
            Say::Answer {
                text: "Kept, word for word. The next person who asks about capture retries gets \
                       your sentence rather than a summary of it.",
                grounded: true,
            },
        ],
        read_ms: 6_600,
    },
    // ── 5 · ✅ SHIPPED · `GET /memories` (`commands.rs:798`) renders `MemoryItem` as
    //       `source · kind · chunks · trust`, and `slack` is a real ingested memory kind
    //       (`memory_routing.ACCOUNT_LEVEL_KINDS`).
    //
    //       🔴 A PROPOSAL ALREADY PARKED IN SLACK. He is about to suggest a thing his own teammate
    //       suggested on Thursday, to silence. Nobody reads four days of #eng, and this is the cost
    //       of that: two people design the same thing a week apart.
    Beat {
        typed: &[Key::Type(
            "we should probably just centralise all the retry config",
        )],
        think_ms: 4_000,
        reply: &[
            Say::Table {
                name: "memories",
                columns: HELD,
                rows: &[
                    "source | kind | chunks | trust",
                    "slack:#eng | slack | 214 | externally authored",
                    "docs/adr | decision | 41 | team authored",
                    "git:storefront | commit | 1,308 | team authored",
                ],
            },
            Say::Wait(2_000),
            Say::Answer {
                text: "Sam proposed one retry policy module in #eng on Thursday. Three people \
                       reacted and nobody replied. Your design already has him behind it.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::System(
                "No ticket exists for it. Open one, and Sam gets an answer four days late instead of never.",
            ),
        ],
        read_ms: 8_200,
    },
    // ── 6 · 🔴🔴 THE CENTRE OF THE FILM, AND THE FRAME NOBODY ELSE CAN BUILD.
    //
    //       🟡 BUILT, NOT SURFACED · `POST /turns`, team-scoped. Devon is on his own Claude plan in
    //       his own terminal; his key and Ravi's resolve to one team namespace, so his prompts come
    //       back here. Every tool on the market can read Devon's COMMITS. This reads what he TYPED.
    //
    //       ⚠️ **AND THE SECOND HALF DOES NOT EXIST, SO ESTELLE SAYS SO.** The log stores
    //       `role="user"` rows only. Devon's ANSWERS are not in it. The old version of this beat
    //       drew them anyway, under a column headed "what they were told". The refusal is now the
    //       strongest line in the film.
    Beat {
        typed: &[Key::Type(
            "what has devon been asking about the capture retries",
        )],
        think_ms: 4_600,
        reply: &[
            Say::Table {
                name: "turns",
                columns: TURN,
                rows: &[
                    "at | who | what was typed, word for word",
                    "09:40 | devon | where do we bound the capture retries? i cant find a cap anywhere",
                    "09:52 | devon | does stripe retry the capture on its own or do i need to",
                    "10:07 | devon | adding the idempotency key on my branch, is that the right place",
                    "10:41 | devon | whats the column type for idempotency_key on payments",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "Devon asked where the capture retries are bounded at 09:40. He adds the \
                       idempotency key on his branch, which is the half that makes your change safe.",
                grounded: true,
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "I have his questions. I do not have the answers he got, so I will not tell \
                       you what he was told.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Table {
                name: "presence",
                columns: PRESENT,
                rows: &[
                    "who | since | files in flight",
                    "Devon | 09:12 today | billing/capture.py, billing/keys.py",
                ],
            },
            Say::Wait(1_400),
            Say::Answer {
                text: "Devon is inside billing/capture.py right now. You are both an hour from a \
                       merge conflict neither of you has heard about.",
                grounded: true,
            },
        ],
        read_ms: 10_400,
    },
    // ── 7 · 🟡 BUILT, NOT SURFACED · `POST /turns` again, on a different question.
    //
    //       🔴 **THE LINE THE FOUNDER KEPT.** Nobody disagrees — not because the room is polite,
    //       but because two people already answered this on Monday and their questions are on
    //       record. ⚠️ Marcus's REVERT is a commit; what he was TOLD is not stored, and the answer
    //       says which of the two it is reading.
    Beat {
        typed: &[Key::Type("does anyone disagree with the postgres plan")],
        think_ms: 4_200,
        reply: &[
            Say::Table {
                name: "turns",
                columns: TURN,
                rows: &[
                    "at | who | what was typed, word for word",
                    "Mon 10:22 | marcus | what should the status cache use if not redis",
                    "Mon 11:30 | marcus | reverting the redis status cache, main is clean",
                    "Mon 14:10 | sam | does postgres add latency on the capture path",
                ],
            },
            Say::Wait(2_400),
            Say::Answer {
                text: "Nobody disagrees. Marcus asked your question on Monday and reverted the \
                       Redis cache an hour later. Sam asked the latency question you would ask next.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Answer {
                text: "I read what they typed. Marcus's revert is in the repo, so he acted on \
                       whatever he was told.",
                grounded: true,
            },
        ],
        read_ms: 8_400,
    },
    // ── 8 · ✅ SHIPPED · `POST /work` + `POST /orchestra` + `orchestra_view`, the same renderer the
    //       live session and catalog screen 20 call. The work starts, and it does not need him.
    Beat {
        typed: &[Key::Type("ok write my half")],
        think_ms: 3_400,
        reply: &[
            Say::Command {
                name: "work",
                lines: &[
                    "capture    billing/capture.py \u{b7} bounded retry \u{b7} plan accepted",
                    "apply      write_sandbox \u{b7} open_pr \u{b7} nothing merges here",
                ],
            },
            Say::Wait(1_400),
            Say::Orchestra(&CAPTURE_HALF),
            Say::Wait(1_600),
            Say::Answer {
                text: "Four workers have it. Ask me anything while they run.",
                grounded: true,
            },
        ],
        read_ms: 7_600,
    },
    // ── 9 · 🔴 THE FOUNDER'S NOTE, LITERALLY: HE INTERRUPTS IT, AND IT KEEPS WORKING.
    //
    //       ✅ SHIPPED · the same two surfaces as beat 8. The worker table lands **mid-word**,
    //       through [`Key::Interrupt`], so the cursor never leaves his line and the fleet visibly
    //       moves while he is still asking about something else.
    Beat {
        typed: &[
            Key::Type("while thats going "),
            Key::Pause(1_200),
            Key::Type("what did priya actually change in the fetch"),
            // The four workers report in. Nothing below touches his composer.
            Key::Interrupt(&[Say::Orchestra(&FLEET_MID)]),
            Key::Pause(1_600),
            Key::Type("er"),
        ],
        think_ms: 3_800,
        reply: &[
            Say::Answer {
                text: "Priya put a two-deep retry with a backoff in claims/upstream.py. She capped \
                       it because the insurer counts requests per minute, not per call.",
                grounded: true,
            },
            Say::Wait(1_800),
            Say::System(
                "Nothing stopped for that question. Two of the four workers finished while you typed.",
            ),
        ],
        read_ms: 7_400,
    },
    // ── 10 · ✅ SHIPPED · he interrupts a second time, the fleet finishes on camera, and the gate
    //        runs on what it wrote. Two interrupts rather than one: once is an accident, twice is
    //        how the tool behaves.
    Beat {
        typed: &[
            Key::Type("and is sams runbook worth reading before i "),
            Key::Interrupt(&[Say::Orchestra(&FLEET_DONE)]),
            Key::Pause(1_400),
            Key::Type("merge"),
        ],
        think_ms: 3_600,
        reply: &[
            Say::Answer {
                text: "Sam wrote it on Monday for the new webhook. Read it after this merges, \
                       because it covers the retry path you are changing.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::Command {
                name: "work",
                lines: &[
                    "capture    a bounded retry around the capture call \u{b7} written",
                    "gate       0 findings",
                ],
            },
        ],
        read_ms: 7_400,
    },
    // ── 11 · ✅ SHIPPED · `POST /gate` and the grounded answer path.
    //
    //        🔴 THE MEMORY CATCH. Film 1's near-miss shape with a team fact instead of a solo one:
    //        the code compiles, the gate passes it, and only the record of what this TEAM already
    //        tried catches it.
    Beat {
        typed: &[Key::Type("anything wrong with it")],
        think_ms: 4_000,
        reply: &[
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
        read_ms: 8_200,
    },
    // ── 12 · ✅ SHIPPED · `POST /work`, `POST /gate`, `POST /review`. He fixes it, and the two
    //        halves meet without either of them designing the other's.
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
                text: "Your half is ready. It writes the same key Devon's branch writes, so the two \
                       branches merge without a second design.",
                grounded: true,
            },
        ],
        read_ms: 6_800,
    },
    // ── 13 · 🟡 BUILT, NOT SURFACED · the READ half is shipped (`GET /presence`); the WRITE half is
    //        `POST /presence` (`api_console.py:111-127`), and `endpoint.rs` declares `Presence` as
    //        `[Get]` only, so this terminal can read a handoff note and cannot leave one.
    //
    //        🔴 **THE BEAT WHERE IT STOPS BEING SINGLE-PLAYER — AND THE LIMIT IS ON SCREEN.**
    //        The founder asked for *"three members have been alerted, you're the only one here"*.
    //        ⛔ **NOTHING IN THE PRODUCT ALERTS A PERSON.** No DM, no per-member email, no push;
    //        the only human-facing push is a Slack CHANNEL post that needs a connected app and a
    //        per-channel opt-in, and monitor signals fire it rather than presence. So Estelle says
    //        what it did and what it did not do, and the moment survives intact: nobody was paged,
    //        and three people still come back to his reasoning.
    Beat {
        typed: &[
            Key::Burst("tell the others "),
            Key::Type("what we worked out"),
        ],
        think_ms: 4_400,
        reply: &[
            Say::Table {
                name: "presence",
                columns: NOTES,
                rows: &[
                    "who | the note they left | at",
                    "Sam | retry policy module, no ticket yet | Thu 16:20",
                    "Marcus | redis is out of the status path, keep it out | Mon 10:31",
                ],
            },
            Say::Wait(2_000),
            Say::Answer {
                text: "Marcus, Priya and Sam worked overnight and none of them is here now. You are \
                       the only person active on this repo.",
                grounded: true,
            },
            Say::Wait(1_800),
            Say::Answer {
                text: "Your capture decision sits on this file beside theirs. Nobody was paged, and \
                       nobody will be. They read it when they open the file.",
                grounded: true,
            },
            Say::Wait(1_600),
            Say::System("Three people come back to a note that names your half and Devon's."),
        ],
        read_ms: 8_800,
    },
    // ── 14 · ✅ SHIPPED · the grounded answer path. The close. He knows what the team knows, and
    //        none of it came from a meeting.
    Beat {
        typed: &[Key::Type("anything else waiting on me")],
        think_ms: 3_600,
        reply: &[
            Say::Answer {
                text: "One thing. Priya opened a rate-limit branch on Friday and it is waiting on \
                       your review. Everything else moves without you.",
                grounded: true,
            },
            Say::Wait(1_800),
            Say::System(
                "You caught up on four days in nine minutes, and nobody wrote you a summary.",
            ),
        ],
        read_ms: 7_000,
    },
];
