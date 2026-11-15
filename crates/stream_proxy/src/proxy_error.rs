/// Why the proxy could not do what was asked.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    /// The proxy's listening socket could not be opened or its thread not started.
    #[error("cannot start the stream proxy: {0}")]
    Start(#[source] std::io::Error),
    /// The stream's address is not an `http` or `https` address.
    #[error("the stream proxy only opens http and https streams")]
    UnsupportedAddress,
    /// The proxy has been shut down.
    #[error("the stream proxy has been shut down")]
    ShutDown,
}
