//! Connects `CloudflareBannerState` button clicks to [`PromptQueue`] and `WatchHandle`.

use slint::ComponentHandle;

use crate::cloudflare_prompts::BannerCommand;
use crate::{CloudStream, CloudflareBannerState};

use super::bridge_context::BridgeContext;
use super::cloudflare_drain::render_banner;

pub(crate) fn wire_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let banner = ui.global::<CloudflareBannerState>();
    let c = ctx.clone();
    banner.on_verify(move || handle_primary(&c));
    let c = ctx.clone();
    banner.on_dismiss(move || handle_dismiss(&c));
    let c = ctx.clone();
    banner.on_next_waiting(move || handle_cycle(&c));
}

fn handle_primary(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let Some(command) = ctx.prompts.borrow().primary_action() else {
        return;
    };
    let extras = ctx.extras.borrow();
    let Some(watch) = extras.cloudflare.as_ref() else {
        return;
    };
    match command {
        BannerCommand::Verify { id } => {
            tracing::debug!(request_id = %id, "viewer approved Cloudflare verification");
            watch.verify_now(&id);
        }
        BannerCommand::Show { id } => {
            tracing::debug!(request_id = %id, "viewer opened Cloudflare verification window");
            watch.show_check(&id);
        }
        BannerCommand::Retry { id } => {
            let attempt = watch.retry_check(&id);
            tracing::debug!(failed_id = %id, attempt_id = %attempt, "viewer retried Cloudflare verification");
            let mut queue = ctx.prompts.borrow_mut();
            queue.begin_retry(&id, attempt);
            render_banner(&ui, &queue);
        }
        BannerCommand::Dismiss { .. } => {}
    }
}

fn handle_dismiss(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let mut queue = ctx.prompts.borrow_mut();
    let command = queue.dismiss_shown();
    render_banner(&ui, &queue);
    drop(queue);
    if let Some(BannerCommand::Dismiss { id }) = command {
        tracing::debug!(request_id = %id, "viewer dismissed active Cloudflare verification");
        if let Some(watch) = ctx.extras.borrow().cloudflare.as_ref() {
            watch.dismiss_check(&id);
        }
    }
}

fn handle_cycle(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let mut queue = ctx.prompts.borrow_mut();
    if queue.cycle_next() {
        let snap = queue.snapshot();
        tracing::debug!(
            phase = ?snap.phase,
            host = %snap.host,
            waiting = snap.waiting_count,
            "viewer cycled to next waiting Cloudflare check"
        );
        render_banner(&ui, &queue);
    }
}
