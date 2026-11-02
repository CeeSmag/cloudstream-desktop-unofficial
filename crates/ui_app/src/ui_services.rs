use std::sync::Arc;

use app_services::{
    CatalogService, Clock, ExtensionService, LibraryService, PlaybackService, RepositoryService,
    SettingsService, StreamResolver,
};

/// Everything the UI may ask for. Built once in `app_shell`.
#[derive(Clone)]
pub struct UiServices {
    /// Repositories the user subscribed to.
    pub repositories: RepositoryService,
    /// Extension install/load.
    pub extensions: ExtensionService,
    /// Search, home page and details.
    pub catalog: CatalogService,
    /// Title/episode to stream resolution.
    pub streams: Arc<StreamResolver>,
    /// Playback, or `None` when the player could not be started.
    pub playback: Option<Arc<PlaybackService>>,
    /// Why playback is unavailable, shown in Settings.
    pub player_problem: Option<String>,
    /// Saved titles and history.
    pub library: LibraryService,
    /// Typed settings.
    pub settings: SettingsService,
    /// Time source for timestamps the UI records.
    pub clock: Arc<dyn Clock>,
}
