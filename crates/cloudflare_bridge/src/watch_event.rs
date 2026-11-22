//! Events delivered to the Cloudflare banner.

use crate::HelperStatus;

/// Something the UI should show about a verification request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudflareEvent {
    /// A pending request is waiting for the viewer's permission.
    NeedsPermission {
        /// The site.
        host: String,
        /// The challenged request.
        id: String,
    },
    /// The viewer approved this host on an active request.
    PermissionGranted {
        /// The site.
        host: String,
    },
    /// The helper is processing the request.
    Verifying {
        /// The pending request.
        id: String,
        /// The site.
        host: String,
    },
    /// The request was verified.
    Passed {
        /// The site.
        host: String,
        /// The completed request.
        id: String,
    },
    /// The request was not verified.
    Failed {
        /// The site.
        host: String,
        /// The completed request.
        id: String,
        /// Why, for the viewer.
        reason: String,
        /// Whether the viewer can start a fresh attempt for this challenge.
        retry: bool,
    },
    /// The request expired or was dismissed.
    Dismissed {
        /// The retired request.
        id: String,
    },
}

impl CloudflareEvent {
    /// The request this event is about, when it is about one.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::NeedsPermission { id, .. }
            | Self::Verifying { id, .. }
            | Self::Passed { id, .. }
            | Self::Failed { id, .. }
            | Self::Dismissed { id } => Some(id),
            Self::PermissionGranted { .. } => None,
        }
    }
}

impl From<HelperStatus> for CloudflareEvent {
    fn from(status: HelperStatus) -> Self {
        match status {
            HelperStatus::WindowOpen { host, id } => Self::Verifying { host, id },
            HelperStatus::Passed { host, id } => Self::Passed { host, id },
            HelperStatus::Failed { host, id, reason } => Self::Failed {
                host,
                id,
                reason,
                retry: true,
            },
        }
    }
}
