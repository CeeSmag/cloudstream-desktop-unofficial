//! The self-updater: finds the latest release, checks its signed checksum list, downloads the file
//! for this system, checks its SHA-256 and hands it to an install helper. Ported from the former
//! Tauri shell; the Slint shell's own updater installed downloads without any check.
#![forbid(unsafe_code)]

mod update_assets;
mod update_download;
mod update_error;
mod update_folder;
mod update_helper_script;
mod update_install;
mod update_prepare;
mod update_verify;
mod update_version;

use std::path::Path;

pub use update_download::DownloadProgress;
pub use update_error::UpdateError;

/// What the latest release offers this installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    /// The running version.
    pub current_version: String,
    /// The latest release's version.
    pub latest_version: String,
    /// True when the latest release is newer.
    pub has_update: bool,
    /// The file this system would install, if the release has one.
    pub asset_name: Option<String>,
    /// Its size in bytes.
    pub asset_size: Option<u64>,
    /// The release's title.
    pub release_name: Option<String>,
    /// The release notes.
    pub release_notes: Option<String>,
}

/// Asks GitHub for the latest release.
pub fn check_latest() -> Result<UpdateCheck, UpdateError> {
    let release = update_download::fetch_latest_release()?;
    let portable = update_install::is_portable_environment();
    let asset = update_assets::select_platform_asset(&release.assets, portable);
    Ok(UpdateCheck {
        current_version: update_download::CURRENT_VERSION.to_owned(),
        latest_version: update_version::clean_version(&release.tag_name).to_owned(),
        has_update: update_prepare::offers_update(&release.tag_name),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_size: asset.map(|asset| asset.size),
        release_name: release.name.clone(),
        release_notes: release.body.clone(),
    })
}

/// Downloads and checks the update into `<data_dir>/updates/`, then starts the helper that
/// replaces the app once it exits. The caller closes the app when this returns `Ok`.
pub fn download_and_install(
    data_dir: &Path,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<(), UpdateError> {
    let prepared = update_prepare::prepare_update(data_dir, on_progress)?;
    update_install::start_install_helper(&prepared.file, &prepared.method, &prepared.folder)?;
    tracing::info!(
        downloaded = %prepared.file.display(),
        method = ?prepared.method,
        "checked update downloaded; the helper installs it when the app closes"
    );
    Ok(())
}

/// Removes update downloads left by earlier attempts (run once at start, off the UI thread).
pub fn remove_stale_downloads(data_dir: &Path) {
    update_folder::cleanup_stale_updates(data_dir);
}
