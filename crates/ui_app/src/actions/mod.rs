//! The only place the UI calls `app_services`. Screens emit `UiAction`s; this module performs them.
mod browse_actions;
mod dispatcher;
mod playback_actions;
mod settings_actions;
mod ui_action;

pub(crate) use dispatcher::Dispatcher;
pub(crate) use playback_actions::handle_resolved;
pub(crate) use ui_action::UiAction;
