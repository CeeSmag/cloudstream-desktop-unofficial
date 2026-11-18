//! State the UI thread keeps between callbacks.

use std::collections::HashSet;

use app_services::PlaybackItem;
use playback_session::{LiveSources, OpenControl};

use crate::request_tickets::RequestTickets;
use crate::update::UpdateInfo;

/// Optional runtime extras that are not part of [`app_services::UiServices`].
#[derive(Default)]
pub struct AppExtras {
    /// Sidecar Cloudflare challenge watcher.
    pub cloudflare: Option<cloudflare_bridge::WatchHandle>,
    /// Data directory for logs / updates / wipe marker.
    pub data_dir: Option<std::path::PathBuf>,
}

/// The title page, the opening video and the Cloudflare question on screen right now.
pub(crate) struct Session {
    pub(crate) control: OpenControl,
    pub(crate) live: LiveSources,
    pub(crate) links: Vec<core_model::PlayableLink>,
    pub(crate) subtitles: Vec<core_model::SubtitleTrack>,
    pub(crate) selected: usize,
    pub(crate) target: Option<PlaybackItem>,
    pub(crate) episode_data: String,
    pub(crate) details: DetailsTarget,
    pub(crate) last_frame: u64,
    pub(crate) update: Option<UpdateInfo>,
    pub(crate) cf_id: Option<String>,
    pub(crate) cf_host: Option<String>,
    pub(crate) allowed_cf: HashSet<String>,
    pub(crate) tickets: RequestTickets,
}

/// Which title the details page shows.
#[derive(Default, Clone)]
pub(crate) struct DetailsTarget {
    pub(crate) provider: String,
    pub(crate) url: String,
    pub(crate) data: String,
    pub(crate) media_key: String,
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
            details: DetailsTarget::default(),
            last_frame: 0,
            update: None,
            cf_id: None,
            cf_host: None,
            allowed_cf: HashSet::new(),
            tickets: RequestTickets::default(),
        }
    }
}
