//! In-app update checker and self-installer for CloudStream desktop. The work lives in the
//! `update_*` files; these commands run it off the window's thread and turn errors into text.
#![forbid(unsafe_code)]

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::data_commands::DataFolder;
use crate::update_assets::select_platform_asset;
use crate::update_download::{fetch_latest_release, DownloadProgress, CURRENT_VERSION};
use crate::update_error::UpdateError;
use crate::update_install::{is_portable_environment, start_install_helper};
use crate::update_prepare::{offers_update, prepare_update};
use crate::update_version::clean_version;

const UPDATE_PROGRESS_EVENT: &str = "update-progress";

/// GitHub release information sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Currently running version (e.g. "0.3.0").
    pub current_version: String,
    /// Latest version available on GitHub (e.g. "0.3.1").
    pub latest_version: String,
    /// True when a newer release exists.
    pub has_update: bool,
    /// Release title/name from GitHub.
    pub release_name: Option<String>,
    /// Release notes / body from GitHub.
    pub release_notes: Option<String>,
    /// Download address of the file for this system (shown only; `apply_update` finds the file
    /// itself and never takes an address from the page).
    pub download_url: Option<String>,
    /// File name of the asset.
    pub asset_name: Option<String>,
    /// Total byte size of the asset, if known.
    pub asset_size: Option<u64>,
    /// Whether the app is currently executing from a portable bundle.
    pub is_portable: bool,
}

/// Download progress event emitted to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressPayload {
    /// Bytes received so far.
    pub downloaded_bytes: u64,
    /// Total bytes expected from Content-Length header.
    pub total_bytes: u64,
    /// Calculated percentage (0-100).
    pub percent: u32,
}

fn check_latest_release() -> Result<UpdateCheckResult, UpdateError> {
    let release = fetch_latest_release()?;
    let is_portable = is_portable_environment();
    let asset = select_platform_asset(&release.assets, is_portable);
    Ok(UpdateCheckResult {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: clean_version(&release.tag_name).to_string(),
        has_update: offers_update(&release.tag_name),
        download_url: asset.map(|asset| asset.browser_download_url.clone()),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_size: asset.map(|asset| asset.size),
        release_name: release.name,
        release_notes: release.body,
        is_portable,
    })
}

/// Checks GitHub for the latest release and returns update status.
#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateCheckResult, String> {
    tauri::async_runtime::spawn_blocking(check_latest_release)
        .await
        .map_err(|error| format!("Update check stopped unexpectedly: {error}"))?
        .map_err(|error| {
            tracing::warn!(%error, "update check failed");
            error.to_string()
        })
}

fn percent_of(progress: DownloadProgress) -> u32 {
    match progress.total_bytes {
        0 => 0,
        total => {
            let percent = progress.downloaded_bytes.saturating_mul(100) / total;
            u32::try_from(percent.min(100)).unwrap_or(100)
        }
    }
}

/// Sends `update-progress` to the page whenever the whole percentage changes.
fn progress_reporter(app: &AppHandle) -> impl FnMut(DownloadProgress) + '_ {
    let mut last_percent = 0;
    move |progress| {
        let percent = percent_of(progress);
        if percent != last_percent {
            last_percent = percent;
            let payload = UpdateProgressPayload {
                downloaded_bytes: progress.downloaded_bytes,
                total_bytes: progress.total_bytes,
                percent,
            };
            // Progress is only a display; the download goes on without it (e.g. while the window closes).
            if let Err(error) = app.emit(UPDATE_PROGRESS_EVENT, payload) {
                tracing::debug!(%error, percent, "cannot send update progress to the page");
            }
        }
    }
}

fn install_update(app: &AppHandle, data_dir: &Path) -> Result<(), UpdateError> {
    let prepared = prepare_update(data_dir, &mut progress_reporter(app))?;
    start_install_helper(&prepared.file, &prepared.target, &prepared.folder)?;
    tracing::info!(
        downloaded = %prepared.file.display(),
        target = %prepared.target.display(),
        "checked update downloaded; closing so the helper can install it"
    );
    app.exit(0);
    Ok(())
}

/// Downloads the latest release's file for this system, checks it against the release's signed
/// checksum list and replaces the application with it. The page cannot choose what is downloaded.
#[tauri::command]
pub async fn apply_update(app: AppHandle, folder: State<'_, DataFolder>) -> Result<(), String> {
    let data_dir = folder.0.clone();
    tauri::async_runtime::spawn_blocking(move || install_update(&app, &data_dir))
        .await
        .map_err(|error| format!("Update stopped unexpectedly: {error}"))?
        .map_err(|error| {
            tracing::warn!(%error, "update failed");
            error.to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(downloaded_bytes: u64, total_bytes: u64) -> DownloadProgress {
        DownloadProgress {
            downloaded_bytes,
            total_bytes,
        }
    }

    #[test]
    fn percent_of_unknown_or_overshooting_size_stays_within_0_to_100() {
        assert_eq!(percent_of(progress(50, 200)), 25);
        assert_eq!(percent_of(progress(200, 200)), 100);
        assert_eq!(percent_of(progress(500, 0)), 0);
        assert_eq!(percent_of(progress(300, 200)), 100);
        assert_eq!(percent_of(progress(u64::MAX, 1)), 100);
    }
}
