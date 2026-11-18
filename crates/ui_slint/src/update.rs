//! GitHub update check / apply helpers for the Slint shell.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const RELEASES_URL: &str =
    "https://api.github.com/repos/CeeSmag/cloudstream-rs-egui/releases/latest";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Result of asking GitHub for a newer build.
#[derive(Debug, Clone)]
pub(crate) struct UpdateInfo {
    pub label: String,
    pub download_url: Option<String>,
}

/// Checks GitHub for a newer release than `current`.
pub(crate) fn check_for_updates(current: &str) -> Result<UpdateInfo, String> {
    let body = http_get(RELEASES_URL)?;
    let release: Release =
        serde_json::from_str(&body).map_err(|error| format!("release JSON: {error}"))?;
    let newer = is_newer(&release.tag_name, current);
    let asset = pick_asset(&release.assets);
    let label = if newer {
        format!(
            "Update available: {} (you have {})",
            release.tag_name, current
        )
    } else {
        format!("Up to date ({current})")
    };
    Ok(UpdateInfo {
        label,
        download_url: asset.map(|a| a.browser_download_url.clone()),
    })
}

/// Downloads the release asset into `dir` and returns the local path.
pub(crate) fn download_update(url: &str, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("update.bin");
    let path = dir.join(name);
    let bytes = http_bytes(url)?;
    fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Starts a helper that replaces the running AppImage/exe after this process exits.
pub(crate) fn apply_downloaded(path: &Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    {
        let script = format!(
            "#!/bin/sh\nwhile kill -0 {pid} 2>/dev/null; do sleep 0.2; done\ncp -f \"{src}\" \"{dst}\"\nchmod +x \"{dst}\"\n\"{dst}\" &\nrm -f \"$0\"\n",
            pid = std::process::id(),
            src = path.display(),
            dst = current.display(),
        );
        let helper = std::env::temp_dir().join("cloudstream-apply-update.sh");
        fs::write(&helper, script).map_err(|error| error.to_string())?;
        let _ = Command::new("chmod").arg("+x").arg(&helper).status();
        Command::new("sh")
            .arg(&helper)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path, current);
        Err("apply_update helper is only wired for Linux AppImage in this build".into())
    }
}

fn pick_asset(assets: &[Asset]) -> Option<&Asset> {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    assets.iter().find(|asset| {
        let name = asset.name.to_lowercase();
        (name.ends_with(".appimage") && name.contains(arch))
            || (name.contains("portable") && name.ends_with(".exe"))
            || name.ends_with("-setup.exe")
    })
}

fn is_newer(remote: &str, current: &str) -> bool {
    let remote = remote.trim_start_matches('v');
    let current = current.trim_start_matches('v');
    match (
        semver::Version::parse(remote),
        semver::Version::parse(current),
    ) {
        (Ok(remote), Ok(current)) => remote > current,
        _ => remote != current,
    }
}

fn http_get(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .header("User-Agent", "cloudstream-rs-egui")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| error.to_string())?;
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .take(1_000_000)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}

fn http_bytes(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .header("User-Agent", "cloudstream-rs-egui")
        .call()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(200_000_000)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
