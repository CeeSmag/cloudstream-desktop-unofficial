//! Results of background work, handed to the Slint event loop.

use crate::bridge::{ExtensionsCatalog, SearchSummary};
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
    HomeLoaded(Ticket, Vec<(String, Vec<core_model::SearchResult>)>),
    SearchLoaded(Ticket, SearchSummary),
    BrowseLoaded(Ticket, Result<app_services::BrowsePage, String>),
    BrowseMatched(Ticket, BrowseMatch),
    DetailsLoaded(Ticket, Box<DetailsPage>),
    DetailsFailed(Ticket, String),
    OpenProgress(Ticket, playback_session::OpenProgress),
    OpenReady(Ticket, usize),
    OpenUpdate(Ticket, playback_session::CheckedOpen),
    OpenFailed(Ticket, String),
    /// Online skip times for the stream the ticket was issued for.
    SkipTimesFound(Ticket, Vec<crate::skip_segments::Segment>),
    Notice(String),
    UpdateLabel(String),
    /// A plugin or repository change finished; the message says how.
    ExtensionChanged(String),
    ExtensionsLoaded(Box<ExtensionsCatalog>),
}

impl UiEvent {
    /// The request this answers, for answers that a newer request can make stale.
    pub(crate) fn ticket(&self) -> Option<Ticket> {
        match self {
            Self::HomeLoaded(ticket, _)
            | Self::SearchLoaded(ticket, _)
            | Self::BrowseLoaded(ticket, _)
            | Self::BrowseMatched(ticket, _)
            | Self::DetailsLoaded(ticket, _)
            | Self::DetailsFailed(ticket, _)
            | Self::OpenProgress(ticket, _)
            | Self::OpenReady(ticket, _)
            | Self::OpenUpdate(ticket, _)
            | Self::OpenFailed(ticket, _)
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
    pub meta: String,
    pub resume: String,
    /// The episode (or movie) the viewer left off at.
    pub resume_media_key: Option<String>,
    pub episodes: Vec<EpisodeRow>,
    pub poster_url: Option<String>,
    pub provider: String,
    pub url: String,
    pub data: String,
    pub media_key: String,
    /// The title's ids in the skip-time databases, and the provider's kind of title.
    pub skip_ids: skip_times::ExternalIds,
    pub media_type: String,
    /// The provider's genres and tags.
    pub tags: Vec<String>,
    /// "Name (role)" of each cast member, in the provider's order.
    pub cast: Vec<String>,
}

/// One episode line of a title page.
#[derive(Debug, Clone, Default)]
pub(crate) struct EpisodeRow {
    pub name: String,
    pub code: String,
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
}

/// The provider's entry found for a Browse title, if any.
#[derive(Debug, Clone)]
pub(crate) struct BrowseMatch {
    pub title: String,
    pub found: Option<core_model::SearchResult>,
}

/// Which poster model an artwork result belongs to.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PosterKind {
    Search,
    Browse,
    Library,
    Continue,
    Shelf,
}
