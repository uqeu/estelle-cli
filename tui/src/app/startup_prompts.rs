//! Startup warnings, model migration prompts, and bootstrap prompt helpers.
//!
//! These helpers run before or during `App::run` bootstrap. They translate configuration and model
//! catalog state into one-time TUI prompts or warning cells without owning the main event loop.

use super::*;
use codex_config::ConfigLayerSource;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Hash)]
struct SkillLoadWarningKey {
    path: PathBuf,
    message: String,
}

#[derive(Debug, Default)]
pub(super) struct SkillLoadWarningState {
    active: HashSet<SkillLoadWarningKey>,
}

impl SkillLoadWarningState {
    pub(super) fn clear(&mut self) {
        self.active.clear();
    }

    pub(super) fn newly_active_errors(&mut self, errors: &[SkillErrorInfo]) -> Vec<SkillErrorInfo> {
        let previous = std::mem::take(&mut self.active);
        let mut current = HashSet::new();
        let mut newly_active = Vec::new();

        for error in errors {
            let key = SkillLoadWarningKey {
                path: error.path.clone(),
                message: error.message.clone(),
            };
            let was_active = previous.contains(&key);
            if current.insert(key) && !was_active {
                newly_active.push(error.clone());
            }
        }

        self.active = current;
        newly_active
    }
}

pub(super) fn emit_skill_load_warnings(app_event_tx: &AppEventSender, errors: &[SkillErrorInfo]) {
    if errors.is_empty() {
        return;
    }

    let error_count = errors.len();
    app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
        crate::history_cell::new_warning_event(format!(
            "Skipped loading {error_count} skill(s) due to invalid SKILL.md files."
        )),
    )));

    for error in errors {
        let path = error.path.display();
        let message = error.message.as_str();
        app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
            crate::history_cell::new_warning_event(format!("{path}: {message}")),
        )));
    }
}

pub(super) fn emit_project_config_warnings(app_event_tx: &AppEventSender, config: &Config) {
    let mut disabled_folders = Vec::new();

    for layer in config.config_layer_stack.get_layers(
        ConfigLayerStackOrdering::LowestPrecedenceFirst,
        /*include_disabled*/ true,
    ) {
        let ConfigLayerSource::Project { dot_codex_folder } = &layer.name else {
            continue;
        };
        let Some(disabled_reason) = &layer.disabled_reason else {
            continue;
        };
        disabled_folders.push((
            dot_codex_folder.as_path().display().to_string(),
            disabled_reason.clone(),
        ));
    }

    if disabled_folders.is_empty() {
        return;
    }

    let mut message = concat!(
        "Project-local config, hooks, and exec policies are disabled in the following folders ",
        "until the project is trusted, but skills still load.\n",
    )
    .to_string();
    for (index, (folder, reason)) in disabled_folders.iter().enumerate() {
        let display_index = index + 1;
        message.push_str(&format!("    {display_index}. {folder}\n"));
        message.push_str(&format!("       {reason}\n"));
    }

    app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
        history_cell::new_warning_event(message),
    )));
}

pub(super) fn emit_system_bwrap_warning(app_event_tx: &AppEventSender, config: &Config) {
    let Some(message) =
        codex_sandboxing::system_bwrap_warning(config.permissions.permission_profile())
    else {
        return;
    };

    app_event_tx.send(AppEvent::InsertHistoryCell(Box::new(
        history_cell::new_warning_event(message),
    )));
}

pub(super) const MODEL_AVAILABILITY_NUX_MAX_SHOW_COUNT: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StartupTooltipOverride {
    pub(super) model_slug: String,
    pub(super) message: String,
}

pub(super) fn select_model_availability_nux(
    available_models: &[ModelPreset],
    nux_config: &ModelAvailabilityNuxConfig,
) -> Option<StartupTooltipOverride> {
    let preset = available_models
        .iter()
        .find(|preset| preset.availability_nux.is_some())?;
    let ModelAvailabilityNux { message } = preset.availability_nux.as_ref()?;
    let shown_count = nux_config
        .shown_count
        .get(&preset.model)
        .copied()
        .unwrap_or_default();
    (shown_count < MODEL_AVAILABILITY_NUX_MAX_SHOW_COUNT).then(|| StartupTooltipOverride {
        model_slug: preset.model.clone(),
        message: message.clone(),
    })
}

pub(super) async fn prepare_startup_tooltip_override(
    config: &mut Config,
    available_models: &[ModelPreset],
    is_first_run: bool,
) -> Option<String> {
    if is_first_run || !config.show_tooltips {
        return None;
    }

    let tooltip_override =
        select_model_availability_nux(available_models, &config.model_availability_nux)?;

    let shown_count = config
        .model_availability_nux
        .shown_count
        .get(&tooltip_override.model_slug)
        .copied()
        .unwrap_or_default();
    let next_count = shown_count.saturating_add(1);
    let mut updated_shown_count = config.model_availability_nux.shown_count.clone();
    updated_shown_count.insert(tooltip_override.model_slug.clone(), next_count);

    if let Err(err) = ConfigEditsBuilder::for_config(config)
        .set_model_availability_nux_count(&updated_shown_count)
        .apply()
        .await
    {
        tracing::error!(
            error = %err,
            model = %tooltip_override.model_slug,
            "failed to persist model availability nux count"
        );
        return Some(tooltip_override.message);
    }

    config.model_availability_nux.shown_count = updated_shown_count;
    Some(tooltip_override.message)
}

pub(super) fn normalize_harness_overrides_for_cwd(
    mut overrides: ConfigOverrides,
    base_cwd: &AbsolutePathBuf,
) -> Result<ConfigOverrides> {
    if overrides.additional_writable_roots.is_empty() {
        return Ok(overrides);
    }

    let mut normalized = Vec::with_capacity(overrides.additional_writable_roots.len());
    for root in overrides.additional_writable_roots.drain(..) {
        let absolute = base_cwd.join(root);
        normalized.push(absolute.into_path_buf());
    }
    overrides.additional_writable_roots = normalized;
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PathBufExt;
    use pretty_assertions::assert_eq;
    use ratatui::text::Line;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn normalize_harness_overrides_resolves_relative_add_dirs() -> Result<()> {
        let temp_dir = tempdir()?;
        let base_cwd = temp_dir.path().join("base").abs();
        std::fs::create_dir_all(base_cwd.as_path())?;

        let overrides = ConfigOverrides {
            additional_writable_roots: vec![PathBuf::from("rel")],
            ..Default::default()
        };
        let normalized = normalize_harness_overrides_for_cwd(overrides, &base_cwd)?;

        assert_eq!(
            normalized.additional_writable_roots,
            vec![base_cwd.join("rel").into_path_buf()]
        );
        Ok(())
    }

    fn skill_error(path: &str, message: &str) -> SkillErrorInfo {
        SkillErrorInfo {
            path: PathBuf::from(path),
            message: message.to_string(),
        }
    }

    fn render_line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn render_skill_load_warning_cells(errors: &[SkillErrorInfo]) -> String {
        let (tx, mut rx) = unbounded_channel();
        let app_event_tx = AppEventSender::new(tx);

        emit_skill_load_warnings(&app_event_tx, errors);

        let mut rendered = Vec::new();
        while let Ok(AppEvent::InsertHistoryCell(cell)) = rx.try_recv() {
            rendered.extend(
                cell.display_lines(/*width*/ 120)
                    .iter()
                    .map(render_line_text),
            );
        }
        rendered.join("\n")
    }

    #[test]
    fn skill_load_warning_state_suppresses_repeated_active_errors() {
        let mut state = SkillLoadWarningState::default();
        let error = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid description");

        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            vec![error.clone()]
        );
        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            Vec::<SkillErrorInfo>::new()
        );
    }

    #[test]
    fn skill_load_warning_state_reemits_after_error_clears() {
        let mut state = SkillLoadWarningState::default();
        let error = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid description");

        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            vec![error.clone()]
        );
        assert_eq!(state.newly_active_errors(&[]), Vec::<SkillErrorInfo>::new());
        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            vec![error]
        );
    }

    #[test]
    fn skill_load_warning_state_displays_new_message_for_active_path() {
        let mut state = SkillLoadWarningState::default();
        let initial = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid description");
        let changed = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid frontmatter");

        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&initial)),
            vec![initial]
        );
        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&changed)),
            vec![changed]
        );
    }

    #[test]
    fn skill_load_warning_state_clear_allows_active_error_again() {
        let mut state = SkillLoadWarningState::default();
        let error = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid description");

        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            vec![error.clone()]
        );
        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            Vec::<SkillErrorInfo>::new()
        );

        state.clear();

        assert_eq!(
            state.newly_active_errors(std::slice::from_ref(&error)),
            vec![error]
        );
    }

    #[test]
    fn repeated_active_skill_load_warning_renders_once() {
        let mut state = SkillLoadWarningState::default();
        let error = skill_error("/repo/.codex/skills/abc/SKILL.md", "invalid description");

        let first_errors = state.newly_active_errors(std::slice::from_ref(&error));
        let repeated_errors = state.newly_active_errors(std::slice::from_ref(&error));
        let rendered = [
            render_skill_load_warning_cells(&first_errors),
            render_skill_load_warning_cells(&repeated_errors),
        ]
        .into_iter()
        .filter(|output| !output.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

        insta::assert_snapshot!(rendered, @r"
⚠ Skipped loading 1 skill(s) due to invalid SKILL.md files.
⚠ /repo/.codex/skills/abc/SKILL.md: invalid description
");
    }
}
