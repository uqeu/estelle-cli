use std::collections::BTreeSet;

use estelle_client::AccountResponse;
use estelle_client::CommandReply;
use estelle_client::FleetSnapshot;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use serde_json::Value;

use crate::Theme;
use crate::cols;

const MAX_RECEIPT_ROWS: usize = 14;
const MAX_LIVE_MODELS: usize = 8;
const MAX_WORKERS: usize = 64;
const MAX_CALLS_PER_WORKER: usize = 64;
const MAX_RECEIPT_STAGES: usize = 3;

#[derive(Clone, Debug, PartialEq)]
enum Money {
    Exact(f64),
    Upper(f64),
    Lower(f64),
    Partial(f64),
    NotMeasured,
}

impl Money {
    fn display(&self) -> String {
        match self {
            Self::Exact(value) => format!("${value:.6}"),
            Self::Upper(value) => format!("${value:.6} ceiling"),
            Self::Lower(value) => format!("${value:.6} floor"),
            Self::Partial(value) => format!("${value:.6} partial"),
            Self::NotMeasured => "not measured".to_string(),
        }
    }

    fn exact(&self) -> Option<f64> {
        match self {
            Self::Exact(value) => Some(*value),
            Self::Upper(_) | Self::Lower(_) | Self::Partial(_) | Self::NotMeasured => None,
        }
    }
}

#[derive(Clone, Debug)]
struct CostRow {
    role: String,
    model: String,
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
    vendor: Money,
}

#[derive(Clone, Debug)]
struct Receipt {
    title: String,
    rows: Vec<CostRow>,
    vendor_total: Money,
    estelle_total: Money,
    limit: String,
}

#[derive(Clone, Debug)]
enum Capacity {
    NotRequested,
    Loading,
    Measured {
        held: u64,
        cap: u64,
        remaining: Option<u64>,
        exact: Option<bool>,
    },
    Failed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct CostLedger {
    latest: Option<Receipt>,
    session_vendor_usd: f64,
    session_receipts: usize,
    session_incomplete: bool,
    capacity: Capacity,
}

impl Default for CostLedger {
    fn default() -> Self {
        Self {
            latest: None,
            session_vendor_usd: 0.0,
            session_receipts: 0,
            session_incomplete: false,
            capacity: Capacity::NotRequested,
        }
    }
}

impl CostLedger {
    pub(crate) fn reset_session(&mut self) {
        self.latest = None;
        self.session_vendor_usd = 0.0;
        self.session_receipts = 0;
        self.session_incomplete = false;
    }

    pub(crate) fn observe(&mut self, name: &str, reply: &CommandReply) {
        let receipt = match name {
            "work" => work_receipt(reply),
            "orchestra" => orchestra_receipt(reply),
            _ => return,
        };
        self.session_receipts += 1;
        if let Some(value) = receipt.vendor_total.exact() {
            self.session_vendor_usd += value;
        } else {
            self.session_incomplete = true;
        }
        self.latest = Some(receipt);
        assert!(self.session_vendor_usd.is_finite());
        assert!(self.session_vendor_usd >= 0.0);
    }

    pub(crate) fn capacity_loading(&mut self) {
        self.capacity = Capacity::Loading;
    }

    pub(crate) fn apply_capacity(&mut self, result: Result<Value, String>) {
        self.capacity = match result {
            Ok(value) => capacity_from_value(&value).unwrap_or_else(Capacity::Failed),
            Err(error) => Capacity::Failed(error),
        };
    }

    pub(crate) fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: Theme,
        account: Option<&AccountResponse>,
        fleet: Option<&FleetSnapshot>,
    ) {
        let width = usize::from(area.width);
        let model_width = width.saturating_sub(58).max(12);
        let columns = [
            cols::Col::l(13),
            cols::Col::l(model_width),
            cols::Col::r(8),
            cols::Col::r(8),
            cols::Col::r(17),
        ];
        let mut lines = self.summary_lines(theme, account);
        lines.extend([
            Line::from(""),
            cols::head(
                &columns,
                &[
                    "ROLE / WORKER",
                    "SERVED MODEL",
                    "IN TOKENS",
                    "OUT TOKENS",
                    "VENDOR LIST",
                ],
                theme.ghost(),
                1,
            ),
        ]);
        if let Some(receipt) = &self.latest {
            lines.push(Line::styled(
                &receipt.title,
                Style::default()
                    .fg(theme.semantic())
                    .add_modifier(Modifier::BOLD),
            ));
            for row in receipt.rows.iter().take(MAX_RECEIPT_ROWS) {
                lines.push(cost_line(row, &columns, theme));
            }
            if receipt.rows.is_empty() {
                lines.push(Line::styled(
                    "  No priced role or model rows were returned",
                    Style::default().fg(theme.alert()),
                ));
            }
            lines.push(Line::styled(
                &receipt.limit,
                Style::default().fg(theme.ghost()),
            ));
        } else {
            lines.push(Line::styled(
                "No completed Work or Orchestra receipt in this session",
                Style::default().fg(theme.alert()),
            ));
        }
        append_live_fleet(&mut lines, fleet, &columns, theme);
        lines.extend([
            Line::from(""),
            Line::styled(
                "Ctrl+S or Ctrl+Shift+S close   Esc close   no savings claim is made",
                Style::default().fg(theme.ghost()),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().fg(theme.primary())),
            area,
        );
    }

    fn summary_lines(&self, theme: Theme, account: Option<&AccountResponse>) -> Vec<Line<'static>> {
        let session = if self.session_receipts == 0 {
            "not measured".to_string()
        } else if self.session_incomplete {
            format!(
                "${:.6} measured; session incomplete",
                self.session_vendor_usd
            )
        } else {
            format!("${:.6}", self.session_vendor_usd)
        };
        let (vendor, estelle) = self.latest.as_ref().map_or_else(
            || ("not measured".to_string(), "not measured".to_string()),
            |receipt| {
                (
                    receipt.vendor_total.display(),
                    receipt.estelle_total.display(),
                )
            },
        );
        vec![
            Line::styled(
                "SPEND",
                Style::default()
                    .fg(theme.semantic())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(format!("Session vendor-list estimate  {session}")),
            Line::from(format!("Latest provider-list receipt  {vendor}")),
            Line::from(format!("Latest Estelle token invoice  {estelle}")),
            Line::from(plan_line(account)),
            Line::from(memory_line(&self.capacity)),
            Line::styled(
                "Vendor-list estimates are not Estelle charges or provider invoices.",
                Style::default().fg(theme.ghost()),
            ),
        ]
    }
}

fn work_receipt(reply: &CommandReply) -> Receipt {
    let routing = reply.extra.get("routing");
    let plan = routing
        .and_then(|value| value.get("stage_usage"))
        .and_then(|value| value.get("plan"));
    let implementation = routing
        .and_then(|value| value.get("stage_usage"))
        .and_then(|value| value.get("implementation"));
    let review = routing.and_then(|value| value.get("review"));
    let usages = [
        ("plan", plan),
        ("implement", implementation),
        ("review", review),
    ];
    let rows = usages
        .iter()
        .flat_map(|(role, usage)| usage_rows(role, *usage))
        .collect::<Vec<_>>();
    let usage_values = usages
        .iter()
        .filter_map(|(_, usage)| *usage)
        .collect::<Vec<_>>();
    Receipt {
        title: "WORK RECEIPT".to_string(),
        rows,
        vendor_total: sum_owner(&usage_values, "est_cost_usd", "cost_known"),
        estelle_total: sum_owner(&usage_values, "estelle_billed_usd", "estelle_billed_usd"),
        limit: "Per-role rows come only from routing.stage_usage and routing.review.".to_string(),
    }
}

fn orchestra_receipt(reply: &CommandReply) -> Receipt {
    let usage = reply.extra.get("usage");
    let mut rows = usage_rows("fleet total", usage);
    for agent in reply
        .extra
        .get("agents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_WORKERS)
    {
        let worker = agent
            .get("worker")
            .and_then(Value::as_str)
            .unwrap_or("worker");
        for call in agent
            .get("model_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(MAX_CALLS_PER_WORKER)
        {
            if rows.len() >= MAX_RECEIPT_ROWS {
                break;
            }
            rows.push(row_from_call(worker, call));
        }
    }
    let receipt = reply.extra.get("cost_receipt");
    Receipt {
        title: "ORCHESTRA RECEIPT".to_string(),
        rows,
        vendor_total: receipt_money(receipt, "vendor_list_usd"),
        estelle_total: receipt_money(receipt, "estelle_billed_usd"),
        limit: "Fleet totals are priced by served model. Worker-call cost stays not measured unless the server attaches it.".to_string(),
    }
}

fn usage_rows(role: &str, usage: Option<&Value>) -> Vec<CostRow> {
    usage
        .and_then(|value| value.get("by_model"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_RECEIPT_ROWS)
        .map(|row| CostRow {
            role: role.to_string(),
            model: row
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("not returned")
                .to_string(),
            tokens_in: integer(row.get("tokens_in")),
            tokens_out: integer(row.get("tokens_out")),
            vendor: row_money(row),
        })
        .collect()
}

fn row_from_call(worker: &str, call: &Value) -> CostRow {
    CostRow {
        role: worker.to_string(),
        model: call
            .get("served_model")
            .and_then(Value::as_str)
            .unwrap_or("not returned")
            .to_string(),
        tokens_in: integer(call.get("tokens_in")),
        tokens_out: integer(call.get("tokens_out")),
        vendor: row_money(call),
    }
}

fn row_money(row: &Value) -> Money {
    let Some(value) = amount(row.get("est_cost_usd")) else {
        return Money::NotMeasured;
    };
    if row.get("price_known").and_then(Value::as_bool) != Some(true) {
        return Money::NotMeasured;
    }
    match (
        row.get("cost_is_upper_bound").and_then(Value::as_bool),
        row.get("cost_is_lower_bound").and_then(Value::as_bool),
    ) {
        (Some(true), _) => Money::Upper(value),
        (_, Some(true)) => Money::Lower(value),
        _ => Money::Exact(value),
    }
}

fn receipt_money(receipt: Option<&Value>, field: &str) -> Money {
    let Some(receipt) = receipt else {
        return Money::NotMeasured;
    };
    let Some(value) = amount(receipt.get(field)) else {
        return Money::NotMeasured;
    };
    if field == "estelle_billed_usd" {
        return Money::Exact(value);
    }
    match receipt.get("state").and_then(Value::as_str) {
        Some("measured") => Money::Exact(value),
        Some("upper-bound") => Money::Upper(value),
        Some("lower-bound") => Money::Lower(value),
        Some("incomplete" | "bounded-both-directions") => Money::Partial(value),
        _ => Money::NotMeasured,
    }
}

fn sum_owner(usages: &[&Value], field: &str, known_field: &str) -> Money {
    if usages.is_empty() {
        return Money::NotMeasured;
    }
    let mut total = 0.0;
    assert!(usages.len() <= MAX_RECEIPT_STAGES);
    for usage in usages.iter().take(MAX_RECEIPT_STAGES) {
        let Some(value) = amount(usage.get(field)) else {
            return Money::NotMeasured;
        };
        if field == "est_cost_usd" && usage.get(known_field).and_then(Value::as_bool) != Some(true)
        {
            return Money::NotMeasured;
        }
        total += value;
    }
    if total.is_finite() && total >= 0.0 {
        Money::Exact(total)
    } else {
        Money::NotMeasured
    }
}

fn cost_line(row: &CostRow, columns: &[cols::Col; 5], theme: Theme) -> Line<'static> {
    let tokens_in = token_text(row.tokens_in);
    let tokens_out = token_text(row.tokens_out);
    let vendor = row.vendor.display();
    // `cols::row` BORROWS its cells and three of these are locals, so the row is `Line<'_>`.
    // `cols::owned` re-owns the spans; it exists for exactly this call shape.
    cols::owned(cols::row(
        columns,
        &[
            cols::Cell(&row.role, theme.primary()),
            cols::Cell(&row.model, theme.primary()),
            cols::Cell(&tokens_in, theme.primary()),
            cols::Cell(&tokens_out, theme.primary()),
            cols::Cell(
                &vendor,
                if matches!(row.vendor, Money::NotMeasured) {
                    theme.alert()
                } else {
                    theme.semantic()
                },
            ),
        ],
        1,
    ))
}

fn append_live_fleet(
    lines: &mut Vec<Line<'_>>,
    fleet: Option<&FleetSnapshot>,
    columns: &[cols::Col; 5],
    theme: Theme,
) {
    let Some(fleet) =
        fleet.filter(|fleet| !matches!(fleet.state.as_str(), "done" | "completed" | "failed"))
    else {
        return;
    };
    let models = fleet
        .models
        .iter()
        .map(String::as_str)
        .chain((!fleet.model.is_empty()).then_some(fleet.model.as_str()))
        .collect::<BTreeSet<_>>();
    lines.extend([
        Line::from(""),
        Line::styled(
            format!("ORCHESTRA IN FLIGHT  state {}", fleet.state),
            Style::default()
                .fg(theme.semantic())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    for model in models.into_iter().take(MAX_LIVE_MODELS) {
        let row = CostRow {
            role: "in flight".to_string(),
            model: model.to_string(),
            tokens_in: None,
            tokens_out: None,
            vendor: Money::NotMeasured,
        };
        lines.push(cost_line(&row, columns, theme));
    }
    lines.push(Line::styled(
        "Live snapshots do not join worker tasks to models or costs; completed receipts own those facts.",
        Style::default().fg(theme.ghost()),
    ));
}

fn capacity_from_value(value: &Value) -> Result<Capacity, String> {
    let held = integer(value.get("held_tokens"))
        .ok_or_else(|| "Memory estimate omitted held_tokens".to_string())?;
    let cap = integer(value.get("cap")).ok_or_else(|| "Memory estimate omitted cap".to_string())?;
    let remaining = match value.get("remaining_tokens") {
        Some(Value::Null) | None => None,
        other => integer(other),
    };
    if cap != 0 && remaining.is_none() {
        return Err("Memory estimate omitted remaining_tokens for a limited plan".to_string());
    }
    if remaining.is_some_and(|value| value > cap) {
        return Err("Memory estimate returned remaining_tokens above the plan cap".to_string());
    }
    assert!(cap == 0 || remaining.is_some());
    assert!(cap == 0 || remaining.is_some_and(|value| value <= cap));
    Ok(Capacity::Measured {
        held,
        cap,
        remaining,
        exact: value.get("exact").and_then(Value::as_bool),
    })
}

fn plan_line(account: Option<&AccountResponse>) -> String {
    let budget = account.and_then(|account| amount(account.extra.get("budget_usd")));
    let spent = account.and_then(|account| amount(account.extra.get("period_spend_usd")));
    match (budget, spent) {
        (Some(budget), Some(spent)) => format!(
            "Estelle plan remaining      ${:.2}  (${spent:.2} spent of ${budget:.2})",
            budget - spent
        ),
        _ => "Estelle plan remaining      not measured".to_string(),
    }
}

fn memory_line(capacity: &Capacity) -> String {
    match capacity {
        Capacity::NotRequested => "Memory used / plan          not measured".to_string(),
        Capacity::Loading => {
            "Memory used / plan          measuring against sweep capacity".to_string()
        }
        Capacity::Measured {
            held,
            cap: 0,
            exact,
            ..
        } => format!(
            "Memory used / plan          {} / unlimited{}",
            tokens(*held),
            precision(*exact)
        ),
        Capacity::Measured {
            held,
            cap,
            remaining,
            exact,
        } => format!(
            "Memory used / plan          {} / {}  ({} remaining){}",
            tokens(*held),
            tokens(*cap),
            remaining
                .map(tokens)
                .unwrap_or_else(|| "not measured".to_string()),
            precision(*exact),
        ),
        Capacity::Failed(error) => format!("Memory used / plan          not measured  ({error})"),
    }
}

fn precision(exact: Option<bool>) -> &'static str {
    match exact {
        Some(true) => "",
        Some(false) => "  repo size estimated from bytes",
        None => "  precision not returned",
    }
}

fn tokens(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn token_text(value: Option<u64>) -> String {
    value
        .map(tokens)
        .unwrap_or_else(|| "not measured".to_string())
}

fn integer(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

fn amount(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_cost_is_not_rendered_as_zero_but_received_zero_is() {
        assert_eq!(row_money(&json!({})).display(), "not measured");
        assert_eq!(
            row_money(&json!({"est_cost_usd": 0.0, "price_known": true})).display(),
            "$0.000000"
        );
        assert_eq!(
            receipt_money(
                Some(&json!({"state": "measured", "estelle_billed_usd": 0.0})),
                "estelle_billed_usd"
            )
            .display(),
            "$0.000000"
        );
    }

    #[test]
    fn work_receipt_keeps_role_models_tokens_and_two_money_owners() {
        let reply: CommandReply = serde_json::from_value(json!({"routing": {
            "stage_usage": {
                "plan": {"by_model": [{"model": "claude-opus", "tokens_in": 10, "tokens_out": 2, "est_cost_usd": 0.03, "price_known": true}], "est_cost_usd": 0.03, "cost_known": true, "estelle_billed_usd": 0.0},
                "implementation": {"by_model": [{"model": "kimi-code", "tokens_in": 20, "tokens_out": 4, "est_cost_usd": 0.01, "price_known": true}], "est_cost_usd": 0.01, "cost_known": true, "estelle_billed_usd": 0.0}
            },
            "review": {"by_model": [{"model": "claude-opus", "tokens_in": 5, "tokens_out": 1, "est_cost_usd": 0.005, "price_known": true}], "est_cost_usd": 0.005, "cost_known": true, "estelle_billed_usd": 0.0}
        }})).expect("work reply");
        let receipt = work_receipt(&reply);
        assert_eq!(
            receipt
                .rows
                .iter()
                .map(|row| row.role.as_str())
                .collect::<Vec<_>>(),
            ["plan", "implement", "review"]
        );
        assert_eq!(receipt.rows[1].model, "kimi-code");
        assert_eq!(receipt.rows[1].tokens_in, Some(20));
        assert_eq!(receipt.vendor_total, Money::Exact(0.045));
        assert_eq!(receipt.estelle_total, Money::Exact(0.0));
    }

    #[test]
    fn orchestra_does_not_allocate_global_cost_to_worker_calls() {
        let reply: CommandReply = serde_json::from_value(json!({
            "usage": {"by_model": [{"model": "gpt-5.6-sol", "tokens_in": 30, "tokens_out": 6, "est_cost_usd": 0.02, "price_known": true}]},
            "cost_receipt": {"state": "measured", "vendor_list_usd": 0.02, "estelle_billed_usd": 0.0},
            "agents": [{"worker": "worker-1", "model_calls": [{"served_model": "gpt-5.6-sol", "tokens_in": 30, "tokens_out": 6}]}]
        })).expect("orchestra reply");
        let receipt = orchestra_receipt(&reply);
        assert_eq!(receipt.rows[0].vendor, Money::Exact(0.02));
        assert_eq!(receipt.rows[1].vendor, Money::NotMeasured);
        assert_eq!(receipt.estelle_total, Money::Exact(0.0));
    }

    #[test]
    fn capacity_preserves_unlimited_and_measured_remaining_as_distinct_states() {
        let unlimited = capacity_from_value(
            &json!({"held_tokens": 12, "cap": 0, "remaining_tokens": null, "exact": false}),
        )
        .expect("unlimited");
        let measured = capacity_from_value(
            &json!({"held_tokens": 12, "cap": 100, "remaining_tokens": 88, "exact": true}),
        )
        .expect("measured");
        assert!(memory_line(&unlimited).contains("unlimited"));
        assert!(memory_line(&measured).contains("88 remaining"));
        assert!(
            capacity_from_value(&json!({"held_tokens": 12, "cap": 100})).is_err(),
            "a malformed remote response must render a refusal rather than panic"
        );
    }
}
