//! Results of background work, handed to the Slint event loop.

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
    HomeLoaded(Vec<(String, Vec<core_model::SearchResult>)>),
    SearchLoaded(Vec<core_model::SearchResult>),
    BrowseLoaded(Vec<BrowseRow>),
    DetailsLoaded(Box<DetailsPage>),
    DetailsFailed(String),
    OpenProgress(playback_session::OpenProgress),
    OpenReady {
        index: usize,
    },
    OpenUpdate(playback_session::CheckedOpen),
    OpenFailed(String),
    Notice(String),
    CloudflareNeeded {
        host: String,
        url: String,
        id: String,
    },
    UpdateLabel(String),
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

/// One TMDB title on the Browse screen.
#[derive(Debug, Clone)]
pub(crate) struct BrowseRow {
    pub title: String,
    pub subtitle: String,
    pub poster: String,
    pub url: String,
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
