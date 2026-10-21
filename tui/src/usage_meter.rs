//! `/me`'s usage line: the plan, the share of it used this period, when it resets, and a bar —
//! the shape of Claude Code's `/usage` (`Plan: Ultra · 37% used this period · resets Nov 20`).
//!
//! 🔴 **A SHARE, NEVER A CREDIT TOTAL.** The founder's ruling (2026-10-21) is that a customer sees
//! how much of the plan is used, not credit, token or dollar counts. `/me` used to print
//! `balance $x | budget $y | spent this period $z`; this module replaces that line.
//!
//! The arithmetic is the web's, on purpose (`web/app/dashboard/settings/usage/usage-model.ts`,
//! `planShare`): `spent_this_period / grant`, rounded DOWN, clamped 0–100, `<1%` for small non-zero
//! use. Two owners of one rule will drift, so the rule is written out here AND pinned by tests that
//! use the web's own examples; change both or neither.
//!
//! ⚠️ **A FAILED READ IS NEVER 0%.** "0% used" claims nothing was spent; a read that did not answer
//! claims nothing at all, so it renders `Usage unavailable` with the reason and draws no bar.
//!
//! Source: `GET /credits/forecast` → `balance_parts.{spent_this_period, grant, period_end,
//! period_source}` (backend `src/estelle/serve/api_credits.py::handle_credits_forecast`).
//! LIMIT: the denominator is the plan's grant only; purchased top-ups are a separate pool the
//! forecast reports apart, so a customer with top-ups can read 100% while still able to work.

use chrono::DateTime;
use chrono::FixedOffset;
use serde_json::Value;

/// Where `execute_remote_command` records the forecast on `/me`'s reply. The server's `/me` never
/// sends this key; the client adds it, so the renderer reads one reply instead of two.
pub(crate) const FORECAST_FIELD: &str = "credits_forecast";
/// Cells in the bar. A named constant so the width is a decision, not a literal in the loop.
pub(crate) const BAR_CELLS: usize = 20;
/// Longest server-supplied reason shown after "Usage unavailable".
const MAX_REASON_CHARS: usize = 120;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Share {
    Ok {
        percent: u8,
        label: String,
        over: bool,
    },
    Unavailable {
        reason: String,
    },
}

/// `spent` of `grant` as a whole percent — the web's `planShare`, rule for rule.
pub(crate) fn plan_share(spent: Option<f64>, grant: Option<f64>) -> Share {
    let unavailable = |reason: &str| Share::Unavailable {
        reason: reason.to_string(),
    };
    let (Some(spent), Some(grant)) = (
        spent.filter(|v| v.is_finite()),
        grant.filter(|v| v.is_finite()),
    ) else {
        return unavailable("the server did not report this period's use against your plan");
    };
    if grant <= 0.0 {
        return unavailable("the server reported a plan allowance of zero");
    }
    if spent < 0.0 {
        return unavailable("the server reported negative use");
    }
    let percent = ((spent / grant) * 100.0).floor().clamp(0.0, 100.0) as u8;
    debug_assert!(percent <= 100, "clamp failed: {percent}");
    let label = if spent > 0.0 && percent == 0 {
        "<1%".to_string()
    } else {
        format!("{percent}%")
    };
    Share::Ok {
        percent,
        label,
        over: spent > grant,
    }
}

/// The reason a forecast reply cannot be read, mirroring the web's `failed()`: a non-2xx
/// `http_status`, an `error` field, or no object at all.
fn failure(reply: &Value) -> Option<String> {
    let Some(object) = reply.as_object() else {
        return Some("the server sent no readable reply".to_string());
    };
    if let Some(status) = object.get("http_status").and_then(Value::as_u64)
        && !(200..300).contains(&status)
    {
        return Some(format!("the usage read failed (HTTP {status})"));
    }
    match object.get("error") {
        None | Some(Value::Null) => None,
        Some(Value::String(message)) if !message.trim().is_empty() => Some(message.clone()),
        Some(error) => Some(
            error
                .get("message")
                .and_then(Value::as_str)
                .filter(|message| !message.trim().is_empty())
                .unwrap_or("the usage read failed")
                .to_string(),
        ),
    }
}

/// `GET /credits/forecast` (or the error envelope the caller recorded in its place) → the share.
pub(crate) fn share_from_forecast(reply: &Value) -> Share {
    if let Some(reason) = failure(reply) {
        return Share::Unavailable { reason };
    }
    let Some(parts) = reply.get("balance_parts").and_then(Value::as_object) else {
        return Share::Unavailable {
            reason: "this server does not report usage against your plan yet".to_string(),
        };
    };
    plan_share(
        parts.get("spent_this_period").and_then(Value::as_f64),
        parts.get("grant").and_then(Value::as_f64),
    )
}

/// The plan name the web shows: the plan when `/me` says it is active, else Free.
pub(crate) fn plan_label(plan: Option<&str>, plan_active: Option<bool>) -> String {
    let name = match (plan.map(str::trim).filter(|p| !p.is_empty()), plan_active) {
        (Some(plan), Some(true)) => plan,
        _ => "free",
    };
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// "resets Nov 20" in the caller's offset; a one-time grant says so; anything unreadable is named.
pub(crate) fn reset_text(reply: &Value, offset: FixedOffset) -> String {
    let parts = reply.get("balance_parts");
    let field = |name: &str| parts.and_then(|p| p.get(name)).and_then(Value::as_str);
    if let Some(end) = field("period_end").and_then(|end| DateTime::parse_from_rfc3339(end).ok()) {
        return format!("resets {}", end.with_timezone(&offset).format("%b %-d"));
    }
    match field("period_source") {
        Some("lifetime") => "one-time allowance, does not reset".to_string(),
        _ => "reset date not reported".to_string(),
    }
}

/// `███████░░░░░░░░░░░░░ 37%`. Any non-zero use draws at least one cell, so it never reads as empty
/// (a cell is 5%; 2% used to draw nothing beside the label "2%", measured on the first smoke run).
pub(crate) fn bar(percent: u8, label: &str) -> String {
    debug_assert!(
        percent <= 100,
        "a share is clamped before it is drawn: {percent}"
    );
    let mut filled = usize::from(percent) * BAR_CELLS / 100;
    if filled == 0 && (percent > 0 || label == "<1%") {
        filled = 1;
    }
    let filled = filled.min(BAR_CELLS);
    format!(
        "{}{} {label}",
        "█".repeat(filled),
        "░".repeat(BAR_CELLS - filled)
    )
}

/// The usage block `/me` prints: one headline, then the bar (absent when usage is unavailable).
pub(crate) fn usage_lines(
    plan: &str,
    forecast: Option<&Value>,
    offset: FixedOffset,
) -> Vec<String> {
    let Some(forecast) = forecast else {
        return vec![format!(
            "Plan: {plan} · Usage unavailable · the usage read did not run"
        )];
    };
    match share_from_forecast(forecast) {
        Share::Unavailable { reason } => {
            let reason: String = reason.chars().take(MAX_REASON_CHARS).collect();
            vec![format!("Plan: {plan} · Usage unavailable · {reason}")]
        }
        Share::Ok {
            percent,
            label,
            over,
        } => {
            let used = if over {
                "100% used this period (all of it)".to_string()
            } else {
                format!("{label} used this period")
            };
            vec![
                format!("Plan: {plan} · {used} · {}", reset_text(forecast, offset)),
                bar(percent, &label),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).expect("utc")
    }

    fn forecast(spent: f64, grant: f64) -> Value {
        json!({"balance_parts": {"spent_this_period": spent, "grant": grant,
            "period_start": "2026-10-20T00:00:00Z", "period_end": "2026-11-20T00:00:00Z", "period_source": "stripe"}})
    }

    #[test]
    fn share_rounds_down_clamps_and_marks_small_use_like_the_web() {
        let ok = |spent, grant| plan_share(Some(spent), Some(grant));
        assert_eq!(
            ok(37.9, 100.0),
            Share::Ok {
                percent: 37,
                label: "37%".into(),
                over: false
            }
        );
        assert_eq!(
            ok(0.4, 100.0),
            Share::Ok {
                percent: 0,
                label: "<1%".into(),
                over: false
            }
        );
        assert_eq!(
            ok(0.0, 100.0),
            Share::Ok {
                percent: 0,
                label: "0%".into(),
                over: false
            }
        );
        assert_eq!(
            ok(99.99, 100.0),
            Share::Ok {
                percent: 99,
                label: "99%".into(),
                over: false
            }
        );
        assert_eq!(
            ok(250.0, 100.0),
            Share::Ok {
                percent: 100,
                label: "100%".into(),
                over: true
            }
        );
    }

    #[test]
    fn share_refuses_what_it_cannot_compute_instead_of_saying_zero() {
        for (spent, grant) in [
            (None, Some(100.0)),
            (Some(1.0), None),
            (Some(f64::NAN), Some(100.0)),
            (Some(1.0), Some(0.0)),
            (Some(-1.0), Some(100.0)),
        ] {
            assert!(
                matches!(plan_share(spent, grant), Share::Unavailable { .. }),
                "{spent:?}/{grant:?}"
            );
        }
    }

    #[test]
    fn the_headline_is_plan_share_and_reset_with_a_bar_and_no_credit_totals() {
        let lines = usage_lines("Ultra", Some(&forecast(370.0, 1000.0)), utc());
        assert_eq!(
            lines,
            vec![
                "Plan: Ultra · 37% used this period · resets Nov 20".to_string(),
                "███████░░░░░░░░░░░░░ 37%".to_string(),
            ]
        );
        let joined = lines.join("\n");
        for raw in ["370", "1000", "$", "credit", "balance"] {
            assert!(!joined.contains(raw), "a raw total leaked: {raw}\n{joined}");
        }
    }

    #[test]
    fn small_use_draws_a_sliver_and_over_use_says_all_of_it() {
        assert_eq!(
            usage_lines("Pro", Some(&forecast(1.0, 1000.0)), utc())[1],
            "█░░░░░░░░░░░░░░░░░░░ <1%"
        );
        assert_eq!(
            usage_lines("Pro", Some(&forecast(20.0, 1000.0)), utc())[1],
            "█░░░░░░░░░░░░░░░░░░░ 2%"
        );
        assert_eq!(
            usage_lines("Pro", Some(&forecast(0.0, 1000.0)), utc())[1],
            "░░░░░░░░░░░░░░░░░░░░ 0%"
        );
        let over = usage_lines("Pro", Some(&forecast(1200.0, 1000.0)), utc());
        assert_eq!(
            over[0],
            "Plan: Pro · 100% used this period (all of it) · resets Nov 20"
        );
        assert_eq!(over[1], format!("{} 100%", "█".repeat(BAR_CELLS)));
    }

    #[test]
    fn a_failed_read_is_usage_unavailable_never_zero_percent_and_draws_no_bar() {
        let cases = [
            (
                json!({"error": {"message": "not authorised"}, "http_status": 401}),
                "HTTP 401",
            ),
            (
                json!({"error": {"message": "pool exhausted"}}),
                "pool exhausted",
            ),
            (json!({"error": "down"}), "down"),
            (json!({"on_track": true}), "does not report usage"),
            (json!("nonsense"), "no readable reply"),
        ];
        for (reply, reason) in cases {
            let lines = usage_lines("Ultra", Some(&reply), utc());
            assert_eq!(lines.len(), 1, "no bar on a failed read: {lines:?}");
            assert!(
                lines[0].starts_with("Plan: Ultra · Usage unavailable · "),
                "{lines:?}"
            );
            assert!(
                lines[0].contains(reason),
                "reason {reason:?} missing: {lines:?}"
            );
            assert!(
                !lines[0].contains('%'),
                "a failed read rendered a percentage: {lines:?}"
            );
        }
        assert_eq!(
            usage_lines("Free", None, utc()),
            vec!["Plan: Free · Usage unavailable · the usage read did not run"]
        );
    }

    #[test]
    fn reset_follows_the_period_end_in_the_given_offset_and_names_what_it_lacks() {
        let reply = forecast(1.0, 10.0);
        assert_eq!(reset_text(&reply, utc()), "resets Nov 20");
        // 2026-11-20T00:00Z is still Nov 19 in New York; the date is the reader's, not UTC's.
        assert_eq!(
            reset_text(&reply, FixedOffset::west_opt(5 * 3600).expect("est")),
            "resets Nov 19"
        );
        let lifetime = json!({"balance_parts": {"period_end": null, "period_source": "lifetime"}});
        assert_eq!(
            reset_text(&lifetime, utc()),
            "one-time allowance, does not reset"
        );
        let unknown =
            json!({"balance_parts": {"period_end": null, "period_source": "unavailable"}});
        assert_eq!(reset_text(&unknown, utc()), "reset date not reported");
    }

    #[test]
    fn plan_label_matches_the_web_active_plan_else_free() {
        assert_eq!(plan_label(Some("ultra"), Some(true)), "Ultra");
        assert_eq!(plan_label(Some("ultra"), Some(false)), "Free");
        assert_eq!(plan_label(Some("ultra"), None), "Free");
        assert_eq!(plan_label(None, Some(true)), "Free");
    }
}
