//! Local paths for the curated-plugins snapshot on disk.
//!
//! THE NETWORK SYNC IS DELETED (attack-11 egress audit, 2026-08-13): this module used to clone
//! github.com/openai/plugins.git and fall back to api.github.com zipballs and a
//! chatgpt.com-hosted backup archive on every startup. Upstream's release channel is the wrong
//! product's egress, so the sync is gone; what remains is WHERE the snapshot lives, for the
//! local loader and marketplace policy that still read it.

use std::path::Path;
use std::path::PathBuf;

const CURATED_PLUGINS_RELATIVE_DIR: &str = ".tmp/plugins";
const CURATED_PLUGINS_SHA_FILE: &str = ".tmp/plugins.sha";

pub fn curated_plugins_repo_path(codex_home: &Path) -> PathBuf {
    codex_home.join(CURATED_PLUGINS_RELATIVE_DIR)
}

pub fn curated_plugins_api_marketplace_path(codex_home: &Path) -> PathBuf {
    curated_plugins_repo_path(codex_home).join(".agents/plugins/api_marketplace.json")
}

pub fn read_curated_plugins_sha(codex_home: &Path) -> Option<String> {
    read_sha_file(curated_plugins_sha_path(codex_home).as_path())
}

fn curated_plugins_sha_path(codex_home: &Path) -> PathBuf {
    codex_home.join(CURATED_PLUGINS_SHA_FILE)
}

pub fn has_local_curated_plugins_snapshot(codex_home: &Path) -> bool {
    curated_plugins_repo_path(codex_home)
        .join(".agents/plugins/marketplace.json")
        .is_file()
        && codex_home.join(CURATED_PLUGINS_SHA_FILE).is_file()
}

fn read_sha_file(sha_path: &Path) -> Option<String> {
    std::fs::read_to_string(sha_path)
        .ok()
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
}

#[cfg(test)]
#[path = "startup_sync_tests.rs"]
mod tests;
