//! Getting a checked update onto disk. The backend reads the latest release itself (the page only
//! says "install"), checks the signed checksum list before downloading anything big, downloads the
//! file for this system into its own folder and compares its SHA-256 with the signed list. Any
//! failure removes the folder; nothing unchecked is ever handed to the install helper.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::update_assets::{
    find_release_file, select_platform_asset, GitHubAsset, GitHubRelease, CHECKSUMS_FILE,
    CHECKSUMS_SIGNATURE_FILE,
};
use crate::update_download::{
    download_update, fetch_latest_release, fetch_release_text, DownloadProgress, CURRENT_VERSION,
};
use crate::update_error::UpdateError;
use crate::update_folder::{create_update_folder, discard_update_folder};
use crate::update_install::{install_method, is_portable_environment, InstallMethod};
use crate::update_verify::{
    check_download, verify_checksums, ReleaseChecksums, UPDATE_SIGNING_KEY,
};
use crate::update_version::is_version_newer;

/// A downloaded and checked update, ready for the install helper.
#[derive(Debug)]
pub(crate) struct PreparedUpdate {
    /// The downloaded file.
    pub(crate) file: PathBuf,
    /// How it replaces the app (move over a file, or run it as the installer).
    pub(crate) method: InstallMethod,
    /// The attempt's own folder, where the helper script goes.
    pub(crate) folder: PathBuf,
}

/// Whether the release tagged `tag` is an update. A tag that is not a version is not offered, because
/// guessing could offer an older release.
pub(crate) fn offers_update(tag: &str) -> bool {
    is_version_newer(tag, CURRENT_VERSION).unwrap_or_else(|error| {
        tracing::warn!(%error, current = CURRENT_VERSION, "not offering the latest release");
        false
    })
}

/// Reads the release's checksum list and checks its signature with the built-in key.
fn fetch_checked_checksums(assets: &[GitHubAsset]) -> Result<ReleaseChecksums, UpdateError> {
    let sums = find_release_file(assets, CHECKSUMS_FILE).ok_or(UpdateError::NotSigned)?;
    let signature =
        find_release_file(assets, CHECKSUMS_SIGNATURE_FILE).ok_or(UpdateError::NotSigned)?;
    let sums = fetch_release_text(&sums.browser_download_url, "download the checksum list")?;
    let signature = fetch_release_text(
        &signature.browser_download_url,
        "download the release signature",
    )?;
    verify_checksums(&sums, &signature, UPDATE_SIGNING_KEY)
}

/// The file for this system and its signed SHA-256, from a release that is newer than this version.
fn choose_checked_file(release: &GitHubRelease) -> Result<(&GitHubAsset, String), UpdateError> {
    if !offers_update(&release.tag_name) {
        return Err(UpdateError::NotNewer {
            latest: release.tag_name.clone(),
        });
    }
    let asset = select_platform_asset(&release.assets, is_portable_environment())
        .ok_or(UpdateError::NoAsset)?;
    let checksums = fetch_checked_checksums(&release.assets)?;
    let expected = checksums.expected_for(&asset.name)?.to_string();
    tracing::info!(tag = %release.tag_name, file = %asset.name, "release signature is valid");
    Ok((asset, expected))
}

/// Downloads `asset` into `folder` and fails unless it matches `expected`.
fn download_checked(
    asset: &GitHubAsset,
    expected: &str,
    folder: &Path,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PathBuf, UpdateError> {
    let file = folder.join(&asset.name);
    let sha256 = download_update(&asset.browser_download_url, &file, on_progress)?;
    check_download(expected, &sha256, &asset.name)?;
    tracing::info!(file = %asset.name, "download matches the signed checksum");
    Ok(file)
}

/// Reads the latest release, checks it and downloads its file for this system into a new folder
/// under `data_dir`. On any failure the folder is removed.
pub(crate) fn prepare_update(
    data_dir: &Path,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PreparedUpdate, UpdateError> {
    let method = install_method()?;
    let release = fetch_latest_release()?;
    let (asset, expected) = choose_checked_file(&release)?;
    let folder = create_update_folder(data_dir, SystemTime::now())?;
    match download_checked(asset, &expected, &folder, on_progress) {
        Ok(file) => Ok(PreparedUpdate {
            file,
            method,
            folder,
        }),
        Err(error) => {
            discard_update_folder(&folder);
            Err(error)
        }
    }
}
