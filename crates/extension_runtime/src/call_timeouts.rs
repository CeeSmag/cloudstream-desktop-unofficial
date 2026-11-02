use std::time::Duration;

/// Time allowed for ordinary provider calls (search, load, links).
pub(crate) const PROVIDER_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Time allowed for loading an extension; the first load converts bytecode and is slow.
pub(crate) const EXTENSION_LOAD_TIMEOUT: Duration = Duration::from_secs(180);
