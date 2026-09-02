//! The one place this client calls an Estelle MCP tool, and the one place a refusal is recognised.
//!
//! 🔴 **A REFUSAL ARRIVES ON THE SUCCESS PATH, WHICH IS WHY IT NEEDED A TYPE.**
//!
//! `serve/mcp/__init__.py:1174` returns the graph-currency refusal as ORDINARY text content —
//! HTTP 200, `isError` unset, a normal `content[0].text` that happens to begin `CANNOT ANSWER: `.
//! Measured against production on 2026-09-02: `chokepoints{"repo":"uqeu/estelle"}` answered
//!
//! > `CANNOT ANSWER: uqeu/estelle: currency UNKNOWN — this repo has never been swept …`
//!
//! and so did `core_files` and `import_cycles`. Nothing on the wire distinguishes that from an
//! answer except the first fourteen bytes.
//!
//! ⚠️ **THE COST OF NOT HAVING THIS TYPE IS ALREADY IN THE PRODUCT.**
//! [`crate::production_hud::fetch`] splits every tool reply on newlines and takes the first three
//! as chokepoint FILE NAMES. Handed the sentence above it renders the refusal as three rows of
//! graph data — a screen that says "here is your risk map" over a server that said it has no
//! graph. That is the [`Outcome::CannotAnswer`] arm's whole reason to exist: the caller is made to
//! handle the refusal, because a `String` return let it be forgotten.
//!
//! 🔴 **AND A REFUSAL IS NOT AN ERROR.** Modelling it as `Err` would be the same defect wearing
//! the other coat: an error is something that went wrong and can be retried, and this is the
//! server correctly declining to guess. It is a THIRD outcome, and the type says so.

use estelle_client::{Client, Repo};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// The exact prefix the server puts on a withheld answer.
///
/// ⚠️ **DEFINED FROM THE SERVER, NOT INVENTED HERE.** `serve/mcp/__init__.py:1174` builds it as
/// `"CANNOT ANSWER: " + health.describe() + …`; `agent/graph_tools.py:77` uses the same opening
/// for `locate`'s prose refusal. Both include the trailing space, so a tool that answered the
/// literal string `"CANNOT ANSWERED"` — were one ever to exist — is not mistaken for a refusal.
pub(crate) const REFUSAL_PREFIX: &str = "CANNOT ANSWER: ";

/// What a tool call produced: an answer, or the server declining to give one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// Text the tool actually produced. Never empty — an empty body is an `Err`, because a blank
    /// answer and a refusal to answer are the two things this module exists to keep apart.
    Answered(String),
    /// The server withheld the answer and said why, in its own words.
    ///
    /// 🔴 **THE SERVER'S SENTENCE IS CARRIED VERBATIM, MINUS THE PREFIX.** A client that
    /// paraphrases a refusal into "no data" throws away the only part the user can act on — which
    /// repo, how stale, and what to run. The prefix comes off because it is a tag for this parser,
    /// not a word the reader needs twice.
    CannotAnswer(String),
}

impl Outcome {
    /// The lines an ANSWER carries, trimmed and blank-stripped. A refusal yields none.
    ///
    /// ⚠️ This is the accessor that makes the old bug unwriteable: there is no way to get lines
    /// out of a refusal, so a caller that wants rows must decide what a refusal looks like.
    pub(crate) fn lines(&self) -> Vec<String> {
        match self {
            Self::Answered(text) => text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(estelle_client::mask_secret)
                .collect(),
            Self::CannotAnswer(_) => Vec::new(),
        }
    }
}

/// Classify one tool's text reply. Split out from the request so it is testable without a server.
pub(crate) fn classify(text: &str) -> Outcome {
    match text.strip_prefix(REFUSAL_PREFIX) {
        Some(reason) => Outcome::CannotAnswer(estelle_client::mask_secret(reason.trim())),
        None => Outcome::Answered(text.to_string()),
    }
}

/// Call one Estelle MCP tool against one repo.
///
/// The `repo` argument is inserted rather than left to the caller: every navigation tool takes it,
/// and a call that omits it silently targets whatever the account's default repo is — which is a
/// different question from the one the screen asked.
pub(crate) async fn call(
    client: &Client,
    repo: &Repo,
    name: &str,
    mut arguments: Value,
    cancel: &CancellationToken,
) -> Result<Outcome, String> {
    let Some(object) = arguments.as_object_mut() else {
        return Err(format!("{name} arguments were not an object"));
    };
    object.insert("repo".to_string(), Value::String(repo.as_str().to_string()));
    let response: Value = client
        .post(
            estelle_client::Endpoint::Mcp,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }),
            cancel,
        )
        .await
        .map_err(|error| format!("{name} request failed: {error}"))?;
    text_of(name, &response).map(|text| classify(&text))
}

/// Pull the single text block out of an MCP `tools/call` result.
///
/// 🔴 **EVERY FAILURE MODE IS NAMED SEPARATELY.** A protocol error, a tool error, a missing result
/// and an empty body are four different facts, and collapsing them into one message is how a
/// caller ends up retrying something that will never succeed.
pub(crate) fn text_of(name: &str, response: &Value) -> Result<String, String> {
    if let Some(error) = response.get("error") {
        return Err(format!(
            "{name} returned a protocol error: {}",
            estelle_client::mask_secret(&error.to_string())
        ));
    }
    let result = response
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{name} omitted the MCP result object"))?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(format!("{name} reported a tool failure"));
    }
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| item.get("text").and_then(Value::as_str))
                .flatten()
        })
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{name} returned no text content"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 🔴 THE PRODUCTION SENTENCE, VERBATIM. Captured from `chokepoints{"repo":"uqeu/estelle"}` on
    /// 2026-09-02. It is pinned here rather than paraphrased so a server-side rewording that
    /// changes the prefix trips this test instead of silently turning refusals back into data.
    const PRODUCTION_REFUSAL: &str = "CANNOT ANSWER: uqeu/estelle: currency UNKNOWN — this repo has never been swept, so there is no graph to date. This is NOT the same as up to date; symbols added since the last sweep will be flagged as hallucinations. Sweep this repo first — nothing has been indexed for it yet. Navigation is withheld because a stale or undated graph cannot prove either that a symbol is absent or that its recorded line is current.";

    #[test]
    fn the_production_refusal_is_a_refusal_and_not_three_rows_of_graph_data() {
        let outcome = classify(PRODUCTION_REFUSAL);
        let Outcome::CannotAnswer(reason) = &outcome else {
            panic!("the production refusal classified as an answer: {outcome:?}");
        };
        assert!(
            reason.starts_with("uqeu/estelle: currency UNKNOWN"),
            "the server's own sentence must survive, minus the tag: {reason}"
        );
        assert!(
            reason.contains("Sweep this repo first"),
            "the actionable half is the half a user needs: {reason}"
        );
        // The whole point: a refusal yields NO rows. `production_hud` used to take the first three
        // lines of exactly this string and draw them as chokepoint file names.
        assert!(outcome.lines().is_empty(), "{outcome:?}");
    }

    #[test]
    fn an_answer_keeps_its_lines() {
        let outcome = classify("api.py  (0.81)\n\nserve/memory.py  (0.64)\n");
        assert_eq!(
            outcome.lines(),
            ["api.py  (0.81)", "serve/memory.py  (0.64)"]
        );
    }

    /// ⚠️ A NEGATIVE CONTROL FOR THE PREFIX ITSELF. Without this, `classify` could match on
    /// "CANNOT" and nobody would notice until a real answer began with that word.
    #[test]
    fn a_sentence_that_merely_mentions_the_phrase_is_still_an_answer() {
        let outcome = classify("api.py  — the docstring says CANNOT ANSWER: never all clear");
        assert!(matches!(outcome, Outcome::Answered(_)), "{outcome:?}");
        assert_eq!(outcome.lines().len(), 1);
    }

    #[test]
    fn each_failure_mode_says_which_one_it_was() {
        assert_eq!(
            text_of("chokepoints", &json!({"error": {"message": "no"}})).unwrap_err(),
            "chokepoints returned a protocol error: {\"message\":\"no\"}"
        );
        assert_eq!(
            text_of("chokepoints", &json!({})).unwrap_err(),
            "chokepoints omitted the MCP result object"
        );
        assert_eq!(
            text_of("chokepoints", &json!({"result": {"isError": true}})).unwrap_err(),
            "chokepoints reported a tool failure"
        );
        assert_eq!(
            text_of(
                "chokepoints",
                &json!({"result": {"content": [{"type": "text", "text": "   "}]}})
            )
            .unwrap_err(),
            "chokepoints returned no text content"
        );
    }
}
