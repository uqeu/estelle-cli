//! 🎬 **THE FILMS. THIS FILE IS DATA — REORDER IT WITHOUT TOUCHING ANY OTHER.**
//!
//! Every beat is a `const`: what gets typed, how long Estelle takes, what comes back, and how long
//! it sits. No branch of [`crate::demo_session`] reads a beat's CONTENT to decide what to do with
//! it, so moving a beat, deleting one, or rewriting a line is an edit here and nowhere else.
//!
//! ## Film 1 has to work with the sound off
//!
//! The founder's bar: *"James should understand what Estelle does by the end of even the FIRST
//! demo."* So film 1 is ordered to say nine things in nine beats, each visible without narration —
//! cited answers about his own code · a refusal it can justify · a credential stopped before it
//! leaves the machine · a provider dying without losing the plan · which provider · a one-line swap
//! that resumes · the team's own decision contradicting him · a PR that does not merge · the bill.
//! `film_one_stands_alone_with_the_sound_off` pins each of those to a needle, so trimming a beat to
//! fit the runtime is a test failure rather than a quiet loss of the film's whole argument.
//!
//! ## [`dress`] is why the right-hand rail has something on it
//!
//! 🔴 **`App::new` LEAVES THE RAIL EMPTY, AND AN EMPTY RAIL IS WHAT HE SAW.** The five bands read
//! `prod_overview`, `prod_agent_health`, `prod_issues`, `prod_github_status` and
//! `prod_proposed_prs`; with all of them `None` the rail draws five rules over `no read yet · GET
//! /agent/health` and reads as broken. ⚠️ **And `client: None` is a separate, louder failure** —
//! `live_renderer.rs:1492` returns `Live Monitor unavailable. / Run /login here.` before it looks at
//! the overview at all, so dressing the overview without dressing the client changes nothing. The
//! client here is pointed at a dead port (`127.0.0.1:9`), copied from the gallery's own home
//! fixture: it constructs, it is never dialled, and no film makes a network call.

use crate::cols::Col;
use crate::design_book::session::{Beat, Film, Key, Say};

/// Strings that exist ONLY in a fixture, used by the gate test in both directions.
///
/// 🔴 The positive half is why this is a list and not an `assert!(!contains)`: an absence check
/// passes identically over a player that drew nothing at all.
pub(crate) const FIXTURE_NEEDLES: &[&str] = &[
    "claims/upstream.py:141",
    "0009-upstream-retry-budget.md:31",
    "PR #412",
];

/// 🔴 **WHAT A BEAT SAYS WHEN NOBODY ASKED FOR FIXTURES.**
///
/// Every number in these films is invented. `Say::Screen` goes through `design_book::render` and is
/// gated; the film's own prose and tables are not, and the gate test caught them leaking
/// `claims/upstream.py:141` on a default run. **A second door into the fixtures is exactly the
/// defect the book's one-owner rule exists to prevent**, so the player swaps a beat's whole reply
/// for this — on the same timeline, so the runtime he rehearses against does not move.
///
/// ⚠️ This is NOT the on-screen banner he asked to have removed. That banner stamped
/// *"design fixture · the numbers on this screen were NOT measured"* across every frame of a vision
/// film, and it is gone. This is what the film plays when the fixture flag is OFF — which is the
/// default, and which is the actual safety property. The flag stayed; the watermark went.
pub(crate) const SHUT: &[Say] = &[Say::System(
    "fixtures are off. play the film with  estelle demo --session 1 --demo",
)];

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
const FIXTURE_KEY: &str = "sk-ant-api03-notarealkey-demo-fixture-0000000000";

// ── FILM 1 · SABLE · the day the cloud goes down ─────────────────────────────────────────────

/// A two-person team shipping AI agents that read insurance claims. Mid-afternoon, Team plan,
/// real work already in progress.
const SABLE: &[Beat] = &[
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
                    "claims/upstream.py:141 | ",
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

/// 🔴 **PUT SOMETHING ON THE RIGHT-HAND RAIL, OR THE FILM HAS NO RIGHT-HAND SIDE.**
///
/// The rail is PERMANENT in the design — it needs no flag, only a terminal at least
/// `session_view::DESIGN_WIDTH` (81) columns wide. What it needs from us is DATA: five bands read
/// five `Option` fields that `App::new` leaves `None`.
///
/// ⚠️ **`client` IS NOT OPTIONAL HERE AND IT IS THE ONE THAT BITES.** `app_health_lines` returns
/// `Live Monitor unavailable. / Run /login here.` at `live_renderer.rs:1492` **before it reads the
/// overview**, so dressing the overview alone changes nothing on screen. The client points at a
/// dead port, copied from the gallery's own home fixture: it constructs, it is never dialled, and
/// no film makes a network call.
///
/// ⛔ **Two things are deliberately NOT set.** `prod_graph` replaces the whole five-band rail with
/// the code-graph view (`live_renderer.rs:2061`), and a non-empty `citations` replaces the rail with
/// the citation pane (`live_renderer.rs:2510`). Both are real product surfaces and both would take
/// the rail off screen, which is the defect this function exists to fix.
pub(crate) fn dress(app: &mut crate::App, fixtures: bool) {
    use serde_json::json;

    app.auth_resolved = true;
    // With the gate shut the rail stays honest: no invented services, no invented agents. The
    // frame is still the real two-pane layout — it is the NUMBERS that are withheld, not the app.
    if !fixtures {
        return;
    }
    app.header.plan = Some("team".to_string());
    app.header.indexed = Some(true);
    app.header.files = Some(1_284);
    app.account = serde_json::from_value(json!({
        "email": "you@sable.dev",
        "plan": "team",
        "seats": 2,
        "team": {"id": "team-sable", "name": "Sable", "role": "owner",
                 "is_admin": true, "is_owner": true}
    }))
    .ok();
    // ⚠️ `ApiKey::new` rejects only the empty string, and this is not it — but the result is
    // matched rather than unwrapped, because a demo that panics on a key it constructed itself is
    // a demo that dies on camera.
    if let Ok(key) = estelle_client::ApiKey::new("estelle_demo_fixture_key") {
        app.client = estelle_client::Client::new(
            "http://127.0.0.1:9/",
            key,
            estelle_client::MINIMUM_TIMEOUT,
        )
        .ok();
    }
    app.prod_overview = serde_json::from_value(json!({
        "app": "claims-agent",
        "org": "sable",
        "series": {
            "window_s": 3600,
            "bucket_s": 300,
            "requests_source": "monitor_ingest",
            "buckets": [
                {"t": 1, "errors": 0, "requests": 612},
                {"t": 2, "errors": 1, "requests": 640},
                {"t": 3, "errors": 0, "requests": 628},
                {"t": 4, "errors": 2, "requests": 655}
            ]
        },
        "uptime": {"checks": 3, "up": 3, "down": 0},
        "uptime_checks": [
            {"check_id": "c1", "name": "claims-api", "url": "https://claims/health",
             "enabled": true, "up": true, "last_status": 200,
             "last_latency_ms": 118.2, "last_checked": 1788392400.0},
            {"check_id": "c2", "name": "insurer-proxy", "url": "https://proxy/health",
             "enabled": true, "up": true, "last_status": 200,
             "last_latency_ms": 244.7, "last_checked": 1788392370.0},
            {"check_id": "c3", "name": "worker", "url": "https://worker/health",
             "enabled": true, "up": true, "last_status": 200,
             "last_latency_ms": 61.0, "last_checked": 1788392340.0}
        ]
    }))
    .ok();
    // ⛔ No `patch` on any issue: `queue_lines` renders a patch at a hardcoded 96 columns
    // (`live_renderer.rs:1876`) inside a ~30-column rail, which blows the rail out.
    app.prod_issues = serde_json::from_value(json!({
        "issues": [{
            "key": "sable-88",
            "status": "unresolved",
            "signal": {"title": "ReadTimeout in fetch_claim"},
            "count": 12,
            "events_in_window": 4,
            "bind_status": "bound",
            "repair": {"status": "proposed", "pr": null, "patch": null,
                       "patch_absent_reason": "waiting on the repro suite"}
        }],
        "counts": {"unresolved": 1},
        "window_s": 3600
    }))
    .ok();
    app.prod_agent_health = serde_json::from_value(json!({
        "enabled": true,
        "observed_at": 1788392400.0,
        "stale_after_s": 120,
        "counts": {"reporting": 2, "degraded": 0, "silent": null},
        "agents": [
            {"id": "claims-reader", "state": "healthy", "events": 1420,
             "last_seen": 1788392370.0, "current_signal": null},
            {"id": "policy-matcher", "state": "healthy", "events": 903,
             "last_seen": 1788392340.0, "current_signal": null}
        ]
    }))
    .ok();
    app.prod_github_status = serde_json::from_value(json!({
        "connected": true, "provider": "github", "login": "sable-eng",
        "observed_at": 1788392400.0, "absent_reason": null
    }))
    .ok();
    app.prod_proposed_prs = serde_json::from_value(json!({
        "prs": [{
            "number": 412,
            "title": "Cap upstream retries at 2",
            "url": "https://github.com/sable/claims-agent/pull/412",
            "repo": "sable/claims-agent",
            "issue_key": "sable-88",
            "repair_status": "pr",
            "gate": {"state": "clean", "verdict": "merge", "blockers": 0, "verified": true},
            "gate_absent_reason": null,
            "created_at": "2026-09-02T13:40:00Z",
            "updated_at": "2026-09-02T13:44:00Z"
        }],
        "next_cursor": null,
        "has_more": false
    }))
    .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_book::session::Say;

    /// 🔴 **THE FIXTURE CREDENTIAL IS STILL REFUSED BY THE SHIPPED FENCE.**
    ///
    /// This is the assertion that keeps beat 3 honest. If someone shortens the string, or the
    /// engine's allowlist grows to cover it, the beat quietly becomes a DRAWING of a refusal the
    /// product would not actually make — and nothing else in the suite would notice, because the
    /// frame renders identically either way.
    #[test]
    fn the_fixture_credential_is_really_refused_by_the_shipped_fence() {
        let (shape, line) = estelle_client::find_secret_shape(FIXTURE_KEY)
            .expect("the film's fixture key must be refused by the fence the CLI runs");
        assert_eq!(shape, "an sk- API key");
        assert_eq!(line, 1);
        // ⚠️ The beat does NOT quote `shape` verbatim, and that is deliberate — see
        // `the_credential_beat_survives_the_products_own_redactor`. What it must not do is name a
        // DIFFERENT credential from the one the fence caught.
        let spoken = credential_beat();
        assert!(
            spoken.contains("Anthropic API key"),
            "beat 3 does not name the credential the fence caught: {spoken:?}"
        );
        assert!(
            shape.contains("API key"),
            "the fence's own wording moved: {shape:?}"
        );
    }

    /// The three lines of the credential refusal, as the script writes them.
    fn credential_beat() -> String {
        match &SABLE[2].reply[0] {
            Say::Failure(lines) => lines.join(" "),
            _ => String::new(),
        }
    }

    /// 🔴 **THE REFUSAL MUST SURVIVE THE PRODUCT'S OWN REDACTOR.**
    ///
    /// `mask_secret` replaces an entire line that merely CONTAINS `sk-`, `ghp_`, `github_pat_` or
    /// `estelle_live_`, and `transcript.rs:419` runs every Failure line through it. The first draft
    /// of beat 3 quoted the fence's shape name — *"an sk- API key"* — and therefore rendered as
    /// `[credential hidden]`: **the refusal redacted its own reason**, and the single most important
    /// beat in the film said nothing at all.
    ///
    /// ⚠️ **THE POSITIVE CONTROL IS THE SECOND HALF.** A test that only asserted "these lines are
    /// not masked" would pass on empty strings, so the fixture key itself is asserted to STILL be
    /// masked — the redactor is working, and the refusal is simply worded to get past it.
    #[test]
    fn the_credential_beat_survives_the_products_own_redactor() {
        let Say::Failure(lines) = &SABLE[2].reply[0] else {
            panic!("beat 3's first say must be the refusal banner");
        };
        for line in lines {
            assert_eq!(
                estelle_client::mask_secret(line),
                *line,
                "the refusal redacts its own reason: {line:?}"
            );
        }
        assert_eq!(
            estelle_client::mask_secret(FIXTURE_KEY),
            "[credential hidden]",
            "the redactor stopped masking the key — this test now proves nothing"
        );
    }

    /// ⛔ The fixture credential is inert to the parent repo's own source scanners, which exempt
    /// any match carrying an example marker. Two markers, so losing one is not losing the property.
    #[test]
    fn the_fixture_credential_carries_two_example_markers() {
        let lowered = FIXTURE_KEY.to_ascii_lowercase();
        for marker in ["notarealkey", "0000000000"] {
            assert!(
                lowered.contains(marker),
                "the fixture key lost its {marker:?} marker and is no longer inert by construction"
            );
        }
    }

    /// Everything Estelle says in a film, as one string.
    fn spoken() -> String {
        FILMS
            .iter()
            .flat_map(|film| film.beats.iter())
            .flat_map(|beat| beat.reply.iter())
            .map(|say| match say {
                Say::Answer { text, .. } => (*text).to_string(),
                Say::System(text) => (*text).to_string(),
                Say::Failure(lines) => lines.join(" "),
                Say::Command { name, lines } => format!("{name} {}", lines.join(" ")),
                Say::Table { name, rows, .. } => format!("{name} {}", rows.join(" ")),
                Say::Gate => "gate refused fastapi_turbo".to_string(),
                // Measured live, so it carries no script text to audit.
                Say::LocalFleet => "local fleet measured on this machine".to_string(),
                Say::Wait(_) => String::new(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// ⛔ **NO AI-SPEAK.** No beat may narrate its own helpfulness, hedge, apologise or greet.
    #[test]
    fn nothing_estelle_says_is_ai_speak() {
        const BANNED: &[&str] = &[
            "I noticed",
            "I've taken",
            "I have taken",
            "Let me",
            "let me",
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
        let text = spoken();
        for banned in BANNED {
            assert!(!text.contains(banned), "a film says {banned:?}");
        }
    }

    /// ⛔ **NEVER "SAVED".** A saving is only a fact against a measured counterfactual, and no film
    /// runs one. The honest frame is the cache split, which is a real field.
    #[test]
    fn no_film_claims_a_saving() {
        let text = spoken().to_ascii_lowercase();
        for claim in ["saved", "savings", "you save", "cheaper than"] {
            assert!(!text.contains(claim), "a film claims a saving: {claim:?}");
        }
    }

    /// Film 1 carries all nine statements a silent viewer has to leave with.
    #[test]
    fn film_one_stands_alone_with_the_sound_off() {
        let text = spoken();
        for statement in [
            "claims/upstream.py:141",      // 1 · cited answers about your own code
            "fastapi_turbo",               // 2 · it refuses what it cannot ground
            "blocked this prompt",         // 3 · the credential fence
            "529",                         // 4 · the provider dies
            "Provider returned 529",       // 5 · the provider names its own failure
            "on your hardware",            // 6 · the work moves to HIS machine
            "Codex plan",                  // 6b · a plan is spent by this CLI, never a server
            "Your team decided otherwise", // 7 · team memory contradicts him
            "Nothing merged",              // 8 · propose-only
            "billed by Estelle",           // 9 · what it cost
        ] {
            assert!(
                text.contains(statement),
                "film 1 lost the beat carrying {statement:?} — it no longer stands alone"
            );
        }
    }

    /// 🔴 **THE RAIL HAS SOMETHING ON IT.** `dress` is the difference between the five bands
    /// carrying Sable's services and them reading `no read yet · GET /agent/health`, which is what
    /// the founder saw. Asserted on the FIELDS rather than on a rendered frame so the failure names
    /// which band went dark.
    ///
    /// ⚠️ The `client` assertion is the one that matters most: without it `app_health_lines`
    /// short-circuits to `Live Monitor unavailable.` and every other field here is invisible.
    #[test]
    fn dressing_fills_every_band_the_rail_reads() {
        let mut app = crate::App::new(crate::Args {
            command: None,
            repo: Some("sable/claims-agent".to_string()),
        });
        dress(&mut app, true);
        assert!(
            app.client.is_some(),
            "without a client the rail says Live Monitor unavailable"
        );
        assert!(app.prod_overview.is_some(), "the app band is dark");
        assert!(app.prod_agent_health.is_some(), "the agents band is dark");
        assert!(
            app.prod_issues.is_some(),
            "the estelle/queue bands are dark"
        );
        assert!(app.prod_github_status.is_some(), "the github band is dark");
        assert!(
            app.prod_proposed_prs.is_some(),
            "no proposed PRs on the rail"
        );
        // ⛔ And the two that would take the rail OFF the screen are untouched.
        assert!(
            app.prod_graph.is_none(),
            "prod_graph replaces the whole rail"
        );
        assert!(
            app.citations.is_empty(),
            "citations replace the rail with the citation pane"
        );
    }

    /// With the gate shut the rail carries no invented numbers, and the app is still the real app.
    #[test]
    fn dressing_with_the_gate_shut_invents_nothing() {
        let mut app = crate::App::new(crate::Args {
            command: None,
            repo: Some("sable/claims-agent".to_string()),
        });
        dress(&mut app, false);
        assert!(
            app.prod_overview.is_none(),
            "a shut gate must invent no services"
        );
        assert!(
            app.prod_agent_health.is_none(),
            "a shut gate must invent no agents"
        );
        assert!(app.account.is_none(), "a shut gate must invent no account");
    }
}
