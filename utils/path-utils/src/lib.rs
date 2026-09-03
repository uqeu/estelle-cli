//! Path normalization, symlink resolution, and atomic writes shared across Codex crates.

pub(crate) mod env;
pub use env::is_wsl;

use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashSet;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tempfile::NamedTempFile;

pub fn normalize_for_path_comparison(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let canonical = path.as_ref().canonicalize()?;
    Ok(normalize_for_wsl(canonical))
}

/// Compare paths after applying Codex's filesystem normalization.
///
/// If either path cannot be normalized, this falls back to direct path equality.
pub fn paths_match_after_normalization(left: impl AsRef<Path>, right: impl AsRef<Path>) -> bool {
    if let (Ok(left), Ok(right)) = (
        normalize_for_path_comparison(left.as_ref()),
        normalize_for_path_comparison(right.as_ref()),
    ) {
        return left == right;
    }
    left.as_ref() == right.as_ref()
}

pub fn normalize_for_native_workdir(path: impl AsRef<Path>) -> PathBuf {
    normalize_for_native_workdir_with_flag(path.as_ref().to_path_buf(), cfg!(windows))
}

pub struct SymlinkWritePaths {
    pub read_path: Option<PathBuf>,
    pub write_path: PathBuf,
}

/// Resolve the final filesystem target for `path` while retaining a safe write path.
///
/// This follows symlink chains (including relative symlink targets) until it reaches a
/// non-symlink path. If the chain cycles or any metadata/link resolution fails, it
/// returns `read_path: None` and uses the original absolute path as `write_path`.
/// There is no fixed max-resolution count; cycles are detected via a visited set.
pub fn resolve_symlink_write_paths(path: &Path) -> io::Result<SymlinkWritePaths> {
    let root = AbsolutePathBuf::from_absolute_path(path)
        .map(AbsolutePathBuf::into_path_buf)
        .unwrap_or_else(|_| path.to_path_buf());
    let mut current = root.clone();
    let mut visited = HashSet::new();

    // Follow symlink chains while guarding against cycles.
    loop {
        let meta = match std::fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(SymlinkWritePaths {
                    read_path: Some(current.clone()),
                    write_path: current,
                });
            }
            Err(_) => {
                return Ok(SymlinkWritePaths {
                    read_path: None,
                    write_path: root,
                });
            }
        };

        if !meta.file_type().is_symlink() {
            return Ok(SymlinkWritePaths {
                read_path: Some(current.clone()),
                write_path: current,
            });
        }

        // If we've already seen this path, the chain cycles.
        if !visited.insert(current.clone()) {
            return Ok(SymlinkWritePaths {
                read_path: None,
                write_path: root,
            });
        }

        let target = match std::fs::read_link(&current) {
            Ok(target) => target,
            Err(_) => {
                return Ok(SymlinkWritePaths {
                    read_path: None,
                    write_path: root,
                });
            }
        };

        let next = if target.is_absolute() {
            AbsolutePathBuf::from_absolute_path(&target)
        } else if let Some(parent) = current.parent() {
            Ok(AbsolutePathBuf::resolve_path_against_base(&target, parent))
        } else {
            return Ok(SymlinkWritePaths {
                read_path: None,
                write_path: root,
            });
        };

        let next = match next {
            Ok(path) => path.into_path_buf(),
            Err(_) => {
                return Ok(SymlinkWritePaths {
                    read_path: None,
                    write_path: root,
                });
            }
        };

        current = next;
    }
}

/// The three durable steps, in the only order that is correct.
///
/// Ported from orca's `durable-file-write.ts` (MIT), which states the failure this exists to
/// prevent: *"rename() is atomic for readers but not durable. Without fsync on the file and its
/// directory, a power loss after a successful rename can leave the old contents, or an empty
/// inode."* Their sharper note is the one that shaped this seam — *"fsync BEFORE rename. A rename
/// that lands first can expose a zero-length file."*
///
/// **Where ours improves on theirs:** orca proves the order by monkey-patching `fsyncSync` and
/// `renameSync` on Node's `fs` module — a test that can only exist in a language with a mutable
/// module registry, and one that leaves the real call sites unproven. Here the ordering lives
/// behind a trait, so the *same* code path the product runs is the one the test drives; production
/// and test differ only in which syscalls the three methods make. Their test also asserts a
/// hardcoded platform list for directory-fsync support; ours reports it as a distinct
/// [`DurableStep::DirSyncUnsupported`] step, so "this platform cannot fsync a directory" is a
/// recorded outcome rather than an assumption baked into the assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableStep {
    /// The file's own contents reached the disk.
    SyncFile,
    /// The temp file took the target's name. Readers see all-or-nothing from here.
    Rename,
    /// The directory entry reached the disk, so the rename itself survives a power loss.
    SyncDir,
    /// The platform offers no way to fsync a directory. Recorded, never silently skipped.
    DirSyncUnsupported,
}

/// The syscalls [`write_durably_with`] makes, behind a seam so the ORDER can be asserted.
trait DurableOps {
    fn sync_file(&mut self, file: &std::fs::File) -> io::Result<()>;
    fn rename(&mut self, tmp: NamedTempFile, target: &Path) -> io::Result<()>;
    fn sync_dir(&mut self, dir: &Path) -> io::Result<()>;
}

struct RealDurableOps;

impl DurableOps for RealDurableOps {
    fn sync_file(&mut self, file: &std::fs::File) -> io::Result<()> {
        file.sync_all()
    }

    fn rename(&mut self, tmp: NamedTempFile, target: &Path) -> io::Result<()> {
        tmp.persist(target).map(|_| ()).map_err(io::Error::from)
    }

    fn sync_dir(&mut self, dir: &Path) -> io::Result<()> {
        // Opening a directory read-only and fsyncing it is the POSIX way to make a rename durable.
        // Windows has no equivalent; there the rename's own durability is the filesystem's problem,
        // which is why the caller treats this as best-effort rather than fatal.
        #[cfg(unix)]
        {
            std::fs::File::open(dir)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "directory fsync is not available on this platform",
            ))
        }
    }
}

/// fsync the contents, THEN rename, THEN fsync the directory — recording each step it completes.
///
/// The recorded steps are what makes the order falsifiable: an fsync moved after the rename still
/// fsyncs a file, so a log of fsyncs alone reads identically for the correct and the broken order.
/// The rename has to be in the same log for the sequence to mean anything.
fn write_durably_with<O: DurableOps>(
    ops: &mut O,
    write_path: &Path,
    contents: &str,
    steps: &mut Vec<DurableStep>,
) -> io::Result<()> {
    let parent = write_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path {} has no parent directory", write_path.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;

    ops.sync_file(tmp.as_file())?;
    steps.push(DurableStep::SyncFile);

    ops.rename(tmp, write_path)?;
    steps.push(DurableStep::Rename);

    // Best-effort by design: the bytes and the name are already safe by this point, so a platform
    // that cannot fsync a directory must not turn a successful write into an error. It is RECORDED
    // rather than skipped, because "we did not do it" and "we could not do it" are different facts.
    match ops.sync_dir(parent) {
        Ok(()) => steps.push(DurableStep::SyncDir),
        Err(_) => steps.push(DurableStep::DirSyncUnsupported),
    }
    Ok(())
}

/// Write `contents` to `write_path` atomically **and durably**.
///
/// Atomic was already true — a reader never saw a half-written file, because the write went to a
/// temp file and was renamed into place. Durable was not: there was no `fsync` on either the file
/// or its parent directory, so a power loss after the rename could still expose the OLD contents or
/// a zero-length inode. Every caller of this function is writing something a user would be upset to
/// lose or to find truncated — `config.toml` (`core/src/config/edit.rs:762`), plugin config
/// (`config/src/plugin_edit.rs:74`), the remote plugin catalog cache
/// (`core-plugins/src/remote/catalog_cache.rs:141`).
pub fn write_atomically(write_path: &Path, contents: &str) -> io::Result<()> {
    let mut steps = Vec::new();
    write_durably_with(&mut RealDurableOps, write_path, contents, &mut steps)
}

fn normalize_for_wsl(path: PathBuf) -> PathBuf {
    normalize_for_wsl_with_flag(path, env::is_wsl())
}

fn normalize_for_native_workdir_with_flag(path: PathBuf, is_windows: bool) -> PathBuf {
    if is_windows {
        dunce::simplified(&path).to_path_buf()
    } else {
        path
    }
}

fn normalize_for_wsl_with_flag(path: PathBuf, is_wsl: bool) -> PathBuf {
    if !is_wsl {
        return path;
    }

    if !is_wsl_case_insensitive_path(&path) {
        return path;
    }

    lower_ascii_path(path)
}

fn is_wsl_case_insensitive_path(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::path::Component;

        let mut components = path.components();
        let Some(Component::RootDir) = components.next() else {
            return false;
        };
        let Some(Component::Normal(mnt)) = components.next() else {
            return false;
        };
        if !ascii_eq_ignore_case(mnt.as_bytes(), b"mnt") {
            return false;
        }
        let Some(Component::Normal(drive)) = components.next() else {
            return false;
        };
        let drive_bytes = drive.as_bytes();
        drive_bytes.len() == 1 && drive_bytes[0].is_ascii_alphabetic()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

#[cfg(target_os = "linux")]
fn ascii_eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(lhs, rhs)| lhs.to_ascii_lowercase() == *rhs)
}

#[cfg(target_os = "linux")]
fn lower_ascii_path(path: PathBuf) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::ffi::OsStringExt;

    // WSL mounts Windows drives under /mnt/<drive>, which are case-insensitive.
    let bytes = path.as_os_str().as_bytes();
    let mut lowered = Vec::with_capacity(bytes.len());
    for byte in bytes {
        lowered.push(byte.to_ascii_lowercase());
    }
    PathBuf::from(OsString::from_vec(lowered))
}

#[cfg(not(target_os = "linux"))]
fn lower_ascii_path(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
#[path = "path_utils_tests.rs"]
mod tests;
