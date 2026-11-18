//! The Cloudflare check dialog: allow, done, cancel.

use cloudflare_bridge::Outcome;

use crate::CloudStream;

use super::bridge_context::BridgeContext;

pub(crate) fn wire_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_allow_cf_host(move || allow_host(&c));
    let c = ctx.clone();
    ui.on_cf_done(move || {
        let passed = Outcome::Passed {
            user_agent: "Mozilla/5.0 CloudStream".into(),
            cookies: Vec::new(),
        };
        answer(&c, &passed);
        c.window().inspect(|ui| ui.set_show_cf(false));
    });
    let c = ctx.clone();
    ui.on_cf_cancel(move || {
        answer(&c, &Outcome::Declined("viewer cancelled".into()));
        c.window().inspect(|ui| ui.set_show_cf(false));
    });
}

fn allow_host(ctx: &BridgeContext) {
    let Some(host) = ctx.session.borrow().cf_host.clone() else {
        return;
    };
    ctx.session.borrow_mut().allowed_cf.insert(host);
    if let Some(ui) = ctx.window() {
        if let Err(error) = open::that(ui.get_cf_url().as_str()) {
            tracing::warn!(%error, "could not open the system browser");
        }
    }
    answer(
        ctx,
        &Outcome::Waiting("viewer allowed host; complete check in browser".into()),
    );
}

fn answer(ctx: &BridgeContext, outcome: &Outcome) {
    let id = ctx.session.borrow().cf_id.clone();
    let (Some(dir), Some(id)) = (ctx.cloudflare_dir(), id) else {
        return;
    };
    if let Err(error) = cloudflare_bridge::write_result(&dir, &id, outcome) {
        tracing::warn!(%error, %id, "could not answer the sidecar");
    }
}
