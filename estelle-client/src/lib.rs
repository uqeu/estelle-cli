#![deny(clippy::print_stderr, clippy::print_stdout)]

mod auth;
mod auth_record;
mod endpoint;
mod repo;
pub mod secret_engine;
mod types;

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub use auth_record::AuthRecord;
pub use endpoint::API_ENDPOINTS;
pub use endpoint::Endpoint;
pub use endpoint::HttpMethod;
use futures::StreamExt;
pub use repo::Repo;
pub use repo::RepoResolver;
pub use repo::is_repository;
pub use repo::repo_from_remote_url;
use reqwest::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
pub use types::*;
use url::Url;
use zeroize::Zeroizing;

pub const DEFAULT_BASE_URL: &str = "https://api.fatelabs.ca/";
pub const MINIMUM_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// Runtime contract understood by this client. The server advertises its accepted minimum/current range
/// on every response; enforcement deliberately waits for the product's compatibility policy decision.
pub const CLIENT_PROTOCOL_VERSION: u64 = 1;
/// Cross-repo Python↔Rust hook fixture/schema contract understood by this binary.
pub const HOOK_CONTRACT_VERSION: u64 = 1;

/// How many times a single request may be sent when the server asks us to come back later.
///
/// 🔑 A FIXED, STATED BOUND (Power of Ten rule 2). The server sheds a dependency-bound route in
/// `api.py:_guard` — **before** the handler dispatches, so the shed request had no effect and a
/// second attempt is not a re-execution. That makes retrying safe even for POST, which is exactly
/// why the bound has to be explicit: the only thing stopping an unbounded loop is this number.
pub const SHED_MAX_ATTEMPTS: u32 = 3;

/// The longest advertised cooldown this client will actually wait out.
///
/// Past this the request is NOT retried and the server's own 503 goes to the caller. Waiting an
/// arbitrary interval because a header said so would turn a fast, legible failure into a hang.
pub const SHED_MAX_WAIT: Duration = Duration::from_secs(30);

/// The single owner of "did the server ask us to come back, and may we?".
///
/// Returns the interval to wait, or `None` when this response must be surfaced as-is. `None` is the
/// default for everything uncertain — an unparseable header, a zero, an interval past
/// [`SHED_MAX_WAIT`], or any status the server did not pair with an interval.
///
/// ⚠️ The status alone is never enough. Retrying every 503 would have this client hammer a server
/// that never advertised a cooldown; the HEADER is the authorisation, and `tests.rs`
/// pins that with a negative control.
fn shed_delay_for(status: reqwest::StatusCode, retry_after: Option<&str>) -> Option<Duration> {
    if status != reqwest::StatusCode::SERVICE_UNAVAILABLE
        && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return None;
    }
    let seconds: u64 = retry_after?.trim().parse().ok()?;
    if seconds == 0 {
        return None;
    }
    let wait = Duration::from_secs(seconds);
    if wait > SHED_MAX_WAIT {
        None
    } else {
        Some(wait)
    }
}

#[derive(Clone)]
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::EmptyApiKey);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    fn expose(&self) -> &str {
        self.0.as_str()
    }

    /// Build the complete bearer value for a local client configuration. The returned
    /// allocation is zeroized on drop and never participates in `Debug` output.
    pub fn bearer_header_value(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("Bearer {}", self.expose()))
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey([REDACTED])")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the API key is empty")]
    EmptyApiKey,
    #[error("no Estelle credential is configured")]
    NoCredential,
    #[error("Estelle credential storage failed: {0}")]
    CredentialIo(#[from] std::io::Error),
    #[error("Estelle credential file is malformed")]
    MalformedCredential,
    #[error("Estelle credential file mode is {mode:04o}; expected 0600 or stricter")]
    InsecureCredentialPermissions { mode: u32 },
    #[error("Estelle credential storage failed: {0}")]
    CredentialStore(String),
    #[error("the HTTP timeout must be at least 120 seconds")]
    TimeoutTooShort,
    #[error("invalid Estelle API base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),
    #[error("request cancelled")]
    Cancelled,
    #[error("{endpoint:?} does not support {method}")]
    UnsupportedMethod {
        endpoint: Endpoint,
        method: &'static str,
    },
    #[error("{0:?} requires a repository scope")]
    RepoRequired(Endpoint),
    #[error("{0:?} does not accept a repository scope")]
    RepoNotAccepted(Endpoint),
    #[error("request body must serialize to a JSON object")]
    BodyMustBeObject,
    #[error("request query must be a flat object of scalar values")]
    QueryMustBeFlat,
    #[error("Estelle request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Estelle JSON contract failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Estelle returned HTTP {status}: {message}")]
    Http {
        status: reqwest::StatusCode,
        message: String,
    },
    #[error("Estelle returned an empty response")]
    EmptyResponse,
    #[error("invalid durable job id")]
    InvalidJobId,
    #[error("Estelle returned an invalid durable job progress stream")]
    InvalidProgressStream,
    #[error("HTTP receipt failed: {0}")]
    ReceiptIo(String),
}

impl Error {
    pub fn is_explicit_auth_rejection(&self) -> bool {
        matches!(
            self,
            Self::Http { status, .. }
                if matches!(
                    *status,
                    reqwest::StatusCode::UNAUTHORIZED
                        | reqwest::StatusCode::FORBIDDEN
                        | reqwest::StatusCode::NOT_FOUND
                )
        )
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
    api_key: ApiKey,
    receipt_path: Option<Arc<PathBuf>>,
}

impl Client {
    pub fn production(api_key: ApiKey) -> Result<Self, Error> {
        Self::new(DEFAULT_BASE_URL, api_key, DEFAULT_TIMEOUT)
    }

    pub fn new(base_url: &str, api_key: ApiKey, timeout: Duration) -> Result<Self, Error> {
        if timeout < MINIMUM_TIMEOUT {
            return Err(Error::TimeoutTooShort);
        }
        let base_url = Url::parse(base_url)?;
        let http = reqwest::Client::builder().timeout(timeout).build()?;
        // 🔴 THE RECEIPT PATH IS READ HERE, AT THE ONE CONSTRUCTOR EVERYTHING FUNNELS THROUGH.
        //
        // It used to be read in `production()` only. The TUI calls `production()` twice and `new()`
        // thirteen times — `session_server.rs` alone builds a client six times — so almost every
        // request the app made wrote no receipt, and the public-binary probe reported **"not
        // observed" for all 26 routes it asserts, 29 contracts, while the same session was visibly
        // pulling a 275-file repo graph**. The calls happened; nothing recorded them.
        //
        // That is this repo's own rule about a guard reachable only from the path you remembered to
        // instrument: it is a guard on that path, not on the system. `production()` delegates here,
        // so there is now exactly one place that decides, and no constructor can miss it.
        let receipt_path = std::env::var_os("ESTELLE_RECEIPT_PATH")
            .filter(|path| !path.is_empty())
            .map(|path| Arc::new(PathBuf::from(path)));
        Ok(Self {
            http,
            base_url,
            api_key,
            receipt_path,
        })
    }

    pub fn with_receipt_path(mut self, path: PathBuf) -> Self {
        self.receipt_path = Some(Arc::new(path));
        self
    }

    pub async fn account(&self, cancel: &CancellationToken) -> Result<AccountResponse, Error> {
        self.get(Endpoint::Account, &NoQuery, cancel).await
    }

    pub async fn overview(&self, cancel: &CancellationToken) -> Result<OverviewResponse, Error> {
        self.get(Endpoint::Overview, &NoQuery, cancel).await
    }

    pub async fn repos(&self, cancel: &CancellationToken) -> Result<ReposResponse, Error> {
        self.get(Endpoint::Repos, &NoQuery, cancel).await
    }

    /// Read one caller-bound durable job. The strict locator shape prevents a job id from
    /// becoming a path/query injection surface; the bearer credential remains the authority.
    pub async fn job(
        &self,
        job_id: &str,
        cancel: &CancellationToken,
    ) -> Result<JobSnapshot, Error> {
        let suffix = job_id.strip_prefix("job_").ok_or(Error::InvalidJobId)?;
        if suffix.len() != 24
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::InvalidJobId);
        }
        let request_path = format!("jobs/{job_id}");
        let url = self.base_url.join(&request_path)?;
        let request = self
            .http
            .get(url)
            .bearer_auth(self.api_key.expose())
            .header(
                "X-Estelle-Client-Protocol",
                CLIENT_PROTOCOL_VERSION.to_string(),
            )
            .header("X-Estelle-Hook-Contract", HOOK_CONTRACT_VERSION.to_string())
            .header("X-Estelle-Client-Version", env!("CARGO_PKG_VERSION"));
        let response = self.send_honoring_shed(request, cancel).await?;
        let status = response.status();
        let bytes = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            bytes = response.bytes() => bytes?,
        };
        self.write_receipt_path(
            &Method::GET,
            &format!("/{request_path}"),
            &[],
            None,
            status,
            &bytes,
        )
        .await?;
        if !status.is_success() {
            return Err(Error::Http {
                status,
                message: error_message(&bytes),
            });
        }
        if bytes.is_empty() {
            return Err(Error::EmptyResponse);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Watch one caller-bound durable job as changed phase snapshots arrive over NDJSON.
    /// Heartbeats carry no phase and are deliberately ignored; only server-read durable facts reach
    /// `on_snapshot`. The terminal snapshot is returned as the command receipt.
    pub async fn stream_job<F>(
        &self,
        job_id: &str,
        cancel: &CancellationToken,
        mut on_snapshot: F,
    ) -> Result<JobSnapshot, Error>
    where
        F: FnMut(&JobSnapshot),
    {
        let suffix = job_id.strip_prefix("job_").ok_or(Error::InvalidJobId)?;
        if suffix.len() != 24
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::InvalidJobId);
        }
        let request_path = format!("jobs/{job_id}/events");
        let url = self.base_url.join(&request_path)?;
        let request = self
            .http
            .get(url)
            .bearer_auth(self.api_key.expose())
            .header(
                "X-Estelle-Client-Protocol",
                CLIENT_PROTOCOL_VERSION.to_string(),
            )
            .header("X-Estelle-Hook-Contract", HOOK_CONTRACT_VERSION.to_string())
            .header("X-Estelle-Client-Version", env!("CARGO_PKG_VERSION"));
        let response = self.send_honoring_shed(request, cancel).await?;
        let status = response.status();
        if !status.is_success() {
            let bytes = tokio::select! {
                () = cancel.cancelled() => return Err(Error::Cancelled),
                bytes = response.bytes() => bytes?,
            };
            return Err(Error::Http {
                status,
                message: error_message(&bytes),
            });
        }

        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        let mut receipt = Vec::new();
        while let Some(chunk) = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            chunk = stream.next() => chunk,
        } {
            let chunk = chunk?;
            receipt.extend_from_slice(&chunk);
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
                let line = buffer.drain(..=end).collect::<Vec<_>>();
                let line = &line[..line.len().saturating_sub(1)];
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let event: Value = serde_json::from_slice(line)?;
                let kind = event
                    .get("event")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if kind == "heartbeat" {
                    continue;
                }
                if !matches!(kind, "progress" | "complete") {
                    return Err(Error::InvalidProgressStream);
                }
                let snapshot: JobSnapshot = serde_json::from_value(
                    event
                        .get("snapshot")
                        .cloned()
                        .ok_or(Error::InvalidProgressStream)?,
                )?;
                if (kind == "complete") != snapshot.terminal {
                    return Err(Error::InvalidProgressStream);
                }
                on_snapshot(&snapshot);
                if kind == "complete" {
                    self.write_receipt_path(
                        &Method::GET,
                        &format!("/{request_path}"),
                        &[],
                        None,
                        status,
                        &receipt,
                    )
                    .await?;
                    return Ok(snapshot);
                }
            }
        }
        Err(Error::InvalidProgressStream)
    }

    pub async fn github_status(
        &self,
        cancel: &CancellationToken,
    ) -> Result<GithubStatusResponse, Error> {
        self.get(Endpoint::GithubStatus, &NoQuery, cancel).await
    }

    pub async fn proposed_prs(
        &self,
        query: &ProposedPrsQuery,
        cancel: &CancellationToken,
    ) -> Result<ProposedPrsResponse, Error> {
        self.get(Endpoint::ProposedPrs, query, cancel).await
    }

    pub async fn chat_completion(
        &self,
        repo: &Repo,
        request: &ChatCompletionRequest,
        cancel: &CancellationToken,
    ) -> Result<ChatCompletionResponse, Error> {
        self.send(
            Method::POST,
            Endpoint::ChatCompletions,
            Some(request),
            None::<&NoQuery>,
            Some(repo),
            cancel,
        )
        .await
    }

    pub async fn deep_search(
        &self,
        repo: &Repo,
        request: &DeepSearchRequest,
        cancel: &CancellationToken,
    ) -> Result<DeepSearchResponse, Error> {
        self.post_scoped(Endpoint::DeepSearch, repo, request, cancel)
            .await
    }

    pub async fn suite_dispatch(
        &self,
        request: &SuiteDispatchRequest,
        cancel: &CancellationToken,
    ) -> Result<SuiteDispatchResponse, Error> {
        self.post(Endpoint::TurnRoute, request, cancel).await
    }

    pub async fn route_within_plan(
        &self,
        repo: &Repo,
        request: &PlanRouteRequest,
        cancel: &CancellationToken,
    ) -> Result<PlanRouteResponse, Error> {
        self.post_scoped(Endpoint::Route, repo, request, cancel)
            .await
    }

    pub async fn orchestra_run(
        &self,
        repo: &Repo,
        request: &OrchestraRunRequest,
        cancel: &CancellationToken,
    ) -> Result<OrchestraRunResponse, Error> {
        self.post_scoped(Endpoint::OrchestraRun, repo, request, cancel)
            .await
    }

    pub async fn orchestra_status(
        &self,
        repo: &Repo,
        query: &OrchestraStatusQuery,
        cancel: &CancellationToken,
    ) -> Result<OrchestraStatusResponse, Error> {
        self.get_scoped(Endpoint::OrchestraStatus, repo, query, cancel)
            .await
    }

    pub async fn get<Q, R>(
        &self,
        endpoint: Endpoint,
        query: &Q,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        Q: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.send(
            Method::GET,
            endpoint,
            None::<&NoBody>,
            Some(query),
            None,
            cancel,
        )
        .await
    }

    pub async fn get_scoped<Q, R>(
        &self,
        endpoint: Endpoint,
        repo: &Repo,
        query: &Q,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        Q: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.send(
            Method::GET,
            endpoint,
            None::<&NoBody>,
            Some(query),
            Some(repo),
            cancel,
        )
        .await
    }

    pub async fn post<B, R>(
        &self,
        endpoint: Endpoint,
        body: &B,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.send(
            Method::POST,
            endpoint,
            Some(body),
            None::<&NoQuery>,
            None,
            cancel,
        )
        .await
    }

    pub async fn post_scoped<B, R>(
        &self,
        endpoint: Endpoint,
        repo: &Repo,
        body: &B,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.send(
            Method::POST,
            endpoint,
            Some(body),
            None::<&NoQuery>,
            Some(repo),
            cancel,
        )
        .await
    }

    pub async fn put<B, R>(
        &self,
        endpoint: Endpoint,
        body: &B,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        B: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.send(
            Method::PUT,
            endpoint,
            Some(body),
            None::<&NoQuery>,
            None,
            cancel,
        )
        .await
    }

    async fn send<B, Q, R>(
        &self,
        method: Method,
        endpoint: Endpoint,
        body: Option<&B>,
        query: Option<&Q>,
        repo: Option<&Repo>,
        cancel: &CancellationToken,
    ) -> Result<R, Error>
    where
        B: Serialize + ?Sized,
        Q: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        endpoint.validate_method(&method)?;
        match (endpoint.requires_repo(), repo) {
            (true, None) => return Err(Error::RepoRequired(endpoint)),
            (false, Some(_)) if endpoint != Endpoint::ChatCompletions => {
                return Err(Error::RepoNotAccepted(endpoint));
            }
            _ => {}
        }

        let url = self.base_url.join(endpoint.path())?;
        let mut receipt_query = Vec::new();
        let mut receipt_body = None;
        let mut request = self
            .http
            .request(method.clone(), url)
            .bearer_auth(self.api_key.expose())
            .header(
                "X-Estelle-Client-Protocol",
                CLIENT_PROTOCOL_VERSION.to_string(),
            )
            .header("X-Estelle-Hook-Contract", HOOK_CONTRACT_VERSION.to_string())
            .header("X-Estelle-Client-Version", env!("CARGO_PKG_VERSION"));
        if let Some(query) = query {
            let pairs = query_pairs(query)?;
            request = request.query(&pairs);
            receipt_query = pairs;
        }
        if endpoint.requires_repo() && endpoint != Endpoint::ChatCompletions && body.is_none() {
            let repo = repo.ok_or(Error::RepoRequired(endpoint))?;
            request = request.query(&[("repo", repo.as_str())]);
        }
        if let Some(body) = body {
            let mut value = serde_json::to_value(body)?;
            if endpoint.requires_repo() && endpoint != Endpoint::ChatCompletions {
                let object = value.as_object_mut().ok_or(Error::BodyMustBeObject)?;
                let repo = repo.ok_or(Error::RepoRequired(endpoint))?;
                object.insert(
                    "repo".to_string(),
                    serde_json::Value::String(repo.to_string()),
                );
            }
            receipt_body = Some(value.clone());
            request = request.json(&value);
        }
        if endpoint == Endpoint::ChatCompletions {
            let repo = repo.ok_or(Error::RepoRequired(endpoint))?;
            request = request.header("X-Estelle-Repo", repo.as_str());
        }

        let response = self.send_honoring_shed(request, cancel).await?;
        let status = response.status();
        let bytes = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            bytes = response.bytes() => bytes?,
        };
        self.write_receipt(
            &method,
            endpoint,
            &receipt_query,
            receipt_body,
            status,
            &bytes,
        )
        .await?;
        if !status.is_success() {
            return Err(Error::Http {
                status,
                message: error_message(&bytes),
            });
        }
        if bytes.is_empty() {
            return Err(Error::EmptyResponse);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Send `request`, honouring a server-advertised cooldown up to [`SHED_MAX_ATTEMPTS`] times.
    ///
    /// 🔴 THE DEFECT THIS CLOSES. `estelle sweep` met `503 dependency slow-path cooldown; retry
    /// after the advertised interval` and exited 1 — the server behaving exactly as designed read
    /// to the user as a hard failure. The Python client already honours `Retry-After`
    /// (`serve/backend.py:585`); this one ignored it, and eight further receipt contracts went
    /// unobserved downstream of that one exit.
    ///
    /// The last response is returned even when every attempt was shed, so an unrelenting cooldown
    /// still surfaces as the server's own 503 and is never swallowed into a fake success.
    async fn send_honoring_shed(
        &self,
        request: reqwest::RequestBuilder,
        cancel: &CancellationToken,
    ) -> Result<reqwest::Response, Error> {
        let mut pending = request;
        let mut attempt: u32 = 1;
        loop {
            // `try_clone` returns None only for a streaming body; every request this client builds
            // carries a JSON or empty body. A None here means "cannot retry", never a panic.
            let next = pending.try_clone();
            let response = tokio::select! {
                () = cancel.cancelled() => return Err(Error::Cancelled),
                response = pending.send() => response?,
            };
            let advertised = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let Some(wait) = shed_delay_for(response.status(), advertised.as_deref()) else {
                return Ok(response);
            };
            let (Some(retryable), true) = (next, attempt < SHED_MAX_ATTEMPTS) else {
                return Ok(response);
            };
            tokio::select! {
                () = cancel.cancelled() => return Err(Error::Cancelled),
                () = tokio::time::sleep(wait) => {}
            }
            pending = retryable;
            attempt += 1;
        }
    }

    async fn write_receipt(
        &self,
        method: &Method,
        endpoint: Endpoint,
        query: &[(String, String)],
        body: Option<Value>,
        status: reqwest::StatusCode,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.write_receipt_path(
            method,
            &format!("/{}", endpoint.path()),
            query,
            body,
            status,
            bytes,
        )
        .await
    }

    async fn write_receipt_path(
        &self,
        method: &Method,
        request_path: &str,
        query: &[(String, String)],
        body: Option<Value>,
        status: reqwest::StatusCode,
        bytes: &[u8],
    ) -> Result<(), Error> {
        let Some(path) = &self.receipt_path else {
            return Ok(());
        };
        let response = serde_json::from_slice(bytes)
            .unwrap_or_else(|_| serde_json::json!({"non_json_bytes": bytes.len()}));
        let value = redact_receipt_value(serde_json::json!({
            "request": {
                "method": method.as_str(),
                "path": request_path,
                "query": query,
                "body": body,
            },
            "response": {"status": status.as_u16(), "body": response},
        }));
        let mut line = serde_json::to_vec(&value).map_err(Error::Json)?;
        line.push(b'\n');
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .await
            .map_err(|error| Error::ReceiptIo(error.to_string()))?;
        file.write_all(&line)
            .await
            .map_err(|error| Error::ReceiptIo(error.to_string()))?;
        file.sync_data()
            .await
            .map_err(|error| Error::ReceiptIo(error.to_string()))
    }
}

fn redact_receipt_value(value: Value) -> Value {
    match value {
        Value::String(value) => Value::String(redact_secrets(&value)),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_receipt_value).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    let sensitive = matches!(
                        lower.as_str(),
                        "authorization" | "credential" | "secret" | "token"
                    ) || lower.ends_with("_key")
                        || lower.ends_with("_token");
                    let value = if sensitive {
                        Value::String("[credential hidden]".to_string())
                    } else {
                        redact_receipt_value(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        value => value,
    }
}

fn query_pairs<Q>(query: &Q) -> Result<Vec<(String, String)>, Error>
where
    Q: Serialize + ?Sized,
{
    let value = serde_json::to_value(query)?;
    let Value::Object(object) = value else {
        return if value.is_null() {
            Ok(Vec::new())
        } else {
            Err(Error::QueryMustBeFlat)
        };
    };
    let mut pairs = Vec::new();
    for (key, value) in object {
        match value {
            Value::Null => {}
            Value::Array(values) => {
                for value in values {
                    pairs.push((key.clone(), query_scalar(value)?));
                }
            }
            value => pairs.push((key, query_scalar(value)?)),
        }
    }
    Ok(pairs)
}

fn query_scalar(value: Value) -> Result<String, Error> {
    match value {
        Value::String(value) => Ok(value),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err(Error::QueryMustBeFlat),
    }
}

fn error_message(bytes: &[u8]) -> String {
    serde_json::from_slice::<ErrorEnvelope>(bytes)
        .ok()
        .map(|body| body.error.message)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| "the server returned a non-Estelle error body".to_string())
}

#[derive(Serialize)]
struct NoBody;

#[cfg(test)]
mod tests;
pub use auth::CredentialSource;
pub use auth::CredentialStore;
pub use auth::ResolvedCredential;
pub use auth::find_secret_shape;
pub use auth::is_secret_shaped;
pub use auth::mask_secret;
pub use auth::redact_secrets;
pub use secret_engine::SecretFinding;
pub use secret_engine::find_secret_shapes;
pub use secret_engine::redact_secrets_engine;
