//! Gets one subtitle file ready for the player: from the disk cache if it is there, otherwise
//! downloaded (retrying a host that is slow or busy), cleaned up and saved.

use std::sync::Arc;
use std::time::Duration;

use core_model::SubtitleTrack;

use crate::hls_subtitles::flatten_playlist;
use crate::normalize_subtitle::{normalize_subtitle, NormalizeError};
use crate::subtitle_cache::{CacheError, ReadySubtitle, SubtitleCache};
use crate::subtitle_fetcher::{FetchError, SubtitleFetcher};

/// How many times a download is tried in all, and the pause before each new try (growing).
#[derive(Debug, Clone, Copy)]
pub struct LoadOptions {
    /// Tries in all (the first one included).
    pub attempts: u32,
    /// Pause before the second try; the third waits twice as long, and so on.
    pub backoff: Duration,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            attempts: 3,
            backoff: Duration::from_millis(400),
        }
    }
}

/// Why a subtitle could not be made ready.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The download failed on every try.
    #[error("the download failed after {attempts} tries: {reason}")]
    Download {
        /// The last failure.
        reason: FetchError,
        /// How many tries were made.
        attempts: u32,
    },
    /// The host answered, but not with a subtitle.
    #[error("not a usable subtitle: {0}")]
    Unusable(#[from] NormalizeError),
    /// A subtitle playlist without any cue.
    #[error("the subtitle playlist has no cues")]
    EmptyPlaylist,
    /// The cleaned-up file could not be saved.
    #[error(transparent)]
    Save(#[from] CacheError),
    /// Nobody wants the file any more.
    #[error("stopped: the file is no longer wanted")]
    Stopped,
}

/// A subtitle that is ready, and how it got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSubtitle {
    /// The file.
    pub ready: ReadySubtitle,
    /// True when it was on disk already and nothing was downloaded.
    pub from_cache: bool,
}

/// Downloads, cleans and saves subtitle files.
pub struct SubtitleLoader {
    cache: SubtitleCache,
    fetcher: Arc<dyn SubtitleFetcher>,
    options: LoadOptions,
}

impl SubtitleLoader {
    /// A loader that keeps its files in `cache` and downloads through `fetcher`.
    pub fn new(
        cache: SubtitleCache,
        fetcher: Arc<dyn SubtitleFetcher>,
        options: LoadOptions,
    ) -> Self {
        Self {
            cache,
            fetcher,
            options,
        }
    }

    /// The cache, for the per-title lists.
    pub fn cache(&self) -> &SubtitleCache {
        &self.cache
    }

    /// Makes `track` ready. `keep_going` is asked between tries, so a file nobody wants any more
    /// is not tried again.
    #[tracing::instrument(level = "debug", skip_all, fields(language = %track.lang, url = %track.url))]
    pub fn load(
        &self,
        track: &SubtitleTrack,
        keep_going: &dyn Fn() -> bool,
    ) -> Result<LoadedSubtitle, LoadError> {
        let ready = |path| ReadySubtitle {
            language: track.lang.clone(),
            path,
            url: track.url.clone(),
        };
        if let Some(path) = self.cache.lookup(&track.url) {
            return Ok(LoadedSubtitle {
                ready: ready(path),
                from_cache: true,
            });
        }
        let body = self.download(track, keep_going)?;
        let normalized = normalize_subtitle(&body)?;
        let (text, extension) = if normalized.extension == "m3u8" {
            let flat = flatten_playlist(&normalized.text, &track.url, &*self.fetcher);
            (flat.ok_or(LoadError::EmptyPlaylist)?, "vtt")
        } else {
            (normalized.text, normalized.extension)
        };
        let path = self.cache.store(&track.url, &text, extension)?;
        Ok(LoadedSubtitle {
            ready: ready(path),
            from_cache: false,
        })
    }

    fn download(
        &self,
        track: &SubtitleTrack,
        keep_going: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, LoadError> {
        let headers: Vec<(String, String)> = track
            .headers
            .iter()
            .flatten()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let mut attempt = 0;
        loop {
            attempt += 1;
            if !keep_going() {
                return Err(LoadError::Stopped);
            }
            match self.fetcher.fetch(&track.url, &headers) {
                Ok(body) => return Ok(body),
                Err(reason) if reason.is_transient() && attempt < self.options.attempts => {
                    tracing::debug!(%reason, attempt, url = %track.url, "subtitle download failed, trying again");
                    std::thread::sleep(self.options.backoff * attempt);
                }
                Err(reason) => {
                    return Err(LoadError::Download {
                        reason,
                        attempts: attempt,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "subtitle_loader_tests.rs"]
mod tests;
