//! Results of background work, handed to the Slint event loop.

use crate::bridge::{ExtensionsCatalog, HomeShelf, SearchSummary};
use crate::request_tickets::Ticket;

/// One finished piece of background work for the UI thread to show.
#[derive(Debug)]
pub(crate) enum UiEvent {
    StartupProgress {
        finished: usize,
        total: usize,
        current: String,
    },
    StartupDone {
        providers: Vec<String>,
        error: Option<String>,
    },
    HomeLoaded(Ticket, Vec<HomeShelf>),
    HomeSectionPageLoaded(Ticket, HomeSectionPage),
    SearchLoaded(Ticket, SearchSummary),
    BrowseLoaded(Ticket, Result<app_services::BrowsePage, String>),
    BrowseMatched(Ticket, BrowseMatch),
    DetailsLoaded(Ticket, Box<DetailsPage>),
    DetailsFailed(Ticket, String),
    OpenProgress(Ticket, playback_session::OpenProgress),
    OpenReady(Ticket, usize),
    OpenUpdate(Ticket, playback_session::CheckedOpen),
    OpenFinished(Ticket),
    OpenFailed(Ticket, String),
    /// A downloaded, normalized local subtitle file ready for the player.
    SubtitleReady(Ticket, ReadySubtitleEvent),
    /// Online skip times for the stream the ticket was issued for.
    SkipTimesFound(Ticket, Vec<crate::skip_segments::Segment>),
    Notice(String),
    UpdateState(UpdateViewState),
    UpdateDownloadProgress {
        label: String,
        percent: u8,
    },
    /// The data folder was measured (Settings > Storage).
    StorageMeasured(app_services::StorageUsage),
    /// A plugin or repository change finished; the message says how.
    ExtensionChanged {
        message: String,
        watch_repositories: bool,
    },
    ExtensionsLoaded(Box<ExtensionsCatalog>),
    /// Repositories discovered asynchronously by an aggregator plugin (such as Mega).
    RepositoriesDiscovered {
        added: usize,
        repositories: Vec<core_model::RepositoryEntry>,
    },
    /// Discovered repositories have settled; reload the full plugin catalog once.
    ReloadExtensionsCatalog,
}

impl UiEvent {
    /// The request this answers, for answers that a newer request can make stale.
    pub(crate) fn ticket(&self) -> Option<Ticket> {
        match self {
            Self::HomeLoaded(ticket, _)
            | Self::HomeSectionPageLoaded(ticket, _)
            | Self::SearchLoaded(ticket, _)
            | Self::BrowseLoaded(ticket, _)
            | Self::BrowseMatched(ticket, _)
            | Self::DetailsLoaded(ticket, _)
            | Self::DetailsFailed(ticket, _)
            | Self::OpenProgress(ticket, _)
            | Self::OpenReady(ticket, _)
            | Self::OpenUpdate(ticket, _)
            | Self::OpenFinished(ticket)
            | Self::OpenFailed(ticket, _)
            | Self::SubtitleReady(ticket, _)
            | Self::SkipTimesFound(ticket, _) => Some(*ticket),
            _ => None,
        }
    }
}

/// A loaded title page.
#[derive(Debug, Clone)]
pub(crate) struct DetailsPage {
    pub title: String,
    pub plot: String,
    pub resume: String,
    /// The episode (or movie) the viewer left off at.
    pub resume_media_key: Option<String>,
    pub episodes: Vec<EpisodeRow>,
    pub poster_url: Option<String>,
    pub background_url: Option<String>,
    pub provider: String,
    pub url: String,
    pub data: String,
    pub media_key: String,
    /// The title's ids in the skip-time databases, and the provider's kind of title.
    pub skip_ids: skip_times::ExternalIds,
    pub media_type: String,
    pub year: Option<i32>,
    /// CloudStream rating scale (out of 10,000).
    pub rating: Option<i32>,
    pub duration_minutes: Option<i32>,
    /// The provider's genres and tags.
    pub tags: Vec<String>,
    /// Cast members in the provider's order (with optional portraits and roles).
    pub cast_members: Vec<core_model::CastMember>,
}

/// One episode line of a title page.
#[derive(Debug, Clone, Default)]
pub(crate) struct EpisodeRow {
    pub name: String,
    pub code: String,
    /// `"Episode 1"` (or `"Episode"` when unnumbered).
    pub episode_label: String,
    pub data: String,
    /// Seen to 80 % or more.
    pub watched: bool,
    /// How far it was watched, 0 to 1 (0 when never started).
    pub progress: f32,
    /// The provider's summary of the episode.
    pub description: String,
    /// Season number, -1 when unknown.
    pub season: i32,
    /// Episode number, if known.
    pub episode: Option<i32>,
    /// Optional episode still / thumbnail address.
    pub poster_url: Option<String>,
}

/// The provider's entry found for a Browse title, if any.
#[derive(Debug, Clone)]
pub(crate) struct BrowseMatch {
    pub title: String,
    pub found: Option<core_model::SearchResult>,
}

/// One loaded page of an expanded Home shelf.
#[derive(Debug, Clone)]
pub(crate) struct HomeSectionPage {
    pub(crate) shelf_index: usize,
    pub(crate) page: u32,
    pub(crate) result: Result<(Vec<core_model::SearchResult>, bool), String>,
}

/// A normalized local subtitle ready to hand to the player, paired with its original remote URL.
#[derive(Debug, Clone)]
pub(crate) struct ReadySubtitleEvent {
    pub(crate) remote_url: String,
    pub(crate) track: core_model::SubtitleTrack,
}

/// Which poster model an artwork result belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PosterKind {
    Search,
    Browse,
    Library,
    Continue,
    Shelf,
    HomeExpanded,
}

/// Snapshot of the Settings > App updates card and top-bar update pill.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UpdateViewState {
    pub(crate) label: String,
    pub(crate) badge: String,
    pub(crate) version: String,
    pub(crate) notes: String,
    pub(crate) can_apply: bool,
    pub(crate) busy: bool,
    pub(crate) downloading: bool,
    pub(crate) progress_percent: u8,
}
