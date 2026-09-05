#[cfg(unix)]
mod symlinks {
    use super::super::resolve_symlink_write_paths;
    use pretty_assertions::assert_eq;
    use std::os::unix::fs::symlink;

    #[test]
    fn symlink_cycles_fall_back_to_root_write_path() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let a = dir.path().join("a");
        let b = dir.path().join("b");

        symlink(&b, &a)?;
        symlink(&a, &b)?;

        let resolved = resolve_symlink_write_paths(&a)?;

        assert_eq!(resolved.read_path, None);
        assert_eq!(resolved.write_path, a);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod wsl {
    use super::super::normalize_for_wsl_with_flag;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn wsl_mnt_drive_paths_lowercase() {
        let normalized =
            normalize_for_wsl_with_flag(PathBuf::from("/mnt/C/Users/Dev"), /*is_wsl*/ true);

        assert_eq!(normalized, PathBuf::from("/mnt/c/users/dev"));
    }

    #[test]
    fn wsl_non_drive_paths_unchanged() {
        let path = PathBuf::from("/mnt/cc/Users/Dev");
        let normalized = normalize_for_wsl_with_flag(path.clone(), /*is_wsl*/ true);

        assert_eq!(normalized, path);
    }

    #[test]
    fn wsl_non_mnt_paths_unchanged() {
        let path = PathBuf::from("/home/Dev");
        let normalized = normalize_for_wsl_with_flag(path.clone(), /*is_wsl*/ true);

        assert_eq!(normalized, path);
    }
}

mod native_workdir {
    use super::super::normalize_for_native_workdir_with_flag;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_verbatim_paths_are_simplified() {
        let path = PathBuf::from(r"\\?\D:\c\x\worktrees\2508\swift-base");
        let normalized = normalize_for_native_workdir_with_flag(path, /*is_windows*/ true);

        assert_eq!(
            normalized,
            PathBuf::from(r"D:\c\x\worktrees\2508\swift-base")
        );
    }

    #[test]
    fn non_windows_paths_are_unchanged() {
        let path = PathBuf::from(r"\\?\D:\c\x\worktrees\2508\swift-base");
        let normalized =
            normalize_for_native_workdir_with_flag(path.clone(), /*is_windows*/ false);

        assert_eq!(normalized, path);
    }
}

mod path_comparison {
    use super::super::paths_match_after_normalization;
    use std::path::PathBuf;

    #[test]
    fn matches_identical_existing_paths() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;

        assert!(paths_match_after_normalization(dir.path(), dir.path()));
        Ok(())
    }

    #[test]
    fn falls_back_to_raw_equality_when_paths_cannot_be_normalized() {
        assert!(paths_match_after_normalization(
            PathBuf::from("missing"),
            PathBuf::from("missing"),
        ));
        assert!(!paths_match_after_normalization(
            PathBuf::from("missing-a"),
            PathBuf::from("missing-b"),
        ));
    }

    #[cfg(windows)]
    #[test]
    fn matches_windows_verbatim_paths() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let verbatim_dir = PathBuf::from(format!(r"\\?\{}", dir.path().display()));

        assert!(paths_match_after_normalization(verbatim_dir, dir.path()));
        Ok(())
    }
}

/// 🔬 **THE SYSCALL-ORDER PROOF.**
///
/// Ported from orca's `durable-file-write-syscall-proof.test.ts` (MIT), whose header names the
/// trap precisely: *"an fsync moved after the rename still fsyncs a file, so a fsync-only log reads
/// identically for the correct and the broken order."* A test that merely asserts "we called
/// `sync_all` twice" passes on the broken sequence. The rename has to be in the same log.
///
/// **Where ours improves on theirs:** orca proves this by monkey-patching `fsyncSync`/`renameSync`
/// on Node's `fs` module, which asserts on a *substituted* implementation and cannot exist in Rust.
/// Here the recorder and production share one code path — `write_durably_with` — so the sequence
/// asserted is the sequence the product executes; only the three syscalls differ. And where orca
/// hardcodes a platform list for directory-fsync support, we record
/// [`DurableStep::DirSyncUnsupported`] as an outcome, so a platform that cannot fsync a directory
/// is a fact in the log rather than an assumption in the assertion.
mod durable_write_order {
    use super::super::*;
    use std::path::Path;

    /// Records what was called, in order, and performs the real effects so the file still lands.
    struct RecordingOps {
        log: Vec<&'static str>,
        dir_sync_supported: bool,
    }

    impl DurableOps for RecordingOps {
        fn sync_file(&mut self, file: &std::fs::File) -> std::io::Result<()> {
            self.log.push("fsync:file");
            file.sync_all()
        }

        fn rename(&mut self, tmp: NamedTempFile, target: &Path) -> std::io::Result<()> {
            self.log.push("rename");
            tmp.persist(target)
                .map(|_| ())
                .map_err(std::io::Error::from)
        }

        fn sync_dir(&mut self, dir: &Path) -> std::io::Result<()> {
            self.log.push("fsync:dir");
            if !self.dir_sync_supported {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "simulated platform without directory fsync",
                ));
            }
            RealDurableOps.sync_dir(dir)
        }
    }

    #[test]
    fn contents_are_fsynced_before_the_rename_and_the_directory_after() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("nested").join("config.toml");
        let mut ops = RecordingOps {
            log: Vec::new(),
            dir_sync_supported: true,
        };
        let mut steps = Vec::new();

        write_durably_with(&mut ops, &target, "model = \"gpt-5.5\"\n", &mut steps)
            .expect("durable write");

        // The rename is IN the log. Without it, moving the file fsync after the rename would
        // produce the identical assertion and this test would certify the broken order.
        assert_eq!(ops.log, vec!["fsync:file", "rename", "fsync:dir"]);
        assert_eq!(
            steps,
            vec![
                DurableStep::SyncFile,
                DurableStep::Rename,
                DurableStep::SyncDir
            ]
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("target readable"),
            "model = \"gpt-5.5\"\n"
        );
    }

    #[test]
    fn a_platform_without_directory_fsync_still_writes_and_says_so() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("config.toml");
        let mut ops = RecordingOps {
            log: Vec::new(),
            dir_sync_supported: false,
        };
        let mut steps = Vec::new();

        // The bytes and the name are already durable by the time the directory fsync is attempted,
        // so an unsupported platform must not turn a good write into an error.
        write_durably_with(&mut ops, &target, "contents", &mut steps).expect("write must succeed");

        assert_eq!(ops.log, vec!["fsync:file", "rename", "fsync:dir"]);
        assert_eq!(
            steps,
            vec![
                DurableStep::SyncFile,
                DurableStep::Rename,
                DurableStep::DirSyncUnsupported
            ],
            "an unsupported directory fsync must be RECORDED, never silently skipped"
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("target readable"),
            "contents"
        );
    }

    #[test]
    fn the_shipped_entry_point_writes_through_the_same_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("a").join("b").join("shipped.toml");
        write_atomically(&target, "durable").expect("write_atomically");
        assert_eq!(
            std::fs::read_to_string(&target).expect("target readable"),
            "durable"
        );
    }
}
