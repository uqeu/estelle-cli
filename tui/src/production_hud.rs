use crate::mask_secret;
use crate::theme::Palette;
use crate::theme::pulse;
use estelle_client::{Client, Repo};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use serde_json::Value;
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
}

pub async fn fetch(
    client: &Client,
    repo: &Repo,
    issue_key: String,
    failing_symbol: String,
    failing_file: String,
) -> Result<ProductionGraph, String> {
    let blast = call_graph_tool(
        client,
        repo,
        "blast_radius",
        serde_json::json!({"file": failing_file}),
    );
    let chokepoints = call_graph_tool(client, repo, "chokepoints", serde_json::json!({}));
    let subsystems = call_graph_tool(client, repo, "subsystems", serde_json::json!({}));
    let core_files = call_graph_tool(client, repo, "core_files", serde_json::json!({}));
    let (blast, chokepoints, subsystems, core_files) =
        tokio::try_join!(blast, chokepoints, subsystems, core_files)?;
    let meaningful_lines = |text: String| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(mask_secret)
            .collect::<Vec<_>>()
    };
    let blast_radius = meaningful_lines(blast);
    let chokepoints = meaningful_lines(chokepoints).into_iter().take(3).collect();
    let healthy_subsystems = meaningful_lines(subsystems)
        .into_iter()
        .filter(|subsystem| !subsystem.contains(&failing_file))
        .take(4)
        .collect();
    let core_files = meaningful_lines(core_files).into_iter().take(3).collect();
    Ok(ProductionGraph {
        issue_key,
        failing_symbol,
        failing_file,
        healthy_subsystems,
        blast_radius,
        chokepoints,
        core_files,
        drill_down: false,
    })
}

async fn call_graph_tool(
    client: &Client,
    repo: &Repo,
    name: &str,
    mut arguments: Value,
) -> Result<String, String> {
    let Some(arguments) = arguments.as_object_mut() else {
        return Err(format!("{name} arguments were not an object"));
    };
    arguments.insert("repo".to_string(), Value::String(repo.as_str().to_string()));
    let response: Value = client
        .post(
            estelle_client::Endpoint::Mcp,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }),
            &CancellationToken::new(),
        )
        .await
        .map_err(|error| format!("{name} request failed: {error}"))?;
    if let Some(error) = response.get("error") {
        return Err(format!(
            "{name} returned a protocol error: {}",
            mask_secret(&error.to_string())
        ));
    }
    let result = response
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{name} omitted the MCP result object"))?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(format!("{name} reported a tool failure"));
    }
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| item.get("text").and_then(Value::as_str))
                .flatten()
        })
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{name} returned no text content"))
}

pub fn lines(
    graph: &ProductionGraph,
    palette: &Palette,
    tick: u64,
    pulse_enabled: bool,
) -> Vec<Line<'static>> {
    let mut output = vec![Line::from(vec![
        Span::styled(
            "╌╌ production · code graph ",
            Style::default().fg(palette.dim),
        ),
        Span::styled("╌╌╌╌╌╌╌╌╌╌", Style::default().fg(palette.dim)),
    ])];

    if graph.healthy_subsystems.is_empty() {
        output.push(Line::styled(
            "healthy subsystem context unavailable",
            Style::default().fg(palette.dim),
        ));
    } else {
        output.push(Line::styled(
            "HEALTHY SUBSYSTEMS",
            Style::default().fg(palette.mid),
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
    output.push(Line::styled(
        "FAILING PATH",
        Style::default().fg(palette.mid),
    ));
    output.push(Line::from(vec![
        Span::styled("▲ ", pulse(palette.red, tick, pulse_enabled)),
        Span::styled(
            if graph.failing_symbol.is_empty() {
                "unbound symbol".to_string()
            } else {
                graph.failing_symbol.clone()
            },
            pulse(palette.red, tick, pulse_enabled),
        ),
        Span::styled(
            format!("  {}", graph.failing_file),
            Style::default().fg(palette.dim),
        ),
    ]));

    if graph.blast_radius.is_empty() {
        output.push(Line::styled(
            "  blast radius returned no dependants",
            Style::default().fg(palette.dim),
        ));
    } else {
        for file in &graph.blast_radius {
            output.push(Line::from(vec![
                Span::styled("├─ blast  ", Style::default().fg(palette.warn)),
                Span::styled(file.clone(), Style::default().fg(palette.warn)),
            ]));
        }
    }
    for file in &graph.chokepoints {
        output.push(Line::from(vec![
            Span::styled("├─ choke  ", Style::default().fg(palette.dim)),
            Span::styled(file.clone(), Style::default().fg(palette.mid)),
        ]));
    }
    for file in &graph.core_files {
        output.push(Line::from(vec![
            Span::styled("└─ core   ", Style::default().fg(palette.dim)),
            Span::styled(file.clone(), Style::default().fg(palette.mid)),
        ]));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ScreenTheme;
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
        let rendered = lines(&graph, &palette, 0, true);
        let spans = rendered
            .iter()
            .flat_map(|line| line.spans.iter())
            .collect::<Vec<_>>();

        assert!(spans.iter().any(|span| {
            span.content.contains("charge_card") && span.style.fg == Some(palette.red)
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
        let text = lines(&graph, &palette, 0, false)
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
