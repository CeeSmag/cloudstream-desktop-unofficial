//! The subtitle files kept on disk, and a list per title of the ones the provider once offered.
//!
//! A file is named after its address, so asking for the same address again finds it at once. A
//! title keeps an append-only list of what it was given, so a title that once had Arabic subtitles
//! still has them when the provider's subtitle service is slow or down.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use core_model::stable_key as cache_key; // names of files kept on disk

const FILES_DIR: &str = "files";
const TITLES_DIR: &str = "titles";
const TITLE_LIST_EXTENSION: &str = "jsonl";

/// A subtitle file that is on disk and ready to be given to the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadySubtitle {
    /// The language as the provider named it.
    pub language: String,
    /// Where the cleaned-up file is.
    pub path: PathBuf,
    /// The address it was downloaded from.
    pub url: String,
}

/// One line of a title's list.
#[derive(Debug, Serialize, Deserialize)]
struct TitleEntry {
    language: String,
    url: String,
    file: String,
}

/// Why something could not be saved.
#[derive(Debug, thiserror::Error)]
#[error("cannot save {what}: {reason}")]
pub struct CacheError {
    what: String,
    reason: String,
}

/// The cache folder.
pub struct SubtitleCache {
    root: PathBuf,
    ttl: Duration,
    temporary: AtomicU64,
}

impl SubtitleCache {
    /// A cache in `root` whose files and lists are forgotten after `ttl`.
    pub fn new(root: PathBuf, ttl: Duration) -> Self {
        Self {
            root,
            ttl,
            temporary: AtomicU64::new(0),
        }
    }

    fn files(&self) -> PathBuf {
        self.root.join(FILES_DIR)
    }

    fn is_fresh(&self, path: &Path) -> bool {
        let age = fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok());
        age.is_some_and(|age| age <= self.ttl)
    }

    /// The saved file for `url`, if there is a fresh, non-empty one.
    pub fn lookup(&self, url: &str) -> Option<PathBuf> {
        let prefix = format!("{}.", cache_key(url));
        fs::read_dir(self.files())
            .ok()?
            .flatten()
            .find_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let usable = name.starts_with(&prefix)
                    && !name.contains(".tmp")
                    && entry.metadata().is_ok_and(|meta| meta.len() > 0)
                    && self.is_fresh(&entry.path());
                usable.then(|| entry.path())
            })
    }

    /// Saves `text` as the file for `url` (written aside and renamed, so a half-written file is
    /// never mistaken for a subtitle) and returns where it is.
    pub fn store(&self, url: &str, text: &str, extension: &str) -> Result<PathBuf, CacheError> {
        let failed = |reason: String| CacheError {
            what: format!("the subtitle of {url}"),
            reason,
        };
        fs::create_dir_all(self.files()).map_err(|error| failed(error.to_string()))?;
        let name = format!("{}.{extension}", cache_key(url));
        let target = self.files().join(&name);
        let aside = self.files().join(format!(
            "{name}.tmp{}-{}",
            std::process::id(),
            self.temporary.fetch_add(1, Ordering::SeqCst)
        ));
        fs::write(&aside, text).map_err(|error| failed(error.to_string()))?;
        fs::rename(&aside, &target).map_err(|error| failed(error.to_string()))?;
        Ok(target)
    }

    fn title_list(&self, title_key: &str) -> PathBuf {
        self.root
            .join(TITLES_DIR)
            .join(format!("{}.{TITLE_LIST_EXTENSION}", cache_key(title_key)))
    }

    /// Notes that `subtitle` was offered for the title, so it is found again next time.
    pub fn remember_for_title(
        &self,
        title_key: &str,
        subtitle: &ReadySubtitle,
    ) -> Result<(), CacheError> {
        let failed = |reason: String| CacheError {
            what: format!("the subtitle list of a title ({title_key})"),
            reason,
        };
        let list = self.title_list(title_key);
        if let Some(folder) = list.parent() {
            fs::create_dir_all(folder).map_err(|error| failed(error.to_string()))?;
        }
        let file = subtitle
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let entry = TitleEntry {
            language: subtitle.language.clone(),
            url: subtitle.url.clone(),
            file,
        };
        let line = serde_json::to_string(&entry).map_err(|error| failed(error.to_string()))?;
        let mut handle = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&list)
            .map_err(|error| failed(error.to_string()))?;
        writeln!(handle, "{line}").map_err(|error| failed(error.to_string()))
    }

    /// The subtitles this title was offered before whose files are still on disk, each once.
    pub fn cached_for_title(&self, title_key: &str) -> Vec<ReadySubtitle> {
        let list = self.title_list(title_key);
        let Ok(text) = fs::read_to_string(&list) else {
            return Vec::new();
        };
        if !self.is_fresh(&list) {
            return Vec::new();
        }
        let mut seen = std::collections::HashSet::new();
        text.lines()
            .filter_map(|line| serde_json::from_str::<TitleEntry>(line).ok())
            .filter(|entry| seen.insert(entry.url.clone()))
            .filter_map(|entry| {
                let path = self.files().join(&entry.file);
                (self.is_fresh(&path) && fs::metadata(&path).is_ok_and(|meta| meta.len() > 0))
                    .then_some(ReadySubtitle {
                        language: entry.language,
                        path,
                        url: entry.url,
                    })
            })
            .collect()
    }

    /// Deletes files and lists older than the time to live; returns how many were removed.
    pub fn prune(&self) -> usize {
        [self.files(), self.root.join(TITLES_DIR)]
            .iter()
            .filter_map(|folder| fs::read_dir(folder).ok())
            .flat_map(|entries| entries.flatten())
            .filter(|entry| !self.is_fresh(&entry.path()))
            .filter(|entry| fs::remove_file(entry.path()).is_ok())
            .count()
    }
}

#[cfg(test)]
#[path = "subtitle_cache_tests.rs"]
mod tests;
