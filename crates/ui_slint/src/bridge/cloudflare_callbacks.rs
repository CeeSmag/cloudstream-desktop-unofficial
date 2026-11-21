//! The Cloudflare banner's buttons.

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::preference_callbacks::saved;

pub(crate) fn wire_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_cf_verify(move || verify_site(&c));
    let weak = ctx.ui.clone();
    ui.on_cf_dismiss(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_cf_state(0);
        }
    });
}

/// The viewer allowed the site: remember it and open its verification window now.
fn verify_site(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let host = ui.get_cf_host().to_string();
    if ui.get_cf_state() == 2 {
        if let Some(watch) = ctx.extras.borrow().cloudflare.as_ref() {
            watch.show_check(&host, &ui.get_cf_request_id());
        }
        return;
    }
    saved(
        "Cloudflare site",
        ctx.services.settings.allow_cloudflare_host(&host),
    );
    match ctx.extras.borrow().cloudflare.as_ref() {
        Some(watch) => {
            watch.verify_now(&host);
            ui.set_cf_request_id("".into());
            ui.set_cf_state(2);
        }
        None => {
            ui.set_cf_state(4);
            ui.set_cf_message("verification is not available in this build".into());
        }
    }
}
