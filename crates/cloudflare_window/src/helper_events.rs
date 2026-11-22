//! What wakes the helper's event loop besides its windows.

use cloudflare_bridge::{FetchRequest, HelperCommand};

/// Sent to the event loop from the input threads and from page script callbacks.
#[derive(Debug)]
pub(crate) enum HelperEvent {
    /// A command from the app.
    Command(HelperCommand),
    /// The app closed the helper's stdin (it quit or crashed): close everything.
    InputClosed,
    /// The sidecar wants a request made in a site's window.
    Fetch(FetchRequest),
    /// A native browser event owned by one verification request.
    Browser { id: String },
    /// A page answered whether fetch `id` is done (JSON-encoded).
    Polled {
        window: String,
        id: String,
        raw: String,
    },
}
