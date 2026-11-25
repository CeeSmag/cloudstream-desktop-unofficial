//! Where the setup is downloaded to: a new folder `<data>/webview2-setup/<pid>-<nanos>/` for every
//! attempt, inside the user's own data folder rather than the shared temp folder (another user
//! could swap a file there between the signature check and the start). The folder is removed when
//! the attempt ends, whichever way it ends.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::SetupError;

/// The folder inside the data folder that holds attempts.
pub const SETUP_FOLDER: &str = "webview2-setup";
/// The downloaded setup's file name.
pub const SETUP_FILE: &str = "MicrosoftEdgeWebview2Setup.exe";
/// Attempts older than this are left-overs (an attempt is capped far below it).
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// One attempt's folder; removed on drop.
#[derive(Debug)]
pub struct AttemptFolder {
    path: PathBuf,
}

impl AttemptFolder {
    /// Makes a fresh folder for one attempt, first removing attempts older than an hour.
    pub fn create(data_dir: &Path, now: SystemTime) -> Result<Self, SetupError> {
        let root = data_dir.join(SETUP_FOLDER);
        std::fs::create_dir_all(&root)
            .map_err(|error| SetupError::disk("create", &root, &error))?;
        remove_stale(&root, now);
        let nanos = now
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let path = root.join(format!("{}-{nanos}", std::process::id()));
        std::fs::create_dir(&path).map_err(|error| SetupError::disk("create", &path, &error))?;
        Ok(Self { path })
    }

    /// Where the setup is written.
    pub fn setup_file(&self) -> PathBuf {
        self.path.join(SETUP_FILE)
    }

    /// The folder itself.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for AttemptFolder {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            // Only costs disk space; the next attempt removes it once it is old.
            tracing::warn!(path = %self.path.display(), %error, "cannot remove a WebView2 setup folder");
        }
    }
}

fn is_stale(entry: &std::fs::DirEntry, now: SystemTime) -> bool {
    entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(is_old)
}

/// Whether an attempt of this age is a left-over.
fn is_old(age: Duration) -> bool {
    age > STALE_AFTER
}

fn remove_stale(root: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten().filter(|entry| is_stale(entry, now)) {
        let path = entry.path();
        if let Err(error) = std::fs::remove_dir_all(&path) {
            tracing::warn!(path = %path.display(), %error, "cannot remove an old WebView2 setup folder");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_wv2_folder_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_attempt_gets_its_own_folder_that_goes_when_dropped() {
        let data = data_dir("drop");
        let folder = AttemptFolder::create(&data, SystemTime::now()).unwrap();
        assert_eq!(
            folder.path().parent(),
            Some(data.join(SETUP_FOLDER).as_path())
        );
        assert_eq!(folder.setup_file(), folder.path().join(SETUP_FILE));
        std::fs::write(folder.setup_file(), b"setup").unwrap();
        let path = folder.path().to_path_buf();
        drop(folder);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn old_attempts_are_removed_and_recent_ones_kept() {
        let data = data_dir("stale");
        let old = AttemptFolder::create(&data, SystemTime::now()).unwrap();
        let old_path = old.path().to_path_buf();
        std::mem::forget(old);
        let soon = SystemTime::now() + Duration::from_secs(30 * 60);
        let recent = AttemptFolder::create(&data, soon).unwrap();
        assert!(old_path.is_dir(), "half an hour is not old");
        let later = SystemTime::now() + Duration::from_secs(2 * 60 * 60);
        let newest = AttemptFolder::create(&data, later).unwrap();
        assert!(!old_path.exists(), "two hours is old");
        assert!(newest.path().is_dir());
        drop((recent, newest));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn attempts_are_old_only_after_a_full_hour() {
        assert!(!is_old(Duration::ZERO));
        assert!(!is_old(STALE_AFTER));
        assert!(is_old(STALE_AFTER + Duration::from_nanos(1)));
    }

    #[test]
    fn an_unwritable_data_folder_is_a_disk_error() {
        let data = data_dir("blocked");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(SETUP_FOLDER), b"a file, not a folder").unwrap();
        let error = AttemptFolder::create(&data, SystemTime::now()).unwrap_err();
        assert!(
            matches!(
                error,
                SetupError::Disk {
                    action: "create",
                    ..
                }
            ),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&data);
    }
}
