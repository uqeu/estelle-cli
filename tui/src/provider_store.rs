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
