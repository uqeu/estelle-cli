//! One owner for the OS-thread stack bound that Codex tests need.
//!
//! `#[cfg(test)]`, so it is the owner for this crate's *unit* tests only. The
//! workspace-wide owner of the same bound is `RUST_MIN_STACK` in
//! `.cargo/config.toml`, which also covers Tokio worker threads and every other
//! crate. Integration tests under `core/tests/suite/` are a separate crate and
//! still declare their own copy; see the receipt's limits section.
//!
//! # Why the bound exists — measured, not assumed
//!
//! `ThreadManager::start_thread` (`core/src/thread_manager.rs:774`)
//! -> `ThreadManagerState::spawn_thread_with_source` (`core/src/thread_manager.rs:1540`)
//! -> `Session::spawn` (`core/src/session/mod.rs:491`)
//! -> `Session::spawn_internal` (`core/src/session/mod.rs:515`)
//! -> `Session::new` (`core/src/session/mod.rs:719`)
//! -> `Session::schedule_startup_prewarm` (`core/src/session_startup_prewarm.rs:185`)
//!
//! is a chain of large `async` state machines. Every future `.await`ed without an
//! intervening `Box::pin` is stored inline in its parent's state machine, so the
//! parent's *stack frame* grows by the whole child. On aarch64 the compiler emits a
//! page-probe loop for these frames, e.g. `sub x9, sp, #0x1b, lsl #12` — 110,592
//! bytes for one frame at `session_startup_prewarm.rs:185`.
//!
//! Measured 2026-09-01, aarch64-apple-darwin, `test` profile, rustc 1.95.0, from the
//! SIGABRT crash report of
//! `agent::control::residency::tests::interrupted_v2_agent_is_lost_after_residency_eviction`
//! (frame sizes read out of each function's prologue):
//!
//! | frame | bytes   | function                                                    |
//! |-------|---------|-------------------------------------------------------------|
//! | #22   | 545,504 | the test's own `async` body                                  |
//! | #9    | 283,056 | `Session::new` inner async block                             |
//! | #11   | 255,488 | `Session::new` outer async block                             |
//! | #13   | 162,448 | `Session::spawn_internal`                                    |
//! | #10   | 156,576 | `Session::new` middle async block                            |
//! | #15   | 143,776 | `Session::spawn`                                             |
//! | #44   | 124,176 | `Runtime::block_on` frame of the test fn                     |
//! | #7    | 112,736 | `Session::schedule_startup_prewarm`                          |
//! | #17   | 112,448 | `ThreadManagerState::spawn_thread_with_source`               |
//! | #21   |  49,008 | `ThreadManager::start_thread`                                |
//! | #19   |  43,328 | `ThreadManager::start_thread_inner`                          |
//!
//! **54 frames, 2,106,480 bytes total, and there is no recursion on this path.**
//! The backtrace is 54 frames deep, not thousands, and every frame is a distinct
//! function; the depth is a compile-time property of the call graph. That is why a
//! larger stack is a *fix* here and not a way of moving a cliff: an unbounded
//! recursion would swallow any bound we chose, and this one does not.
//!
//! Measured requirement, by bisecting `RUST_MIN_STACK` against the real binary:
//! the chain **overflows at 2,112 KiB and passes at 2,128 KiB**. libtest hands a
//! spawned test thread exactly [`LIBTEST_DEFAULT_THREAD_STACK_BYTES`], so the
//! chain misses the default by roughly 64 KiB and aborts the whole process.
//!
//! Its sibling `residency_slot_reservation_unloads_oldest_idle_v2_agent` passes at
//! 2,032 KiB — **16 KiB of headroom on a 2,048 KiB stack, 0.78%.** It is not
//! healthy, it is one struct field away from the same abort, which is why both
//! tests take this bound rather than only the one that is red today.

use std::future::Future;

/// The stack libtest gives a spawned test thread when `RUST_MIN_STACK` is unset.
///
/// This is the number the session-startup chain misses. It is written here so a
/// guard can assert against it by name instead of a literal buried in a test.
pub(crate) const LIBTEST_DEFAULT_THREAD_STACK_BYTES: usize = 2 * 1024 * 1024;

/// The stack budget a Codex test thread gets.
///
/// Kept equal to the `RUST_MIN_STACK` in `.cargo/config.toml`, which is the
/// workspace-wide owner of this bound; this constant covers the case where a test
/// binary is run directly rather than through `cargo test`, and it is the only
/// knob the load-bearing guard below can turn.
///
/// 8 MiB, not more. It is the number `release.yml:88` and nine ad-hoc
/// declarations in this tree already use, and it leaves roughly 4x headroom over
/// the measured 2,128 KiB requirement. 16 MiB was measured on the same machine
/// and cost 56 seconds of suite wall-clock (49.6s -> 105.6s) and one timeout in a
/// websocket test that passes in isolation. Stack past the measurement is a cost,
/// not a margin.
pub(crate) const TEST_STACK_SIZE_BYTES: usize = 8 * 1024 * 1024;

/// Overrides [`TEST_STACK_SIZE_BYTES`] for one process.
///
/// This exists for exactly one caller: the guard in this module's `tests`, which
/// re-executes the test binary at [`LIBTEST_DEFAULT_THREAD_STACK_BYTES`] to prove
/// the bound is load-bearing. Nothing in a normal run reads it.
pub(crate) const TEST_STACK_SIZE_OVERRIDE_ENV: &str = "CODEX_TEST_STACK_SIZE_BYTES";

/// Upper sanity limit. A request past this is a typo (a shifted constant, a
/// units mix-up), not a stack budget any test needs.
const MAX_PLAUSIBLE_TEST_STACK_BYTES: usize = 512 * 1024 * 1024;

/// Resolve the stack budget for this process.
///
/// A malformed or out-of-range override is a hard error rather than a silent
/// fallback: an override that quietly does nothing would make the load-bearing
/// guard pass for the wrong reason.
pub(crate) fn resolve_test_stack_size_bytes() -> usize {
    let Ok(raw) = std::env::var(TEST_STACK_SIZE_OVERRIDE_ENV) else {
        return TEST_STACK_SIZE_BYTES;
    };
    let parsed: usize = raw.parse().unwrap_or_else(|err| {
        panic!("{TEST_STACK_SIZE_OVERRIDE_ENV}={raw:?} is not a byte count: {err}")
    });
    assert!(
        parsed > 0 && parsed <= MAX_PLAUSIBLE_TEST_STACK_BYTES,
        "{TEST_STACK_SIZE_OVERRIDE_ENV}={parsed} is outside 1..={MAX_PLAUSIBLE_TEST_STACK_BYTES}"
    );
    parsed
}

/// Run an async test body on an OS thread sized by [`TEST_STACK_SIZE_BYTES`].
///
/// `#[tokio::test]` builds a current-thread runtime and calls `block_on` on the
/// thread libtest handed it, so the whole future graph lives on libtest's 2 MiB
/// stack. Any test that walks the session-startup chain must not do that. This is
/// the same shape `arg0/src/lib.rs:228` uses in production for the same reason.
///
/// A panic inside `body` is re-raised on the caller's thread, so libtest still
/// reports it as an ordinary test failure with the original payload.
pub(crate) fn block_on_with_test_stack<F, Fut, T>(test_name: &str, body: F) -> T
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T>,
    T: Send + 'static,
{
    assert!(
        !test_name.is_empty(),
        "a sized test thread must be named so a crash report can identify it"
    );
    let stack_size_bytes = resolve_test_stack_size_bytes();
    assert!(
        stack_size_bytes > 0 && stack_size_bytes <= MAX_PLAUSIBLE_TEST_STACK_BYTES,
        "resolved test stack {stack_size_bytes} is outside 1..={MAX_PLAUSIBLE_TEST_STACK_BYTES}"
    );

    let handle = std::thread::Builder::new()
        .name(test_name.to_owned())
        .stack_size(stack_size_bytes)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build a current-thread runtime on the sized test thread");
            runtime.block_on(body())
        })
        .expect("spawn the sized test thread");

    match handle.join() {
        Ok(value) => value,
        Err(panic_payload) => std::panic::resume_unwind(panic_payload),
    }
}

#[cfg(test)]
#[path = "test_stack_bound_tests.rs"]
mod tests;
