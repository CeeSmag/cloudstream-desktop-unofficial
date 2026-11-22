//! Shows what the Cloudflare watcher reports, as a banner under the top bar.

use std::time::Duration;

use cloudflare_bridge::CloudflareEvent;
use slint::Model;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::home_loading::load_home;
use super::preference_callbacks::saved;

/// How long "verified" stays on screen.
const VERIFIED_SHOWN: Duration = Duration::from_secs(6);

/// The banner's look: state (see `CloudflareBanner`), site and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudflareBannerView {
    pub(crate) state: i32,
    pub(crate) host: String,
    pub(crate) message: String,
    request_id: String,
    can_retry: bool,
}

/// The banner for `event`.
fn banner_for(event: CloudflareEvent) -> Option<CloudflareBannerView> {
    let (state, host, message, request_id, can_retry) = match event {
        CloudflareEvent::NeedsPermission { host, id } => (1, host, String::new(), id, false),
        CloudflareEvent::Verifying { host, id } => (2, host, String::new(), id, false),
        CloudflareEvent::Passed { host, id } => (3, host, String::new(), id, false),
        CloudflareEvent::Failed {
            host,
            id,
            reason,
            retry,
        } => (4, host, reason, id, retry),
        CloudflareEvent::Dismissed { id } => (0, String::new(), String::new(), id, false),
        CloudflareEvent::PermissionGranted { .. } => return None,
    };
    Some(CloudflareBannerView {
        state,
        host,
        message,
        request_id,
        can_retry,
    })
}

/// Shows every event the watcher sent since the last tick.
pub(crate) fn drain_cloudflare(ctx: &BridgeContext, ui: &CloudStream) {
    let pending: Vec<CloudflareEvent> = {
        let extras = ctx.extras.borrow();
        let Some(watch) = extras.cloudflare.as_ref() else {
            return;
        };
        watch.events.try_iter().collect()
    };
    for event in pending {
        if let CloudflareEvent::PermissionGranted { host } = &event {
            saved(
                "Cloudflare site",
                ctx.services.settings.allow_cloudflare_host(host),
            );
        }
        let Some(view) = banner_for(event) else {
            continue;
        };
        if !view.matches_prompt(ui.get_cf_state(), ui.get_cf_request_id().as_str()) {
            continue;
        }
        if view.state == 3 {
            after_verified(ctx, ui, &view.request_id);
        }
        ui.set_cf_state(view.state);
        ui.set_cf_request_id(view.request_id.into());
        ui.set_cf_host(view.host.into());
        ui.set_cf_message(view.message.into());
        ui.set_cf_can_retry(view.can_retry);
    }
}

impl CloudflareBannerView {
    fn matches_prompt(&self, current: i32, current_id: &str) -> bool {
        match self.state {
            0 | 3 => self.request_id == current_id,
            4 => current == 0 || self.request_id == current_id,
            _ => true,
        }
    }
}

/// The site passed: Home is loaded again if it came back empty, and the banner hides itself.
fn after_verified(ctx: &BridgeContext, ui: &CloudStream, id: &str) {
    if ui.get_home_shelves().row_count() == 0 {
        load_home(ctx);
    }
    let weak = ctx.ui.clone();
    let id = id.to_owned();
    slint::Timer::single_shot(VERIFIED_SHOWN, move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_cf_state() == 3 && ui.get_cf_request_id() == id.as_str() {
                ui.set_cf_state(0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_for_maps_each_event_to_its_state() {
        let host = || "site.test".to_owned();
        assert_eq!(
            banner_for(CloudflareEvent::NeedsPermission {
                host: host(),
                id: "r1".into()
            })
            .unwrap()
            .state,
            1
        );
        assert_eq!(
            banner_for(CloudflareEvent::Verifying {
                host: host(),
                id: "r1".into()
            })
            .unwrap()
            .state,
            2
        );
        assert_eq!(
            banner_for(CloudflareEvent::Passed {
                host: host(),
                id: "r1".into()
            })
            .unwrap()
            .state,
            3
        );
        let failed = banner_for(CloudflareEvent::Failed {
            host: host(),
            id: "r1".into(),
            reason: "closed".into(),
            retry: true,
        })
        .unwrap();
        assert_eq!((failed.state, failed.message.as_str()), (4, "closed"));
        assert!(!failed.matches_prompt(2, "r2"));
    }
}
