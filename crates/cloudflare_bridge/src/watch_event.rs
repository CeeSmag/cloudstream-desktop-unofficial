//! Events delivered to the Cloudflare banner.
use crate::HelperStatus;

/// Something the UI should show about a site's Cloudflare check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudflareEvent {
    /// The site needs a check and the viewer has not allowed it yet (offer Verify).
    NeedsPermission {
        /// The site.
        host: String,
    },
    /// A verification window is open for the site.
    Verifying {
        /// The request waiting for this check.
        id: String,
        /// The site.
        host: String,
    },
    /// The check was passed.
    Passed {
        /// The site.
        host: String,
    },
    /// The check was not passed.
    Failed {
        /// The site.
        host: String,
        /// Why, for the viewer.
        reason: String,
    },
}

impl From<HelperStatus> for CloudflareEvent {
    fn from(status: HelperStatus) -> Self {
        match status {
            HelperStatus::WindowOpen { host, id } => CloudflareEvent::Verifying { host, id },
            HelperStatus::Passed { host } => CloudflareEvent::Passed { host },
            HelperStatus::Failed { host, reason } => CloudflareEvent::Failed { host, reason },
        }
    }
}
