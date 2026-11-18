//! Slint desktop UI for CloudStream.
//!
//! Slint's generated bindings use a small amount of `unsafe`; this crate keeps that confined to
//! the include_modules expansion (same pattern as `player_mpv` for FFI).
#![deny(unsafe_code)]
#![allow(missing_docs)]
#![allow(clippy::too_many_lines)]

mod bridge;
mod poster_images;
mod posters;
mod runner;
mod update;

pub use app_services::UiServices;
pub use bridge::{run_desktop_app, run_desktop_app_with, AppExtras};

#[allow(unsafe_code)]
mod generated {
    slint::include_modules!();
}

pub use generated::*;
