//! Where the intro, recap and credits of an episode or film are.
//!
//! Providers do not carry these times. The reference app asks community databases keyed by the
//! title's ids (AniSkip by MyAnimeList id, TheIntroDB by TMDb or IMDb id, IntroDB by IMDb id) and
//! uses the first that knows; this crate does the same. Nothing here touches the player: it turns
//! a title, episode and length into a list of [`SkipSegment`]s, or an empty list.

#![forbid(unsafe_code)]

mod ani_skip;
mod clean_segments;
mod external_ids;
mod intro_db;
mod skip_cache;
mod skip_fetcher;
mod skip_lookup;
mod skip_segment;
#[cfg(test)]
mod test_fetcher;
mod the_intro_db;
mod to_ms;

pub use external_ids::ExternalIds;
pub use skip_fetcher::{SkipError, SkipFetcher, UreqSkipFetcher};
pub use skip_lookup::SkipLookup;
pub use skip_segment::{SkipKind, SkipQuery, SkipSegment, TitleKind};
