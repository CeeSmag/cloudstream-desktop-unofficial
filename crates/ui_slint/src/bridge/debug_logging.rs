//! Settings > General > Debug logging: detailed JSONL log files for tracing everything in the app,
//! UI, video player (mpv), Cloudflare helper, and plugin sidecar processes.

use crate::background_work::spawn_work;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::preference_callbacks::saved;

pub(crate) fn wire_debug_logging(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_debug_logging(move |on| {
        tracing::info!(enabled = on, "user toggled debug logging");
        saved(
            "debug logging",
            c.services.settings.set_debug_logging_enabled(on),
        );
        let applied = apply(&c, on, true);
        if let Some(ui) = c.window() {
            ui.set_debug_logging(on && applied);
            ui.set_notice(notice(on, applied).into());
        }
    });
}

/// Applies the saved choice (off unless the viewer switched it on). Runs before extensions start
/// so sidecar workers inherit the setting on launch without duplicate RPCs.
pub(crate) fn apply_saved_debug_logging(ctx: &BridgeContext, ui: &CloudStream) {
    let on = ctx
        .services
        .settings
        .debug_logging_enabled()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the debug logging choice; leaving it off");
            false
        });
    // Off is the default and needs nothing switched.
    ui.set_debug_logging(on && apply(ctx, true, false));
}

/// Switches the app's debug files, mpv verbosity, Cloudflare helper verbosity, and sidecar detail.
/// Returns false when the debug log files themselves cannot be opened.
fn apply(ctx: &BridgeContext, on: bool, spawn_for_sidecars: bool) -> bool {
    let result = match ctx.extras.borrow().debug_logs.as_ref() {
        Some(switch) => switch(on),
        None => Ok(()),
    };
    let effective = on && result.is_ok();
    if let Some(playback) = &ctx.services.playback {
        playback.set_verbose_player_log(effective);
    }
    if let Some(cloudflare) = ctx.extras.borrow().cloudflare.as_ref() {
        cloudflare.set_debug_logging(effective);
    }
    if spawn_for_sidecars {
        // Live plugin processes are asked over their pipe, which can wait on a busy plugin.
        let services = ctx.owned_services();
        spawn_work("debug-logging", move || {
            services.extensions.set_debug_logging(effective);
        });
    } else {
        // At startup no workers are running yet; setting the pool flag synchronously is instant.
        ctx.services.extensions.set_debug_logging(effective);
    }
    match result {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, on, "could not switch the debug log files");
            false
        }
    }
}

/// The toast after the switch was flipped.
pub(crate) fn notice(on: bool, applied: bool) -> &'static str {
    match (on, applied) {
        (true, true) => "Debug logging on: detailed .jsonl logs go to logs/debug",
        (true, false) => "Debug logging could not start: the log files cannot be written",
        (false, _) => "Debug logging off",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_says_where_logs_go_or_why_not() {
        assert!(notice(true, true).contains("logs/debug"));
        assert!(notice(true, true).contains(".jsonl"));
        assert!(notice(true, false).contains("could not"));
        assert_eq!(notice(false, true), "Debug logging off");
    }
}
