//! "Delete all saved data". A running app cannot delete its own database (Windows keeps open files
//! locked), so the settings page only leaves a marker file and restarts the app; the wipe runs at the
//! next start, before the database, the logs or the extensions are opened.

use std::io;
use std::path::Path;
use std::time::Duration;

/// Marker the settings page leaves to ask for a wipe at the next start.
pub const WIPE_MARKER: &str = "delete_all_data.pending";

/// Folders that start like this hold the program the portable exe unpacked beside the data
/// (`runtime-<version>`); the wipe must leave them, they are what is running.
const KEEP_PREFIX: &str = "runtime-";

/// How often a wipe tries again when something is still in use (the app that asked for it may take a
/// moment to exit), and the pause between tries.
#[derive(Debug, Clone, Copy)]
pub struct RetryPlan {
    /// Passes over the data folder; at least one is always made.
    pub attempts: u32,
    /// Wait between two passes.
    pub pause: Duration,
}

impl Default for RetryPlan {
    /// About ten seconds in all.
    fn default() -> Self {
        Self {
            attempts: 50,
            pause: Duration::from_millis(200),
        }
    }
}

/// What a start-up wipe did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WipeOutcome {
    /// Nobody asked for a wipe.
    NotRequested,
    /// Everything was removed.
    Done {
        /// Files and folders removed.
        removed: usize,
    },
    /// Some entries could not be removed; the marker stays so the next start tries again.
    Failed {
        /// Entries still there.
        remaining: usize,
    },
}

impl WipeOutcome {
    /// Writes the result to the log (logging starts after the wipe, so this is called afterwards).
    pub fn log(&self) {
        match self {
            Self::NotRequested => {}
            Self::Done { removed } => {
                tracing::info!(removed, "all saved data was deleted, as requested");
            }
            Self::Failed { remaining } => {
                tracing::warn!(
                    remaining,
                    "could not delete all saved data; will retry at the next start"
                );
            }
        }
    }
}

/// Asks for everything saved to be deleted at the next start.
pub fn request_wipe(data_dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(
        data_dir.join(WIPE_MARKER),
        b"delete everything at the next start\n",
    )
}

/// Deletes everything in `data_dir` when a wipe was requested; call it first thing at start-up.
pub fn wipe_if_requested(data_dir: &Path, retry: RetryPlan) -> WipeOutcome {
    // A folder without a parent is a drive root: never empty that, whatever a marker says.
    if data_dir.parent().is_none() || !data_dir.join(WIPE_MARKER).is_file() {
        return WipeOutcome::NotRequested;
    }
    let attempts = retry.attempts.max(1);
    let mut removed = 0;
    let mut remaining = 0;
    for attempt in 1..=attempts {
        let pass = remove_entries(data_dir);
        removed += pass.removed;
        remaining = pass.failed;
        if remaining == 0 {
            return finish(data_dir, removed);
        }
        if attempt < attempts {
            std::thread::sleep(retry.pause);
        }
    }
    WipeOutcome::Failed { remaining }
}

/// Removes the marker, last, so a wipe that stopped halfway is repeated.
fn finish(data_dir: &Path, removed: usize) -> WipeOutcome {
    match std::fs::remove_file(data_dir.join(WIPE_MARKER)) {
        Ok(()) => WipeOutcome::Done { removed },
        Err(_) => WipeOutcome::Failed { remaining: 1 },
    }
}

#[derive(Debug, Default)]
struct Pass {
    removed: usize,
    failed: usize,
}

/// One pass over the folder: removes every entry except the marker and the kept program folders.
fn remove_entries(data_dir: &Path) -> Pass {
    let mut pass = Pass::default();
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        pass.failed += 1;
        return pass;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == WIPE_MARKER || name.starts_with(KEEP_PREFIX) {
            continue;
        }
        let path = entry.path();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => pass.removed += 1,
            Err(_) => pass.failed += 1,
        }
    }
    pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const QUICK: RetryPlan = RetryPlan {
        attempts: 1,
        pause: Duration::ZERO,
    };

    /// A fresh data folder with some saved data in it.
    fn data_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_wipe_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subtitles")).expect("create subtitles");
        std::fs::create_dir_all(dir.join("logs")).expect("create logs");
        std::fs::write(dir.join("cloudstream.db"), b"database").expect("write db");
        std::fs::write(dir.join("subtitles").join("a.vtt"), b"cue").expect("write subtitle");
        dir
    }

    #[test]
    fn without_a_request_nothing_is_touched() {
        let dir = data_folder("idle");
        assert_eq!(wipe_if_requested(&dir, QUICK), WipeOutcome::NotRequested);
        assert!(dir.join("cloudstream.db").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_not_a_request() {
        let dir = std::env::temp_dir().join("cs_wipe_missing_folder_that_does_not_exist");
        assert_eq!(wipe_if_requested(&dir, QUICK), WipeOutcome::NotRequested);
    }

    #[test]
    fn a_request_removes_files_and_folders_and_then_the_marker() {
        let dir = data_folder("all");
        request_wipe(&dir).expect("request");
        let outcome = wipe_if_requested(&dir, QUICK);
        assert_eq!(outcome, WipeOutcome::Done { removed: 3 });
        let left: Vec<_> = std::fs::read_dir(&dir).expect("read").flatten().collect();
        assert!(left.is_empty(), "left behind: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_unpacked_portable_program_is_kept() {
        let dir = data_folder("keep");
        std::fs::create_dir_all(dir.join("runtime-0.3.4")).expect("runtime folder");
        std::fs::write(dir.join("runtime-0.3.4").join("desktop_tauri.exe"), b"exe").expect("exe");
        request_wipe(&dir).expect("request");
        assert!(matches!(
            wipe_if_requested(&dir, QUICK),
            WipeOutcome::Done { .. }
        ));
        assert!(dir
            .join("runtime-0.3.4")
            .join("desktop_tauri.exe")
            .is_file());
        assert!(!dir.join("cloudstream.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_start_does_nothing_more() {
        let dir = data_folder("twice");
        request_wipe(&dir).expect("request");
        let _ = wipe_if_requested(&dir, QUICK);
        std::fs::write(dir.join("cloudstream.db"), b"new data").expect("new data");
        assert_eq!(wipe_if_requested(&dir, QUICK), WipeOutcome::NotRequested);
        assert!(dir.join("cloudstream.db").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drive_root_is_never_emptied() {
        let root = PathBuf::from(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(wipe_if_requested(&root, QUICK), WipeOutcome::NotRequested);
    }
}
