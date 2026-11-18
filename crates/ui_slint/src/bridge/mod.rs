//! Wire Slint callbacks to application services.

mod catalog;
mod events;
mod events_extra;
mod open;
mod poster_hydrate;
mod run;
mod session;
mod settings;
mod settings_extra;
mod settings_prefs;
mod wire;
mod wire_extra;

pub use run::{run_desktop_app, run_desktop_app_with};
pub use session::AppExtras;
