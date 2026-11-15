//! The switch for the local stream proxy.

use crate::{ServiceError, SettingsService};

const STREAM_PROXY_KEY: &str = "stream_proxy";

impl SettingsService {
    /// Whether HLS streams go through the local stream proxy (it adds the headers the stream's
    /// host wants to every request). On until the viewer switches it off, for the day a host
    /// serves a stream better directly than through the proxy.
    pub fn stream_proxy(&self) -> Result<bool, ServiceError> {
        self.flag(STREAM_PROXY_KEY)
    }

    /// Remembers whether HLS streams go through the local stream proxy.
    pub fn set_stream_proxy(&self, enabled: bool) -> Result<(), ServiceError> {
        self.set_flag(STREAM_PROXY_KEY, enabled)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    #[test]
    fn stream_proxy_is_on_until_switched_off_and_the_choice_is_kept() {
        let service = service();
        assert!(service.stream_proxy().expect("read"));
        service.set_stream_proxy(false).expect("set");
        assert!(!service.stream_proxy().expect("read"));
        service.set_stream_proxy(true).expect("set");
        assert!(service.stream_proxy().expect("read"));
    }

    #[test]
    fn stream_proxy_does_not_change_the_other_playback_options() {
        let service = service();
        service.set_stream_proxy(false).expect("set");
        assert!(service.autoplay_next().expect("read"));
        assert!(service.skip_lookup().expect("read"));
    }
}
