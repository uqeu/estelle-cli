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

use crate::design_book::script_cartwheel::CARTWHEEL;
use crate::design_book::script_estelle::ESTELLE_REPO;
use crate::design_book::script_sable::SABLE;
use crate::design_book::session::{Film, Say};

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

/// Every film, in the order `--session N` selects them.
pub(crate) const FILMS: &[Film] = &[
    Film {
        number: 1,
        repo: "sable/claims-agent",
        branch: "feat/retry-budget",
        beats: SABLE,
    },
    Film {
        number: 2,
        repo: "cartwheel/storefront",
        branch: "main",
        beats: CARTWHEEL,
    },
    // 🔴 Film 3 is set in THIS repo on purpose — see `script_estelle`'s own note.
    Film {
        number: 3,
        repo: "uqeu/estelle",
        branch: "main",
        beats: ESTELLE_REPO,
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
pub(crate) fn dress(app: &mut crate::App, film: &Film, fixtures: bool) {
    use serde_json::json;

    app.auth_resolved = true;
    // With the gate shut the rail stays honest: no invented services, no invented agents. The
    // frame is still the real two-pane layout — it is the NUMBERS that are withheld, not the app.
    if !fixtures {
        return;
    }
    app.header.plan = Some(
        match film.number {
            3 => "ultra",
            _ => "team",
        }
        .to_string(),
    );
    app.header.indexed = Some(true);
    app.header.files = Some(1_284);
    // 🔴 **THE RAIL BELONGS TO THE COMPANY THE FILM IS SET AT.** A Cartwheel session showing
    // Sable's `claims-api` on the right would be a continuity error a viewer catches before any
    // of the copy lands — and film 3 is on Ultra, which is a different plan with no team at all.
    let (email, plan, seats, team) = match film.number {
        1 => ("you@sable.dev", "team", 2, Some(("team-sable", "Sable"))),
        2 => (
            "you@cartwheel.shop",
            "team",
            6,
            Some(("team-cartwheel", "Cartwheel")),
        ),
        _ => ("khai@fatelabs.ca", "ultra", 1, None),
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
    let services: [&str; 3] = match film.number {
        1 => ["claims-api", "insurer-proxy", "worker"],
        2 => ["checkout", "catalog", "webhooks"],
        _ => ["api.fatelabs.ca", "sweep-worker", "monitor-ingest"],
    };
    #[allow(non_snake_case, reason = "read as a fixed triple by the JSON below")]
    let SERVICES = services;
    app.prod_overview = serde_json::from_value(json!({
        // ⚠️ The rail joins org and app, so handing it the whole slug for BOTH printed
        // `cartwheel/storefront/cartwheel/storefront`. Split it once, here.
        "app": film.repo.split('/').next_back().unwrap_or(film.repo),
        "org": film.repo.split('/').next().unwrap_or(film.repo),
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
            {"check_id": "c1", "name": SERVICES[0], "url": "https://a/health",
             "enabled": true, "up": true, "last_status": 200,
             "last_latency_ms": 118.2, "last_checked": 1788392400.0},
            {"check_id": "c2", "name": SERVICES[1], "url": "https://b/health",
             "enabled": true, "up": true, "last_status": 200,
             "last_latency_ms": 244.7, "last_checked": 1788392370.0},
            {"check_id": "c3", "name": SERVICES[2], "url": "https://c/health",
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
            "signal": {"title": match film.number { 1 => "ReadTimeout in fetch_claim", 2 => "checkout.session.create failing", _ => "sweep 503 on batches over 500KB" }},
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
    // The agents belong to the company the film is set at, for the same reason the services do.
    #[allow(non_snake_case, reason = "read as a fixed pair by the JSON below")]
    let AGENTS: [&str; 2] = match film.number {
        1 => ["claims-reader", "policy-matcher"],
        2 => ["cart-worker", "fulfilment"],
        _ => ["sweep-worker", "hook-relay"],
    };
    app.prod_agent_health = serde_json::from_value(json!({
        "enabled": true,
        "observed_at": 1788392400.0,
        "stale_after_s": 120,
        "counts": {"reporting": 2, "degraded": 0, "silent": null},
        "agents": [
            {"id": AGENTS[0], "state": "healthy", "events": 1420,
             "last_seen": 1788392370.0, "current_signal": null},
            {"id": AGENTS[1], "state": "healthy", "events": 903,
             "last_seen": 1788392340.0, "current_signal": null}
        ]
    }))
    .ok();
    app.prod_github_status = serde_json::from_value(json!({
        "connected": true, "provider": "github", "login": match film.number { 1 => "sable-eng", 2 => "cartwheel-eng", _ => "uqeu" },
        "observed_at": 1788392400.0, "absent_reason": null
    }))
    .ok();
    app.prod_proposed_prs = serde_json::from_value(json!({
        "prs": [{
            "number": match film.number { 1 => 412, 2 => 418, _ => 421 },
            "title": match film.number {
                1 => "Cap upstream retries at 2",
                2 => "Move checkout off removed Stripe fields",
                _ => "Split the sweep upload and poll budgets" },
            "url": format!("https://github.com/{}/pull/1", film.repo),
            "repo": film.repo,
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
    use crate::design_book::script_sable::{FIXTURE_KEY, SABLE};
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
