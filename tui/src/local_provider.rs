//! Local endpoint acquisition, informed by Goose local-inference's explicit
//! model registry and jcode's no-key localhost profiles (both MIT).

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::login;
use crate::provider_catalog;

#[derive(Serialize)]
struct LocalProviderSnapshot<'a> {
    provider: &'a str,
    base_url: &'a str,
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<&'a str>,
}

fn store_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle/providers/local.json"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory is unavailable"))
}

pub(crate) fn configured_present() -> bool {
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

pub(crate) fn run(
    provider: &str,
    supplied_base: Option<&str>,
    model: Option<&str>,
) -> io::Result<()> {
    let descriptor = provider_catalog::resolve(provider)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown local provider"))?;
    if descriptor.auth != provider_catalog::AuthKind::LocalEndpoint {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider is not a local/custom endpoint",
        ));
    }
    let prompted_base = if supplied_base.is_none() && descriptor.requires_base_url() {
        login::read_plain_value(b"Provider API base URL: ")?
    } else {
        None
    };
    let route =
        provider_catalog::login_route(provider, supplied_base.or(prompted_base.as_deref()))?;
    let secret = if route.requires_key {
        login::read_secret_value(b"Provider API key: ")?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Interrupted,
                "this non-local endpoint requires an API key; nothing was stored",
            )
        })?
    } else {
        Zeroizing::new(String::new())
    };
    let secret = (!secret.is_empty()).then_some(secret);
    persist_and_receipt(
        &route,
        model,
        secret.as_deref().map(String::as_str),
        &store_path()?,
        &mut io::stdout(),
    )
}

fn persist_and_receipt(
    route: &provider_catalog::LoginRoute,
    model: Option<&str>,
    api_key: Option<&str>,
    destination: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "local provider store has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = parent.join(format!(".local.json.tmp-{}", std::process::id()));
    let snapshot = LocalProviderSnapshot {
        provider: route.provider.id,
        base_url: route.base_url.as_deref().unwrap_or_default(),
        model: model.unwrap_or_default(),
        api_key,
    };
    let encoded = Zeroizing::new(serde_json::to_vec_pretty(&snapshot).map_err(io::Error::other)?);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let write_result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(&encoded)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, destination)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;

    writeln!(
        output,
        "Endpoint configured: {} · {}.",
        route.provider.display_name,
        route.base_url.as_deref().unwrap_or_default()
    )?;
    writeln!(
        output,
        "Credential: {}.",
        if api_key.is_some() {
            "stored privately"
        } else {
            "not required for this local endpoint"
        }
    )?;
    writeln!(
        output,
        "Endpoint acquisition is complete; provider runtime binding is not yet proven. Run estelle doctor."
    )?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;
    use zeroize::Zeroizing;

    use super::*;

    #[test]
    fn endpoint_snapshot_is_private_and_receipt_never_renders_the_key() {
        let dir = tempdir().expect("tempdir");
        let destination = dir.path().join("providers/local.json");
        let secret = Zeroizing::new("local-test-secret".to_string());
        let route = crate::provider_catalog::login_route(
            "openai-compatible",
            Some("https://models.example.test/v1"),
        )
        .expect("custom route");
        let mut output = Vec::new();

        persist_and_receipt(
            &route,
            Some("test-model"),
            Some(secret.as_str()),
            &destination,
            &mut output,
        )
        .expect("persist local endpoint");

        assert_eq!(
            fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let receipt = String::from_utf8(output).expect("UTF-8 receipt");
        assert!(receipt.contains("Endpoint configured"));
        assert!(receipt.contains("runtime binding is not yet proven"));
        assert!(!receipt.contains(secret.as_str()));
    }
}
