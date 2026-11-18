use app_services::{ExtensionLoadReport, ProviderSearchOutcome};
use core_model::{
    HomePageSection, LoadDetails, PlayableLink, PluginListing, ProviderInfo, RepositoryEntry,
    SubtitleTrack,
};

use crate::app_model::PlayTarget;
use crate::notice::Notice;
use crate::update_check::UpdateInfo;

/// The result of background work, applied to the model on the UI thread.
pub(crate) enum UiEvent {
    /// Installed extensions finished loading at startup.
    ExtensionsLoaded(Result<ExtensionLoadReport, String>),
    /// A provider's home page rows arrived.
    HomeLoaded {
        provider: String,
        rows: Result<Vec<HomePageSection>, String>,
    },
    /// A search over all enabled providers finished.
    SearchFinished {
        query: String,
        outcomes: Vec<ProviderSearchOutcome>,
    },
    /// Title details arrived.
    DetailsLoaded {
        url: String,
        result: Result<LoadDetails, String>,
    },
    /// Streams for a title/episode were searched and checked.
    StreamsResolved {
        target: PlayTarget,
        result: Result<CheckedStreams, String>,
    },
    /// Update check finished.
    UpdateChecked(Result<Option<UpdateInfo>, String>),
    /// A repository was added (or why not).
    RepositoryAdded(Result<RepositoryEntry, String>),
    /// A repository's plugin list arrived.
    RepositoryPlugins {
        url: String,
        result: Result<Vec<PluginListing>, String>,
    },
    /// A plugin finished installing.
    PluginInstalled {
        name: String,
        result: Result<Vec<ProviderInfo>, String>,
    },
    /// A message to show as-is.
    Notice(Notice),
}

/// Checked open result delivered to the UI thread.
#[derive(Debug, Clone)]
pub(crate) struct CheckedStreams {
    /// Links in play order.
    pub(crate) links: Vec<PlayableLink>,
    /// Preferred index when a working stream was found.
    pub(crate) play_index: Option<usize>,
    /// Subtitles from the provider.
    pub(crate) subtitles: Vec<SubtitleTrack>,
}
