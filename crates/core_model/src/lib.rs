//! Pure data types shared across the app. No I/O.
#![forbid(unsafe_code)]

mod home_page;
mod load_details;
mod playable_link;
mod provider_info;
mod search_result;

pub use home_page::{HomePage, HomePageSection};
pub use load_details::{EpisodeInfo, LoadDetails};
pub use playable_link::{PlayableLink, PlayableLinks, SubtitleTrack};
pub use provider_info::{MainPageEntry, ProviderInfo};
pub use search_result::{SearchPage, SearchResult};
