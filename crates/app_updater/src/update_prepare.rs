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
use crate::update_install::{InstallMethod, Installation};
use crate::update_source::UpdateSource;
use crate::update_verify::{check_download, verify_checksums, ReleaseChecksums};
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

/// Reads the release's checksum list and checks its signature with `signing_key`.
fn fetch_checked_checksums(
    assets: &[GitHubAsset],
    signing_key: &str,
) -> Result<ReleaseChecksums, UpdateError> {
    let sums = find_release_file(assets, CHECKSUMS_FILE).ok_or(UpdateError::NotSigned)?;
    let signature =
        find_release_file(assets, CHECKSUMS_SIGNATURE_FILE).ok_or(UpdateError::NotSigned)?;
    let sums = fetch_release_text(&sums.browser_download_url, "download the checksum list")?;
    let signature = fetch_release_text(
        &signature.browser_download_url,
        "download the release signature",
    )?;
    verify_checksums(&sums, &signature, signing_key)
}

/// The file for this system and its signed SHA-256, from a release that is newer than this version.
/// `portable` picks the portable file over the installer.
fn choose_checked_file<'a>(
    release: &'a GitHubRelease,
    signing_key: &str,
    portable: bool,
) -> Result<(&'a GitHubAsset, String), UpdateError> {
    if !offers_update(&release.tag_name) {
        return Err(UpdateError::NotNewer {
            latest: release.tag_name.clone(),
        });
    }
    let asset = select_platform_asset(&release.assets, portable).ok_or(UpdateError::NoAsset)?;
    let checksums = fetch_checked_checksums(&release.assets, signing_key)?;
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

/// Reads the latest release of `source`, checks it and downloads its file for `installation`
/// into a new folder under `data_dir`. On any failure the folder is removed.
pub(crate) fn prepare_update(
    data_dir: &Path,
    source: &UpdateSource<'_>,
    installation: Installation,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PreparedUpdate, UpdateError> {
    let release = fetch_latest_release(source.releases_url)?;
    let (asset, expected) =
        choose_checked_file(&release, source.signing_key, installation.portable)?;
    let folder = create_update_folder(data_dir, SystemTime::now())?;
    match download_checked(asset, &expected, &folder, on_progress) {
        Ok(file) => Ok(PreparedUpdate {
            file,
            method: installation.method,
            folder,
        }),
        Err(error) => {
            discard_update_folder(&folder);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_verify::UPDATE_SIGNING_KEY;

    fn release(tag: &str, assets: Vec<GitHubAsset>) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_owned(),
            draft: false,
            name: None,
            body: None,
            assets,
        }
    }

    #[test]
    fn offers_update_checks_semver_and_rejects_unreadable_tags() {
        assert!(offers_update("v99.0.0"));
        assert!(!offers_update("v0.0.1"));
        assert!(!offers_update(CURRENT_VERSION));
        assert!(!offers_update("not-a-semver-tag"));
    }

    #[test]
    fn choose_checked_file_rejects_not_newer_missing_asset_and_unsigned() {
        assert!(matches!(
            choose_checked_file(&release("v0.0.1", Vec::new()), UPDATE_SIGNING_KEY, false),
            Err(UpdateError::NotNewer { .. })
        ));
        assert!(matches!(
            choose_checked_file(&release("v99.0.0", Vec::new()), UPDATE_SIGNING_KEY, false),
            Err(UpdateError::NoAsset)
        ));
        let asset = GitHubAsset {
            name: format!("CloudStream-99.0.0-{}.AppImage", std::env::consts::ARCH),
            browser_download_url: "https://example.invalid/app.AppImage".into(),
            size: 1024,
        };
        let win_asset = GitHubAsset {
            name: "CloudStream_99.0.0_x64-setup.exe".into(),
            browser_download_url: "https://example.invalid/setup.exe".into(),
            size: 1024,
        };
        let win_portable = GitHubAsset {
            name: "CloudStream_99.0.0_x64_portable.exe".into(),
            browser_download_url: "https://example.invalid/portable.exe".into(),
            size: 1024,
        };
        assert!(matches!(
            choose_checked_file(
                &release("v99.0.0", vec![asset, win_asset, win_portable]),
                UPDATE_SIGNING_KEY,
                false
            ),
            Err(UpdateError::NotSigned)
        ));
    }
}
