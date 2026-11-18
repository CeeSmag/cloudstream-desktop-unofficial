//! Results of background work, handed to the Slint event loop.

use crate::bridge::SearchSummary;
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
    BrowseLoaded(Ticket, Result<Vec<app_services::BrowseItem>, String>),
    BrowseMatched(Ticket, BrowseMatch),
    DetailsLoaded(Ticket, Box<DetailsPage>),
    DetailsFailed(Ticket, String),
    OpenProgress(Ticket, playback_session::OpenProgress),
    OpenReady(Ticket, usize),
    OpenUpdate(Ticket, playback_session::CheckedOpen),
    OpenFailed(Ticket, String),
    Notice(String),
    CloudflareNeeded {
        host: String,
        url: String,
        id: String,
    },
    UpdateLabel(String),
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
            | Self::OpenFailed(ticket, _) => Some(*ticket),
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
    pub episodes: Vec<EpisodeRow>,
    pub provider: String,
    pub url: String,
    pub data: String,
    pub media_key: String,
}

/// One episode line of a title page.
#[derive(Debug, Clone)]
pub(crate) struct EpisodeRow {
    pub name: String,
    pub code: String,
    pub data: String,
    pub watched: bool,
    /// Season number, -1 when unknown.
    pub season: i32,
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
