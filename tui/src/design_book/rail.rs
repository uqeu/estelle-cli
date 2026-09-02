//! The production rail, MOVING — one function, called every frame.
//!
//! 🔴 **A STATIC RAIL IS WHY TWO FILMS LOOKED LIKE SCREENSHOTS.** `dress` set one JSON blob per
//! film and no beat ever touched it again, so for two minutes the latency read `118ms` on every
//! single frame, the timestamps never advanced and the sparkline never moved. The session below it
//! was alive and the right-hand third of the screen was a photograph. **That is the difference
//! between a product running and a product being demonstrated**, and it is the same defect the
//! founder has named all night in other forms.
//!
//! ## What moves, and why each one
//!
//! * **Latency jitters per service.** The number a viewer's eye lands on first. Deterministic, so a
//!   re-shot take matches the last one frame for frame.
//! * **Counters climb.** Requests and agent events only go up, at a rate that suits the company.
//! * **Timestamps advance.** `observed` and `last seen` track the film clock.
//! * **The sparkline walks.** Buckets shift, so the little glyph row is never the same twice.
//! * **Incidents ramp and recover.** Film 3 needs the rail to climb for twenty seconds **while he
//!   is typing something else**, and then come back down on camera.
//!
//! ⚠️ **DETERMINISTIC ON PURPOSE — A RECORDING IS REHEARSED.** Every value here is a pure function
//! of `(film, elapsed_ms)`. An RNG would make each take different, so a fluff at 4:12 could not be
//! re-shot against the same footage, and a test could not assert on any of it.
//!
//! ⛔ **THIS FILE INVENTS NUMBERS, SO IT RUNS ONLY UNDER THE FIXTURE FLAG.** With the gate shut it
//! returns before writing anything, and the rail keeps whatever honest empty state it had.

use serde_json::json;

use crate::design_book::session::Film;

/// The clock the films date themselves against: 2026-09-02T13:40Z.
///
/// ⚠️ The gallery's own fixture used `4102444800`, which is the YEAR 2100, and `main.rs` already
/// carries a note about that value making every worker read `clock ahead`.
const EPOCH: f64 = 1_788_392_400.0;

/// One company's traffic, so the rail reads like the business the film is set at.
struct Profile {
    services: [&'static str; 3],
    agents: [&'static str; 2],
    /// Requests in a five-minute bucket, before jitter. A side project and a storefront are not
    /// the same shape, and a viewer who reads the number should believe the company.
    base_requests: u64,
    /// Base latency per service, in milliseconds.
    latency: [f64; 3],
    /// Errors ramp from `start_s`, and recover at `recover_s`.
    incident: Option<(u32, u32)>,
}

fn profile(film: &Film) -> Profile {
    match film.number {
        // A solo side project. Quiet, healthy, and it stays that way: film 1's argument is about
        // models and money, and a rail that misbehaves would pull the eye off it.
        1 => Profile {
            services: ["inkwell-web", "inkwell-api", "billing"],
            agents: ["indexer", "mailer"],
            base_requests: 41,
            latency: [88.0, 132.0, 61.0],
            incident: None,
        },
        // A busy storefront on an ordinary day. 🔴 ALIVE BUT CALM: the founder's own word. Traffic
        // moves, a PR opens, a teammate's commit lands — movement without alarm.
        2 => Profile {
            services: ["checkout", "catalog", "webhooks"],
            agents: ["cart-worker", "fulfilment"],
            base_requests: 812,
            latency: [104.0, 71.0, 143.0],
            incident: None,
        },
        // 🔴 THE SAME STOREFRONT, THE NIGHT IT GOES DOWN.
        //
        // **THE WINDOW IS TIED TO THE SCRIPT, NOT PICKED.** The ramp starts at 20 s and the
        // interrupt fires at about 39 s, so the rail climbs for roughly twenty seconds WHILE HE
        // TYPES and he does not react to it — the founder's own note that the wait is the beat. It
        // recovers at 118 s, which is where beat 8 applies the fix, so the rail comes back on
        // camera rather than after the cut.
        _ => Profile {
            services: ["checkout", "catalog", "webhooks"],
            agents: ["cart-worker", "fulfilment"],
            base_requests: 794,
            latency: [109.0, 74.0, 138.0],
            incident: Some((20, 118)),
        },
    }
}

/// Deterministic jitter in `[-span, +span]`, keyed on two coordinates.
fn wobble(a: u64, b: u64, span: f64) -> f64 {
    let mixed = a
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(b.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(0x94D0_49BB_1331_11EB);
    let unit = ((mixed >> 33) % 2_001) as f64 / 1_000.0 - 1.0;
    unit * span
}

/// How badly the incident is biting at `t`, from 0.0 to 1.0.
fn severity(profile: &Profile, t: u32) -> f64 {
    let Some((start, recover)) = profile.incident else {
        return 0.0;
    };
    if t < start {
        return 0.0;
    }
    if t >= recover {
        // 🔴 IT RECOVERS ON CAMERA. A film that shows an outage and never shows it end has told
        // half a story, and the half it left out is the one we are selling.
        let since = f64::from(t - recover);
        return (1.0 - since / 18.0).max(0.0);
    }
    let ramp = f64::from(t - start) / 22.0;
    ramp.min(1.0)
}

/// Rewrite every time-varying field on the rail for this moment of this film.
///
/// Called once per frame by the player, and by the guards at a chosen `elapsed_ms`.
pub(crate) fn tick(app: &mut crate::App, film: &Film, elapsed_ms: u32, fixtures: bool) {
    if !fixtures {
        return;
    }
    let profile = profile(film);
    let t = elapsed_ms / 1000;
    let bite = severity(&profile, t);
    let now = EPOCH + f64::from(t);

    // ── the sparkline and the request counter ────────────────────────────────────────────────
    // Four buckets that WALK: bucket `i` is keyed on `t/3 + i`, so the row shifts about every
    // three seconds rather than redrawing the same glyphs for two minutes.
    let step = u64::from(t) / 3;
    let buckets = (0..4u64)
        .map(|i| {
            let requests = (profile.base_requests as f64
                + wobble(step + i, 7, profile.base_requests as f64 * 0.08))
            .max(1.0) as u64;
            // Under an incident the errors climb with the severity; otherwise a healthy service
            // still drops the occasional request, which is what makes zero look measured.
            // 🔴 A HEALTHY SYSTEM SHOWS ZERO, AND THAT IS NOT LAZINESS. The band paints its mark
            // `▲` the moment any error exists, so a baseline of "one or two, jittering" made a calm
            // film open on an alarm. What moves on a healthy rail is the REQUEST count and the
            // latency; the error row earns its colour by staying at zero until something is wrong.
            let errors = if bite > 0.0 {
                (requests as f64 * bite * 0.34).round() as u64
            } else {
                0
            };
            json!({"t": step + i, "errors": errors, "requests": requests})
        })
        .collect::<Vec<_>>();

    // ── the services ─────────────────────────────────────────────────────────────────────────
    // 🔴 THE WHOLE LINE GOES DOWN, NOT ONE SERVICE. An outage that politely confines itself to the
    // service you were looking at is not an outage anyone has had.
    let down = if bite > 0.55 { 3 } else { 0 };
    let checks = profile
        .services
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let base = profile.latency[index];
            // Latency degrades with the incident as well as jittering.
            let latency =
                base * (1.0 + bite * 6.0) + wobble(u64::from(t), index as u64, base * 0.07);
            let up = bite <= 0.55;
            json!({
                "check_id": format!("c{index}"),
                "name": name,
                "url": format!("https://{name}/health"),
                "enabled": true,
                "up": up,
                "last_status": if up { 200 } else { 503 },
                "last_latency_ms": (latency * 10.0).round() / 10.0,
                "last_checked": now - f64::from(index as u32 * 3),
            })
        })
        .collect::<Vec<_>>();

    app.prod_overview = serde_json::from_value(json!({
        "app": film.repo.split('/').next_back().unwrap_or(film.repo),
        "org": film.repo.split('/').next().unwrap_or(film.repo),
        "series": {
            "window_s": 3600,
            "bucket_s": 300,
            "requests_source": "monitor_ingest",
            "buckets": buckets,
        },
        "uptime": {"checks": 3, "up": 3 - down, "down": down},
        "uptime_checks": checks,
    }))
    .ok();

    // ── the agents ───────────────────────────────────────────────────────────────────────────
    // Events only ever climb, at a rate that suits the company.
    let rate = profile.base_requests / 8;
    app.prod_agent_health = serde_json::from_value(json!({
        "enabled": true,
        "observed_at": now,
        "stale_after_s": 120,
        "counts": {
            "reporting": if bite > 0.55 { 1 } else { 2 },
            "degraded": if bite > 0.55 { 1 } else { 0 },
            "silent": null,
        },
        "agents": [
            {"id": profile.agents[0],
             "state": if bite > 0.55 { "degraded" } else { "healthy" },
             "events": 1_420 + u64::from(t) * rate,
             "last_seen": now - 4.0,
             "current_signal": null},
            {"id": profile.agents[1], "state": "healthy",
             "events": 903 + u64::from(t) * (rate / 2).max(1),
             "last_seen": now - 11.0,
             "current_signal": null},
        ],
    }))
    .ok();

    tick_work(app, film, t, bite, now);
}

/// The issue queue and the open PRs — the two bands that carry a story rather than a number.
fn tick_work(app: &mut crate::App, film: &Film, t: u32, bite: f64, now: f64) {
    // ⛔ No `patch` on any issue: `queue_lines` renders a patch at a hardcoded 96 columns
    // (`live_renderer.rs:1876`) inside a ~30-column rail, which blows the rail out.
    let issue = match (film.number, bite > 0.0) {
        (2, _) => Some(("cart-114", "slow image variant on /catalog", "proposed")),
        (3, true) => Some(("cart-120", "checkout.session.create failing", "proposed")),
        (3, false) => None,
        _ => Some(("ink-31", "stripe webhook signature mismatch", "proposed")),
    };
    app.prod_issues = serde_json::from_value(match issue {
        Some((key, title, status)) => json!({
            "issues": [{
                "key": key, "status": "unresolved",
                "signal": {"title": title},
                "count": 4 + u64::from(t) / 6,
                "events_in_window": 4,
                "bind_status": "bound",
                "repair": {"status": status, "pr": null, "patch": null,
                           "patch_absent_reason": "waiting on the repro suite"},
            }],
            "counts": {"unresolved": 1},
            "window_s": 3600,
        }),
        None => json!({"issues": [], "counts": {"unresolved": 0}, "window_s": 3600}),
    })
    .ok();

    // 🔴 A PR THAT OPENS WHILE HE WATCHES. Film 2's rail has to be alive and CALM: a second PR
    // appearing at 40 s is movement a viewer notices without ever being alarmed by it.
    let mut prs = vec![json!({
        "number": match film.number { 1 => 207, 2 => 418, _ => 421 },
        "title": match film.number {
            1 => "Migrate billing off the removed Stripe fields",
            2 => "Cache catalog image variants",
            _ => "Move checkout onto the current Stripe fields" },
        "url": format!("https://github.com/{}/pull/1", film.repo),
        "repo": film.repo,
        "repair_status": "pr",
        "gate": {"state": "clean", "verdict": "merge", "blockers": 0, "verified": true},
        "gate_absent_reason": null,
        "created_at": "2026-09-02T13:40:00Z",
        "updated_at": "2026-09-02T13:44:00Z",
    })];
    if film.number == 2 && t >= 40 {
        prs.push(json!({
            "number": 419,
            "title": "Status page: incident feed",
            "url": format!("https://github.com/{}/pull/2", film.repo),
            "repo": film.repo,
                "repair_status": "pr",
            "gate": {"state": "clean", "verdict": "merge", "blockers": 0, "verified": true},
            "gate_absent_reason": null,
            "created_at": "2026-09-02T13:52:00Z",
            "updated_at": "2026-09-02T13:52:00Z",
        }));
    }
    // 🔴 **`.ok()` SWALLOWED A FIXTURE TYPO AND THE BAND WENT DARK WITH NO ERROR.**
    // `ProposedPr::issue_key` is a `String` with `#[serde(default)]`, and a default applies when
    // the KEY IS ABSENT — never when it is present and `null`. One `"issue_key": null` voided the
    // whole payload, `.ok()` turned that into `None`, and the rail simply drew nothing. In a
    // release build degrading quietly is right; in a test it must be loud, or the next fixture
    // typo is found on camera.
    let parsed =
        serde_json::from_value(json!({"prs": prs, "next_cursor": null, "has_more": false}));
    debug_assert!(
        parsed.is_ok(),
        "the proposed-PR fixture does not parse: {parsed:?}"
    );
    app.prod_proposed_prs = parsed.ok();
    let _ = now;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_book::script;

    fn app_for(number: u8) -> (crate::App, &'static Film) {
        let film = script::film(number).expect("film");
        let mut app = crate::App::new(crate::Args {
            command: None,
            repo: Some(film.repo.to_string()),
        });
        script::dress(&mut app, film, true);
        (app, film)
    }

    /// Everything the rail draws, as one string.
    fn rail_text(app: &crate::App) -> String {
        crate::live_renderer::production_workspace_lines(app, 60)
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
            .collect()
    }

    /// 🔴 **THE RAIL IS DIFFERENT ON EVERY FRAME IT IS ASKED FOR.**
    ///
    /// This is the whole point of the file, asserted the only way that means anything: render the
    /// rail at many moments and count the DISTINCT results. A static rail returns one.
    #[test]
    fn the_rail_never_draws_the_same_frame_twice_for_long() {
        for number in [1u8, 2, 3] {
            let (mut app, film) = app_for(number);
            let seen = (0..40u32)
                .map(|i| {
                    tick(&mut app, film, i * 3_000, true);
                    rail_text(&app)
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert!(
                seen.len() > 20,
                "film {number}: the rail drew only {} distinct frames in 40 samples",
                seen.len()
            );
        }
    }

    /// The same moment renders the same rail, every time. A recording is rehearsed.
    #[test]
    fn the_rail_is_deterministic() {
        for number in [1u8, 2, 3] {
            let (mut a, film) = app_for(number);
            let (mut b, _) = app_for(number);
            for at in [0u32, 17_000, 61_000, 140_000] {
                tick(&mut a, film, at, true);
                tick(&mut b, film, at, true);
                assert_eq!(rail_text(&a), rail_text(&b), "film {number} at {at} ms");
            }
        }
    }

    /// 🔴 **FILM 3'S OUTAGE ARRIVES, TAKES THE WHOLE LINE DOWN, AND RECOVERS ON CAMERA.**
    ///
    /// ⚠️ Three assertions, not one: healthy BEFORE, down DURING, healthy AFTER. A test that only
    /// checked the middle would pass on a rail that was broken from the first frame.
    #[test]
    fn film_three_goes_down_and_comes_back() {
        let (mut app, film) = app_for(3);
        tick(&mut app, film, 4_000, true);
        assert!(
            rail_text(&app).contains("3/3 up"),
            "film 3 starts unhealthy"
        );

        tick(&mut app, film, 60_000, true);
        let during = rail_text(&app);
        assert!(
            during.contains("0/3 up"),
            "the whole line must go down:\n{during}"
        );

        tick(&mut app, film, 175_000, true);
        assert!(
            rail_text(&app).contains("3/3 up"),
            "film 3 never recovers, so the film tells half a story"
        );
    }

    /// Films 1 and 2 stay healthy for their whole length. Alive is not the same as alarming.
    #[test]
    fn the_calm_films_never_go_down() {
        for number in [1u8, 2] {
            let (mut app, film) = app_for(number);
            for i in 0..60u32 {
                tick(&mut app, film, i * 6_000, true);
                let text = rail_text(&app);
                assert!(
                    text.contains("3/3 up"),
                    "film {number} went down at {}s, which it must never do:\n{text}",
                    i * 6
                );
            }
        }
    }

    /// Counters climb and never fall back. A counter that walks downwards reads as a bug.
    #[test]
    fn agent_event_counters_only_ever_climb() {
        let (mut app, film) = app_for(2);
        let mut last = 0u64;
        for i in 0..30u32 {
            tick(&mut app, film, i * 4_000, true);
            let events = app
                .prod_agent_health
                .as_ref()
                .and_then(|health| health.agents.first().and_then(|agent| agent.events))
                .expect("an agent with an event count");
            assert!(events >= last, "events fell from {last} to {events}");
            last = events;
        }
        assert!(last > 1_420, "the counter never moved at all");
    }

    /// ⛔ With the fixture gate shut the rail invents nothing, at any moment of any film.
    #[test]
    fn a_shut_gate_ticks_nothing() {
        let film = script::film(3).expect("film 3");
        let mut app = crate::App::new(crate::Args {
            command: None,
            repo: Some(film.repo.to_string()),
        });
        script::dress(&mut app, film, false);
        for at in [0u32, 60_000, 175_000] {
            tick(&mut app, film, at, false);
            assert!(app.prod_overview.is_none(), "a shut gate invented services");
            assert!(
                app.prod_agent_health.is_none(),
                "a shut gate invented agents"
            );
        }
    }
}
