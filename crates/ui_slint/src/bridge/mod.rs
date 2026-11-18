//! Connects the Slint window to the application services.

mod app_start;
mod bridge_context;
mod browse_loading;
mod callback_wiring;
mod catalog_callbacks;
mod catalog_events;
mod cloudflare_callbacks;
mod cloudflare_drain;
mod details_callbacks;
mod details_loading;
mod event_dispatch;
mod extension_settings_callbacks;
mod home_callbacks;
mod home_loading;
mod initial_settings;
mod player_callbacks;
mod player_polling;
mod player_track_callbacks;
mod poster_hydrate;
mod preference_callbacks;
mod session;
mod storage_settings_callbacks;
mod stream_events;
mod stream_opening;
mod title_search;
mod update_callbacks;

pub use app_start::{run_desktop_app, run_desktop_app_with};
pub use session::AppExtras;
pub(crate) use title_search::SearchSummary;
