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
use crate::secret_engine;

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
///
/// Two passes, contract first: the seven legacy shapes run as always (shape only, no entropy
/// gate — a tight prefix needs none) and keep their established names, which the hook contract
/// tests pin. Anything they miss goes through the shared secret engine (the full gitleaks set
/// plus the Estelle-local extensions, WITH entropy gates and allowlists on), so the fence now
/// also catches the rest of the catalogue — slack, GCP, JWTs, DSN passwords, and the base64
/// sweep. An engine finding's name is its rule id. A value any rule's upstream allowlist
/// names as a published example (e.g. AWS's AKIAIOSFODNN7EXAMPLE) never fires either pass —
/// the 2026-08-10 false positive was exactly such a fixture.
pub fn find_secret_shape(value: &str) -> Option<(&'static str, usize)> {
    for (index, line) in value.lines().enumerate() {
        if let Some((name, _)) = SECRET_SHAPE_RES.iter().find(|(_, re)| {
            re.find_iter(line)
                .any(|m| !secret_engine::engine().is_allowlisted(m.as_str()))
        }) {
            return Some((*name, index + 1));
        }
    }
    // The engine pass reports the earliest line it fires on — the refusal should point at the
    // first credential, not the first rule in the catalogue.
    secret_engine::find_secret_shapes(value)
        .iter()
        .min_by_key(|finding| finding.line)
        .map(|finding| (finding.rule, finding.line))
}

/// Redact every credential-shaped value, in place. THE CHECKPOINT WIRE'S RULE (finding F-2,
/// 2026-08-13): a transcript is not a reviewed diff, so no exemptions exist here — the shape is
/// named so the loss is visible downstream; the VALUE never survives. NOT the same job as
/// `mask_secret`: that masks a whole credential-bearing FIELD for display; this redacts a value
/// embedded in prose.
///
/// The shared engine runs first (full catalogue + entropy + allowlists + the base64 sweep) and
/// marks `[REDACTED:<rule>:<fingerprint>]`; the seven legacy shapes then catch what the engine's
/// entropy gate deliberately passes over (the contract's own fixture is an all-one-letter GitHub
/// token) and keep their established `[redacted: <shape>]` marker. Allowlisted published
/// examples are exempt from BOTH passes.
pub fn redact_secrets(value: &str) -> String {
    let mut out = secret_engine::redact_secrets_engine(value);
    for (name, re) in SECRET_SHAPE_RES.iter() {
        out = re
            .replace_all(&out, |captures: &regex::Captures| {
                let matched = captures.get(0).map(|m| m.as_str()).unwrap_or_default();
                if secret_engine::engine().is_allowlisted(matched) {
                    matched.to_string()
                } else {
                    format!("[redacted: {name}]")
                }
            })
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
        // 🔴 BOTH NAMES ARE ACCEPTED, AND THAT IS NOT A CONVENIENCE — IT IS A CORRECTNESS FIX.
        // `ESTELLE_API_KEY` is what this client has always read. `ESTELLE_KEY` is what the published
        // documentation at fatelabs.ca/docs tells users to export, in eight separate places, and what
        // the onboarding page hands them. Before 2026-08-31 a user who followed our own docs exported
        // a variable nothing read, and got a credential error while looking at the instruction that
        // caused it. Documentation IS a claim about the system; when the two disagree, one of them is
        // a defect, and the cheaper honest fix is to make the claim true.
        // `ESTELLE_API_KEY` still wins when both are set, so no existing setup changes behaviour.
        self.resolve_with_environment(
            env::var_os("ESTELLE_API_KEY").or_else(|| env::var_os("ESTELLE_KEY")),
        )
    }

    /// Resolve against an EXPLICITLY SUPPLIED environment value rather than the process's own.
    ///
    /// `resolve()` is the ambient wrapper and keeps production precedence (ESTELLE_API_KEY wins).
    /// This is the deterministic form: every caller that must not depend on whatever happens to be
    /// exported — every test, and any host embedding this client with its own notion of
    /// environment — passes the value in. It is `pub` because the TUI crate's login tests were
    /// asserting on `CredentialSource::Stored` through the ambient path and went red for anyone
    /// who had ESTELLE_API_KEY set, which is the normal state for a user of the product. Widening
    /// this is cheaper and safer than teaching tests to mutate global environment state.
    pub fn resolve_with_environment(
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
