//! Built-in pet asset cache ownership.
//!
//! THE CDN FETCH IS DELETED (attack-11 egress audit, 2026-08-13): built-in pets used to
//! download spritesheets from persistent.oaistatic.com on first use. A third-party asset CDN
//! is not the user's provider and not Estelle, so the download path is gone. A built-in pet is
//! available only when a structurally valid spritesheet already exists in the local cache;
//! otherwise the caller gets "asset unavailable" and the pet quietly disables for the session.

#[cfg(test)]
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_http_client::RouteAwareClientPool;

use super::catalog;

const PET_PACK_VERSION: &str = "v1";
const PET_PACK_DIR: &str = "cache/tui-pets";

pub(crate) fn builtin_spritesheet_path(codex_home: &Path, file: &str) -> PathBuf {
    pack_dir(codex_home).join("assets").join(file)
}

/// Ensure that a built-in pet's spritesheet is present and structurally valid — LOCAL CACHE
/// ONLY. There is no download fallback in this build; any error here means "the asset is
/// unavailable", and higher layers treat that as a reason to disable the pet, never to fetch.
pub(crate) async fn ensure_builtin_pet(
    codex_home: &Path,
    pet: catalog::BuiltinPet,
    _http_client: &RouteAwareClientPool,
) -> Result<()> {
    let destination = builtin_spritesheet_path(codex_home, pet.spritesheet_file);
    tokio::task::spawn_blocking(move || validate_cached_spritesheet(&destination))
        .await
        .context("join pet spritesheet cache validation task")?
        .with_context(|| {
            format!(
                "pet asset {} is not available locally",
                pet.spritesheet_file
            )
        })
}

fn pack_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(PET_PACK_DIR).join(PET_PACK_VERSION)
}

fn validate_cached_spritesheet(path: &Path) -> Result<()> {
    let (width, height) =
        image::image_dimensions(path).with_context(|| format!("read {}", path.display()))?;
    if width != catalog::SPRITESHEET_WIDTH || height != catalog::SPRITESHEET_HEIGHT {
        bail!(
            "invalid pet spritesheet dimensions for {}: expected {}x{}, got {}x{}",
            path.display(),
            catalog::SPRITESHEET_WIDTH,
            catalog::SPRITESHEET_HEIGHT,
            width,
            height
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn write_test_pack(codex_home: &Path) {
    let assets_dir = pack_dir(codex_home).join("assets");
    fs::create_dir_all(&assets_dir).unwrap();
    for pet in catalog::BUILTIN_PETS {
        let path = assets_dir.join(pet.spritesheet_file);
        catalog::write_test_spritesheet(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_test_pack_installs_all_builtins() {
        let dir = tempfile::tempdir().unwrap();

        write_test_pack(dir.path());

        for pet in catalog::BUILTIN_PETS {
            let path = builtin_spritesheet_path(dir.path(), pet.spritesheet_file);
            assert!(path.is_file());
            validate_cached_spritesheet(&path).unwrap();
        }
    }
}
