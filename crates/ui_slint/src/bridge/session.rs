//! State the UI thread keeps between callbacks.

use app_services::PlaybackItem;
use playback_session::{LiveSources, OpenControl};

use crate::browse_filters::BrowseFilters;
use crate::focus_grid::FocusGrid;
use crate::playback_watch::{PlaybackWatch, SourceCheck};
use crate::source_menu::SourceFilter;

use super::focus_screens::FocusScreen;
use super::player_timeline::Timeline;
use crate::request_tickets::RequestTickets;
use crate::ui_events::EpisodeRow;

/// Asks for "delete all saved data" at the next start (built by the shell, which runs the wipe).
pub type WipeRequest = Box<dyn Fn() -> std::io::Result<()>>;

/// Switches the app's debug log files on or off (built by the shell, which owns the logger).
pub type DebugLogSwitch = Box<dyn Fn(bool) -> std::io::Result<()>>;

/// Optional runtime extras that are not part of [`app_services::UiServices`].
#[derive(Default)]
pub struct AppExtras {
    /// Sidecar Cloudflare challenge watcher.
    pub cloudflare: Option<cloudflare_bridge::WatchHandle>,
    /// Data directory for logs / updates.
    pub data_dir: Option<std::path::PathBuf>,
    /// Schedules the data wipe.
    pub request_wipe: Option<WipeRequest>,
    /// Settings > General > Debug logging.
    pub debug_logs: Option<DebugLogSwitch>,
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
    pub(crate) shown_tracks: Vec<player_api::MediaTrack>,
    /// The playing stream's chapters, skip parts and skip button.
    pub(crate) timeline: Timeline,
    /// The size the player renders at (physical pixels).
    pub(crate) surface_size: (u32, u32),
    pub(crate) tickets: RequestTickets,
    /// The TMDB titles Browse shows, in the grid's order.
    pub(crate) browse_items: Vec<app_services::BrowseItem>,
    /// What Browse shows, and the last page loaded with the number of pages.
    pub(crate) browse_filters: BrowseFilters,
    pub(crate) browse_page: (u32, u32),
    /// The episodes of the title page, in the provider's order (all seasons).
    pub(crate) episodes: Vec<EpisodeRow>,
    /// What to do when the playing stream fails or ends.
    pub(crate) watch: PlaybackWatch,
    /// The filters of the Sources menu (reset for every new video).
    pub(crate) source_filter: SourceFilter,
    /// How each source of the open list fared in its check.
    pub(crate) source_checks: Vec<SourceCheck>,
    /// The video the viewer left (still loaded, paused) by its media key.
    pub(crate) away_key: Option<String>,
    /// The plugins the repositories offer, in the repositories' order.
    pub(crate) available: Vec<app_services::PluginHit>,
    /// Internal names of the installed plugins (for the list's installed marks and filter).
    pub(crate) installed_names: Vec<String>,
    /// How many plugins the list shows ("Show more" adds a page).
    pub(crate) plugin_page: usize,
    /// Where the remote's focus is on each screen.
    pub(crate) focus: std::collections::HashMap<FocusScreen, FocusGrid>,
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
    /// The title's ids in the skip-time databases, and the provider's kind of title.
    pub(crate) skip_ids: skip_times::ExternalIds,
    pub(crate) media_type: String,
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
            shown_tracks: Vec::new(),
            timeline: Timeline::default(),
            surface_size: (0, 0),
            tickets: RequestTickets::default(),
            browse_items: Vec::new(),
            browse_filters: BrowseFilters::default(),
            browse_page: (0, 0),
            episodes: Vec::new(),
            watch: PlaybackWatch::default(),
            source_filter: SourceFilter::default(),
            source_checks: Vec::new(),
            away_key: None,
            available: Vec::new(),
            installed_names: Vec::new(),
            plugin_page: 0,
            focus: std::collections::HashMap::new(),
        }
    }
}
