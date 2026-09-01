//! Durable per-pane scrollback, bounded on the way in and on the way out.
//!
//! Ported from Orca's `src/main/terminal-scrollback-snapshots.ts` (MIT).
//!
//! Four properties, each of which is a bug if you drop it:
//!
//! 1. **Two different bounds.** We store up to [`SCROLLBACK_STORE_BYTE_LIMIT`] and replay only
//!    [`SCROLLBACK_REPLAY_BYTE_LIMIT`]. See `crate::terminal_scrollback_limits`.
//!
//! 2. **A 5 MiB file never enters memory whole.** [`read_snapshot`] stats the file and seeks to
//!    `size - length`, so the read is bounded by the REPLAY limit rather than by the file. A
//!    read-then-truncate would allocate ten times what it returns, at exactly the moment the user
//!    is waiting for a pane to come back.
//!
//! 3. **Truncation keeps the TAIL and never splits a character.** The end of a terminal buffer is
//!    the part anyone wants. Slicing a byte offset out of UTF-8 text produces either a panic or a
//!    replacement glyph, so both the write path and the read path walk forward off any
//!    continuation byte they land on.
//!
//! 4. **The ref is re-validated before EVERY path construction.** Not once at the boundary, not
//!    when it is minted: every single time, immediately before it is joined to a root. A ref is
//!    persisted in session state, which means it is attacker-reachable for anyone who can write
//!    that file, and a ref that reached us from disk has no provenance at all. Validating at the
//!    join is what makes traversal unrepresentable rather than merely unlikely.
//!
//! Writes are atomic: a temporary file, then a rename, so a crash mid-write leaves the previous
//! snapshot intact rather than a half-written one. The directory is `0o700` and the file `0o600`,
//! because scrollback is a verbatim record of the user's terminal and routinely contains
//! credentials they pasted.

use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;
use std::path::PathBuf;

use sha2::Digest;
use sha2::Sha256;

use crate::terminal_scrollback_limits::SCROLLBACK_REPLAY_BYTE_LIMIT;
use crate::terminal_scrollback_limits::SCROLLBACK_STORE_BYTE_LIMIT;

/// Version prefix on every ref, so a future format change is distinguishable rather than
/// ambiguous.
const REF_PREFIX: &str = "v1-";
/// The hash is truncated to this many hex characters. Fixed width is what makes the validator a
/// shape check rather than a parse.
const REF_HEX_LEN: usize = 32;

/// A validated snapshot ref. The only thing that can be turned into a path.
///
/// Construction is the validation, so a `SnapshotRef` in hand is a proof, not a promise. Note
/// that [`snapshot_path`] revalidates anyway: the type stops an unvalidated string from getting
/// in, and the revalidation stops a future refactor from removing that guarantee silently.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct SnapshotRef(String);

impl SnapshotRef {
    /// Mint a ref for a pane. Deterministic, so the same pane finds its own snapshot again.
    pub(crate) fn for_pane(session_id: &str, pane_id: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(session_id.as_bytes());
        // A separator that cannot occur in either component, so ("a", "bc") and ("ab", "c") are
        // different panes rather than the same digest.
        hasher.update([0u8]);
        hasher.update(pane_id.as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(REF_PREFIX.len() + REF_HEX_LEN);
        out.push_str(REF_PREFIX);
        for byte in digest.iter().take(REF_HEX_LEN / 2) {
            out.push_str(&format!("{byte:02x}"));
        }
        SnapshotRef(out)
    }

    /// Accept a ref that came from somewhere we do not control, or refuse it.
    ///
    /// Strict by construction: a fixed prefix and exactly [`REF_HEX_LEN`] LOWERCASE hex digits and
    /// nothing else. There is no separator, no dot and no case folding to reason about, so there
    /// is no encoding of `..`, of a path separator, of a NUL, or of a drive letter that survives
    /// this. Refusing is always safe here; the cost of a rejected ref is a pane that starts empty.
    pub(crate) fn parse(candidate: &str) -> Option<Self> {
        let rest = candidate.strip_prefix(REF_PREFIX)?;
        if rest.len() != REF_HEX_LEN {
            return None;
        }
        // `is_ascii_hexdigit` would accept uppercase, which would let two distinct refs name one
        // file on a case-insensitive filesystem.
        if !rest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        Some(SnapshotRef(candidate.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The path a ref names under `root`, or `None` if the ref does not survive revalidation.
///
/// Revalidated on every call, on purpose. See the module docs.
pub(crate) fn snapshot_path(root: &Path, snapshot_ref: &SnapshotRef) -> Option<PathBuf> {
    let revalidated = SnapshotRef::parse(snapshot_ref.as_str())?;
    Some(root.join(format!("{}.bin", revalidated.as_str())))
}

/// The tail of `text` that fits in `max_bytes`, cut on a character boundary.
///
/// Walks FORWARD off a continuation byte, so the result is always at most `max_bytes` and always
/// valid UTF-8. Returns a borrowed slice: nothing is copied to decide a buffer already fits.
pub(crate) fn trailing_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut start = text.len() - max_bytes;
    // Bounded by `text.len()`; `is_char_boundary` is true at `text.len()` so this terminates.
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Write `buffer`'s tail for `snapshot_ref`, atomically.
///
/// Returns `Ok(false)` when the ref does not revalidate, which is a refusal rather than an error:
/// there is nothing the caller can do about a bad ref except not use it.
pub(crate) fn write_snapshot(
    root: &Path,
    snapshot_ref: &SnapshotRef,
    buffer: &str,
) -> std::io::Result<bool> {
    let Some(path) = snapshot_path(root, snapshot_ref) else {
        return Ok(false);
    };
    create_private_dir(root)?;

    let bytes = trailing_utf8(buffer, SCROLLBACK_STORE_BYTE_LIMIT);
    // A named temporary in the SAME directory, so the rename below is atomic rather than a
    // cross-device copy that can be observed half-done.
    let mut temp = tempfile::Builder::new()
        .prefix(".scrollback-")
        .suffix(".tmp")
        .tempfile_in(root)?;
    set_private_file_mode(temp.as_file())?;
    std::io::Write::write_all(&mut temp, bytes.as_bytes())?;
    std::io::Write::flush(&mut temp)?;
    temp.persist(&path).map_err(|err| err.error)?;
    Ok(true)
}

/// Read back at most [`SCROLLBACK_REPLAY_BYTE_LIMIT`] from the END of the snapshot.
///
/// The whole file is never read. A missing snapshot is `Ok(None)`, not an error: a pane with no
/// history is the ordinary case on a first run.
pub(crate) fn read_snapshot(
    root: &Path,
    snapshot_ref: &SnapshotRef,
) -> std::io::Result<Option<String>> {
    let Some(path) = snapshot_path(root, snapshot_ref) else {
        return Ok(None);
    };
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let size = file.metadata()?.len();
    let length = size.min(SCROLLBACK_REPLAY_BYTE_LIMIT as u64);
    if length == 0 {
        return Ok(Some(String::new()));
    }
    // The seek is the point: a 5 MiB file costs us `length` bytes of memory, not 5 MiB.
    file.seek(SeekFrom::Start(size - length))?;
    let mut bytes = vec![0u8; length as usize];
    file.read_exact(&mut bytes)?;

    // The seek can land inside a character. Walk forward off any continuation byte.
    let mut start = 0usize;
    while start < bytes.len() && (bytes[start] & 0xc0) == 0x80 {
        start += 1;
    }
    Ok(Some(String::from_utf8_lossy(&bytes[start..]).into_owned()))
}

/// Best-effort removal. A stale snapshot is harmless and bounded by the per-file cap.
pub(crate) fn delete_snapshot(root: &Path, snapshot_ref: &SnapshotRef) -> std::io::Result<()> {
    let Some(path) = snapshot_path(root, snapshot_ref) else {
        return Ok(());
    };
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn create_private_dir(root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_file_mode(file: &fs::File) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = file;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_ref() -> SnapshotRef {
        SnapshotRef::for_pane("session-1", "pane-1")
    }

    // ------------------------------------------------- the ref validator

    #[test]
    fn a_hostile_ref_can_never_become_a_path() {
        // Every shape that turns a join into an escape, plus the near-misses that a length check
        // alone or a `contains("..")` check alone would let through.
        let hostile = [
            "../../../etc/passwd",
            "v1-../../../etc/passwd",
            "v1-../../../../../../../../etc/shadow",
            "v1-/etc/passwd",
            "v1-..",
            "v1-.",
            // Right prefix, right length, wrong alphabet: `.` and `/` are not hex.
            "v1-................................",
            "v1-../../../../../../../../../../..",
            "v1-0000000000000000000000000000000/",
            // Right shape but uppercase: two refs naming one file on a case-folding filesystem.
            "v1-00000000000000000000000000000ABC",
            // Off-by-one in both directions.
            "v1-0000000000000000000000000000000",
            "v1-000000000000000000000000000000000",
            // Absolute and UNC paths.
            "/etc/passwd",
            "\\\\server\\share",
            "C:\\Windows\\System32",
            // Embedded separators and terminators.
            "v1-0000000000000000000000000000000\u{0}",
            "v1-00000000\u{0}0000000000000000000000",
            "v1-000000000000000\n0000000000000000",
            // Wrong or absent prefix.
            "v2-00000000000000000000000000000000",
            "00000000000000000000000000000000",
            "",
            "v1-",
        ];
        for candidate in hostile {
            assert!(
                SnapshotRef::parse(candidate).is_none(),
                "the validator accepted a hostile ref: {candidate:?}"
            );
        }
    }

    #[test]
    fn a_hostile_ref_that_somehow_reached_a_path_call_is_still_refused() {
        // The revalidation is not decoration. Construct the newtype BY HAND, bypassing `parse`
        // exactly the way a future refactor or a deserializer would, and prove the path
        // construction still refuses it. Without the revalidation in `snapshot_path` this test
        // produces `/root/../../../etc/passwd.bin`.
        let smuggled = SnapshotRef("v1-../../../etc/passwd".to_string());
        assert_eq!(snapshot_path(Path::new("/snapshots"), &smuggled), None);
    }

    #[test]
    fn a_valid_ref_stays_directly_under_the_root() {
        let root = Path::new("/snapshots");
        let Some(path) = snapshot_path(root, &a_ref()) else {
            panic!("a minted ref must produce a path");
        };
        // The strongest available statement: the parent IS the root, with no traversal in between.
        assert_eq!(path.parent(), Some(root));
        assert!(!path.to_string_lossy().contains(".."));
        assert!(path.to_string_lossy().ends_with(".bin"));
    }

    #[test]
    fn a_minted_ref_survives_its_own_validator_and_is_pane_specific() {
        let minted = a_ref();
        assert_eq!(SnapshotRef::parse(minted.as_str()).as_ref(), Some(&minted));
        assert_eq!(minted.as_str().len(), REF_PREFIX.len() + REF_HEX_LEN);

        // Deterministic for the same pane...
        assert_eq!(SnapshotRef::for_pane("s", "p"), SnapshotRef::for_pane("s", "p"));
        // ...distinct across panes, and the separator makes the split unambiguous.
        assert_ne!(SnapshotRef::for_pane("s", "p"), SnapshotRef::for_pane("s", "q"));
        assert_ne!(SnapshotRef::for_pane("ab", "c"), SnapshotRef::for_pane("a", "bc"));
    }

    // ------------------------------------------------ UTF-8 safe truncation

    #[test]
    fn truncation_keeps_the_tail_and_never_splits_a_character() {
        // Three-byte characters against a limit that is NOT a multiple of three, so the naive
        // `&text[text.len() - max..]` lands mid-character and panics.
        let text = "\u{4e2d}".repeat(1_000);
        let kept = trailing_utf8(&text, 100);

        // Valid UTF-8 of whole characters only...
        assert_eq!(kept.chars().count(), 33);
        assert_eq!(kept.len(), 99);
        assert!(kept.len() <= 100, "the cut exceeded the limit");
        assert!(kept.chars().all(|c| c == '\u{4e2d}'));
        // ...and it is the TAIL, which is the half a terminal user wants.
        assert!(text.ends_with(kept));
    }

    #[test]
    fn truncation_is_tail_preserving_across_every_offset_of_a_mixed_string() {
        // Sweep every limit across mixed character widths. A boundary bug shows at exactly one
        // offset, so a single-limit test would very likely miss it.
        let text = "a\u{e9}\u{4e2d}\u{1F600}".repeat(50);
        for max in 0..=text.len() {
            let kept = trailing_utf8(&text, max);
            assert!(kept.len() <= max, "limit {max} exceeded: {} bytes", kept.len());
            assert!(text.ends_with(kept), "limit {max} did not keep the tail");
            // Losing at most 3 bytes to the boundary walk is the most that can be right.
            assert!(
                max - kept.len() < 4,
                "limit {max} discarded {} bytes to find a boundary",
                max - kept.len()
            );
        }
    }

    #[test]
    fn a_buffer_under_the_limit_is_returned_whole() {
        let text = "\u{1F600} short";
        assert_eq!(trailing_utf8(text, SCROLLBACK_STORE_BYTE_LIMIT), text);
        assert_eq!(trailing_utf8(text, text.len()), text);
    }

    // ---------------------------------------------------- the round trip

    #[test]
    fn a_snapshot_round_trips_and_the_read_is_bounded_by_the_replay_limit() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        let snapshot_ref = a_ref();

        // Deliberately larger than the STORE limit, so both bounds are exercised at once.
        let buffer = "\u{4e2d}".repeat(3 * 1024 * 1024);
        assert!(buffer.len() > SCROLLBACK_STORE_BYTE_LIMIT);

        let Ok(true) = write_snapshot(dir.path(), &snapshot_ref, &buffer) else {
            panic!("the write refused a minted ref");
        };
        let Ok(Some(replayed)) = read_snapshot(dir.path(), &snapshot_ref) else {
            panic!("the snapshot did not read back");
        };

        // Bounded by REPLAY, not by STORE and not by the buffer.
        assert!(
            replayed.len() <= SCROLLBACK_REPLAY_BYTE_LIMIT,
            "replayed {} bytes, over the replay limit",
            replayed.len()
        );
        assert!(
            replayed.len() > SCROLLBACK_REPLAY_BYTE_LIMIT - 4,
            "replayed only {} bytes; the read is not filling the window",
            replayed.len()
        );

        // The file on disk kept the STORE limit, which is the larger of the two. If these were
        // one constant, this pair of assertions could not both hold.
        let Some(path) = snapshot_path(dir.path(), &snapshot_ref) else {
            panic!("path");
        };
        let Ok(meta) = fs::metadata(&path) else {
            panic!("no snapshot on disk");
        };
        assert!(meta.len() <= SCROLLBACK_STORE_BYTE_LIMIT as u64);
        assert!(meta.len() > SCROLLBACK_REPLAY_BYTE_LIMIT as u64);

        // What came back is the TAIL of what went in, still whole characters.
        assert!(buffer.ends_with(&replayed));
        assert!(replayed.chars().all(|c| c == '\u{4e2d}'));
        assert!(!replayed.contains('\u{FFFD}'), "the seek split a character");
    }

    #[test]
    fn the_seek_lands_mid_character_and_the_read_still_returns_whole_ones() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        let snapshot_ref = a_ref();
        // 3-byte characters filling more than the replay window guarantee the seek offset
        // `size - 512KiB` is not on a boundary: 512*1024 is not divisible by 3.
        let buffer = "\u{4e2d}".repeat(SCROLLBACK_REPLAY_BYTE_LIMIT);
        let Ok(true) = write_snapshot(dir.path(), &snapshot_ref, &buffer) else {
            panic!("write");
        };
        let Ok(Some(replayed)) = read_snapshot(dir.path(), &snapshot_ref) else {
            panic!("read");
        };
        assert_ne!(SCROLLBACK_REPLAY_BYTE_LIMIT % 3, 0, "the fixture is not exercising the hazard");
        assert!(!replayed.contains('\u{FFFD}'));
        assert!(replayed.chars().all(|c| c == '\u{4e2d}'));
        assert!(buffer.ends_with(&replayed));
    }

    #[test]
    fn a_missing_snapshot_reads_as_absent_rather_than_failing() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        assert_eq!(
            read_snapshot(dir.path(), &SnapshotRef::for_pane("never", "written")).ok(),
            Some(None)
        );
    }

    #[test]
    fn a_rewrite_replaces_the_previous_snapshot_whole() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        let snapshot_ref = a_ref();
        let _ = write_snapshot(dir.path(), &snapshot_ref, "the long first buffer");
        let _ = write_snapshot(dir.path(), &snapshot_ref, "second");
        let Ok(Some(replayed)) = read_snapshot(dir.path(), &snapshot_ref) else {
            panic!("read");
        };
        // A write that appended, or that left the tail of the longer previous body behind, shows
        // up right here.
        assert_eq!(replayed, "second");

        // ...and the atomic write left no temporary files behind.
        let Ok(entries) = fs::read_dir(dir.path()) else {
            panic!("read_dir");
        };
        let names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "stray files in the snapshot dir: {names:?}");
        assert!(names[0].ends_with(".bin"), "unexpected file: {names:?}");
    }

    #[test]
    fn deleting_is_idempotent_and_a_deleted_snapshot_reads_as_absent() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        let snapshot_ref = a_ref();
        let _ = write_snapshot(dir.path(), &snapshot_ref, "body");
        assert!(delete_snapshot(dir.path(), &snapshot_ref).is_ok());
        assert!(delete_snapshot(dir.path(), &snapshot_ref).is_ok());
        assert_eq!(read_snapshot(dir.path(), &snapshot_ref).ok(), Some(None));
    }

    #[cfg(unix)]
    #[test]
    fn scrollback_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(dir) = tempfile::tempdir() else {
            panic!("no temp dir");
        };
        let root = dir.path().join("terminal-scrollback");
        let snapshot_ref = a_ref();
        // Scrollback is a verbatim record of the terminal and routinely holds pasted credentials.
        let _ = write_snapshot(&root, &snapshot_ref, "export TOKEN=hunter2");

        let Some(path) = snapshot_path(&root, &snapshot_ref) else {
            panic!("path");
        };
        let (Ok(dir_meta), Ok(file_meta)) = (fs::metadata(&root), fs::metadata(&path)) else {
            panic!("no snapshot on disk");
        };
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);
    }
}
