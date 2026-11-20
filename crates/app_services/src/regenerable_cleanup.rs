//! "Clean up storage now": removes what the app makes again when needed.

use std::path::Path;

/// Folders under the data folder that only hold copies the app downloads or saves again.
pub const REGENERABLE_FOLDERS: [&str; 3] = ["sources", "subtitles", "artwork"];

/// What a cleanup removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanupReport {
    /// Files removed.
    pub files: u64,
    /// Bytes they took.
    pub bytes: u64,
}

impl CleanupReport {
    /// Combines another cleanup pass into this report.
    pub fn add(&mut self, other: Self) {
        self.files = self.files.saturating_add(other.files);
        self.bytes = self.bytes.saturating_add(other.bytes);
    }
}

/// Removes every file in the regenerable folders of `data_dir` (the folders stay).
pub fn clean_regenerable(data_dir: &Path) -> CleanupReport {
    let mut report = CleanupReport::default();
    for folder in REGENERABLE_FOLDERS {
        remove_files_in(&data_dir.join(folder), &mut report);
    }
    tracing::info!(
        files = report.files,
        bytes = report.bytes,
        "regenerable data removed"
    );
    report
}

fn remove_files_in(dir: &Path, report: &mut CleanupReport) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_files_in(&path, report);
            continue;
        }
        let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                report.files += 1;
                report.bytes += bytes;
            }
            Err(error) => tracing::debug!(%error, "a file in use was kept"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_regenerable_removes_only_the_regenerable_folders() {
        let dir = std::env::temp_dir().join(format!("cs_cleanup_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for folder in ["sources", "artwork/nested", "extensions"] {
            std::fs::create_dir_all(dir.join(folder)).expect("folder");
        }
        std::fs::write(dir.join("sources/a.json"), b"12345").expect("a");
        std::fs::write(dir.join("artwork/nested/b.img"), b"123").expect("b");
        std::fs::write(dir.join("extensions/plugin.cs3"), b"keep").expect("plugin");
        std::fs::write(dir.join("cloudstream.db"), b"keep").expect("db");
        let report = clean_regenerable(&dir);
        assert_eq!(report, CleanupReport { files: 2, bytes: 8 });
        assert!(dir.join("extensions/plugin.cs3").exists());
        assert!(dir.join("cloudstream.db").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
