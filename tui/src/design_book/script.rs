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

use crate::design_book::rail;
use crate::design_book::script_cartwheel::CARTWHEEL;
use crate::design_book::script_incident::INCIDENT;
use crate::design_book::script_solo::SOLO;
use crate::design_book::session::{Film, Say};

/// Strings that exist ONLY in a fixture, used by the gate test in both directions.
///
/// 🔴 The positive half is why this is a list and not an `assert!(!contains)`: an absence check
/// passes identically over a player that drew nothing at all.
pub(crate) const FIXTURE_NEEDLES: &[&str] =
    &["billing/hooks.py:88", "amount_capturable_updated", "$52.65"];

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

/// Every film, in the order `--session N` selects them.
pub(crate) const FILMS: &[Film] = &[
    Film {
        number: 1,
        repo: "saltbox/inkwell",
        branch: "feat/billing",
        beats: SOLO,
    },
    Film {
        number: 2,
        repo: "cartwheel/storefront",
        branch: "main",
        beats: CARTWHEEL,
    },
    // 🔴 Film 3 is set in THIS repo on purpose — see `script_estelle`'s own note.
    // 🔴 Film 3 is the SAME storefront as film 2, on the night it goes down. Two films at one
    // company is the founder's own structure, and it is what lets film 3 open on a team and a
    // codebase the viewer already knows.
    Film {
        number: 3,
        repo: "cartwheel/storefront",
        branch: "main",
        beats: INCIDENT,
    },
];

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
/// The plan a film's account is on. **One owner**, because the header and the account both print
/// it and they drifted apart the moment film 3 changed company.
fn plan_of(film: &Film) -> &'static str {
    match film.number {
        1 => "ultra",
        _ => "team",
    }
}

pub(crate) fn dress(app: &mut crate::App, film: &Film, fixtures: bool) {
    use serde_json::json;

    app.auth_resolved = true;
    // With the gate shut the rail stays honest: no invented services, no invented agents. The
    // frame is still the real two-pane layout — it is the NUMBERS that are withheld, not the app.
    if !fixtures {
        return;
    }
    // ⚠️ The header's plan and the account's plan are TWO READS OF ONE FACT, and they disagreed:
    // film 3 rendered `ultra` in the header over a Cartwheel TEAM account. One source now.
    app.header.plan = Some(plan_of(film).to_string());
    app.header.indexed = Some(true);
    app.header.files = Some(1_284);
    // 🔴 **THE RAIL BELONGS TO THE COMPANY THE FILM IS SET AT.** A Cartwheel session showing a
    // solo developer's services on the right is a continuity error a viewer catches before any of
    // the copy lands.
    let (email, plan, seats, team) = match film.number {
        1 => ("you@saltbox.dev", plan_of(film), 1, None),
        2 => (
            "you@cartwheel.shop",
            plan_of(film),
            6,
            Some(("team-cartwheel", "Cartwheel")),
        ),
        _ => (
            "you@cartwheel.shop",
            plan_of(film),
            6,
            Some(("team-cartwheel", "Cartwheel")),
        ),
    };
    app.account = serde_json::from_value(match team {
        Some((id, name)) => json!({
            "email": email, "plan": plan, "seats": seats,
            "team": {"id": id, "name": name, "role": "owner",
                     "is_admin": true, "is_owner": true}
        }),
        None => json!({"email": email, "plan": plan, "seats": seats}),
    })
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
    // 🔴 **EVERYTHING THAT MOVES BELONGS TO `rail::tick`, NOT HERE.** `dress` used to set the
    // overview, the agents, the issues and the PRs once, and nothing touched them again — so the
    // right-hand third of the screen was a photograph for two minutes while the session below it
    // was alive. Identity is static and stays here; every number is a function of the film clock.
    // The GitHub binding is identity, not motion: which account is connected does not change
    // during a film, so it stays here rather than being rewritten sixty times a second.
    app.prod_github_status = serde_json::from_value(json!({
        "connected": true,
        "provider": "github",
        "login": match film.number { 1 => "saltbox", 2 => "cartwheel-eng", _ => "cartwheel-eng" },
        "observed_at": 1_788_392_400.0,
        "absent_reason": null,
    }))
    .ok();
    rail::tick(app, film, 0, fixtures);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_book::script_solo::{FIXTURE_KEY, SOLO};
    use crate::design_book::session::{Beat, Say};

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

    /// The three lines of the credential refusal, FOUND rather than indexed.
    ///
    /// ⚠️ **IT WAS `SOLO[2]`, THEN `SOLO[10]`, AND BOTH WENT STALE THE MOMENT A BEAT MOVED.** The
    /// script is data the founder reorders — that is the whole point of the file — so a guard that
    /// pins a beat by POSITION is a guard that breaks on the edit it exists to survive. It searches
    /// for the refusal now, and fails loudly if the film ever loses it.
    fn credential_lines() -> &'static [&'static str; 3] {
        SOLO.iter()
            .flat_map(|beat| beat.reply.iter())
            .find_map(|say| match say {
                Say::Failure(lines) if lines[0].contains("did not go out") => Some(lines),
                _ => None,
            })
            .expect("film 1 must still carry the credential refusal")
    }

    fn credential_beat() -> String {
        credential_lines().join(" ")
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
        let lines = credential_lines();
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
        // 🔴 **THE INTERRUPT'S LINES LIVE IN `typed`, NOT IN `reply`, AND THIS SWEEP MISSED THEM.**
        // `Key::Interrupt` carries the most important sentence in film 3 — "142 checkouts failed
        // since 23:04" and the trust line under it — and because it sits in the KEYSTROKE stream
        // rather than the reply, every prose guard here was blind to it: the AI-speak ban, the
        // "saved" ban and the stands-alone needles all swept past the one beat the film exists for.
        // A guard that covers the shape it expects and not the shape that exists is the third
        // species of green.
        FILMS
            .iter()
            .flat_map(|film| film.beats.iter())
            .flat_map(|beat| {
                beat.reply
                    .iter()
                    .chain(beat.typed.iter().flat_map(|key| match key {
                        crate::design_book::session::Key::Interrupt(says) => says.iter(),
                        _ => [].iter(),
                    }))
            })
            .map(|say| match say {
                Say::Answer { text, .. } => (*text).to_string(),
                Say::System(text) => (*text).to_string(),
                Say::Failure(lines) => lines.join(" "),
                Say::Command { name, lines } => format!("{name} {}", lines.join(" ")),
                Say::Table { name, rows, .. } => format!("{name} {}", rows.join(" ")),
                // The gate's own blockers ARE user-facing prose, so they belong in the sweep.
                Say::Gate(fixture) => {
                    let mut text = format!("{} {}", fixture.detail, fixture.note);
                    for (claim, finding) in fixture.blockers {
                        text.push_str(&format!(" {claim} {finding}"));
                    }
                    text
                }
                // The fleet's own prose — the batch name and the narrator line — IS user-facing,
                // so it belongs in the sweep. The worker rows are the renderer's words, not the
                // script's, and `orchestra_view` has its own guards over those.
                Say::Orchestra(fleet) => {
                    let mut text = format!("{} {}", fleet.batch, fleet.narrator);
                    for worker in fleet.workers {
                        if let Some(action) = worker.action {
                            text.push(' ');
                            text.push_str(action);
                        }
                    }
                    text
                }
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
        // 🔴 **FILM 1 ARGUES ONE THING: THE PLAN SURVIVED THE PROVIDER, AND A SMALL MODEL
        // FINISHED THE JOB.** Each needle is a beat a silent viewer has to see, so trimming a beat
        // is a test failure rather than a quiet loss of the film's whole argument.
        for statement in [
            "billing/hooks.py",                   // 1 · it read his actual repo
            "reached your Anthropic usage limit", // 2 · the wall, and it is a cap, not an outage
            "All ten workers stopped",            // 3 · ten subagents die at once
            "the decisions from steps 1 to 4",    // 3b · the plan and the work survive it
            "$52.65",                             // 4 · cost is the REASON he moves
            "codex",        // 5 · a plan he already pays for does the thinking
            "this machine", // 6 · his own hardware does the writing
            "the worker has no data for this event", // 7 · why it went and looked, unprompted
            "context7",     // 7b · research feeds it live docs
            "every name resolves in this repo", // 8 · the gate checks the small model
            "not idempotent", // 9 · a rival family argues with it
            "did not go out", // 10 · the credential fence
            ".env file",    // 10b · and he is told what to do about it
            "455 tests",    // 11 · the long task finishes, correct
            "what you actually paid", // 12 · the bill
        ] {
            assert!(
                text.contains(statement),
                "film 1 lost the beat carrying {statement:?} — it no longer stands alone"
            );
        }
    }

    /// Every [`Say`] in one beat, in the order the player fires them.
    ///
    /// ⚠️ **THE INTERRUPT'S LINES LIVE IN `typed`, AND THAT IS EXACTLY WHERE THE AUTONOMY IS.**
    /// A sweep that runs while he is mid-word is a `Say` inside a `Key::Interrupt`, so a scan of
    /// `beat.reply` alone would report a film with no autonomous work in it at all — the same hole
    /// `spoken()` next door already had to be corrected for.
    fn beat_says(beat: &'static Beat) -> Vec<&'static Say> {
        beat.typed
            .iter()
            .flat_map(|key| match key {
                crate::design_book::session::Key::Interrupt(says) => says.iter(),
                _ => [].iter(),
            })
            .chain(beat.reply.iter())
            .collect()
    }

    /// Everything the hands type in film 1, as one lowercase string.
    fn typed_in_film_one() -> String {
        SOLO.iter()
            .flat_map(|beat| beat.typed.iter())
            .filter_map(|key| match key {
                crate::design_book::session::Key::Type(text)
                | crate::design_book::session::Key::Burst(text)
                | crate::design_book::session::Key::Oops(text) => Some(*text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    }

    /// 🔴 **FILM 1 IS AUTONOMOUS, AND THAT IS COUNTABLE RATHER THAN A TONE.**
    ///
    /// The founder, on the cut this replaced: *"you don't have to keep telling it to do this, to do
    /// this, to do this… he's gonna be like dude, you just have to keep talking to Estelle to keep
    /// doing stuff."* That cut had **19 beats**, and a beat IS a human turn — `Beat::typed` is
    /// submitted, so there is no beat Estelle starts by itself. Autonomy in this player is
    /// therefore **fewer beats carrying longer replies**, and each clause below is one reading of
    /// that.
    ///
    /// ⚠️ **CLAUSE 3 IS THE ONLY STRONG ONE AND I AM SAYING SO.** A beat count is a budget and the
    /// banned-phrase list is a spelling check — rename `review it` to `check it` and it passes. What
    /// cannot be renamed around is the STRUCTURE: the gate refusal and the repair that answers it
    /// are in one beat, so no user turn can sit between them.
    #[test]
    fn film_one_does_not_make_him_drive_every_step() {
        // 1 · the budget. 19 turns was the complaint; this is the ceiling that keeps it answered.
        assert!(
            SOLO.len() <= 12,
            "film 1 is back to {} human turns \u{2014} the founder's whole note on this film",
            SOLO.len()
        );

        // 2 · he interrupts it, and the film keeps talking underneath his half-typed line.
        let interrupts = SOLO
            .iter()
            .flat_map(|beat| beat.typed.iter())
            .filter(|key| matches!(key, crate::design_book::session::Key::Interrupt(_)))
            .count();
        assert!(
            interrupts >= 1,
            "nothing in film 1 arrives while he is mid-word \u{2014} it is turn-taking again"
        );

        // 3 · 🔴 THE ONE THAT CANNOT BE RENAMED AROUND. The gate refuses Estelle's own code and
        //     Estelle repairs it INSIDE THE SAME BEAT, so no typed line can sit between them.
        let refusing = SOLO
            .iter()
            .find(|beat| {
                beat_says(beat)
                    .iter()
                    .any(|say| matches!(say, Say::Gate(_)))
            })
            .expect("film 1 must still carry a gate refusal");
        let says = beat_says(refusing);
        let refused_at = says
            .iter()
            .position(|say| matches!(say, Say::Gate(_)))
            .expect("the refusal was just found");
        let repaired = says[refused_at + 1..].iter().any(|say| match say {
            Say::Command { lines, .. } => lines.iter().any(|line| line.contains("repair")),
            _ => false,
        });
        assert!(
            repaired,
            "the gate refuses and nothing repairs it in the same beat \u{2014} a human turn is \
             carrying the recovery again"
        );

        // 4 · the five lines he read back at me, by name. ⚠️ A spelling check, and weak on its own.
        let typed = typed_in_film_one();
        for driven in [
            "fix it",
            "look it up",
            "review it",
            "pull it forward",
            "is the graph still current",
            "finish the rest",
        ] {
            assert!(
                !typed.contains(driven),
                "film 1 makes him type {driven:?} again"
            );
        }
        // The positive control: he is still IN the film. A guard over what he does not say passes
        // identically over a film in which he never types at all.
        assert!(
            typed.contains("whats worker 8"),
            "he no longer checks in on the fleet \u{2014} this guard is measuring an empty film"
        );
    }

    /// 🔴 **FILM 2 ARGUES ONE THING: THE TEAM'S REASONING IS IN THE ROOM WITH HIM.**
    ///
    /// Its centre is the frame no competitor can build — not *"Devon committed to this file"*,
    /// which any tool reads from git, but **the words Devon typed at his own agent**. Each needle is
    /// a beat a silent viewer has to see, so trimming one is a test failure rather than a quiet loss
    /// of the film's whole argument.
    ///
    /// ⚠️ **THREE NEEDLES CHANGED WHEN THE SURFACE AUDIT LANDED, AND EACH ONE MARKS A CUT BEAT.**
    /// `"record why this path differs"` and `"posted to #eng"` pinned a `/choose` menu and a
    /// `● /slack` receipt that no shipped surface answers, and `"asked about this same retry path"`
    /// pinned a table whose second column — *what they were told* — has no store behind it: the turn
    /// log holds `role="user"` rows only. The replacements pin what the product can actually do,
    /// **including its refusal**, which is now the film's strongest line.
    #[test]
    fn film_two_stands_alone_with_the_sound_off() {
        let text = spoken();
        for statement in [
            "merged #401",                           // 1 · the repo moved while he was away
            "0009-upstream-retry-budget.md:31",      // 2 · a decision, volunteered, with its line
            "reads it instead of guessing",          // 3 · he disagrees, and his reason is kept
            "Sam proposed",                          // 4 · a proposal already parked in Slack
            "where do we bound the capture retries", // 5 · 🔴 a teammate's PROMPT, word for word
            "I do not have the answers he got",      // 6 · 🔴 and the half it refuses to invent
            "inside billing/capture.py right now",   // 7 · a teammate mid-flight in the same file
            "Nobody disagrees",                      // 8 · the team already answered this
            "Nothing stopped for that question",     // 9 · he interrupts it and it keeps working
            "Only the record of what this team already tried", // 10 · memory beats the gate
            "Nobody was paged",                      // 11 · the team beat, with its limit stated
            "waiting on your review",                // 12 · what still needs him
        ] {
            assert!(
                text.contains(statement),
                "film 2 lost the beat carrying {statement:?} \u{2014} it no longer stands alone"
            );
        }
    }

    /// 🔴 **FILM 3 ARGUES ONE THING: HE FOUND OUT FROM ESTELLE, NOT FROM A DASHBOARD.**
    ///
    /// Its centre is an interrupt he did not ask for, **three more turns nobody asked for after
    /// it**, a refusal of our OWN repair at the worst possible moment, and a recovery on camera.
    /// Each needle is a beat a silent viewer has to see.
    #[test]
    fn film_three_stands_alone_with_the_sound_off() {
        let text = spoken();
        for statement in [
            "serve/sweep.py:112", // 1 · ordinary work, cited, before anything breaks
            "142 checkouts failed since 23:04", // 2 · 🔴 the user-visible fact, not a metric
            "I would not normally interrupt", // 3 · the trust line
            "Nobody bought anything", // 4 · volunteered: what the outage means to the business
            "2026-09-01",         // 5 · volunteered: the cause, to the version
            "I start on checkout now", // 6 · 🔴 it began without being asked
            "stop, and wait for you", // 7 · the plan streams back, and its last step is the promise
            "put it back when this is over", // 8 · his sentence is held, and he is told so
            "1 active",           // 9 · 🔴 he is the only session open, off the shipped /presence
            "We fix it together", // 10 · and he is not alone anyway
            "Not for this one",   // 11 · 🔴 the local-model question, ANSWERED NO, with a reason
            "no such method in stripe 12.4.0", // 12 · 🔴 the gate refuses OUR repair
            "nothing left the sandbox", // 13 · the refusal cost nothing
            "I will not merge it for you", // 14 · propose-only, in his own favourite words
            "recovered 23:15",    // 15 · it comes back, on camera
            "held it while we worked", // 16 · the sentence returns
            "not a better model", // 17 · the local fleet finishes, with no comparative claim
            "billed by Estelle",  // 18 · the bill
        ] {
            assert!(
                text.contains(statement),
                "film 3 lost the beat carrying {statement:?} \u{2014} it no longer stands alone"
            );
        }
    }

    /// 🔴 **ESTELLE SENDS THE NEXT MESSAGE ITSELF, AND THE COUNT IS THE ASSERTION.**
    ///
    /// The founder's note is that the product does not wait to be asked: *"I want Estelle to
    /// autonomously just send its next message instead of waiting for him to type a message, if it
    /// finds an unresolved error."* One interrupt satisfies "it spoke first" and says nothing about
    /// "it kept going", so this counts the [`Key::Interrupt`] blocks in film 3 and requires
    /// **more than one** — the second and later ones are the turns nobody typed anything to
    /// provoke, which is the whole note.
    ///
    /// ⚠️ It counts INTERRUPT BLOCKS, not `Say`s: a single interrupt carrying six lines is still
    /// one turn, and inflating the number that way is exactly the green this must not give.
    #[test]
    fn film_three_keeps_talking_without_being_asked() {
        use crate::design_book::session::Key;
        let interrupts = film(3)
            .expect("film 3")
            .beats
            .iter()
            .flat_map(|beat| beat.typed.iter())
            .filter(|key| matches!(key, Key::Interrupt(_)))
            .count();
        assert!(
            interrupts > 1,
            "film 3 speaks unprompted {interrupts} time(s) \u{2014} it waits to be asked again"
        );
    }

    /// 🔴 **THE FLEET IS THE PRODUCT'S RENDERER, AND NO HAND-TYPED CELL NAMES A MODEL.**
    ///
    /// Film 3 drew its work as a `Say::Command { name: "work" }` whose rows read
    /// `checkout   serve/checkout.py \u{b7} claude-opus-4-8` — **a per-worker model column, which is
    /// the one cell `orchestra_view` refuses to draw**, because `FleetAgent` carries neither a
    /// model nor a cost. The founder read the difference off the screen unaided: *"in orchestra it
    /// actually shows each model going… it kind of seems like you faked it."*
    ///
    /// ⚠️ **TWO CLAUSES, TWO ASSERTIONS.** "Use the real renderer" and "name no model in a typed
    /// cell" are different properties, and a film could satisfy either alone. The first is a
    /// count of [`Say::Orchestra`]; the second sweeps every `Command` line and every `Table` row
    /// for a model name. The fleet's own `models` roster is untouched by the sweep on purpose —
    /// that line is where a model name IS true.
    #[test]
    fn film_three_draws_its_workers_with_the_products_own_renderer() {
        let beats = film(3).expect("film 3").beats;
        let fleets = beats
            .iter()
            .flat_map(|beat| {
                beat.reply
                    .iter()
                    .chain(beat.typed.iter().flat_map(|key| match key {
                        crate::design_book::session::Key::Interrupt(says) => says.iter(),
                        _ => [].iter(),
                    }))
            })
            .filter(|say| matches!(say, Say::Orchestra(_)))
            .count();
        assert!(
            fleets >= 2,
            "film 3 draws {fleets} orchestra block(s) \u{2014} the work is hand-typed again"
        );

        // Every model name that could plausibly be typed into a cell. The catalogue names are the
        // local fleet's own, so a film cannot smuggle one in under a different spelling either.
        let mut names = vec!["claude-opus", "claude-sonnet", "gpt-", "codex"];
        names.extend(crate::design_book::session::LOCAL_FLEET.iter().copied());
        let mut swept = 0usize;
        for beat in beats {
            for say in beat.reply.iter().chain(beat.typed.iter().flat_map(|key| {
                match key {
                    crate::design_book::session::Key::Interrupt(says) => says.iter(),
                    _ => [].iter(),
                }
            })) {
                let cells: Vec<&str> = match say {
                    Say::Command { lines, .. } => lines.to_vec(),
                    Say::Table { rows, .. } => rows.to_vec(),
                    _ => Vec::new(),
                };
                for cell in cells {
                    swept += 1;
                    for name in &names {
                        assert!(
                            !cell.contains(name),
                            "film 3 types the model {name:?} into a cell: {cell:?} \u{2014}                              the roster line is the only honest place for it"
                        );
                    }
                }
            }
        }
        // The positive control. Without it the sweep passes identically over a film with no
        // tables at all, which is the vacuous half of every absence check we have written.
        assert!(
            swept > 20,
            "only {swept} cells swept \u{2014} the model-cell guard proves nothing"
        );
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
        dress(&mut app, film(1).expect("film 1"), true);
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
        dress(&mut app, film(1).expect("film 1"), false);
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
