//! Proof that the stack bound in the parent module is load-bearing, and the
//! fault injector that proves the suite-abort guard can fire.

use super::LIBTEST_DEFAULT_THREAD_STACK_BYTES;
use super::TEST_STACK_SIZE_BYTES;
use super::TEST_STACK_SIZE_OVERRIDE_ENV;
use std::process::Command;
use std::process::Output;

/// A real test that walks the whole session-startup chain. It is the subject of
/// the measurement in the parent module's docs, so it is the only honest probe
/// for "does the bound still matter".
const SESSION_STARTUP_CANARY_TEST: &str =
    "agent::control::residency::tests::interrupted_v2_agent_is_lost_after_residency_eviction";

/// Set on the child so a future edit cannot make the guard recurse into itself.
const GUARD_CHILD_ENV: &str = "CODEX_TEST_STACK_GUARD_CHILD";

/// Setting this makes [`suite_abort_injector_for_guard_proof`] abort the whole
/// process. It is how `scripts/run-cargo-test-guarded.py` is shown to be able to
/// fail against the real harness rather than only against a fake one.
pub(crate) const ABORT_INJECTOR_ENV: &str = "CODEX_SUITE_ABORT_INJECTOR";

fn run_canary_with_stack(stack_size_bytes: usize) -> Output {
    let exe = std::env::current_exe().expect("locate this test binary");
    Command::new(exe)
        .arg("--exact")
        .arg(SESSION_STARTUP_CANARY_TEST)
        .arg("--test-threads=1")
        .env(TEST_STACK_SIZE_OVERRIDE_ENV, stack_size_bytes.to_string())
        .env(GUARD_CHILD_ENV, "1")
        .env_remove(ABORT_INJECTOR_ENV)
        .output()
        .expect("re-execute this test binary")
}

/// The bound must fail in BOTH directions.
///
/// * Shrink the chain until libtest's default is enough and the first half fails,
///   which is the signal to delete the bound rather than let it rot into
///   decoration.
/// * Grow the chain past [`TEST_STACK_SIZE_BYTES`], or delete the sized thread,
///   and the second half fails.
///
/// Neither half asserts on an exit status alone: a process can die by signal for
/// reasons that have nothing to do with a stack, and a process can exit 0 having
/// run no tests at all. Both halves assert on the harness's own words.
#[test]
fn session_startup_stack_bound_is_load_bearing_and_sufficient() {
    if std::env::var_os(GUARD_CHILD_ENV).is_some() {
        // Already inside a child; do not fork further.
        return;
    }

    let at_libtest_default = run_canary_with_stack(LIBTEST_DEFAULT_THREAD_STACK_BYTES);
    let default_stderr = String::from_utf8_lossy(&at_libtest_default.stderr).into_owned();
    assert!(
        at_libtest_default.status.code().is_none(),
        "the bound has stopped being load-bearing: {SESSION_STARTUP_CANARY_TEST} survived \
         libtest's default {LIBTEST_DEFAULT_THREAD_STACK_BYTES}-byte thread stack \
         (exit {:?}). Re-measure the chain and delete TEST_STACK_SIZE_BYTES if it is no \
         longer needed — do not leave a bound nobody can fail.\n--- child stderr ---\n{default_stderr}",
        at_libtest_default.status.code(),
    );
    assert!(
        default_stderr.contains("has overflowed its stack"),
        "the child died without overflowing its stack, so this guard is measuring the \
         wrong fault.\n--- child stderr ---\n{default_stderr}"
    );

    let at_bound = run_canary_with_stack(TEST_STACK_SIZE_BYTES);
    let bound_stdout = String::from_utf8_lossy(&at_bound.stdout).into_owned();
    let bound_stderr = String::from_utf8_lossy(&at_bound.stderr).into_owned();
    assert!(
        at_bound.status.success(),
        "{SESSION_STARTUP_CANARY_TEST} does not fit in TEST_STACK_SIZE_BYTES \
         ({TEST_STACK_SIZE_BYTES} bytes).\n--- child stdout ---\n{bound_stdout}\
         \n--- child stderr ---\n{bound_stderr}"
    );
    assert!(
        bound_stdout.contains("1 passed"),
        "the child exited 0 without running the canary, so this half proves nothing.\
         \n--- child stdout ---\n{bound_stdout}"
    );
}

/// Not a test: a fault injector, inert unless [`ABORT_INJECTOR_ENV`] is set.
///
/// A suite that can die mid-run needs a guard that notices, and a guard nobody
/// has ever seen go red is a claim. This is how that claim gets checked against
/// the real harness on demand:
///
/// ```text
/// CODEX_SUITE_ABORT_INJECTOR=1 scripts/run-cargo-test-guarded.py -p codex-core --lib
/// ```
#[test]
#[expect(
    clippy::print_stderr,
    reason = "an unexplained SIGABRT is exactly the unreadable failure this lane exists to \
              remove; the injector must say why it fired, and tracing is not installed here"
)]
fn suite_abort_injector_for_guard_proof() {
    if std::env::var_os(ABORT_INJECTOR_ENV).is_none() {
        return;
    }
    eprintln!(
        "suite_abort_injector_for_guard_proof: aborting on request from {ABORT_INJECTOR_ENV}"
    );
    std::process::abort();
}
