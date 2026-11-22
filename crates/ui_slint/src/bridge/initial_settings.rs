//! Shows the saved settings when the window opens.

use app_services::UiServices;
use player_api::all_languages;

use super::performance_callbacks::show_performance;
use super::preference_callbacks::apply_player_language_preferences;
use super::subtitle_languages::show_subtitle_languages;
use crate::posters::string_model;
use crate::CloudStream;

/// Copies every saved setting into the window's properties and the player.
pub(crate) fn apply_settings_to_ui(ui: &CloudStream, services: &UiServices) {
    apply_switches(ui, services);
    show_tuning_values(ui, services);
    show_performance(ui, services);
    apply_languages(ui, services);
    apply_player_language_preferences(services);
    ui.set_update_label("Updates not checked yet".into());
}

/// Shows the video cache, storage limit and plugin process limit in force, with their choices.
pub(crate) fn show_tuning_values(ui: &CloudStream, services: &UiServices) {
    apply_cache_choices(ui, services);
    apply_storage_and_workers(ui, services);
}

fn apply_switches(ui: &CloudStream, services: &UiServices) {
    let settings = &services.settings;
    ui.set_autoplay_next(settings.autoplay_next().unwrap_or(true));
    ui.set_skip_lookup(settings.skip_lookup().unwrap_or(true));
    ui.set_stream_proxy(settings.stream_proxy().unwrap_or(true));
    ui.set_auto_subtitles(settings.auto_select_subtitles().unwrap_or(true));
}

fn apply_cache_choices(ui: &CloudStream, services: &UiServices) {
    match services.settings.video_cache() {
        Ok(cache) => {
            ui.set_cache_size_value(format!("{} MB", cache.size_mb).into());
            ui.set_ahead_value(format!("{}%", cache.ahead_percent).into());
        }
        Err(error) => tracing::warn!(%error, "could not read the video cache setting"),
    }
    let sizes = app_services::VIDEO_CACHE_SIZES_MB.iter();
    ui.set_cache_size_labels(string_model(sizes.map(|mb| format!("{mb} MB"))));
    let shares = app_services::VIDEO_CACHE_AHEAD_PERCENTS.iter();
    ui.set_ahead_labels(string_model(shares.map(|percent| format!("{percent}%"))));
}

fn apply_storage_and_workers(ui: &CloudStream, services: &UiServices) {
    match services.settings.storage_cap_gb() {
        Ok(cap) => {
            ui.set_storage_label(format!("Limit {cap} GB · measured when Settings opens").into());
            ui.set_storage_value(format!("{cap} GB").into());
        }
        Err(error) => tracing::warn!(%error, "could not read the storage limit"),
    }
    let limits = memory_budget::STORAGE_LIMIT_CHOICES_GB.iter();
    ui.set_storage_labels(string_model(limits.map(|gb| format!("{gb} GB"))));
    let worker = services.settings.plugin_worker_limit().unwrap_or(Some(1));
    ui.set_worker_value(worker_label(worker).into());
    let labels = app_services::PLUGIN_WORKER_LIMIT_CHOICES
        .iter()
        .map(|limit| worker_label(*limit));
    ui.set_worker_labels(string_model(labels));
}

fn apply_languages(ui: &CloudStream, services: &UiServices) {
    let labels = language_labels();
    ui.set_audio_labels(string_model(labels));
    match services.settings.preferred_audio() {
        Ok(Some(language)) => ui.set_audio_value(language.name.into()),
        Ok(None) => ui.set_audio_value("Stream default".into()),
        Err(error) => {
            tracing::warn!(%error, "could not read the audio language; showing English");
            ui.set_audio_value("English".into());
        }
    }
    match services.settings.preferred_subtitles() {
        Ok(languages) => show_subtitle_languages(ui, &languages),
        Err(error) => {
            tracing::warn!(%error, "could not read the subtitle languages");
            show_subtitle_languages(ui, &[]);
        }
    }
}

/// The label of the plugin worker limit drop-down.
pub(crate) fn worker_label(limit: Option<usize>) -> String {
    limit.map_or_else(|| "Unlimited".to_owned(), |count| count.to_string())
}

fn language_labels() -> Vec<String> {
    let mut labels = vec!["Stream default".to_owned()];
    labels.extend(
        all_languages()
            .iter()
            .map(|language| language.name.to_owned()),
    );
    labels
}
