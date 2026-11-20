//! Settings > General: video cache, storage limit, plugin processes, cleanup and data wipe.

use app_services::{clean_regenerable, measure_storage};
use slint::SharedString;

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::player_callbacks::report;
use super::preference_callbacks::saved;
use super::storage_usage_view::measure_storage_usage;

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
        ui.set_storage_value(format!("{gigabytes} GB").into());
    }
    let cleanup = ctx.extras.borrow().cleanup.clone();
    let dir = ctx.extras.borrow().data_dir.clone();
    match (cleanup, dir) {
        (Some(handle), Some(dir)) => {
            let sender = ctx.sender();
            spawn_work("storage-cap-enforce", move || {
                let _ = handle.enforce_cap_now();
                sender.send(UiEvent::StorageMeasured(measure_storage(&dir)));
            });
        }
        _ => measure_storage_usage(ctx),
    }
}

fn set_worker_limit(ctx: &BridgeContext, value: SharedString) {
    let limit = worker_limit_from_label(value.as_str());
    saved(
        "plugin workers",
        ctx.services.settings.set_plugin_worker_limit(limit),
    );
    // Apply now (egui does the same); settings alone only take effect on the next start.
    ctx.services.extensions.set_resident_worker_limit(limit);
    if let Some(ui) = ctx.window() {
        ui.set_worker_value(value);
    }
}

/// "1" / "2" / anything else → Some(1) / Some(2) / unlimited.
pub(crate) fn worker_limit_from_label(value: &str) -> Option<usize> {
    match value {
        "1" => Some(1),
        "2" => Some(2),
        _ => None,
    }
}

fn clean_storage(ctx: &BridgeContext) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    let cleanup = ctx.extras.borrow().cleanup.clone();
    ctx.sender().notice("Cleaning up…");
    let sender = ctx.sender();
    spawn_work("storage-cleanup-now", move || {
        let report = match cleanup {
            Some(handle) => handle.run_manual_now(),
            None => clean_regenerable(&dir),
        };
        sender.send(UiEvent::StorageMeasured(measure_storage(&dir)));
        let megabytes = report.bytes / (1024 * 1024);
        sender.notice(format!(
            "Cleaned up {} files ({megabytes} MB). Sources, subtitles and posters load again when needed.",
            report.files
        ));
    });
}

fn request_wipe(ctx: &BridgeContext) {
    let result = match ctx.extras.borrow().request_wipe.as_ref() {
        Some(request) => request(),
        None => Err(std::io::Error::other("this build cannot delete its data")),
    };
    match result {
        Ok(()) => {
            if let Err(error) = slint::quit_event_loop() {
                tracing::warn!(%error, "could not close window for data wipe");
                ctx.sender()
                    .notice("All saved data will be deleted when you restart CloudStream");
            }
        }
        Err(error) => {
            tracing::error!(%error, "could not schedule the data wipe");
            ctx.sender()
                .notice(format!("Could not delete saved data: {error}"));
        }
    }
}

/// The number a drop-down label starts with ("256 MB" → 256).
fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_limit_from_label_maps_menu_choices() {
        assert_eq!(worker_limit_from_label("1"), Some(1));
        assert_eq!(worker_limit_from_label("2"), Some(2));
        assert_eq!(worker_limit_from_label("Unlimited"), None);
    }
}
