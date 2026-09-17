//! A recall arm that did not FINISH is not a repository that holds nothing.
//!
//! 🔴 **MEASURED 2026-09-17 ON THE FOUNDER'S OWN MACHINE, FROM THIS HOOK'S OWN FLIGHT RECORDER.**
//! `~/.estelle/context-hook.jsonl` plus its rotation, n=95 real `UserPromptSubmit` groundings over
//! 3.97 h (02:16:10→06:14:13) — **the three probes this lane issued itself are excluded**, because a
//! measurement that counts the measurer's own traffic is not a measurement of the founder's session.
//! Reproduced by `bench/retrieval-relevance-20260910/hook_latency.py` in the server repo.
//!
//! | outcome | n | share |
//! |---|---|---|
//! | answered **carrying repository memory** | 42 | 44.2% |
//! | answered **carrying NOTHING** (`counts.recall == 0`) | **45** | **47.4%** |
//! | `transport_failed` | 4 | 4.2% |
//! | `awaiting_response` (our 20 s deadline) | 4 | 4.2% |
//!
//! **43 of the 87 answered calls had the server's `recall` stage land on 7.9–8.1 s — its own
//! `RECALL_DEADLINE_S` — and ALL 43 of those carried `counts.recall == 0`. 43/43.** The failure the
//! founder SEES is the 20 s abandonment, and it is 4% of calls. The failure he does not see was
//! eleven times larger.
//!
//! ⚠️ **AND THE SERVER MOVED UNDER THE MEASUREMENT, WHICH IS WHY BOTH HALVES ARE QUOTED.** Database
//! work landed mid-window; splitting at the last of the flat-8 s run (a point INFERRED from the
//! data, not from a deploy record) gives **before, n=59: 22.0% carried memory, 67.8% carried
//! nothing, p50 11,859 ms** and **after, n=36: 80.6% carried memory, 13.9% carried nothing,
//! p50 3,212 ms**. So the cause is largely fixed on the server and
//! **one turn in six is still silently ungrounded** — which is exactly the population this arm
//! exists to stop lying about. n=40 is small and the log is live; the direction of the remaining
//! error is not known.
//!
//! ⛔ **AND THE HOOK HAD NO WAY TO SPELL IT.** `context_recall` classified on the `recall` FIELD
//! ALONE — non-empty → `Grounded`, empty → `NothingToRecall`, absent → `Ungrounded` — so an
//! abandoned retrieval landed in one of two arms and **both of them lie**:
//!
//! * **Empty field** → `NothingToRecall`, whose envelope tells the model, in words, *"That is a
//!   measured empty result, not a failure to reach Estelle."* A retrieval that gave up at 8 s
//!   measured nothing. This is a confident false negative on the one endpoint that exists to
//!   prevent those.
//! * **Filled field** (what production serves today — `estelle/serve/recall_expiry.py`'s
//!   `expired_recall_envelope` leads the body with a refusal sentence so that every already-shipped
//!   client at least reads honest TEXT) → `Grounded`, so the model is handed a **disclaimer as
//!   retrieval**, the human is told nothing at all, and the flight recorder files it under
//!   `answered`.
//!
//! The server lane wrote this hand-off down rather than guessing at it
//! (`recall_expiry.py::reads_as_grounded`): *"Fixing the enum needs a fourth arm in `cli-rs` keyed
//! on `RECALL_EXPIRED_FIELD`, which lives in another repository and cannot be shipped by a server
//! deploy."* This module is that fourth arm's evidence half.
//!
//! ⚠️ **WHAT THIS MODULE DOES NOT DO, SAID OUT LOUD AND FIRST.** It does not make recall finish, it
//! is not a bound on anything, and it does not recover the lost grounding. It converts a silent
//! wrong answer into a loud true one. The latency itself belongs to the server's recall arm and is
//! bounded there; a client cannot make an 8 s call fast, it can only refuse to misreport it.

use serde_json::Value;

/// The server's MACHINE-READABLE fact. Defined by `estelle/serve/recall_expiry.py`
/// (`RECALL_EXPIRED_FIELD`), which documents it as *"Present only when the arm expired — never
/// `False` on a healthy answer."*
///
/// ⚖️ That property is what lets this be read as a bare presence test rather than a truthiness
/// test: a field whose absence and whose `false` mean the same thing is the defect family this
/// whole module lives inside, so the server does not send `false` and this does not look for it.
/// The test `only_a_true_expiry_flag_counts` pins that an explicit `false` is still not an expiry.
pub const RECALL_EXPIRED_FIELD: &str = "recall_expired";

/// The server's HUMAN-READABLE fact: the first words of the body it substitutes on expiry
/// (`recall_expiry.py`'s `RECALL_EXPIRED_PREFIX`). Asserted there to be the leading bytes of the
/// payload, so a prefix test here means the same thing a prefix test means there.
pub const RECALL_EXPIRED_PREFIX: &str = "RECALL DID NOT COMPLETE - ";

/// Did the server tell us its retrieval never finished?
///
/// 🔑 **ONE OWNER, TWO EVIDENCE SOURCES, AND THAT IS DELIBERATE RATHER THAN SLOPPY.** Power of Ten
/// #9 forbids two PLACES computing one derived fact; it does not forbid one place reading two
/// signals. Both live here, ranked, and each is independently sufficient — because each one alone
/// fails in a way the other covers:
///
/// 1. [`RECALL_EXPIRED_FIELD`] is the durable, parseable fact and is preferred. It is also the
///    NEWER of the two: a server rolled back past `recall_expiry.py` stops sending it.
/// 2. [`RECALL_EXPIRED_PREFIX`] is the sentence every already-shipped client renders. It survives
///    a rollback of the structured field, and it is what the founder's binary is reading today.
///
/// ⚠️ **THE LIMIT.** Both signals come from the same server module, so this detector cannot see an
/// expiry from a server that reports neither — an older build, or a future one that renames both.
/// On such a server this returns `false` and the caller degrades to the previous (wrong)
/// classification. That is a silent-exemption shape, so it is named here rather than left for a
/// reader to discover: `an_unannounced_expiry_is_not_detectable` asserts exactly that blind spot
/// instead of pretending it does not exist.
///
/// ⛔ It is NOT keyed on `degraded`. `api_intel` sets `degraded = not recall_ok or not facts_ok`,
/// so a healthy recall beside a failed FACTS arm carries `degraded: true` — reading it here would
/// report "retrieval did not complete" over a retrieval that completed fine. One meaning per name.
pub fn recall_did_not_complete(result: &Value, recall: &str) -> bool {
    if result.get(RECALL_EXPIRED_FIELD).and_then(Value::as_bool) == Some(true) {
        return true;
    }
    recall.trim_start().starts_with(RECALL_EXPIRED_PREFIX)
}

/// The ONE line the human sees. Short on purpose: this fires on a large share of prompts while the
/// server's recall arm is contended, and a paragraph on the hot path is a notice somebody mutes.
///
/// ⚠️ It says DEGRADED rather than "did not ground", because the sibling `Ungrounded` line already
/// owns that phrase for *we never got an answer at all* — and a reader who sees one sentence for
/// two different facts learns to read neither.
pub const DEGRADED_HUMAN_LINE: &str = "⚠ Estelle's grounding was DEGRADED this turn — the server's \
                                       retrieval did not complete, so no repository memory reached \
                                       your prompt.";

/// What leads the MODEL's copy, in front of the server's own account of the failure.
///
/// 🔑 It leads rather than follows for the same reason the server's sentence does: a truncating
/// reader must still meet the part that changes what may be concluded.
pub const DEGRADED_CONTEXT_LEAD: &str = "Estelle did not ground this turn: its retrieval was abandoned before it completed. Treat this \
     as MISSING grounding, NEVER as a search that found nothing, and verify any claim about this \
     repository by reading the files before stating it. The server's own account follows.\n\n";

/// The model-facing body for a degraded turn: our refusal, then the server's verbatim.
pub fn degraded_context(server_account: &str) -> String {
    format!("{DEGRADED_CONTEXT_LEAD}{}", server_account.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The envelope production serves today, read out of `estelle/serve/recall_expiry.py`
    /// (`expired_recall_envelope`) rather than invented — a double friendlier than production
    /// certifies code production rejects.
    fn expired_body() -> (Value, String) {
        let sentence = format!(
            "{RECALL_EXPIRED_PREFIX}Estelle's semantic recall for uqeu/estelle did not finish \
             inside its 8s budget and returned NO content. Nothing was retrieved on this turn, and \
             that is a fact about the RETRIEVAL, never about the repository — treat this as MISSING \
             grounding, not as a search that found nothing."
        );
        (
            json!({
                "recall": sentence,
                "degraded": true,
                RECALL_EXPIRED_FIELD: true,
                "repo": "uqeu/estelle",
            }),
            sentence,
        )
    }

    #[test]
    fn the_shipped_expired_envelope_is_recognised() {
        let (body, sentence) = expired_body();
        assert!(recall_did_not_complete(&body, &sentence));
    }

    /// 🔬 EACH EVIDENCE SOURCE ALONE MUST FLIP IT, OR ONE OF THEM IS DECORATION. A detector that
    /// only fires when BOTH signals are present is a detector that fires on neither half of the
    /// rollback it was written for.
    #[test]
    fn the_structured_field_alone_is_enough() {
        let body = json!({"recall": "the retry policy lives in serve/backend.py",
                          RECALL_EXPIRED_FIELD: true});
        assert!(
            recall_did_not_complete(&body, "the retry policy lives in serve/backend.py"),
            "the parseable fact must stand on its own — a server that stops leading the body with \
             the sentence would otherwise silently un-fix this"
        );
    }

    #[test]
    fn the_sentence_alone_is_enough() {
        let sentence = format!("{RECALL_EXPIRED_PREFIX}recall did not finish inside its 8s budget");
        let body = json!({"recall": sentence});
        assert!(
            recall_did_not_complete(&body, &sentence),
            "a server rolled back past the structured field still announces itself in the TEXT, \
             which is the channel every already-shipped client renders"
        );
    }

    /// 🧪 THE CONTROL. Without this the detector could be `true` unconditionally and every test
    /// above would still pass.
    #[test]
    fn a_healthy_answer_is_not_an_expiry() {
        let body = json!({"recall": "the retry policy lives in serve/backend.py",
                          "repo": "uqeu/estelle"});
        assert!(!recall_did_not_complete(
            &body,
            "the retry policy lives in serve/backend.py"
        ));
        assert!(
            !recall_did_not_complete(&json!({"recall": ""}), ""),
            "a MEASURED empty is not an expiry — collapsing them would move the lie rather than \
             remove it"
        );
    }

    /// ⛔ `degraded` IS A DIFFERENT FACT. `api_intel` sets it from `recall_ok OR facts_ok`, so a
    /// completed recall beside a failed facts arm carries it. Reading it here would report an
    /// expiry over a retrieval that finished.
    #[test]
    fn a_degraded_flag_is_not_an_expiry() {
        let body = json!({"recall": "the retry policy lives in serve/backend.py",
                          "degraded": true});
        assert!(!recall_did_not_complete(
            &body,
            "the retry policy lives in serve/backend.py"
        ));
    }

    /// ⚖️ ABSENCE AND `false` MUST NOT BE THE SAME BYTES HERE EITHER.
    #[test]
    fn only_a_true_expiry_flag_counts() {
        let body = json!({"recall": "grounded text", RECALL_EXPIRED_FIELD: false});
        assert!(!recall_did_not_complete(&body, "grounded text"));
    }

    /// 🔴 THE BLIND SPOT, ASSERTED RATHER THAN HIDDEN. A server that expires its recall and
    /// announces it in NEITHER channel is invisible to this detector, and the caller falls back to
    /// the old, wrong classification. Writing the limit as a test is what stops a later reader
    /// believing this covers every expiry.
    #[test]
    fn an_unannounced_expiry_is_not_detectable() {
        let body = json!({"recall": "", "timings": {"stages": {"recall": 8.001}}});
        assert!(
            !recall_did_not_complete(&body, ""),
            "if this ever goes red someone taught the detector to read `timings` — good, but then \
             the doc comment above is stale and must move with it"
        );
    }

    #[test]
    fn the_model_body_leads_with_the_refusal_and_keeps_the_server_account() {
        let (_, sentence) = expired_body();
        let body = degraded_context(&sentence);
        assert!(body.starts_with(DEGRADED_CONTEXT_LEAD));
        assert!(body.contains(RECALL_EXPIRED_PREFIX));
        assert!(
            body.contains("MISSING grounding"),
            "the clause that changes what may be concluded has to survive: {body}"
        );
    }
}
