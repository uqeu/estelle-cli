//! Ported from the rival-CLI teardown, take-list #2 and #6.
//!
//! opencode (`packages/core/src/permission.ts:147-160`, MIT,
//! Copyright (c) 2025 opencode) closes the always-allow escalation path two
//! ways: the configured ruleset is evaluated ALONE first and a `deny` there is
//! final, and across a multi-resource request the most restrictive effect wins:
//!
//! ```ts
//! if (denied(input, rules)) return { effect: "deny" as const, rules }
//! const all = [...rules, ...(yield* savedRules())]
//! ...
//! const effect = effects.includes("deny") ? "deny" : effects.includes("ask") ? "ask" : "allow"
//! ```
//!
//! We get the same guarantee from a different mechanism: `Decision` is declared
//! least-to-most restrictive and `Evaluation::from_matches` takes the `max` of
//! every matched rule. That is correct TODAY and it was, until this file, an
//! UNASSERTED accident of declaration order -- reordering two enum variants (a
//! change no reviewer would flag) would silently turn every configured deny
//! into an allow. These tests exist to make that reorder go red.

#![allow(clippy::expect_used)]

use codex_execpolicy::Decision;
use codex_execpolicy::Policy;
use pretty_assertions::assert_eq;

fn tokens(cmd: &[&str]) -> Vec<String> {
    cmd.iter().map(std::string::ToString::to_string).collect()
}

/// `Decision` is ordered LEAST restrictive to MOST restrictive, and
/// `Evaluation::from_matches` relies on that ordering via `max()`. Pin it.
#[test]
fn decision_ordering_is_least_to_most_restrictive() {
    assert!(Decision::Allow < Decision::Prompt);
    assert!(Decision::Prompt < Decision::Forbidden);
    assert_eq!(
        Decision::Forbidden,
        Decision::Allow.max(Decision::Forbidden),
        "`max` must mean MORE RESTRICTIVE; if this fails, every `deny` in the \
         tree has silently become an `allow`"
    );
}

/// The adversarial case: a user clicks "always allow" for `curl` (which appends
/// an allow prefix rule -- see `ExecPolicyManager::append_amendment_and_update`,
/// `core/src/exec_policy.rs`), while an operator has explicitly denied `curl`.
/// The deny must win, in both insertion orders.
#[test]
fn a_saved_always_allow_cannot_override_a_configured_deny() {
    for saved_first in [true, false] {
        let mut policy = Policy::empty();
        let cmd = tokens(&["curl", "https://example.com"]);
        if saved_first {
            policy
                .add_prefix_rule(&tokens(&["curl"]), Decision::Allow)
                .expect("saved always-allow");
            policy
                .add_prefix_rule(&tokens(&["curl"]), Decision::Forbidden)
                .expect("configured deny");
        } else {
            policy
                .add_prefix_rule(&tokens(&["curl"]), Decision::Forbidden)
                .expect("configured deny");
            policy
                .add_prefix_rule(&tokens(&["curl"]), Decision::Allow)
                .expect("saved always-allow");
        }

        let evaluation = policy.check_multiple([cmd].iter(), &|_| Decision::Allow);
        assert_eq!(
            Decision::Forbidden,
            evaluation.decision,
            "a saved always-allow overrode a configured deny \
             (saved rule inserted {})",
            if saved_first { "first" } else { "second" }
        );
    }
}

/// The exemption shape, stated as a test: the rule is "deny wins", NOT "the
/// policy always refuses". With no deny present, a saved always-allow is still
/// honoured -- otherwise the guarantee above would be indistinguishable from a
/// policy that simply denies everything, and would pass on a broken tree.
#[test]
fn a_saved_always_allow_still_works_when_nothing_denies_it() {
    let mut policy = Policy::empty();
    policy
        .add_prefix_rule(&tokens(&["curl"]), Decision::Allow)
        .expect("saved always-allow");

    let evaluation = policy
        .check_multiple([tokens(&["curl", "https://example.com"])].iter(), &|_| {
            Decision::Prompt
        });
    assert_eq!(Decision::Allow, evaluation.decision);
}

/// opencode's "most restrictive resource wins" across a MULTI-command request
/// (`permission.ts:160`). `apply_patch` touching five files, or a `&&` chain,
/// is one approval decision over many subjects: one denied subject denies all.
#[test]
fn one_denied_command_in_a_batch_denies_the_batch() {
    let mut policy = Policy::empty();
    policy
        .add_prefix_rule(&tokens(&["ls"]), Decision::Allow)
        .expect("allow ls");
    policy
        .add_prefix_rule(&tokens(&["curl"]), Decision::Forbidden)
        .expect("deny curl");

    let batch = [
        tokens(&["ls", "-la"]),
        tokens(&["curl", "https://example.com"]),
    ];
    assert_eq!(
        Decision::Forbidden,
        policy
            .check_multiple(batch.iter(), &|_| Decision::Allow)
            .decision,
    );
}
