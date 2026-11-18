//! Downloaded poster files kept on disk, so Home fills at once on the next start.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use core_model::stable_key;

/// Posters older than this are downloaded again (artwork rarely changes, addresses do).
const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// The folder is trimmed, oldest first, to this size.
const MAX_FOLDER_BYTES: u64 = 256 * 1024 * 1024;

/// A folder of downloaded poster files named by a hash of their address.
#[derive(Debug, Clone)]
pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    /// A cache in `dir` (created on the first save).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file_for(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.img", stable_key(url)))
    }

    /// The saved bytes of `url`, unless missing or too old.
    pub(crate) fn load(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.file_for(url);
        let age = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())?;
        if age > MAX_AGE {
            return None;
        }
        std::fs::read(&path).ok()
    }

    /// Saves the bytes of `url`: written whole, then renamed, so a crash never leaves half a file.
    pub(crate) fn save(&self, url: &str, bytes: &[u8]) {
        let path = self.file_for(url);
        let partial = path.with_extension("tmp");
        let written = std::fs::create_dir_all(&self.dir)
            .and_then(|()| std::fs::write(&partial, bytes))
            .and_then(|()| std::fs::rename(&partial, &path));
        if let Err(error) = written {
            tracing::debug!(%error, "a poster was not saved to disk; it is downloaded again next time");
            let _ignored = std::fs::remove_file(&partial);
        }
    }

    /// Removes files older than the limit, then the oldest until the folder fits; returns how many.
    pub fn prune(&self) -> usize {
        let mut files = list_files(&self.dir);
        files.sort_by_key(|file| std::cmp::Reverse(file.modified));
        let mut kept_bytes = 0u64;
        let mut removed = 0;
        for file in files {
            let too_old = file.modified.elapsed().is_ok_and(|age| age > MAX_AGE);
            if too_old || kept_bytes + file.bytes > MAX_FOLDER_BYTES {
                if std::fs::remove_file(&file.path).is_ok() {
                    removed += 1;
                }
            } else {
                kept_bytes += file.bytes;
            }
        }
        removed
    }
}

struct CachedFile {
    path: PathBuf,
    modified: SystemTime,
    bytes: u64,
}

fn list_files(dir: &Path) -> Vec<CachedFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some(CachedFile {
                path: entry.path(),
                modified: meta.modified().ok()?,
                bytes: meta.len(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_artwork_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_then_load_returns_the_same_bytes() {
        let dir = folder("roundtrip");
        let cache = DiskCache::new(&dir);
        assert_eq!(cache.load("https://a.test/p.jpg"), None);
        cache.save("https://a.test/p.jpg", b"poster");
        assert_eq!(cache.load("https://a.test/p.jpg"), Some(b"poster".to_vec()));
        assert_eq!(cache.load("https://a.test/other.jpg"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_keeps_fresh_small_folder() {
        let dir = folder("prune");
        let cache = DiskCache::new(&dir);
        cache.save("https://a.test/1.jpg", b"one");
        cache.save("https://a.test/2.jpg", b"two");
        assert_eq!(cache.prune(), 0);
        assert!(cache.load("https://a.test/1.jpg").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
