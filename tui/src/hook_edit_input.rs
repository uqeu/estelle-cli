//! Read-only reconstruction of every candidate in a host edit.
//!
//! Patch syntax and replacement semantics belong to codex-apply-patch. This adapter
//! supplies a bounded immutable filesystem snapshot and never calls its writer.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use codex_apply_patch::{ApplyPatchFileChange, Hunk, MaybeApplyPatchVerified};
use codex_exec_server::{
    CopyOptions, CreateDirectoryOptions, ExecutorFileSystem, ExecutorFileSystemFuture,
    FileMetadata, FileSystemReadStream, FileSystemSandboxContext, ReadDirectoryEntry,
    RemoveOptions,
};
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use serde_json::Value;

pub(crate) const MAX_EDIT_FILES: usize = 32;
pub(crate) const MAX_FILE_BYTES: usize = 512 * 1024;
pub(crate) const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct CandidateEdit {
    pub(crate) path: PathBuf,
    /// None is a deletion, not an empty checked file.
    pub(crate) content: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PreparedEdits {
    pub(crate) edits: Vec<CandidateEdit>,
    root: PathBuf,
    originals: HashMap<PathBuf, Option<Vec<u8>>>,
}

#[derive(Debug)]
pub(crate) struct GateFile {
    pub(crate) path: String,
    pub(crate) diff: String,
    pub(crate) has_added_code: bool,
    /// A separate candidate-artifact check, not the user's historical patch.
    pub(crate) candidate_diff: Option<String>,
}

impl PreparedEdits {
    /// Canonical post-images for surviving files. Deletions are separate tombstones,
    /// never empty-file sources; admission requires the server's v2 acknowledgment.
    pub(crate) fn post_images(&self) -> Result<BTreeMap<String, String>, String> {
        self.edits
            .iter()
            .filter_map(|edit| {
                edit.content.as_ref().map(|content| {
                    self.relative(&edit.path)
                        .map(|path| (path, content.clone()))
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn has_deletions(&self) -> bool {
        self.edits.iter().any(|edit| edit.content.is_none())
    }

    pub(crate) fn deleted_paths(&self) -> Result<std::collections::BTreeSet<String>, String> {
        self.edits
            .iter()
            .filter(|edit| edit.content.is_none())
            .map(|edit| self.relative(&edit.path))
            .collect()
    }

    fn relative(&self, path: &Path) -> Result<String, String> {
        let path = path
            .strip_prefix(&self.root)
            .map_err(|_| "candidate escaped its project")?
            .to_str()
            .ok_or("candidate path is not UTF-8")?;
        // Existing server unified-diff path parsing does not unquote arbitrary git
        // filenames. Do not send a different path than the one being edited.
        if path
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\"' | '\\'))
        {
            return Err(
                "this filename cannot be represented unambiguously by the gate diff parser".into(),
            );
        }
        Ok(path.into())
    }

    pub(crate) fn gate_files(&self) -> Result<Vec<GateFile>, String> {
        self.edits
            .iter()
            .map(|edit| {
                let path = self.relative(&edit.path)?;
                let before = std::str::from_utf8(
                    self.originals
                        .get(&edit.path)
                        .and_then(Option::as_deref)
                        .unwrap_or_default(),
                )
                .map_err(|_| "source is not UTF-8; no text gate ruling is available")?;
                let after = edit.content.as_deref().unwrap_or("");
                let mut options = diffy::DiffOptions::new();
                options
                    .set_original_filename(format!("a/{path}"))
                    .set_modified_filename(format!("b/{path}"));
                let patch = options.create_patch(before, after);
                let has_added_code = patch.hunks().iter().flat_map(diffy::Hunk::lines).any(
                    |line| matches!(line, diffy::Line::Insert(text) if !text.trim().is_empty()),
                );
                let candidate_diff =
                    if !before.is_empty() && before != after && !after.trim().is_empty() {
                        Some(format!(
                            "diff --git a/{path} b/{path}\n{}",
                            options.create_patch("", after)
                        ))
                    } else {
                        None
                    };
                Ok(GateFile {
                    path: path.clone(),
                    diff: format!("diff --git a/{path} b/{path}\n{patch}"),
                    has_added_code,
                    candidate_diff,
                })
            })
            .collect()
    }

    /// Recheck the exact files after remote verification. A concurrent edit invalidates
    /// the evidence; never approve a different starting state than the one inspected.
    pub(crate) fn unchanged(&self) -> bool {
        self.originals.iter().all(|(path, before)| {
            safe_path(&self.root, path).is_ok()
                && read_bounded(path).is_ok_and(|after| &after == before)
        })
    }
}

fn path_uri(path: &Path) -> Result<PathUri, String> {
    AbsolutePathBuf::from_absolute_path(path)
        .map(|path| PathUri::from_abs_path(&path))
        .map_err(|_| "the edit path is not absolute".to_string())
}

/// Reject traversal and symlink components before any source read. The same check
/// is repeated before an allow. Host process cwd is the authority, not a patch field.
fn safe_path(root: &Path, raw: &Path) -> Result<PathBuf, String> {
    let relative = if raw.is_absolute() {
        raw.strip_prefix(root)
            .map_err(|_| "edit is outside the active project")?
    } else {
        raw
    };
    if relative.as_os_str().is_empty() {
        return Err("edit has no file path".into());
    }
    let mut path = root.to_path_buf();
    for part in relative.components() {
        let Component::Normal(name) = part else {
            return Err("edit path contains traversal or an unsupported component".into());
        };
        let name_text = name.to_string_lossy();
        if matches!(name_text.as_ref(), ".git" | ".ssh" | ".aws" | ".env")
            || name_text.starts_with(".env.")
        {
            return Err("edit targets credential or repository-control data".into());
        }
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("symlink edit paths are not verified".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err("edit path metadata could not be read".into()),
        }
    }
    Ok(path)
}

pub(crate) fn scoped_source_bytes(root: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "project root could not be resolved")?;
    let path = safe_path(&root, Path::new(path))?;
    let bytes = read_bounded(&path)?;
    safe_path(&root, &path)?;
    Ok(bytes)
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("source file metadata could not be read".into()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("only regular source files can be verified".into());
    }
    if meta.len() > MAX_FILE_BYTES as u64 {
        return Err("source file exceeds the verification byte limit".into());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // If the final component changes after metadata inspection, never follow
        // a new symlink or block opening a substituted FIFO.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "source file could not be read")?;
    let opened = file
        .metadata()
        .map_err(|_| "opened source metadata is unavailable")?;
    if !opened.is_file() || opened.len() > MAX_FILE_BYTES as u64 {
        return Err("opened source is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "source file could not be read")?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("source file grew beyond the verification byte limit".into());
    }
    Ok(Some(bytes))
}

fn root_and_scope(root: &Path, host_cwd: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "active project directory could not be resolved")?;
    if !host_cwd.trim().is_empty() {
        let cwd = Path::new(host_cwd)
            .canonicalize()
            .map_err(|_| "host cwd could not be resolved")?;
        if cwd != root {
            return Err(
                "host cwd differs from the project used to resolve repository scope".into(),
            );
        }
    }
    Ok(root)
}

pub(crate) async fn prepare(
    tool: &str,
    input: &Value,
    root: &Path,
    host_cwd: &str,
) -> Result<PreparedEdits, String> {
    let root = root_and_scope(root, host_cwd)?;
    if tool == "apply_patch" {
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .ok_or("apply_patch requires the host's tool_input.command patch body")?;
        prepare_patch(command, root).await
    } else {
        prepare_direct(tool, input, root)
    }
}

async fn prepare_patch(command: &str, root: PathBuf) -> Result<PreparedEdits, String> {
    if command.len() > MAX_TOTAL_BYTES {
        return Err("patch exceeds the verification byte limit".into());
    }
    let args = codex_apply_patch::parse_patch(command)
        .map_err(|_| "patch syntax could not be verified")?;
    if args.hunks.is_empty() || args.hunks.len() > MAX_EDIT_FILES {
        return Err("patch has no files or exceeds the verification file limit".into());
    }
    let mut seen = HashSet::new();
    let mut originals = HashMap::new();
    let mut snapshot = SnapshotFs::default();
    let mut total = 0usize;
    for hunk in &args.hunks {
        let (source, destination, required) = match hunk {
            Hunk::AddFile { path, .. } => (path, None, false),
            Hunk::DeleteFile { path } => (path, None, true),
            Hunk::UpdateFile {
                path, move_path, ..
            } => (path, move_path.as_ref(), true),
        };
        for (raw, must_exist) in
            std::iter::once((source, required)).chain(destination.map(|path| (path, false)))
        {
            let path = safe_path(&root, raw)?;
            // The upstream action map has one value per path; reject ambiguous repeated
            // targets instead of silently losing a hunk in that map.
            if !seen.insert(path.clone()) {
                return Err("patch repeats or aliases a file target".into());
            }
            let bytes = read_bounded(&path)?;
            if must_exist && bytes.is_none() {
                return Err("patch source file does not exist".into());
            }
            if let Some(bytes) = &bytes {
                total = total.saturating_add(bytes.len());
                if total > MAX_TOTAL_BYTES {
                    return Err("source snapshot exceeds the verification byte limit".into());
                }
                snapshot.files.insert(path_uri(&path)?, bytes.clone());
            }
            safe_path(&root, &path)?;
            originals.insert(path, bytes);
        }
    }
    let cwd = path_uri(&root)?;
    let action = match codex_apply_patch::verify_apply_patch_args(args, &cwd, &snapshot, None).await
    {
        MaybeApplyPatchVerified::Body(action) => action,
        _ => return Err("patch could not be applied to the inspected source snapshot".into()),
    };
    let mut edits = Vec::new();
    let mut candidate_bytes = 0usize;
    for (path, change) in action.changes() {
        if matches!(
            change,
            ApplyPatchFileChange::Update {
                move_path: Some(_),
                ..
            }
        ) {
            edits.push(CandidateEdit {
                path: path.to_path_buf(),
                content: None,
            });
        }
        let (path, content) = match change {
            ApplyPatchFileChange::Add { content } => (path, Some(content.clone())),
            ApplyPatchFileChange::Delete { .. } => (path, None),
            ApplyPatchFileChange::Update {
                move_path,
                new_content,
                ..
            } => (
                move_path.as_ref().unwrap_or(path),
                Some(new_content.clone()),
            ),
        };
        if let Some(content) = &content {
            candidate_bytes = candidate_bytes.saturating_add(content.len());
            if content.len() > MAX_FILE_BYTES || candidate_bytes > MAX_TOTAL_BYTES {
                return Err("candidate source exceeds the verification byte limit".into());
            }
        }
        edits.push(CandidateEdit {
            path: path.to_path_buf(),
            content,
        });
    }
    edits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(PreparedEdits {
        edits,
        root,
        originals,
    })
}

fn prepare_direct(tool: &str, input: &Value, root: PathBuf) -> Result<PreparedEdits, String> {
    if !matches!(tool, "Write" | "Edit") {
        return Err("this edit tool has no verified candidate adapter".into());
    }
    let raw = input
        .get("file_path")
        .and_then(Value::as_str)
        .ok_or("edit has no file_path")?;
    let path = safe_path(&root, Path::new(raw))?;
    let before = read_bounded(&path)?;
    let content = if tool == "Write" {
        input
            .get("content")
            .and_then(Value::as_str)
            .ok_or("Write has no content")?
            .to_string()
    } else {
        let old = input
            .get("old_string")
            .and_then(Value::as_str)
            .ok_or("Edit has no old_string")?;
        let new = input
            .get("new_string")
            .and_then(Value::as_str)
            .ok_or("Edit has no new_string")?;
        if old.is_empty() {
            return Err("empty Edit anchors cannot be reconstructed safely".into());
        }
        let original = std::str::from_utf8(before.as_deref().ok_or("Edit source does not exist")?)
            .map_err(|_| "Edit source is not UTF-8")?;
        let count = original.matches(old).count();
        let all = input
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if count == 0 || (!all && count != 1) {
            return Err("Edit anchor is absent or ambiguous".into());
        }
        if all {
            original.replace(old, new)
        } else {
            original.replacen(old, new, 1)
        }
    };
    if content.len() > MAX_FILE_BYTES {
        return Err("candidate source exceeds the verification byte limit".into());
    }
    let originals = HashMap::from([(path.clone(), before)]);
    Ok(PreparedEdits {
        edits: vec![CandidateEdit {
            path,
            content: Some(content),
        }],
        root,
        originals,
    })
}

#[derive(Default)]
struct SnapshotFs {
    files: HashMap<PathUri, Vec<u8>>,
}
fn unsupported<'a, T: Send + 'a>() -> ExecutorFileSystemFuture<'a, T> {
    Box::pin(async {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "hook verifier filesystem is read-only",
        ))
    })
}
impl ExecutorFileSystem for SnapshotFs {
    fn canonicalize<'a>(
        &'a self,
        _: &'a PathUri,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, PathUri> {
        unsupported()
    }
    fn read_file<'a>(
        &'a self,
        path: &'a PathUri,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, Vec<u8>> {
        Box::pin(async move {
            self.files.get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "source absent from bounded snapshot",
                )
            })
        })
    }
    fn read_file_stream<'a>(
        &'a self,
        _: &'a PathUri,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, FileSystemReadStream> {
        unsupported()
    }
    fn write_file<'a>(
        &'a self,
        _: &'a PathUri,
        _: Vec<u8>,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        unsupported()
    }
    fn create_directory<'a>(
        &'a self,
        _: &'a PathUri,
        _: CreateDirectoryOptions,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        unsupported()
    }
    fn get_metadata<'a>(
        &'a self,
        _: &'a PathUri,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, FileMetadata> {
        unsupported()
    }
    fn read_directory<'a>(
        &'a self,
        _: &'a PathUri,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, Vec<ReadDirectoryEntry>> {
        unsupported()
    }
    fn remove<'a>(
        &'a self,
        _: &'a PathUri,
        _: RemoveOptions,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        unsupported()
    }
    fn copy<'a>(
        &'a self,
        _: &'a PathUri,
        _: &'a PathUri,
        _: CopyOptions,
        _: Option<&'a FileSystemSandboxContext>,
    ) -> ExecutorFileSystemFuture<'a, ()> {
        unsupported()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    async fn canonical_patch_reconstructs_every_file_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.py"), "def go():\n    return old()\n").unwrap();
        fs::write(root.join("delete.py"), "gone()\n").unwrap();
        let patch = "*** Begin Patch\n*** Update File: a.py\n*** Move to: renamed.py\n@@\n-    return old()\n+    return new()\n*** Add File: b.py\n+def b():\n+    return 2\n*** Delete File: delete.py\n*** End Patch";
        let edits = prepare("apply_patch", &json!({"command":patch}), root, "")
            .await
            .unwrap();
        assert_eq!(edits.edits.len(), 4);
        assert!(edits.has_deletions());
        assert!(!edits.post_images().unwrap().contains_key("a.py"));
        let files = edits.gate_files().unwrap();
        assert_eq!(files.iter().filter(|file| file.has_added_code).count(), 2);
        assert!(edits.edits.iter().any(|e| e.path.ends_with("renamed.py")
            && e.content.as_deref() == Some("def go():\n    return new()\n")));
        assert!(edits.edits.iter().any(|e| e.path.ends_with("b.py")
            && e.content.as_deref() == Some("def b():\n    return 2\n")));
        assert!(
            edits
                .edits
                .iter()
                .any(|e| e.path.ends_with("delete.py") && e.content.is_none())
        );
        assert_eq!(
            fs::read_to_string(root.join("a.py")).unwrap(),
            "def go():\n    return old()\n"
        );
        assert!(!root.join("b.py").exists());
        assert!(!root.join("renamed.py").exists());
        assert!(root.join("delete.py").exists());
        assert!(edits.unchanged());
        fs::write(root.join("a.py"), "changed\n").unwrap();
        assert!(!edits.unchanged());
    }
    #[tokio::test]
    async fn malformed_duplicate_outside_and_oversized_patches_pause() {
        let dir = tempfile::tempdir().unwrap();
        for body in [
            "not a patch".to_string(),
            "*** Begin Patch\n*** Add File: ../outside.py\n+x\n*** End Patch".to_string(),
            "*** Begin Patch\n*** Add File: a.py\n+x\n*** Add File: a.py\n+y\n*** End Patch"
                .to_string(),
            "x".repeat(MAX_TOTAL_BYTES + 1),
        ] {
            assert!(
                prepare("apply_patch", &json!({"command":body}), dir.path(), "")
                    .await
                    .is_err()
            );
        }
        assert!(
            prepare(
                "apply_patch",
                &json!({"input":"*** Begin Patch\n*** Add File: a.py\n+x\n*** End Patch"}),
                dir.path(),
                ""
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn direct_edit_preserves_full_file_and_exact_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.py"), "def go():\n    return old()\n").unwrap();
        let edits=prepare("Edit",&json!({"file_path":"a.py","old_string":"    return old()","new_string":"    return new()"}),dir.path(),"").await.unwrap();
        assert_eq!(
            edits.edits[0].content.as_deref(),
            Some("def go():\n    return new()\n")
        );
        let write = prepare(
            "Write",
            &json!({"file_path":"b.py","content":"    indented\n"}),
            dir.path(),
            "",
        )
        .await
        .unwrap();
        assert_eq!(write.edits[0].content.as_deref(), Some("    indented\n"));
    }
    #[tokio::test]
    async fn cwd_mismatch_is_not_rebound_silently() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        assert!(
            prepare(
                "Write",
                &json!({"file_path":"a.py","content":"x"}),
                root.path(),
                other.path().to_str().unwrap()
            )
            .await
            .is_err()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn symlinks_and_secret_paths_are_rejected_before_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private.py"), "secret\n").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("alias")).unwrap();
        for path in ["alias/private.py", ".env", ".git/config"] {
            assert!(
                prepare(
                    "Write",
                    &json!({"file_path":path,"content":"x"}),
                    root.path(),
                    ""
                )
                .await
                .is_err()
            );
        }
    }
}
