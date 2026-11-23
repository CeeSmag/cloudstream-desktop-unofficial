//! Talking to GitHub: reading the latest release and downloading one of its files.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::update_assets::GitHubRelease;
use crate::update_error::UpdateError;

/// The version of the running program.
pub(crate) const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// How long the release check may take in total.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the whole download may take; the files are 100-250 MB.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
/// How long connecting to GitHub may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The checksum list and its signature are a few hundred bytes; anything far bigger is not one.
const RELEASE_TEXT_LIMIT: u64 = 64 * 1024;
/// Bytes read from the connection per step; each step reports progress.
const CHUNK_BYTES: usize = 64 * 1024;
/// Owner may read, write, run; others may read and run (an AppImage must be runnable).
#[cfg(unix)]
const EXECUTABLE_MODE: u32 = 0o755;

/// How far a download has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Bytes received so far.
    pub downloaded_bytes: u64,
    /// Bytes expected, 0 when the server did not say.
    pub total_bytes: u64,
}

fn http_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .http_status_as_error(true)
        .build()
        .into()
}

fn user_agent() -> String {
    format!("CloudStream-Desktop/{CURRENT_VERSION}")
}

/// The newest published release, pre-releases included (GitHub lists newest first). The
/// `/releases/latest` endpoint is not used: it never returns a pre-release.
pub(crate) fn newest_published(releases: Vec<GitHubRelease>) -> Option<GitHubRelease> {
    releases.into_iter().find(|release| !release.draft)
}

/// Reads the release list at `releases_url` and returns its newest published release,
/// pre-releases included.
pub(crate) fn fetch_latest_release(releases_url: &str) -> Result<GitHubRelease, UpdateError> {
    let response = http_agent(REQUEST_TIMEOUT)
        .get(releases_url)
        .header("User-Agent", &user_agent())
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|error| UpdateError::request("check for updates", error))?;
    let releases: Vec<GitHubRelease> = serde_json::from_reader(response.into_body().into_reader())
        .map_err(UpdateError::Metadata)?;
    newest_published(releases).ok_or_else(|| {
        UpdateError::Stream(std::io::Error::other("GitHub lists no published release"))
    })
}

fn content_length(response: &ureq::http::Response<ureq::Body>) -> u64 {
    response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Copies `reader` into `dest`, telling `on_progress` after every chunk, and returns the SHA-256 of
/// everything copied (hashed on the way, so the file is not read twice).
fn copy_with_progress(
    reader: &mut dyn Read,
    dest: &Path,
    total_bytes: u64,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<[u8; 32], UpdateError> {
    let mut file = File::create(dest).map_err(|error| UpdateError::disk("create", dest, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut downloaded_bytes: u64 = 0;
    loop {
        let read = reader.read(&mut buffer).map_err(UpdateError::Stream)?;
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        file.write_all(chunk)
            .map_err(|error| UpdateError::disk("write", dest, error))?;
        hasher.update(chunk);
        downloaded_bytes += chunk.len() as u64;
        on_progress(DownloadProgress {
            downloaded_bytes,
            total_bytes,
        });
    }
    file.flush()
        .map_err(|error| UpdateError::disk("write", dest, error))?;
    Ok(hasher.finalize().into())
}

/// Downloads `url` to `dest` and returns the file's SHA-256. The file appears under `dest` only
/// once it is complete (it is written to `<dest>.part` first).
pub(crate) fn download_update(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(DownloadProgress),
) -> Result<[u8; 32], UpdateError> {
    let mut partial = dest.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);

    let response = http_agent(DOWNLOAD_TIMEOUT)
        .get(url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| UpdateError::request("download the update", error))?;
    let total_bytes = content_length(&response);
    let mut reader = response.into_body().into_reader();
    let sha256 = copy_with_progress(&mut reader, &partial, total_bytes, on_progress)?;

    std::fs::rename(&partial, dest).map_err(|error| UpdateError::disk("move", dest, error))?;
    make_executable(dest);
    Ok(sha256)
}

/// Downloads a small text file of the release (the checksum list, its signature).
pub(crate) fn fetch_release_text(url: &str, what: &'static str) -> Result<String, UpdateError> {
    http_agent(REQUEST_TIMEOUT)
        .get(url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|error| UpdateError::request(what, error))?
        .into_body()
        .with_config()
        .limit(RELEASE_TEXT_LIMIT)
        .read_to_string()
        .map_err(|error| UpdateError::request(what, error))
}

/// The AppImage must be runnable. If this fails the helper script runs `chmod +x` again, so a failure
/// here is logged, not fatal.
#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::Permissions::from_mode(EXECUTABLE_MODE);
    if let Err(error) = std::fs::set_permissions(path, mode) {
        tracing::warn!(path = %path.display(), %error, "cannot mark the update as runnable");
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_published_skips_drafts_and_keeps_prereleases() {
        let releases: Vec<GitHubRelease> = serde_json::from_str(
            r#"[{"tag_name":"v2","draft":true,"assets":[]},
                {"tag_name":"v1","prerelease":true,"assets":[]}]"#,
        )
        .unwrap();
        assert_eq!(newest_published(releases).unwrap().tag_name, "v1");
    }

    #[test]
    fn newest_published_only_drafts_returns_none() {
        assert!(newest_published(Vec::new()).is_none());
    }

    #[test]
    fn copy_with_progress_several_chunks_copies_all_and_reports_rising_totals() {
        let dir = std::env::temp_dir().join(format!("cs_update_copy_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("file.part");
        let data = vec![7u8; 150 * 1024];
        let mut seen = Vec::new();

        let sha256 = copy_with_progress(&mut data.as_slice(), &dest, 150 * 1024, &mut |progress| {
            seen.push(progress.downloaded_bytes)
        })
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert_eq!(seen.last(), Some(&(150 * 1024)));
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(sha256, <[u8; 32]>::from(Sha256::digest(&data)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_with_progress_known_input_returns_its_sha256() {
        let dir = std::env::temp_dir().join(format!("cs_update_sha_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sha256 = copy_with_progress(&mut b"abc".as_slice(), &dir.join("abc"), 3, &mut |_| {});
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            crate::update_verify::to_hex(&sha256.unwrap()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn copy_with_progress_missing_folder_returns_disk_error() {
        let dest = std::env::temp_dir()
            .join("cs_no_such_folder_9f2c")
            .join("file.part");
        let result = copy_with_progress(&mut [1u8, 2, 3].as_slice(), &dest, 3, &mut |_| {});
        assert!(matches!(
            result,
            Err(UpdateError::Disk {
                action: "create",
                ..
            })
        ));
    }
}
