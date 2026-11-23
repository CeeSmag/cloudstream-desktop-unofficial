//! Where an update is downloaded to: a new folder `<data>/updates/<pid>-<nanos>/` for every attempt.
//! It is inside the user's own data folder, not the shared temp folder, where on a PC with several
//! users someone else could swap the file between the check and the install. The helper script that
//! installs the update is written there too.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::update_error::UpdateError;

/// Name of the folder inside the data folder that holds update attempts.
const UPDATES_FOLDER: &str = "updates";

/// Attempts older than this are removed when a new one starts. An attempt still running cannot be
/// this old: the download stops after 5 minutes (`update_download::DOWNLOAD_TIMEOUT`).
const STALE_AFTER: Duration = Duration::from_secs(60 * 60);

/// Makes a fresh folder for one update attempt, first removing attempts older than `STALE_AFTER`
/// (their install helper has run long ago, or they failed).
pub(crate) fn create_update_folder(
    data_dir: &Path,
    now: SystemTime,
) -> Result<PathBuf, UpdateError> {
    let updates = data_dir.join(UPDATES_FOLDER);
    std::fs::create_dir_all(&updates)
        .map_err(|error| UpdateError::disk("create", &updates, error))?;
    remove_stale_attempts(&updates, now);
    let nanos = now
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let folder = updates.join(format!("{}-{nanos}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|error| UpdateError::disk("create", &folder, error))?;
    Ok(folder)
}

fn is_stale(entry: &std::fs::DirEntry, now: SystemTime) -> bool {
    entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age > STALE_AFTER)
}

/// Removes old attempts. Failing to remove one is logged and does not stop the new attempt: it only
/// costs disk space, and the next attempt tries again.
fn remove_stale_attempts(updates: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(updates) else {
        return;
    };
    for entry in entries.flatten().filter(|entry| is_stale(entry, now)) {
        let path = entry.path();
        match std::fs::remove_dir_all(&path) {
            Ok(()) => tracing::info!(path = %path.display(), "removed an old update attempt"),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "cannot remove an old update attempt")
            }
        }
    }
}

/// Deletes leftover update downloads under `<data>/updates/` (hourly cleanup and Clean up now).
pub(crate) fn cleanup_stale_updates(data_dir: &Path) {
    let updates = data_dir.join(UPDATES_FOLDER);
    if !updates.is_dir() {
        tracing::debug!(dir = %updates.display(), "no updates folder to tidy");
        return;
    }
    remove_stale_attempts(&updates, SystemTime::now());
}

/// Removes a failed attempt's folder. A failure is only logged: the next attempt removes it later.
pub(crate) fn discard_update_folder(folder: &Path) {
    if let Err(error) = std::fs::remove_dir_all(folder) {
        tracing::warn!(path = %folder.display(), %error, "cannot remove a failed update attempt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_data_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cs_update_folder_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn create_update_folder_fresh_data_dir_makes_folder_under_updates() {
        let data_dir = test_data_dir("fresh");
        let folder = create_update_folder(&data_dir, SystemTime::now()).unwrap();
        assert!(folder.is_dir());
        assert_eq!(
            folder.parent(),
            Some(data_dir.join(UPDATES_FOLDER).as_path())
        );
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn create_update_folder_two_hours_later_removes_the_old_attempt() {
        let data_dir = test_data_dir("stale");
        let old = create_update_folder(&data_dir, SystemTime::now()).unwrap();
        let later = SystemTime::now() + Duration::from_secs(2 * 60 * 60);
        let new = create_update_folder(&data_dir, later).unwrap();
        assert!(!old.exists());
        assert!(new.is_dir());
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn create_update_folder_recent_attempt_is_kept() {
        let data_dir = test_data_dir("recent");
        let first = create_update_folder(&data_dir, SystemTime::now()).unwrap();
        // Half an hour on: well past a download's 5-minute limit, still not an hour old.
        let soon = SystemTime::now() + Duration::from_secs(30 * 60);
        let second = create_update_folder(&data_dir, soon).unwrap();
        assert!(first.is_dir() && second.is_dir() && first != second);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn discard_update_folder_removes_it_with_its_files() {
        let data_dir = test_data_dir("discard");
        let folder = create_update_folder(&data_dir, SystemTime::now()).unwrap();
        std::fs::write(folder.join("CloudStream_update.exe.part"), b"half").unwrap();
        discard_update_folder(&folder);
        assert!(!folder.exists());
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
