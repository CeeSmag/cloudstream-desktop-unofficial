//! How much of the playing video is kept in memory (Settings > General > "Video cache"): a size,
//! and the share of it read ahead of the playing position; the rest keeps what was just watched.

use player_api::CacheLimits;

use crate::{ServiceError, SettingsService};

pub(crate) const SIZE_KEY: &str = "video_cache_mb";
pub(crate) const AHEAD_KEY: &str = "video_cache_ahead_percent";
/// The sizes (in MB) the viewer can choose.
pub const VIDEO_CACHE_SIZES_MB: [u32; 7] = memory_budget::VIDEO_CACHE_CHOICES_MIB;
/// The ahead shares (in percent) the viewer can choose.
pub const VIDEO_CACHE_AHEAD_PERCENTS: [u8; 4] = memory_budget::READ_AHEAD_CHOICES_PERCENT;
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
}

impl SettingsService {
    /// The video cache in force: what the viewer chose, else the profile's size and read-ahead
    /// (256 MB with 75 % ahead on a balanced PC). A stored value that is not one of the choices
    /// (edited by hand) is ignored.
    pub fn video_cache(&self) -> Result<VideoCache, ServiceError> {
        let budget = self.performance_budget()?;
        Ok(VideoCache {
            size_mb: budget.video_cache_mib,
            ahead_percent: budget.video_read_ahead_percent,
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
    fn two_gb_half_ahead_is_one_gb_each_way() {
        let choice = cache(2048, 50);
        let watching = choice.while_watching();
        assert_eq!(
            (watching.ahead_bytes, watching.behind_bytes),
            (1024 * MB, 1024 * MB)
        );
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
