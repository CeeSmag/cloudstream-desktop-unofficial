//! State the UI thread keeps between callbacks.

use app_services::PlaybackItem;
use playback_session::{LiveSources, OpenControl};

use crate::browse_filters::BrowseFilters;
use crate::click_sorter::ClickSorter;
use crate::cursor_policy::CursorPolicy;
use crate::focus_grid::FocusGrid;
use crate::playback_watch::{PlaybackWatch, SourceCheck};
use crate::repository_discovery::RepositoryDiscovery;
use crate::seek_scheduler::SeekScheduler;
use crate::source_menu::SourceFilter;

use super::focus_screens::FocusScreen;
use super::home_expanded::ExpandedShelfState;
use super::home_loading::HomeShelf;
use super::player_timeline::Timeline;
use super::subtitle_delivery::SubtitleDeliveryHandle;
use crate::request_tickets::{RequestKind, RequestTickets};
use crate::ui_events::EpisodeRow;

/// The title page and the opening video on screen right now.
#[derive(Default)]
pub(crate) struct Session {
    pub(crate) control: OpenControl,
    pub(crate) live: LiveSources,
    pub(crate) links: Vec<core_model::PlayableLink>,
    /// Downloaded, normalized local subtitle files ready to pass to mpv.
    pub(crate) ready_subtitles: Vec<core_model::SubtitleTrack>,
    /// Remote URLs already delivered as ready local subtitles (dedup across refresh).
    pub(crate) delivered_subtitle_urls: std::collections::HashSet<String>,
    /// Union of provider-advertised subtitles for the open title (preserved across refresh).
    pub(crate) provider_subtitles: Vec<core_model::SubtitleTrack>,
    /// Background downloader/normalizer for the open video's external subtitles.
    pub(crate) subtitle_delivery: Option<SubtitleDeliveryHandle>,
    /// True while initial open or Refresh Sources is still searching/checking streams.
    pub(crate) sources_loading: bool,
    /// True while the provider's link/subtitle search has not finished yet.
    pub(crate) sources_searching: bool,
    /// True when the active background source pass was triggered by Refresh Sources.
    pub(crate) sources_refreshing: bool,
    pub(crate) selected: usize,
    pub(crate) target: Option<PlaybackItem>,
    pub(crate) episode_data: String,
    pub(crate) details: DetailsTarget,
    pub(crate) last_frame: u64,
    /// The track menus on screen (audio, subtitles), so they are rebuilt only when they change.
    pub(crate) shown_tracks: Vec<player_api::MediaTrack>,
    /// The playing stream's chapters, skip parts and skip button.
    pub(crate) timeline: Timeline,
    /// The arrow keys' jumps, spaced out against rate limits.
    pub(crate) seeks: SeekScheduler,
    /// Wheel movement that has not made a whole percent of volume yet.
    pub(crate) wheel_carry: f32,
    /// A click on the video that may still turn into a double click.
    pub(crate) clicks: ClickSorter,
    /// Whether the mouse pointer is hidden (while the player has hidden its controls).
    pub(crate) cursor: CursorPolicy,
    /// The size the player renders at (physical pixels).
    pub(crate) surface_size: (u32, u32),
    pub(crate) tickets: RequestTickets,
    /// The Home shelves of the active provider, with their paging metadata.
    pub(crate) home_shelves: Vec<HomeShelf>,
    /// Currently expanded Home shelf, if any.
    pub(crate) home_expanded: Option<ExpandedShelfState>,
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
    /// Token from `TitleCache::keep_only` so leave/close can save only this title.
    pub(crate) kept_sources: Option<title_cache::KeptTitle>,
    /// The saved repositories, in the database's order.
    pub(crate) repositories: Vec<core_model::RepositoryEntry>,
    /// The plugins the repositories offer, in the repositories' order.
    pub(crate) available: Vec<app_services::PluginHit>,
    /// Installed plugins' internal names mapped to `installed_at_ms` (0 when unknown).
    pub(crate) installed_times: std::collections::HashMap<String, i64>,
    /// How many plugins the Plugins tab shows ("Show more" adds a page).
    pub(crate) plugin_page: usize,
    /// How many plugins the open repository detail view shows ("Show more" adds a page).
    pub(crate) repo_detail_page: usize,
    /// Where the remote's focus is on each screen.
    pub(crate) focus: std::collections::HashMap<FocusScreen, FocusGrid>,
    /// Background watcher collecting repositories added by aggregator plugins.
    pub(crate) repository_discovery: RepositoryDiscovery,
}

/// Which title the details page shows.
#[derive(Default, Clone)]
pub(crate) struct DetailsTarget {
    pub(crate) provider: String,
    pub(crate) url: String,
    pub(crate) data: String,
    pub(crate) media_key: String,
    pub(crate) poster_url: Option<String>,
    pub(crate) background_url: Option<String>,
    /// The episode (or movie) the viewer left off at.
    pub(crate) resume_media_key: Option<String>,
    /// The title's ids in the skip-time databases, and the provider's kind of title.
    pub(crate) skip_ids: skip_times::ExternalIds,
    pub(crate) media_type: String,
    /// When true, episodes are listed newest-first instead of oldest-first.
    pub(crate) episodes_descending: bool,
    /// Active 20-episode chunk index within the selected season.
    pub(crate) episode_chunk: usize,
    /// Ephemeral artwork URLs hydrated for this title page (forgotten on close).
    pub(crate) ephemeral_urls: Vec<String>,
}

impl Session {
    /// Stops an in-flight open and drops its late answers (and skip lookups / key seeks).
    /// Used when leaving or closing the player so `OpenReady` cannot reopen it.
    pub(crate) fn abandon_open(&mut self) {
        self.control.cancel_open();
        if let Some(delivery) = self.subtitle_delivery.take() {
            delivery.stop();
        }
        self.sources_loading = false;
        self.sources_searching = false;
        self.sources_refreshing = false;
        self.tickets.cancel(RequestKind::Open);
        self.tickets.cancel(RequestKind::Skips);
        self.seeks.cancel();
        self.clicks = ClickSorter::default();
    }

    /// Clears per-stream UI state when a different video starts opening.
    pub(crate) fn reset_for_new_open(&mut self) {
        if let Some(delivery) = self.subtitle_delivery.take() {
            delivery.stop();
        }
        self.ready_subtitles.clear();
        self.delivered_subtitle_urls.clear();
        self.provider_subtitles.clear();
        self.sources_loading = true;
        self.sources_searching = true;
        self.sources_refreshing = false;
        self.tickets.cancel(RequestKind::Skips);
        self.seeks.cancel();
        self.timeline = Timeline::default();
        self.source_checks.clear();
        self.shown_tracks.clear();
    }

    /// Stops the in-flight open/refresh and starts a new one under a fresh ticket and control.
    pub(crate) fn begin_source_refresh(&mut self) -> (OpenControl, crate::request_tickets::Ticket) {
        self.control.cancel_open();
        if let Some(delivery) = self.subtitle_delivery.take() {
            delivery.stop();
        }
        self.control = OpenControl::default();
        self.sources_loading = true;
        self.sources_searching = true;
        self.sources_refreshing = true;
        let ticket = self.tickets.issue(RequestKind::Open);
        (self.control.clone(), ticket)
    }
}

/// Keeps the playing stream selected by URL after `links` is replaced during open or refresh.
pub(crate) fn resolve_selected_index(
    links: &[core_model::PlayableLink],
    playing_url: Option<&str>,
    fallback: Option<usize>,
) -> usize {
    if let Some(url) = playing_url {
        if let Some(index) = links.iter().position(|link| link.url == url) {
            return index;
        }
    }
    fallback.filter(|index| *index < links.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandon_open_cancels_open_skips_and_waiting_seeks() {
        let mut session = Session::default();
        let open = session.tickets.issue(RequestKind::Open);
        let skips = session.tickets.issue(RequestKind::Skips);
        let _ = session.seeks.press(1_000, std::time::Instant::now());
        let _ = session.seeks.press(2_000, std::time::Instant::now());
        assert!(session.seeks.pending().is_some());
        session.abandon_open();
        assert!(!session.tickets.is_current(open));
        assert!(!session.tickets.is_current(skips));
        assert!(session.seeks.pending().is_none());
        assert!(session
            .control
            .cancel
            .load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn reset_for_new_open_clears_timeline_skips_seeks_and_checks() {
        let mut session = Session::default();
        let skips = session.tickets.issue(RequestKind::Skips);
        session.timeline.read_for = Some(1_400_000);
        session.source_checks = vec![crate::playback_watch::SourceCheck::Working];
        let _ = session.seeks.press(1_000, std::time::Instant::now());
        let _ = session.seeks.press(2_000, std::time::Instant::now());
        session.reset_for_new_open();
        assert!(!session.tickets.is_current(skips));
        assert_eq!(session.timeline.read_for, None);
        assert!(session.source_checks.is_empty());
        assert!(session.seeks.pending().is_none());
    }

    #[test]
    fn begin_source_refresh_cancels_prior_control_and_issues_new_ticket() {
        use std::sync::atomic::Ordering;
        let mut session = Session::default();
        let prior_control = session.control.clone();
        let prior = session.tickets.issue(RequestKind::Open);
        let (control, ticket) = session.begin_source_refresh();
        assert!(prior_control.cancel.load(Ordering::SeqCst));
        assert!(!session.tickets.is_current(prior));
        assert!(session.tickets.is_current(ticket));
        assert!(!control.cancel.load(Ordering::SeqCst));
    }
}
