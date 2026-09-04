//! 🔴 A STALE PLUGIN IS INDISTINGUISHABLE FROM A CURRENT ONE AT THE POINT OF USE.
//!
//! Measured on the founder's own machine, 2026-09-04:
//!
//! * `~/.claude/plugins/known_marketplaces.json` → `fatelabs` = github `uqeu/estelle-cli`
//! * `~/.claude/plugins/marketplaces/fatelabs` → clone pinned at `7f83f89`, 2026-08-31
//! * that clone's `.claude-plugin/marketplace.json` → the `estelle` entry = **0.2.31**
//! * npm `dist-tags.latest` and the highest git tag → **0.2.33**
//!
//! His Claude Code had been running the plugin **two releases behind, silently, for days**, and
//! nothing told him. This module is the thing that tells him.
//!
//! ⚠️ **DETECTION, NOT PREVENTION.** It prints one line naming both versions and the refresh
//! command. It never updates anything, never writes into the plugin tree, and never asks.
//!
//! # 🔴 ONE OWNER FOR "WHAT VERSION IS LATEST" — AND IT IS NOT THIS FILE
//!
//! [`crate::version_check`] already owns that fact: it reads the GitHub releases API, caches for
//! 24h, is bounded at 3s / 256KB, is a **declared egress sink** (`docs/egress-sinks.toml`,
//! `cli-version-check`), and honours `ESTELLE_NO_VERSION_CHECK`. This module adds **no second
//! source**. It contributes only a new *subject*: `version_check` compares the running BINARY
//! against latest; this compares the installed PLUGIN against the same latest, through
//! [`version_check::compare`] and [`version_check::Version`].
//!
//! A first draft of this file fetched `registry.npmjs.org` itself. That would have been a second
//! owner of the same fact, free to disagree with the first, plus an undeclared egress host.
//!
//! # Why the BINARY's version is not the answer
//!
//! The plugin's hooks run `npx -y @fatelabs/estelle@0` (`estelle-plugin/hooks/hooks.json`), so
//! npm resolves the newest 0.x on every call and the binary is usually current. What goes stale
//! is the plugin's own tree — its `hooks.json` timeouts, its skills, its commands — because
//! Claude Code only re-copies when the installed and resolved versions differ. That is exactly
//! why the founder's binary was fine and his hooks were not, and it is why the subject here is
//! the installed plugin manifest on disk, never `CARGO_PKG_VERSION`.
//!
//! # The latency constraint, which is the hard part
//!
//! The `welcome` hook has a **5-second host budget** (`HOOK_TABLE`, `top_level.rs`) and a cold
//! `npx` path has been measured at **162 seconds**. So this path **never touches the network**:
//! it reads [`version_check::cached_latest_only`] and returns. Cold or expired cache means
//! silence this session and a detached [`version_check::refresh_cache`] for the next one. Every
//! failure mode — offline, opted out, missing, malformed, unreadable, plugin not installed — is
//! silent and instant.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::version_check::{self, Status, Version};

/// The marketplace this plugin ships under, as it appears in `known_marketplaces.json`.
const MARKETPLACE: &str = "fatelabs";

/// The plugin's name inside that marketplace.
const PLUGIN: &str = "estelle";

/// The manifest Claude Code keeps for the installed marketplace clone.
fn manifest_path(home: &Path) -> PathBuf {
    home.join(".claude")
        .join("plugins")
        .join("marketplaces")
        .join(MARKETPLACE)
        .join(".claude-plugin")
        .join("marketplace.json")
}

/// The installed plugin's version, or `None` — silently — when the plugin is absent, the file is
/// unreadable or malformed, or it does not name this plugin.
///
/// "Not installed" and "cannot tell" must both be quiet. This notice may only ever speak when it
/// KNOWS, which is the same rule that makes [`Status::Unknown`] a distinct state from
/// [`Status::UpToDate`] next door.
fn installed_version(home: &Path) -> Option<Version> {
    let text = std::fs::read_to_string(manifest_path(home)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    // ⚠️ `plugins[].version`, NOT `metadata.version`. They are different facts — `metadata` is
    // the marketplace's own schema version, 1.0.0 — and `release.yml:49` asserts the tag against
    // this same `plugins[].version`. Matched BY NAME rather than by index, so a second plugin
    // appearing in the marketplace cannot silently redirect the read.
    let raw = value
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(PLUGIN))?
        .get("version")?
        .as_str()?;
    version_check::parse_version(raw)
}

/// 🔴 **THE PURE CORE.** The line for a given comparison, or `None`.
///
/// `None` for every state except `Behind` — silence when current, and silence when unknown,
/// because a reassuring line over an unknown is a lie. Same rule, and the same [`Status`], as
/// `version_check::notice`.
///
/// The line names BOTH versions: "your plugin is out of date" with no numbers is a sentence the
/// reader can neither act on nor verify.
pub(crate) fn notice(status: Status) -> Option<String> {
    let Status::Behind { running, latest } = status else {
        return None;
    };
    debug_assert!(running < latest, "Behind must mean running < latest");
    Some(format!(
        "Estelle plugin {running} is behind {latest}. Its hooks, skills and commands come from \
         the plugin tree, not from the binary, so they stay at {running} until you refresh it:\n\
         \n  /plugin marketplace update {MARKETPLACE}\n"
    ))
}

/// The comparison, without a network, a clock, or a spawn. Extracted so the decision is testable.
fn status_in(home: &Path) -> Status {
    version_check::compare(
        installed_version(home),
        version_check::cached_latest_only(home),
    )
}

/// The `welcome` hook's line, or `None`.
///
/// Reads only. When the cache cannot answer, stays silent and spawns a detached refresh so the
/// next session can.
pub(crate) async fn welcome_line() -> Option<String> {
    let home = dirs::home_dir()?;
    // `version_check` caches under CODEX_HOME; the plugin manifest lives under the real home.
    // Resolve both, and never let a failure to find either one speak.
    let codex_home = codex_utils_home_dir::find_codex_home()
        .ok()?
        .into_path_buf();
    let status = version_check::compare(
        installed_version(&home),
        version_check::cached_latest_only(&codex_home),
    );
    if matches!(status, Status::Unknown) {
        // Cold, expired, opted out, or no plugin. Say nothing THIS session; if a refresh is
        // possible it makes the NEXT one able to speak. `refresh_cache` re-checks the opt-out.
        tokio::spawn(async move { version_check::refresh_cache(&codex_home).await });
        return None;
    }
    notice(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(home: &Path, body: &str) {
        let dir = manifest_path(home).parent().expect("parent").to_path_buf();
        std::fs::create_dir_all(&dir).expect("create");
        std::fs::write(manifest_path(home), body).expect("write");
    }

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        version_check::parse_version(&format!("{major}.{minor}.{patch}")).expect("version")
    }

    /// 🔴 **THE DISCRIMINATING PAIR. A CHECK THAT CAN ONLY EVER BE QUIET IS NOT A CHECK.**
    ///
    /// Both arms of the same function, in one test, so neither can be deleted without the other
    /// going red. The positive arm is the exact state measured on the founder's machine.
    #[test]
    fn the_notice_fires_when_behind_and_is_silent_when_current() {
        // POSITIVE CONTROL — founder's machine, 2026-09-04: plugin 0.2.31, latest 0.2.33.
        let fired = notice(Status::Behind {
            running: v(0, 2, 31),
            latest: v(0, 2, 33),
        })
        .expect("a stale plugin must be named");
        assert!(
            fired.contains("0.2.31"),
            "the notice hid the installed version: {fired}"
        );
        assert!(
            fired.contains("0.2.33"),
            "the notice hid the latest version: {fired}"
        );
        assert!(
            fired.contains("/plugin marketplace update fatelabs"),
            "the notice named a problem and no remedy: {fired}"
        );

        // NEGATIVE CONTROL — same function, current install, must be silent.
        assert_eq!(notice(Status::UpToDate), None, "it nagged a current plugin");
        // And silence over an unknown: a reassuring line there would be a lie.
        assert_eq!(notice(Status::Unknown), None, "it spoke without knowing");
    }

    /// 🔴 END-TO-END THROUGH THE REAL FILESYSTEM READ, BOTH ARMS.
    ///
    /// The test above proves the message; this proves the *plumbing* — that a real manifest on
    /// disk plus a real cached latest reaches `Behind`, and that bumping the manifest to the
    /// latest silences it. A pure-function pair cannot catch a wrong JSON path.
    #[test]
    fn a_stale_manifest_on_disk_reaches_behind_and_a_current_one_does_not() {
        let home = tempfile::tempdir().expect("tempdir");
        // The cache `version_check` owns, written in its own format, fresh.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        std::fs::write(
            home.path().join("version-check.json"),
            format!(r#"{{"checked_at_unix":{now},"latest_tag":"v0.2.33"}}"#),
        )
        .expect("write cache");

        write_manifest(
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert_eq!(
            installed_version(home.path()),
            Some(v(0, 2, 31)),
            "the manifest read did not reach the named plugin"
        );
        let behind = version_check::compare(
            installed_version(home.path()),
            version_check::cached_latest_only(home.path()),
        );
        assert!(
            notice(behind).is_some(),
            "a 0.2.31 plugin against a 0.2.33 cache said nothing"
        );

        // NEGATIVE ARM — same paths, same cache, current manifest.
        write_manifest(
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"estelle","version":"0.2.33"}]}"#,
        );
        let current = version_check::compare(
            installed_version(home.path()),
            version_check::cached_latest_only(home.path()),
        );
        assert_eq!(
            notice(current),
            None,
            "a current plugin was told it was stale"
        );
    }

    /// The version is read from the NAMED plugin, never `metadata.version` and never by index.
    #[test]
    fn it_reads_the_named_plugin_not_the_marketplace_metadata_and_not_index_zero() {
        let home = tempfile::tempdir().expect("tempdir");
        write_manifest(
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"decoy","version":"9.9.9"},
                           {"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert_eq!(
            installed_version(home.path()),
            Some(v(0, 2, 31)),
            "it read metadata.version or plugins[0] instead of the named plugin"
        );
    }

    /// Every "cannot tell" is silent, and each reaches `Unknown` rather than `UpToDate`.
    #[test]
    fn a_missing_or_broken_or_foreign_manifest_is_silent() {
        let home = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            installed_version(home.path()),
            None,
            "spoke with no plugin installed"
        );

        for body in [
            "{not json",
            r#"{"metadata":{"version":"1.0.0"}}"#,
            r#"{"plugins":[]}"#,
            r#"{"plugins":[{"name":"someone-else","version":"0.2.31"}]}"#,
            r#"{"plugins":[{"name":"estelle","version":"not-a-version"}]}"#,
            r#"{"plugins":[{"name":"estelle"}]}"#,
        ] {
            write_manifest(home.path(), body);
            assert_eq!(
                installed_version(home.path()),
                None,
                "spoke on a manifest it could not read: {body}"
            );
            assert_eq!(
                notice(status_in(home.path())),
                None,
                "reached a message from an unreadable manifest: {body}"
            );
        }
    }

    /// 🔴 **THE COMPARISON IS NUMERIC, AND THE LEXICAL TRAP IS NOT WHERE I FIRST WROTE IT.**
    ///
    /// My first version of this test asserted `"0.2.33" > "0.3.0"` as strings. That is FALSE —
    /// `'2' < '3'`, so string order happens to agree with version order there — and the test
    /// caught my wrong premise on the first run. Recorded rather than quietly deleted, because
    /// the corrected case is the one that actually matters.
    ///
    /// The real trap is a two-digit patch: `"0.2.9" > "0.2.33"` IS true as text and false as a
    /// version, and this project shipped 0.2.9 through 0.2.33, so it is a range real installs sit
    /// in. Both the trap and the 0.3.0 boundary are asserted.
    #[test]
    fn the_comparison_is_numeric_not_lexical() {
        assert!(
            "0.2.9" > "0.2.33",
            "the premise of this test died: string order no longer traps on a two-digit patch"
        );
        assert!(
            notice(version_check::compare(Some(v(0, 2, 9)), Some(v(0, 2, 33)))).is_some(),
            "0.2.9 is BEHIND 0.2.33 and the check missed it — this is a lexical comparison"
        );
        assert_eq!(
            notice(version_check::compare(Some(v(0, 2, 33)), Some(v(0, 2, 9)))),
            None,
            "0.2.33 is AHEAD of 0.2.9 and the check nagged anyway"
        );

        // The boundary this actually shipped across.
        assert!(
            notice(version_check::compare(Some(v(0, 2, 33)), Some(v(0, 3, 0)))).is_some(),
            "0.2.33 is BEHIND 0.3.0 and the check missed it"
        );
        assert_eq!(
            notice(version_check::compare(Some(v(0, 3, 0)), Some(v(0, 2, 33)))),
            None,
            "0.3.0 is AHEAD of 0.2.33 and the check nagged anyway"
        );
    }

    /// A cold cache is `Unknown`, never `UpToDate` — the whole point of the third state. With a
    /// perfectly good manifest present, the absence of a cached latest must still be silence.
    #[test]
    fn a_cold_cache_is_unknown_not_up_to_date() {
        let home = tempfile::tempdir().expect("tempdir");
        write_manifest(
            home.path(),
            r#"{"plugins":[{"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert!(
            version_check::cached_latest_only(home.path()).is_none(),
            "an empty directory produced a cached latest"
        );
        assert!(
            matches!(status_in(home.path()), Status::Unknown),
            "a cold cache reported something other than Unknown"
        );
        assert_eq!(notice(status_in(home.path())), None);
    }

    /// The manifest path is the one Claude Code actually writes, and this module never reads or
    /// writes anywhere else under `.claude` — a detector that writes into the tree it audits is
    /// how a detector becomes a cause.
    #[test]
    fn the_manifest_path_is_the_marketplace_clone() {
        assert_eq!(
            manifest_path(Path::new("/home/someone")),
            Path::new(
                "/home/someone/.claude/plugins/marketplaces/fatelabs/.claude-plugin/marketplace.json"
            )
        );
    }
}
