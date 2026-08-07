use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repo(String);

impl Default for Repo {
    fn default() -> Self {
        Self("unknown/repo".to_string())
    }
}

impl Repo {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug)]
pub struct RepoResolver {
    override_repo: Option<Repo>,
    root: PathBuf,
}

impl RepoResolver {
    pub fn new(override_repo: Option<Repo>, root: impl Into<PathBuf>) -> Self {
        Self {
            override_repo,
            root: root.into(),
        }
    }

    pub fn resolve(&self) -> Option<Repo> {
        self.override_repo.clone().or_else(|| repo_for(&self.root))
    }
}

fn repo_for(root: &Path) -> Option<Repo> {
    let remote = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|url| repo_from_remote_url(&url));
    remote.or_else(|| {
        root.canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(Repo::new)
    })
}

pub fn repo_from_remote_url(url: &str) -> Option<Repo> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let (_, name) = trimmed.rsplit_once('/')?;
    let owner = trimmed
        .get(..trimmed.len().saturating_sub(name.len() + 1))?
        .rsplit(['/', ':'])
        .next()?;
    Repo::new(format!("{owner}/{name}"))
}
