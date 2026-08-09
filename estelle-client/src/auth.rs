use std::env;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;

use codex_keyring_store::DefaultKeyringStore;
use codex_keyring_store::KeyringStore;
use codex_secrets::LocalSecretsNamespace;
use codex_secrets::SecretName;
use codex_secrets::SecretScope;
use codex_secrets::SecretsBackendKind;
use codex_secrets::SecretsManager;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;

use crate::ApiKey;
use crate::Error;

static SECRET_SHAPE: LazyLock<Result<Regex, regex::Error>> = LazyLock::new(|| {
    Regex::new(
        r"(?:estelle_live_[A-Za-z0-9_-]{12,}|sk-[A-Za-z0-9_-]{16,}|sk_live_[A-Za-z0-9]{10,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)",
    )
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    Environment,
    SecureStore,
    Stored,
}

#[derive(Clone, Debug)]
pub struct ResolvedCredential {
    pub api_key: ApiKey,
    pub source: CredentialSource,
}

#[derive(Clone)]
pub struct CredentialStore {
    path: PathBuf,
    secure: Option<SecretsManager>,
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialStore")
            .field("path", &self.path)
            .field("secure", &self.secure.is_some())
            .finish()
    }
}

impl CredentialStore {
    pub fn default_location() -> Result<Self, Error> {
        let home = dirs::home_dir().ok_or(Error::NoCredential)?;
        let estelle_home = home.join(".estelle");
        Ok(Self::new_secure(
            estelle_home,
            Arc::new(DefaultKeyringStore),
        ))
    }

    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            secure: None,
        }
    }

    pub fn new_secure(
        estelle_home: impl Into<PathBuf>,
        keyring_store: Arc<dyn KeyringStore>,
    ) -> Self {
        let estelle_home = estelle_home.into();
        let secure = SecretsManager::new_with_keyring_store_and_namespace(
            estelle_home.clone(),
            SecretsBackendKind::Local,
            keyring_store,
            LocalSecretsNamespace::EstelleAuth,
        );
        Self {
            path: estelle_home.join("auth.json"),
            secure: Some(secure),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn resolve(&self) -> Result<ResolvedCredential, Error> {
        if let Some(key) = env::var_os("ESTELLE_API_KEY")
            .and_then(|value| value.into_string().ok())
            .and_then(|value| ApiKey::new(value).ok())
        {
            return Ok(ResolvedCredential {
                api_key: key,
                source: CredentialSource::Environment,
            });
        }
        if let Some(secure) = &self.secure
            && let Ok(Some(value)) = secure.get(&SecretScope::Global, &credential_name()?)
        {
            return Ok(ResolvedCredential {
                api_key: ApiKey::new(value)?,
                source: CredentialSource::SecureStore,
            });
        }
        let raw = fs::read(&self.path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::NoCredential
            } else {
                Error::CredentialIo(source)
            }
        })?;
        let stored: StoredCredential =
            serde_json::from_slice(&raw).map_err(|_| Error::MalformedCredential)?;
        Ok(ResolvedCredential {
            api_key: ApiKey::new(stored.key)?,
            source: CredentialSource::Stored,
        })
    }

    pub fn write(&self, key: &ApiKey) -> Result<(), Error> {
        if let Some(secure) = &self.secure
            && secure
                .set(&SecretScope::Global, &credential_name()?, key.expose())
                .is_ok()
        {
            remove_if_present(&self.path)?;
            return Ok(());
        }
        self.write_legacy_file(key)
    }

    fn write_legacy_file(&self, key: &ApiKey) -> Result<(), Error> {
        let parent = self.path.parent().ok_or_else(|| {
            Error::CredentialIo(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "credential path has no parent",
            ))
        })?;
        fs::create_dir_all(parent)?;
        set_mode(parent, 0o700)?;

        let temporary = parent.join(format!(".auth.json.tmp-{}", std::process::id()));
        let result: Result<(), Error> = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            set_create_mode(&mut options, 0o600);
            let mut file = options.open(&temporary)?;
            serde_json::to_writer_pretty(&mut file, &StoredCredentialRef { key: key.expose() })?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            set_mode(&temporary, 0o600)?;
            fs::rename(&temporary, &self.path)?;
            set_mode(&self.path, 0o600)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    /// Delete the stored credential. This is NEVER the reaction to a single rejection: a 401
    /// from one route is route scope, not proof of a bad key — measured on prod, `login`
    /// verified and a question succeeded on the SAME credential that one `/me` 401 then wiped.
    /// The legitimate caller has cross-route evidence (repeated rejections across DIFFERENT
    /// routes) and says so out loud before calling this. Callers decide; this only deletes.
    pub fn delete_stored(&self, source: CredentialSource) -> Result<bool, Error> {
        if source == CredentialSource::SecureStore {
            let Some(secure) = &self.secure else {
                return Ok(false);
            };
            return secure
                .delete(&SecretScope::Global, &credential_name()?)
                .map_err(|error| Error::CredentialStore(error.to_string()));
        }
        if source != CredentialSource::Stored {
            return Ok(false);
        }
        remove_if_present(&self.path).map(|()| true)
    }
}

fn credential_name() -> Result<SecretName, Error> {
    SecretName::new("ESTELLE_API_KEY").map_err(|error| Error::CredentialStore(error.to_string()))
}

fn remove_if_present(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::CredentialIo(error)),
    }
}

#[derive(Deserialize)]
struct StoredCredential {
    key: String,
}

#[derive(Serialize)]
struct StoredCredentialRef<'a> {
    key: &'a str,
}

pub fn is_secret_shaped(value: &str) -> bool {
    match SECRET_SHAPE.as_ref() {
        Ok(pattern) => pattern.is_match(value),
        Err(_) => true,
    }
}

pub fn mask_secret(value: &str) -> String {
    if !is_secret_shaped(value)
        && !["estelle_live_", "sk-", "ghp_", "github_pat_"]
            .iter()
            .any(|prefix| value.contains(prefix))
    {
        return value.to_string();
    }
    "[credential hidden]".to_string()
}

#[cfg(unix)]
fn set_create_mode(options: &mut OpenOptions, mode: u32) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(mode);
}

#[cfg(not(unix))]
fn set_create_mode(_options: &mut OpenOptions, _mode: u32) {}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}
