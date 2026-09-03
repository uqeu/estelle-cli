//! 🔴 **THE SUITE MUST BE ABLE TO FINISH, AND "IT FINISHED" IS NOT SOMETHING A GREEN CAN TELL YOU.**
//!
//! libtest runs every test on a spawned worker thread, and a spawned thread gets a 2 MiB stack by
//! default — not the process main stack. The largest full-`App` fixture in this crate needs between
//! 5 and 6 MiB. Below that, `cargo test -p estelle-tui --lib` overflows and `SIGABRT`s the whole
//! binary partway through: **no failure block, no counts, and no `test result:` line**, while every
//! other target in the workspace prints `ok`. Measured on this crate at 3,260 declared tests, the
//! process died after ~136 of them.
//!
//! That failure is invisible in exactly the way this codebase cares about most. There is no red to
//! notice — there is an *absence*, and an absence reads as a pass to anyone scanning for `FAILED`.
//! It is why every recorded baseline for this crate said "154 tests" while 3,251 of them had never
//! run, and why eight genuine failures sat unseen behind the abort for a month.
//!
//! The bound is owned by `.cargo/config.toml`'s `[env]` block, so it applies on every path: a bare
//! `cargo test`, an editor's runner, and CI alike. This module asserts the bound **reached this
//! process**, which is the part that was actually broken — the same 8 MiB was already written down
//! twice (the Windows `link-arg=/STACK:8388608` stanzas, and `release.yml:88`) and still did not
//! hold for a developer running the suite locally.
//!
//! ⚠️ **Limit, stated plainly:** this asserts the *configured* budget, not the *achieved* one. It
//! cannot prove any particular test fits, and a future fixture that needs 9 MiB will overflow with
//! this test still green. What it does catch is the regression that actually happened — the setting
//! going missing from the path a developer runs — and it fails closed when it does.

/// The measured floor. Between 5 and 6 MiB is what the largest full-`App` fixture needs; 8 MiB is
/// the next conventional size up and matches what the Windows main thread is already linked with.
const REQUIRED_TEST_STACK_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::REQUIRED_TEST_STACK_BYTES;

    #[test]
    fn the_test_worker_stack_budget_reached_this_process() {
        let raw = std::env::var("RUST_MIN_STACK").unwrap_or_default();

        assert!(
            !raw.is_empty(),
            "RUST_MIN_STACK is unset in the test process, so libtest workers get the 2 MiB default \
             and this suite aborts partway with no `test result:` line — which reads as a pass. \
             The floor is owned by `.cargo/config.toml`'s [env] block; if you are running the \
             binary directly rather than through cargo, export RUST_MIN_STACK={REQUIRED_TEST_STACK_BYTES} \
             yourself."
        );

        let configured: usize = raw.parse().unwrap_or_else(|_| {
            panic!("RUST_MIN_STACK is set to {raw:?}, which is not a byte count")
        });

        assert!(
            configured >= REQUIRED_TEST_STACK_BYTES,
            "RUST_MIN_STACK is {configured} bytes but this crate's largest fixture needs \
             {REQUIRED_TEST_STACK_BYTES}. Below the floor the suite does not fail — it ABORTS, and \
             prints no counts at all."
        );
    }
}
