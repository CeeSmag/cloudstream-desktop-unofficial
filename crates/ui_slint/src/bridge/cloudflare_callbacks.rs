//! The Cloudflare banner's buttons.

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::cloudflare_drain::render_prompts;

pub(crate) fn wire_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_cf_verify(move || verify_site(&c));
    let c = ctx.clone();
    ui.on_cf_dismiss(move || {
        if let Some(ui) = c.window() {
            let id = ui.get_cf_request_id();
            if let Some(watch) = c.extras.borrow().cloudflare.as_ref() {
                watch.dismiss_check(&id);
            }
            let mut queue = c.prompts.borrow_mut();
            queue.finish_shown(&id);
            render_prompts(&ui, &queue);
        }
    });
}

/// Sends the action for the exact request displayed in the banner.
fn verify_site(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let id = ui.get_cf_request_id();
    let extras = ctx.extras.borrow();
    if let Some(watch) = extras.cloudflare.as_ref() {
        match ui.get_cf_state() {
            1 => watch.verify_now(&id),
            2 => watch.show_check(&id),
            4 if ui.get_cf_can_retry() => {
                let mut queue = ctx.prompts.borrow_mut();
                queue.retrying(watch.retry_check(&id));
                render_prompts(&ui, &queue);
            }
            _ => {}
        }
    }
}
