//! Integration tests for `app_updater` stale-download cleanup and error formatting.
#![allow(clippy::expect_used)] // test code: failing fast is the point

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use app_updater::{remove_stale_downloads, UpdateError};

/// A data folder of its own, removed when the test ends.
struct DataDir(PathBuf);

impl DataDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        Self(std::env::temp_dir().join(format!("cs_upd_{}_{nanos}", std::process::id())))
    }

    /// An update attempt's folder (with a half-downloaded file) last changed `age` ago.
    fn attempt(&self, name: &str, age: Duration) -> PathBuf {
        let folder = self.0.join("updates").join(name);
        std::fs::create_dir_all(&folder).expect("mkdir attempt");
        std::fs::write(folder.join("CloudStream.AppImage.part"), b"partial").expect("write");
        set_folder_time(&folder, SystemTime::now() - age);
        folder
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn set_folder_time(folder: &Path, time: SystemTime) {
    std::fs::File::open(folder)
        .and_then(|handle| handle.set_modified(time))
        .expect("set the folder's time");
}

#[cfg(windows)]
fn set_folder_time(folder: &Path, time: SystemTime) {
    use std::os::windows::fs::OpenOptionsExt;
    /// Lets `CreateFile` open a folder.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(folder)
        .and_then(|handle| handle.set_modified(time))
        .expect("set the folder's time");
}

const HOUR: Duration = Duration::from_secs(3600);

#[test]
fn remove_stale_downloads_removes_attempts_over_an_hour_old_and_keeps_newer_ones() {
    let data = DataDir::new();
    let old = data.attempt("1-old", 3 * HOUR);
    let running = data.attempt("2-running", HOUR / 2);

    remove_stale_downloads(&data.0);

    assert!(!old.exists(), "an attempt from hours ago is removed");
    assert!(
        running.is_dir(),
        "an attempt that may still be downloading is kept"
    );
}

#[test]
fn remove_stale_downloads_without_an_updates_folder_does_nothing() {
    let data = DataDir::new();
    std::fs::create_dir_all(&data.0).expect("mkdir");
    remove_stale_downloads(&data.0);
    assert!(
        !data.0.join("updates").exists(),
        "no folder is made for nothing to clean"
    );
}

#[test]
fn update_error_variants_render_human_readable_messages() {
    assert!(!UpdateError::NoAsset.to_string().is_empty());
    assert!(!UpdateError::NotSigned.to_string().is_empty());
    assert!(!UpdateError::NotNewer {
        latest: "v0.4.8".into()
    }
    .to_string()
    .is_empty());
    assert!(!UpdateError::ChecksumList("bad line".into())
        .to_string()
        .is_empty());
    assert!(!UpdateError::ChecksumMismatch {
        file: "app.AppImage".into()
    }
    .to_string()
    .is_empty());
    assert!(!UpdateError::MissingChecksum {
        file: "app.AppImage".into()
    }
    .to_string()
    .is_empty());
    assert!(
        !UpdateError::ReadOnlyTarget(std::path::PathBuf::from("/opt/app"))
            .to_string()
            .is_empty()
    );
    assert!(!UpdateError::UnsafePath(std::path::PathBuf::from("a\"b"))
        .to_string()
        .is_empty());
}
