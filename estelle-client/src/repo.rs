use std::fmt;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repo(String);

/// The wire value for "no repository was resolved".
///
/// ⚠️ This stays on the WIRE unchanged — the server's contract for an unidentified caller is not
/// this lane's to redefine. What changed is that it is no longer allowed onto the SCREEN: see
/// [`Repo::is_unresolved`], which the frame uses to render `no repo` instead. A placeholder that
/// reaches a rule reads as a repository called `repo` owned by `unknown`.
pub const UNRESOLVED_REPO: &str = "unknown/repo";

impl Default for Repo {
    fn default() -> Self {
        Self(UNRESOLVED_REPO.to_string())
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

    /// True when no repository could be identified, so the surface must say so rather than
    /// print a name. One owner for that question, because the alternative is every rule
    /// comparing against the placeholder string itself and one of them getting it wrong.
    pub fn is_unresolved(&self) -> bool {
        self.0 == UNRESOLVED_REPO
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
    // Resolve an existing path before repository discovery. The old fallback accepted the basename of a
    // nonexistent client-supplied ACP cwd, turning a typo into an authoritative repository identity.
    let root = root.canonicalize().ok()?;
    // Use the library parser rather than executing `git` through inherited PATH. ACP's cwd is client
    // supplied; letting that request choose which executable resolves its repository is code execution.
    //
    // ⚠️ **THE DIRECTORY FALLBACK IS DELIBERATE HERE AND IS PINNED BY A CROSS-LANGUAGE TEST.**
    // `top_level::rust_repo_name_matches_the_python_hook_contract` drives this against the live
    // Python `repo_name_for` hook, which names a urlless checkout after its directory. Removing
    // the fallback here breaks that parity, so the question "what name would a hook compute for
    // this path" keeps its lenient answer.
    //
    // 🔴 The DIFFERENT question — "is there a repository here at all" — is [`is_repository`], and
    // it is the one the interface must ask before printing a name. Run from `~`, this function
    // answers `khai`, which is a correct answer to the question it is asked and a fabricated
    // identity if a rule prints it. One function per question; see `App::new`.
    let remote = gix::discover(&root)
        .ok()
        .and_then(|repo| {
            let remote = repo.find_remote("origin").ok()?;
            remote
                .url(gix::remote::Direction::Fetch)
                .map(ToString::to_string)
        })
        .and_then(|url| repo_from_remote_url(&url));
    remote.or_else(|| {
        root.file_name()
            .and_then(|name| name.to_str())
            .and_then(Repo::new)
    })
}

/// Is there a git repository at `root` or above it?
///
/// 🔴 **THIS IS NOT THE SAME QUESTION AS "WHAT IS THIS REPO CALLED", AND CONFLATING THEM IS THE
/// BUG.** `repo_for` will happily name any directory after itself — that is what the Python hook
/// parity requires. But a NAME is not an IDENTITY: run from a home directory, the interface
/// labelled every surface `session · khai`, `production · khai`, `ask · khai` and invited the user
/// to `Ask about khai`, for a repository that does not exist. The interface asks THIS before it
/// prints anything.
pub fn is_repository(root: &Path) -> bool {
    root.canonicalize()
        .ok()
        .is_some_and(|root| gix::discover(&root).is_ok())
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
