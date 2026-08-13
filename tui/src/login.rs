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

// ── ChatGPT device-code login ───────────────────────────────────────────────
// `estelle login --chatgpt`: sign in with a ChatGPT plan instead of pasting an Estelle API
// key. Device code is the default — NOT the crate's browser flow — because the browser flow
// needs a localhost callback and a display on THIS machine, and estelle runs headless (SSH,
// CI, containers) where there is none. The flow is the inherited codex-login one: request a
// user code, show it, poll, exchange, persist.
const CHATGPT_ISSUER: &str = "https://auth.openai.com";

/// Where the ChatGPT credential lives: ~/.estelle/chatgpt/ — a DEDICATED dir, never ~/.estelle
/// itself. The login crate writes <codex_home>/auth.json and the Estelle API-key store IS
/// ~/.estelle/auth.json; passing the Estelle home as codex_home would clobber one store with
/// the other.
pub(crate) fn chatgpt_auth_home() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle").join("chatgpt"))
        .ok_or_else(|| io::Error::other("could not locate the home directory for ChatGPT auth"))
}

fn chatgpt_auth_route_config() -> codex_login::AuthRouteConfig {
    codex_login::AuthRouteConfig::from_http_client_factory(
        codex_http_client::HttpClientFactory::new(
            codex_http_client::OutboundProxyPolicy::ReqwestDefault,
        ),
    )
}

fn chatgpt_server_options(codex_home: PathBuf, issuer: &str) -> codex_login::ServerOptions {
    let mut options = codex_login::ServerOptions::new(
        codex_home,
        codex_login::CLIENT_ID.to_string(),
        /*forced_chatgpt_workspace_id*/ None,
        // File storage: the credential must be inspectable on disk (the status line below is
        // its receipt), and the crate's file backend writes it 0600.
        codex_login::AuthCredentialsStoreMode::File,
        codex_login::AuthKeyringBackendKind::default(),
        chatgpt_auth_route_config(),
    );
    options.issuer = issuer.to_string();
    options.open_browser = false;
    options
}

async fn run_chatgpt_with(
    issuer: &str,
    codex_home: PathBuf,
    out: &mut impl Write,
) -> io::Result<()> {
    std::fs::create_dir_all(&codex_home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&codex_home, std::fs::Permissions::from_mode(0o700))?;
    }
    // A re-run replaces the stored credential cleanly: remove the old local auth first so no
    // stale token can survive next to the new one. Local-only on purpose — a revocation call
    // would make every re-login depend on the network, and a failed delete must not block a
    // fresh sign-in.
    if let Err(error) = codex_login::logout(
        &codex_home,
        codex_login::AuthCredentialsStoreMode::File,
        codex_login::AuthKeyringBackendKind::default(),
    ) {
        let _ = writeln!(
            out,
            "Could not clear a previous ChatGPT credential ({error}); continuing."
        );
    }
    let options = chatgpt_server_options(codex_home.clone(), issuer);
    let device_code = codex_login::request_device_code(&options).await?;
    writeln!(out, "Sign in with your ChatGPT plan:")?;
    writeln!(out, "  1. Open {}", device_code.verification_url)?;
    writeln!(
        out,
        "  2. Enter the one-time code {} (expires in 15 minutes)",
        device_code.user_code
    )?;
    writeln!(
        out,
        "Continue only if you started this login yourself. If a website or another person gave you this code, cancel."
    )?;
    out.flush()?;
    codex_login::complete_device_code_login(options, device_code).await?;
    let account = codex_login::load_auth_dot_json(
        &codex_home,
        codex_login::AuthCredentialsStoreMode::File,
        codex_login::AuthKeyringBackendKind::default(),
    )?
    .and_then(|auth| auth.tokens)
    .and_then(|tokens| tokens.account_id);
    // The receipt: WHICH credential this is and WHERE it lives — the auth-method evidence a
    // later receipt (B2) cites rather than re-derives.
    writeln!(out, "Signed in with ChatGPT (device code).")?;
    if let Some(account) = account {
        writeln!(out, "ChatGPT account: {account}")?;
    }
    writeln!(
        out,
        "ChatGPT-plan credential stored at {} (mode 0600).",
        codex_home.join("auth.json").display()
    )?;
    writeln!(out, "Auth method: chatgpt-device-code")?;
    out.flush()
}

pub(crate) async fn run_chatgpt() -> io::Result<()> {
    let mut out = io::stdout();
    let codex_home = chatgpt_auth_home()?;
    match run_chatgpt_with(CHATGPT_ISSUER, codex_home, &mut out).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = writeln!(out, "ChatGPT sign-in failed: {error}");
            Err(error)
        }
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
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use estelle_client::CredentialSource;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::Request;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    use super::*;

    fn chatgpt_jwt(account_id: &str) -> String {
        let encode = |value: serde_json::Value| {
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).expect("JWT part"))
        };
        format!(
            "{}.{}.{}",
            encode(serde_json::json!({"alg": "none", "typ": "JWT"})),
            encode(serde_json::json!({
                "https://api.openai.com/auth": {"chatgpt_account_id": account_id}
            })),
            URL_SAFE_NO_PAD.encode(b"sig")
        )
    }

    async fn mock_device_usercode(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/accounts/deviceauth/usercode"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_auth_id": "device-auth-123",
                "user_code": "CODE-12345",
                "interval": "0"
            })))
            .mount(server)
            .await;
    }

    async fn mock_device_poll(server: &MockServer) {
        let attempts = Arc::new(AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/api/accounts/deviceauth/token"))
            .respond_with(move |_: &Request| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(404)
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "authorization_code": "poll-code-321",
                        "code_challenge": "code-challenge-321",
                        "code_verifier": "code-verifier-321"
                    }))
                }
            })
            .mount(server)
            .await;
    }

    async fn mock_oauth_token(server: &MockServer, account_id: &str, access_token: &str) {
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id_token": chatgpt_jwt(account_id),
                "access_token": access_token,
                "refresh_token": "refresh-token-123"
            })))
            .mount(server)
            .await;
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

    #[tokio::test]
    async fn chatgpt_device_code_login_stores_a_chatgpt_plan_credential() {
        let server = MockServer::start().await;
        mock_device_usercode(&server).await;
        mock_device_poll(&server).await;
        mock_oauth_token(&server, "account-1", "access-token-123").await;
        let home = tempdir().expect("home");
        let mut out = Vec::new();

        run_chatgpt_with(server.uri().as_str(), home.path().to_path_buf(), &mut out)
            .await
            .expect("device-code login");

        let auth = codex_login::load_auth_dot_json(
            home.path(),
            codex_login::AuthCredentialsStoreMode::File,
            codex_login::AuthKeyringBackendKind::default(),
        )
        .expect("auth loads")
        .expect("auth.json written");
        let tokens = auth.tokens.expect("tokens");
        assert_eq!(tokens.access_token, "access-token-123");
        assert_eq!(tokens.account_id.as_deref(), Some("account-1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(home.path().join("auth.json"))
                    .expect("auth.json metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let printed = String::from_utf8(out).expect("output text");
        assert!(printed.contains("/codex/device"), "verification URL: {printed}");
        assert!(printed.contains("CODE-12345"), "user code: {printed}");
        assert!(printed.contains("ChatGPT"), "auth method named: {printed}");
        assert!(
            printed.contains(home.path().join("auth.json").to_string_lossy().as_ref()),
            "storage location named: {printed}"
        );
    }

    #[tokio::test]
    async fn chatgpt_login_rerun_replaces_the_stored_credential() {
        let home = tempdir().expect("home");
        let first = MockServer::start().await;
        mock_device_usercode(&first).await;
        mock_device_poll(&first).await;
        mock_oauth_token(&first, "account-1", "access-token-OLD").await;
        run_chatgpt_with(first.uri().as_str(), home.path().to_path_buf(), &mut Vec::new())
            .await
            .expect("first login");

        let second = MockServer::start().await;
        mock_device_usercode(&second).await;
        mock_device_poll(&second).await;
        mock_oauth_token(&second, "account-2", "access-token-NEW").await;
        run_chatgpt_with(second.uri().as_str(), home.path().to_path_buf(), &mut Vec::new())
            .await
            .expect("second login");

        let auth = codex_login::load_auth_dot_json(
            home.path(),
            codex_login::AuthCredentialsStoreMode::File,
            codex_login::AuthKeyringBackendKind::default(),
        )
        .expect("auth loads")
        .expect("auth.json written");
        let tokens = auth.tokens.expect("tokens");
        assert_eq!(tokens.access_token, "access-token-NEW");
        assert_eq!(tokens.account_id.as_deref(), Some("account-2"));
    }

    #[tokio::test]
    async fn chatgpt_login_failure_stores_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/accounts/deviceauth/usercode"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let home = tempdir().expect("home");

        let error = run_chatgpt_with(server.uri().as_str(), home.path().to_path_buf(), &mut Vec::new())
            .await
            .expect_err("a failed device-code request must fail the login");

        assert!(error.to_string().contains("device code"), "{error}");
        assert!(!home.path().join("auth.json").exists());
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
                assert_eq!(
                    store.resolve().expect("stored key").source,
                    CredentialSource::Stored
                );
            }
        }
    }
}
