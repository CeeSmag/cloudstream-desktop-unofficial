//! What is kept on disk for titles: only the open video's, and what its saved sources learn from
//! playback (which stream played well and which failed).

use title_cache::KeptTitle;

use crate::source_manager::SourceManager;

impl SourceManager {
    /// Makes the title being opened the only one with saved sources and subtitles on disk: what
    /// was kept for every other title (the previous video) is removed, so that video searches and
    /// downloads again when it is opened again. Late saves of the replaced video are refused.
    pub(crate) fn keep_only(&self, title_key: &str) -> KeptTitle {
        let subtitles_removed = self.subtitles.cache().keep_only_title(title_key);
        let kept = self.titles.keep_only(title_key);
        tracing::debug!(subtitles_removed, "only the open video keeps what is saved");
        kept
    }

    /// Notes that the stream at `url` played well for the title, so it is tried first next time.
    pub fn note_played(&self, title_key: &str, url: &str) {
        if let Err(error) = self.titles.mark_played_well(title_key, url) {
            tracing::warn!(%error, "cannot note the stream that played well");
        }
    }

    /// Forgets everything kept on disk for a title: its saved sources and its subtitle files.
    pub fn forget_title(&self, title_key: &str) {
        match self.titles.forget(title_key) {
            Ok(had_sources) => {
                let subtitle_files = self.subtitles.cache().forget_title(title_key);
                tracing::info!(
                    had_sources,
                    subtitle_files,
                    "forgot what was kept for a title"
                );
            }
            Err(error) => tracing::warn!(%error, "cannot forget the saved sources of a title"),
        }
    }

    /// Notes that the stream at `url` failed, so the next opening of the title skips it.
    pub fn note_broken(&self, title_key: &str, url: &str) {
        if let Err(error) = self.titles.mark_broken(title_key, url) {
            tracing::warn!(%error, "cannot note the stream that failed");
        }
    }
}
