//! libmpv implementation of the `VideoPlayer` contract (loaded at runtime).
#![deny(unsafe_code)]

mod libmpv_locator;
mod mpv_api;
mod mpv_event;
mod mpv_option_format;
mod mpv_player;
mod status_tracker;

pub use libmpv_locator::locate_libmpv;
pub use mpv_player::{MpvPlayer, PlayerOptions};
