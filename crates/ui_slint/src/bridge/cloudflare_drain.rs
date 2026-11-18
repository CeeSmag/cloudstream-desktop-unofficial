//! Picks up Cloudflare verification requests from the sidecar's exchange folder watcher.

use cloudflare_bridge::{CloudflareEvent, Outcome, VerifyRequest};

use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

/// Handles every request the watcher found since the last tick.
pub(crate) fn drain_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let pending: Vec<CloudflareEvent> = {
        let extras = ctx.extras.borrow();
        let Some(watch) = extras.cloudflare.as_ref() else {
            return;
        };
        watch.events.try_iter().collect()
    };
    for CloudflareEvent::Needed(request) in pending {
        handle_request(ctx, ui, request);
    }
}

fn handle_request(ctx: &BridgeContext, ui: &CloudStream, request: VerifyRequest) {
    let allowed = ctx.session.borrow().allowed_cf.contains(&request.host);
    if !allowed {
        ctx.sender().send(UiEvent::CloudflareNeeded {
            host: request.host,
            url: request.url,
            id: request.id,
        });
        return;
    }
    if let Err(error) = open::that(&request.url) {
        tracing::warn!(%error, host = %request.host, "could not open the system browser");
    }
    if let Some(dir) = ctx.cloudflare_dir() {
        let outcome = Outcome::Waiting("complete verification in browser".into());
        if let Err(error) = cloudflare_bridge::write_result(&dir, &request.id, &outcome) {
            tracing::warn!(%error, id = %request.id, "could not answer the sidecar");
        }
    }
    ui.set_notice(format!("Cloudflare: complete check for {}", request.host).into());
}
