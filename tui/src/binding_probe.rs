//! Does a stored provider credential actually WORK? One real call, bounded, that can fail.
//!
//! 🔴 **THE DEFECT THIS EXISTS TO CLOSE.** Every login path in this crate ended by printing
//! *"provider runtime binding is not yet proven. Run estelle doctor."* — and `estelle doctor` was a
//! synchronous local-file inspector with zero network calls, which printed the same sentence back. The
//! remedy named by the message was a second copy of the complaint, so a user who followed the
//! instruction was returned to the instruction. Underneath, the credential store had no reader at all:
//! nothing in production could load back what login had saved.
//!
//! ⚠️ **PRESENCE IS NOT CAPABILITY.** A file on disk proves a login wrote something; it says nothing
//! about whether the provider will answer. This is the same lesson the server side paid for with
//! `/health` carrying a build SHA while the process served different code: a label identifies what was
//! stored, only a probe identifies what works.
//!
//! ⛔ What this deliberately is NOT: it is not a health check for the provider, and it never asserts
//! the model is good. It answers exactly one question — *will this endpoint accept this credential* —
//! and every other question stays visibly unanswered.

use std::time::Duration;

/// Hard ceiling on a single probe. A named constant, not a literal: an unbounded probe would hang
/// `doctor` on a wedged endpoint, and a diagnostic that hangs is worse than one that says "unknown".
pub(crate) const PROBE_TIMEOUT_S: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Binding {
    /// Nothing stored for this provider. Not a failure — the ordinary state before a login.
    NotConfigured,
    /// The endpoint answered and accepted the credential.
    Bound { detail: String },
    /// The endpoint answered and REFUSED. The credential is present and wrong (or unentitled).
    Refused { status: u16 },
    /// The endpoint answered, but not with an authentication refusal or a usable model list.
    EndpointError { status: u16 },
    /// Success status with bytes that do not satisfy the model-list response contract.
    InvalidResponse,
    /// We never got an answer. ⚠️ Distinct from `Refused` on purpose: they have opposite fixes —
    /// one is a bad credential, the other is a dead endpoint, and collapsing them sends the user to
    /// re-run a login that was never the problem.
    Unreachable { reason: String },
}

impl Binding {
    /// A failure is something the user must act on. `NotConfigured` is not one.
    pub(crate) fn is_failure(&self) -> bool {
        matches!(
            self,
            Binding::Refused { .. }
                | Binding::EndpointError { .. }
                | Binding::InvalidResponse
                | Binding::Unreachable { .. }
        )
    }

    pub(crate) fn line(&self, provider: &str) -> String {
        match self {
            Binding::NotConfigured => format!("{provider} binding  not configured"),
            Binding::Bound { detail } => format!("{provider} binding  BOUND · {detail}"),
            Binding::Refused { status } => format!(
                "{provider} binding  FAIL · the endpoint answered {status} and refused the stored \
                 credential — the credential is present and not accepted, so re-run the login"
            ),
            Binding::EndpointError { status } => format!(
                "{provider} binding  FAIL · the endpoint answered {status}; only 401/403 mean the \
                 stored credential was refused — inspect the endpoint and provider state"
            ),
            Binding::InvalidResponse => format!(
                "{provider} binding  FAIL · the endpoint answered 2xx without a model-list data \
                 array — acceptance could not be verified"
            ),
            Binding::Unreachable { reason } => format!(
                "{provider} binding  FAIL · no answer within {PROBE_TIMEOUT_S}s: {reason} — the \
                 endpoint did not reply, which is NOT a rejected credential; check the endpoint first"
            ),
        }
    }
}

/// Probe an OpenAI-compatible endpoint by listing models.
///
/// `/models` is chosen because it is the cheapest authenticated read these APIs offer: it spends no
/// tokens, mutates nothing, and still exercises the credential. A completion request would prove the
/// same thing and bill for it.
pub(crate) async fn probe_openai_compatible(
    client: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
) -> Binding {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let mut request = client
        .get(&url)
        .timeout(Duration::from_secs(PROBE_TIMEOUT_S));
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    match request.send().await {
        Err(error) => Binding::Unreachable {
            reason: error.to_string(),
        },
        Ok(response) if matches!(response.status().as_u16(), 401 | 403) => Binding::Refused {
            status: response.status().as_u16(),
        },
        Ok(response) if !response.status().is_success() => Binding::EndpointError {
            status: response.status().as_u16(),
        },
        Ok(response) => {
            let count = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|body| {
                    body.get("data")
                        .and_then(|data| data.as_array().map(Vec::len))
                });
            match count {
                Some(n) => Binding::Bound {
                    detail: format!("{url} answered · {n} model(s) offered"),
                },
                None => Binding::InvalidResponse,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_configured_is_not_a_failure() {
        // The ordinary pre-login state must never make `doctor` exit non-zero, or a fresh install
        // reports broken.
        assert!(!Binding::NotConfigured.is_failure());
        assert!(!Binding::Bound { detail: "x".into() }.is_failure());
    }

    #[test]
    fn both_failure_kinds_are_failures() {
        assert!(Binding::Refused { status: 401 }.is_failure());
        assert!(Binding::EndpointError { status: 500 }.is_failure());
        assert!(Binding::InvalidResponse.is_failure());
        assert!(
            Binding::Unreachable {
                reason: "connection refused".into()
            }
            .is_failure()
        );
    }

    #[test]
    fn refused_and_unreachable_never_read_the_same() {
        // 🔴 They have OPPOSITE fixes. Collapsing them sends the user to re-run a login that was
        // never the problem, which is exactly the loop this module exists to end.
        let refused = Binding::Refused { status: 401 }.line("local");
        let unreachable = Binding::Unreachable {
            reason: "connection refused".into(),
        }
        .line("local");
        assert_ne!(refused, unreachable);
        assert!(refused.contains("re-run the login"));
        assert!(unreachable.contains("check the endpoint first"));
        assert!(!unreachable.contains("re-run the login"));
    }

    #[test]
    fn a_failure_line_never_says_not_yet_proven() {
        // The phrase this module replaces. It appeared 16 times across 5 files and was the entire
        // user-visible outcome of a successful login; if it comes back, so has the defect.
        for binding in [
            Binding::Refused { status: 403 },
            Binding::EndpointError { status: 429 },
            Binding::InvalidResponse,
            Binding::Unreachable {
                reason: "timed out".into(),
            },
            Binding::Bound {
                detail: "ok".into(),
            },
            Binding::NotConfigured,
        ] {
            assert!(!binding.line("local").contains("not yet proven"));
        }
    }

    #[test]
    fn the_timeout_is_stated_in_the_message_the_user_reads() {
        // A bound that is not visible to the person waiting is a bound only the author knows about.
        let line = Binding::Unreachable { reason: "x".into() }.line("local");
        assert!(line.contains(&PROBE_TIMEOUT_S.to_string()));
    }

    #[tokio::test]
    async fn model_list_probe_distinguishes_acceptance_auth_refusal_and_server_failure() {
        use wiremock::Mock;
        use wiremock::MockServer;
        use wiremock::ResponseTemplate;
        use wiremock::matchers::method;
        use wiremock::matchers::path;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "local-model"}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/auth/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/server/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/invalid/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not a model list"))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();

        assert!(matches!(
            probe_openai_compatible(&client, &format!("{}/ok", server.uri()), None).await,
            Binding::Bound { .. }
        ));
        assert_eq!(
            probe_openai_compatible(&client, &format!("{}/auth", server.uri()), Some("bad")).await,
            Binding::Refused { status: 401 }
        );
        assert_eq!(
            probe_openai_compatible(&client, &format!("{}/server", server.uri()), None).await,
            Binding::EndpointError { status: 500 }
        );
        assert_eq!(
            probe_openai_compatible(&client, &format!("{}/invalid", server.uri()), None).await,
            Binding::InvalidResponse
        );
    }
}
