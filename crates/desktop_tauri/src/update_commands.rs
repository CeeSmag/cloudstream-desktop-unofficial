//! In-app update checker and self-installer for CloudStream desktop.
#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/CeeSmag/cloudstream-desktop-unofficial/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const UPDATE_PROGRESS_EVENT: &str = "update-progress";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

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
    /// Direct download URL for the target platform asset.
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

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(Duration::from_secs(10)))
        .http_status_as_error(true)
        .build();
    config.into()
}

fn clean_version(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

fn parse_version_numbers(ver: &str) -> Option<(u32, u32, u32)> {
    let clean = clean_version(ver);
    let mut parts = clean.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split('-')
        .next()?
        .parse::<u32>()
        .ok()?;
    Some((major, minor, patch))
}

fn is_version_newer(latest: &str, current: &str) -> bool {
    if let (Some(l), Some(c)) = (
        parse_version_numbers(latest),
        parse_version_numbers(current),
    ) {
        l > c
    } else {
        clean_version(latest) != clean_version(current)
    }
}

fn is_portable_environment() -> bool {
    std::env::var_os("CS_PORTABLE_EXE").is_some()
        || std::env::var_os("APPIMAGE").is_some()
        || std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("portable.txt").exists()))
            .unwrap_or(false)
}

fn select_windows_asset(assets: &[GitHubAsset], is_portable: bool) -> Option<&GitHubAsset> {
    if is_portable {
        assets
            .iter()
            .find(|a| {
                let name = a.name.to_lowercase();
                name.ends_with("portable.exe") || name.ends_with("_portable.exe")
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_lowercase().ends_with("portable.zip"))
            })
    } else {
        assets.iter().find(|a| {
            let name = a.name.to_lowercase();
            name.ends_with("setup.exe") || name.ends_with("-setup.exe")
        })
    }
}

fn select_linux_asset(assets: &[GitHubAsset]) -> Option<&GitHubAsset> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        ""
    };
    assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.ends_with(".appimage")
                && (arch.is_empty()
                    || name.contains(arch)
                    || (arch == "x86_64" && (name.contains("x64") || name.contains("amd64"))))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| a.name.to_lowercase().ends_with(".appimage"))
        })
}

fn select_platform_asset(assets: &[GitHubAsset], is_portable: bool) -> Option<&GitHubAsset> {
    if cfg!(windows) {
        select_windows_asset(assets, is_portable)
    } else if cfg!(target_os = "linux") {
        select_linux_asset(assets)
    } else {
        None
    }
}

/// Checks GitHub for the latest release and returns update status.
#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateCheckResult, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let agent = http_agent(REQUEST_TIMEOUT);
        let user_agent = format!("CloudStream-Desktop/{CURRENT_VERSION}");
        let response = agent
            .get(GITHUB_RELEASES_API)
            .header("User-Agent", &user_agent)
            .header("Accept", "application/vnd.github.v3+json")
            .call()
            .map_err(|e| format!("Failed to check for updates: {e}"))?;

        let release: GitHubRelease = serde_json::from_reader(response.into_body().into_reader())
            .map_err(|e| format!("Failed to parse release metadata: {e}"))?;

        let is_portable = is_portable_environment();
        let has_update = is_version_newer(&release.tag_name, CURRENT_VERSION);
        let asset = select_platform_asset(&release.assets, is_portable);

        Ok(UpdateCheckResult {
            current_version: CURRENT_VERSION.to_string(),
            latest_version: clean_version(&release.tag_name).to_string(),
            has_update,
            release_name: release.name,
            release_notes: release.body,
            download_url: asset.map(|a| a.browser_download_url.clone()),
            asset_name: asset.map(|a| a.name.clone()),
            asset_size: asset.map(|a| a.size),
            is_portable,
        })
    })
    .await
    .map_err(|e| format!("Update check task joined with error: {e}"))?
}

fn check_target_writable(target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        let test_file = parent.join(format!(".cs_update_test_{}", std::process::id()));
        if std::fs::write(&test_file, b"").is_err() {
            return Err(format!(
                "Cannot update: destination '{}' is read-only. Please place the executable in a writable folder.",
                parent.display()
            ));
        }
        let _ = std::fs::remove_file(&test_file);
    }
    Ok(())
}

fn target_executable_path() -> Result<PathBuf, String> {
    if let Some(portable_env) = std::env::var_os("CS_PORTABLE_EXE") {
        let path = PathBuf::from(portable_env);
        if path.exists() {
            check_target_writable(&path)?;
            return Ok(path);
        }
    }
    if cfg!(target_os = "linux") {
        if let Some(appimage_env) = std::env::var_os("APPIMAGE") {
            let path = PathBuf::from(appimage_env);
            if path.exists() {
                check_target_writable(&path)?;
                return Ok(path);
            }
        }
    }
    let current =
        std::env::current_exe().map_err(|e| format!("Cannot locate running executable: {e}"))?;
    check_target_writable(&current)?;
    Ok(current)
}

#[cfg(windows)]
fn spawn_updater_and_exit(app: &AppHandle, downloaded: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let pid = std::process::id();
    let script_path = std::env::temp_dir().join(format!("cs_update_{pid}.bat"));

    let script_content = format!(
        "@echo off\r\n\
         :wait\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         tasklist /fi \"PID eq {pid}\" | find \"{pid}\" >nul\r\n\
         if not errorlevel 1 goto wait\r\n\
         move /y \"{downloaded}\" \"{target}\"\r\n\
         start \"\" \"{target}\"\r\n\
         del \"%~f0\"\r\n",
        pid = pid,
        downloaded = downloaded.display(),
        target = target.display()
    );

    std::fs::write(&script_path, script_content)
        .map_err(|e| format!("Failed to write update helper: {e}"))?;

    let mut cmd = std::process::Command::new("cmd.exe");
    cmd.args(["/c", &script_path.to_string_lossy()]);
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd.spawn()
        .map_err(|e| format!("Failed to spawn update process: {e}"))?;

    app.exit(0);
    Ok(())
}

#[cfg(not(windows))]
fn spawn_updater_and_exit(app: &AppHandle, downloaded: &Path, target: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let pid = std::process::id();
    let script_path = std::env::temp_dir().join(format!("cs_update_{pid}.sh"));

    let script_content = format!(
        "#!/bin/sh\n\
         while kill -0 {pid} 2>/dev/null; do sleep 0.5; done\n\
         sleep 0.5\n\
         chmod +x \"{downloaded}\" 2>/dev/null || true\n\
         mv -f \"{downloaded}\" \"{target}\" || cp -f \"{downloaded}\" \"{target}\"\n\
         chmod +x \"{target}\" 2>/dev/null || true\n\
         unset APPDIR\n\
         nohup \"{target}\" >/dev/null 2>&1 &\n\
         rm -f \"$0\"\n",
        pid = pid,
        downloaded = downloaded.display(),
        target = target.display()
    );

    std::fs::write(&script_path, script_content)
        .map_err(|e| format!("Failed to write update helper: {e}"))?;
    let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));

    std::process::Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|e| format!("Failed to spawn update process: {e}"))?;

    app.exit(0);
    Ok(())
}

fn emit_progress_if_changed(app: &AppHandle, downloaded: u64, total: u64, last_pct: &mut u32) {
    let pct = if total > 0 {
        ((downloaded as f64 / total as f64) * 100.0) as u32
    } else {
        0
    };
    if pct != *last_pct {
        *last_pct = pct;
        let _ = app.emit(
            UPDATE_PROGRESS_EVENT,
            UpdateProgressPayload {
                downloaded_bytes: downloaded,
                total_bytes: total,
                percent: pct,
            },
        );
    }
}

fn stream_to_file(
    app: &AppHandle,
    response: ureq::http::Response<ureq::Body>,
    dest: &Path,
) -> Result<(), String> {
    let total_bytes = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut reader = response.into_body().into_reader();
    let mut file =
        std::fs::File::create(dest).map_err(|e| format!("Failed to create temporary file: {e}"))?;

    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded_bytes: u64 = 0;
    let mut last_percent: u32 = 0;

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Error while downloading stream: {e}"))?;
        if bytes_read == 0 {
            break;
        }
        let chunk = buffer
            .get(..bytes_read)
            .ok_or_else(|| "Buffer slice out of bounds".to_string())?;
        file.write_all(chunk)
            .map_err(|e| format!("Error writing file chunk: {e}"))?;

        downloaded_bytes += bytes_read as u64;
        emit_progress_if_changed(app, downloaded_bytes, total_bytes, &mut last_percent);
    }

    file.flush()
        .map_err(|e| format!("Failed to flush downloaded file: {e}"))?;
    Ok(())
}

fn download_and_finalize(
    app: &AppHandle,
    download_url: &str,
    target: &Path,
) -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let extension = target.extension().and_then(|e| e.to_str()).unwrap_or("exe");
    let temp_dest = temp_dir.join(format!("CloudStream_update.{extension}"));
    let temp_part = temp_dir.join(format!("CloudStream_update.{extension}.part"));

    let agent = http_agent(DOWNLOAD_TIMEOUT);
    let user_agent = format!("CloudStream-Desktop/{CURRENT_VERSION}");
    let response = agent
        .get(download_url)
        .header("User-Agent", &user_agent)
        .call()
        .map_err(|e| format!("Failed to download update: {e}"))?;

    stream_to_file(app, response, &temp_part)?;

    if temp_dest.exists() {
        let _ = std::fs::remove_file(&temp_dest);
    }
    std::fs::rename(&temp_part, &temp_dest)
        .map_err(|e| format!("Failed to finalize update payload: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp_dest, std::fs::Permissions::from_mode(0o755));
    }

    Ok(temp_dest)
}

/// Downloads the specified release asset and replaces the application.
#[tauri::command]
pub async fn apply_update(app: AppHandle, download_url: String) -> Result<(), String> {
    let app_handle = Arc::new(app);
    let app_clone = Arc::clone(&app_handle);

    tauri::async_runtime::spawn_blocking(move || {
        let target = target_executable_path()?;
        let temp_dest = download_and_finalize(&app_clone, &download_url, &target)?;
        spawn_updater_and_exit(&app_clone, &temp_dest, &target)
    })
    .await
    .map_err(|e| format!("Update download task joined with error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_clean_and_compare() {
        assert!(is_version_newer("v0.3.1", "0.3.0"));
        assert!(is_version_newer("0.4.0", "0.3.0"));
        assert!(is_version_newer("v1.0.0", "0.3.0"));
        assert!(!is_version_newer("v0.3.0", "0.3.0"));
        assert!(!is_version_newer("v0.2.9", "0.3.0"));
        assert!(!is_version_newer("0.3.0", "0.3.0"));
    }

    #[test]
    fn asset_selection_picks_matching_target() {
        let assets = vec![
            GitHubAsset {
                name: "CloudStream_0.3.1_x64-setup.exe".to_string(),
                browser_download_url: "https://example.com/setup.exe".to_string(),
                size: 100,
            },
            GitHubAsset {
                name: "CloudStream_0.3.1_x64_portable.exe".to_string(),
                browser_download_url: "https://example.com/portable.exe".to_string(),
                size: 150,
            },
            GitHubAsset {
                name: "CloudStream-0.3.1-x86_64.AppImage".to_string(),
                browser_download_url: "https://example.com/appimage".to_string(),
                size: 200,
            },
        ];

        if cfg!(target_os = "linux") {
            let selected = select_platform_asset(&assets, true);
            assert_eq!(
                selected.map(|a| a.name.as_str()),
                Some("CloudStream-0.3.1-x86_64.AppImage")
            );
        } else if cfg!(windows) {
            let portable = select_platform_asset(&assets, true);
            assert_eq!(
                portable.map(|a| a.name.as_str()),
                Some("CloudStream_0.3.1_x64_portable.exe")
            );

            let installer = select_platform_asset(&assets, false);
            assert_eq!(
                installer.map(|a| a.name.as_str()),
                Some("CloudStream_0.3.1_x64-setup.exe")
            );
        }
    }
}
