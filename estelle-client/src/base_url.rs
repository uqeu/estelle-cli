//! The one owner of "which Estelle API does this process talk to".
//!
//! Every client that is not handed an explicit URL — the TUI, `estelle hook <mode>`, `estelle mcp-server`,
//! `estelle acp` and the headless commands — resolves its base URL here, through
//! [`crate::Client::production`]. A second reader of [`BASE_URL_ENV`] anywhere else is a second owner of
//! one decision, and the two will disagree.
//!
//! ⚠️ AN INVALID OVERRIDE FAILS CLOSED; IT NEVER FALLS BACK TO PRODUCTION. A developer who sets
//! `ESTELLE_API_URL` to a local server and mistypes it must get a refusal, not a session that quietly
//! sends their transcript to `api.fatelabs.ca` while they believe it is going to `localhost`.

use url::Host;
use url::Url;

use crate::DEFAULT_BASE_URL;
use crate::Error;

/// Environment variable that points every Estelle client at a different API (e.g. a local server).
pub const BASE_URL_ENV: &str = "ESTELLE_API_URL";

/// Hosts that may be reached over plain `http`. Exactly these three spellings; a suffix match would
/// admit `localhost.attacker.example`.
const PLAINTEXT_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// The base URL this process uses: [`BASE_URL_ENV`] when it is set and non-empty, else production.
pub fn configured_base_url() -> Result<Url, Error> {
    resolve_base_url(std::env::var(BASE_URL_ENV).ok().as_deref())
}

/// Pure form of [`configured_base_url`]: `setting` is the raw value of [`BASE_URL_ENV`], if any.
///
/// Rules: empty or absent → [`DEFAULT_BASE_URL`]; otherwise an absolute `https` URL, or `http` only for
/// `localhost` / `127.0.0.1` / `[::1]`; no credentials, query or fragment (each would either leak or be
/// silently dropped by `Url::join`). The returned path always ends in `/` so endpoint joins append.
pub fn resolve_base_url(setting: Option<&str>) -> Result<Url, Error> {
    let raw = setting.map(str::trim).filter(|value| !value.is_empty());
    let Some(raw) = raw else {
        return Ok(Url::parse(DEFAULT_BASE_URL)?);
    };
    let mut url = Url::parse(raw)?;
    let refuse = |reason: &str| Error::DisallowedBaseUrl(format!("{BASE_URL_ENV}: {reason}"));
    let host = match url.host() {
        Some(Host::Domain(name)) => name.to_ascii_lowercase(),
        Some(Host::Ipv4(ip)) => ip.to_string(),
        Some(Host::Ipv6(ip)) => format!("[{ip}]"),
        None => return Err(refuse("the URL has no host")),
    };
    match url.scheme() {
        "https" => {}
        "http" if PLAINTEXT_HOSTS.contains(&host.as_str()) => {}
        "http" => {
            return Err(refuse(
                "plain http is allowed only for localhost or 127.0.0.1",
            ));
        }
        other => return Err(refuse(&format!("scheme {other:?} is not http(s)"))),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refuse(
            "credentials in the URL are not allowed; use ESTELLE_API_KEY",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(refuse(
            "a query or fragment would be dropped from every request",
        ));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    debug_assert!(
        url.path().ends_with('/'),
        "join() would replace the last segment"
    );
    debug_assert!(matches!(url.scheme(), "https" | "http"));
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(setting: &str) -> String {
        resolve_base_url(Some(setting))
            .expect("allowed base URL")
            .to_string()
    }

    fn refused(setting: &str) -> bool {
        matches!(
            resolve_base_url(Some(setting)),
            Err(Error::DisallowedBaseUrl(_) | Error::InvalidBaseUrl(_))
        )
    }

    #[test]
    fn absent_or_blank_is_production() {
        assert_eq!(resolve_base_url(None).unwrap().as_str(), DEFAULT_BASE_URL);
        assert_eq!(
            resolve_base_url(Some("")).unwrap().as_str(),
            DEFAULT_BASE_URL
        );
        assert_eq!(
            resolve_base_url(Some("  ")).unwrap().as_str(),
            DEFAULT_BASE_URL
        );
    }

    #[test]
    fn local_http_is_allowed_and_gains_a_trailing_slash() {
        assert_eq!(ok("http://localhost:8787"), "http://localhost:8787/");
        assert_eq!(ok("http://127.0.0.1:8787/"), "http://127.0.0.1:8787/");
        assert_eq!(ok("http://[::1]:8787"), "http://[::1]:8787/");
        assert_eq!(ok("http://LOCALHOST:8787"), "http://localhost:8787/");
    }

    #[test]
    fn https_anywhere_is_allowed_and_a_path_prefix_survives_join() {
        let url = resolve_base_url(Some("https://staging.example.com/api")).unwrap();
        assert_eq!(url.as_str(), "https://staging.example.com/api/");
        assert_eq!(
            url.join("session/events").unwrap().as_str(),
            "https://staging.example.com/api/session/events"
        );
    }

    #[test]
    fn remote_plain_http_is_refused() {
        assert!(refused("http://api.fatelabs.ca/"));
        assert!(refused("http://localhost.attacker.example:8787"));
        assert!(refused("http://127.0.0.2:8787"));
        assert!(refused("http://10.0.0.1:8787"));
    }

    #[test]
    fn non_http_schemes_credentials_query_and_garbage_are_refused() {
        assert!(refused("ftp://localhost/"));
        assert!(refused("file:///tmp/x"));
        assert!(refused("https://user:pass@example.com/"));
        assert!(refused("https://example.com/?key=1"));
        assert!(refused("https://example.com/#frag"));
        assert!(refused("localhost:8787"));
        assert!(refused("not a url"));
    }
}
