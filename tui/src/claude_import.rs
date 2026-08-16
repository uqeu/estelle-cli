use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::process::Stdio;

use serde::Deserialize;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::provider_store;

const CLAUDE_CODE_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";
#[cfg(target_os = "macos")]
const CLAUDE_CODE_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

#[derive(Deserialize)]
struct ClaudeCodeFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeCredential>,
}

#[derive(Deserialize, Serialize)]
struct ClaudeCredential {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken", default)]
    refresh_token: String,
    #[serde(rename = "expiresAt", default)]
    expires_at: serde_json::Value,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
    #[serde(default)]
    scopes: serde_json::Value,
}

fn parse_blob(blob: &str) -> io::Result<ClaudeCredential> {
    let credential = serde_json::from_str::<ClaudeCodeFile>(blob)
        .ok()
        .and_then(|file| file.claude_ai_oauth)
        .or_else(|| serde_json::from_str::<ClaudeCredential>(blob).ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Claude credential shape was not recognised",
            )
        })?;
    if credential.access_token.trim().is_empty() && credential.refresh_token.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Claude credential contained no access or refresh token",
        ));
    }
    Ok(credential)
}

fn store_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle/providers/claude.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

fn persist(credential: &ClaudeCredential, destination: &Path) -> io::Result<()> {
    provider_store::write_private_json(credential, destination, "claude.json")
}

fn source_blob() -> io::Result<(Zeroizing<String>, &'static str)> {
    if let Ok(value) = std::env::var(CLAUDE_CODE_TOKEN_ENV)
        && !value.trim().is_empty()
    {
        return Ok((Zeroizing::new(value), "CLAUDE_CODE_OAUTH_TOKEN"));
    }
    #[cfg(target_os = "macos")]
    {
        let value = read_keychain_blob()?;
        Ok((value, "Claude Code macOS Keychain"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Claude Code stored no importable credential in CLAUDE_CODE_OAUTH_TOKEN (the Keychain import is macOS-only)",
        ))
    }
}

#[cfg(target_os = "macos")]
fn read_keychain_blob() -> io::Result<Zeroizing<String>> {
    use std::io::Read;
    use std::time::Duration;
    use std::time::Instant;

    let mut child = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            CLAUDE_CODE_KEYCHAIN_SERVICE,
            "-w",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => {
                let mut bytes = Zeroizing::new(Vec::new());
                child
                    .stdout
                    .take()
                    .ok_or_else(|| io::Error::other("Keychain stdout was unavailable"))?
                    .read_to_end(&mut bytes)?;
                let value = String::from_utf8(bytes.to_vec()).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "Keychain value was not UTF-8")
                })?;
                return Ok(Zeroizing::new(value));
            }
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Claude Code Keychain credential was unavailable",
                ));
            }
            None if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(25));
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out reading Claude Code credentials from the macOS Keychain",
                ));
            }
        }
    }
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

fn write_import_receipt(
    output: &mut impl Write,
    source: &str,
    destination: &Path,
) -> io::Result<()> {
    writeln!(output, "Imported from {source}.")?;
    writeln!(
        output,
        "Estelle-owned snapshot: {} (mode 0600).",
        destination.display()
    )?;
    writeln!(
        output,
        "Credential import is complete; provider runtime binding is not yet proven. Run estelle doctor."
    )?;
    output.flush()
}

pub(crate) fn run() -> io::Result<()> {
    let mut output = io::stdout();
    output.write_all(
        b"Imports the credential Claude Code stored on this machine. Estelle reads it once and never moves, deletes, or modifies Claude Code's copy.\n",
    )?;
    let (blob, source) = source_blob()?;
    let credential = parse_blob(&blob)?;
    if credential.refresh_token.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Claude Code credential has no refresh token, so Estelle refused to store a short-lived dead end",
        ));
    }
    let destination = store_path()?;
    persist(&credential, &destination)?;
    write_import_receipt(&mut output, source, &destination)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn wrapped_claude_code_blob_is_snapshotted_without_touching_the_source() {
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/claude.json");
        let secret = "claude-access-secret";
        let blob = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{secret}","refreshToken":"refresh-secret","expiresAt":4102444800000,"subscriptionType":"max","scopes":["user:inference"]}}}}"#
        );

        let credential = parse_blob(&blob).expect("Claude Code credential");
        persist(&credential, &destination).expect("snapshot");

        let stored = fs::read_to_string(&destination).expect("stored snapshot");
        assert!(stored.contains(secret));
        assert_eq!(
            fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            blob.matches(secret).count(),
            1,
            "source blob stayed unchanged"
        );
    }

    #[test]
    fn import_receipt_does_not_claim_the_unbound_runtime_is_connected() {
        let mut output = Vec::new();
        write_import_receipt(
            &mut output,
            "Claude Code macOS Keychain",
            Path::new("/tmp/claude.json"),
        )
        .expect("receipt");

        let rendered = String::from_utf8(output).expect("UTF-8 receipt");
        assert!(rendered.contains("runtime binding is not yet proven"));
        assert!(rendered.contains("Run estelle doctor"));
        assert!(!rendered.contains("connected"));
    }
}
