//! Compatibility cleanup for credentials written by the retired Claude Code import flow.
//!
//! New Claude acquisition goes through the server-issued OAuth URL. Keeping only presence and delete
//! prevents an upgrade from stranding an old local secret without retaining a production path that reads
//! another product's credential store.

use std::fs;
use std::io;
use std::path::PathBuf;

fn store_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle/providers/claude.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

pub(crate) fn imported_credential_present() -> bool {
    store_path().is_ok_and(|path| path.is_file())
}

pub(crate) fn logout() -> io::Result<bool> {
    let path = store_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
