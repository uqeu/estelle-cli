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
//! # 🔴 AND THEN IT TOLD A CODEX USER TO RUN A CLAUDE CODE COMMAND (2026-09-05)
//!
//! The first version of this file had exactly ONE plugin tree in it — `~/.claude/plugins/…` —
//! and exactly one refresh command, Claude Code's. Both are wrong inside Codex, and on
//! 2026-09-05 the founder read the consequence in his own Codex terminal:
//!
//! ```text
//!   Estelle plugin 0.2.33 is behind 0.3.0 … refresh it:  /plugin marketplace update fatelabs
//!   codex: Unrecognized command '/plugin'
//! ```
//!
//! Measured that morning, on that machine:
//!
//! | tree                                                          | estelle version |
//! | ------------------------------------------------------------- | --------------- |
//! | `~/.claude/plugins/marketplaces/fatelabs/.claude-plugin/…json` | **0.2.33**      |
//! | `~/.codex/.tmp/marketplaces/fatelabs/.claude-plugin/…json`     | **0.3.1**       |
//! | `~/.codex/plugins/cache/fatelabs/estelle/`                     | **0.3.1**       |
//! | `gh release list --repo uqeu/estelle-cli` latest               | **v0.3.1**      |
//!
//! So the plugin serving that Codex session was **0.3.1 and current**, and the notice fired at
//! all only because it read a DIFFERENT HOST'S plugin tree. `0.2.33` was a true fact about the
//! wrong subject. (The other half, `0.3.0`, was honestly stale rather than wrong:
//! `~/.codex/version-check.json` was written 2026-09-05T03:35:07Z, when v0.3.0 really was the
//! newest release — v0.3.1 was published 08:20:02Z, 4.75 hours later, and the cache TTL is 24 h.
//! That lag is the deliberate price of never touching the network on a 5-second hook budget.)
//!
//! ▶ **SO THE SUBJECT AND THE REMEDY BOTH COME FROM THE HOST, AND WITH NO HOST THERE IS NO
//! NOTICE.** A staleness claim needs a subject; without knowing which plugin tree — if any — is
//! serving this hook, there is nothing this module can truthfully say, so it says nothing. That
//! is the founder's rule applied at its strongest: naming no command beats naming a wrong one,
//! and a wrong remedy sends the reader to break something else. [`running_host`] is where that
//! is decided, and the case it refuses to answer is exactly the one above: **two trees, no way
//! to tell which is serving.**
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
//! is the plugin's own tree — its `hooks.json` timeouts, its skills, its commands — because the
//! host only re-copies when the installed and resolved versions differ. That is exactly why the
//! founder's binary was fine and his hooks were not, and it is why the subject here is the
//! installed plugin manifest on disk, never `CARGO_PKG_VERSION`.
//!
//! # The latency constraint, which is the hard part
//!
//! The `welcome` hook has a **5-second host budget** (`HOOK_TABLE`, `top_level.rs`) and a cold
//! `npx` path has been measured at **162 seconds**. So this path **never touches the network**:
//! it reads [`version_check::cached_latest_only`] and returns. Cold or expired cache means
//! silence this session and a detached [`version_check::refresh_cache`] for the next one. Every
//! failure mode — offline, opted out, missing, malformed, unreadable, plugin not installed,
//! host unknown — is silent and instant.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::top_level::HookHost;
use crate::version_check::{self, Status, Version};

/// The marketplace this plugin ships under, as it appears in `known_marketplaces.json`.
const MARKETPLACE: &str = "fatelabs";

/// The plugin's name inside that marketplace.
const PLUGIN: &str = "estelle";

/// The environment variables a host exports into a PLUGIN hook's process, most specific first.
///
/// 🔴 **THE VARIABLE NAME CANNOT TELL THE HOSTS APART, AND THE PATH CAN.** Codex's own plugin
/// discovery sets BOTH — `PLUGIN_ROOT` and, in its own words, `CLAUDE_PLUGIN_ROOT` "for OOTB
/// compat with existing plugins that use this env var"
/// (vendored `hooks/src/engine/discovery.rs:234-239`), and those variables really do reach the
/// hook process: `command.envs(&handler.env)` at `hooks/src/engine/command_runner.rs:191`. So a
/// `CLAUDE_`-prefixed variable is NOT evidence of Claude Code. What is evidence is where the
/// path points, which is why [`host_for_plugin_root`] classifies the value and not the key.
const PLUGIN_ROOT_VARS: [&str; 2] = ["PLUGIN_ROOT", "CLAUDE_PLUGIN_ROOT"];

/// Claude Code's marketplace clones, relative to the real home directory.
const CLAUDE_MARKETPLACES_DIR: [&str; 3] = [".claude", "plugins", "marketplaces"];

/// The manifest file inside a marketplace clone. Identical on both hosts — Codex clones the same
/// repository into its own root, so the FILE is shared and only its ROOT differs.
const MARKETPLACE_MANIFEST: [&str; 2] = [".claude-plugin", "marketplace.json"];

/// The refresh command for one host, verified against that host's own CLI.
///
/// 🔴 **BOTH STRINGS WERE READ OFF `--help` ON THE FOUNDER'S MACHINE, NOT REMEMBERED**, because
/// the defect this function exists to fix was a remembered command:
///
/// * `claude plugin marketplace update [name]` — `claude --version` 2.1.258
/// * `codex plugin marketplace upgrade [MARKETPLACE_NAME]` — `codex --version` codex-cli 0.153.2
///
/// The CLI form is used for BOTH rather than either host's in-session slash command, because a
/// slash command is a claim about a chat surface that varies by version and by host, and a
/// subcommand printed by `--help` is a claim about the binary the reader already has.
fn refresh_command(host: HookHost) -> String {
    match host {
        HookHost::Claude => format!("claude plugin marketplace update {MARKETPLACE}"),
        HookHost::Codex => format!("codex plugin marketplace upgrade {MARKETPLACE}"),
    }
}

/// The marketplace manifest for ONE host's plugin tree.
///
/// Codex's root is not spelled out here: [`codex_core_plugins::installed_marketplaces::marketplace_install_root`]
/// is the one owner of it in the vendored host, and a second spelling of `.tmp/marketplaces`
/// would be free to drift from the host that actually writes it.
fn manifest_path(host: HookHost, home: &Path, codex_home: &Path) -> PathBuf {
    let root = match host {
        HookHost::Claude => CLAUDE_MARKETPLACES_DIR
            .iter()
            .fold(home.to_path_buf(), |path, segment| path.join(segment)),
        HookHost::Codex => {
            codex_core_plugins::installed_marketplaces::marketplace_install_root(codex_home)
        }
    };
    MARKETPLACE_MANIFEST
        .iter()
        .fold(root.join(MARKETPLACE), |path, segment| path.join(segment))
}

/// The host whose plugin tree contains `root`, or `None` when it is neither host's.
///
/// Codex is tested FIRST and the test is `starts_with`, so a Codex home that happens to live
/// under `~/.claude` could not be misread as Claude Code's. `None` is a real answer — the
/// `install-hooks` door invokes the binary directly with no plugin tree involved at all, and in
/// that case there is genuinely no plugin whose staleness we could report.
fn host_for_plugin_root(root: &Path, home: &Path, codex_home: &Path) -> Option<HookHost> {
    if root.starts_with(codex_home) {
        return Some(HookHost::Codex);
    }
    let claude = CLAUDE_MARKETPLACES_DIR
        .first()
        .map(|segment| home.join(segment))?;
    root.starts_with(claude).then_some(HookHost::Claude)
}

/// The host this hook is running inside, or `None` when it cannot be established.
///
/// TWO SIGNALS, IN THIS ORDER, AND NEITHER IS A GUESS:
///
/// 1. **The plugin root the host exported into this process.** Authoritative — it is the tree
///    this very hook was launched from — and present whenever a plugin door fired us.
/// 2. **The only host that has this plugin installed at all.** Reached when no plugin root is
///    exported, which is what the `install-hooks` door looks like. If exactly one host has an
///    Estelle plugin tree, that is the tree a notice could be about; if BOTH do, the answer is
///    ambiguous and this returns `None`.
///
/// ⚠️ **AMBIGUOUS MEANS SILENT, AND THAT IS THE FOUNDER'S CASE.** On 2026-09-05 both trees
/// existed on his machine — Claude Code's at 0.2.33 and Codex's at 0.3.1 — and the shipped code
/// resolved that ambiguity by always picking Claude Code's. Picking either one here would be the
/// same defect with better odds.
///
/// 🚫 **WHAT IS DELIBERATELY NOT A SIGNAL: `CLAUDECODE`.** Claude Code exports it (measured:
/// present in every child process of a Claude Code session, 2026-09-05) and the vendored Codex
/// tree never sets it — but Codex does not clear its parent's environment either, so a `codex`
/// launched from inside a Claude Code session inherits it. It identifies an ANCESTOR, not the
/// host that fired this hook, and this answer decides which command a customer is told to run.
fn running_host(home: &Path, codex_home: &Path) -> Option<HookHost> {
    PLUGIN_ROOT_VARS
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find_map(|root| host_for_plugin_root(&root, home, codex_home))
        .or_else(|| only_host_with_this_plugin(home, codex_home))
}

/// The single host that has this plugin installed, or `None` when that is zero hosts or two.
fn only_host_with_this_plugin(home: &Path, codex_home: &Path) -> Option<HookHost> {
    let mut installed = [HookHost::Claude, HookHost::Codex]
        .into_iter()
        .filter(|host| installed_version(*host, home, codex_home).is_some());
    let first = installed.next()?;
    installed.next().is_none().then_some(first)
}

/// The installed plugin's version for one host, or `None` — silently — when that host has no
/// plugin, the file is unreadable or malformed, or it does not name this plugin.
///
/// "Not installed" and "cannot tell" must both be quiet. This notice may only ever speak when it
/// KNOWS, which is the same rule that makes [`Status::Unknown`] a distinct state from
/// [`Status::UpToDate`] next door.
fn installed_version(host: HookHost, home: &Path, codex_home: &Path) -> Option<Version> {
    let text = std::fs::read_to_string(manifest_path(host, home, codex_home)).ok()?;
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
/// reader can neither act on nor verify. And it names the remedy for the host it is speaking
/// INTO — `host` is a required argument rather than an `Option`, so "we do not know the host"
/// cannot reach a sentence at all. That is deliberate: the alternative, a command-free sentence
/// on an unknown host, would still be a staleness claim about a tree we could not identify.
pub(crate) fn notice(status: Status, host: HookHost) -> Option<String> {
    let Status::Behind { running, latest } = status else {
        return None;
    };
    debug_assert!(running < latest, "Behind must mean running < latest");
    let refresh = refresh_command(host);
    Some(format!(
        "Estelle plugin {running} is behind {latest}. Its hooks, skills and commands come from \
         the plugin tree, not from the binary, so they stay at {running} until you refresh it:\n\
         \n  {refresh}\n"
    ))
}

/// 🔴 THE ONE OWNER OF THE COMPARISON, without a network, a clock, or a spawn.
///
/// Three roots, because they are genuinely three places: Claude Code's plugin manifest lives
/// under the real home directory, Codex's lives under `CODEX_HOME`, and `version_check`'s cache
/// lives under `CODEX_HOME` too. In tests they are the same tempdir. Taking them as parameters
/// is what lets [`welcome_line`] and every test go through THIS function — an earlier draft
/// inlined the comparison in `welcome_line` and left this one used only by tests, which clippy
/// caught as dead code and which is the classic shape of a guard that tests something production
/// does not do.
fn status_for(host: HookHost, home: &Path, codex_home: &Path, cache_home: &Path) -> Status {
    version_check::compare(
        installed_version(host, home, codex_home),
        version_check::cached_latest_only(cache_home),
    )
}

/// The `welcome` hook's line, or `None`.
///
/// Reads only. When the cache cannot answer, stays silent and spawns a detached refresh so the
/// next session can. When the HOST cannot be identified there is no subject and no remedy, so it
/// returns before reading anything at all.
pub(crate) async fn welcome_line() -> Option<String> {
    let home = dirs::home_dir()?;
    // `version_check` caches under CODEX_HOME; Claude Code's plugin tree lives under the real
    // home and Codex's under CODEX_HOME. Resolve both, and never let a failure to find either
    // one speak.
    let codex_home = codex_utils_home_dir::find_codex_home()
        .ok()?
        .into_path_buf();
    // No host means no plugin tree is serving this hook — the `install-hooks` door runs the
    // binary directly. Nothing to say, and nothing worth warming a cache for.
    let host = running_host(&home, &codex_home)?;
    let status = status_for(host, &home, &codex_home, &codex_home);
    if matches!(status, Status::Unknown) {
        // Cold, expired, opted out, or no plugin. Say nothing THIS session; if a refresh is
        // possible it makes the NEXT one able to speak. `refresh_cache` re-checks the opt-out.
        tokio::spawn(async move { version_check::refresh_cache(&codex_home).await });
        return None;
    }
    notice(status, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(host: HookHost, home: &Path, body: &str) {
        let path = manifest_path(host, home, home);
        let dir = path.parent().expect("parent").to_path_buf();
        std::fs::create_dir_all(&dir).expect("create");
        std::fs::write(path, body).expect("write");
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
        let fired = notice(
            Status::Behind {
                running: v(0, 2, 31),
                latest: v(0, 2, 33),
            },
            HookHost::Claude,
        )
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
            fired.contains("claude plugin marketplace update fatelabs"),
            "the notice named a problem and no remedy: {fired}"
        );

        // NEGATIVE CONTROL — same function, current install, must be silent.
        assert_eq!(
            notice(Status::UpToDate, HookHost::Claude),
            None,
            "it nagged a current plugin"
        );
        // And silence over an unknown: a reassuring line there would be a lie.
        assert_eq!(
            notice(Status::Unknown, HookHost::Claude),
            None,
            "it spoke without knowing"
        );
    }

    /// 🔴 **THE REMEDY IS THE HOST'S OWN, AND NEITHER HOST EVER SEES THE OTHER'S.**
    ///
    /// The founder read `/plugin marketplace update fatelabs` in a Codex session and Codex
    /// answered `Unrecognized command '/plugin'`. A wrong remedy is worse than none: it sends
    /// the reader to break something else. So this asserts BOTH directions — each host gets its
    /// own command, and the other host's command appears NOWHERE in the line, including the
    /// slash form that actually shipped.
    #[test]
    fn each_host_is_told_its_own_refresh_command_and_never_the_other_host_s() {
        let behind = Status::Behind {
            running: v(0, 2, 33),
            latest: v(0, 3, 1),
        };
        let codex = notice(behind, HookHost::Codex).expect("a stale Codex plugin");
        assert!(
            codex.contains("codex plugin marketplace upgrade fatelabs"),
            "Codex was not given the Codex command: {codex}"
        );
        for foreign in ["/plugin", "claude plugin"] {
            assert!(
                !codex.contains(foreign),
                "a Codex session was told to run {foreign:?}: {codex}"
            );
        }

        let claude = notice(behind, HookHost::Claude).expect("a stale Claude Code plugin");
        assert!(
            claude.contains("claude plugin marketplace update fatelabs"),
            "Claude Code was not given the Claude command: {claude}"
        );
        assert!(
            !claude.contains("codex plugin"),
            "a Claude Code session was told to run a codex command: {claude}"
        );
    }

    /// 🔴 **THE SUBJECT IS THE HOST'S OWN TREE — THE OTHER HOST'S STALENESS IS NOT AN ANSWER.**
    ///
    /// This is the founder's incident, reconstructed from the two manifests measured on his
    /// machine that morning: Claude Code's clone at 0.2.33 and Codex's at 0.3.1, with 0.3.1
    /// cached as latest. Running under Codex, the correct output is SILENCE; the shipped code
    /// said "0.2.33 is behind" because `~/.claude/…` was the only tree it knew.
    #[test]
    fn a_stale_tree_on_one_host_is_not_a_notice_on_the_other() {
        let home = tempfile::tempdir().expect("tempdir");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        std::fs::write(
            home.path().join("version-check.json"),
            format!(r#"{{"checked_at_unix":{now},"latest_tag":"v0.3.1"}}"#),
        )
        .expect("write cache");

        write_manifest(
            HookHost::Claude,
            home.path(),
            r#"{"plugins":[{"name":"estelle","version":"0.2.33"}]}"#,
        );
        write_manifest(
            HookHost::Codex,
            home.path(),
            r#"{"plugins":[{"name":"estelle","version":"0.3.1"}]}"#,
        );
        // The two trees really are two files, not one read twice.
        assert_ne!(
            manifest_path(HookHost::Claude, home.path(), home.path()),
            manifest_path(HookHost::Codex, home.path(), home.path())
        );
        assert_eq!(
            installed_version(HookHost::Claude, home.path(), home.path()),
            Some(v(0, 2, 33))
        );
        assert_eq!(
            installed_version(HookHost::Codex, home.path(), home.path()),
            Some(v(0, 3, 1))
        );

        // THE INCIDENT: running under Codex, whose plugin is current.
        assert_eq!(
            notice(
                status_for(HookHost::Codex, home.path(), home.path(), home.path()),
                HookHost::Codex
            ),
            None,
            "a Codex session was nagged about Claude Code's stale plugin tree"
        );
        // THE POSITIVE CONTROL, same fixture: under Claude Code the same data must speak.
        assert!(
            notice(
                status_for(HookHost::Claude, home.path(), home.path(), home.path()),
                HookHost::Claude
            )
            .is_some(),
            "Claude Code's genuinely stale tree said nothing"
        );
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
            HookHost::Claude,
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert_eq!(
            installed_version(HookHost::Claude, home.path(), home.path()),
            Some(v(0, 2, 31)),
            "the manifest read did not reach the named plugin"
        );
        let behind = status_for(HookHost::Claude, home.path(), home.path(), home.path());
        assert!(
            notice(behind, HookHost::Claude).is_some(),
            "a 0.2.31 plugin against a 0.2.33 cache said nothing"
        );

        // NEGATIVE ARM — same paths, same cache, current manifest.
        write_manifest(
            HookHost::Claude,
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"estelle","version":"0.2.33"}]}"#,
        );
        let current = status_for(HookHost::Claude, home.path(), home.path(), home.path());
        assert_eq!(
            notice(current, HookHost::Claude),
            None,
            "a current plugin was told it was stale"
        );
    }

    /// The version is read from the NAMED plugin, never `metadata.version` and never by index.
    #[test]
    fn it_reads_the_named_plugin_not_the_marketplace_metadata_and_not_index_zero() {
        let home = tempfile::tempdir().expect("tempdir");
        write_manifest(
            HookHost::Claude,
            home.path(),
            r#"{"metadata":{"version":"1.0.0"},
                "plugins":[{"name":"decoy","version":"9.9.9"},
                           {"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert_eq!(
            installed_version(HookHost::Claude, home.path(), home.path()),
            Some(v(0, 2, 31)),
            "it read metadata.version or plugins[0] instead of the named plugin"
        );
    }

    /// Every "cannot tell" is silent, and each reaches `Unknown` rather than `UpToDate`.
    #[test]
    fn a_missing_or_broken_or_foreign_manifest_is_silent() {
        let home = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            installed_version(HookHost::Claude, home.path(), home.path()),
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
            write_manifest(HookHost::Claude, home.path(), body);
            assert_eq!(
                installed_version(HookHost::Claude, home.path(), home.path()),
                None,
                "spoke on a manifest it could not read: {body}"
            );
            assert_eq!(
                notice(
                    status_for(HookHost::Claude, home.path(), home.path(), home.path()),
                    HookHost::Claude
                ),
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
            notice(
                version_check::compare(Some(v(0, 2, 9)), Some(v(0, 2, 33))),
                HookHost::Claude
            )
            .is_some(),
            "0.2.9 is BEHIND 0.2.33 and the check missed it — this is a lexical comparison"
        );
        assert_eq!(
            notice(
                version_check::compare(Some(v(0, 2, 33)), Some(v(0, 2, 9))),
                HookHost::Claude
            ),
            None,
            "0.2.33 is AHEAD of 0.2.9 and the check nagged anyway"
        );

        // The boundary this actually shipped across.
        assert!(
            notice(
                version_check::compare(Some(v(0, 2, 33)), Some(v(0, 3, 0))),
                HookHost::Claude
            )
            .is_some(),
            "0.2.33 is BEHIND 0.3.0 and the check missed it"
        );
        assert_eq!(
            notice(
                version_check::compare(Some(v(0, 3, 0)), Some(v(0, 2, 33))),
                HookHost::Claude
            ),
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
            HookHost::Claude,
            home.path(),
            r#"{"plugins":[{"name":"estelle","version":"0.2.31"}]}"#,
        );
        assert!(
            version_check::cached_latest_only(home.path()).is_none(),
            "an empty directory produced a cached latest"
        );
        assert!(
            matches!(
                status_for(HookHost::Claude, home.path(), home.path(), home.path()),
                Status::Unknown
            ),
            "a cold cache reported something other than Unknown"
        );
        assert_eq!(
            notice(
                status_for(HookHost::Claude, home.path(), home.path(), home.path()),
                HookHost::Claude
            ),
            None
        );
    }

    /// The manifest paths are the ones each host actually writes, and this module never reads or
    /// writes anywhere else — a detector that writes into the tree it audits is how a detector
    /// becomes a cause.
    ///
    /// Codex's root is asserted against the vendored host's own owner of it rather than a
    /// literal, so a change there fails here instead of drifting silently.
    #[test]
    fn the_manifest_paths_are_each_host_s_marketplace_clone() {
        let home = Path::new("/home/someone");
        let codex_home = Path::new("/home/someone/.codex");
        assert_eq!(
            manifest_path(HookHost::Claude, home, codex_home),
            Path::new(
                "/home/someone/.claude/plugins/marketplaces/fatelabs/.claude-plugin/marketplace.json"
            )
        );
        assert_eq!(
            manifest_path(HookHost::Codex, home, codex_home),
            codex_core_plugins::installed_marketplaces::marketplace_install_root(codex_home)
                .join("fatelabs/.claude-plugin/marketplace.json")
        );
    }

    /// 🔴 **THE HOST IS READ OFF A PATH, NOT OFF A VARIABLE NAME.**
    ///
    /// `CLAUDE_PLUGIN_ROOT` is set by BOTH hosts (`hooks/src/engine/discovery.rs:236`, "for OOTB
    /// compat"), so keying on the name would classify every Codex plugin session as Claude Code
    /// — which is the bug this file was opened for, in a different disguise. The row that
    /// matters is the third: a `CLAUDE_`-named variable pointing into the Codex home is Codex.
    #[test]
    fn the_running_host_is_classified_by_where_the_plugin_root_points() {
        let home = Path::new("/home/someone");
        let codex_home = Path::new("/home/someone/.codex");
        let rows: [(&str, Option<HookHost>); 5] = [
            (
                "/home/someone/.claude/plugins/marketplaces/fatelabs/estelle-plugin",
                Some(HookHost::Claude),
            ),
            (
                "/home/someone/.codex/plugins/cache/fatelabs/estelle/0.3.1",
                Some(HookHost::Codex),
            ),
            // The measured shape of the founder's Codex session: a CLAUDE_-named variable whose
            // value is a Codex path. Name says Claude, path says Codex, and the path wins.
            (
                "/home/someone/.codex/.tmp/marketplaces/fatelabs/estelle-plugin",
                Some(HookHost::Codex),
            ),
            // Neither host's tree: no subject, no notice.
            ("/opt/somewhere-else/estelle", None),
            ("/home/someone", None),
        ];
        for (root, expected) in rows {
            assert_eq!(
                host_for_plugin_root(Path::new(root), home, codex_home),
                expected,
                "{root}"
            );
        }
    }

    /// The founder's rule at its strongest: two candidate trees and no way to choose is SILENCE.
    ///
    /// All three arms of the fallback in one test, so none can be deleted without the others
    /// going red — and the middle arm is the founder's own machine, where both hosts had the
    /// plugin and the shipped code picked Claude Code's every time.
    #[test]
    fn the_fallback_speaks_only_when_exactly_one_host_has_the_plugin() {
        let home = tempfile::tempdir().expect("tempdir");
        let manifest = r#"{"plugins":[{"name":"estelle","version":"0.2.33"}]}"#;

        // ZERO trees: nothing to be stale.
        assert_eq!(only_host_with_this_plugin(home.path(), home.path()), None);

        // ONE tree: that host, and it is the host whose tree exists — not a default.
        write_manifest(HookHost::Claude, home.path(), manifest);
        assert_eq!(
            only_host_with_this_plugin(home.path(), home.path()),
            Some(HookHost::Claude)
        );

        // TWO trees — the founder's machine. Ambiguous, therefore silent.
        write_manifest(HookHost::Codex, home.path(), manifest);
        assert_eq!(
            only_host_with_this_plugin(home.path(), home.path()),
            None,
            "with a plugin installed on BOTH hosts it picked one anyway"
        );

        // And the Codex-only direction, so the ONE arm is not secretly "always Claude".
        let codex_only = tempfile::tempdir().expect("tempdir");
        write_manifest(HookHost::Codex, codex_only.path(), manifest);
        assert_eq!(
            only_host_with_this_plugin(codex_only.path(), codex_only.path()),
            Some(HookHost::Codex)
        );
    }

    /// An unusable plugin root is not a host, and `running_host` must not invent one from it.
    #[test]
    fn an_unusable_plugin_root_is_not_a_host() {
        let home = Path::new("/home/someone");
        let codex_home = Path::new("/home/someone/.codex");
        // Not `remove_var` — this test must not mutate an environment other tests share. The
        // classifier is driven directly with the values `running_host` would have found.
        assert_eq!(host_for_plugin_root(Path::new(""), home, codex_home), None);
        assert_eq!(
            host_for_plugin_root(Path::new("/opt/elsewhere"), home, codex_home),
            None
        );
    }
}
