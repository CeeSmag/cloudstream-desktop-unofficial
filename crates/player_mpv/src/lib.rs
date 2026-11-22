//! libmpv implementation of the `VideoPlayer` contract (loaded at runtime).
#![deny(unsafe_code)]

mod cache_limits;
mod chapter_reader;
mod diagnostics_timer;
mod event_worker;
mod frame_renderer;
mod hwdec_problem;
mod libmpv_locator;
mod load_tracker;
mod loadfile_log;
mod mpv_api;
mod mpv_event;
mod mpv_gl_output;
mod mpv_log;
mod mpv_option_format;
mod mpv_player;
mod mpv_render_api;
mod pending_subtitles;
mod playback_diagnostics;
mod player_configuration;
mod preferred_audio;
mod preferred_subtitles;
mod status_tracker;
mod subtitle_activity;
mod subtitle_attacher;
mod track_reader;
mod track_refresh;

pub use libmpv_locator::{auto_detect_player, locate_libmpv, probe_libmpv};
pub use mpv_player::{MpvPlayer, PlayerOptions};
