//! Settings > General > Debug logging: detailed log files for tracing odd behaviour, in the app
//! and in the plugin processes (like the old DebugLoggingCard). The choice is kept and applied at
//! every start; before, the Slint app saved it but never switched anything.

use crate::background_work::spawn_work;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::preference_callbacks::saved;

pub(crate) fn wire_debug_logging(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_debug_logging(move |on| {
        saved(
            "debug logging",
            c.services.settings.set_debug_logging_enabled(on),
        );
        let applied = apply(&c, on);
        if let Some(ui) = c.window() {
            ui.set_debug_logging(on && applied);
            ui.set_notice(notice(on, applied).into());
        }
    });
}

/// Applies the saved choice (off unless the viewer switched it on).
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
    ui.set_debug_logging(on && apply(ctx, true));
}

/// Switches the app's debug files and the plugins' detail; false when the files cannot be opened.
fn apply(ctx: &BridgeContext, on: bool) -> bool {
    let result = match ctx.extras.borrow().debug_logs.as_ref() {
        Some(switch) => switch(on),
        None => Ok(()),
    };
    // The plugin processes are asked over their pipe, which can wait on a busy plugin.
    let services = ctx.owned_services();
    spawn_work("debug-logging", move || {
        services.extensions.set_debug_logging(on)
    });
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
        (true, true) => "Debug logging on: detailed logs go to the logs/debug folder",
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
        assert!(notice(true, false).contains("could not"));
        assert_eq!(notice(false, true), "Debug logging off");
    }
}
