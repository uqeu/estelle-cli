//! Keystroke arbitration: who wins a keystroke, Estelle or the child process on the PTY.
//!
//! A pure `event -> disposition` function with no terminal I/O, no global state and no clock, so
//! every branch is reachable from a test. Ported from Orca's
//! `src/renderer/src/components/terminal-pane/terminal-shortcut-policy.ts` (MIT), which is years
//! of terminal-compatibility bug reports compressed into one table.
//!
//! Three things decide a keystroke:
//!
//! 1. **A user-facing policy.** Under [`TerminalShortcutPolicy::EstelleFirst`] an app chord keeps
//!    working inside a terminal surface. Under `TerminalFirst` an app-scoped chord yields to the
//!    shell, which is the escape hatch for people running their own TUI inside ours. Chords whose
//!    scope IS the terminal keep working under both, because yielding them would leave the pane
//!    with no way to close itself.
//!
//! 2. **The kitty keyboard protocol.** When the child has negotiated KKP it emits its own,
//!    better-specified sequences, and injecting a legacy fallback on top of that is how alt+arrow
//!    arrives at a TUI as `alt+b` / `alt+f`. Every byte fallback below that has a native KKP
//!    encoding is gated on [`KittyKeyboard`], and under negotiation the policy DECLINES rather
//!    than substituting.
//!
//! 3. **A concrete control-byte table**, for the chords no terminal encodes on its own.
//!
//! ## Where this deliberately diverges from Orca
//!
//! Orca's function returns `null` for two different facts: "no rule matched this key" and "a rule
//! matched and is deliberately standing down so the native encoding wins". Those coincide at the
//! call site, so the collapse is invisible there, but a reader cannot tell a declined key from an
//! unrecognised one, and neither can a test. [`TerminalKeyDisposition`] keeps them apart, so that
//! [`TerminalKeyDisposition::DeferToNativeEncoding`] is an assertion a test can make about the KKP
//! gate specifically rather than an absence it has to infer.

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::KeyboardEnhancementFlags;

/// Whether a remapped app chord yields to the shell inside a terminal surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TerminalShortcutPolicy {
    /// App chords keep working inside a terminal. The default, and what most people want.
    #[default]
    EstelleFirst,
    /// App-scoped chords yield to the child; only terminal-scoped chords are still claimed.
    TerminalFirst,
}

/// What the child has told us about the kitty keyboard protocol.
///
/// **Absent is not zero, and that distinction is the whole point of this type.** A child that has
/// proven it is NOT using KKP and a child we simply have not heard from are different facts:
/// laundering the second into the first is how a legacy fallback gets injected under a protocol
/// that would have encoded the key correctly. Our own startup probe already models this honestly
/// as `Option<bool>` (`tui/src/terminal_probe.rs`), so the three states survive all the way here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum KittyKeyboard {
    /// We have not been able to prove either state.
    #[default]
    Unknown,
    /// Proven not negotiated: legacy byte fallbacks are the child's only chance at these chords.
    NotNegotiated,
    /// Proven negotiated, with these flags. Empty flags are still `NotNegotiated` in effect; see
    /// [`KittyKeyboard::emits_native_sequences`].
    Negotiated(KeyboardEnhancementFlags),
}

impl KittyKeyboard {
    /// True when the child will emit its own sequence for a modified key, so any fallback we
    /// inject would reach it as a DIFFERENT chord rather than as the one the user pressed.
    ///
    /// [`KittyKeyboard::Unknown`] resolves here to `false`, matching Orca's `?? 0` at the same
    /// decision point. It is a real choice with a real cost either way and it is made once, here,
    /// rather than at each of the five call sites: resolving Unknown as negotiated would silently
    /// drop word-navigation for every plain readline shell we have not probed, which is the far
    /// more common child. The narrower failure is the one we take.
    pub(crate) fn emits_native_sequences(self) -> bool {
        match self {
            KittyKeyboard::Negotiated(flags) => !flags.is_empty(),
            KittyKeyboard::Unknown | KittyKeyboard::NotNegotiated => false,
        }
    }
}

/// The scope a claimed chord belongs to, which is what `TerminalFirst` filters on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChordScope {
    /// Belongs to the terminal surface itself. Survives `TerminalFirst`.
    Terminal,
    /// Belongs to the wider app. Yields to the child under `TerminalFirst`.
    App,
}

/// An app action claimed away from the child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalAppAction {
    CopySelection,
    SelectAll,
    ToggleSearch,
    ClearScrollback,
    ClosePane,
}

impl TerminalAppAction {
    pub(crate) fn scope(self) -> ChordScope {
        match self {
            TerminalAppAction::CopySelection
            | TerminalAppAction::SelectAll
            | TerminalAppAction::ToggleSearch => ChordScope::App,
            TerminalAppAction::ClearScrollback | TerminalAppAction::ClosePane => {
                ChordScope::Terminal
            }
        }
    }
}

/// Where a scroll chord takes the viewport. Our own scrollback moves; the child sees nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScrollTarget {
    Top,
    Bottom,
}

/// What the policy decided to do with the key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalShortcutAction {
    /// Write these exact bytes to the child instead of whatever the terminal would have sent.
    SendInput(&'static str),
    /// Move our viewport. The child never sees this key.
    ScrollViewport(ScrollTarget),
    /// Estelle claims the chord.
    App(TerminalAppAction),
}

/// The three answers, kept apart on purpose. See the module docs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalKeyDisposition {
    /// No rule matched. The caller forwards whatever the terminal produced.
    Unclaimed,
    /// A rule matched and stood down because the child negotiated KKP and encodes this key
    /// natively. Behaves like `Unclaimed` at the call site and is NOT the same fact.
    DeferToNativeEncoding,
    /// Do this.
    Act(TerminalShortcutAction),
}

/// Everything about the host and the child that the table below is allowed to read.
///
/// Passed in rather than probed so the function stays pure and every combination is reachable
/// from a test without a live PTY.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TerminalKeyContext {
    /// Client platform, which is what the modifier conventions follow.
    pub(crate) is_mac: bool,
    /// What the child has negotiated.
    pub(crate) kitty: KittyKeyboard,
    /// Whether the chord yields to the shell.
    pub(crate) policy: TerminalShortcutPolicy,
    /// A local Windows ConPTY child. PSReadLine binds ctrl+arrow itself and prints a stray `b`/`f`
    /// if we translate; remote and WSL children run readline and need the translation.
    pub(crate) is_local_windows_conpty: bool,
}

impl Default for TerminalKeyContext {
    fn default() -> Self {
        Self {
            is_mac: cfg!(target_os = "macos"),
            kitty: KittyKeyboard::Unknown,
            policy: TerminalShortcutPolicy::EstelleFirst,
            is_local_windows_conpty: false,
        }
    }
}

// The control-byte table. Every one of these is a chord the terminal either encodes as something
// the child does not bind, or does not encode at all.
const CTRL_W: &str = "\u{17}";
const CTRL_U: &str = "\u{15}";
const CTRL_K: &str = "\u{0b}";
const CTRL_A: &str = "\u{01}";
const CTRL_E: &str = "\u{05}";
const ALT_BACKSPACE: &str = "\u{1b}\u{7f}";
const ALT_B: &str = "\u{1b}b";
const ALT_F: &str = "\u{1b}f";
const ESC_CR: &str = "\u{1b}\r";
const CR: &str = "\r";
const CSI_U_SHIFT_ENTER: &str = "\u{1b}[13;2u";
const CSI_U_CTRL_ENTER: &str = "\u{1b}[13;5u";

/// Resolve one key press against the policy.
///
/// Key RELEASES and REPEATS are not arbitrated: a release that reached us at all means the child
/// negotiated event reporting, and re-encoding one as a press would double every keystroke.
pub(crate) fn resolve_terminal_key(
    event: KeyEvent,
    context: TerminalKeyContext,
) -> TerminalKeyDisposition {
    if event.kind != KeyEventKind::Press {
        return TerminalKeyDisposition::Unclaimed;
    }

    if let Some(action) = app_chord(event, context.is_mac) {
        let yields = context.policy == TerminalShortcutPolicy::TerminalFirst
            && action.scope() == ChordScope::App;
        if yields {
            return TerminalKeyDisposition::Unclaimed;
        }
        return TerminalKeyDisposition::Act(TerminalShortcutAction::App(action));
    }

    byte_fallback(event, context)
}

/// Chords Estelle claims for itself before the child sees them.
fn app_chord(event: KeyEvent, is_mac: bool) -> Option<TerminalAppAction> {
    let primary = if is_mac {
        KeyModifiers::SUPER
    } else {
        KeyModifiers::CONTROL
    };
    let shift_primary = primary | KeyModifiers::SHIFT;

    match (event.modifiers, event.code) {
        (m, KeyCode::Char('c')) if m == shift_primary => Some(TerminalAppAction::CopySelection),
        (m, KeyCode::Char('a')) if m == primary => Some(TerminalAppAction::SelectAll),
        (m, KeyCode::Char('f')) if m == primary => Some(TerminalAppAction::ToggleSearch),
        (m, KeyCode::Char('k')) if m == primary => Some(TerminalAppAction::ClearScrollback),
        (m, KeyCode::Char('w')) if m == primary => Some(TerminalAppAction::ClosePane),
        _ => None,
    }
}

/// The byte table, in Orca's order. Each arm states which modifiers must be ABSENT, because a
/// chord that matches on presence alone swallows every superset of itself.
fn byte_fallback(event: KeyEvent, context: TerminalKeyContext) -> TerminalKeyDisposition {
    let m = event.modifiers;
    let ctrl = m.contains(KeyModifiers::CONTROL);
    let alt = m.contains(KeyModifiers::ALT);
    let shift = m.contains(KeyModifiers::SHIFT);
    let meta = m.contains(KeyModifiers::SUPER);

    // shift+enter: a newline inside a prompt rather than a submit.
    if shift && !ctrl && !alt && !meta && event.code == KeyCode::Enter {
        // Not a decline: CSI-u IS the negotiated encoding, so under KKP we send it ourselves
        // rather than standing down. Both branches send, and they send different bytes.
        return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(
            if context.kitty.emits_native_sequences() {
                CSI_U_SHIFT_ENTER
            } else {
                ESC_CR
            },
        ));
    }

    // ctrl+enter: CSI-u everywhere except a local ConPTY that has not negotiated, where a bare
    // carriage return is the only thing PSReadLine will accept.
    if ctrl && !shift && !alt && !meta && event.code == KeyCode::Enter {
        let csi_u = !context.is_local_windows_conpty || context.kitty.emits_native_sequences();
        return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(if csi_u {
            CSI_U_CTRL_ENTER
        } else {
            CR
        }));
    }

    // ctrl+backspace: delete the previous word. No terminal encodes this.
    if ctrl && !shift && !alt && !meta && event.code == KeyCode::Backspace {
        return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(CTRL_W));
    }

    // The cmd+* line-editing set, macOS only, where cmd is the line-granularity modifier.
    if context.is_mac && meta && !ctrl && !alt && !shift {
        let action = match event.code {
            KeyCode::Backspace => Some(TerminalShortcutAction::SendInput(CTRL_U)),
            KeyCode::Delete => Some(TerminalShortcutAction::SendInput(CTRL_K)),
            // No terminal maps cmd+arrow; readline's line-start/line-end are the intent.
            KeyCode::Left => Some(TerminalShortcutAction::SendInput(CTRL_A)),
            KeyCode::Right => Some(TerminalShortcutAction::SendInput(CTRL_E)),
            // cmd+up/down is scrollback on macOS, and must not write escape bytes to the shell.
            KeyCode::Up => Some(TerminalShortcutAction::ScrollViewport(ScrollTarget::Top)),
            KeyCode::Down => Some(TerminalShortcutAction::ScrollViewport(ScrollTarget::Bottom)),
            _ => None,
        };
        if let Some(action) = action {
            return TerminalKeyDisposition::Act(action);
        }
    }

    // alt+backspace: delete the previous word.
    if alt && !ctrl && !shift && !meta && event.code == KeyCode::Backspace {
        // A KKP child binds the CSI 127;3u the terminal emits natively. The legacy fallback would
        // bypass that binding entirely.
        if context.kitty.emits_native_sequences() {
            return TerminalKeyDisposition::DeferToNativeEncoding;
        }
        return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(ALT_BACKSPACE));
    }

    // alt+left/right: word navigation.
    if alt && !ctrl && !shift && !meta {
        if let Some(bytes) = word_nav_bytes(event.code) {
            // THE bug this gate exists for: a KKP child binds alt+arrow through the native
            // CSI 1;3D/C, and \eb / \ef would arrive at it as alt+b / alt+f instead.
            if context.kitty.emits_native_sequences() {
                return TerminalKeyDisposition::DeferToNativeEncoding;
            }
            return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(bytes));
        }
    }

    // ctrl+left/right: word navigation off macOS, where ctrl+arrow is not reserved.
    if !context.is_mac && ctrl && !alt && !shift && !meta {
        if let Some(bytes) = word_nav_bytes(event.code) {
            // PSReadLine binds ctrl+arrow itself; translating prints a stray b/f in the prompt.
            if context.is_local_windows_conpty {
                return TerminalKeyDisposition::DeferToNativeEncoding;
            }
            return TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(bytes));
        }
    }

    TerminalKeyDisposition::Unclaimed
}

/// readline ignores the terminal's own `\e[1;3D` / `\e[1;5C`, so word-nav is `\eb` / `\ef`.
fn word_nav_bytes(code: KeyCode) -> Option<&'static str> {
    match code {
        KeyCode::Left => Some(ALT_B),
        KeyCode::Right => Some(ALT_F),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KKP_DISAMBIGUATE: KeyboardEnhancementFlags =
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;

    fn negotiated() -> KittyKeyboard {
        KittyKeyboard::Negotiated(KKP_DISAMBIGUATE)
    }

    fn ctx(kitty: KittyKeyboard) -> TerminalKeyContext {
        TerminalKeyContext {
            is_mac: false,
            kitty,
            policy: TerminalShortcutPolicy::EstelleFirst,
            is_local_windows_conpty: false,
        }
    }

    fn mac(kitty: KittyKeyboard) -> TerminalKeyContext {
        TerminalKeyContext {
            is_mac: true,
            ..ctx(kitty)
        }
    }

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn sent(disposition: TerminalKeyDisposition) -> Option<&'static str> {
        match disposition {
            TerminalKeyDisposition::Act(TerminalShortcutAction::SendInput(bytes)) => Some(bytes),
            _ => None,
        }
    }

    // ------------------------------------------------------------ the KKP gate
    //
    // Each of these asserts BOTH states. A single-state test certifies half the contract and
    // reads as a verdict on the whole: it passes just as green against an implementation that
    // ignores the gate entirely.

    #[test]
    fn alt_arrow_defers_under_kkp_and_injects_word_nav_without_it() {
        let negotiated_left =
            resolve_terminal_key(press(KeyCode::Left, KeyModifiers::ALT), ctx(negotiated()));
        let legacy_left = resolve_terminal_key(
            press(KeyCode::Left, KeyModifiers::ALT),
            ctx(KittyKeyboard::NotNegotiated),
        );

        // Under KKP the terminal's own CSI 1;3D is what the child bound. Injecting \eb would
        // reach it as alt+b, which is the entire reason this gate exists.
        assert_eq!(
            negotiated_left,
            TerminalKeyDisposition::DeferToNativeEncoding
        );
        assert_eq!(sent(legacy_left), Some("\u{1b}b"));
        assert_ne!(negotiated_left, legacy_left);

        let negotiated_right =
            resolve_terminal_key(press(KeyCode::Right, KeyModifiers::ALT), ctx(negotiated()));
        let legacy_right = resolve_terminal_key(
            press(KeyCode::Right, KeyModifiers::ALT),
            ctx(KittyKeyboard::NotNegotiated),
        );
        assert_eq!(
            negotiated_right,
            TerminalKeyDisposition::DeferToNativeEncoding
        );
        assert_eq!(sent(legacy_right), Some("\u{1b}f"));
    }

    #[test]
    fn alt_backspace_defers_under_kkp_and_injects_the_legacy_pair_without_it() {
        let under_kkp = resolve_terminal_key(
            press(KeyCode::Backspace, KeyModifiers::ALT),
            ctx(negotiated()),
        );
        let without = resolve_terminal_key(
            press(KeyCode::Backspace, KeyModifiers::ALT),
            ctx(KittyKeyboard::NotNegotiated),
        );

        assert_eq!(under_kkp, TerminalKeyDisposition::DeferToNativeEncoding);
        assert_eq!(sent(without), Some("\u{1b}\u{7f}"));
        assert_ne!(under_kkp, without);
    }

    #[test]
    fn shift_enter_sends_csi_u_under_kkp_and_esc_cr_without_it() {
        let under_kkp =
            resolve_terminal_key(press(KeyCode::Enter, KeyModifiers::SHIFT), ctx(negotiated()));
        let without = resolve_terminal_key(
            press(KeyCode::Enter, KeyModifiers::SHIFT),
            ctx(KittyKeyboard::NotNegotiated),
        );

        // Both branches SEND here, and they send different bytes: CSI-u is application input, so
        // it is only correct once the child has asked for it.
        assert_eq!(sent(under_kkp), Some("\u{1b}[13;2u"));
        assert_eq!(sent(without), Some("\u{1b}\r"));
        assert_ne!(under_kkp, without);
    }

    #[test]
    fn ctrl_enter_sends_csi_u_under_kkp_and_a_bare_cr_on_an_unnegotiated_conpty() {
        let conpty = |kitty| TerminalKeyContext {
            is_local_windows_conpty: true,
            ..ctx(kitty)
        };

        let under_kkp = resolve_terminal_key(
            press(KeyCode::Enter, KeyModifiers::CONTROL),
            conpty(negotiated()),
        );
        let without = resolve_terminal_key(
            press(KeyCode::Enter, KeyModifiers::CONTROL),
            conpty(KittyKeyboard::NotNegotiated),
        );

        assert_eq!(sent(under_kkp), Some("\u{1b}[13;5u"));
        assert_eq!(sent(without), Some("\r"));
        assert_ne!(under_kkp, without);
    }

    #[test]
    fn ctrl_enter_off_conpty_is_csi_u_in_both_kkp_states_and_that_is_deliberate() {
        // Stated out loud rather than left as an untested gap: away from a local ConPTY the gate
        // does NOT branch on KKP, because a query-only TUI binds CSI-u without ever negotiating.
        // A test that asserted a difference here would be asserting a bug.
        let under_kkp =
            resolve_terminal_key(press(KeyCode::Enter, KeyModifiers::CONTROL), ctx(negotiated()));
        let without = resolve_terminal_key(
            press(KeyCode::Enter, KeyModifiers::CONTROL),
            ctx(KittyKeyboard::NotNegotiated),
        );
        assert_eq!(sent(under_kkp), Some("\u{1b}[13;5u"));
        assert_eq!(under_kkp, without);
    }

    // -------------------------------------------------- absent is not zero

    #[test]
    fn unknown_kkp_is_a_distinct_value_that_resolves_to_the_legacy_fallback() {
        // The three states are distinguishable as DATA.
        assert_ne!(KittyKeyboard::Unknown, KittyKeyboard::NotNegotiated);
        assert_ne!(KittyKeyboard::Unknown, negotiated());

        // ...and Unknown resolves to "no native encoding", which is a decision with a cost, made
        // once, and named. If this ever flips, these three lines are where it is argued.
        assert!(!KittyKeyboard::Unknown.emits_native_sequences());
        assert_eq!(
            sent(resolve_terminal_key(
                press(KeyCode::Left, KeyModifiers::ALT),
                ctx(KittyKeyboard::Unknown)
            )),
            Some("\u{1b}b")
        );
    }

    #[test]
    fn negotiating_with_empty_flags_is_not_negotiating() {
        // A child that pushed an empty flag set has asked for nothing, so it encodes nothing
        // natively and the fallbacks still apply.
        let empty = KittyKeyboard::Negotiated(KeyboardEnhancementFlags::empty());
        assert!(!empty.emits_native_sequences());
        assert_eq!(
            sent(resolve_terminal_key(
                press(KeyCode::Left, KeyModifiers::ALT),
                ctx(empty)
            )),
            Some("\u{1b}b")
        );
    }

    // ------------------------------------------------------ the byte table

    #[test]
    fn the_control_byte_table_is_exactly_orcas() {
        let cases: [(KeyCode, KeyModifiers, bool, &str); 7] = [
            (KeyCode::Backspace, KeyModifiers::CONTROL, false, "\u{17}"),
            (KeyCode::Backspace, KeyModifiers::SUPER, true, "\u{15}"),
            (KeyCode::Delete, KeyModifiers::SUPER, true, "\u{0b}"),
            (KeyCode::Left, KeyModifiers::SUPER, true, "\u{01}"),
            (KeyCode::Right, KeyModifiers::SUPER, true, "\u{05}"),
            (KeyCode::Left, KeyModifiers::CONTROL, false, "\u{1b}b"),
            (KeyCode::Right, KeyModifiers::CONTROL, false, "\u{1b}f"),
        ];
        for (code, modifiers, is_mac, expected) in cases {
            let context = if is_mac {
                mac(KittyKeyboard::NotNegotiated)
            } else {
                ctx(KittyKeyboard::NotNegotiated)
            };
            assert_eq!(
                sent(resolve_terminal_key(press(code, modifiers), context)),
                Some(expected),
                "wrong bytes for {code:?} + {modifiers:?}"
            );
        }
    }

    #[test]
    fn cmd_up_and_down_scroll_our_viewport_instead_of_writing_escape_bytes() {
        for (code, target) in [
            (KeyCode::Up, ScrollTarget::Top),
            (KeyCode::Down, ScrollTarget::Bottom),
        ] {
            let disposition = resolve_terminal_key(
                press(code, KeyModifiers::SUPER),
                mac(KittyKeyboard::NotNegotiated),
            );
            assert_eq!(
                disposition,
                TerminalKeyDisposition::Act(TerminalShortcutAction::ScrollViewport(target))
            );
            // The point of the arm: nothing reaches the child.
            assert_eq!(sent(disposition), None);
        }
    }

    #[test]
    fn the_cmd_table_is_macos_only() {
        for code in [KeyCode::Backspace, KeyCode::Delete, KeyCode::Left] {
            assert_eq!(
                resolve_terminal_key(
                    press(code, KeyModifiers::SUPER),
                    ctx(KittyKeyboard::NotNegotiated)
                ),
                TerminalKeyDisposition::Unclaimed,
                "{code:?} claimed cmd off macOS"
            );
        }
    }

    #[test]
    fn ctrl_arrow_defers_on_a_local_conpty_because_psreadline_binds_it() {
        let conpty = TerminalKeyContext {
            is_local_windows_conpty: true,
            ..ctx(KittyKeyboard::NotNegotiated)
        };
        assert_eq!(
            resolve_terminal_key(press(KeyCode::Left, KeyModifiers::CONTROL), conpty),
            TerminalKeyDisposition::DeferToNativeEncoding
        );
    }

    #[test]
    fn ctrl_arrow_is_not_claimed_on_macos_where_it_is_reserved() {
        assert_eq!(
            resolve_terminal_key(
                press(KeyCode::Left, KeyModifiers::CONTROL),
                mac(KittyKeyboard::NotNegotiated)
            ),
            TerminalKeyDisposition::Unclaimed
        );
    }

    // ------------------------------------------------- the policy toggle

    #[test]
    fn terminal_first_yields_app_chords_and_keeps_terminal_chords() {
        let terminal_first = TerminalKeyContext {
            policy: TerminalShortcutPolicy::TerminalFirst,
            ..ctx(KittyKeyboard::NotNegotiated)
        };
        let estelle_first = ctx(KittyKeyboard::NotNegotiated);

        // An app-scoped chord stands down so the shell can have it...
        let search = press(KeyCode::Char('f'), KeyModifiers::CONTROL);
        assert_eq!(
            resolve_terminal_key(search, estelle_first),
            TerminalKeyDisposition::Act(TerminalShortcutAction::App(
                TerminalAppAction::ToggleSearch
            ))
        );
        assert_eq!(
            resolve_terminal_key(search, terminal_first),
            TerminalKeyDisposition::Unclaimed
        );

        // ...but a terminal-scoped one does not, or the pane could never be closed.
        let close = press(KeyCode::Char('w'), KeyModifiers::CONTROL);
        assert_eq!(
            resolve_terminal_key(close, terminal_first),
            TerminalKeyDisposition::Act(TerminalShortcutAction::App(TerminalAppAction::ClosePane))
        );
        assert_eq!(
            resolve_terminal_key(close, terminal_first),
            resolve_terminal_key(close, estelle_first)
        );
    }

    // ------------------------------------------------- negative controls
    //
    // A table that matched on modifier PRESENCE would swallow every superset of each chord, and
    // every assertion above would still be green.

    #[test]
    fn a_superset_of_a_claimed_chord_is_not_that_chord() {
        for modifiers in [
            KeyModifiers::CONTROL | KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyModifiers::CONTROL | KeyModifiers::SUPER,
        ] {
            assert_eq!(
                resolve_terminal_key(
                    press(KeyCode::Backspace, modifiers),
                    ctx(KittyKeyboard::NotNegotiated)
                ),
                TerminalKeyDisposition::Unclaimed,
                "ctrl+backspace matched under extra modifiers {modifiers:?}"
            );
        }
    }

    #[test]
    fn an_ordinary_keystroke_is_never_claimed() {
        for code in [
            KeyCode::Char('a'),
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::Left,
            KeyCode::Up,
        ] {
            assert_eq!(
                resolve_terminal_key(
                    press(code, KeyModifiers::NONE),
                    ctx(KittyKeyboard::NotNegotiated)
                ),
                TerminalKeyDisposition::Unclaimed,
                "{code:?} was claimed with no modifiers held"
            );
        }
    }

    #[test]
    fn releases_and_repeats_are_not_arbitrated() {
        // A release only reaches us because the child asked for event reporting. Re-encoding one
        // as a press doubles every keystroke it touches.
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            let mut event = press(KeyCode::Left, KeyModifiers::ALT);
            event.kind = kind;
            assert_eq!(
                resolve_terminal_key(event, ctx(KittyKeyboard::NotNegotiated)),
                TerminalKeyDisposition::Unclaimed,
                "{kind:?} was arbitrated"
            );
        }
    }

    #[test]
    fn declining_and_not_recognising_are_different_answers() {
        // Orca collapses both into `null`. Keeping them apart is what lets the KKP tests above
        // assert the gate rather than infer it from an absence.
        let declined = resolve_terminal_key(press(KeyCode::Left, KeyModifiers::ALT), ctx(negotiated()));
        let unrecognised = resolve_terminal_key(
            press(KeyCode::Char('q'), KeyModifiers::NONE),
            ctx(negotiated()),
        );
        assert_eq!(declined, TerminalKeyDisposition::DeferToNativeEncoding);
        assert_eq!(unrecognised, TerminalKeyDisposition::Unclaimed);
        assert_ne!(declined, unrecognised);
    }
}
