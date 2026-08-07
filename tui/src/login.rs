use std::io;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::Write;

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
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoginOutcome {
    StoredVerified,
    StoredUnverified,
    Rejected,
}

pub(crate) fn read_secret_line(
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Option<ApiKey>> {
    output.write_all(b"Estelle key: ")?;
    output.flush()?;
    let mut value = Zeroizing::new(String::new());
    input.read_line(&mut value)?;
    output.write_all(b"\n")?;
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    ApiKey::new(value.to_string())
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
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

    fn finish(self) -> Result<Option<ApiKey>, Error> {
        if self.0.trim().is_empty() {
            return Ok(None);
        }
        ApiKey::new(self.0.trim().to_string()).map(Some)
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste);
    }
}

fn read_secret_tty() -> io::Result<Option<ApiKey>> {
    let mut output = io::stdout();
    output.write_all(b"Estelle key: ")?;
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
    buffer
        .finish()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn read_secret() -> io::Result<Option<ApiKey>> {
    if io::stdin().is_terminal() {
        read_secret_tty()
    } else {
        read_secret_line(&mut io::stdin().lock(), &mut io::stdout())
    }
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
                assert_eq!(
                    store.resolve().expect("stored key").source,
                    CredentialSource::Stored
                );
            }
        }
    }
}
