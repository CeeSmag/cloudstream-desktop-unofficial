//! Shared UI session state for the Slint shell.

use std::collections::HashSet;

use app_services::PlaybackItem;
use playback_session::{LiveSources, OpenControl};

use crate::update::UpdateInfo;

/// Optional runtime extras that are not part of [`UiServices`].
#[derive(Default)]
pub struct AppExtras {
    /// Sidecar Cloudflare challenge watcher.
    pub cloudflare: Option<cloudflare_bridge::WatchHandle>,
    /// Data directory for logs / updates / wipe marker.
    pub data_dir: Option<std::path::PathBuf>,
}

pub(crate) struct Session {
    pub(crate) control: OpenControl,
    pub(crate) live: LiveSources,
    pub(crate) links: Vec<core_model::PlayableLink>,
    pub(crate) subtitles: Vec<core_model::SubtitleTrack>,
    pub(crate) selected: usize,
    pub(crate) target: Option<PlaybackItem>,
    pub(crate) episode_data: String,
    pub(crate) details_provider: String,
    pub(crate) details_url: String,
    pub(crate) details_data: String,
    pub(crate) details_media_key: String,
    pub(crate) last_frame: u64,
    pub(crate) update: Option<UpdateInfo>,
    pub(crate) cf_id: Option<String>,
    pub(crate) cf_host: Option<String>,
    pub(crate) allowed_cf: HashSet<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            control: OpenControl::default(),
            live: LiveSources::new(),
            links: Vec::new(),
            subtitles: Vec::new(),
            selected: 0,
            target: None,
            episode_data: String::new(),
            details_provider: String::new(),
            details_url: String::new(),
            details_data: String::new(),
            details_media_key: String::new(),
            last_frame: 0,
            update: None,
            cf_id: None,
            cf_host: None,
            allowed_cf: HashSet::new(),
        }
    }
}
