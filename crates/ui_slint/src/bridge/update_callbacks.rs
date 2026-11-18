//! Settings > Updates and the logs folder.

use crate::background_work::spawn_work;
use crate::ui_events::UiEvent;
use crate::update;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

pub(crate) fn wire_updates(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_check_updates(move || check_updates(&c));
    let c = ctx.clone();
    ui.on_apply_update(move || apply_update(&c));
    let c = ctx.clone();
    ui.on_open_logs(move || {
        if let Some(dir) = c.extras.borrow().data_dir.as_ref() {
            if let Err(error) = open::that(dir.join("logs")) {
                tracing::warn!(%error, "could not open the logs folder");
            }
        }
    });
}

fn check_updates(ctx: &BridgeContext) {
    let sender = ctx.sender();
    let current = env!("CARGO_PKG_VERSION");
    spawn_work("updates", move || {
        match update::check_for_updates(current) {
            Ok(info) => {
                sender.send(UiEvent::UpdateLabel(info.label.clone()));
                sender.notice(info.label);
            }
            Err(error) => sender.notice(format!("Update check failed: {error}")),
        }
    });
}

fn apply_update(ctx: &BridgeContext) {
    let url = ctx
        .session
        .borrow()
        .update
        .as_ref()
        .and_then(|info| info.download_url.clone());
    let Some(url) = url else {
        ctx.sender().notice("Check for updates first");
        return;
    };
    let dir = ctx
        .extras
        .borrow()
        .data_dir
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    let sender = ctx.sender();
    spawn_work("apply-update", move || {
        let staged = update::download_update(&url, &dir.join("updates"))
            .and_then(|path| update::apply_downloaded(&path));
        match staged {
            Ok(()) => sender.notice("Update staged — the app will restart after exit"),
            Err(error) => sender.notice(format!("Update failed: {error}")),
        }
    });
}
