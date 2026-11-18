//! Wire Slint callbacks to application services.

mod catalog;
mod events;
mod open;
mod run;
mod session;
mod settings;
mod settings_extra;
mod wire;

pub use run::{run_desktop_app, run_desktop_app_with};
pub use session::AppExtras;
