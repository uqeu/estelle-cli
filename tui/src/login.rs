use std::io;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Write;
use std::path::PathBuf;

use crossterm::event::DisableBracketedPaste;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::execute;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use estelle_client::ApiKey;
use estelle_client::Client;
use estelle_client::CredentialStore;
use estelle_client::Error;
use tokio_util::sync::CancellationToken;
use url::Url;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoginOutcome {
    StoredVerified,
    StoredUnverified,
    Rejected,
}

#[cfg(test)]
pub(crate) fn read_secret_line(
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Option<ApiKey>> {
    read_secret_value_line(b"Estelle key: ", input, output)?
        .map(|value| ApiKey::new(value.to_string()))
        .transpose()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

pub(crate) fn read_secret_value_line(
    prompt: &[u8],
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Option<Zeroizing<String>>> {
    output.write_all(prompt)?;
    output.flush()?;
    let mut value = Zeroizing::new(String::new());
    input.read_line(&mut value)?;
    output.write_all(b"\n")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(Zeroizing::new(trimmed.to_string())))
}

pub(crate) async fn validate_and_store(
    client: &Client,
    store: &CredentialStore,
    key: ApiKey,
    cancel: &CancellationToken,
) -> Result<LoginOutcome, Error> {
    match client.account(cancel).await {
        Ok(_) => {
            store.write(&key)?;
            Ok(LoginOutcome::StoredVerified)
        }
        Err(error) if error.is_explicit_auth_rejection() => Ok(LoginOutcome::Rejected),
        Err(_) => {
            store.write(&key)?;
            Ok(LoginOutcome::StoredUnverified)
        }
    }
}

pub(crate) async fn connect_claude_plan_with<F>(
    client: &Client,
    cancel: &CancellationToken,
    open_browser: F,
    poll_interval: std::time::Duration,
    max_polls: usize,
) -> io::Result<()>
where
    F: FnOnce(&str) -> io::Result<()>,
{
    let start: estelle_client::OAuthStartResponse = client
        .post(
            estelle_client::Endpoint::OAuthStart,
            &serde_json::json!({}),
            cancel,
        )
        .await
        .map_err(io::Error::other)?;
    let authorize = Url::parse(&start.authorize_url).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "server returned an invalid OAuth URL",
        )
    })?;
    if authorize.scheme() != "https" || authorize.host_str().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "server returned a non-HTTPS OAuth URL",
        ));
    }
    open_browser(authorize.as_str())?;
    for _ in 0..max_polls {
        let account = client.account(cancel).await.map_err(io::Error::other)?;
        if account
            .extra
            .get("uses_plan")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(());
        }
        tokio::select! {
            () = cancel.cancelled() => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "OAuth login cancelled"));
            }
            () = tokio::time::sleep(poll_interval) => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "Claude sign-in did not reach the account before the bounded wait ended",
    ))
}

pub(crate) async fn run_claude_plan(client: &Client) -> io::Result<()> {
    io::stdout().write_all(
        b"Opening Claude sign-in in your browser. Estelle will wait for the server callback.\n",
    )?;
    connect_claude_plan_with(
        client,
        &CancellationToken::new(),
        |url| webbrowser::open(url).map_err(io::Error::other),
        std::time::Duration::from_secs(1),
        120,
    )
    .await?;
    io::stdout()
        .write_all(b"Claude subscription connected and read back from your Estelle account.\n")?;
    Ok(())
}

#[derive(Default)]
struct SecretBuffer(Zeroizing<String>);

impl SecretBuffer {
    fn push(&mut self, value: &str) -> usize {
        let mut count = 0;
        for character in value.chars().filter(|character| !character.is_control()) {
            self.0.push(character);
            count += 1;
        }
        count
    }

    fn pop(&mut self) -> bool {
        self.0.pop().is_some()
    }

    fn finish(self) -> Option<Zeroizing<String>> {
        if self.0.trim().is_empty() {
            return None;
        }
        Some(Zeroizing::new(self.0.trim().to_string()))
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste);
    }
}

fn read_secret_value_tty(prompt: &[u8]) -> io::Result<Option<Zeroizing<String>>> {
    let mut output = io::stdout();
    output.write_all(prompt)?;
    output.flush()?;
    enable_raw_mode()?;
    let _guard = RawModeGuard;
    execute!(output, EnableBracketedPaste)?;
    let mut buffer = SecretBuffer::default();
    let mut cancelled = false;
    loop {
        match crossterm::event::read()? {
            Event::Paste(value) => {
                let count = buffer.push(&value);
                output.write_all("*".repeat(count).as_bytes())?;
                output.flush()?;
            }
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                match key.code {
                    KeyCode::Enter => break,
                    KeyCode::Backspace if buffer.pop() => {
                        output.write_all(b"\x08 \x08")?;
                        output.flush()?;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cancelled = true;
                        break;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cancelled = true;
                        break;
                    }
                    KeyCode::Char(character) => {
                        let mut encoded = [0_u8; 4];
                        buffer.push(character.encode_utf8(&mut encoded));
                        output.write_all(b"*")?;
                        output.flush()?;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    output.write_all(b"\n")?;
    if cancelled {
        return Ok(None);
    }
    Ok(buffer.finish())
}

pub(crate) fn read_secret_value(prompt: &[u8]) -> io::Result<Option<Zeroizing<String>>> {
    if io::stdin().is_terminal() {
        read_secret_value_tty(prompt)
    } else {
        read_secret_value_line(prompt, &mut io::stdin().lock(), &mut io::stdout())
    }
}

pub(crate) fn read_plain_value(prompt: &[u8]) -> io::Result<Option<String>> {
    let mut output = io::stdout();
    output.write_all(prompt)?;
    output.flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim();
    Ok((!value.is_empty()).then(|| value.to_string()))
}

fn read_secret() -> io::Result<Option<ApiKey>> {
    read_secret_value(b"Estelle key: ")?
        .map(|value| ApiKey::new(value.to_string()))
        .transpose()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

/// Legacy ChatGPT credentials remain readable and removable so upgrading does not strand a
/// secret. New acquisition is deliberately unavailable: the inherited device flow presents
/// Codex's first-party OAuth client ID, not an Estelle-owned client.
pub(crate) fn chatgpt_auth_home() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle").join("chatgpt"))
        .ok_or_else(|| io::Error::other("could not locate the home directory for ChatGPT auth"))
}

pub(crate) fn chatgpt_credential_present() -> bool {
    let Ok(home) = chatgpt_auth_home() else {
        return false;
    };
    codex_login::load_auth_dot_json(
        &home,
        codex_login::AuthCredentialsStoreMode::File,
        codex_login::AuthKeyringBackendKind::default(),
    )
    .ok()
    .flatten()
    .and_then(|auth| auth.tokens)
    .is_some()
}

pub(crate) fn logout_chatgpt() -> io::Result<bool> {
    let home = chatgpt_auth_home()?;
    let present = chatgpt_credential_present();
    codex_login::logout(
        &home,
        codex_login::AuthCredentialsStoreMode::File,
        codex_login::AuthKeyringBackendKind::default(),
    )?;
    Ok(present)
}

pub(crate) async fn run() -> io::Result<LoginOutcome> {
    let Some(key) = read_secret()? else {
        io::stdout().write_all(b"Login cancelled. Nothing was stored.\n")?;
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "login cancelled; no credential was written",
        ));
    };
    let client = Client::production(key.clone()).map_err(io::Error::other)?;
    let store = CredentialStore::default_location().map_err(io::Error::other)?;
    let outcome = validate_and_store(&client, &store, key, &CancellationToken::new())
        .await
        .map_err(io::Error::other)?;
    let mut output = io::stdout();
    match outcome {
        LoginOutcome::StoredVerified => {
            output.write_all(b"Credential verified and stored.\n")?;
        }
        LoginOutcome::StoredUnverified => {
            output.write_all(
                b"Credential stored, but Estelle could not verify it. Retry when the service is reachable.\n",
            )?;
        }
        LoginOutcome::Rejected => {
            output.write_all(b"Estelle rejected the credential. Nothing was stored.\n")?;
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::Path;
    use std::time::Duration;

    use estelle_client::CredentialSource;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;

    #[test]
    fn provider_secret_reader_accepts_non_estelle_keys_without_echoing_them() {
        let secret = "anthropic-test-secret";
        let mut input = Cursor::new(format!("{secret}\n"));
        let mut output = Vec::new();

        let value = read_secret_value_line(b"Provider API key: ", &mut input, &mut output)
            .expect("secret input")
            .expect("non-empty secret");

        assert_eq!(value.as_str(), secret);
        let rendered = String::from_utf8(output).expect("utf-8 output");
        assert_eq!(rendered, "Provider API key: \n");
        assert!(!rendered.contains(secret));
    }

    #[test]
    fn chatgpt_tokens_do_not_land_in_the_estelle_credential_store() {
        let home = chatgpt_auth_home().expect("chatgpt home");
        let estelle_store = CredentialStore::default_location().expect("estelle store");
        assert!(
            home.ends_with(Path::new(".estelle").join("chatgpt")),
            "dedicated dir: {home:?}"
        );
        assert_ne!(
            home.join("auth.json"),
            estelle_store.path(),
            "the login crate writes <codex_home>/auth.json; passing ~/.estelle would collide with the Estelle API-key store"
        );
    }

    #[test]
    fn piped_login_input_is_captured_without_echoing_the_credential() {
        let secret = format!("estelle_live_{}", "a".repeat(24));
        let mut input = Cursor::new(format!("{secret}\n"));
        let mut screen = Vec::new();

        let key = read_secret_line(&mut input, &mut screen)
            .expect("secret read")
            .expect("key");

        assert_eq!(format!("{key:?}"), "ApiKey([REDACTED])");
        let rendered = String::from_utf8(screen).expect("screen text");
        assert!(!rendered.contains(&secret));
        assert!(rendered.contains("Estelle key"));
    }

    #[tokio::test]
    async fn login_stores_success_refuses_rejection_and_keeps_failure_to_ask() {
        for (status, expected) in [
            (200, LoginOutcome::StoredVerified),
            (401, LoginOutcome::Rejected),
            (502, LoginOutcome::StoredUnverified),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/account"))
                .respond_with(
                    ResponseTemplate::new(status).set_body_json(serde_json::json!({
                        "plan": "ultra"
                    })),
                )
                .mount(&server)
                .await;
            let client = estelle_client::Client::new(
                &format!("{}/", server.uri()),
                estelle_client::ApiKey::new(format!("estelle_live_{}", "b".repeat(24)))
                    .expect("key"),
                Duration::from_secs(120),
            )
            .expect("client");
            let home = tempdir().expect("temp home");
            let store = estelle_client::CredentialStore::new(home.path().join("auth.json"));

            let outcome = validate_and_store(
                &client,
                &store,
                estelle_client::ApiKey::new(format!("estelle_live_{}", "b".repeat(24)))
                    .expect("key"),
                &CancellationToken::new(),
            )
            .await
            .expect("login outcome");

            assert_eq!(outcome, expected);
            if status == 401 {
                assert!(!store.path().exists());
            } else {
                // `resolve()` reads ESTELLE_API_KEY / ESTELLE_KEY off the AMBIENT process
                // environment (`estelle-client/src/auth.rs:154`) and returns before ever opening
                // the file. On a machine with a key exported — the normal state for anyone using
                // the product — this asserted the environment branch instead of the login write it
                // exists to check. Fourth test in this family; drive the hermetic seam.
                assert_eq!(
                    store
                        .resolve_with_environment(None)
                        .expect("stored key")
                        .source,
                    CredentialSource::Stored
                );
            }
        }
    }

    #[tokio::test]
    async fn claude_oauth_opens_the_server_url_and_waits_for_the_persisted_account_binding() {
        use std::sync::{Arc, Mutex};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/start"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "authorize_url": "https://provider.example/oauth/authorize?state=opaque"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "uses_plan": true
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            ApiKey::new("estelle_test_key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let opened = Arc::new(Mutex::new(Vec::new()));
        let opened_by_browser = opened.clone();

        connect_claude_plan_with(
            &client,
            &CancellationToken::new(),
            move |url| {
                opened_by_browser
                    .lock()
                    .expect("browser record")
                    .push(url.to_string());
                Ok(())
            },
            Duration::ZERO,
            1,
        )
        .await
        .expect("connected plan");

        assert_eq!(
            opened.lock().expect("opened URL").as_slice(),
            ["https://provider.example/oauth/authorize?state=opaque"]
        );
    }

    #[tokio::test]
    async fn claude_oauth_refuses_a_non_https_authorize_url_before_opening_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/start"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "authorize_url": "http://provider.example/oauth/authorize"
            })))
            .mount(&server)
            .await;
        let client = Client::new(
            &format!("{}/", server.uri()),
            ApiKey::new("estelle_test_key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let opened = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let browser_called = opened.clone();

        let error = connect_claude_plan_with(
            &client,
            &CancellationToken::new(),
            move |_| {
                browser_called.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
            Duration::ZERO,
            1,
        )
        .await
        .expect_err("HTTP authorize URL must be refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!opened.load(std::sync::atomic::Ordering::SeqCst));
    }
}
