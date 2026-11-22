//! The Cloudflare banner's buttons.

use crate::CloudStream;

use super::bridge_context::BridgeContext;

pub(crate) fn wire_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_cf_verify(move || verify_site(&c));
    let c = ctx.clone();
    ui.on_cf_dismiss(move || {
        if let Some(ui) = c.window() {
            if let Some(watch) = c.extras.borrow().cloudflare.as_ref() {
                watch.dismiss_check(&ui.get_cf_request_id());
            }
            ui.set_cf_state(0);
            ui.set_cf_request_id("".into());
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
                ui.set_cf_request_id(watch.retry_check(&id).into());
                ui.set_cf_can_retry(false);
                ui.set_cf_state(2);
            }
            _ => {}
        }
    }
}
