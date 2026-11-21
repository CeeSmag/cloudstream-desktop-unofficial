//! Shows what the Cloudflare watcher reports, as a banner under the top bar.

use std::time::Duration;

use cloudflare_bridge::CloudflareEvent;
use slint::Model;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::home_loading::load_home;

/// How long "verified" stays on screen.
const VERIFIED_SHOWN: Duration = Duration::from_secs(6);

/// The banner's look: state (see `CloudflareBanner`), site and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudflareBannerView {
    pub(crate) state: i32,
    pub(crate) host: String,
    pub(crate) message: String,
    request_id: String,
}

/// The banner for `event`.
pub(crate) fn banner_for(event: CloudflareEvent) -> CloudflareBannerView {
    let (state, host, message, request_id) = match event {
        CloudflareEvent::NeedsPermission { host } => (1, host, String::new(), String::new()),
        CloudflareEvent::Verifying { host, id } => (2, host, String::new(), id),
        CloudflareEvent::Passed { host } => (3, host, String::new(), String::new()),
        CloudflareEvent::Failed { host, reason } => (4, host, reason, String::new()),
    };
    CloudflareBannerView {
        state,
        host,
        message,
        request_id,
    }
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
        let view = banner_for(event);
        if view.state == 3 {
            after_verified(ctx, ui, &view.host);
        }
        ui.set_cf_state(view.state);
        ui.set_cf_request_id(view.request_id.into());
        ui.set_cf_host(view.host.into());
        ui.set_cf_message(view.message.into());
    }
}

/// The site passed: Home is loaded again if it came back empty, and the banner hides itself.
fn after_verified(ctx: &BridgeContext, ui: &CloudStream, host: &str) {
    if ui.get_home_shelves().row_count() == 0 {
        load_home(ctx);
    }
    let weak = ctx.ui.clone();
    let host = host.to_owned();
    slint::Timer::single_shot(VERIFIED_SHOWN, move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_cf_state() == 3 && ui.get_cf_host() == host.as_str() {
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
            banner_for(CloudflareEvent::NeedsPermission { host: host() }).state,
            1
        );
        assert_eq!(
            banner_for(CloudflareEvent::Verifying {
                host: host(),
                id: "r1".into()
            })
            .state,
            2
        );
        assert_eq!(
            banner_for(CloudflareEvent::Passed { host: host() }).state,
            3
        );
        let failed = banner_for(CloudflareEvent::Failed {
            host: host(),
            reason: "closed".into(),
        });
        assert_eq!((failed.state, failed.message.as_str()), (4, "closed"));
    }
}
