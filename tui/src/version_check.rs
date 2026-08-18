//! Tell the user when their CLI is behind. Never install anything.
//!
//! The install path exists "so it constantly makes sure you update it". This
//! module is the honest half of that promise: it learns the newest published
//! release, says so when the running binary is older, and prints the one
//! command that fixes it. It never replaces the binary and never shells out —
//! a program that silently rewrites itself is exactly the trust violation this
//! product exists to prevent. Tell, then let them run it.
//!
//! Three states, and the third is load-bearing: a check that could not reach
//! the network reports [`Status::Unknown`], never [`Status::UpToDate`]. "I
//! could not answer" and "you are current" are opposite claims, and collapsing
//! them would make this module report green over the case it exists to catch.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;

/// The one published install command. This string is duplicated by design in
/// `web/lib/cli.ts` (`CLI_INSTALL`) in the `estelle` repo — two languages, two
/// repos, no shared constant possible. `installer_url_is_the_release_asset`
/// below pins this half; the web half is pinned by its own test. If you change
/// one, change both, and keep the release asset as the canonical source.
pub const INSTALL_COMMAND: &str = "curl --proto '=https' --tlsv1.2 -fsSL \\\n  https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh | sh";

/// Where the newest published release is announced.
const RELEASE_API: &str = "https://api.github.com/repos/uqeu/estelle-cli/releases/latest";

/// Ask GitHub at most once a day. Unauthenticated API calls are rate limited
/// per IP, so an uncached check on every invocation would break itself.
const CHECK_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Declared before the call, not after it. A version check must never be the
/// reason a command feels slow.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(3);

/// A release payload has no business being larger than this. A capped read
/// means "cannot answer", never "that is all there is" — see [`fetch_latest`].
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Set to any non-empty value to silence the check entirely.
const OPT_OUT_ENV: &str = "ESTELLE_NO_VERSION_CHECK";

const CACHE_FILENAME: &str = "version-check.json";

/// A three-component release version. Pre-release suffixes are deliberately not
/// modelled: the installer only accepts `vX.Y.Z` tags, so anything else is not
/// a version this CLI can be behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The outcome of a check. [`Status::Unknown`] is not a failure to report — it
/// is the honest answer when the newest release could not be established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    UpToDate,
    Behind { running: Version, latest: Version },
    Unknown,
}

#[derive(Debug, Deserialize, Serialize)]
struct Cache {
    checked_at_unix: u64,
    latest_tag: String,
}

#[derive(Deserialize)]
struct ReleasePayload {
    tag_name: String,
}

/// Parse `v0.2.20` or `0.2.20`. Returns `None` for anything that is not
/// exactly three numeric components, so a garbage or HTML body can never be
/// mistaken for a version.
pub fn parse_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim();
    let body = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let mut parts = body.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

/// The version this binary was built as.
pub fn running_version() -> Option<Version> {
    parse_version(env!("CARGO_PKG_VERSION"))
}

/// Pure comparison. Extracted so the decision is testable without a network,
/// a clock, or a filesystem.
pub fn compare(running: Option<Version>, latest: Option<Version>) -> Status {
    let (Some(running), Some(latest)) = (running, latest) else {
        return Status::Unknown;
    };
    if running < latest {
        Status::Behind { running, latest }
    } else {
        Status::UpToDate
    }
}

/// The message a user sees. Returns `None` for every state except `Behind` —
/// silence is correct when we are current, and silence is *required* when we
/// do not know, because a reassuring line over an unknown is a lie.
pub fn notice(status: Status) -> Option<String> {
    let Status::Behind { running, latest } = status else {
        return None;
    };
    debug_assert!(running < latest, "Behind must mean running < latest");
    Some(format!(
        "estelle {running} is behind {latest}. Update with:\n\n  {INSTALL_COMMAND}\n"
    ))
}

fn cache_path(home: &Path) -> PathBuf {
    home.join(CACHE_FILENAME)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read the cached answer when it is still inside [`CHECK_TTL`]. A cache from
/// the future (clock moved backwards) is treated as expired rather than
/// trusted forever.
fn cached_latest(home: &Path, now: u64) -> Option<Version> {
    let raw = std::fs::read_to_string(cache_path(home)).ok()?;
    if raw.len() > MAX_BODY_BYTES {
        return None;
    }
    let cache: Cache = serde_json::from_str(&raw).ok()?;
    let age = now.checked_sub(cache.checked_at_unix)?;
    if age > CHECK_TTL.as_secs() {
        return None;
    }
    parse_version(&cache.latest_tag)
}

fn store_latest(home: &Path, tag: &str, now: u64) {
    let cache = Cache {
        checked_at_unix: now,
        latest_tag: tag.to_string(),
    };
    let Ok(encoded) = serde_json::to_string(&cache) else {
        return;
    };
    let _ = std::fs::create_dir_all(home);
    let _ = std::fs::write(cache_path(home), encoded);
}

/// Ask GitHub for the newest release tag. Every failure path returns `None`,
/// which becomes [`Status::Unknown`] — never a claim of currency.
async fn fetch_latest() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(NETWORK_TIMEOUT)
        .user_agent(concat!("estelle-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;
    let response = client.get(RELEASE_API).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    let payload: ReleasePayload = serde_json::from_str(&body).ok()?;
    Some(payload.tag_name)
}

/// The automatic check that runs before a subcommand. Honours the opt-out and
/// the once-a-day cache. Never panics, never blocks longer than
/// [`NETWORK_TIMEOUT`], never writes outside `home`.
pub async fn check(home: &Path) -> Status {
    if std::env::var_os(OPT_OUT_ENV).is_some_and(|v| !v.is_empty()) {
        return Status::Unknown;
    }
    run(home, true).await
}

/// The explicit check behind `estelle upgrade --check`. Ignores both the cache
/// and the opt-out: the user just asked in so many words, and answering from a
/// day-old file would not be answering the question they asked.
pub async fn check_ignoring_cache(home: &Path) -> Status {
    run(home, false).await
}

async fn run(home: &Path, use_cache: bool) -> Status {
    let now = now_unix();
    if use_cache && let Some(latest) = cached_latest(home, now) {
        return compare(running_version(), Some(latest));
    }
    let Some(tag) = fetch_latest().await else {
        return Status::Unknown;
    };
    let latest = parse_version(&tag);
    if latest.is_some() {
        store_latest(home, &tag, now);
    }
    compare(running_version(), latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn parses_tagged_and_bare_versions() {
        assert_eq!(parse_version("v0.2.20"), Some(v(0, 2, 20)));
        assert_eq!(parse_version("0.2.20"), Some(v(0, 2, 20)));
        assert_eq!(parse_version("  v1.10.3\n"), Some(v(1, 10, 3)));
    }

    #[test]
    fn rejects_everything_that_is_not_three_numbers() {
        // A rate-limit body, an HTML error page and a four-part tag must all
        // fail to parse rather than becoming a version we compare against.
        for bad in [
            "",
            "latest",
            "v0.2",
            "0.2.20.1",
            "v0.2.x",
            "<!doctype html>",
            "{\"message\":\"API rate limit exceeded\"}",
            "v0.2.20-rc1",
        ] {
            assert_eq!(parse_version(bad), None, "should not parse: {bad}");
        }
    }

    #[test]
    fn orders_by_component_not_lexically() {
        // "0.2.9" > "0.2.20" lexically; the whole point is that it is not.
        assert!(v(0, 2, 9) < v(0, 2, 20));
        assert!(v(0, 10, 0) > v(0, 9, 99));
        assert!(v(1, 0, 0) > v(0, 999, 999));
    }

    #[test]
    fn behind_only_when_strictly_older() {
        assert_eq!(
            compare(Some(v(0, 2, 20)), Some(v(0, 2, 21))),
            Status::Behind {
                running: v(0, 2, 20),
                latest: v(0, 2, 21)
            }
        );
        assert_eq!(
            compare(Some(v(0, 2, 21)), Some(v(0, 2, 21))),
            Status::UpToDate
        );
    }

    #[test]
    fn a_newer_local_build_is_not_behind() {
        // The workspace is routinely bumped ahead of the published release.
        // That developer must not be nagged to "update" to an older version.
        assert_eq!(
            compare(Some(v(0, 2, 21)), Some(v(0, 2, 20))),
            Status::UpToDate
        );
    }

    #[test]
    fn an_unanswerable_check_is_unknown_never_uptodate() {
        // The load-bearing assertion of this module.
        assert_eq!(compare(Some(v(0, 2, 20)), None), Status::Unknown);
        assert_eq!(compare(None, Some(v(0, 2, 20))), Status::Unknown);
        assert_eq!(compare(None, None), Status::Unknown);
    }

    #[test]
    fn only_behind_produces_a_message() {
        assert_eq!(notice(Status::UpToDate), None);
        assert_eq!(notice(Status::Unknown), None);
        let message = notice(Status::Behind {
            running: v(0, 2, 20),
            latest: v(0, 2, 21),
        })
        .expect("behind must speak");
        assert!(message.contains("0.2.20 is behind 0.2.21"), "{message}");
        assert!(message.contains(INSTALL_COMMAND), "{message}");
    }

    #[test]
    fn the_notice_never_offers_to_run_the_update_itself() {
        let message = notice(Status::Behind {
            running: v(0, 2, 20),
            latest: v(0, 2, 21),
        })
        .expect("behind must speak");
        // Tell, then let them run it. If this ever reads like a prompt to
        // auto-apply, the consent property has been lost.
        for forbidden in ["[y/N]", "Installing", "Updating now", "press enter"] {
            assert!(
                !message.contains(forbidden),
                "must not auto-apply: {message}"
            );
        }
    }

    #[test]
    fn installer_url_is_the_release_asset_and_is_tls_pinned() {
        // Task 3's decision, pinned. The release asset is canonical because a
        // human cuts the release; raw.githubusercontent.com/main ships the
        // instant someone pushes, with no gate and no rollback point.
        assert!(
            INSTALL_COMMAND.contains(
                "https://github.com/uqeu/estelle-cli/releases/latest/download/install.sh"
            )
        );
        assert!(!INSTALL_COMMAND.contains("raw.githubusercontent.com"));
        assert!(INSTALL_COMMAND.contains("--proto '=https'"));
        assert!(INSTALL_COMMAND.contains("--tlsv1.2"));
    }

    #[test]
    fn running_version_is_parseable() {
        // If the workspace version ever stops being vX.Y.Z, this check goes
        // permanently Unknown and silently stops working. Fail loudly instead.
        assert!(running_version().is_some(), "CARGO_PKG_VERSION must parse");
    }

    #[test]
    fn cache_round_trips_and_expires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        let now = 1_000_000u64;
        store_latest(home, "v0.2.20", now);
        assert_eq!(cached_latest(home, now), Some(v(0, 2, 20)));
        // Inside the window.
        assert_eq!(
            cached_latest(home, now + CHECK_TTL.as_secs()),
            Some(v(0, 2, 20))
        );
        // Past it.
        assert_eq!(cached_latest(home, now + CHECK_TTL.as_secs() + 1), None);
    }

    #[test]
    fn a_backwards_clock_expires_the_cache_instead_of_trusting_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        store_latest(home, "v0.2.20", 1_000_000);
        // now < checked_at: checked_sub returns None, so we re-check rather
        // than treating a future-stamped cache as eternally fresh.
        assert_eq!(cached_latest(home, 999_999), None);
    }

    #[test]
    fn a_corrupt_cache_is_ignored_not_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        std::fs::write(cache_path(home), "{ not json").expect("write");
        assert_eq!(cached_latest(home, 1), None);
        std::fs::write(
            cache_path(home),
            r#"{"checked_at_unix":1,"latest_tag":"garbage"}"#,
        )
        .expect("write");
        assert_eq!(cached_latest(home, 1), None);
    }

    #[test]
    fn a_missing_cache_is_ignored_not_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(cached_latest(dir.path(), 1), None);
    }
}
