//! GitHub Copilot device login adapted from jcode's Copilot auth flow (MIT).

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use serde::Deserialize;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::provider_store;

const GITHUB_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

struct Endpoints {
    device: String,
    token: String,
}

impl Endpoints {
    fn production() -> Self {
        Self {
            device: GITHUB_DEVICE_CODE_URL.to_string(),
            token: GITHUB_ACCESS_TOKEN_URL.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct CopilotSnapshot<'a> {
    provider: &'static str,
    oauth_token: &'a str,
}

fn store_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle/providers/copilot.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

pub(crate) fn credential_present() -> bool {
    store_path().is_ok_and(|path| path.is_file())
}

pub(crate) fn logout() -> io::Result<bool> {
    let path = store_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) async fn run() -> io::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(io::Error::other)?;
    run_with(
        &client,
        &Endpoints::production(),
        &store_path()?,
        &mut io::stdout(),
    )
    .await
}

async fn run_with(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    destination: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    let response = client
        .post(&endpoints.device)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", GITHUB_COPILOT_CLIENT_ID),
            ("scope", "read:user"),
        ])
        .send()
        .await
        .map_err(io::Error::other)?;
    if !response.status().is_success() {
        return Err(io::Error::other(format!(
            "GitHub device flow returned HTTP {}",
            response.status()
        )));
    }
    let device: DeviceCodeResponse = response.json().await.map_err(io::Error::other)?;
    writeln!(
        output,
        "Open {} and enter code {}.",
        device.verification_uri, device.user_code
    )?;
    writeln!(output, "Waiting for GitHub authorization…")?;
    output.flush()?;

    let device_code = Zeroizing::new(device.device_code);
    let token = poll_for_token(
        client,
        endpoints,
        device_code.as_str(),
        device.interval,
        device.expires_in,
    )
    .await?;
    persist_token(token.as_str(), destination)?;
    writeln!(output, "GitHub Copilot credential stored privately.")?;
    writeln!(
        output,
        "GitHub authorized the device flow; no Copilot model request was made during login."
    )?;
    output.flush()
}

async fn poll_for_token(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    device_code: &str,
    initial_interval: u64,
    expires_in: u64,
) -> io::Result<Zeroizing<String>> {
    let deadline = Instant::now() + Duration::from_secs(expires_in.clamp(1, 1_800));
    let mut interval = initial_interval;
    loop {
        ensure_not_expired(deadline)?;
        tokio::time::sleep(Duration::from_secs(interval)).await;
        ensure_not_expired(deadline)?;
        let token = poll_once(client, endpoints, device_code).await?;
        if let Some(access_token) = interpret_token_response(token, &mut interval)? {
            return Ok(access_token);
        }
    }
}

fn ensure_not_expired(deadline: Instant) -> io::Result<()> {
    if Instant::now() < deadline {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "GitHub device code expired; run login again",
    ))
}

async fn poll_once(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    device_code: &str,
) -> io::Result<AccessTokenResponse> {
    let response = client
        .post(&endpoints.token)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", GITHUB_COPILOT_CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(io::Error::other)?;
    response.json().await.map_err(io::Error::other)
}

fn interpret_token_response(
    token: AccessTokenResponse,
    interval: &mut u64,
) -> io::Result<Option<Zeroizing<String>>> {
    if let Some(access_token) = token.access_token {
        return Ok(Some(Zeroizing::new(access_token)));
    }
    match token.error.as_deref() {
        Some("authorization_pending") => Ok(None),
        Some("slow_down") => {
            *interval = interval.saturating_add(5).min(30);
            Ok(None)
        }
        Some("expired_token") => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "GitHub device code expired; run login again",
        )),
        Some("access_denied") => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "GitHub authorization was denied; nothing was stored",
        )),
        Some(error) => Err(io::Error::other(format!(
            "GitHub authorization failed with status {error}"
        ))),
        None => Err(io::Error::other(
            "GitHub returned no access token or status",
        )),
    }
}

fn persist_token(token: &str, destination: &Path) -> io::Result<()> {
    provider_store::write_private_json(
        &CopilotSnapshot {
            provider: "copilot",
            oauth_token: token,
        },
        destination,
        "copilot.json",
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;

    #[tokio::test]
    async fn device_flow_stores_token_privately_without_printing_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "device-secret",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://github.example/device",
                "expires_in": 30,
                "interval": 0
            })))
            .expect(1)
            .mount(&server)
            .await;
        let token = "gho_copilot_test_secret";
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": token,
                "token_type": "bearer",
                "scope": "read:user"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/copilot.json");
        let endpoints = Endpoints {
            device: format!("{}/device", server.uri()),
            token: format!("{}/token", server.uri()),
        };
        let mut output = Vec::new();

        run_with(
            &reqwest::Client::new(),
            &endpoints,
            &destination,
            &mut output,
        )
        .await
        .expect("Copilot device login");

        assert_eq!(
            fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let receipt = String::from_utf8(output).expect("UTF-8 receipt");
        assert!(receipt.contains("ABCD-EFGH"));
        assert!(!receipt.contains(token));
    }

    #[tokio::test]
    async fn provider_error_body_is_not_rendered_or_persisted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "device-secret",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://github.example/device",
                "expires_in": 30,
                "interval": 0
            })))
            .mount(&server)
            .await;
        let secret = "provider-error-secret";
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "error": "server_error",
                "error_description": secret
            })))
            .mount(&server)
            .await;
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/copilot.json");
        let endpoints = Endpoints {
            device: format!("{}/device", server.uri()),
            token: format!("{}/token", server.uri()),
        };
        let mut output = Vec::new();

        let error = run_with(
            &reqwest::Client::new(),
            &endpoints,
            &destination,
            &mut output,
        )
        .await
        .expect_err("provider error");

        assert!(!destination.exists());
        assert!(!error.to_string().contains(secret));
        assert!(!String::from_utf8(output).unwrap().contains(secret));
    }
}
