//! Lightweight GitHub release check for the egui product (no Tauri updater).

use std::io::Read;

use serde::Deserialize;

/// Latest release summary for the Settings card.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// Tag of the newer release, e.g. `v0.4.9`.
    pub tag: String,
    /// Download URL for this PC's asset, when one was found.
    pub download_url: Option<String>,
    /// Asset file name.
    pub asset_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Asks GitHub for the latest `CeeSmag/cloudstream-rs-egui` release newer than `current`.
pub fn check_for_update(current: &str) -> Result<Option<UpdateInfo>, String> {
    let url = "https://api.github.com/repos/CeeSmag/cloudstream-rs-egui/releases/latest";
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
    let release: GitHubRelease =
        serde_json::from_str(&body).map_err(|error| format!("release JSON: {error}"))?;
    if !is_newer(&release.tag_name, current)? {
        return Ok(None);
    }
    let asset = pick_asset(&release.assets);
    Ok(Some(UpdateInfo {
        tag: release.tag_name,
        download_url: asset.map(|item| item.browser_download_url.clone()),
        asset_name: asset.map(|item| item.name.clone()),
    }))
}

fn is_newer(latest: &str, current: &str) -> Result<bool, String> {
    let clean = |tag: &str| -> String {
        tag.trim().trim_start_matches(['v', 'V']).to_owned()
    };
    let latest = semver::Version::parse(&clean(latest)).map_err(|e| e.to_string())?;
    let current = semver::Version::parse(&clean(current)).map_err(|e| e.to_string())?;
    if !latest.pre.is_empty() && current.pre.is_empty() {
        return Ok(false);
    }
    Ok(latest.cmp_precedence(&current).is_gt())
}

fn pick_asset(assets: &[GitHubAsset]) -> Option<&GitHubAsset> {
    let lower = |name: &str| name.to_ascii_lowercase();
    if cfg!(windows) {
        let portable = std::env::var_os("CS_PORTABLE_EXE").is_some();
        assets.iter().find(|asset| {
            let name = lower(&asset.name);
            if portable {
                name.ends_with("portable.exe")
            } else {
                name.ends_with("setup.exe")
            }
        })
    } else {
        let cpu = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        };
        assets
            .iter()
            .find(|asset| {
                let name = lower(&asset.name);
                name.ends_with(".appimage") && name.contains(cpu)
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| lower(&asset.name).ends_with(".appimage"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_tag_is_detected() {
        assert!(is_newer("v0.4.9", "0.4.8").unwrap());
        assert!(!is_newer("v0.4.8", "0.4.8").unwrap());
    }
}
