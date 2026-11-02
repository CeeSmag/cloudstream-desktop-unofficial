/// Why a call to the extension runtime failed.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum GatewayError {
    /// The extension runtime could not be started or is not configured.
    #[error("extension runtime is not available: {0}")]
    Unavailable(String),
    /// The runtime did not answer in time.
    #[error("extension call '{method}' timed out after {seconds}s")]
    Timeout {
        /// Name of the call that timed out.
        method: String,
        /// Time limit that was exceeded, in seconds.
        seconds: u64,
    },
    /// Communication with the runtime broke (crash, closed pipe, write error).
    #[error("extension runtime transport failed: {0}")]
    Transport(String),
    /// The extension or runtime reported a failure for this call.
    #[error("extension reported {kind}: {message}")]
    Remote {
        /// Error class reported by the runtime (for example `NotImplementedError`).
        kind: String,
        /// Human-readable detail.
        message: String,
    },
    /// The reply did not have the expected shape.
    #[error("cannot decode reply of '{method}': {reason}")]
    Decode {
        /// Name of the call whose reply was malformed.
        method: String,
        /// Why decoding failed.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_message_names_method_and_limit() {
        let error = GatewayError::Timeout {
            method: "search".to_owned(),
            seconds: 30,
        };
        assert_eq!(
            error.to_string(),
            "extension call 'search' timed out after 30s"
        );
    }

    #[test]
    fn remote_message_includes_kind_and_detail() {
        let error = GatewayError::Remote {
            kind: "NotImplementedError".to_owned(),
            message: "nope".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "extension reported NotImplementedError: nope"
        );
    }
}
