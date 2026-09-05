//! 🔴 **"WHAT VERSION IS THIS CLI" HAS THREE OWNERS AND NOTHING MADE THEM AGREE.**
//!
//! The number a customer actually installs comes from `npm-shim/package.json`. The number the
//! binary prints — `estelle --version`, the MCP `clientInfo`, the update check — comes from
//! `CARGO_PKG_VERSION`. Nothing compared them, so a release could ship a package labelled one
//! version containing a binary that says another, and every gate would stay green.
//!
//! A third owner lives in the **other repository**: `release/cli-current.json` in the `estelle`
//! repo, which `scripts/verify_cli_release_order.py current` compares against GitHub Releases and
//! npm on a six-hourly schedule. That check is sound and it passes — but it is scheduled-only, so
//! it can never block a release, it lives in a repo that does not contain the thing it guards, and
//! **nothing ties it to this tree at all**: at the time of writing the intent file said `0.2.32`
//! while this tree said `0.2.33`, and both were correct, because one describes what is published
//! and the other what is being built. That is a legitimate difference, which is exactly why an
//! equality check across repos would be wrong and why this module does not attempt one.
//!
//! What this crate CAN own is the set with no legitimate reason to differ: the version the binary
//! reports and the three files that publish a version to a customer. `release.yml` already checks
//! all four — and only at release time, on a `workflow_dispatch`, by which point a mismatch is
//! discovered under pressure. This asserts the same contract on every path that runs `cargo test`.
//! Its own comment records why it grew to four: *"the manifest sat at 0.1.0 while the CLI shipped
//! 0.2.20 — caught only because this check was extended to look."*
//!
//! ⚠️ **Limit, stated:** this proves the four IN-TREE owners agree. It says nothing about what was
//! actually published — only a read-back from npm and GitHub can do that, which is what
//! `verify_cli_release_order.py` is for. It is the half of the contract this repository can
//! enforce, and naming the other half is the point of this paragraph.

#[cfg(test)]
mod tests {
    /// Every file in this repository that publishes a version to a customer, read at compile time.
    const NPM_SHIM_PACKAGE_JSON: &str = include_str!("../../npm-shim/package.json");
    const PLUGIN_JSON: &str = include_str!("../../estelle-plugin/.claude-plugin/plugin.json");
    const MARKETPLACE_JSON: &str = include_str!("../../.claude-plugin/marketplace.json");

    /// Pull the FIRST `"version": "x.y.z"` out of a manifest without taking a JSON dependency on a
    /// crate that does not otherwise need one.
    fn first_version(source: &str, what: &str) -> String {
        let needle = "\"version\"";
        let start = source
            .find(needle)
            .unwrap_or_else(|| panic!("{what} must declare a version"));
        let after_colon = source[start + needle.len()..]
            .find(':')
            .map(|offset| start + needle.len() + offset + 1)
            .unwrap_or_else(|| panic!("{what}: the version key must be followed by a colon"));
        let rest = &source[after_colon..];
        let open = rest
            .find('"')
            .unwrap_or_else(|| panic!("{what}: the version must be a string"));
        let rest = &rest[open + 1..];
        let close = rest
            .find('"')
            .unwrap_or_else(|| panic!("{what}: the version string must be closed"));
        rest[..close].to_string()
    }

    #[test]
    fn every_file_that_publishes_a_version_agrees_with_the_binary() {
        let binary = env!("CARGO_PKG_VERSION");
        let publishers = [
            (
                "npm-shim/package.json",
                first_version(NPM_SHIM_PACKAGE_JSON, "npm-shim"),
            ),
            (
                "estelle-plugin/.claude-plugin/plugin.json",
                first_version(PLUGIN_JSON, "plugin.json"),
            ),
            (
                // ⚠️ NOT the first `"version"` in the file. `marketplace.json` carries TWO: a
                // `metadata.version` describing the marketplace's own schema (1.0.0) and the real
                // one inside `plugins[]`. The first draft of this test read `metadata.version` and
                // reported a mismatch that did not exist — the same shape as reading `$NF` off a
                // table and getting timestamps instead of tags. `release.yml` asks for it by path
                // (`plugins[0].version`); so does this, by anchoring on the array first.
                ".claude-plugin/marketplace.json",
                first_version(
                    &MARKETPLACE_JSON[MARKETPLACE_JSON
                        .find("\"plugins\"")
                        .expect("marketplace.json must declare a plugins array")..],
                    "marketplace.json plugins[0]",
                ),
            ),
        ];

        // Vacuity guard: a parse that silently produced an empty string would make every
        // comparison below meaningless, and `""` is exactly what a slightly-wrong parser returns.
        for (file, version) in &publishers {
            assert!(
                version.split('.').count() >= 3
                    && version.chars().next().is_some_and(|c| c.is_ascii_digit()),
                "parsed {version:?} out of {file}, which is not a version — the parser is wrong, \
                 so the comparison below would prove nothing"
            );
        }

        let disagreeing: Vec<String> = publishers
            .iter()
            .filter(|(_, version)| version != binary)
            .map(|(file, version)| format!("{file} says {version}"))
            .collect();

        assert!(
            disagreeing.is_empty(),
            "the binary reports {binary} but {disagreeing:?}. A customer would install something \
             labelled one version containing a binary that says another. Bump all of them, and \
             remember the owner in the OTHER repo: release/cli-current.json names the version that \
             must be CURRENT on GitHub Releases and npm."
        );
    }
}
