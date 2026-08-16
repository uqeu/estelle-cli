use std::io;
use std::io::Write;

use estelle_client::Client;
use estelle_client::CommandReply;
use estelle_client::CredentialStore;
use estelle_client::Endpoint;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::login;

#[derive(Serialize)]
struct ProviderKeyRequest<'a> {
    provider: &'a str,
    provider_key: &'a str,
    base_url: &'a str,
    model: &'a str,
    label: &'a str,
}

async fn set_provider_key_with(
    client: &Client,
    provider: &str,
    base_url: Option<&str>,
    model: Option<&str>,
    label: Option<&str>,
    secret: Zeroizing<String>,
    output: &mut impl Write,
) -> io::Result<()> {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider name cannot be empty",
        ));
    }
    let request = ProviderKeyRequest {
        provider: &provider,
        provider_key: secret.as_str(),
        base_url: base_url.unwrap_or_default(),
        model: model.unwrap_or_default(),
        label: label.unwrap_or_default(),
    };
    let receipt: CommandReply = client
        .post(Endpoint::ProviderKey, &request, &CancellationToken::new())
        .await
        .map_err(io::Error::other)?;
    let stored_provider = receipt.provider.as_deref().unwrap_or(&provider);
    writeln!(output, "Provider credential stored for {stored_provider}.")?;
    if receipt
        .extra
        .get("verified")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        writeln!(output, "Verification: accepted by {stored_provider}.")?;
    } else {
        writeln!(
            output,
            "Verification: not observed; the credential was stored without claiming provider acceptance."
        )?;
    }
    if let Some(model) = receipt
        .extra
        .get("provider_model")
        .and_then(serde_json::Value::as_str)
        .filter(|model| !model.trim().is_empty())
    {
        writeln!(output, "Model: {model}.")?;
    }
    output.flush()
}

pub(crate) async fn run(
    provider: &str,
    base_url: Option<&str>,
    model: Option<&str>,
    label: Option<&str>,
) -> io::Result<()> {
    let Some(secret) = login::read_secret_value(b"Provider API key: ")? else {
        io::stdout().write_all(b"Provider login cancelled. Nothing was stored.\n")?;
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "provider login cancelled; no credential was sent",
        ));
    };
    let store = CredentialStore::default_location().map_err(io::Error::other)?;
    let credential = store.resolve().map_err(io::Error::other)?;
    let client = Client::production(credential.api_key).map_err(io::Error::other)?;
    set_provider_key_with(
        &client,
        provider,
        base_url,
        model,
        label,
        secret,
        &mut io::stdout(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use estelle_client::ApiKey;
    use estelle_client::Client;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_json;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use zeroize::Zeroizing;

    use super::*;

    #[tokio::test]
    async fn provider_key_is_posted_once_and_never_appears_in_the_receipt() {
        let server = MockServer::start().await;
        let secret = "provider-secret-only-on-the-wire";
        Mock::given(method("POST"))
            .and(path("/key"))
            .and(header("authorization", "Bearer estelle_live_test-only"))
            .and(body_json(serde_json::json!({
                "provider": "anthropic",
                "provider_key": secret,
                "base_url": "",
                "model": "claude-opus",
                "label": "production"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "provider": "anthropic",
                "provider_model": "claude-opus",
                "configured": ["anthropic"],
                "verified": true
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            ApiKey::new("estelle_live_test-only").expect("Estelle key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let mut output = Vec::new();

        set_provider_key_with(
            &client,
            "anthropic",
            None,
            Some("claude-opus"),
            Some("production"),
            Zeroizing::new(secret.to_string()),
            &mut output,
        )
        .await
        .expect("provider key write");

        let receipt = String::from_utf8(output).expect("receipt");
        assert!(receipt.contains("Provider credential stored for anthropic."));
        assert!(receipt.contains("Verification: accepted by anthropic."));
        assert!(receipt.contains("Model: claude-opus."));
        assert!(!receipt.contains(secret));
    }
}
