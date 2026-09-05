//! `GET /spend` — what THIS account's own provider keys were charged, as a vendor-list ESTIMATE.
//!
//! 🔴 **THE DEFECT THIS CLOSES.** The SPEND screen read `CommandReply` values observed in the
//! CURRENT SESSION and nothing else, so a user who had not just run `/work` or `/orchestra` saw
//! `not measured` on every row — a true statement about the session that reads as a claim about the
//! account. The server grew `GET /spend?days=` on 2026-09-05 (`src/estelle/serve/provider_spend.py`
//! in the API repo); this module is the wire to it.
//!
//! 🔴 **WHAT THE NUMBER IS, said before the number is drawn.** It is published list prices applied
//! to a token count Estelle recorded, and the recorded count is a COMBINED input+output total, so
//! no split is known. That is why the server ships `floor_usd`/`ceiling_usd` beside every
//! `estimate_usd` and why this renderer refuses to draw one without the other: the bracket can span
//! ~50x, and a lone figure in a money column claims a precision this data does not have.
//!
//! ⚠️ **A ZERO IS NEVER A MISSING MEASUREMENT, AND THAT RULE LIVES IN EXACTLY ONE PLACE HERE.**
//! [`money_for`] is the only function that turns a `state` plus a JSON number into something this
//! screen will print, and everything downstream — the table cell AND the reconciliation arithmetic —
//! reads its result. So an `unavailable` row cannot render `$0.00` and cannot be summed into a
//! total, and neither of those is a property of the renderer remembering to check.

use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use serde_json::Value;
use serde_json::json;

use super::costs::Money;
use super::costs::tokens;
use crate::Theme;
use crate::cols;

/// The period this screen asks the server for, in days.
///
/// ONE OWNER: the query [`spend_query`] sends and the heading [`SpendReport::lines`] prints both
/// read this constant, so a screen captioned "last 30 days" cannot be showing seven.
pub(crate) const PERIOD_DAYS: u64 = 30;

/// The label a `by_provider` row is printed under when the server could not tie its models to
/// exactly one of the account's keys. It is a row, not a footnote, because the provider column plus
/// this row is what has to equal the footer.
const UNATTRIBUTED: &str = "unattributed";

/// The footer row's label. Deliberately the SERVER's `total`, never a sum this screen computed.
const TOTAL: &str = "TOTAL";

/// The basis string this renderer knows how to draw. An envelope that declares anything else is
/// refused rather than rendered: an unrecognised basis means we do not know what kind of number we
/// were handed, and a money column is the last place to guess.
const EXPECTED_BASIS: &str = "vendor_list_estimate";

/// Bounds, all named, all applied before the list is taken (Power of Ten rule 3).
const MAX_PROVIDER_ROWS: usize = 12;
const MAX_UNPRICED_NAMED: usize = 3;
const MAX_NOTE_LINES: usize = 8;

/// Half of the server's rounding unit. Every dollar figure in the envelope is `round(x, 6)`, so a
/// sum of `n` rounded parts can differ from the rounded sum of the same underlying values by at most
/// `n` halves of that unit. [`tolerance`] turns that into a bound rather than a guessed epsilon.
const HALF_MICRODOLLAR: f64 = 5e-7;

/// The query `GET /spend` is called with. Lives here, beside [`PERIOD_DAYS`], so the caller cannot
/// ask for one window while this screen captions another.
pub(crate) fn spend_query() -> Value {
    json!({ "days": PERIOD_DAYS })
}

/// One money row: a provider, the unattributed bucket, or the report total.
///
/// `estimate` is NOT read from the envelope directly — it is [`Money::amount`] of the SAME `money`
/// value the cell renders. That is the point: display and arithmetic have one owner, so a row that
/// prints "not measured" is structurally incapable of contributing to a sum.
#[derive(Clone, Debug, PartialEq)]
struct SpendRow {
    label: String,
    calls: Option<u64>,
    tokens: Option<u64>,
    money: Money,
    estimate: Option<f64>,
    bracket: Option<(f64, f64)>,
    reports_own_cost: bool,
    note: Option<String>,
}

/// Whether the provider column plus the unattributed row actually equals the footer.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Balance {
    /// The parts add up to the total within the server's own rounding unit.
    Reconciled,
    /// Nothing was priced, so there is no arithmetic to check.
    NothingPriced,
    /// The parts do NOT add up. Rendered as a loud line rather than silently drawn over.
    ///
    /// ⚠️ `total` is an `Option` for the same reason every other figure on this screen is: the
    /// footer can be UNMEASURED while rows below it are priced, and the first draft of this variant
    /// carried an `f64` and reported that case as "the server's total is $0.000000" — printing a
    /// zero for an absence, inside the very line whose job is to catch a money table lying.
    Mismatch { parts: f64, total: Option<f64> },
}

#[derive(Clone, Debug)]
pub(super) struct SpendReport {
    period_days: u64,
    records_read: u64,
    records_capped: bool,
    rows: Vec<SpendRow>,
    unattributed: Option<SpendRow>,
    total: SpendRow,
    estelle_charged: Money,
    assumption: String,
    notes: Vec<String>,
}

/// Turn a `state` plus the envelope's `estimate_usd` into the one value this screen will print.
///
/// 🔴 **THE LOAD-BEARING FUNCTION.** Read the refusals first:
///
/// * `unavailable` is `NotMeasured` **whatever number arrives with it**. The server sends `null`,
///   but a future server that sent `0.0` with that state must still not print `$0.00` — the STATE
///   is the claim, and the number is only ever a detail of it. It is an explicit arm rather than a
///   guard clause above the match ON PURPOSE: an early `if state == "unavailable"` was written
///   here first, and a mutant that DELETED it did not fail a single test, because `unavailable`
///   then fell to the same `_` arm and produced the identical answer. A refusal no mutation can
///   reach is decoration; as an arm it can be flipped, and flipping it goes red.
/// * `no-vendor-bill` is a real, exact `$0.00`, and it is refused if the accompanying figure is
///   not zero. "Every served model is zero-rated" and "we computed $4.10" cannot both be true, and
///   the honest response to a contradiction is to say nothing rather than pick a side.
/// * `measured` is the ONLY state that renders an unqualified dollar figure, and it means a
///   PROVIDER reported a cost. No provider Estelle calls reports one today, so this arm is
///   expected to be unreachable in production; it exists so that the day one does, the screen does
///   not have to change to tell the truth about it.
/// * An UNRECOGNISED state prints nothing. A new state name arriving from a newer server is
///   exactly the case where rendering the number anyway would invent a claim.
fn money_for(state: &str, estimate: Option<f64>) -> Money {
    // No re-validation of `value` here. [`amount`] is the one owner of "is this a usable dollar
    // figure" and every caller reaches this through it; a second copy of that rule would be a
    // second owner of one derived fact, and — measured the same way the deleted clauses above were
    // — an unkillable one, because the boundary would always have rejected the input first.
    let Some(value) = estimate else {
        return Money::NotMeasured;
    };
    match state {
        "unavailable" => Money::NotMeasured,
        "measured" => Money::Exact(value),
        "no-vendor-bill" if value == 0.0 => Money::NoVendorBill,
        "upper-bound" => Money::Upper(value),
        "lower-bound" => Money::Lower(value),
        "incomplete" | "bounded-both-directions" => Money::Estimate(value),
        _ => Money::NotMeasured,
    }
}

fn amount(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn count(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The floor..ceiling pair, or `None` when the server did not ship a usable one.
///
/// TWO CLAUSES, AND THAT IS THE WHOLE LIST: both ends must be present and usable numbers
/// ([`amount`] owns what "usable" means), and the pair must CONTAIN the estimate it sits beside. A
/// "bracket" that does not contain the number beside it is not a weaker claim than no bracket — it
/// is a wrong one.
///
/// ⚠️ **THERE WAS A THIRD CLAUSE — `floor > ceiling` — AND A MUTANT PROVED IT WAS DECORATION.**
/// Deleting it failed no test, and it could not: an inverted pair defines an EMPTY interval, so the
/// containment clause below already rejects it for every possible estimate. The only case
/// containment misses is an absent estimate, and a row with no estimate is `Money::NotMeasured`,
/// whose bracket cell [`bracket_text`] never draws. So the clause had no reachable effect and is
/// gone; `a_bracket_whose_floor_is_above_its_ceiling_is_refused` keeps the BEHAVIOUR pinned and
/// names which clause enforces it.
fn bracket_for(row: &Value, estimate: Option<f64>) -> Option<(f64, f64)> {
    let floor = amount(row.get("floor_usd"))?;
    let ceiling = amount(row.get("ceiling_usd"))?;
    if estimate.is_some_and(|value| value < floor || value > ceiling) {
        return None;
    }
    Some((floor, ceiling))
}

/// The dim sub-line a row carries, or `None` for an unremarkable one.
///
/// Additive on purpose: a healthy row draws no extra line at all rather than a line saying nothing.
/// Two rows are NOT unremarkable and both are the same rule — a figure that is missing something:
/// an `unavailable` row carries the server's `reason` (which absence this is), and an `incomplete`
/// one says that part of its own period was not priced at any rate, so its dollars are a FLOOR.
fn note_for(row: &Value, state: &str) -> Option<String> {
    if let Some(reason) = text(row.get("reason")) {
        return Some(reason);
    }
    if state != "incomplete" {
        return None;
    }
    let named = row
        .get("unpriced_models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(Value::as_str)
                .take(MAX_UNPRICED_NAMED)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|named| !named.is_empty());
    Some(named.map_or_else(
        || {
            "part of this period could not be priced at any rate and is in NO figure on this row"
                .to_string()
        },
        |named| format!("model(s) with no published price, in NO figure on this row: {named}"),
    ))
}

fn spend_row(label: &str, value: &Value) -> SpendRow {
    let state = text(value.get("state")).unwrap_or_default();
    let money = money_for(&state, amount(value.get("estimate_usd")));
    let estimate = money.amount();
    SpendRow {
        label: label.to_string(),
        bracket: bracket_for(value, estimate),
        note: note_for(value, &state),
        calls: count(value.get("calls")),
        tokens: count(value.get("tokens")),
        money,
        estimate,
        reports_own_cost: value.get("reports_own_cost").and_then(Value::as_bool) == Some(true),
    }
}

/// The largest difference between "the parts" and "the total" that the server's own rounding can
/// produce. `parts` is the number of independently-rounded figures being added up.
fn tolerance(parts: usize) -> f64 {
    HALF_MICRODOLLAR * (parts as f64 + 2.0)
}

/// Does the provider column plus the unattributed row equal the footer?
///
/// 🔴 **THIS IS CHECKED, NOT ASSUMED.** The server documents that `by_provider + unattributed_total
/// == total` by construction, and a documented invariant is exactly the kind of claim that is worth
/// one comparison at the point of drawing: a money table whose column does not add up to its own
/// footer is read as an arithmetic bug in the product, and the reader has no way to tell that from
/// a real one. When it does not reconcile the screen SAYS SO instead of drawing over it.
fn balance(rows: &[SpendRow], unattributed: Option<&SpendRow>, total: &SpendRow) -> Balance {
    let parts = rows
        .iter()
        .chain(unattributed)
        .filter_map(|row| row.estimate)
        .collect::<Vec<_>>();
    let Some(expected) = total.estimate else {
        return if parts.is_empty() {
            Balance::NothingPriced
        } else {
            // Priced rows under an UNMEASURED footer. That is a mismatch and it is reported as one,
            // with the footer named as absent rather than as zero.
            Balance::Mismatch {
                parts: parts.iter().sum(),
                total: None,
            }
        };
    };
    let sum = parts.iter().sum::<f64>();
    if (sum - expected).abs() <= tolerance(parts.len()) {
        Balance::Reconciled
    } else {
        Balance::Mismatch {
            parts: sum,
            total: Some(expected),
        }
    }
}

/// Read one `/spend` envelope, or say why it could not be read.
///
/// Refuses rather than renders when the envelope does not declare the basis this screen knows how
/// to draw, or when it declares itself not an estimate. Both refusals are the same rule: this
/// renderer bolts a specific meaning onto every number it prints, and a number whose meaning is not
/// the one advertised must not be printed under that meaning.
pub(super) fn parse(value: &Value) -> Result<SpendReport, String> {
    let basis = text(value.get("basis")).unwrap_or_default();
    if basis != EXPECTED_BASIS {
        return Err(format!(
            "the spend envelope declared basis {basis:?}, not {EXPECTED_BASIS:?}; this screen \
             renders the vendor-list estimate only"
        ));
    }
    if value.get("is_estimate").and_then(Value::as_bool) != Some(true) {
        return Err(
            "the spend envelope did not declare itself an estimate; this screen labels every \
             figure as one and will not relabel the server's"
                .to_string(),
        );
    }
    let total = value
        .get("total")
        .filter(|total| total.is_object())
        .ok_or_else(|| "the spend envelope carried no total".to_string())?;
    let rows = value
        .get("by_provider")
        .and_then(Value::as_array)
        .ok_or_else(|| "the spend envelope carried no by_provider list".to_string())?
        .iter()
        .take(MAX_PROVIDER_ROWS)
        .map(|row| {
            let label = text(row.get("provider")).unwrap_or_else(|| "not named".to_string());
            spend_row(&label, row)
        })
        .collect::<Vec<_>>();
    let unattributed = value
        .get("unattributed_total")
        .filter(|total| total.is_object())
        .map(|total| spend_row(UNATTRIBUTED, total));
    let mut notes = Vec::new();
    for key in ["cap_note", "unattribution_note", "unnamed_note"] {
        if let Some(note) = text(value.get(key)) {
            notes.push(note);
        }
    }
    Ok(SpendReport {
        period_days: count(value.get("period_days")).unwrap_or(PERIOD_DAYS),
        records_read: count(value.get("records_read")).unwrap_or(0),
        records_capped: value.get("records_capped").and_then(Value::as_bool) == Some(true),
        rows,
        unattributed,
        total: spend_row(TOTAL, total),
        // Estelle's OWN charge. `estelle_charged_usd` is an EXACT invoice, never an estimate, which
        // is why it maps to `Exact` and is never handed to `money_for`: it is a different bill.
        estelle_charged: amount(value.get("estelle_charged_usd"))
            .map_or(Money::NotMeasured, Money::Exact),
        assumption: text(value.get("assumption")).unwrap_or_default(),
        notes,
    })
}

fn columns(width: usize) -> [cols::Col; 5] {
    let bracket = width.saturating_sub(60).max(22);
    [
        cols::Col::l(13),
        cols::Col::r(7),
        cols::Col::r(9),
        cols::Col::r(20),
        cols::Col::l(bracket),
    ]
}

fn bracket_text(row: &SpendRow) -> String {
    match (row.bracket, &row.money) {
        (_, Money::NotMeasured) => String::new(),
        (Some((floor, ceiling)), _) => format!("${floor:.6} .. ${ceiling:.6}"),
        // 🔴 A PRICED FIGURE WITH NO BRACKET IS THE ONE CASE THAT MUST SHOUT. The combined token
        // count means the true cost can sit anywhere in a ~50x span; a lone number claims a
        // precision this data does not have, so the missing bracket is reported, not omitted.
        (None, _) => "bracket not returned".to_string(),
    }
}

fn count_text(value: Option<u64>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "not measured".to_string(),
    }
}

fn token_text(value: Option<u64>) -> String {
    match value {
        Some(value) => tokens(value),
        None => "not measured".to_string(),
    }
}

fn row_line(
    row: &SpendRow,
    columns: &[cols::Col; 5],
    theme: Theme,
    emphasis: bool,
) -> Line<'static> {
    let calls = count_text(row.calls);
    let tokens = token_text(row.tokens);
    let money = row.money.display();
    let bracket = bracket_text(row);
    let measured = !matches!(row.money, Money::NotMeasured);
    let label_color = if emphasis {
        theme.semantic()
    } else {
        theme.primary()
    };
    // `*` marks a provider whose API could hand us a REAL cost that we do not read yet. The legend
    // under the table says so; marking it is how "we are estimating a number that is available"
    // stays visible instead of becoming a fact nobody remembers.
    let label = if row.reports_own_cost {
        format!("{} *", row.label)
    } else {
        row.label.clone()
    };
    cols::owned(cols::row(
        columns,
        &[
            cols::Cell(&label, label_color),
            cols::Cell(&calls, theme.primary()),
            cols::Cell(&tokens, theme.primary()),
            cols::Cell(
                &money,
                if measured {
                    theme.semantic()
                } else {
                    theme.alert()
                },
            ),
            cols::Cell(
                &bracket,
                if row.bracket.is_some() || !measured {
                    theme.ghost()
                } else {
                    theme.alert()
                },
            ),
        ],
        1,
    ))
}

/// Wrap `body` to `width`, bounded to `MAX_NOTE_LINES` lines so one long server sentence cannot
/// take the table off the screen. A truncated note ends in `…` rather than simply stopping.
fn wrapped(body: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    let width = width.max(20);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in body.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
            if lines.len() >= MAX_NOTE_LINES {
                break;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if lines.len() < MAX_NOTE_LINES && !current.is_empty() {
        lines.push(current);
    } else if lines.len() >= MAX_NOTE_LINES
        && let Some(last) = lines.last_mut()
    {
        last.push('…');
    }
    lines
        .into_iter()
        .map(|line| Line::styled(line, style))
        .collect()
}

impl SpendReport {
    fn balance(&self) -> Balance {
        balance(&self.rows, self.unattributed.as_ref(), &self.total)
    }

    pub(super) fn lines(&self, theme: Theme, width: usize) -> Vec<Line<'static>> {
        let columns = columns(width);
        // 🔴 EVERY PROSE LINE BELOW IS WRAPPED, NOT CLIPPED, AND THAT IS NOT A COSMETIC CHOICE.
        // The first capture of this screen at 80 columns ended the caption at "Every figu" and the
        // footnote at "an estimate like the res" — a DISCLAIMER cut off mid-word still reads as a
        // sentence, so the reader loses the limit and keeps the number.
        let ghost = Style::default().fg(theme.ghost());
        let mut lines = vec![Line::styled(
            format!(
                "PROVIDER KEY SPEND  last {} days  {} recorded call(s) read",
                self.period_days, self.records_read
            ),
            Style::default()
                .fg(theme.semantic())
                .add_modifier(Modifier::BOLD),
        )];
        lines.extend(wrapped(
            "A vendor-list ESTIMATE from published prices, not a provider invoice. Every figure \
             carries its floor .. ceiling bracket.",
            width,
            ghost,
        ));
        lines.push(cols::head(
            &columns,
            &[
                "PROVIDER",
                "CALLS",
                "TOKENS",
                "LIST ESTIMATE",
                "FLOOR .. CEILING",
            ],
            theme.ghost(),
            1,
        ));
        for row in self.rows.iter().take(MAX_PROVIDER_ROWS) {
            lines.push(row_line(row, &columns, theme, false));
            lines.extend(note_line(row, theme, width));
        }
        if self.rows.is_empty() {
            lines.push(Line::styled(
                "  The server returned no provider row for this account",
                Style::default().fg(theme.alert()),
            ));
        }
        if let Some(unattributed) = &self.unattributed {
            lines.push(row_line(unattributed, &columns, theme, false));
            lines.extend(note_line(unattributed, theme, width));
        }
        lines.push(row_line(&self.total, &columns, theme, true));
        lines.extend(note_line(&self.total, theme, width));
        lines.extend(self.balance_lines(theme, width));
        if self.rows.iter().any(|row| row.reports_own_cost) {
            lines.extend(wrapped(
                "* this provider's API can report a real cost and Estelle does not read it yet, so \
                 its figure is an estimate like the rest.",
                width,
                ghost,
            ));
        }
        lines.extend(wrapped(
            &format!(
                "Estelle charged this period: {} (a separate bill; never summed with the estimate \
                 above)",
                self.estelle_charged.display()
            ),
            width,
            Style::default().fg(theme.primary()),
        ));
        if self.records_capped {
            lines.extend(wrapped(
                "This period reached the server's retention cap: the figures above are a FLOOR over \
                 a TRUNCATED window.",
                width,
                Style::default().fg(theme.alert()),
            ));
        }
        for note in self.notes.iter().take(MAX_NOTE_LINES) {
            lines.extend(wrapped(note, width, ghost));
        }
        if !self.assumption.is_empty() {
            lines.extend(wrapped(&self.assumption, width, ghost));
        }
        lines
    }

    fn balance_lines(&self, theme: Theme, width: usize) -> Vec<Line<'static>> {
        match self.balance() {
            // Silence is correct here: a table that adds up is not news, and a line saying so on
            // every render trains the reader to stop reading the line.
            Balance::Reconciled | Balance::NothingPriced => Vec::new(),
            Balance::Mismatch { parts, total } => wrapped(
                &format!(
                    "The rows above sum to ${parts:.6} but the server's total is {}. This table \
                     does not add up; trust neither figure.",
                    total.map_or_else(
                        || "not measured".to_string(),
                        |total| format!("${total:.6}")
                    )
                ),
                width,
                Style::default()
                    .fg(theme.alert())
                    .add_modifier(Modifier::BOLD),
            ),
        }
    }
}

fn note_line(row: &SpendRow, theme: Theme, width: usize) -> Vec<Line<'static>> {
    let Some(note) = &row.note else {
        return Vec::new();
    };
    let mut lines = wrapped(
        note,
        width.saturating_sub(4),
        Style::default().fg(theme.ghost()),
    );
    for line in &mut lines {
        line.spans.insert(0, ratatui::text::Span::raw("   "));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape production returns, transcribed from the server module that builds it
    /// (`provider_spend.spend_report`) and from the figures the shipping commit measured against
    /// real production rows on 2026-09-05.
    fn envelope() -> Value {
        json!({
            "basis": "vendor_list_estimate",
            "is_estimate": true,
            "assumption": "a vendor-list ESTIMATE, not an invoice: the recorded token count for each call is a COMBINED input+output total.",
            "period_days": 30,
            "records_read": 7,
            "records_capped": false,
            "by_provider": [
                {"provider": "anthropic", "reports_own_cost": false, "models": [],
                 "state": "unavailable", "calls": 0, "tokens": 0, "estimate_usd": null,
                 "floor_usd": null, "ceiling_usd": null, "estelle_billed_usd": null,
                 "reason": "no call served by a anthropic model was recorded in this period"},
                {"provider": "gemini", "reports_own_cost": false, "models": [],
                 "state": "bounded-both-directions", "calls": 3, "tokens": 21754,
                 "estimate_usd": 0.032631, "floor_usd": 0.001632, "ceiling_usd": 0.081577,
                 "estelle_billed_usd": 0.0},
                {"provider": "openai", "reports_own_cost": false, "models": [],
                 "state": "bounded-both-directions", "calls": 1, "tokens": 5341,
                 "estimate_usd": 0.002403, "floor_usd": 0.000107, "ceiling_usd": 0.006409,
                 "estelle_billed_usd": 0.0},
                {"provider": "openrouter", "reports_own_cost": true, "models": [],
                 "state": "bounded-both-directions", "calls": 3, "tokens": 3666,
                 "estimate_usd": 0.006186, "floor_usd": 0.000275, "ceiling_usd": 0.016497,
                 "estelle_billed_usd": 0.0}
            ],
            "total": {"state": "bounded-both-directions", "calls": 7, "tokens": 30761,
                      "estimate_usd": 0.04122, "floor_usd": 0.002014, "ceiling_usd": 0.104483,
                      "estelle_billed_usd": 0.0},
            "estelle_charged_usd": 0.0,
            "provider_reported_costs": 0,
            "no_provider_reports_cost": true
        })
    }

    fn cell_texts(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn rendered(report: &SpendReport) -> String {
        report
            .lines(Theme::default(), 120)
            .iter()
            .map(cell_texts)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 🔴 THE CONTRACT, WITH ITS CONTROL IN THE SAME ASSERTION SET.
    ///
    /// A renderer that printed NOTHING would satisfy "an unavailable row does not say $0.00", so
    /// the priced row's own number is asserted in the same test. Both halves have to hold.
    #[test]
    fn an_unavailable_row_says_not_measured_while_a_priced_row_says_its_number() {
        let report = parse(&envelope()).expect("envelope");
        let text = rendered(&report);
        assert!(text.contains("not measured"), "{text}");
        assert!(text.contains("$0.032631"), "{text}");
        assert!(text.contains("$0.001632 .. $0.081577"), "{text}");
        // The CONTROL on the control: the unavailable provider must appear as a ROW, not be
        // dropped. A dropped row also never renders `$0.00`, and dropping it would be a different
        // lie about the same account.
        assert!(text.contains("anthropic"), "{text}");
        let anthropic = report
            .rows
            .iter()
            .find(|row| row.label == "anthropic")
            .expect("anthropic row");
        assert_eq!(anthropic.money, Money::NotMeasured);
        assert_eq!(
            anthropic.estimate, None,
            "an unmeasured row must carry no summable number"
        );
    }

    /// The state, not the number, is the claim. A server that shipped `unavailable` beside a real
    /// `0.0` must still not print a dollar figure.
    #[test]
    fn unavailable_beats_a_number_that_arrives_with_it() {
        assert_eq!(money_for("unavailable", Some(0.0)), Money::NotMeasured);
        assert_eq!(money_for("unavailable", Some(4.10)), Money::NotMeasured);
        assert_eq!(money_for("unavailable", None), Money::NotMeasured);
        // CONTROL: the same helper does print a number when the state licenses one.
        assert_eq!(money_for("measured", Some(4.10)), Money::Exact(4.10));
    }

    /// `measured` is the only unqualified figure; every other printable state carries its kind in
    /// the text. An unknown state prints nothing at all.
    #[test]
    fn only_measured_renders_an_unqualified_figure() {
        assert_eq!(money_for("measured", Some(1.5)).display(), "$1.500000");
        assert_eq!(
            money_for("no-vendor-bill", Some(0.0)).display(),
            "$0.000000 no vendor bill"
        );
        assert_eq!(
            money_for("bounded-both-directions", Some(1.5)).display(),
            "$1.500000 estimate"
        );
        assert_eq!(
            money_for("incomplete", Some(1.5)).display(),
            "$1.500000 estimate"
        );
        assert_eq!(
            money_for("upper-bound", Some(1.5)).display(),
            "$1.500000 ceiling"
        );
        assert_eq!(
            money_for("lower-bound", Some(1.5)).display(),
            "$1.500000 floor"
        );
        assert_eq!(
            money_for("a-state-shipped-after-this-cli", Some(1.5)).display(),
            "not measured"
        );
        // A `no-vendor-bill` that is not zero is a contradiction, and the answer to a contradiction
        // is silence, not a coin flip.
        assert_eq!(money_for("no-vendor-bill", Some(4.10)), Money::NotMeasured);
    }

    #[test]
    fn the_provider_column_plus_unattributed_equals_the_footer() {
        let report = parse(&envelope()).expect("envelope");
        assert_eq!(report.balance(), Balance::Reconciled);
        let mut broken = envelope();
        broken["total"]["estimate_usd"] = json!(9.99);
        let report = parse(&broken).expect("envelope");
        // CONTROL: the check can fail, and it names both numbers when it does.
        assert!(
            matches!(report.balance(), Balance::Mismatch { .. }),
            "{:?}",
            report.balance()
        );
        assert!(rendered(&report).contains("does not add up"));
    }

    /// 🔴 EVEN THE LINE THAT CATCHES A LYING MONEY TABLE MUST NOT PRINT A ZERO FOR AN ABSENCE.
    ///
    /// Priced rows under an UNMEASURED footer is a real mismatch, and its first draft reported it
    /// as "the server's total is $0.000000" — inventing the one number this whole screen exists to
    /// refuse, inside its own integrity check.
    #[test]
    fn a_mismatch_against_an_unmeasured_total_says_not_measured_not_zero() {
        let mut value = envelope();
        value["total"] = json!({"state": "unavailable", "calls": 7, "tokens": 0,
                                "estimate_usd": null, "reason": "nothing was priced"});
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(text.contains("does not add up"), "{text}");
        assert!(
            text.contains("the server's total is not measured"),
            "{text}"
        );
        assert!(!text.contains("total is $0.000000"), "{text}");
        // CONTROL: a real numeric footer that disagrees still prints its number.
        let mut value = envelope();
        value["total"]["estimate_usd"] = json!(9.99);
        assert!(
            rendered(&parse(&value).expect("envelope")).contains("the server's total is $9.990000")
        );
    }

    #[test]
    fn an_unattributed_bucket_is_a_row_and_still_reconciles() {
        let mut value = envelope();
        value["by_provider"] = json!([
            {"provider": "gemini", "reports_own_cost": false, "state": "bounded-both-directions",
             "calls": 3, "tokens": 21754, "estimate_usd": 0.032631, "floor_usd": 0.001632,
             "ceiling_usd": 0.081577}
        ]);
        value["unattributed_total"] = json!({
            "state": "bounded-both-directions", "calls": 4, "tokens": 9007,
            "estimate_usd": 0.008589, "floor_usd": 0.000382, "ceiling_usd": 0.022906
        });
        value["total"]["estimate_usd"] = json!(0.04122);
        let report = parse(&value).expect("envelope");
        assert_eq!(report.balance(), Balance::Reconciled);
        let text = rendered(&report);
        assert!(text.contains("unattributed"), "{text}");
        assert!(text.contains("$0.008589"), "{text}");
    }

    #[test]
    fn a_priced_row_with_no_bracket_says_so_rather_than_standing_alone() {
        let mut value = envelope();
        value["by_provider"][1]["floor_usd"] = json!(null);
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(text.contains("bracket not returned"), "{text}");
        // CONTROL: the rows that DID ship a bracket still print it.
        assert!(text.contains("$0.000107 .. $0.006409"), "{text}");
    }

    /// A bracket that does not contain its own estimate is a wrong claim, not a weak one.
    ///
    /// ⚠️ **THE FIRST VERSION OF THIS TEST PROVED NOTHING, AND A MUTANT SAID SO.** It moved the
    /// ceiling BELOW the floor, which the ordering clause one line above already rejects — so
    /// deleting the containment clause entirely left the test green. The bracket here is ordered
    /// and merely excludes the estimate, which is the only shape that reaches the clause it names.
    #[test]
    fn a_bracket_that_excludes_its_estimate_is_refused() {
        let mut value = envelope();
        value["by_provider"][1]["floor_usd"] = json!(0.001);
        value["by_provider"][1]["ceiling_usd"] = json!(0.002);
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(text.contains("bracket not returned"), "{text}");
        assert!(!text.contains("$0.001000 .. $0.002000"), "{text}");
        // CONTROL: the same ordered pair IS accepted once it actually contains the estimate.
        let mut value = envelope();
        value["by_provider"][1]["floor_usd"] = json!(0.001);
        value["by_provider"][1]["ceiling_usd"] = json!(0.9);
        let report = parse(&value).expect("envelope");
        assert!(rendered(&report).contains("$0.001000 .. $0.900000"));
    }

    /// An inverted bracket is refused. Enforced by the CONTAINMENT clause, not an ordering one:
    /// `floor > ceiling` is an empty interval, so no estimate is inside it. Named here so the
    /// behaviour stays pinned after the redundant ordering clause was deleted for failing to be
    /// killable — see [`bracket_for`].
    #[test]
    fn a_bracket_whose_floor_is_above_its_ceiling_is_refused() {
        let mut value = envelope();
        value["by_provider"][1]["floor_usd"] = json!(0.9);
        value["by_provider"][1]["ceiling_usd"] = json!(0.001);
        let report = parse(&value).expect("envelope");
        assert!(rendered(&report).contains("bracket not returned"));
    }

    /// Half a bracket is not a bracket. Each end is read separately, so each end's absence gets its
    /// own assertion — a `?` that was deleted on one line only would otherwise go unnoticed.
    #[test]
    fn half_a_bracket_is_refused_from_either_end() {
        for end in ["floor_usd", "ceiling_usd"] {
            let mut value = envelope();
            value["by_provider"][1][end] = json!(null);
            let report = parse(&value).expect("envelope");
            assert!(
                rendered(&report).contains("bracket not returned"),
                "a missing {end} was rendered as a bracket"
            );
        }
    }

    #[test]
    fn an_envelope_with_an_unknown_basis_is_refused_rather_than_rendered() {
        let mut value = envelope();
        value["basis"] = json!("provider_invoice");
        assert!(parse(&value).is_err());
        let mut value = envelope();
        value["is_estimate"] = json!(false);
        assert!(parse(&value).is_err());
        let mut value = envelope();
        value["total"] = json!(null);
        assert!(parse(&value).is_err());
        // CONTROL: the untouched envelope parses, so the refusals above are about the mutation.
        assert!(parse(&envelope()).is_ok());
    }

    #[test]
    fn estelle_own_charge_is_its_own_line_and_is_never_added_to_the_estimate() {
        let mut value = envelope();
        value["estelle_charged_usd"] = json!(4.25);
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(
            text.contains("Estelle charged this period: $4.250000"),
            "{text}"
        );
        assert!(text.contains("never summed"), "{text}");
        // The estimate footer is untouched by Estelle's own bill.
        assert_eq!(report.balance(), Balance::Reconciled);
        assert!(text.contains("$0.041220"), "{text}");
    }

    /// A negative dollar figure is not a cheap month; it is a broken envelope. [`amount`] is the
    /// one boundary that decides, so this drives the whole `parse` path rather than the helper.
    #[test]
    fn a_negative_figure_is_refused_at_the_boundary() {
        let mut value = envelope();
        value["by_provider"][1]["estimate_usd"] = json!(-0.032631);
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(!text.contains("-$"), "{text}");
        assert!(!text.contains("$-0.032631"), "{text}");
        assert_eq!(report.rows[1].money, Money::NotMeasured);
        // CONTROL: the same field at the same path renders when it is a usable number.
        assert_eq!(
            parse(&envelope()).expect("envelope").rows[1].money,
            Money::Estimate(0.032631)
        );
    }

    /// 🔴 A DISCLAIMER CUT OFF MID-WORD IS STILL READ AS A SENTENCE.
    ///
    /// The first 80-column capture of this screen ended its caption at "Every figu" and its
    /// footnote at "an estimate like the res": the reader loses the LIMIT and keeps the NUMBER,
    /// which is the exact failure this screen exists to prevent. Both halves are asserted — every
    /// line fits, AND the last words still arrive — because a renderer that emitted nothing would
    /// satisfy the first on its own.
    #[test]
    fn no_line_is_clipped_and_no_disclaimer_loses_its_tail_at_eighty_columns() {
        let report = parse(&envelope()).expect("envelope");
        let lines = report.lines(Theme::default(), 80);
        for line in &lines {
            let text = cell_texts(line);
            assert!(
                text.chars().count() <= 80,
                "{} chars: {text}",
                text.chars().count()
            );
        }
        let joined = lines.iter().map(cell_texts).collect::<Vec<_>>().join(" ");
        for tail in [
            "Every figure carries its floor .. ceiling bracket.",
            "its figure is an estimate like the rest.",
            "(a separate bill; never summed with the estimate above)",
            "no call served by a anthropic model was recorded in this period",
        ] {
            assert!(
                joined.contains(tail),
                "lost {tail:?} at 80 columns\n{joined}"
            );
        }
    }

    #[test]
    fn the_query_and_the_caption_read_one_owner() {
        assert_eq!(spend_query(), json!({"days": PERIOD_DAYS}));
        let report = parse(&envelope()).expect("envelope");
        assert!(rendered(&report).contains(&format!("last {PERIOD_DAYS} days")));
    }

    #[test]
    fn a_capped_period_is_reported_as_a_floor_over_a_truncated_window() {
        let mut value = envelope();
        value["records_capped"] = json!(true);
        value["cap_note"] = json!("this period reached the 1000-record retention cap");
        let report = parse(&value).expect("envelope");
        let text = rendered(&report);
        assert!(text.contains("FLOOR over a"), "{text}");
        assert!(text.contains("1000-record retention cap"), "{text}");
        // CONTROL: an uncapped period does not print the warning.
        assert!(!rendered(&parse(&envelope()).expect("envelope")).contains("FLOOR over a"));
    }

    #[test]
    fn every_bound_is_applied_before_the_list_is_taken() {
        let mut value = envelope();
        let row = value["by_provider"][1].clone();
        value["by_provider"] = Value::Array(vec![row; MAX_PROVIDER_ROWS + 9]);
        value["total"]["estimate_usd"] = json!(0.032631 * (MAX_PROVIDER_ROWS + 9) as f64);
        let report = parse(&value).expect("envelope");
        assert_eq!(report.rows.len(), MAX_PROVIDER_ROWS);
        // A truncated read cannot balance, and saying so is the point of the bound: a capped read
        // means "cannot answer", never "that's all there is".
        assert!(matches!(report.balance(), Balance::Mismatch { .. }));
    }
}
