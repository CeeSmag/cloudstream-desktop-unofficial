use std::time::Duration;

/// Time allowed for ordinary provider calls (search, load, links).
pub(crate) const PROVIDER_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Time allowed for resolving the streams of a title: a provider may ask dozens of hosts, and a
/// slow day should not kill the runtime halfway.
pub(crate) const LINK_RESOLVE_TIMEOUT: Duration = Duration::from_secs(150);

/// Time allowed for loading an extension; the first load converts bytecode and is slow.
pub(crate) const EXTENSION_LOAD_TIMEOUT: Duration = Duration::from_secs(180);
