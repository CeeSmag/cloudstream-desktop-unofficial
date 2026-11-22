//! Shows what the Cloudflare watcher reports, as a banner under the top bar.

use std::time::Duration;

use slint::Model;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::cloudflare_prompts::{banner_for, PromptQueue};
use super::home_loading::load_home;
use super::preference_callbacks::saved;

/// How long "verified" stays on screen.
const VERIFIED_SHOWN: Duration = Duration::from_secs(6);

/// Puts the prompt the queue shows (or none) on the banner.
pub(crate) fn render_prompts(ui: &CloudStream, queue: &PromptQueue) {
    match queue.shown() {
        Some(view) => {
            ui.set_cf_state(view.state);
            ui.set_cf_request_id(view.request_id.as_str().into());
            ui.set_cf_host(view.host.as_str().into());
            ui.set_cf_message(view.message.as_str().into());
            ui.set_cf_can_retry(view.can_retry);
        }
        None => {
            ui.set_cf_state(0);
            ui.set_cf_request_id("".into());
        }
    }
    ui.set_cf_more(i32::try_from(queue.more()).unwrap_or(i32::MAX));
}

/// Shows every event the watcher sent since the last tick.
pub(crate) fn drain_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let pending: Vec<cloudflare_bridge::CloudflareEvent> = {
        let extras = ctx.extras.borrow();
        let Some(watch) = extras.cloudflare.as_ref() else {
            return;
        };
        watch.events.try_iter().collect()
    };
    for event in pending {
        if let cloudflare_bridge::CloudflareEvent::PermissionGranted { host } = &event {
            saved(
                "Cloudflare site",
                ctx.services.settings.allow_cloudflare_host(host),
            );
        }
        let Some(view) = banner_for(event) else {
            continue;
        };
        let verified = (view.state == 3).then(|| view.request_id.clone());
        ctx.prompts.borrow_mut().apply(view);
        if let Some(id) = verified {
            if ctx
                .prompts
                .borrow()
                .shown()
                .is_some_and(|shown| shown.request_id == id)
            {
                after_verified(ctx, ui, &id);
            }
        }
    }
    render_prompts(ui, &ctx.prompts.borrow());
}

/// The site passed: Home is loaded again if it came back empty, and the banner hides itself.
fn after_verified(ctx: &BridgeContext, ui: &CloudStream, id: &str) {
    if ui.get_home_shelves().row_count() == 0 {
        load_home(ctx);
    }
    let weak = ctx.ui.clone();
    let prompts = ctx.prompts.clone();
    let id = id.to_owned();
    slint::Timer::single_shot(VERIFIED_SHOWN, move || {
        if let Some(ui) = weak.upgrade() {
            let mut queue = prompts.borrow_mut();
            if queue
                .shown()
                .is_some_and(|shown| shown.state == 3 && shown.request_id == id)
            {
                queue.finish_shown(&id);
                render_prompts(&ui, &queue);
            }
        }
    });
}
