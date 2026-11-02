use std::path::PathBuf;

/// Why an extension file could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionLoadError {
    /// The extension file does not exist or could not be read.
    #[error("cannot read extension file {path}: {source}")]
    Unreadable {
        /// File that failed to read.
        path: PathBuf,
        /// Underlying I/O failure.
        source: std::io::Error,
    },
    /// The extension manifest is missing or malformed.
    #[error("invalid manifest in extension {path}: {reason}")]
    InvalidManifest {
        /// File whose manifest is invalid.
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_manifest_message_names_path_and_reason() {
        let error = ExtensionLoadError::InvalidManifest {
            path: PathBuf::from("a.cs3"),
            reason: "missing name".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "invalid manifest in extension a.cs3: missing name"
        );
    }
}
