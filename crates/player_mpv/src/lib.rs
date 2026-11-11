//! libmpv implementation of the `VideoPlayer` contract (loaded at runtime).
#![deny(unsafe_code)]

mod chapter_reader;
mod event_worker;
mod frame_renderer;
mod libmpv_locator;
mod load_tracker;
mod mpv_api;
mod mpv_event;
mod mpv_log;
mod mpv_option_format;
mod mpv_player;
mod mpv_render_api;
mod pending_subtitles;
mod player_configuration;
mod preferred_audio;
mod preferred_subtitles;
mod status_tracker;
mod subtitle_attacher;
mod track_reader;

pub use libmpv_locator::{auto_detect_player, locate_libmpv, locate_libmpv_candidates};
pub use mpv_player::{MpvPlayer, PlayerOptions};
