use app_services::{ExtensionLoadReport, ProviderSearchOutcome};
use core_model::{
    HomePageSection, LoadDetails, PlayableLinks, PluginListing, ProviderInfo, RepositoryEntry,
};

use crate::app_model::PlayTarget;
use crate::notice::Notice;

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
    /// Streams for a title/episode were resolved.
    StreamsResolved {
        target: PlayTarget,
        result: Result<PlayableLinks, String>,
    },
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
