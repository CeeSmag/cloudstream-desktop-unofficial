//! Settings > General > "Performance & memory": the profile drop-down and Reset, and keeping the
//! card, the cards of the limits it controls and what is running in step with each other.

use app_services::UiServices;

use crate::performance_text::{describe, profile_from_label, profile_labels};
use crate::posters::string_model;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::initial_settings::show_tuning_values;
use super::player_callbacks::report;
use super::preference_callbacks::saved;

pub(crate) fn wire_performance_settings(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_performance_profile(move |label| choose_profile(&c, label.as_str()));
    let c = ctx.clone();
    ui.on_reset_performance(move || reset_to_recommended(&c));
}

fn choose_profile(ctx: &BridgeContext, label: &str) {
    let Some(choice) = profile_from_label(label) else {
        tracing::warn!(label, "unknown performance profile");
        return;
    };
    tracing::debug!(profile = choice.as_text(), "performance profile chosen");
    saved(
        "performance profile",
        ctx.services.settings.set_profile_choice(choice),
    );
    apply_limits_now(ctx);
}

fn reset_to_recommended(ctx: &BridgeContext) {
    tracing::debug!("performance settings reset to recommended");
    saved(
        "performance reset",
        ctx.services.settings.reset_performance(),
    );
    apply_limits_now(ctx);
}

/// Applies the limits in force to what is running (resident plugin processes, the video cache of
/// a playing video) and shows them on their cards.
fn apply_limits_now(ctx: &BridgeContext) {
    let services = &ctx.services;
    match services.settings.plugin_worker_limit() {
        Ok(limit) => services.extensions.set_resident_worker_limit(limit),
        Err(error) => tracing::warn!(%error, "plugin process limit not read"),
    }
    if let (Some(playback), Ok(cache)) = (&services.playback, services.settings.video_cache()) {
        report(
            "cache limits",
            playback.set_cache_limits(cache.while_watching()),
        );
    }
    if let Some(ui) = ctx.window() {
        show_tuning_values(&ui, services);
        show_performance(&ui, services);
    }
}

/// Shows the profile, what the PC is, which preset is followed and the warning.
pub(crate) fn show_performance(ui: &CloudStream, services: &UiServices) {
    ui.set_perf_profile_labels(string_model(profile_labels()));
    let installed = services.extensions.installed().map_or(0, |list| list.len());
    match services.settings.performance_summary(installed) {
        Ok(summary) => {
            let view = describe(&summary);
            ui.set_perf_profile_value(view.profile_value.into());
            ui.set_perf_pc_text(view.pc_text.into());
            ui.set_perf_state_text(view.state_text.into());
            ui.set_perf_warning(view.warning.into());
            ui.set_perf_can_reset(view.can_reset);
        }
        Err(error) => tracing::warn!(%error, "could not read the performance settings"),
    }
}
