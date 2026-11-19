//! How much disk space the data folder uses, in total and per kind of data (Settings > Storage).

use std::path::Path;

/// The folders the Storage card lists, with their names for people, in the card's order.
pub const MEASURED_FOLDERS: [(&str, &str); 6] = [
    ("Plugins", "extensions"),
    ("Saved sources", "sources"),
    ("Subtitles", "subtitles"),
    ("Posters", "artwork"),
    ("Logs", "logs"),
    ("Updates", "updates"),
];

/// One listed folder and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderUsage {
    /// Its name for people ("Saved sources").
    pub label: &'static str,
    /// Bytes in it, sub-folders included.
    pub bytes: u64,
}

/// The data folder's size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageUsage {
    /// Everything in the data folder.
    pub total_bytes: u64,
    /// The listed folders (the rest, such as the database, only counts in the total).
    pub folders: Vec<FolderUsage>,
}

/// Measures `data_dir`. Unreadable entries count as empty; links are not followed.
pub fn measure_storage(data_dir: &Path) -> StorageUsage {
    StorageUsage {
        total_bytes: size_of(data_dir),
        folders: MEASURED_FOLDERS
            .iter()
            .map(|(label, folder)| FolderUsage {
                label,
                bytes: size_of(&data_dir.join(folder)),
            })
            .collect(),
    }
}

fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return if meta.is_file() { meta.len() } else { 0 };
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|entry| size_of(&entry.path())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_storage_totals_and_lists_folders() {
        let dir = std::env::temp_dir().join(format!("cs_usage_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for folder in ["sources/deep", "logs"] {
            std::fs::create_dir_all(dir.join(folder)).expect("folder");
        }
        std::fs::write(dir.join("sources/deep/a.json"), [0u8; 100]).expect("a");
        std::fs::write(dir.join("logs/cloudstream.log"), [0u8; 40]).expect("log");
        std::fs::write(dir.join("cloudstream.db"), [0u8; 7]).expect("db");
        let usage = measure_storage(&dir);
        assert_eq!(usage.total_bytes, 147);
        let bytes = |label: &str| {
            usage
                .folders
                .iter()
                .find(|folder| folder.label == label)
                .map(|folder| folder.bytes)
        };
        assert_eq!(bytes("Saved sources"), Some(100));
        assert_eq!(bytes("Logs"), Some(40));
        assert_eq!(bytes("Updates"), Some(0), "a missing folder is empty");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
