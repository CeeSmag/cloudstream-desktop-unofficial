//! State the UI thread keeps between callbacks.

use app_services::PlaybackItem;
use playback_session::{LiveSources, OpenControl};

use crate::playback_watch::{PlaybackWatch, SourceCheck};
use crate::request_tickets::RequestTickets;
use crate::ui_events::EpisodeRow;
use crate::update::UpdateInfo;

/// Optional runtime extras that are not part of [`app_services::UiServices`].
#[derive(Default)]
pub struct AppExtras {
    /// Sidecar Cloudflare challenge watcher.
    pub cloudflare: Option<cloudflare_bridge::WatchHandle>,
    /// Data directory for logs / updates / wipe marker.
    pub data_dir: Option<std::path::PathBuf>,
}

/// The title page and the opening video on screen right now.
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
    /// The track menus on screen (audio, subtitles), so they are rebuilt only when they change.
    pub(crate) shown_tracks: (Vec<String>, Vec<String>),
    /// The size the player renders at (physical pixels).
    pub(crate) surface_size: (u32, u32),
    pub(crate) update: Option<UpdateInfo>,
    pub(crate) tickets: RequestTickets,
    /// The TMDB titles Browse shows, in the grid's order.
    pub(crate) browse_items: Vec<app_services::BrowseItem>,
    /// The episodes of the title page, in the provider's order (all seasons).
    pub(crate) episodes: Vec<EpisodeRow>,
    /// What to do when the playing stream fails or ends.
    pub(crate) watch: PlaybackWatch,
    /// How each source of the open list fared in its check.
    pub(crate) source_checks: Vec<SourceCheck>,
    /// The video the viewer left (still loaded, paused) by its media key.
    pub(crate) away_key: Option<String>,
    /// The plugins the repositories offer, in the Extensions page's order.
    pub(crate) available: Vec<app_services::PluginHit>,
}

/// Which title the details page shows.
#[derive(Default, Clone)]
pub(crate) struct DetailsTarget {
    pub(crate) provider: String,
    pub(crate) url: String,
    pub(crate) data: String,
    pub(crate) media_key: String,
    pub(crate) poster_url: Option<String>,
    /// The episode (or movie) the viewer left off at.
    pub(crate) resume_media_key: Option<String>,
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
            shown_tracks: (Vec::new(), Vec::new()),
            surface_size: (0, 0),
            update: None,
            tickets: RequestTickets::default(),
            browse_items: Vec::new(),
            episodes: Vec::new(),
            watch: PlaybackWatch::default(),
            source_checks: Vec::new(),
            away_key: None,
            available: Vec::new(),
        }
    }
}
