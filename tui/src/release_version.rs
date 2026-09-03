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
//! What this crate CAN own is the pair that has no legitimate reason to differ: the version the
//! binary reports and the version the npm package claims. That is asserted here, so it is checked
//! on every path that runs `cargo test` rather than on a schedule nobody watches.
//!
//! ⚠️ **Limit, stated:** this proves the two IN-TREE owners agree. It says nothing about what was
//! actually published — only a read-back from npm and GitHub can do that, which is what
//! `verify_cli_release_order.py` is for. It is the half of the contract this repository can
//! enforce, and naming the other half is the point of this paragraph.

#[cfg(test)]
mod tests {
    /// The npm package manifest, read at compile time from this repository.
    const NPM_SHIM_PACKAGE_JSON: &str = include_str!("../../npm-shim/package.json");

    /// Pull `"version": "x.y.z"` out of the manifest without taking a JSON dependency on a crate
    /// that does not otherwise need one.
    fn npm_shim_version() -> String {
        let needle = "\"version\"";
        let start = NPM_SHIM_PACKAGE_JSON
            .find(needle)
            .expect("npm-shim/package.json must declare a version");
        let after_colon = NPM_SHIM_PACKAGE_JSON[start + needle.len()..]
            .find(':')
            .map(|offset| start + needle.len() + offset + 1)
            .expect("the version key must be followed by a colon");
        let rest = &NPM_SHIM_PACKAGE_JSON[after_colon..];
        let open = rest.find('"').expect("the version must be a string");
        let rest = &rest[open + 1..];
        let close = rest.find('"').expect("the version string must be closed");
        rest[..close].to_string()
    }

    #[test]
    fn the_binary_and_the_npm_package_claim_the_same_version() {
        let binary = env!("CARGO_PKG_VERSION");
        let npm = npm_shim_version();

        // Vacuity guard: a parse that silently produced an empty string would make any comparison
        // below meaningless, and `""` is exactly what a slightly-wrong parser returns.
        assert!(
            npm.split('.').count() >= 3 && npm.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "parsed {npm:?} out of npm-shim/package.json, which is not a version — the parser is \
             wrong, so the comparison below would prove nothing"
        );

        assert_eq!(
            binary, npm,
            "the binary reports {binary} but npm-shim/package.json publishes {npm}. A customer \
             would install a package labelled one version containing a binary that says another. \
             Bump both, and remember the third owner in the other repo: release/cli-current.json \
             names the version that must be CURRENT on GitHub Releases and npm."
        );
    }
}
