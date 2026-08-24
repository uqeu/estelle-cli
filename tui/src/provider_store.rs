//! Shared private JSON transaction for Estelle-owned provider snapshots.

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;

use serde::Serialize;
use uuid::Uuid;
use zeroize::Zeroizing;

pub(crate) fn write_private_json(
    value: &impl Serialize,
    destination: &Path,
    stem: &str,
) -> io::Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider credential store has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = parent.join(format!(".{stem}.tmp-{}", Uuid::new_v4()));
    let encoded = Zeroizing::new(serde_json::to_vec_pretty(value).map_err(io::Error::other)?);
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
    write_result
}

/// Read back an Estelle-owned provider snapshot.
///
/// 🔴 **ITS ABSENCE WAS THE WHOLE DEFECT.** Until this existed, `write_private_json` had no partner:
/// every login path stored a credential and **no production code path could load it again** — the only
/// reads of this store anywhere in the crate were inside tests. So "provider runtime binding is not yet
/// proven" was not a caveat about a step that might have failed; there was no step, because there was
/// no way to get the credential back out. The founder hit this as four logins in a row that printed a
/// receipt and did nothing.
///
/// ⚠️ A missing file is `Ok(None)`, not an error: "no provider configured" is a normal state and must
/// not read as a failure. Anything else — unreadable, malformed — is an `Err` that NAMES itself, because
/// a corrupt store that reports "not configured" would send the user back through a login that already
/// worked.
pub(crate) fn read_private_json<T: serde::de::DeserializeOwned>(
    destination: &Path,
) -> io::Result<Option<T>> {
    let blob = match fs::read_to_string(destination) {
        Ok(blob) => blob,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_str(&blob).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "provider snapshot at {} is present but unreadable: {error}",
                destination.display()
            ),
        )
    })
}

#[cfg(test)]
mod read_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq, Eq)]
    struct Snapshot {
        base_url: String,
        #[serde(default)]
        api_key: Option<String>,
    }

    #[test]
    fn a_written_snapshot_reads_back_identically() {
        // 🔴 THE ROUND TRIP THAT DID NOT EXIST. `write_private_json` shipped with no partner, so no
        // production path could load a credential back — which is why every login "succeeded" and
        // nothing ever bound. This test is the contract that gap violated.
        #[derive(serde::Serialize)]
        struct Out<'a> {
            base_url: &'a str,
            api_key: Option<&'a str>,
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("providers/local.json");
        write_private_json(
            &Out {
                base_url: "http://localhost:1234/v1",
                api_key: Some("k"),
            },
            &path,
            "local",
        )
        .expect("write");
        let back: Option<Snapshot> = read_private_json(&path).expect("read");
        assert_eq!(
            back,
            Some(Snapshot {
                base_url: "http://localhost:1234/v1".into(),
                api_key: Some("k".into())
            })
        );
    }

    #[test]
    fn a_missing_file_is_none_and_not_an_error() {
        // "No provider configured" is a normal state. Returning Err here would make a fresh install
        // report a failure, and the user would go hunting for a break that is not there.
        let dir = tempfile::tempdir().expect("tempdir");
        let back: io::Result<Option<Snapshot>> = read_private_json(&dir.path().join("absent.json"));
        assert!(matches!(back, Ok(None)));
    }

    #[test]
    fn a_corrupt_file_is_an_error_that_names_itself_never_a_silent_none() {
        // ⚠️ THE DISTINCTION THAT MATTERS. If a malformed store read as "not configured", the user
        // would be sent back through a login that had already worked, chasing a state they cannot
        // see. Absence and corruption are different facts and must not share an answer.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broken.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, b"{not json").expect("write");
        let back: io::Result<Option<Snapshot>> = read_private_json(&path);
        let error = back.expect_err("a corrupt snapshot must not read as absent");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("present but unreadable"));
        assert!(
            error.to_string().contains("broken.json"),
            "it must name the file"
        );
    }
}
