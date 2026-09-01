//! Framing and timing for text Estelle injects into a child process that has no settlement
//! signal of its own.
//!
//! Ported from Orca's `src/shared/agent-prompt-injection.ts` (MIT).
//!
//! Two independent hazards, and the module exists because both are silent when you get them
//! wrong:
//!
//! 1. **Enter overtaking the body.** The child has to have INGESTED the paste before a submit can
//!    mean "send what I just pasted". There is no acknowledgement to wait on, so the wait is
//!    open-loop: a fixed settle window plus a term proportional to the payload. See
//!    [`agent_prompt_submit_delay`], and read the measured table on
//!    [`WriteHost::ingest_bytes_per_ms`] before touching either number.
//!
//! 2. **Pasted content driving the TUI.** Anything with an `ESC` in it is a command sequence to
//!    whatever is reading the PTY. The body is wrapped in bracketed paste so the child treats it
//!    as data, and every `ESC` inside is replaced with an inert literal first, because bracketed
//!    paste is a request the child is free to ignore and an unsanitised `ESC` inside the brackets
//!    still reaches a child that never enabled the mode.
//!
//! Nothing here disables an input or blocks on agent state. Safety lives in the write gate
//! (`crate::terminal_write_gate`), which fences the delayed suffix separately, so that "the agent
//! is busy" never turns into "the user cannot type".

use std::time::Duration;

/// Opens a bracketed paste. Everything until the closer is data, not keys.
pub(crate) const BRACKETED_PASTE_START: &str = "\u{1b}[200~";
/// Closes a bracketed paste.
pub(crate) const BRACKETED_PASTE_END: &str = "\u{1b}[201~";
/// The submit that follows the body, after the delay.
pub(crate) const AGENT_PROMPT_SUBMIT: &str = "\r";

/// What an `ESC` in pasted content becomes: visible, inert, and impossible to mistake for the
/// byte it replaces.
const INERT_ESCAPE: &str = "<ESC>";
const ESCAPE: char = '\u{1b}';

/// The child still has to ATTACH a completed paste before Enter counts, and that is not ingest.
///
/// It also absorbs the fixed 15-25 ms intercept both measured ConPTY hosts show below their
/// linear term.
const SUBMIT_SETTLE: Duration = Duration::from_millis(500);

/// Largest single write handed to the PTY. Bounded before the write, not discovered during it.
pub(crate) const INPUT_CHUNK_MAX_BYTES: usize = 16 * 1024;

/// Which transport is going to ingest the bytes.
///
/// This follows the host that owns the PTY, NOT the OS the command runs under: a macOS client
/// attached to a Windows runtime pays the Windows rate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum WriteHost {
    /// Anything that is not a Windows ConPTY.
    #[default]
    Posix,
    /// Windows ConPTY, which ingests pasted input linearly and is roughly 64x slower.
    WindowsConpty,
}

impl WriteHost {
    /// Bytes the host can ingest per millisecond.
    ///
    /// Windows ConPTY ingests pasted input linearly, and the cost is ingest rather than
    /// rendering: the child repaints in about 0 ms on both platforms. Two real Win11 hosts,
    /// bundled ConPTY DLL, 16 KiB chunks:
    ///
    /// ```text
    ///   bytes     host A    host B
    ///   2,000      14 ms     25 ms
    ///   8,000      60 ms     89 ms
    ///   40,000    347 ms    440 ms
    ///   160,000  1662 ms   1499 ms
    ///   320,000  3342 ms   2969 ms
    /// ```
    ///
    /// Slopes: 0.0104 ms/byte (A) and 0.0092 ms/byte (B), about 40% host-to-host spread in both
    /// directions. 64 B/ms is 1.5x the slower of the two, so neither host, nor a meaningfully
    /// slower one, can still be ingesting when the wait ends.
    ///
    /// The same walk on macOS drains 320 KB in 26 ms (about 12.3 KB/ms), but at those magnitudes
    /// the samples are noise-dominated (80 KB measured faster than 40 KB), so 4,096 B/ms holds a
    /// 3x margin. It costs 0 ms at real prompt sizes.
    const fn ingest_bytes_per_ms(self) -> u64 {
        match self {
            WriteHost::WindowsConpty => 64,
            WriteHost::Posix => 4_096,
        }
    }
}

/// Lower bound on when a paste of `byte_len` can have reached the child.
pub(crate) fn paste_ingest_delay(host: WriteHost, byte_len: usize) -> Duration {
    if byte_len == 0 {
        return Duration::ZERO;
    }
    let rate = host.ingest_bytes_per_ms();
    // Round up: a partial millisecond of ingest is still a millisecond the child needs.
    Duration::from_millis((byte_len as u64).div_ceil(rate))
}

/// How long to wait between the body and the Enter that submits it.
///
/// 🔴 **NEVER CAPPED, AND THAT IS THE POINT.** A ceiling here silently reintroduces exactly the
/// mid-paste Enter the delay exists to prevent: the payloads that would hit a cap are the ones
/// that need the wait most, so a cap is a guarantee that fails precisely where it matters. If
/// this ever grows a `min(...)`, the test named `the_submit_delay_is_never_capped` is the one
/// that argues with you.
pub(crate) fn agent_prompt_submit_delay(host: WriteHost, byte_len: usize) -> Duration {
    SUBMIT_SETTLE + paste_ingest_delay(host, byte_len)
}

/// Replace every `ESC` with an inert literal so pasted content cannot drive the child's TUI.
///
/// Returns a borrowed string when there is nothing to do, which is the overwhelmingly common
/// case and keeps this off the allocation path for ordinary prompts.
pub(crate) fn sanitize_agent_prompt_text(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains(ESCAPE) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(text.replace(ESCAPE, INERT_ESCAPE))
}

/// The body write: a sanitised prompt inside bracketed paste.
///
/// The brackets are the only `ESC` bytes in the result, by construction.
pub(crate) fn build_agent_prompt_paste(prompt: &str) -> String {
    let sanitized = sanitize_agent_prompt_text(prompt);
    let mut out =
        String::with_capacity(BRACKETED_PASTE_START.len() + sanitized.len() + BRACKETED_PASTE_END.len());
    out.push_str(BRACKETED_PASTE_START);
    out.push_str(&sanitized);
    out.push_str(BRACKETED_PASTE_END);
    out
}

/// Split `text` into writes of at most `max_chunk_bytes`, never splitting a character.
///
/// The cap is taken BEFORE the write rather than discovered inside it, and a chunk boundary that
/// landed mid-character would put a replacement glyph into the child's buffer that no later write
/// can remove.
pub(crate) fn chunk_terminal_input(text: &str, max_chunk_bytes: usize) -> Vec<&str> {
    assert!(
        max_chunk_bytes >= 4,
        "a chunk cap below 4 bytes cannot hold one character, so no split can make progress"
    );
    if text.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    // Bounded: `start` strictly increases by at least one byte per iteration.
    while start < text.len() {
        let mut end = (start + max_chunk_bytes).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        debug_assert!(end > start, "a char boundary walk consumed the whole chunk");
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------- the delay

    #[test]
    fn the_submit_delay_is_never_capped() {
        // The measured ConPTY table, converted to the delay this function must produce. A cap
        // anywhere would flatten the tail of this list, so the tail is what is asserted.
        let ladder = [2_000usize, 8_000, 40_000, 160_000, 320_000, 16 * 1024 * 1024];
        let delays: Vec<Duration> = ladder
            .iter()
            .map(|bytes| agent_prompt_submit_delay(WriteHost::WindowsConpty, *bytes))
            .collect();

        for pair in delays.windows(2) {
            assert!(
                pair[1] > pair[0],
                "the delay stopped growing with the payload: {pair:?}"
            );
        }

        // Proportional, not merely increasing: doubling the payload doubles the ingest term.
        // A `min(...)` cap passes strict monotonicity right up until it bites; this does not.
        for bytes in [8_000usize, 320_000, 4 * 1024 * 1024] {
            let single = agent_prompt_submit_delay(WriteHost::WindowsConpty, bytes) - SUBMIT_SETTLE;
            let double =
                agent_prompt_submit_delay(WriteHost::WindowsConpty, bytes * 2) - SUBMIT_SETTLE;
            assert_eq!(
                double,
                single * 2,
                "ingest is not proportional at {bytes} bytes"
            );
        }

        // And the absolute value at the ceiling, so a rate change cannot pass silently.
        assert_eq!(
            agent_prompt_submit_delay(WriteHost::WindowsConpty, 320_000),
            Duration::from_millis(500 + 5_000)
        );
        assert_eq!(
            agent_prompt_submit_delay(WriteHost::Posix, 16 * 1024 * 1024),
            Duration::from_millis(500 + 4_096)
        );
    }

    #[test]
    fn the_delay_covers_the_measured_conpty_table_with_margin() {
        // The whole justification for 64 B/ms is that the wait outlasts the slowest measurement.
        // If it does not, the constant is wrong, and this is where that shows.
        let measured = [
            (2_000usize, 25u64),
            (8_000, 89),
            (40_000, 440),
            (160_000, 1_662),
            (320_000, 3_342),
        ];
        for (bytes, slowest_observed_ms) in measured {
            let delay = agent_prompt_submit_delay(WriteHost::WindowsConpty, bytes);
            assert!(
                delay > Duration::from_millis(slowest_observed_ms),
                "{bytes} bytes: waited {delay:?} but a real host took {slowest_observed_ms} ms"
            );
        }
    }

    #[test]
    fn an_empty_body_still_settles_but_costs_no_ingest() {
        assert_eq!(paste_ingest_delay(WriteHost::WindowsConpty, 0), Duration::ZERO);
        assert_eq!(
            agent_prompt_submit_delay(WriteHost::Posix, 0),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn a_partial_millisecond_of_ingest_still_costs_a_millisecond() {
        // Rounding down here would let Enter go out while the last bytes are in flight.
        assert_eq!(paste_ingest_delay(WriteHost::Posix, 1), Duration::from_millis(1));
        assert_eq!(
            paste_ingest_delay(WriteHost::Posix, 4_097),
            Duration::from_millis(2)
        );
    }

    #[test]
    fn the_windows_host_is_never_faster_and_is_strictly_slower_once_rounding_stops_hiding_it() {
        // ⚠️ The obvious claim here ("ConPTY waits longer at EVERY size") is FALSE, and asserting
        // it is how this test failed on its first run: at 1 byte both hosts round up to the same
        // whole millisecond. The true invariant is that ConPTY is never given the SHORTER wait.
        for bytes in [1usize, 64, 2_000, 4_096, 320_000, 16 * 1024 * 1024] {
            assert!(
                paste_ingest_delay(WriteHost::WindowsConpty, bytes)
                    >= paste_ingest_delay(WriteHost::Posix, bytes),
                "the ConPTY host was given the SHORTER wait at {bytes} bytes"
            );
        }
        // Above one Posix millisecond of payload the 64x rate gap is strict, so a swap of the two
        // rates still fails here rather than hiding behind the rounding.
        for bytes in [4_097usize, 8_000, 320_000, 16 * 1024 * 1024] {
            assert!(
                paste_ingest_delay(WriteHost::WindowsConpty, bytes)
                    > paste_ingest_delay(WriteHost::Posix, bytes),
                "the host rates are the wrong way round at {bytes} bytes"
            );
        }
    }

    // ------------------------------------------------------- the framing

    #[test]
    fn an_escape_in_pasted_content_cannot_reach_the_child() {
        // The attack this exists for: pasted text that ends the bracket itself and then issues
        // its own sequences.
        let hostile = "hello\u{1b}[201~\u{1b}[2J\u{1b}[1;1Hrm -rf /\r";
        let framed = build_agent_prompt_paste(hostile);

        // Exactly two ESC bytes survive, and they are the two we wrote.
        assert_eq!(framed.matches(ESCAPE).count(), 2);
        assert!(framed.starts_with(BRACKETED_PASTE_START));
        assert!(framed.ends_with(BRACKETED_PASTE_END));

        // The body between the brackets has no ESC at all.
        let Some(body) = framed
            .strip_prefix(BRACKETED_PASTE_START)
            .and_then(|rest| rest.strip_suffix(BRACKETED_PASTE_END))
        else {
            panic!("the frame is the two constants and a body, got {framed:?}");
        };
        assert!(!body.contains(ESCAPE));
        assert!(body.contains("<ESC>[201~"));

        // The payload's own text survives, inert. Sanitising must not delete content.
        assert!(body.contains("rm -rf /"));
        assert!(body.starts_with("hello"));
    }

    #[test]
    fn ordinary_text_is_framed_without_being_rewritten() {
        let framed = build_agent_prompt_paste("fix the flaky test in api.py");
        assert_eq!(
            framed,
            format!("{BRACKETED_PASTE_START}fix the flaky test in api.py{BRACKETED_PASTE_END}")
        );
        // ...and no allocation was needed to decide that.
        assert!(matches!(
            sanitize_agent_prompt_text("fix the flaky test in api.py"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn the_submit_is_separate_from_the_body() {
        // The body must not carry its own newline, or the fence in the write gate has nothing
        // left to protect: the submit would already have gone out with the text.
        let framed = build_agent_prompt_paste("two\nlines");
        assert!(!framed.ends_with(AGENT_PROMPT_SUBMIT));
        // A newline INSIDE the paste is content, and bracketed paste is what keeps it content.
        assert!(framed.contains("two\nlines"));
    }

    // ------------------------------------------------------- the chunking

    #[test]
    fn chunking_never_splits_a_character() {
        // Four-byte characters against a cap that is deliberately not a multiple of four: a
        // byte-slicing implementation panics or produces invalid UTF-8 here.
        let text = "\u{1F600}".repeat(64);
        let chunks = chunk_terminal_input(&text, 7);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 7);
            assert_eq!(chunk.len() % 4, 0, "a chunk boundary landed mid-character");
        }
        assert_eq!(chunks.concat(), text, "chunking lost or reordered content");
    }

    #[test]
    fn chunking_is_lossless_across_mixed_widths() {
        let text = "a\u{e9}\u{4e2d}\u{1F600}".repeat(500);
        for cap in [4usize, 5, 16, 1024, INPUT_CHUNK_MAX_BYTES] {
            let chunks = chunk_terminal_input(&text, cap);
            assert_eq!(chunks.concat(), text, "lossy at cap {cap}");
            assert!(chunks.iter().all(|c| c.len() <= cap && !c.is_empty()));
        }
    }

    #[test]
    fn an_empty_body_produces_no_writes() {
        assert!(chunk_terminal_input("", INPUT_CHUNK_MAX_BYTES).is_empty());
    }

    #[test]
    fn text_under_the_cap_is_one_write() {
        assert_eq!(
            chunk_terminal_input("short", INPUT_CHUNK_MAX_BYTES),
            vec!["short"]
        );
    }
}
