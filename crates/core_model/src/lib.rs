//! Pure data types shared across the app. No I/O.
#![forbid(unsafe_code)]

mod bookmark;
mod discovered_repository;
mod home_page;
mod installed_extension;
mod link_search;
mod load_details;
mod playable_link;
mod plugin_listing;
mod provider_info;
mod repository_entry;
mod search_result;
mod watch_progress;

pub use bookmark::Bookmark;
pub use discovered_repository::DiscoveredRepository;
pub use home_page::{HomePage, HomePageSection};
pub use installed_extension::InstalledExtension;
pub use link_search::{LinkSearchCursor, LinkSearchUpdate};
pub use load_details::{CastMember, EpisodeInfo, LoadDetails};
pub use playable_link::{PlayableLink, PlayableLinks, SubtitleTrack};
pub use plugin_listing::PluginListing;
pub use provider_info::{MainPageEntry, ProviderInfo};
pub use repository_entry::RepositoryEntry;
pub use search_result::{SearchPage, SearchResult};
pub use watch_progress::WatchProgress;
