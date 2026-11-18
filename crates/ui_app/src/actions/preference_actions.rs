//! Preference toggles (debug, proxy, skip, autoplay, workers, languages).

use player_api::{find_language, Language};

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
        UiAction::SetSettingsPage(page) => {
            model.settings.page = page;
            return true;
        }
        UiAction::SetExtensionsTab(tab) => {
            model.settings.extensions_tab = tab;
            return true;
        }
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
        UiAction::SetPreferredAudio(code) => {
            let language = code.as_deref().and_then(find_language);
            report(
                model,
                "Cannot save preferred audio",
                services.settings.set_preferred_audio(language),
            );
            if let Some(playback) = &services.playback {
                playback.set_preferred_audio(language);
            }
        }
        UiAction::SetPreferredSubtitles(codes) => {
            let languages: Vec<Language> = codes.iter().filter_map(|c| find_language(c)).collect();
            report(
                model,
                "Cannot save preferred subtitles",
                services.settings.set_preferred_subtitles(&languages),
            );
            if let Some(playback) = &services.playback {
                playback.set_preferred_subtitles(&languages);
            }
        }
        UiAction::SetAutoSubtitles(enabled) => {
            report(
                model,
                "Cannot save auto subtitles",
                services.settings.set_auto_select_subtitles(enabled),
            );
        }
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
