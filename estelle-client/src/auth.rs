use std::env;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

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

/// The named registry, in match order. A refusal that cannot say WHICH shape fired at WHICH
/// line cannot be tuned (the 2026-08-10 false positive was a scanner-test fixture shaped like an
/// AWS key). The matched value is never returned — shape name + line number only.
const SECRET_SHAPES: &[(&str, &str)] = &[
    ("an Estelle key", r"estelle_live_[A-Za-z0-9_-]{12,}"),
    ("an sk- API key", r"sk-[A-Za-z0-9_-]{16,}"),
    ("a Stripe live key", r"sk_live_[A-Za-z0-9]{10,}"),
    ("a GitHub token", r"ghp_[A-Za-z0-9]{20,}"),
    ("a GitHub PAT", r"github_pat_[A-Za-z0-9_]{20,}"),
    ("an AWS access key", r"AKIA[0-9A-Z]{16}"),
    ("a private key block", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
];

static SECRET_SHAPE_RES: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    SECRET_SHAPES
        .iter()
        .filter_map(|(name, pattern)| Regex::new(pattern).ok().map(|re| (*name, re)))
        .collect()
});

/// The first credential shape found, as (shape name, 1-based line), else None.
pub fn find_secret_shape(value: &str) -> Option<(&'static str, usize)> {
    value.lines().enumerate().find_map(|(index, line)| {
        SECRET_SHAPE_RES
            .iter()
            .find(|(_, re)| re.is_match(line))
            .map(|(name, _)| (*name, index + 1))
    })
}

/// Redact every credential-shaped value, in place, as `[redacted: <shape>]`. THE CHECKPOINT WIRE'S
/// RULE (finding F-2, 2026-08-13): a transcript is not a reviewed diff, so no exemptions exist here —
/// the shape is named so the loss is visible downstream; the VALUE never survives. NOT the same job as
/// `mask_secret`: that masks a whole credential-bearing FIELD for display; this redacts a value embedded
/// in prose.
pub fn redact_secrets(value: &str) -> String {
    let mut out = value.to_string();
    for (name, re) in SECRET_SHAPE_RES.iter() {
        out = re
            .replace_all(&out, format!("[redacted: {name}]"))
            .into_owned();
    }
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    Environment,
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
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialStore")
            .field("path", &self.path)
            .finish()
    }
}

impl CredentialStore {
    pub fn default_location() -> Result<Self, Error> {
        let home = dirs::home_dir().ok_or(Error::NoCredential)?;
        Ok(Self::from_estelle_home(home.join(".estelle")))
    }

    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(crate) fn from_estelle_home(estelle_home: impl Into<PathBuf>) -> Self {
        Self::new(estelle_home.into().join("auth.json"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn resolve(&self) -> Result<ResolvedCredential, Error> {
        self.resolve_with_environment(env::var_os("ESTELLE_API_KEY"))
    }

    pub(crate) fn resolve_with_environment(
        &self,
        environment_value: Option<std::ffi::OsString>,
    ) -> Result<ResolvedCredential, Error> {
        if let Some(key) = environment_value
            .and_then(|value| value.into_string().ok())
            .and_then(|value| ApiKey::new(value).ok())
        {
            return Ok(ResolvedCredential {
                api_key: key,
                source: CredentialSource::Environment,
            });
        }
        let raw = read_private_file(&self.path)?;
        let stored: StoredCredential =
            serde_json::from_slice(&raw).map_err(|_| Error::MalformedCredential)?;
        Ok(ResolvedCredential {
            api_key: ApiKey::new(stored.key)?,
            source: CredentialSource::Stored,
        })
    }

    pub fn write(&self, key: &ApiKey) -> Result<(), Error> {
        self.write_private_file(key)
    }

    fn write_private_file(&self, key: &ApiKey) -> Result<(), Error> {
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
        if source != CredentialSource::Stored {
            return Ok(false);
        }
        remove_if_present(&self.path).map(|()| true)
    }
}

fn read_private_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut file = OpenOptions::new().read(true).open(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::NoCredential
        } else {
            Error::CredentialIo(source)
        }
    })?;
    require_private_mode(&file)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    Ok(raw)
}

#[cfg(unix)]
fn require_private_mode(file: &fs::File) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    let mode = file.metadata()?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(Error::InsecureCredentialPermissions { mode });
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_private_mode(_file: &fs::File) -> Result<(), Error> {
    Ok(())
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
