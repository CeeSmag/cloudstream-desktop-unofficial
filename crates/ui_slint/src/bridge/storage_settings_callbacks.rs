//! Settings > General: video cache, storage limit, plugin processes, cleanup and data wipe.

use slint::SharedString;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;
use super::preference_callbacks::saved;

pub(crate) fn wire_storage_settings(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_video_cache(move |value| set_cache_size(&c, &value));
    let c = ctx.clone();
    ui.on_set_ahead(move |value| set_cache_ahead(&c, &value));
    let c = ctx.clone();
    ui.on_set_storage_cap(move |value| set_storage_cap(&c, &value));
    let c = ctx.clone();
    ui.on_set_worker_limit(move |value| set_worker_limit(&c, value));
    let c = ctx.clone();
    ui.on_clean_storage(move || clean_storage(&c));
    let c = ctx.clone();
    ui.on_confirm_wipe(move || request_wipe(&c));
}

fn set_cache_size(ctx: &BridgeContext, value: &str) {
    let Some(size_mb) = leading_number(value) else {
        return;
    };
    let Ok(mut cache) = ctx.services.settings.video_cache() else {
        tracing::warn!("could not read the video cache setting");
        return;
    };
    cache.size_mb = size_mb;
    saved("video cache", ctx.services.settings.set_video_cache(cache));
    if let Some(ui) = ctx.window() {
        ui.set_video_cache_label(format!("{size_mb} MB").into());
        ui.set_cache_size_value(format!("{size_mb} MB").into());
    }
    if let Some(playback) = &ctx.services.playback {
        report(
            "cache limits",
            playback.set_cache_limits(cache.while_watching()),
        );
    }
}

fn set_cache_ahead(ctx: &BridgeContext, value: &str) {
    let Some(percent) = leading_number(value).and_then(|value| u8::try_from(value).ok()) else {
        return;
    };
    let Ok(mut cache) = ctx.services.settings.video_cache() else {
        tracing::warn!("could not read the video cache setting");
        return;
    };
    cache.ahead_percent = percent;
    saved("video cache", ctx.services.settings.set_video_cache(cache));
    if let Some(ui) = ctx.window() {
        ui.set_ahead_value(format!("{percent}%").into());
    }
    if let Some(playback) = &ctx.services.playback {
        report(
            "cache limits",
            playback.set_cache_limits(cache.while_watching()),
        );
    }
}

fn set_storage_cap(ctx: &BridgeContext, value: &str) {
    let Some(gigabytes) = leading_number(value) else {
        return;
    };
    saved(
        "storage limit",
        ctx.services.settings.set_storage_cap_gb(gigabytes),
    );
    if let Some(ui) = ctx.window() {
        ui.set_storage_label(format!("Storage limit: {gigabytes} GB").into());
        ui.set_storage_value(format!("{gigabytes} GB").into());
    }
}

fn set_worker_limit(ctx: &BridgeContext, value: SharedString) {
    let limit = match value.as_str() {
        "1" => Some(1),
        "2" => Some(2),
        _ => None,
    };
    saved(
        "plugin workers",
        ctx.services.settings.set_plugin_worker_limit(limit),
    );
    if let Some(ui) = ctx.window() {
        ui.set_worker_value(value);
    }
}

fn clean_storage(ctx: &BridgeContext) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    for folder in ["sources", "subtitles", "artwork"] {
        if let Err(error) = std::fs::remove_dir_all(dir.join(folder)) {
            tracing::debug!(folder, %error, "nothing removed");
        }
    }
    ctx.sender().notice("Cleaned regenerable caches");
}

fn request_wipe(ctx: &BridgeContext) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    match std::fs::write(dir.join("delete_all_data.pending"), b"1") {
        Ok(()) => ctx.sender().notice("Data wipe scheduled — restart the app"),
        Err(error) => {
            tracing::error!(%error, "could not schedule the data wipe");
            ctx.sender().notice("Could not schedule the data wipe");
        }
    }
}

/// The number a drop-down label starts with ("256 MB" → 256).
fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}
