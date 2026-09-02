use crate::cols::{Cell, Col, head, row};
use crate::mask_secret;
use crate::theme::Palette;
use estelle_client::{Client, MonitorOverviewResponse, Repo};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProductionGraph {
    pub issue_key: String,
    pub failing_symbol: String,
    pub failing_file: String,
    pub healthy_subsystems: Vec<String>,
    pub blast_radius: Vec<String>,
    pub chokepoints: Vec<String>,
    pub core_files: Vec<String>,
    pub drill_down: bool,
    /// 🔴 **THE SERVER'S REFUSAL, WHEN IT GAVE ONE — AND THE FIELD THAT STOPS IT BEING DATA.**
    ///
    /// Measured against production on 2026-09-02: `chokepoints{"repo":"uqeu/estelle"}` answers
    /// `CANNOT ANSWER: … this repo has never been swept …` on the SUCCESS path — HTTP 200,
    /// `isError` unset, ordinary text content. This pane used to split that sentence on newlines
    /// and draw the first three lines as `choke` rows, i.e. it showed a risk map over a server
    /// that had just said it has no graph.
    ///
    /// ⚠️ When this is `Some`, every list above is EMPTY by construction — see [`fetch`]. There is
    /// no state in which the pane holds both a refusal and rows, because "some of the graph"
    /// is a claim nothing on the wire supports.
    pub withheld: Option<String>,
}

pub async fn fetch(
    client: &Client,
    repo: &Repo,
    issue_key: String,
    failing_symbol: String,
    failing_file: String,
) -> Result<ProductionGraph, String> {
    let cancel = CancellationToken::new();
    let blast = crate::mcp_tool::call(
        client,
        repo,
        "blast_radius",
        serde_json::json!({"file": failing_file}),
        &cancel,
    );
    let chokepoints =
        crate::mcp_tool::call(client, repo, "chokepoints", serde_json::json!({}), &cancel);
    let subsystems =
        crate::mcp_tool::call(client, repo, "subsystems", serde_json::json!({}), &cancel);
    let core_files =
        crate::mcp_tool::call(client, repo, "core_files", serde_json::json!({}), &cancel);
    let (blast, chokepoints, subsystems, core_files) =
        tokio::try_join!(blast, chokepoints, subsystems, core_files)?;
    // 🔴 ONE REFUSAL WITHHOLDS THE WHOLE PANE. All four tools read the same graph through the same
    // currency guard, so if one says the graph cannot be dated, the other three are answering about
    // a graph the server has already declined to stand behind. Drawing the rows that happened to
    // come back would be "some of the risk map", which is the reading a user cannot act on.
    let withheld = [&blast, &chokepoints, &subsystems, &core_files]
        .into_iter()
        .find_map(|outcome| match outcome {
            crate::mcp_tool::Outcome::CannotAnswer(reason) => Some(reason.clone()),
            crate::mcp_tool::Outcome::Answered(_) => None,
        });
    if let Some(reason) = withheld {
        return Ok(ProductionGraph {
            issue_key,
            failing_symbol,
            failing_file,
            withheld: Some(reason),
            ..ProductionGraph::default()
        });
    }
    let blast_radius = blast.lines();
    let chokepoints = chokepoints.lines().into_iter().take(3).collect();
    let healthy_subsystems = subsystems
        .lines()
        .into_iter()
        .filter(|subsystem| !subsystem.contains(&failing_file))
        .take(4)
        .collect();
    let core_files = core_files.lines().into_iter().take(3).collect();
    Ok(ProductionGraph {
        issue_key,
        failing_symbol,
        failing_file,
        healthy_subsystems,
        blast_radius,
        chokepoints,
        core_files,
        drill_down: false,
        withheld: None,
    })
}

/// The service row: glyph, name, last status code, last latency. The name column takes whatever
/// the rail has left, because a truncated service NAME is recoverable and a truncated NUMBER is a
/// different number.
const SERVICE_GLYPH: usize = 2;
const SERVICE_CODE: usize = 4;
const SERVICE_LATENCY: usize = 9;
const SERVICE_GAP: usize = 2;
/// `2 + 2 + <name> + 2 + 4 + 2 + 9`, with the name column excluded.
const SERVICE_FIXED: usize =
    SERVICE_GLYPH + SERVICE_GAP + SERVICE_GAP + SERVICE_CODE + SERVICE_GAP + SERVICE_LATENCY;
/// Below this the rail cannot name a service at all, so the name stops shrinking and truncates.
const MIN_SERVICE_NAME: usize = 8;

fn service_columns(width: usize) -> [Col; 4] {
    let name = width.saturating_sub(SERVICE_FIXED).max(MIN_SERVICE_NAME);
    [
        Col::l(SERVICE_GLYPH),
        Col::l(name),
        Col::r(SERVICE_CODE),
        Col::r(SERVICE_LATENCY),
    ]
}

/// The HUD's service rows — one line per monitored service, from `GET /monitor/overview`'s
/// `uptime_checks` array.
///
/// 🔴 **THIS ARRAY WAS ON THE WIRE AND NOTHING RENDERED IT.** `MonitorOverviewResponse` has carried
/// `uptime_checks` since the type was written; the live rail showed only the `up/checks` roll-up
/// and, before any snapshot arrived, the words "Loading a real Monitor window...". The catalog's
/// screen 9 shows a service list (`● api`, `● postgres`, `◐ postgrest restarting`) as fixture text.
/// These rows are that list, drawn from the real array, by the one renderer both callers use.
///
/// ⚠️ Every absence is printed as an absence. A check that has never been probed prints `no probe`
/// and `—`; it never borrows another row's number and never leaves the cell blank, because a blank
/// cell reads as "measured, and fine".
pub fn service_lines(
    overview: &MonitorOverviewResponse,
    palette: &Palette,
    width: usize,
) -> Vec<Line<'static>> {
    let (rows, unreadable) = overview.uptime_check_rows();
    let columns = service_columns(width);
    // A column header over nothing is noise that reads as a table that failed to load.
    let mut output = if rows.is_empty() {
        Vec::new()
    } else {
        vec![head(
            &columns,
            &["", "service", "code", "latency"],
            palette.dim,
            0,
        )]
    };
    for check in &rows {
        let (glyph, colour) = match (check.enabled, check.up) {
            (Some(false), _) => ("◌", palette.dim),
            (_, Some(true)) => ("●", palette.green),
            (_, Some(false)) => ("▲", palette.red),
            (_, None) => ("·", palette.dim),
        };
        let name = check
            .display_name()
            .map(mask_secret)
            .unwrap_or_else(|| "unnamed check".to_string());
        let code = check
            .last_status
            .map_or_else(|| "—".to_string(), |status| status.to_string());
        let latency = match (check.last_latency_ms, check.last_checked) {
            (Some(ms), _) if ms.is_finite() => format!("{ms:.0}ms"),
            (_, None) => "no probe".to_string(),
            _ => "—".to_string(),
        };
        let code_colour = match check.last_status {
            Some(status) if (200..400).contains(&status) => palette.dim,
            Some(_) => palette.warn,
            None => palette.dim,
        };
        output.push(owned(row(
            &columns,
            &[
                Cell(glyph, colour),
                Cell(&name, palette.mid),
                Cell(&code, code_colour),
                Cell(&latency, palette.dim),
            ],
            0,
        )));
    }
    if rows.is_empty() {
        output.push(Line::styled(
            "no uptime checks registered · POST /monitor/uptime adds one",
            Style::default().fg(palette.dim),
        ));
    }
    if unreadable > 0 {
        output.push(Line::styled(
            format!("{unreadable} uptime row(s) did not match the check shape"),
            Style::default().fg(palette.warn),
        ));
    }
    output
}

/// One file that hangs off the failing symbol: an indent, the kind of relationship, the path.
const RELATED_INDENT: usize = 2;
const RELATED_KIND: usize = 5;
const RELATED_GAP: usize = 2;

fn related_columns(width: usize) -> [Col; 2] {
    let file = width
        .saturating_sub(RELATED_INDENT + RELATED_KIND + RELATED_GAP)
        .max(MIN_SERVICE_NAME);
    [Col::l(RELATED_KIND), Col::l(file)]
}

fn related_row(
    columns: &[Col; 2],
    kind: &str,
    file: &str,
    file_colour: ratatui::style::Color,
    palette: &Palette,
) -> Line<'static> {
    owned(row(
        columns,
        &[Cell(kind, palette.dim), Cell(file, file_colour)],
        RELATED_INDENT,
    ))
}

/// A `cols` row borrows its cells; the live rail needs the line to outlive them.
fn owned(line: Line<'_>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect::<Vec<_>>(),
    )
}

/// The graph rows: which subsystems are healthy, which symbol is failing, and what depends on it.
///
/// ⚠️ **THE `├─`/`└─` TREE CONNECTORS ARE GONE.** They were the last box-drawing glyphs this file
/// emitted into a live frame, and the rule is flat: there are no boxes in Estelle, with no
/// exemption for a connector that only looks like part of one. The relationship they drew — these
/// files hang off that symbol — is now carried by indentation and a `cols` table, so the kind and
/// the path line up in columns instead of being pushed around by a glyph.
pub fn lines(
    graph: &ProductionGraph,
    palette: &Palette,
    width: usize,
    tick: u64,
    pulse_enabled: bool,
) -> Vec<Line<'static>> {
    let mut output = vec![crate::session_view::section_rule(
        "production",
        "code graph",
        width,
        palette,
        palette.green,
    )];

    if graph.healthy_subsystems.is_empty() {
        output.push(Line::styled(
            "healthy subsystem context unavailable",
            Style::default().fg(palette.dim),
        ));
    } else {
        output.push(crate::session_view::section_rule(
            "healthy",
            &format!("{} subsystems", graph.healthy_subsystems.len()),
            width,
            palette,
            palette.green,
        ));
        for subsystem in &graph.healthy_subsystems {
            output.push(Line::from(vec![
                Span::styled("● ", Style::default().fg(palette.green)),
                Span::styled(subsystem.clone(), Style::default().fg(palette.green)),
                Span::styled(
                    "  healthy · no unresolved issue bound",
                    Style::default().fg(palette.dim),
                ),
            ]));
        }
    }

    output.push(Line::from(""));
    output.push(crate::session_view::section_rule(
        "failing",
        "path",
        width,
        palette,
        palette.red,
    ));
    // ⚠️ The symbol's NAME used to pulse with the mark. It is the one string on this pane a
    // reader has to copy accurately, and it was the one moving.
    output.push(crate::marks::headline(
        crate::marks::Mark::Blocked,
        if graph.failing_symbol.is_empty() {
            "unbound symbol"
        } else {
            graph.failing_symbol.as_str()
        },
        &graph.failing_file,
        palette,
        tick,
        pulse_enabled,
    ));

    // 🔴 THE REFUSAL IS DRAWN BY THE SAME MODULE THAT DRAWS THE ROWS, AND IT ENDS THE PANE.
    // Nothing below can be honest once the server has said it cannot date the graph, so the
    // sections that would follow are not drawn at all rather than drawn empty — an empty risk map
    // reads as "no risk", which is the opposite of what the server said.
    if let Some(reason) = &graph.withheld {
        output.push(Line::from(""));
        output.extend(crate::graph_view::lines(
            &crate::graph_view::Surface::Withheld { repo: "", reason },
            palette,
            width,
            tick,
            pulse_enabled,
        ));
        return output;
    }

    let related = related_columns(width);
    if graph.blast_radius.is_empty() {
        output.push(Line::styled(
            "  blast radius returned no dependants",
            Style::default().fg(palette.dim),
        ));
    } else {
        for file in &graph.blast_radius {
            output.push(related_row(&related, "blast", file, palette.warn, palette));
        }
    }

    // 🔴 THE GRAPH ROWS GO THROUGH [`crate::graph_view`], WHICH IS ALSO WHAT DESIGN-BOOK SCREEN 40
    // DRAWS. They were two pictures of one fact: this pane printed `choke  api.py  (0.81)` from a
    // `related_row`, and the book drew a node table beside it that nothing produced. A layout with
    // two owners is the defect this whole design-book pass exists to close, so there is now one
    // function, and the book's version is this one with fixture rows in it.
    let nodes = graph_nodes(graph);
    if !nodes.is_empty() {
        output.push(Line::from(""));
        output.extend(crate::graph_view::lines(
            &crate::graph_view::Surface::Walk {
                repo: "",
                filter: "",
                matched: nodes.len(),
                total: nodes.len(),
                nodes: &nodes,
                selected: None,
            },
            palette,
            width,
            tick,
            pulse_enabled,
        ));
    }

    output.push(Line::from(""));
    if graph.drill_down {
        output.extend([
            Line::styled("flowchart LR", Style::default().fg(palette.cite)),
            Line::styled(
                "event --> symbol --> diff",
                Style::default().fg(palette.cite),
            ),
            Line::styled(
                format!(
                    "event[production event] --> symbol[{}]",
                    graph.failing_symbol
                ),
                Style::default().fg(palette.dim),
            ),
            Line::styled(
                format!("symbol[{}] --> diff[repair diff]", graph.failing_file),
                Style::default().fg(palette.dim),
            ),
        ]);
        for file in &graph.blast_radius {
            output.push(Line::styled(
                format!("symbol --> impacted[{file}]"),
                Style::default().fg(palette.warn),
            ));
        }
        output.push(Line::styled(
            "Esc returns to the production graph",
            Style::default().fg(palette.dim),
        ));
    } else {
        output.push(Line::styled(
            "Enter opens event → symbol → diff",
            Style::default().fg(palette.dim),
        ));
    }

    output
}

/// The pane's graph rows as [`crate::graph_view::Node`]s.
///
/// ⚠️ The `path  (score)` split has ONE owner, [`crate::graph_view::Node::from_tool_line`], beside
/// the type it produces.
///
/// 🔴 **`moves` STAYS `None` HERE AND THAT IS THE HONEST VALUE.** A blast radius is fetched for one
/// file — the failing one — so no other row has been measured, and the walk that would measure the
/// selected row is not built. `None` draws `—`; printing `0` would put "safe to change" on every
/// file nobody asked about.
fn graph_nodes(graph: &ProductionGraph) -> Vec<crate::graph_view::Node> {
    let mut nodes: Vec<crate::graph_view::Node> = graph
        .chokepoints
        .iter()
        .map(|line| {
            crate::graph_view::Node::from_tool_line(line, crate::graph_view::Role::Chokepoint)
        })
        .collect();
    for line in &graph.core_files {
        let node = crate::graph_view::Node::from_tool_line(line, crate::graph_view::Role::Core);
        if !nodes.iter().any(|held| held.path == node.path) {
            nodes.push(node);
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;
    use serde_json::Value;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mount_graph_tool(
        server: &MockServer,
        name: &str,
        arguments: Value,
        text: Option<&str>,
    ) {
        let result = text.map_or_else(
            || json!({"content": []}),
            |text| json!({"content": [{"type": "text", "text": text}], "isError": false}),
        );
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": result
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    fn overview(value: Value) -> MonitorOverviewResponse {
        serde_json::from_value(value).expect("typed overview")
    }

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_registered_service_gets_a_row_with_its_measured_code_and_latency() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&service_lines(
            &overview(json!({
                "uptime": {"checks": 3, "up": 2, "down": 1},
                "uptime_checks": [
                    {"check_id": "c1", "name": "api", "url": "https://api.fatelabs.ca/health",
                     "enabled": true, "up": true, "last_status": 200, "last_latency_ms": 142.4,
                     "last_checked": 4102444800.0},
                    {"check_id": "c2", "name": "postgrest", "url": "https://db/health",
                     "enabled": true, "up": false, "last_status": 503, "last_latency_ms": 1902.0,
                     "last_checked": 4102444700.0},
                    {"check_id": "c3", "name": "worker", "url": "https://worker/health",
                     "enabled": false, "up": true, "last_status": null, "last_latency_ms": null,
                     "last_checked": null}
                ]
            })),
            &palette,
            40,
        ));

        assert!(rendered.contains("service"), "{rendered}");
        assert!(
            rendered.contains("● ") && rendered.contains("api"),
            "{rendered}"
        );
        assert!(
            rendered.contains("▲ ") && rendered.contains("postgrest"),
            "{rendered}"
        );
        assert!(
            rendered.contains("◌ ") && rendered.contains("worker"),
            "{rendered}"
        );
        assert!(
            rendered.contains("200") && rendered.contains("142ms"),
            "{rendered}"
        );
        assert!(
            rendered.contains("503") && rendered.contains("1902ms"),
            "{rendered}"
        );
        // 🔴 A NEVER-PROBED CHECK MAY NOT BORROW ANOTHER ROW'S NUMBER OR SHOW A BLANK.
        assert!(rendered.contains("no probe"), "{rendered}");
        assert!(!rendered.contains("0ms"), "{rendered}");
    }

    #[test]
    fn a_row_that_is_not_a_check_is_counted_out_loud_rather_than_dropped() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&service_lines(
            &overview(json!({"uptime_checks": [
                {"check_id": "c1", "name": "api", "up": true, "last_status": 200,
                 "last_latency_ms": 12.0, "last_checked": 1.0},
                "not-an-object"
            ]})),
            &palette,
            40,
        ));
        assert!(
            rendered.contains("1 uptime row(s) did not match the check shape"),
            "{rendered}"
        );
    }

    #[test]
    fn no_registered_checks_names_the_call_that_registers_one() {
        let palette = ScreenTheme::Dark.palette();
        let rendered = text(&service_lines(&overview(json!({})), &palette, 40));
        assert!(
            rendered.contains("no uptime checks registered · POST /monitor/uptime adds one"),
            "{rendered}"
        );
    }

    #[test]
    fn every_service_row_is_the_same_width_at_every_rail_width() {
        let palette = ScreenTheme::Dark.palette();
        let snapshot = overview(json!({"uptime_checks": [
            {"check_id": "c1", "name": "api", "up": true, "last_status": 200,
             "last_latency_ms": 12.0, "last_checked": 1.0},
            {"check_id": "c2", "name": "a-very-long-service-name-indeed", "up": false,
             "last_status": 500, "last_latency_ms": 9999.0, "last_checked": 1.0}
        ]}));
        for width in [30usize, 46, 80] {
            let widths = service_lines(&snapshot, &palette, width)
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.chars().count())
                        .sum::<usize>()
                })
                .collect::<Vec<_>>();
            assert!(
                widths.windows(2).all(|pair| pair[0] == pair[1]),
                "width {width} produced ragged service rows: {widths:?}"
            );
        }
    }

    #[test]
    fn failing_symbol_is_red_blast_radius_is_amber_and_healthy_context_is_green() {
        let palette = ScreenTheme::Dark.palette();
        let graph = ProductionGraph {
            failing_symbol: "charge_card".to_string(),
            failing_file: "billing.py".to_string(),
            healthy_subsystems: vec!["auth".to_string(), "search".to_string()],
            blast_radius: vec!["checkout.py".to_string()],
            ..Default::default()
        };
        let rendered = lines(&graph, &palette, 82, 0, true);
        let spans = rendered
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        // ⚠️ The failing symbol moved from red to WARN with the mark vocabulary: the demo frame
        // draws a production incident as `▲` in warn (`▲ 03:41:02 checkout-worker
        // SignatureMismatch`), and red is reserved for a REFUSAL — something Estelle declined to
        // do. Nothing refused this; it broke. The colour now says which.
        assert!(spans.iter().any(|span| {
            span.content.contains("charge_card") && span.style.fg == Some(palette.warn)
        }));
        assert!(spans.iter().any(|span| {
            span.content.contains("checkout.py") && span.style.fg == Some(palette.warn)
        }));
        assert!(
            spans.iter().any(|span| {
                span.content.contains("auth") && span.style.fg == Some(palette.green)
            })
        );
    }

    #[test]
    fn enter_drill_down_is_the_event_to_symbol_to_diff_mermaid_path() {
        let palette = ScreenTheme::Dark.palette();
        let graph = ProductionGraph {
            failing_symbol: "charge_card".to_string(),
            failing_file: "billing.py".to_string(),
            blast_radius: vec!["checkout.py".to_string()],
            drill_down: true,
            ..Default::default()
        };
        let text = lines(&graph, &palette, 82, 0, false)
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(text.contains("flowchart LR"));
        assert!(text.contains("event --> symbol --> diff"));
        assert!(text.contains("billing.py"));
        assert!(text.contains("checkout.py"));
    }

    #[tokio::test]
    async fn reads_all_four_graph_tools_through_the_real_client_type() {
        let server = MockServer::start().await;
        mount_graph_tool(
            &server,
            "blast_radius",
            json!({"file": "billing.py", "repo": "uqeu/estelle"}),
            Some("checkout.py\nreceipts.py"),
        )
        .await;
        mount_graph_tool(
            &server,
            "chokepoints",
            json!({"repo": "uqeu/estelle"}),
            Some("api.py  (0.8)"),
        )
        .await;
        mount_graph_tool(
            &server,
            "subsystems",
            json!({"repo": "uqeu/estelle"}),
            Some("billing.py, checkout.py\nauth.py, sessions.py"),
        )
        .await;
        mount_graph_tool(
            &server,
            "core_files",
            json!({"repo": "uqeu/estelle"}),
            Some("models.py  (0.9)"),
        )
        .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("client");

        let graph = fetch(
            &client,
            &Repo::new("uqeu/estelle").expect("repo"),
            "issue-17".to_string(),
            "charge_card".to_string(),
            "billing.py".to_string(),
        )
        .await
        .expect("four graph receipts");

        assert_eq!(graph.blast_radius, ["checkout.py", "receipts.py"]);
        assert_eq!(graph.healthy_subsystems, ["auth.py, sessions.py"]);
        assert_eq!(graph.chokepoints, ["api.py  (0.8)"]);
        assert_eq!(graph.core_files, ["models.py  (0.9)"]);
    }

    /// 🔴 **THE BUG THIS PANE SHIPPED WITH, AND THE TEST THAT KILLS IT.**
    ///
    /// `serve/mcp/__init__.py:1174` returns the graph-currency refusal as ORDINARY text on the
    /// SUCCESS path — HTTP 200, `isError` unset. Probed against production on 2026-09-02,
    /// `chokepoints{"repo":"uqeu/estelle"}` answered exactly the sentence below, and so did
    /// `core_files` and `import_cycles`.
    ///
    /// This pane used to split whatever came back on newlines and draw the first three lines as
    /// `choke` rows. Handed that sentence it rendered a risk map over a server that had just said
    /// it has no graph — and it did so with no error, no warning, and a green suite.
    ///
    /// ⚠️ **THE ASSERTIONS RUN IN BOTH DIRECTIONS.** That the refusal is CARRIED is half of it; the
    /// half that matters is that not one row survives beside it. Delete the `withheld` short-circuit
    /// in `fetch` and the `chokepoints` assertion below goes red on the first line of the sentence.
    #[tokio::test]
    async fn a_currency_refusal_is_carried_as_a_refusal_and_not_drawn_as_three_files() {
        const REFUSAL: &str = "CANNOT ANSWER: uqeu/estelle: currency UNKNOWN — this repo has never been swept, so there is no graph to date.\nSweep this repo first — nothing has been indexed for it yet.";
        let server = MockServer::start().await;
        // Three tools answer normally and ONE refuses. That is the honest shape of the bug: the
        // pane had three real replies in hand and drew the fourth as if it were a fourth.
        mount_graph_tool(
            &server,
            "blast_radius",
            json!({"file": "billing.py", "repo": "uqeu/estelle"}),
            Some("checkout.py"),
        )
        .await;
        mount_graph_tool(
            &server,
            "chokepoints",
            json!({"repo": "uqeu/estelle"}),
            Some(REFUSAL),
        )
        .await;
        mount_graph_tool(
            &server,
            "subsystems",
            json!({"repo": "uqeu/estelle"}),
            Some("billing.py, checkout.py"),
        )
        .await;
        mount_graph_tool(
            &server,
            "core_files",
            json!({"repo": "uqeu/estelle"}),
            Some("models.py  (0.9)"),
        )
        .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("client");

        let graph = fetch(
            &client,
            &Repo::new("uqeu/estelle").expect("repo"),
            "issue-17".to_string(),
            "charge_card".to_string(),
            "billing.py".to_string(),
        )
        .await
        .expect("a refusal is not a transport failure");

        let reason = graph.withheld.as_deref().expect("the refusal is carried");
        assert!(
            reason.starts_with("uqeu/estelle: currency UNKNOWN"),
            "{reason}"
        );
        assert!(
            !reason.starts_with("CANNOT ANSWER"),
            "the tag is for the parser: {reason}"
        );
        // 🔴 NOT ONE ROW SURVIVES. These four were the bug.
        assert!(graph.chokepoints.is_empty(), "{:?}", graph.chokepoints);
        assert!(graph.core_files.is_empty(), "{:?}", graph.core_files);
        assert!(graph.blast_radius.is_empty(), "{:?}", graph.blast_radius);
        assert!(
            graph.healthy_subsystems.is_empty(),
            "{:?}",
            graph.healthy_subsystems
        );

        // And the drawn pane says so, with no table under it.
        let drawn = text(&lines(&graph, &ScreenTheme::Dark.palette(), 110, 0, false));
        assert!(drawn.contains("no walk from here"), "{drawn}");
        assert!(!drawn.contains("centrality"), "no node table: {drawn}");
        assert!(!drawn.contains("models.py"), "no surviving row: {drawn}");
    }

    #[tokio::test]
    async fn refuses_a_vacuous_graph_receipt() {
        let server = MockServer::start().await;
        for (name, arguments) in [
            (
                "blast_radius",
                json!({"file": "billing.py", "repo": "uqeu/estelle"}),
            ),
            ("chokepoints", json!({"repo": "uqeu/estelle"})),
            ("subsystems", json!({"repo": "uqeu/estelle"})),
        ] {
            mount_graph_tool(&server, name, arguments, Some("measured row")).await;
        }
        mount_graph_tool(&server, "core_files", json!({"repo": "uqeu/estelle"}), None).await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            estelle_client::ApiKey::new("test-key").expect("key"),
            estelle_client::MINIMUM_TIMEOUT,
        )
        .expect("client");

        let error = fetch(
            &client,
            &Repo::new("uqeu/estelle").expect("repo"),
            "issue-17".to_string(),
            "charge_card".to_string(),
            "billing.py".to_string(),
        )
        .await
        .expect_err("empty content must keep the HUD red");

        assert!(error.contains("core_files returned no text content"));
    }
}
