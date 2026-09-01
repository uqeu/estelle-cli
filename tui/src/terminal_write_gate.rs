//! Whether bytes may enter a PTY right now, and the fence that keeps a delayed submit honest.
//!
//! Ported from Orca's `src/main/runtime/runtime-terminal-writer.ts` and
//! `agent-session-pty-write-gate.ts` (MIT).
//!
//! ## The rule this module exists to encode
//!
//! 🔴 **SAFETY IS A WRITE GATE, NEVER A DISABLED INPUT.** The composer's enabled state derives
//! from TRANSPORT PRESENCE only, never from "the agent is working". Mid-turn the button may
//! change, but Enter still sends and Escape still interrupts, because a user who cannot type at
//! the moment they want to redirect the agent has lost the only control that matters. Orca is
//! explicit about this (`NativeChatComposer.tsx:154`, whose `canSend` resolves purely to "is a
//! mobile client holding this pty"), and refusing the WRITE rather than the KEYSTROKE is what
//! makes it safe to leave the input live.
//!
//! ## The fence
//!
//! A prompt injection is two writes with a pause between them: the body, then Enter. The pause is
//! [`crate::agent_prompt_injection::agent_prompt_submit_delay`], which is half a second at
//! minimum and grows with the payload. **That is long enough for the pane to be rebound
//! underneath us.** Admitting once, before the body, and then writing the submit on the strength
//! of that stale admission is how an Enter lands in someone else's session.
//!
//! So the admission is taken before the body and RE-ASSERTED against the same fence before the
//! suffix. [`PromptInjection`] makes that structural rather than remembered: there is no way to
//! obtain the submit bytes except through [`PromptInjection::finish`], which revalidates.

use std::collections::HashMap;
use std::time::Duration;

use crate::agent_prompt_injection::AGENT_PROMPT_SUBMIT;
use crate::agent_prompt_injection::INPUT_CHUNK_MAX_BYTES;
use crate::agent_prompt_injection::WriteHost;
use crate::agent_prompt_injection::agent_prompt_submit_delay;
use crate::agent_prompt_injection::build_agent_prompt_paste;
use crate::agent_prompt_injection::chunk_terminal_input;

/// Why a write was refused. Every variant is a normal outcome, not an error condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WriteRefused {
    /// The pane now belongs to a different session than the one that was admitted.
    Rebound,
    /// The pane is no longer bound to anything, so there is nothing to write into.
    Unbound,
}

/// What an admitted write carries forward so its later writes can be fenced against the same
/// lease.
///
/// The fence is what tells two overlapping claims apart when both name the same session: a rebind
/// to the SAME session is still a different lease, and a submit admitted under the old one has no
/// business landing in the new one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WriteAdmittance {
    session_id: Option<String>,
    fence: u64,
}

#[derive(Clone, Debug)]
struct Binding {
    session_id: String,
    fence: u64,
}

/// Maps a live PTY to the session that owns it, and answers whether bytes may enter it.
///
/// With nothing bound, [`TerminalWriteGate::admit`] short-circuits to admitted, so an ordinary
/// single-pane session pays nothing and behaves exactly as it did before the gate existed.
#[derive(Debug, Default)]
pub(crate) struct TerminalWriteGate {
    bindings: HashMap<String, Binding>,
    next_fence: u64,
}

impl TerminalWriteGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Bind a pane to a session, minting a fresh fence.
    ///
    /// Re-binding to the SAME session still mints a new fence, on purpose: the adoption is a new
    /// lease, and a write admitted under the previous one must not complete against it.
    pub(crate) fn bind(&mut self, pty_id: &str, session_id: &str) {
        self.next_fence += 1;
        self.bindings.insert(
            pty_id.to_string(),
            Binding {
                session_id: session_id.to_string(),
                fence: self.next_fence,
            },
        );
    }

    pub(crate) fn unbind(&mut self, pty_id: &str) {
        self.bindings.remove(pty_id);
    }

    /// Take an admission for a write about to start.
    pub(crate) fn admit(&self, pty_id: &str) -> Result<WriteAdmittance, WriteRefused> {
        match self.bindings.get(pty_id) {
            // Nothing bound: nothing to enforce, and nothing to change behaviour for.
            None => Ok(WriteAdmittance {
                session_id: None,
                fence: 0,
            }),
            Some(binding) => Ok(WriteAdmittance {
                session_id: Some(binding.session_id.clone()),
                fence: binding.fence,
            }),
        }
    }

    /// Re-assert an admission that was taken earlier, against the state as it is NOW.
    ///
    /// This is the whole fence. It is called before every write after the first, and in
    /// particular before the delayed submit.
    pub(crate) fn reassert(
        &self,
        pty_id: &str,
        admitted: &WriteAdmittance,
    ) -> Result<(), WriteRefused> {
        match (self.bindings.get(pty_id), &admitted.session_id) {
            // Was unbound and still is: still nothing to enforce.
            (None, None) => Ok(()),
            // It was bound when we started and is not any more.
            (None, Some(_)) => Err(WriteRefused::Unbound),
            // It was unbound when we started and something has since claimed it. The write was
            // admitted against a pane with no owner; it has one now, and it is not ours.
            (Some(_), None) => Err(WriteRefused::Rebound),
            (Some(binding), Some(session_id)) => {
                if &binding.session_id == session_id && binding.fence == admitted.fence {
                    Ok(())
                } else {
                    Err(WriteRefused::Rebound)
                }
            }
        }
    }
}

/// An in-flight prompt injection, holding its own admission.
///
/// The two writes cannot be issued out of order and the submit cannot be issued without
/// revalidating, because the only route to the submit bytes is [`PromptInjection::finish`].
#[derive(Debug)]
pub(crate) struct PromptInjection {
    pty_id: String,
    framed_body: String,
    admitted: WriteAdmittance,
    submit_delay: Duration,
}

impl PromptInjection {
    /// Admit the write and frame the body. Nothing has been written yet.
    pub(crate) fn begin(
        gate: &TerminalWriteGate,
        pty_id: &str,
        prompt: &str,
        host: WriteHost,
    ) -> Result<Self, WriteRefused> {
        // The lease is checked BEFORE any work, so a refused send never takes a claim it will not
        // use.
        let admitted = gate.admit(pty_id)?;
        let framed_body = build_agent_prompt_paste(prompt);
        let submit_delay = agent_prompt_submit_delay(host, framed_body.len());
        Ok(Self {
            pty_id: pty_id.to_string(),
            framed_body,
            admitted,
            submit_delay,
        })
    }

    /// The body writes, each within the transport's per-write cap and on a character boundary.
    pub(crate) fn body_chunks(&self) -> Vec<&str> {
        chunk_terminal_input(&self.framed_body, INPUT_CHUNK_MAX_BYTES)
    }

    /// How long the caller must wait between the last body chunk and [`PromptInjection::finish`].
    pub(crate) fn submit_delay(&self) -> Duration {
        self.submit_delay
    }

    /// Re-assert the admission and yield the submit bytes.
    ///
    /// Consumes `self`, so an injection cannot be submitted twice, and takes the gate again
    /// rather than trusting the copy it took at `begin`.
    pub(crate) fn finish(self, gate: &TerminalWriteGate) -> Result<&'static str, WriteRefused> {
        gate.reassert(&self.pty_id, &self.admitted)?;
        Ok(AGENT_PROMPT_SUBMIT)
    }

    /// Re-assert mid-body, between chunks. A large paste is many writes and a long wall-clock
    /// window of its own.
    pub(crate) fn reassert(&self, gate: &TerminalWriteGate) -> Result<(), WriteRefused> {
        gate.reassert(&self.pty_id, &self.admitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PTY: &str = "pty-1";

    fn bound_gate() -> TerminalWriteGate {
        let mut gate = TerminalWriteGate::new();
        gate.bind(PTY, "session-a");
        gate
    }

    // ------------------------------------------------------- the fence

    #[test]
    fn a_rebind_during_the_pause_refuses_the_submit() {
        // THE defect the fence exists for. The body is admitted and written; during the delay the
        // pane is handed to another session; the Enter must not land there.
        let mut gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("the first admission should succeed");
        };
        assert!(!injection.body_chunks().is_empty());

        // ...the pause is long enough for exactly this to happen.
        gate.bind(PTY, "session-b");

        assert_eq!(injection.finish(&gate), Err(WriteRefused::Rebound));
    }

    #[test]
    fn a_rebind_to_the_same_session_still_refuses_the_submit() {
        // A new adoption of the same session is a NEW LEASE. Comparing session ids alone passes
        // here, which is why the fence is a separate counter and not just an id check.
        let mut gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("admission");
        };
        gate.bind(PTY, "session-a");
        assert_eq!(injection.finish(&gate), Err(WriteRefused::Rebound));
    }

    #[test]
    fn an_unbind_during_the_pause_refuses_the_submit() {
        let mut gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("admission");
        };
        gate.unbind(PTY);
        assert_eq!(injection.finish(&gate), Err(WriteRefused::Unbound));
    }

    #[test]
    fn a_pane_claimed_during_the_pause_refuses_a_submit_admitted_while_unbound() {
        // The asymmetric case: admitted against a pane nobody owned, someone owns it now.
        let mut gate = TerminalWriteGate::new();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("an unbound pane admits");
        };
        gate.bind(PTY, "session-a");
        assert_eq!(injection.finish(&gate), Err(WriteRefused::Rebound));
    }

    #[test]
    fn an_undisturbed_injection_submits() {
        // The negative control. Every refusal above must be caused by the rebinding and not by
        // the fence refusing everything, which would pass all four tests and ship a dead feature.
        let gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("admission");
        };
        assert_eq!(injection.finish(&gate), Ok("\r"));
    }

    #[test]
    fn an_unbound_pane_admits_and_submits_unchanged() {
        // With nothing bound the gate must not change behaviour or cost for an ordinary session.
        let gate = TerminalWriteGate::new();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("admission");
        };
        assert_eq!(injection.finish(&gate), Ok("\r"));
    }

    #[test]
    fn the_mid_body_reassert_sees_a_rebind_too() {
        // A 16 MiB paste is a thousand writes and a wall-clock window of its own, so the body is
        // fenced between chunks as well as before the suffix.
        let mut gate = bound_gate();
        let Ok(injection) =
            PromptInjection::begin(&gate, PTY, &"x".repeat(100_000), WriteHost::Posix)
        else {
            panic!("admission");
        };
        assert!(injection.body_chunks().len() > 1, "the fixture is one write");
        assert_eq!(injection.reassert(&gate), Ok(()));
        gate.bind(PTY, "session-b");
        assert_eq!(injection.reassert(&gate), Err(WriteRefused::Rebound));
    }

    #[test]
    fn a_write_to_a_different_pane_does_not_disturb_this_one() {
        let mut gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "ship it", WriteHost::Posix) else {
            panic!("admission");
        };
        gate.bind("pty-2", "session-b");
        assert_eq!(injection.finish(&gate), Ok("\r"));
    }

    // --------------------------------------------------- body and timing

    #[test]
    fn the_body_is_framed_and_the_submit_is_not_part_of_it() {
        let gate = bound_gate();
        let Ok(injection) = PromptInjection::begin(&gate, PTY, "hello", WriteHost::Posix) else {
            panic!("admission");
        };
        let body = injection.body_chunks().concat();
        assert!(body.starts_with("\u{1b}[200~"));
        assert!(body.ends_with("\u{1b}[201~"));
        // If the submit rode along with the body there would be nothing for the fence to protect.
        assert!(!body.contains('\r'));
    }

    #[test]
    fn the_submit_delay_grows_with_the_prompt_and_with_the_host() {
        let gate = bound_gate();
        let delay = |prompt: &str, host| {
            let Ok(injection) = PromptInjection::begin(&gate, PTY, prompt, host) else {
                panic!("admission");
            };
            injection.submit_delay()
        };

        let small = delay("hi", WriteHost::Posix);
        let large = delay(&"x".repeat(4 * 1024 * 1024), WriteHost::Posix);
        assert!(large > small, "the delay did not grow with the payload");

        // ...and the slower transport waits longer for the same bytes.
        let payload = "x".repeat(320_000);
        assert!(
            delay(&payload, WriteHost::WindowsConpty) > delay(&payload, WriteHost::Posix),
            "the ConPTY host did not get the longer wait"
        );
    }
}
