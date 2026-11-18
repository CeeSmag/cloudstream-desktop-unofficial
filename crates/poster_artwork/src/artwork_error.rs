//! Why a poster could not be shown.

/// What went wrong loading one poster.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtworkError {
    /// The address is not a web address (for example `data:`).
    #[error("unsupported poster address ({0})")]
    Unsupported(String),
    /// The host answered with an error status.
    #[error("HTTP {0}")]
    Status(u16),
    /// The host could not be reached, was too slow or stopped answering.
    #[error("network: {0}")]
    Network(String),
    /// The download was bigger than any poster should be.
    #[error("the image is larger than {0} bytes")]
    TooLarge(u64),
    /// The bytes are not an image this build can read.
    #[error("not a readable image: {0}")]
    Decode(String),
    /// The loader was shut down before this poster was loaded.
    #[error("the poster loader stopped")]
    ShutDown,
}
