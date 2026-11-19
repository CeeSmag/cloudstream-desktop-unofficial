//! Use-case services the UI calls: extensions, catalog, playback, library, settings.
#![forbid(unsafe_code)]

mod browse_fetcher;
mod browse_genres;
mod browse_match;
mod browse_query;
mod browse_service;
mod catalog_service;
mod check_queue;
mod clear_key;
mod clock;
mod cloudflare_setting;
mod extension_load_report;
mod extension_resident;
mod extension_service;
mod home_provider_setting;
mod language_settings;
mod library_service;
mod link_search;
mod playback_away;
mod playback_resume;
mod playback_service;
mod playback_types;
mod plugin_index;
mod plugin_worker_setting;
mod regenerable_cleanup;
mod repository_service;
mod request_router;
mod routed_player;
mod service_error;
mod settings_service;
mod storage_cap_setting;
mod storage_usage;
mod stream_checker;
mod stream_label;
mod stream_proxy_setting;
mod stream_resolver;
mod subtitle_scan;
#[cfg(test)]
mod test_support;
mod ui_services;
mod video_cache_setting;

pub use browse_fetcher::{BrowseFetcher, UreqBrowseFetcher};
pub use browse_genres::{
    movie_genre_id, tv_genre_id, BrowseFilterRequest, BrowseItem, BrowsePage, TMDB_API_KEY,
    TMDB_BASE_URL,
};
pub use browse_match::pick_best_search_match;
pub use browse_service::BrowseClient;

/// Alias for [`BrowseClient`] (TMDB catalog discovery).
pub type BrowseService = BrowseClient;

pub use catalog_service::{CatalogService, ProviderSearchOutcome};
pub use check_queue::CheckQueue;
pub use clear_key::{decode_key_to_hex, normalize_clear_key, normalize_pipe_link};
pub use clock::{Clock, SystemClock};
pub use extension_load_report::{ExtensionLoadReport, LoadProgress};
pub use extension_service::ExtensionService;
pub use language_settings::MAX_PREFERRED_SUBTITLES;
pub use library_service::LibraryService;
pub use link_search::{LinkBatch, LinkSearch};
pub use playback_resume::{is_over_watch_threshold, NEXT_WATCH_EPISODE_PERCENTAGE};
pub use playback_service::PlaybackService;
pub use playback_types::{NextEpisode, PlaybackItem, StreamSelection};
pub use plugin_index::{IndexedRepository, PluginHit, PluginIndex};
pub use plugin_worker_setting::PLUGIN_WORKER_LIMIT_CHOICES;
pub use regenerable_cleanup::{clean_regenerable, CleanupReport, REGENERABLE_FOLDERS};
pub use repository_service::RepositoryService;
pub use request_router::RequestRouter;
pub use routed_player::RoutedPlayer;
pub use service_error::ServiceError;
pub use settings_service::SettingsService;
pub use storage_usage::{measure_storage, FolderUsage, StorageUsage, MEASURED_FOLDERS};
pub use stream_checker::{check_worker_count, LinkHealth, StreamChecker};
pub use stream_label::{describe_link, full_label, source_label, stream_host, stream_kind};
pub use stream_resolver::{
    is_audio_only_url, link_rank, playable_links_best_first, StreamResolver,
};
pub use ui_services::UiServices;
pub use video_cache_setting::{VideoCache, VIDEO_CACHE_AHEAD_PERCENTS, VIDEO_CACHE_SIZES_MB};
