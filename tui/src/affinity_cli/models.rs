use std::collections::BTreeSet;

use estelle_client::CommandReply;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use serde_json::Value;
use serde_json::json;

use crate::Theme;
use crate::cols;

const ROLES: [&str; 3] = ["plan", "implement", "review"];
const MAX_VISIBLE_OPTIONS: usize = 12;
const MAX_ROUTING_ROWS: usize = 16;
const MAX_PROVIDER_ROWS: usize = 32;
const MAX_MODELS_PER_PROVIDER: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Selection {
    Auto,
    Pinned { provider: String, model: String },
}

impl Selection {
    fn mode(&self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Pinned { .. } => "PINNED",
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Auto => "Affinity chooses".to_string(),
            Self::Pinned { provider, model } => format!("{provider} / {model}"),
        }
    }

    fn row(&self, role: &str) -> Value {
        match self {
            Self::Auto => json!({"provider": "*", "task_kind": role, "mode": "auto"}),
            Self::Pinned { provider, model } => json!({
                "provider": provider,
                "task_kind": role,
                "mode": "pinned",
                "model": model,
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ModelsScreen {
    preset: String,
    selections: [Selection; 3],
    options: Vec<Selection>,
    role_index: usize,
    option_index: usize,
    status: String,
    loaded: bool,
    saving: bool,
}

impl ModelsScreen {
    pub(crate) fn loading() -> Self {
        Self {
            preset: String::new(),
            selections: [Selection::Auto, Selection::Auto, Selection::Auto],
            options: vec![Selection::Auto],
            role_index: 0,
            option_index: 0,
            status: "Loading the server-owned effective routing table".to_string(),
            loaded: false,
            saving: false,
        }
    }

    pub(crate) fn from_replies(
        presets: &CommandReply,
        providers: &CommandReply,
    ) -> Result<Self, String> {
        let bundle = presets
            .extra
            .get("bundle")
            .and_then(Value::as_object)
            .ok_or_else(|| "The server returned no effective preset bundle".to_string())?;
        let preset = text(bundle.get("name"))
            .ok_or_else(|| "The effective preset has no name".to_string())?;
        let table = bundle
            .get("routing_table")
            .and_then(Value::as_array)
            .ok_or_else(|| "The effective preset has no display routing table".to_string())?;
        let selections: [Selection; 3] = ROLES
            .map(|role| selection_for(table, role))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| "The effective preset did not contain exactly three roles".to_string())?;
        let options = configured_options(presets, providers);
        assert_eq!(selections.len(), ROLES.len());
        assert!(matches!(options.first(), Some(Selection::Auto)));
        let mut screen = Self {
            preset,
            selections,
            options,
            role_index: 0,
            option_index: 0,
            status: "Effective routing received from the server".to_string(),
            loaded: true,
            saving: false,
        };
        screen.sync_option_cursor();
        Ok(screen)
    }

    pub(crate) fn fail(&mut self, message: String) {
        self.status = message;
        self.loaded = false;
        self.saving = false;
    }

    pub(crate) fn select_role(&mut self, reverse: bool) {
        if !self.loaded {
            return;
        }
        self.role_index = rotate(self.role_index, ROLES.len(), reverse);
        self.sync_option_cursor();
    }

    pub(crate) fn select_option(&mut self, reverse: bool) {
        if !self.loaded || self.saving {
            return;
        }
        self.option_index = rotate(self.option_index, self.options.len(), reverse);
        self.selections[self.role_index] = self.options[self.option_index].clone();
        self.status = "Draft override changed locally; Enter sends the complete preset".to_string();
    }

    pub(crate) fn begin_save(&mut self) -> Option<Value> {
        if !self.loaded || self.saving {
            return None;
        }
        self.saving = true;
        self.status = "Sending one complete preset; the server still owns routing".to_string();
        Some(self.request_body())
    }

    pub(crate) fn apply_saved(&mut self, reply: &CommandReply) -> Result<(), String> {
        let bundle = reply
            .extra
            .get("bundle")
            .and_then(Value::as_object)
            .ok_or_else(|| "Save returned no effective preset read-back".to_string())?;
        let table = bundle
            .get("routing_table")
            .and_then(Value::as_array)
            .ok_or_else(|| "Save returned no effective display routing table".to_string())?;
        for (index, role) in ROLES.iter().enumerate() {
            self.selections[index] = selection_for(table, role)?;
        }
        if let Some(name) = text(bundle.get("name")) {
            self.preset = name;
        }
        self.saving = false;
        self.status = "Saved and read back from the server".to_string();
        self.sync_option_cursor();
        Ok(())
    }

    fn request_body(&self) -> Value {
        let rows = ROLES
            .iter()
            .zip(&self.selections)
            .map(|(role, selection)| selection.row(role))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), ROLES.len());
        assert!(rows.iter().all(|row| row.get("task_kind").is_some()));
        json!({"preset": self.preset, "routing_table": rows})
    }

    fn sync_option_cursor(&mut self) {
        self.option_index = self
            .options
            .iter()
            .position(|option| option == &self.selections[self.role_index])
            .unwrap_or(0);
    }

    pub(crate) fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: Theme) {
        let width = usize::from(area.width);
        let model_width = width.saturating_sub(31).max(12);
        let columns = [cols::Col::l(11), cols::Col::l(8), cols::Col::l(model_width)];
        let mut lines = vec![
            Line::styled(
                "MODELS",
                Style::default()
                    .fg(theme.semantic())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(
                "Affinity chooses by default. Pin only the role that needs a hard override.",
            ),
            Line::from(format!(
                "Preset  {}",
                if self.preset.is_empty() {
                    "not measured"
                } else {
                    &self.preset
                }
            )),
            Line::from(""),
            cols::head(
                &columns,
                &["ROLE", "MODE", "PROVIDER / MODEL"],
                theme.ghost(),
                2,
            ),
        ];
        for (index, role) in ROLES.iter().enumerate() {
            let selection = &self.selections[index];
            let label = selection.label();
            let line = cols::row(
                &columns,
                &[
                    cols::Cell(role, theme.primary()),
                    cols::Cell(
                        selection.mode(),
                        if matches!(selection, Selection::Auto) {
                            theme.semantic()
                        } else {
                            theme.alert()
                        },
                    ),
                    cols::Cell(&label, theme.primary()),
                ],
                2,
            );
            lines.push(highlight(line, index == self.role_index, theme));
        }
        lines.extend([
            Line::from(""),
            Line::styled(
                "OPTIONS FOR SELECTED ROLE",
                Style::default().fg(theme.ghost()),
            ),
        ]);
        for (index, option) in self.options.iter().take(MAX_VISIBLE_OPTIONS).enumerate() {
            let marker = if index == self.option_index { ">" } else { " " };
            let line = Line::from(vec![
                Span::raw(format!("  {marker}  ")),
                Span::raw(option.label()),
            ]);
            lines.push(highlight(line, index == self.option_index, theme));
        }
        lines.extend([
            Line::from(""),
            Line::styled(
                &self.status,
                Style::default().fg(if self.loaded {
                    theme.ghost()
                } else {
                    theme.alert()
                }),
            ),
            Line::styled(
                "Tab / left / right role   up / down option   Enter save   Esc close",
                Style::default().fg(theme.ghost()),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().fg(theme.primary())),
            area,
        );
    }
}

fn selection_for(table: &[Value], role: &str) -> Result<Selection, String> {
    let row = table
        .iter()
        .take(MAX_ROUTING_ROWS)
        .find(|row| row.get("task_kind").and_then(Value::as_str) == Some(role))
        .ok_or_else(|| format!("The effective preset omitted {role}"))?;
    match row.get("mode").and_then(Value::as_str) {
        Some("auto") => Ok(Selection::Auto),
        Some("pinned") => Ok(Selection::Pinned {
            provider: text(row.get("provider"))
                .ok_or_else(|| format!("Pinned {role} omitted provider"))?,
            model: text(row.get("model")).ok_or_else(|| format!("Pinned {role} omitted model"))?,
        }),
        Some(mode) => Err(format!("The effective {role} mode {mode:?} is unsupported")),
        None => Err(format!("The effective {role} mode was not returned")),
    }
}

fn configured_options(presets: &CommandReply, providers: &CommandReply) -> Vec<Selection> {
    let configured = providers
        .extra
        .get("configured")
        .or_else(|| presets.extra.get("configured_providers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_PROVIDER_ROWS)
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut options = vec![Selection::Auto];
    for provider in providers
        .extra
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_PROVIDER_ROWS)
    {
        let Some(id) = text(provider.get("id")) else {
            continue;
        };
        if !configured.contains(id.as_str()) {
            continue;
        }
        for model in provider
            .get("models")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(MAX_MODELS_PER_PROVIDER)
            .filter_map(Value::as_str)
        {
            options.push(Selection::Pinned {
                provider: id.clone(),
                model: model.to_string(),
            });
        }
    }
    assert!(matches!(options.first(), Some(Selection::Auto)));
    assert!(
        options
            .iter()
            .skip(1)
            .all(|option| matches!(option, Selection::Pinned { .. }))
    );
    options
}

fn rotate(index: usize, length: usize, reverse: bool) -> usize {
    assert!(length > 0);
    assert!(index < length);
    if reverse {
        index.checked_sub(1).unwrap_or(length - 1)
    } else {
        (index + 1) % length
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn highlight<'a>(line: Line<'a>, selected: bool, theme: Theme) -> Line<'a> {
    if selected {
        line.style(
            Style::default()
                .fg(theme.background())
                .bg(theme.semantic())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replies() -> (CommandReply, CommandReply) {
        let presets = serde_json::from_value(json!({
            "bundle": {"name": "coding", "routing_table": [
                {"task_kind": "plan", "mode": "auto", "provider": "*"},
                {"task_kind": "implement", "mode": "pinned", "provider": "openai", "model": "gpt-5.6-sol"},
                {"task_kind": "review", "mode": "pinned", "provider": "anthropic", "model": "claude-opus-4-8"}
            ], "_routing_table": [
                {"task_kind": "implement", "mode": "pinned", "provider": "wrong", "model": "stale"}
            ]},
            "configured_providers": ["openai"]
        })).expect("preset reply");
        let providers = serde_json::from_value(json!({
            "configured": ["openai"],
            "providers": [
                {"id": "openai", "models": ["gpt-5.6-sol", "gpt-5.5"]},
                {"id": "anthropic", "models": ["claude-opus-4-8"]}
            ]
        }))
        .expect("provider reply");
        (presets, providers)
    }

    #[test]
    fn effective_display_table_wins_and_unconfigured_models_are_not_offered() {
        let (presets, providers) = replies();
        let screen = ModelsScreen::from_replies(&presets, &providers).expect("models screen");
        assert_eq!(screen.selections[1].label(), "openai / gpt-5.6-sol");
        assert_eq!(screen.selections[2].label(), "anthropic / claude-opus-4-8");
        assert!(
            !screen
                .options
                .iter()
                .any(|option| option.label().contains("anthropic"))
        );
        assert!(
            !screen
                .options
                .iter()
                .any(|option| option.label().contains("stale"))
        );
    }

    #[test]
    fn save_proposes_one_complete_table_without_the_private_routing_key() {
        let (presets, providers) = replies();
        let mut screen = ModelsScreen::from_replies(&presets, &providers).expect("models screen");
        let body = screen.begin_save().expect("save body");
        assert_eq!(body["routing_table"].as_array().map(Vec::len), Some(3));
        assert!(body.get("_routing_table").is_none());
        assert_eq!(body["routing_table"][1]["model"], "gpt-5.6-sol");
    }
}
