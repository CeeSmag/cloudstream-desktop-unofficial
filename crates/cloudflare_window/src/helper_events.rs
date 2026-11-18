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
    /// A page told its browser's `User-Agent` (JSON-encoded).
    Agent { host: String, raw: String },
    /// A page answered whether fetch `id` is done (JSON-encoded).
    Polled { id: String, raw: String },
}
