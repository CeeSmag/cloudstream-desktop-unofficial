//! Projects [`PromptQueue`] to `CloudflareBannerState` and drains watcher events on every UI tick.

use std::time::Instant;

use slint::{ComponentHandle, Model};

use crate::cloudflare_prompts::{BannerPhase, BannerSnapshot, PromptQueue};
use crate::{CloudStream, CloudflareBannerPhase, CloudflareBannerState};

use super::bridge_context::BridgeContext;
use super::home_loading::load_home;
use super::preference_callbacks::saved;

/// Projects `queue`'s current snapshot into the `CloudflareBannerState` Slint global.
pub(crate) fn render_banner(ui: &CloudStream, queue: &PromptQueue) {
    apply_snapshot(ui, &queue.snapshot());
}

fn apply_snapshot(ui: &CloudStream, snap: &BannerSnapshot) {
    let banner = ui.global::<CloudflareBannerState>();
    banner.set_phase(slint_phase(snap.phase));
    banner.set_host(snap.host.as_str().into());
    banner.set_message(snap.message.as_str().into());
    banner.set_can_retry(snap.can_retry);
    banner.set_waiting_count(i32::try_from(snap.waiting_count).unwrap_or(i32::MAX));
    banner.set_can_cycle(snap.can_cycle);
}

fn slint_phase(phase: BannerPhase) -> CloudflareBannerPhase {
    match phase {
        BannerPhase::Hidden => CloudflareBannerPhase::Hidden,
        BannerPhase::NeedsPermission => CloudflareBannerPhase::NeedsPermission,
        BannerPhase::Verifying => CloudflareBannerPhase::Verifying,
        BannerPhase::Verified => CloudflareBannerPhase::Verified,
        BannerPhase::Failed => CloudflareBannerPhase::Failed,
    }
}

/// Drains watcher events and expires any elapsed `"verified"` banner on the UI tick.
pub(crate) fn drain_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let pending = take_pending_events(ctx);
    let now = Instant::now();
    let mut granted = Vec::new();
    let mut any_passed = false;
    let mut changed = false;
    {
        let mut queue = ctx.prompts.borrow_mut();
        for event in pending {
            let outcome = queue.apply_event(event, now);
            if let Some(host) = outcome.granted_host {
                granted.push(host);
            }
            any_passed |= outcome.passed;
            changed |= outcome.changed;
        }
        if queue.expire(now) {
            tracing::debug!("expired verified Cloudflare banner");
            changed = true;
        }
        if changed {
            let snap = queue.snapshot();
            tracing::debug!(
                phase = ?snap.phase,
                host = %snap.host,
                waiting = snap.waiting_count,
                can_cycle = snap.can_cycle,
                "updated Cloudflare banner"
            );
            apply_snapshot(ui, &snap);
        }
    }
    for host in granted {
        saved(
            "Cloudflare site",
            ctx.services.settings.allow_cloudflare_host(&host),
        );
    }
    if any_passed && ui.get_home_shelves().row_count() == 0 && !ui.get_home_loading() {
        load_home(ctx);
    }
}

fn take_pending_events(ctx: &BridgeContext) -> Vec<cloudflare_bridge::CloudflareEvent> {
    let extras = ctx.extras.borrow();
    extras
        .cloudflare
        .as_ref()
        .map(|watch| watch.events.try_iter().collect())
        .unwrap_or_default()
}
