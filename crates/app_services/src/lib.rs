//! Use-case services the UI calls: extensions, catalog, playback, library, settings.
#![forbid(unsafe_code)]

mod catalog_service;
mod check_queue;
mod clock;
mod extension_load_report;
mod extension_service;
mod library_service;
mod link_search;
mod playback_service;
mod plugin_index;
mod repository_service;
mod service_error;
mod settings_service;
mod stream_checker;
mod stream_label;
mod stream_resolver;
#[cfg(test)]
mod test_support;

pub use catalog_service::{CatalogService, ProviderSearchOutcome};
pub use check_queue::CheckQueue;
pub use clock::{Clock, SystemClock};
pub use extension_load_report::{ExtensionLoadReport, LoadProgress};
pub use extension_service::ExtensionService;
pub use library_service::LibraryService;
pub use link_search::{LinkBatch, LinkSearch};
pub use playback_service::{PlaybackItem, PlaybackService, StreamSelection};
pub use plugin_index::{IndexedRepository, PluginHit, PluginIndex};
pub use repository_service::RepositoryService;
pub use service_error::ServiceError;
pub use settings_service::SettingsService;
pub use stream_checker::{LinkHealth, StreamChecker};
pub use stream_label::{describe_link, full_label, source_label, stream_host, stream_kind};
pub use stream_resolver::{
    is_audio_only_url, link_rank, playable_links_best_first, StreamResolver,
};
