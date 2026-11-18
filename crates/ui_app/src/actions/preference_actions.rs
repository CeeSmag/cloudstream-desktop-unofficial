//! Preference toggles (debug logging, proxy, skip, autoplay, worker limit).

use crate::app_model::AppModel;

use super::settings_actions::{refresh_local_state, report};
use super::update_actions;
use super::{Dispatcher, UiAction};

pub(super) fn perform(dispatcher: &Dispatcher, model: &mut AppModel, action: UiAction) -> bool {
    if update_actions::perform(dispatcher, model, &action) {
        return true;
    }
    let services = &dispatcher.services;
    match action {
        UiAction::SetDebugLogging(enabled) => {
            report(
                model,
                "Cannot save debug logging",
                services.settings.set_debug_logging_enabled(enabled),
            );
            services.extensions.set_debug_logging(enabled);
        }
        UiAction::SetStreamProxy(enabled) => {
            report(
                model,
                "Cannot save stream proxy",
                services.settings.set_stream_proxy(enabled),
            );
        }
        UiAction::SetSkipLookup(enabled) => {
            report(
                model,
                "Cannot save skip lookup",
                services.settings.set_skip_lookup(enabled),
            );
        }
        UiAction::SetAutoplayNext(enabled) => {
            report(
                model,
                "Cannot save autoplay",
                services.settings.set_autoplay_next(enabled),
            );
        }
        UiAction::SetPluginWorkerLimit(limit) => set_worker_limit(dispatcher, model, limit),
        _ => return false,
    }
    refresh_local_state(dispatcher, model);
    true
}

fn set_worker_limit(dispatcher: &Dispatcher, model: &mut AppModel, limit: u32) {
    let value = if limit == 0 {
        None
    } else {
        Some(limit as usize)
    };
    report(
        model,
        "Cannot save plugin worker limit",
        dispatcher.services.settings.set_plugin_worker_limit(value),
    );
    dispatcher
        .services
        .extensions
        .set_resident_worker_limit(value);
}
