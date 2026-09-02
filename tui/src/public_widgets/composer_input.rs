//! Public wrapper around the internal ChatComposer for simple, reusable text input.
//!
//! This exposes a minimal interface suitable for other crates (e.g.,
//! codex-cloud-tasks) to reuse the mature composer behavior: multi-line input,
//! paste heuristics, Enter-to-submit, and Shift+Enter for newline.

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Widget;
use std::time::Duration;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ChatComposer;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::slash_commands::ExternalCommand;
use crate::render::renderable::Renderable;

/// Action returned from feeding a key event into the ComposerInput.
pub enum ComposerAction {
    /// The user submitted the current text (typically via Enter). Contains the submitted text.
    Submitted(String),
    /// No submission occurred; UI may need to redraw if `needs_redraw()` returned true.
    None,
}

/// A slash command supplied by an application embedding the mature composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerCommand {
    pub name: String,
    pub description: String,
    /// This command runs when typed EXACTLY and is never offered as a correction.
    ///
    /// Set it on any command that deletes credentials, revokes a key or drops stored memory, on the
    /// day the command is written. See [`crate::bottom_pane::slash_commands::ExternalCommand`] for
    /// the measured incident: the popup's subsequence matcher reached `/logout` from `/logot`, and
    /// this popup's `Enter` arm for an external command submits the selection immediately.
    pub never_guessed: bool,
}

/// Brand colours supplied by an application embedding the complete bottom dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposerPanePalette {
    pub background: Color,
    pub focused_border: Color,
    pub idle_border: Color,
}

impl ComposerCommand {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            never_guessed: false,
        }
    }

    /// A command reachable only by its EXACT name — never completed to, never suggested.
    ///
    /// ⚠️ Deliberately a separate constructor rather than a mutating setter: a destructive command
    /// must be declared as one at the point it enters the catalog, not patched into safety later.
    pub fn new_never_guessed(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            never_guessed: true,
            ..Self::new(name, description)
        }
    }
}

/// A minimal, public wrapper for the internal `ChatComposer` that behaves as a
/// reusable text input field with submit semantics.
pub struct ComposerInput {
    inner: ChatComposer,
    _tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
}

impl ComposerInput {
    /// Create a new composer input with a neutral placeholder.
    pub fn new() -> Self {
        Self::with_config(crate::bottom_pane::ChatComposerConfig::default())
    }

    /// Create a composer that keeps editing and paste behavior but leaves slash-command
    /// discovery and dispatch to the embedding application.
    pub fn plain_text() -> Self {
        Self::with_config(crate::bottom_pane::ChatComposerConfig::plain_text())
    }

    /// Create a plain-text composer with copy owned by the embedding application.
    pub fn plain_text_with_placeholder(placeholder: impl Into<String>) -> Self {
        Self::with_placeholder(
            crate::bottom_pane::ChatComposerConfig::plain_text(),
            placeholder.into(),
        )
    }

    /// Create the full composer with a command catalog owned by the embedding application.
    pub fn with_commands(
        placeholder: impl Into<String>,
        commands: impl IntoIterator<Item = ComposerCommand>,
    ) -> Self {
        let mut composer = Self::with_placeholder(
            crate::bottom_pane::ChatComposerConfig::external_commands(),
            placeholder.into(),
        );
        composer.inner.set_external_commands(
            commands
                .into_iter()
                .map(|command| ExternalCommand {
                    name: command.name,
                    description: command.description,
                    never_guessed: command.never_guessed,
                })
                .collect(),
        );
        composer
    }

    /// Replace the command catalog after construction.
    ///
    /// 🔴 **THE CATALOG WAS FROZEN AT CONSTRUCTION AND THAT IS WHY SKILLS WERE UNCOMPLETABLE.**
    /// `with_commands` was the only writer, so the ~250 skill names — which arrive from the server
    /// long after the composer exists — could never enter the completion set. The composer offered
    /// 63 hardcoded names and nothing else, forever.
    pub fn set_commands(&mut self, commands: impl IntoIterator<Item = ComposerCommand>) {
        self.inner.set_external_commands(
            commands
                .into_iter()
                .map(|command| ExternalCommand {
                    name: command.name,
                    description: command.description,
                    never_guessed: command.never_guessed,
                })
                .collect(),
        );
    }

    fn with_config(config: crate::bottom_pane::ChatComposerConfig) -> Self {
        Self::with_placeholder(config, "Compose new task".to_string())
    }

    fn with_placeholder(
        config: crate::bottom_pane::ChatComposerConfig,
        placeholder: String,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx.clone());
        // `enhanced_keys_supported=true` enables Shift+Enter newline hint/behavior.
        let inner = ChatComposer::new_with_config(
            /*has_input_focus*/ true,
            sender,
            /*enhanced_keys_supported*/ true,
            placeholder,
            /*disable_paste_burst*/ false,
            config,
        );
        Self { inner, _tx: tx, rx }
    }

    /// Returns true if the input is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear the input text.
    pub fn clear(&mut self) {
        self.inner
            .set_text_content(String::new(), Vec::new(), Vec::new());
    }

    /// Replace the visible draft with plain text.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.inner
            .set_text_content(text.into(), Vec::new(), Vec::new());
    }

    /// Return the complete draft, expanding any pending paste placeholders.
    pub fn text(&self) -> String {
        self.inner.current_text_with_pending()
    }

    /// Return the byte offset of the editing caret in the visible draft.
    pub fn cursor(&self) -> usize {
        self.inner.cursor()
    }

    /// Feed a key event into the composer and return a high-level action.
    pub fn input(&mut self, key: KeyEvent) -> ComposerAction {
        let action = match self.inner.handle_key_event(key).0 {
            InputResult::Submitted { text, .. } => ComposerAction::Submitted(text),
            _ => ComposerAction::None,
        };
        self.drain_app_events();
        action
    }

    pub fn handle_paste(&mut self, pasted: String) -> bool {
        let handled = self.inner.handle_paste(pasted);
        self.drain_app_events();
        handled
    }

    /// Override the footer hint items displayed under the composer.
    /// Each tuple is rendered as "<key> <label>", with keys styled.
    pub fn set_hint_items(&mut self, items: Vec<(impl Into<String>, impl Into<String>)>) {
        let mapped: Vec<(String, String)> = items
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        self.inner.set_footer_hint_override(Some(mapped));
    }

    /// Clear any previously set custom hint items and restore the default hints.
    pub fn clear_hint_items(&mut self) {
        self.inner.set_footer_hint_override(/*items*/ None);
    }

    /// Desired height (in rows) for a given width.
    pub fn desired_height(&self, width: u16) -> u16 {
        self.inner.desired_height(width)
    }

    /// Desired height for the complete bordered bottom dock.
    pub fn bottom_pane_desired_height(&self, width: u16) -> u16 {
        self.inner
            .desired_height(width.saturating_sub(2))
            .saturating_add(2)
    }

    /// Compute the on-screen cursor position for the given area.
    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.inner.cursor_pos(area)
    }

    /// Compute the caret position inside the complete bordered bottom dock.
    pub fn bottom_pane_cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.inner.cursor_pos(Block::bordered().inner(area))
    }

    /// Render the input into the provided buffer at `area`.
    pub fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        self.inner.render(area, buf);
    }

    /// Render the mature composer while letting an embedding application own
    /// the canvas colour instead of inheriting Codex's user-message fill.
    pub fn render_ref_with_background(&self, area: Rect, buf: &mut Buffer, background: Color) {
        self.inner.render(area, buf);
        buf.set_style(area, Style::default().bg(background));
    }

    /// Render the complete bottom dock, with the application supplying only brand copy and focus.
    pub fn render_bottom_pane(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        focused: bool,
        palette: ComposerPanePalette,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if focused {
                palette.focused_border
            } else {
                palette.idle_border
            }))
            .title(format!(" {title} "));
        let inner = block.inner(area);
        block.render(area, buf);
        self.render_ref_with_background(inner, buf, palette.background);
    }

    /// Return true if a paste-burst detection is currently active.
    pub fn is_in_paste_burst(&self) -> bool {
        self.inner.is_in_paste_burst()
    }

    /// Flush a pending paste-burst if the inter-key timeout has elapsed.
    /// Returns true if text changed and a redraw is warranted.
    pub fn flush_paste_burst_if_due(&mut self) -> bool {
        let flushed = self.inner.flush_paste_burst_if_due();
        self.drain_app_events();
        flushed
    }

    /// Recommended delay to schedule the next micro-flush frame while a
    /// paste-burst is active.
    pub fn recommended_flush_delay() -> Duration {
        crate::bottom_pane::ChatComposer::recommended_paste_flush_delay()
    }

    fn drain_app_events(&mut self) {
        while self.rx.try_recv().is_ok() {}
    }
}

impl Default for ComposerInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyModifiers;

    #[test]
    fn external_command_popup_submits_the_canonical_command() {
        let mut composer = ComposerInput::with_commands(
            "Ask",
            [ComposerCommand::new("doctor", "probe provider binding")],
        );
        composer.set_text("/doc");

        let action = composer.input(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(matches!(action, ComposerAction::Submitted(text) if text == "/doctor"));
        assert!(composer.is_empty());
    }

    /// 🔴 **THIS TEST USED TO PIN THE DEFECT, AND IT IS INVERTED HERE ON PURPOSE.**
    ///
    /// It previously asserted that an unknown `/name` produces `ComposerAction::None` and leaves
    /// the draft in the composer — which is precisely the silence the founder hit: he typed
    /// `/skill:agent-injection-eval`, pressed enter, and nothing happened. No send, no refusal, the
    /// text still sitting there.
    ///
    /// The old behaviour looked defensible in isolation ("don't send a command that does not
    /// exist"), and that is why it survived: the composer refused, emitted an explanatory
    /// `AppEvent`, and [`ComposerInput::drain_app_events`] threw that event away unread. A refusal
    /// nobody renders is indistinguishable from a dropped keypress.
    ///
    /// The composer is not the dispatcher. It hands the draft on, and `App::submit` — which knows
    /// the aliases, the typo matcher and the server-side skill namespace — decides and SAYS SO.
    /// 🔴 **PRESS THE KEY. THE TEST ABOVE IS THE DEFECT, WITH A HARMLESS COMMAND IN IT.**
    ///
    /// `external_command_popup_submits_the_canonical_command` proves `/doc` + `Enter` **submits
    /// `/doctor`** — one keystroke, no confirmation, the draft rewritten to the popup's selection.
    /// Point that at `/logout`, whose 40-line implementation deletes every stored Estelle, ChatGPT,
    /// Claude, Copilot and local-provider credential, and `/logo` + `Enter` wipes your keys. `/logot`
    /// does the same through the subsequence tier, because `logot` is a subsequence of `logout` —
    /// which is the incident this repo already paid for once, arriving through a second door.
    ///
    /// ⚠️ Driven with a FAKE never-guessed command, deliberately: verifying the real one means
    /// destroying real credentials. What is asserted here is the RULE — a marked command is not
    /// reachable by a partial spelling through the composer's own key handling — plus the two
    /// controls that keep it from passing for the wrong reason.
    #[test]
    fn a_never_guessed_command_is_not_submitted_by_a_partial_spelling() {
        let compose = |text: &str| {
            let mut composer = ComposerInput::with_commands(
                "Ask",
                [
                    ComposerCommand::new_never_guessed("wipe-keys", "delete stored credentials"),
                    ComposerCommand::new("wire", "show the wiring"),
                ],
            );
            composer.set_text(text);
            composer.input(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        };

        for partial in ["/wipe", "/wipekeys", "/wipe-kes", "/w", "/wp"] {
            let action = compose(partial);
            assert!(
                !matches!(&action, ComposerAction::Submitted(text) if text == "/wipe-keys"),
                "{partial} + Enter submitted /wipe-keys — the destructive command ran from a \
                 partial spelling"
            );
        }

        // ⚠️ CONTROL 1 — the command still runs when spelled out. Refusing it outright is the OTHER
        // half of the shipped bug: `/logout` printed "no command" while `/logot` wiped the keys.
        assert!(
            matches!(compose("/wipe-keys"), ComposerAction::Submitted(text) if text == "/wipe-keys"),
            "the exact spelling stopped working — the command is now unreachable, not guarded"
        );
        // ⚠️ CONTROL 2 — completion still works for everything else, so the fence is narrow.
        assert!(
            matches!(compose("/wir"), ComposerAction::Submitted(text) if text == "/wire"),
            "ordinary completion broke: the fence became a blanket"
        );
    }

    #[test]
    fn an_unknown_command_is_handed_to_the_dispatcher_rather_than_silently_eaten() {
        let mut composer = ComposerInput::with_commands(
            "Ask",
            [ComposerCommand::new("doctor", "probe provider binding")],
        );
        composer.set_text("/definitely-missing");

        let action = composer.input(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(
            matches!(&action, ComposerAction::Submitted(text) if text == "/definitely-missing"),
            "the draft must reach the dispatcher, not die here"
        );
        assert!(
            composer.is_empty(),
            "a submitted draft must leave the composer, or the next keystroke appends to it"
        );
    }

    /// A skill invocation is submitted even though no catalog entry names that skill.
    ///
    /// The namespace row `skill:` stands for names the composer cannot know — they live on the
    /// server. Without it the composer adjudicated the whole namespace against a 63-name exact-match
    /// catalog and refused all of it.
    #[test]
    fn a_namespaced_command_submits_without_the_member_being_in_the_catalog() {
        let mut composer = ComposerInput::with_commands(
            "Ask",
            [
                ComposerCommand::new("doctor", "probe provider binding"),
                ComposerCommand::new("skill:", "run one Estelle skill playbook by name"),
            ],
        );
        composer.set_text("/skill:agent-injection-eval");

        let action = composer.input(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(
            matches!(&action, ComposerAction::Submitted(text) if text == "/skill:agent-injection-eval"),
            "a namespaced command must submit"
        );
    }
}
