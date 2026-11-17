//! How much of the playing video is kept in memory (Settings > General > "Video cache"): a size,
//! and the share of it read ahead of the playing position; the rest keeps what was just watched.

use player_api::CacheLimits;

use crate::{ServiceError, SettingsService};

const SIZE_KEY: &str = "video_cache_mb";
const AHEAD_KEY: &str = "video_cache_ahead_percent";
/// The sizes (in MB) the viewer can choose.
pub const VIDEO_CACHE_SIZES_MB: [u32; 5] = [256, 512, 1024, 2048, 4096];
/// The ahead shares (in percent) the viewer can choose.
pub const VIDEO_CACHE_AHEAD_PERCENTS: [u8; 4] = [25, 50, 75, 90];
/// The size until the viewer chooses one.
const DEFAULT_SIZE_MB: u32 = 256;
/// The ahead share until the viewer chooses one: viewers mostly move forward.
const DEFAULT_AHEAD_PERCENT: u8 = 75;
const BYTES_PER_MB: u64 = 1024 * 1024;

/// The viewer's choice for the video cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoCache {
    /// The whole size, in MB.
    pub size_mb: u32,
    /// The share of it read ahead of the playing position, in percent.
    pub ahead_percent: u8,
}

impl Default for VideoCache {
    fn default() -> Self {
        Self {
            size_mb: DEFAULT_SIZE_MB,
            ahead_percent: DEFAULT_AHEAD_PERCENT,
        }
    }
}

impl VideoCache {
    /// Whether both values are among the choices offered.
    pub fn is_offered(self) -> bool {
        VIDEO_CACHE_SIZES_MB.contains(&self.size_mb)
            && VIDEO_CACHE_AHEAD_PERCENTS.contains(&self.ahead_percent)
    }

    /// The limits while the video plays: the whole size, split by the ahead share.
    pub fn while_watching(self) -> CacheLimits {
        let total = u64::from(self.size_mb) * BYTES_PER_MB;
        let ahead = total * u64::from(self.ahead_percent) / 100;
        CacheLimits {
            ahead_bytes: ahead,
            behind_bytes: total - ahead,
        }
    }

    /// The limits while the video waits after the viewer left the player: half of each part. The
    /// player trims what lies behind to it; what it already read ahead it keeps, reading no more.
    pub fn while_away(self) -> CacheLimits {
        let watching = self.while_watching();
        CacheLimits {
            ahead_bytes: watching.ahead_bytes / 2,
            behind_bytes: watching.behind_bytes / 2,
        }
    }
}

impl SettingsService {
    /// The viewer's video cache: 1 GB with 75 % ahead until chosen. A stored value that is not one
    /// of the choices (edited by hand) counts as its default.
    pub fn video_cache(&self) -> Result<VideoCache, ServiceError> {
        let defaults = VideoCache::default();
        let size_mb = self
            .stored(SIZE_KEY)?
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|size| VIDEO_CACHE_SIZES_MB.contains(size))
            .unwrap_or(defaults.size_mb);
        let ahead_percent = self
            .stored(AHEAD_KEY)?
            .and_then(|text| text.parse::<u8>().ok())
            .filter(|share| VIDEO_CACHE_AHEAD_PERCENTS.contains(share))
            .unwrap_or(defaults.ahead_percent);
        Ok(VideoCache {
            size_mb,
            ahead_percent,
        })
    }

    /// Remembers the viewer's video cache; only the offered sizes and shares are accepted.
    pub fn set_video_cache(&self, cache: VideoCache) -> Result<(), ServiceError> {
        if !cache.is_offered() {
            return Err(ServiceError::NotFound(format!(
                "a video cache of {} MB with {} % ahead is not one of the choices",
                cache.size_mb, cache.ahead_percent
            )));
        }
        self.store_value(SIZE_KEY, &cache.size_mb.to_string())?;
        self.store_value(AHEAD_KEY, &cache.ahead_percent.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    const MB: u64 = BYTES_PER_MB;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    fn cache(size_mb: u32, ahead_percent: u8) -> VideoCache {
        VideoCache {
            size_mb,
            ahead_percent,
        }
    }

    #[test]
    fn the_video_cache_is_256_mb_with_three_quarters_ahead_until_chosen() {
        assert_eq!(service().video_cache().expect("read"), cache(256, 75));
    }

    #[test]
    fn a_chosen_video_cache_is_kept_and_other_values_are_refused() {
        let service = service();
        service.set_video_cache(cache(2048, 50)).expect("chosen");
        assert_eq!(service.video_cache().expect("read"), cache(2048, 50));
        assert!(service.set_video_cache(cache(3000, 50)).is_err());
        assert!(service.set_video_cache(cache(2048, 60)).is_err());
        assert_eq!(service.video_cache().expect("unchanged"), cache(2048, 50));
    }

    #[test]
    fn stored_values_that_are_not_choices_count_as_the_defaults() {
        let service = service();
        service.store_value(SIZE_KEY, "999").expect("write");
        service.store_value(AHEAD_KEY, "abc").expect("write");
        assert_eq!(service.video_cache().expect("read"), VideoCache::default());
    }

    #[test]
    fn two_gb_half_ahead_is_one_gb_each_way_and_half_of_each_while_away() {
        let choice = cache(2048, 50);
        let watching = choice.while_watching();
        assert_eq!(
            (watching.ahead_bytes, watching.behind_bytes),
            (1024 * MB, 1024 * MB)
        );
        let away = choice.while_away();
        assert_eq!((away.ahead_bytes, away.behind_bytes), (512 * MB, 512 * MB));
    }

    #[test]
    fn a_bigger_ahead_share_reads_further_ahead_and_the_parts_add_up_to_the_size() {
        let watching = cache(2048, 75).while_watching();
        assert_eq!(
            (watching.ahead_bytes, watching.behind_bytes),
            (1536 * MB, 512 * MB)
        );
        for size in VIDEO_CACHE_SIZES_MB {
            for share in VIDEO_CACHE_AHEAD_PERCENTS {
                let limits = cache(size, share).while_watching();
                assert_eq!(
                    limits.ahead_bytes + limits.behind_bytes,
                    u64::from(size) * MB
                );
            }
        }
    }
}
