//! The release as GitHub describes it, and which of its files fits this PC. The files are chosen by
//! name (see `docs/RELEASING.md`): `*-setup.exe`, `*_portable.exe`, and the `*.AppImage` for this CPU.

use serde::Deserialize;

/// The parts of GitHub's "latest release" answer the updater reads.
#[derive(Debug, Deserialize)]
pub(crate) struct GitHubRelease {
    /// The release's tag, e.g. `v0.4.2`.
    pub(crate) tag_name: String,
    /// The release's title.
    pub(crate) name: Option<String>,
    /// The release notes.
    pub(crate) body: Option<String>,
    /// The files attached to the release.
    pub(crate) assets: Vec<GitHubAsset>,
}

/// One file attached to a release.
#[derive(Debug, Deserialize)]
pub(crate) struct GitHubAsset {
    /// File name, e.g. `CloudStream_0.4.2_x64-setup.exe`.
    pub(crate) name: String,
    /// Where it downloads from.
    pub(crate) browser_download_url: String,
    /// Size in bytes.
    pub(crate) size: u64,
}

fn find_named(assets: &[GitHubAsset], fits: impl Fn(&str) -> bool) -> Option<&GitHubAsset> {
    assets.iter().find(|asset| fits(&asset.name.to_lowercase()))
}

/// The release's checksum list (`sha256sum` output for every release file).
pub(crate) const CHECKSUMS_FILE: &str = "SHA256SUMS";

/// The minisign signature of `CHECKSUMS_FILE`.
pub(crate) const CHECKSUMS_SIGNATURE_FILE: &str = "SHA256SUMS.minisig";

/// The release file named exactly `name`.
pub(crate) fn find_release_file<'a>(
    assets: &'a [GitHubAsset],
    name: &str,
) -> Option<&'a GitHubAsset> {
    assets.iter().find(|asset| asset.name == name)
}

/// The Windows file: the portable exe for a portable copy, else the installer. Only an `.exe`: the
/// helper moves the portable file over the running exe as it is.
pub(crate) fn select_windows_asset(
    assets: &[GitHubAsset],
    is_portable: bool,
) -> Option<&GitHubAsset> {
    if is_portable {
        find_named(assets, |name| name.ends_with("portable.exe"))
    } else {
        find_named(assets, |name| name.ends_with("setup.exe"))
    }
}

fn this_cpu() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        ""
    }
}

fn names_cpu(name: &str, cpu: &str) -> bool {
    cpu.is_empty()
        || name.contains(cpu)
        || (cpu == "x86_64" && (name.contains("x64") || name.contains("amd64")))
}

/// The Linux file: the AppImage built for this CPU, else any AppImage.
pub(crate) fn select_linux_asset(assets: &[GitHubAsset]) -> Option<&GitHubAsset> {
    let cpu = this_cpu();
    find_named(assets, |name| {
        name.ends_with(".appimage") && names_cpu(name, cpu)
    })
    .or_else(|| find_named(assets, |name| name.ends_with(".appimage")))
}

/// The file for the system this program runs on; none for systems without a release file.
pub(crate) fn select_platform_asset(
    assets: &[GitHubAsset],
    is_portable: bool,
) -> Option<&GitHubAsset> {
    if cfg!(windows) {
        select_windows_asset(assets, is_portable)
    } else if cfg!(target_os = "linux") {
        select_linux_asset(assets)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.com/{name}"),
            size: 100,
        }
    }

    fn release_files() -> Vec<GitHubAsset> {
        vec![
            asset("CloudStream_0.3.1_x64-setup.exe"),
            asset("CloudStream_0.3.1_x64_portable.exe"),
            asset("CloudStream-0.3.1-aarch64.AppImage"),
            asset("CloudStream-0.3.1-x86_64.AppImage"),
        ]
    }

    fn name_of(asset: Option<&GitHubAsset>) -> Option<&str> {
        asset.map(|asset| asset.name.as_str())
    }

    #[test]
    fn select_windows_asset_portable_or_installer_picks_matching_file() {
        let assets = release_files();
        assert_eq!(
            name_of(select_windows_asset(&assets, true)),
            Some("CloudStream_0.3.1_x64_portable.exe")
        );
        assert_eq!(
            name_of(select_windows_asset(&assets, false)),
            Some("CloudStream_0.3.1_x64-setup.exe")
        );
    }

    #[test]
    fn select_linux_asset_both_cpus_listed_picks_this_cpu() {
        let assets = release_files();
        let expected = match this_cpu() {
            "aarch64" => "CloudStream-0.3.1-aarch64.AppImage",
            _ => "CloudStream-0.3.1-x86_64.AppImage",
        };
        assert_eq!(name_of(select_linux_asset(&assets)), Some(expected));
    }

    #[test]
    fn find_release_file_checksum_names_match_exactly() {
        let assets = vec![asset("SHA256SUMS.minisig"), asset("SHA256SUMS")];
        assert_eq!(
            name_of(find_release_file(&assets, CHECKSUMS_FILE)),
            Some("SHA256SUMS")
        );
        assert_eq!(
            name_of(find_release_file(&assets, CHECKSUMS_SIGNATURE_FILE)),
            Some("SHA256SUMS.minisig")
        );
        assert!(find_release_file(&assets, "sha256sums").is_none());
    }

    #[test]
    fn select_windows_asset_portable_with_only_a_zip_returns_none() {
        // A zip moved over the portable exe would leave a program that cannot start.
        let assets = vec![asset("CloudStream_0.5.0_x64_portable.zip")];
        assert!(select_windows_asset(&assets, true).is_none());
    }

    #[test]
    fn select_asset_no_matching_file_returns_none() {
        let assets = vec![asset("SHA256SUMS"), asset("source.tar.gz")];
        assert!(select_windows_asset(&assets, true).is_none());
        assert!(select_windows_asset(&assets, false).is_none());
        assert!(select_linux_asset(&assets).is_none());
    }
}
