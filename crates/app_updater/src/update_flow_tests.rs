//! T2: the whole update path (release list, signed checksum list, download, SHA-256 check)
//! against a local stand-in for GitHub. Nothing unchecked may reach the install helper, and a
//! release whose signature fails must not even be downloaded.

use std::path::{Path, PathBuf};

use crate::test_release_server::{
    file_bytes, publish, release_file_names, test_key, ReleasePlan, ReleaseServer,
};
use crate::update_install::{InstallMethod, Installation};
use crate::update_prepare::prepare_update;
use crate::update_source::UpdateSource;
use crate::{check_latest_from, DownloadProgress, UpdateError};

/// The file the updater picks here for an installed (not portable) copy.
fn this_systems_file() -> String {
    let [appimage, setup, _portable] = release_file_names();
    if cfg!(windows) {
        setup
    } else {
        appimage
    }
}

/// A data folder of its own, removed when the test ends.
struct DataDir(PathBuf);

impl DataDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cs_update_flow_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("data folder");
        Self(dir)
    }

    /// The update attempts left in `<data>/updates/`.
    fn attempts(&self) -> usize {
        std::fs::read_dir(self.0.join("updates")).map_or(0, |entries| entries.count())
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn installed() -> Installation {
    Installation {
        method: InstallMethod::ReplaceFile(PathBuf::from("/opt/cloudstream/cloudstream")),
        portable: false,
    }
}

/// Runs the update against `server`, checking with `public_key`; returns the progress seen too.
fn prepare(
    data: &Path,
    server: &ReleaseServer,
    public_key: &str,
) -> (
    Result<crate::update_prepare::PreparedUpdate, UpdateError>,
    Vec<DownloadProgress>,
) {
    let releases = server.url("/releases");
    let source = UpdateSource {
        releases_url: &releases,
        signing_key: public_key,
    };
    let mut seen = Vec::new();
    let result = prepare_update(data, &source, installed(), &mut |progress| {
        seen.push(progress)
    });
    (result, seen)
}

/// A signed release published on a fresh server, prepared into a fresh data folder.
struct Prepared {
    server: ReleaseServer,
    data: DataDir,
    result: Result<crate::update_prepare::PreparedUpdate, UpdateError>,
    seen: Vec<DownloadProgress>,
}

fn prepare_signed(tag: &str) -> Prepared {
    let (keys, public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: true,
        },
    );
    let data = DataDir::new(tag);
    let (result, seen) = prepare(&data.0, &server, &public_key);
    Prepared {
        server,
        data,
        result,
        seen,
    }
}

#[test]
fn a_signed_release_is_downloaded_checked_and_handed_over() {
    let run = prepare_signed("ok");
    let prepared = run.result.expect("a checked update");
    let name = this_systems_file();
    assert_eq!(
        prepared.file.file_name().and_then(|n| n.to_str()),
        Some(name.as_str())
    );
    assert_eq!(
        std::fs::read(&prepared.file).expect("downloaded"),
        file_bytes(&name)
    );
    assert!(prepared.file.starts_with(run.data.0.join("updates")));
    assert_eq!(prepared.folder, prepared.file.parent().expect("a folder"));
    assert_eq!(prepared.method, installed().method);
    let total = file_bytes(&name).len() as u64;
    assert_eq!(
        run.seen.last(),
        Some(&DownloadProgress {
            downloaded_bytes: total,
            total_bytes: total
        })
    );
}

#[test]
fn the_downloaded_file_is_complete_runnable_and_asked_for_by_name() {
    let run = prepare_signed("details");
    let prepared = run.result.expect("a checked update");
    let version = format!("CloudStream-Desktop/{}", env!("CARGO_PKG_VERSION"));
    let agents = run.server.user_agents();
    assert!(
        !agents.is_empty() && agents.iter().all(|agent| *agent == version),
        "GitHub refuses requests without a User-Agent: {agents:?}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&prepared.file)
            .expect("file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "the AppImage must be runnable");
    }
    let mut unfinished = prepared.file.clone().into_os_string();
    unfinished.push(".part");
    assert!(
        !PathBuf::from(unfinished).exists(),
        "the partial file was moved into place"
    );
}

#[test]
fn a_download_that_does_not_match_the_signed_list_is_thrown_away() {
    let (keys, public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: true,
        },
    );
    // Someone between the app and GitHub swaps the file; the signed list still names the real one.
    server.serve(
        &format!("/files/{}", this_systems_file()),
        200,
        b"tampered".to_vec(),
    );
    let data = DataDir::new("tampered");

    let (result, _) = prepare(&data.0, &server, &public_key);

    assert!(
        matches!(result, Err(UpdateError::ChecksumMismatch { .. })),
        "{result:?}"
    );
    assert_eq!(data.attempts(), 0, "the failed attempt's folder is removed");
}

#[test]
fn a_list_signed_by_someone_else_stops_the_update_before_anything_big_is_downloaded() {
    let (keys, _) = test_key();
    let (_, our_public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: true,
        },
    );
    let data = DataDir::new("foreign");

    let (result, _) = prepare(&data.0, &server, &our_public_key);

    assert!(
        matches!(result, Err(UpdateError::Signature(_))),
        "{result:?}"
    );
    assert!(!server.downloaded_a_release_file());
    assert_eq!(
        data.attempts(),
        0,
        "no folder is made for a release that fails its check"
    );
}

#[test]
fn an_unsigned_release_is_not_installed_and_not_downloaded() {
    let (keys, public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: false,
        },
    );
    let data = DataDir::new("unsigned");

    let (result, _) = prepare(&data.0, &server, &public_key);

    assert!(matches!(result, Err(UpdateError::NotSigned)), "{result:?}");
    assert!(!server.downloaded_a_release_file());
}

#[test]
fn a_download_the_host_refuses_is_a_request_error_and_leaves_nothing_behind() {
    let (keys, public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: true,
        },
    );
    server.serve(
        &format!("/files/{}", this_systems_file()),
        404,
        b"gone".to_vec(),
    );
    let data = DataDir::new("refused");

    let (result, _) = prepare(&data.0, &server, &public_key);

    assert!(
        matches!(result, Err(UpdateError::Request { .. })),
        "{result:?}"
    );
    assert_eq!(data.attempts(), 0);
}

#[test]
fn a_release_list_that_cannot_be_read_is_a_metadata_error() {
    let server = ReleaseServer::start();
    server.serve("/releases", 200, b"<html>rate limited</html>".to_vec());
    let data = DataDir::new("garbled");
    let (_, public_key) = test_key();

    let (result, _) = prepare(&data.0, &server, &public_key);

    assert!(
        matches!(result, Err(UpdateError::Metadata(_))),
        "{result:?}"
    );
}

#[test]
fn a_list_of_only_drafts_offers_nothing() {
    let server = ReleaseServer::start();
    server.serve(
        "/releases",
        200,
        br#"[{"tag_name":"v100.0.0","draft":true,"assets":[]}]"#.to_vec(),
    );
    let releases = server.url("/releases");
    let source = UpdateSource {
        releases_url: &releases,
        signing_key: "",
    };
    assert!(check_latest_from(&source, false).is_err());
}

#[test]
fn the_check_reports_the_newest_published_release_and_the_file_for_this_system() {
    let (keys, public_key) = test_key();
    let server = ReleaseServer::start();
    publish(
        &server,
        &ReleasePlan {
            keys: &keys,
            signed: true,
        },
    );
    let releases = server.url("/releases");
    let source = UpdateSource {
        releases_url: &releases,
        signing_key: &public_key,
    };

    let check = check_latest_from(&source, false).expect("checked");

    let name = this_systems_file();
    assert!(check.has_update);
    assert_eq!(
        check.latest_version, "99.0.0",
        "the draft above it is skipped"
    );
    assert_eq!(check.current_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(check.asset_name.as_deref(), Some(name.as_str()));
    assert_eq!(check.asset_size, Some(file_bytes(&name).len() as u64));
    assert_eq!(check.release_name.as_deref(), Some("Big release"));
    assert_eq!(check.release_notes.as_deref(), Some("notes"));
    assert!(
        !server.downloaded_a_release_file(),
        "a check downloads nothing"
    );
}

#[test]
fn a_checksum_list_of_many_files_is_read_whole_and_one_past_the_limit_is_refused() {
    use crate::update_download::fetch_release_text;
    let server = ReleaseServer::start();
    // A release with dozens of files (several systems, debug symbols) lists a few kilobytes.
    let long = "0".repeat(64) + "  file.bin\n";
    let list = long.repeat(100);
    server.serve("/sums", 200, list.clone());
    let read = fetch_release_text(&server.url("/sums"), "download the checksum list");
    assert_eq!(read.expect("read"), list);

    server.serve("/huge", 200, vec![b'a'; 64 * 1024 + 1]);
    let refused = fetch_release_text(&server.url("/huge"), "download the checksum list");
    assert!(
        matches!(refused, Err(UpdateError::Request { .. })),
        "{refused:?}"
    );
}
