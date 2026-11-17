//! Only the video opened last keeps its subtitle files.
//!
//! Opening a video removes the lists of every other title and every file the open title's list
//! does not name, so the previous video downloads its subtitles again when it is opened again. A
//! download of the previous video that finishes late is not noted in a list and its file is
//! removed ([`SubtitleCache::remember_for_title`]); both happen under the same lock as the removal.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::subtitle_cache::{ReadySubtitle, SubtitleCache, TitleEntry, TITLES_DIR};

/// Part of the name of a file that is still being written; such files are left to the cleanup.
const BEING_WRITTEN: &str = ".tmp";

impl SubtitleCache {
    /// Makes `title_key` the only title with saved subtitles: the lists of all other titles and
    /// every file its own list does not name are removed. Returns how many were removed.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn keep_only_title(&self, title_key: &str) -> usize {
        let mut kept = self.kept_lock();
        *kept = Some(title_key.to_owned());
        let own_list = self.title_list(title_key);
        let named = self.files_named_by(title_key);
        let lists = remove_files_where(&self.root.join(TITLES_DIR), |path| path != own_list);
        let files = remove_files_where(&self.files(), |path| {
            let name = file_name(path);
            !name.contains(BEING_WRITTEN) && !named.contains(&name)
        });
        if lists + files > 0 {
            tracing::info!(
                lists,
                files,
                "dropped the subtitles of earlier videos; only the open one keeps its own"
            );
        }
        lists + files
    }

    /// The names of the files the list of `title_key` names.
    fn files_named_by(&self, title_key: &str) -> HashSet<String> {
        fs::read_to_string(self.title_list(title_key))
            .map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str::<TitleEntry>(line).ok())
                    .map(|entry| entry.file)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A download for a video that was replaced finished: its file goes, unless the title that
    /// is open (`open`) names it too. The caller holds the kept-title lock.
    pub(crate) fn drop_for_replaced(&self, open: &str, subtitle: &ReadySubtitle) {
        if self
            .files_named_by(open)
            .contains(&file_name(&subtitle.path))
        {
            return;
        }
        tracing::debug!("a subtitle of a replaced video arrived late; removing it");
        if let Err(error) = fs::remove_file(&subtitle.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "cannot remove a subtitle of a replaced video");
            }
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Removes the files in `folder` that `wanted_gone` picks; returns how many went.
fn remove_files_where(folder: &Path, wanted_gone: impl Fn(&Path) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(folder) else {
        return 0; // nothing was ever saved there
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && wanted_gone(path))
        .filter(|path| match fs::remove_file(path) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "cannot remove a subtitle of an earlier video");
                false
            }
        })
        .count()
}

#[cfg(test)]
#[path = "subtitle_cache_keep_tests.rs"]
mod tests;
