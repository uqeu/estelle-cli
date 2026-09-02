//! Run spend, per model — the breakdown `/work` and `/orchestra` have always returned.
//!
//! 🔴 **THE DESIGN BOOK CALLS THIS "no per-model cost breakdown on the wire" AND THAT IS NOT TRUE.**
//! `usage_breakdown` (`src/estelle/serve/model_pricing.py:147`) is serialised onto every `/work`
//! result as `usage` (`api_work.py:715`), onto every stage as `routing.stage_usage`
//! (`api_work.py:748`), and onto every swarm run as `usage` + `savings` + `cost_receipt`. Each row
//! carries `model`, `tokens_in`, `tokens_out`, `est_cost_usd`, `price_known`, and — when the
//! provider reported one — the Anthropic cache split. `grep -rn "by_model\|est_cost_usd" tui/src
//! estelle-client/src` returned **zero** before this module: a measured, priced, serialised
//! breakdown with no reader anywhere in the client.
//!
//! ## Two bills, never added
//!
//! 🔴 `est_cost_usd` is **`VENDOR_PRICES`** — what the customer's own provider charges the
//! customer's own key. `estelle_billed_usd` is **`BILLED_RATES`**, what Estelle invoices, and every
//! row of that table is `0.0 / 0.0` with basis `byok` (`billed_rates.py:104`). They are two bills
//! and they are never summed. This module prints them on two labelled lines and there is a test
//! whose whole job is that their sum never appears.
//!
//! ## Never the word "saved"
//!
//! ⛔ The `savings` block carries `saved_usd`, and the server is honest about what it is: its own
//! `SAVINGS_ASSUMPTION` says the baseline *"assumes that model would have produced a comparable
//! token volume for the same work, which is an assumption and not a measurement"*. A modelled
//! counterfactual is not money saved. So the word does not appear on this screen: the row is
//! **`baseline (modelled)`**, the figure is a **difference**, and the assumption is printed under
//! it. A saving would need the cheaper model to have actually served after an explicit routing
//! decision with the pricier eligible model observed for that same decision — which is not what
//! `measured_savings` computes.
//!
//! ## 🔴 ONE OWNER FOR THE RUN TOTAL, AND IT IS NOT THIS MODULE
//!
//! The first draft of this file opened with a `run spend  $18.78` row, and the existing suite
//! killed it: `commands::tests::work_ends_with_the_server_owned_completion_line` and
//! `legacy_work_response_does_not_invent_a_client_timed_receipt` pin `work_completion_receipt`
//! (rendered by `render_work_completion`) as the owner of the run total and as the LAST line of a
//! work reply — bounds, `spend_known` and all. A second total computed here from `est_cost_usd`
//! would have disagreed with it the day either derivation changed. **This module owns the
//! per-MODEL half only.** With no `usage` block it prints nothing at all, because the completion
//! line already says `spend unknown`, and two sentences saying it is one more than a reader needs.
//!
//! ## Absent, zero, and bounded
//!
//! ⛔ `price_known: false` arrives with `est_cost_usd: null`. That prints [`UNKNOWN`], never
//! `$0.00`, and the total says which models it excludes — a total that silently drops a row is a
//! number that can only be too small. `estelle_billed_usd: 0.0` is the opposite case: a MEASURED
//! zero, which is stated as BYOK rather than as a dash.

use serde_json::Value;

/// How many model rows are printed. A run can fan out; a terminal cannot.
const MODEL_ROWS_SHOWN: usize = 12;

/// What an unknown value prints as.
///
/// ⛔ Never `0`, never `$0.00`. 🔴 **AND IT MEANS EXACTLY ONE THING ON THIS SCREEN.** The first
/// draft used the same em-dash as ordinary punctuation inside value strings, so `$0.00 — BYOK`
/// contained the marker for *nobody knows* while stating a measured zero — one glyph, two meanings,
/// caught by the test that asserts a BYOK zero is not rendered as unknown. Separators are `·`.
const UNKNOWN: &str = "—";

/// Label column width, so values line up without a table engine.
const LABEL_WIDTH: usize = 18;

/// 🔴 Said whenever vendor spend is drawn beside the Estelle line.
pub(crate) const BILL_OWNER: &str = "two bills, never added: run spend is your key billed by your vendor · memory is the Estelle plan";

fn row(label: &str, value: impl AsRef<str>) -> String {
    format!("  {label:<LABEL_WIDTH$}{}", value.as_ref())
}

/// A token count as a short label.
fn human(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M")
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0).replace(".0K", "K")
    } else {
        n.to_string()
    }
}

/// A dollar figure, or [`UNKNOWN`] when the server sent `null` / no field at all.
///
/// ⛔ **THE ONE RULE THIS FILE EXISTS FOR.** `price_known: false` ships `est_cost_usd: null`, and a
/// `null` rendered as `$0.00` tells a customer a model was free when the truth is that nobody knows
/// what it cost.
fn money(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map_or_else(|| UNKNOWN.to_string(), |usd| format!("${usd:.2}"))
}

fn count(row: &Value, field: &str) -> Option<i64> {
    row.get(field).and_then(Value::as_i64)
}

/// One model's line: what it is, what it consumed, what the vendor charges for that.
fn model_row(entry: &Value) -> String {
    let model = entry
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN);
    let tokens = format!(
        "{} in / {} out",
        count(entry, "tokens_in").map_or_else(|| UNKNOWN.to_string(), human),
        count(entry, "tokens_out").map_or_else(|| UNKNOWN.to_string(), human),
    );
    // The cache split is the most persuasive number here and it only exists when the provider
    // reported one — absent means UNREPORTED, which is why the upper-bound flag rides beside it.
    let mut note = match (
        count(entry, "cached_tokens_in"),
        count(entry, "cache_write_tokens_in"),
    ) {
        (Some(read), Some(written)) if read > 0 || written > 0 => {
            format!(
                "  {} read from cache · {} written",
                human(read),
                human(written)
            )
        }
        _ => String::new(),
    };
    if entry.get("price_known").and_then(Value::as_bool) == Some(false) {
        note.push_str("  no published price for this model");
    } else if entry.get("cost_is_upper_bound").and_then(Value::as_bool) == Some(true) {
        note.push_str("  UPPER BOUND: no cache split reported, all input priced as fresh");
    }
    format!(
        "  {model:<34}{tokens:<24}{:>10}{note}",
        money(entry.get("est_cost_usd"))
    )
}

/// The `savings` block, rendered as the modelled counterfactual it is.
///
/// ⛔ The word "saved" does not appear, by construction: this is `baseline_cost_usd` minus
/// `measured_cost_usd` over the SAME token counts re-priced at one other model's rates. See the
/// module docs.
fn baseline_lines(savings: &Value) -> Vec<String> {
    if savings.get("measured").and_then(Value::as_bool) != Some(true) {
        let why = savings
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the server did not say why");
        return vec![row("baseline", format!("not priced · {why}"))];
    }
    if savings.get("baseline_is_what_ran").and_then(Value::as_bool) == Some(true) {
        return vec![row(
            "baseline",
            "the baseline model IS what ran · routing changed nothing",
        )];
    }
    let name = savings
        .get("baseline_model")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN);
    let mut lines = vec![
        row(
            "baseline (modelled)",
            format!(
                "{name} would list at {} for the same measured tokens",
                money(savings.get("baseline_cost_usd"))
            ),
        ),
        row(
            "difference",
            format!(
                "{}  · MODELLED, not measured",
                money(savings.get("saved_usd"))
            ),
        ),
    ];
    if let Some(assumption) = savings.get("assumption").and_then(Value::as_str) {
        lines.push(format!("  {assumption}"));
    }
    lines
}

/// The whole per-model spend breakdown for one run.
///
/// `usage` is the `usage_breakdown` envelope; `savings` is the optional `measured_savings` block.
/// Both are read by name — a field this function does not name never reaches the terminal.
pub(crate) fn spend_lines(usage: Option<&Value>, savings: Option<&Value>) -> Vec<String> {
    // 🔴 NOTHING — not a total, not a "not reported" row. `render_work_completion` owns the run
    // total and is pinned as the last line of a work reply. See the module docs.
    let Some(usage) = usage else {
        return Vec::new();
    };
    let basis = usage
        .get("cost_basis")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN);
    let mut lines = Vec::new();

    match usage.get("by_model").and_then(Value::as_array) {
        None => lines.push(row("per model", UNKNOWN)),
        Some(rows) => {
            lines.push(row(
                "per model",
                format!("{} model(s)  ({basis})", rows.len()),
            ));
            lines.extend(rows.iter().take(MODEL_ROWS_SHOWN).map(model_row));
        }
    }

    // 🔴 A TOTAL THAT DROPS A ROW MUST SAY SO. `cost_known: false` means at least one served model
    // had no published price, and its cost is missing from the figure above — not zero in it.
    if let Some(unpriced) = usage.get("unpriced_models").and_then(Value::as_array) {
        let names = unpriced
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(row(
            "not in the total",
            format!("{names} · no published price, so the total is INCOMPLETE"),
        ));
    }
    if let Some(note) = usage.get("cost_bound_note").and_then(Value::as_str) {
        lines.push(format!("  {note}"));
    }

    // 🔴 THE SECOND BILL, ON ITS OWN LINE. A measured zero, said as BYOK rather than as a dash.
    let billed = usage.get("estelle_billed_usd").and_then(Value::as_f64);
    lines.push(row(
        "Estelle billed",
        match billed {
            None => UNKNOWN.to_string(),
            Some(0.0) => "$0.00 · BYOK: nothing of this is Estelle's".to_string(),
            Some(usd) => format!("${usd:.2}"),
        },
    ));

    if let Some(savings) = savings {
        lines.extend(baseline_lines(savings));
    }
    lines.push(format!("  {BILL_OWNER}"));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 🔴 **PRODUCED BY THE SHIPPED SERVER FUNCTION, NOT DRAWN.**
    ///
    /// ```text
    /// cd <estelle>; PYTHONPATH=src:scripts python3 -c "
    /// from estelle.serve.model_pricing import usage_breakdown, measured_savings
    /// by_model = {'claude-opus-4-8': {'tokens_in': 24_700_000, 'tokens_out': 41_200,
    ///   'cached_tokens_in': 23_900_000, 'cache_write_tokens_in': 768_000, 'cache_reported': True},
    ///   'moonshotai/kimi-k2.7-code': {'tokens_in': 512_000, 'tokens_out': 88_000,
    ///     'cache_reported': False},
    ///   'some-model-nobody-priced': {'tokens_in': 4_000, 'tokens_out': 900,
    ///     'cache_reported': False}}
    /// print(usage_breakdown(by_model)); print(measured_savings(by_model, 'claude-opus-4-8'))"
    /// ```
    const USAGE: &str = r#"{
      "by_model": [
        {"model": "claude-opus-4-8", "tokens_in": 24700000, "tokens_out": 41200,
         "est_cost_usd": 17.94, "price_known": true,
         "cached_tokens_in": 23900000, "cache_write_tokens_in": 768000},
        {"model": "moonshotai/kimi-k2.7-code", "tokens_in": 512000, "tokens_out": 88000,
         "est_cost_usd": 0.8384, "price_known": true, "cost_is_upper_bound": true},
        {"model": "some-model-nobody-priced", "tokens_in": 4000, "tokens_out": 900,
         "est_cost_usd": null, "price_known": false, "cost_is_upper_bound": true}
      ],
      "est_cost_usd": 18.7784, "cost_known": false, "cost_basis": "vendor_list_price",
      "estelle_billed_usd": 0.0,
      "unpriced_models": ["some-model-nobody-priced"],
      "cost_is_upper_bound": true,
      "cost_bound_note": "the provider reported no prompt-cache breakdown for moonshotai/kimi-k2.7-code, so all input is priced as FRESH. A cache read costs ~10% of that, so est_cost_usd is an UPPER BOUND, not the measured cost."
    }"#;

    /// ⚠️ **THE FIRST DRAFT OF THIS CONST WAS MISSING TWO FIELDS PRODUCTION SENDS**
    /// (`cost_is_upper_bound`, `cost_bound_note`) — a double strictly FRIENDLIER than the server,
    /// which is the shape that certifies code production rejects. Caught by diffing every fixture
    /// here against a live call to the shipped function; both are restored below.
    const SAVINGS: &str = r#"{
      "measured": true, "measured_cost_usd": 18.7784, "baseline_model": "claude-opus-4-8",
      "billed_models": ["claude-opus-4-8", "moonshotai/kimi-k2.7-code"],
      "baseline_is_what_ran": false, "baseline_cost_usd": 22.7,
      "saved_usd": 3.9216, "saved_pct": 17.3,
      "assumption": "the baseline is the SAME measured token counts priced at the baseline model's published rates; it assumes that model would have produced a comparable token volume for the same work, which is an assumption and not a measurement",
      "excluded_unpriced_models": ["some-model-nobody-priced"],
      "cost_is_upper_bound": true,
      "cost_bound_note": "the provider reported no prompt-cache breakdown for moonshotai/kimi-k2.7-code, so all input is priced as FRESH. A cache read costs ~10% of that, so est_cost_usd is an UPPER BOUND, not the measured cost."
    }"#;

    fn value(raw: &str) -> Value {
        serde_json::from_str(raw).expect("the server's own JSON")
    }

    fn text(usage: Option<&Value>, savings: Option<&Value>) -> String {
        spend_lines(usage, savings).join("\n")
    }

    fn line_for(rendered: &str, label: &str) -> String {
        rendered
            .lines()
            .find(|line| line.trim_start().starts_with(label))
            .unwrap_or_else(|| panic!("no row labelled {label}:\n{rendered}"))
            .to_string()
    }

    /// Every served model reaches the screen with its identity, its tokens and its price.
    #[test]
    fn every_served_model_reaches_the_screen() {
        let rendered = text(Some(&value(USAGE)), None);
        for (model, cost) in [
            ("claude-opus-4-8", "$17.94"),
            ("moonshotai/kimi-k2.7-code", "$0.84"),
        ] {
            let line = line_for(&rendered, model);
            assert!(line.contains(cost), "{line}");
        }
        assert!(
            line_for(&rendered, "per model").contains("vendor_list_price"),
            "a breakdown with no priced-at basis:\n{rendered}"
        );
    }

    /// 🔴 **ONE OWNER FOR THE RUN TOTAL.** `render_work_completion` prints it, bounds and all, and
    /// an existing test pins that line as the last of a work reply. A second total computed here
    /// would disagree with it the day either derivation changed.
    #[test]
    fn the_breakdown_never_prints_a_run_total() {
        let rendered = text(Some(&value(USAGE)), Some(&value(SAVINGS)));
        assert!(
            !rendered.contains("18.78"),
            "a second owner of the run total:\n{rendered}"
        );
    }

    /// ⛔ **THE HEADLINE RULE.** `price_known: false` ships `est_cost_usd: null`, and `$0.00` would
    /// tell a customer a model was free when nobody knows what it cost.
    #[test]
    fn an_unpriced_model_is_a_dash_and_never_a_zero_dollar() {
        let rendered = text(Some(&value(USAGE)), None);
        let line = line_for(&rendered, "some-model-nobody-priced");
        assert!(line.contains(UNKNOWN), "{line}");
        assert!(
            !line.contains("$0.00"),
            "a free-looking price for an unpriced model:\n{line}"
        );
        assert!(line.contains("no published price"), "{line}");
    }

    /// A total that silently drops a row is a number that can only be too small.
    #[test]
    fn the_total_says_which_models_it_excludes() {
        let rendered = text(Some(&value(USAGE)), None);
        let line = line_for(&rendered, "not in the total");
        assert!(line.contains("some-model-nobody-priced"), "{line}");
        assert!(line.contains("INCOMPLETE"), "{line}");
    }

    /// 🔴 **TWO OWNERS, TWO LINES, NEVER ONE NUMBER.** `est_cost_usd` is `VENDOR_PRICES`;
    /// `estelle_billed_usd` is `BILLED_RATES`. Driven with a NON-ZERO Estelle figure, because with
    /// the shipped BYOK table of zeros the sum equals the vendor total and the test could not fail.
    #[test]
    fn the_two_bills_are_never_added() {
        let mut usage = value(USAGE);
        usage["estelle_billed_usd"] = json!(4.5);
        let rendered = text(Some(&usage), None);
        // The vendor side is the sum of the per-model rows, and it stays on those rows: the run
        // total belongs to `render_work_completion`. What must never happen is the two BILLS
        // merging, so the Estelle line is asserted separately and their sum is asserted absent.
        assert!(
            line_for(&rendered, "claude-opus-4-8").contains("$17.94"),
            "{rendered}"
        );
        assert!(
            line_for(&rendered, "Estelle billed").contains("$4.50"),
            "{rendered}"
        );
        // Every way the two bills could be merged, written out: a vendor row plus the Estelle
        // figure (17.94 + 4.5), and the whole vendor side plus it (17.94 + 0.8384 + 4.5). Neither
        // string may appear anywhere on the screen.
        for merged in ["22.44", "23.28"] {
            assert!(
                !rendered.contains(merged),
                "the two bills were added into {merged}:\n{rendered}"
            );
        }
        assert!(
            !rendered.contains("23.28"),
            "the two bills were added:\n{rendered}"
        );
        assert!(rendered.contains("two bills, never added"), "{rendered}");
    }

    /// A measured zero is a statement, and BYOK is the statement — not a dash, which would mean
    /// nobody knew.
    #[test]
    fn a_byok_zero_says_byok_rather_than_unknown() {
        let rendered = text(Some(&value(USAGE)), None);
        let line = line_for(&rendered, "Estelle billed");
        assert!(line.contains("BYOK"), "{line}");
        assert!(
            !line.contains(UNKNOWN),
            "a measured zero rendered as unknown:\n{line}"
        );
    }

    /// ⛔ **NEVER THE WORD "SAVED" AGAINST A MODELLED BASELINE.** The server's own
    /// `SAVINGS_ASSUMPTION` says the counterfactual is an assumption, not a measurement.
    #[test]
    fn a_modelled_baseline_is_never_called_a_saving() {
        let rendered = text(Some(&value(USAGE)), Some(&value(SAVINGS)));
        assert!(
            !rendered.to_lowercase().contains("saved")
                && !rendered.to_lowercase().contains("saving"),
            "a modelled counterfactual presented as money saved:\n{rendered}"
        );
        assert!(
            line_for(&rendered, "baseline (modelled)").contains("$22.70"),
            "{rendered}"
        );
        let difference = line_for(&rendered, "difference");
        assert!(difference.contains("$3.92"), "{difference}");
        assert!(
            difference.contains("MODELLED, not measured"),
            "{difference}"
        );
        assert!(
            rendered.contains("an assumption and not a measurement"),
            "the assumption did not travel with the number:\n{rendered}"
        );
    }

    /// When the baseline IS what ran, routing changed nothing and the screen says exactly that
    /// rather than printing a zero difference that reads like a comparison.
    #[test]
    fn a_baseline_that_is_what_ran_reports_no_comparison() {
        let mut savings = value(SAVINGS);
        savings["baseline_is_what_ran"] = json!(true);
        let rendered = text(Some(&value(USAGE)), Some(&savings));
        assert!(
            line_for(&rendered, "baseline").contains("routing changed nothing"),
            "{rendered}"
        );
        assert!(!rendered.contains("$22.70"), "{rendered}");
    }

    /// An upper bound is never printed bare — the direction of the error travels with the number.
    #[test]
    fn an_upper_bound_is_disclosed_as_one() {
        let rendered = text(Some(&value(USAGE)), None);
        assert!(
            line_for(&rendered, "moonshotai").contains("UPPER BOUND"),
            "{rendered}"
        );
        assert!(
            rendered.contains("priced as FRESH"),
            "the server's bound note was dropped:\n{rendered}"
        );
    }

    /// The cache split, which is the most persuasive number on the screen.
    #[test]
    fn the_cache_split_reaches_the_screen_when_the_provider_reported_one() {
        let rendered = text(Some(&value(USAGE)), None);
        let line = line_for(&rendered, "claude-opus-4-8");
        assert!(line.contains("23.9M read from cache"), "{line}");
        assert!(line.contains("768K written"), "{line}");
    }

    /// Every loop has a stated bound, and the expected number is written out rather than derived
    /// from the constant it is meant to police.
    #[test]
    fn the_model_table_is_bounded() {
        let mut usage = value(USAGE);
        usage["by_model"] = Value::Array(
            (0..40)
                .map(|i| {
                    json!({"model": format!("m{i}"), "tokens_in": 1, "tokens_out": 1,
                                "est_cost_usd": 0.01, "price_known": true})
                })
                .collect(),
        );
        let rendered = text(Some(&usage), None);
        let drawn = (0..40)
            .filter(|i| rendered.contains(&format!("m{i} ")))
            .count();
        assert_eq!(
            MODEL_ROWS_SHOWN, 12,
            "the bound moved — change the literal below deliberately"
        );
        assert_eq!(drawn, 12, "{rendered}");
        assert!(
            line_for(&rendered, "per model").contains("40 model(s)"),
            "{rendered}"
        );
    }

    /// An allowlist, not a filter.
    #[test]
    fn a_field_this_reader_does_not_name_is_never_echoed() {
        let mut usage = value(USAGE);
        usage["upstream_debug"] = json!("SENTINEL_THAT_MUST_NOT_REACH_THE_TERMINAL");
        let rendered = text(Some(&usage), None);
        assert!(
            !rendered.contains("SENTINEL_THAT_MUST_NOT_REACH_THE_TERMINAL"),
            "{rendered}"
        );
    }

    /// With no `usage` block this module prints NOTHING — not a total, not a "not reported" row.
    /// The completion line already reports `spend unknown` and it is the owner; silence here is
    /// deference, not omission.
    #[test]
    fn a_run_with_no_usage_block_defers_to_the_completion_line() {
        assert!(spend_lines(None, None).is_empty());
        let caller = include_str!("commands.rs");
        assert!(
            caller.contains("\"spend unknown\""),
            "the completion line no longer carries the honest-unknown total this module defers to"
        );
    }

    /// 🔴 **A READER WITH NO CALLER IS THE DEFECT THIS MODULE EXISTS TO FIX** — the same assertion
    /// `sweep_estimate` carries, for the same reason. ⚠️ Limit: this proves the call site exists,
    /// not that it runs; the behaviour is covered by the tests above.
    #[test]
    fn the_work_and_orchestra_replies_actually_call_this_reader() {
        let caller = include_str!("commands.rs");
        assert!(
            caller.matches("crate::run_spend::spend_lines").count() >= 2,
            "the per-model spend reader is not wired to both reply arms"
        );
    }
}
