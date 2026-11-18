//! Whether leaving the player keeps the stream paused in memory.

use crate::{ServiceError, SettingsService};

const KEY: &str = "keep_video_ready";

impl SettingsService {
    /// Whether a playing video waits paused when the viewer leaves the player. On until switched
    /// off (low-end PCs can free ~1 GB by stopping instead).
    pub fn keep_video_ready(&self) -> Result<bool, ServiceError> {
        self.flag(KEY)
    }

    /// Remembers whether leaving the player keeps the video ready.
    pub fn set_keep_video_ready(&self, enabled: bool) -> Result<(), ServiceError> {
        self.set_flag(KEY, enabled)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        SettingsService::new(SettingsStore::new(Arc::new(
            Database::open_in_memory().expect("memory db"),
        )))
    }

    #[test]
    fn keep_video_ready_is_on_until_switched_off() {
        let service = service();
        assert!(service.keep_video_ready().expect("read"));
        service.set_keep_video_ready(false).expect("off");
        assert!(!service.keep_video_ready().expect("read"));
    }
}
