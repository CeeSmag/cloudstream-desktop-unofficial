//! Which CloudStream plugins the app supports: video plugins (movies, series, anime, cartoons,
//! documentaries, live TV, torrents, NSFW, plain video) and nothing else, no games, music,
//! audiobooks or tools. Pure: no I/O, no logging, no clock.
//!
//! One rule for every place that decides: the plugin catalog, install, loading at start, the
//! provider list, and `tools/extension_recorder` (through the `plugin-content-policy` command of the
//! `cli` feature). Callers keep the facts (a listing's `tvTypes`) and ask again each time, so a change
//! of rule applies to plugins already installed.
//!
//! Repository labels are read leniently ([`parse_labels`]: case, spelling variants, comma lists) and
//! judged strictly: only a video type allows a plugin, except a short list of plugins a person
//! reviewed ([`REVIEWED`]), because `Others` is used by video sites and games alike.
#![forbid(unsafe_code)]

mod media_type;
mod policy;
mod repository_id;
mod review_list;

pub use media_type::{parse_labels, MediaType, ParsedLabels};
pub use policy::{Allowed, ContentPolicy, PluginFacts, Refusal, Verdict};
pub use repository_id::RepositoryId;
pub use review_list::{find_reviewed, ReviewedPlugin, REVIEWED};
