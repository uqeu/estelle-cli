//! The bounds on how much scrollback we keep, replay, and buffer.
//!
//! Ported from Orca's `src/shared/terminal-scrollback-limits.ts` and
//! `src/shared/terminal-scrollback-policy.ts` (MIT).
//!
//! **The store limit and the replay limit are different numbers on purpose, and collapsing them
//! is the mistake this module exists to prevent.** What is worth KEEPING on disk against a future
//! scroll-up is an order of magnitude more than what is worth PUSHING through a terminal on
//! restore: replaying 5 MiB costs the user a visibly frozen pane, and storing only 512 KiB throws
//! away history they will ask for. One constant for both would have to be wrong in one direction.

/// Everything we are willing to keep on disk for one pane.
pub(crate) const SCROLLBACK_STORE_BYTE_LIMIT: usize = 5 * 1024 * 1024;

/// How much of that we replay into a terminal on restore. Deliberately 10x smaller.
pub(crate) const SCROLLBACK_REPLAY_BYTE_LIMIT: usize = 512 * 1024;

/// How much output we hold in memory for one pane while a starved display catches up.
pub(crate) const SCROLLBACK_SESSION_BUFFER_BYTE_LIMIT: usize = 512 * 1024;

pub(crate) const SCROLLBACK_ROWS_DEFAULT: usize = 5_000;
pub(crate) const SCROLLBACK_ROWS_MIN: usize = 1_000;
pub(crate) const SCROLLBACK_ROWS_MAX: usize = 50_000;

/// The floor under the pending-output backlog, whatever the user's scrollback setting is.
pub(crate) const OUTPUT_BACKLOG_MIN_CAP_CHARS: usize = 2 * 1024 * 1024;

/// About 80 columns of text plus escape-sequence overhead. This makes the cap a MEMORY BOUND, not
/// an exact retention guarantee, and the difference is worth saying out loud: a pane emitting
/// long lines will hit the cap with fewer rows retained than the setting names.
const OUTPUT_BACKLOG_CHARS_PER_ROW: usize = 120;

/// Clamp a configured scrollback row count into the supported range.
pub(crate) fn normalize_scrollback_rows(rows: Option<usize>) -> usize {
    match rows {
        None => SCROLLBACK_ROWS_DEFAULT,
        Some(rows) => rows.clamp(SCROLLBACK_ROWS_MIN, SCROLLBACK_ROWS_MAX),
    }
}

/// The pending-output backlog cap, DERIVED from the user's own scrollback setting.
///
/// A flat constant here is a promise the product does not keep. Backlog caps exist to bound
/// memory while a starved display catches up, but a user who deliberately raised scrollback to
/// 50k rows can retain more history than a flat 2 MiB floor allows, so dropping at that floor
/// discards lines their setting said they would keep. The floor stays as a floor; above it the
/// cap tracks the setting.
pub(crate) fn output_backlog_cap_chars(rows: Option<usize>) -> usize {
    let rows = normalize_scrollback_rows(rows);
    OUTPUT_BACKLOG_MIN_CAP_CHARS.max(rows * OUTPUT_BACKLOG_CHARS_PER_ROW)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_keeps_an_order_of_magnitude_more_than_the_replay_pushes() {
        // If these two are ever equalised, one of the two failures they prevent has been
        // reintroduced. Asserting the RELATIONSHIP outlasts asserting either number.
        assert!(SCROLLBACK_STORE_BYTE_LIMIT > SCROLLBACK_REPLAY_BYTE_LIMIT);
        assert_eq!(
            SCROLLBACK_STORE_BYTE_LIMIT / SCROLLBACK_REPLAY_BYTE_LIMIT,
            10
        );
        assert_eq!(SCROLLBACK_STORE_BYTE_LIMIT, 5 * 1024 * 1024);
        assert_eq!(SCROLLBACK_REPLAY_BYTE_LIMIT, 512 * 1024);
    }

    #[test]
    fn the_backlog_cap_follows_the_users_setting_above_the_floor() {
        // The defect a flat constant has: at 50k rows the user asked to keep roughly 6 MiB of
        // text, and a 2 MiB cap silently throws two thirds of it away.
        let at_max = output_backlog_cap_chars(Some(SCROLLBACK_ROWS_MAX));
        assert_eq!(at_max, 50_000 * 120);
        assert!(
            at_max > OUTPUT_BACKLOG_MIN_CAP_CHARS,
            "a 50k-row setting is capped at the flat floor, which is the bug"
        );

        // ...and it is strictly monotonic in the setting once past the floor.
        assert!(
            output_backlog_cap_chars(Some(50_000)) > output_backlog_cap_chars(Some(25_000)),
            "raising scrollback did not raise the backlog cap"
        );
    }

    #[test]
    fn the_floor_still_holds_for_small_settings() {
        // Below the crossover the floor wins, so a 1k-row user is not given a 120 KiB buffer.
        for rows in [SCROLLBACK_ROWS_MIN, 5_000, 10_000] {
            assert_eq!(output_backlog_cap_chars(Some(rows)), OUTPUT_BACKLOG_MIN_CAP_CHARS);
        }
        // The crossover is where 120 chars/row overtakes 2 MiB: 2_097_152 / 120 = 17_476.3, so
        // 17_476 rows is the last row count the floor still covers.
        assert_eq!(
            output_backlog_cap_chars(Some(17_476)),
            OUTPUT_BACKLOG_MIN_CAP_CHARS
        );
        assert!(output_backlog_cap_chars(Some(17_477)) > OUTPUT_BACKLOG_MIN_CAP_CHARS);
    }

    #[test]
    fn out_of_range_settings_are_clamped_and_absent_means_default() {
        assert_eq!(normalize_scrollback_rows(None), SCROLLBACK_ROWS_DEFAULT);
        assert_eq!(normalize_scrollback_rows(Some(0)), SCROLLBACK_ROWS_MIN);
        assert_eq!(normalize_scrollback_rows(Some(1)), SCROLLBACK_ROWS_MIN);
        assert_eq!(
            normalize_scrollback_rows(Some(usize::MAX)),
            SCROLLBACK_ROWS_MAX
        );
        assert_eq!(normalize_scrollback_rows(Some(7_500)), 7_500);
        // Absent is the DEFAULT, not the minimum: an unset setting is not a request for less.
        assert_ne!(
            normalize_scrollback_rows(None),
            normalize_scrollback_rows(Some(0))
        );
    }
}
