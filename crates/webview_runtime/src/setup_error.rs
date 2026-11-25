//! Why installing the WebView2 runtime did not work, in words the viewer can read.

use std::path::Path;

/// One reason the WebView2 setup did not install the runtime.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    /// The setup could not be downloaded (no connection, an HTTP error, a broken transfer).
    #[error("could not download the WebView2 setup: {0}")]
    Download(String),
    /// The download is bigger than any real setup.
    #[error("the WebView2 setup is larger than {limit} bytes")]
    TooLarge {
        /// The most bytes a setup may have.
        limit: u64,
    },
    /// The download was offered from (or redirected to) a host that is not Microsoft's.
    #[error("the WebView2 setup was offered from {0}, which is not a Microsoft site")]
    WrongHost(String),
    /// The downloaded file is not signed by Microsoft.
    #[error("the downloaded setup is not signed by Microsoft ({0})")]
    NotMicrosoft(String),
    /// The viewer stopped the install before the setup ran.
    #[error("the WebView2 install was cancelled")]
    Cancelled,
    /// The setup ran and reported a failure.
    #[error("the WebView2 setup stopped with code {0}")]
    InstallerFailed(i32),
    /// The setup did not finish in time.
    #[error("the WebView2 setup did not finish in time")]
    TimedOut,
    /// The setup ran, but the runtime is still not there.
    #[error("the WebView2 setup finished, but the runtime is still not installed")]
    StillMissing,
    /// A file or folder of the download could not be written, moved or started.
    #[error("could not {action} {path}: {reason}")]
    Disk {
        /// What was being done.
        action: &'static str,
        /// The file or folder.
        path: String,
        /// The system's answer.
        reason: String,
    },
}

impl SetupError {
    /// A disk error for `path`.
    pub fn disk(action: &'static str, path: &Path, error: &std::io::Error) -> Self {
        Self::Disk {
            action,
            path: path.display().to_string(),
            reason: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_errors_name_the_action_the_path_and_the_reason() {
        let error = SetupError::disk(
            "create",
            Path::new("setup-folder"),
            &std::io::Error::other("denied"),
        );
        assert_eq!(error.to_string(), "could not create setup-folder: denied");
    }

    #[test]
    fn every_reason_reads_as_a_sentence() {
        let all = [
            SetupError::Download("timed out".into()),
            SetupError::TooLarge { limit: 7 },
            SetupError::WrongHost("example.org".into()),
            SetupError::NotMicrosoft("NotSigned".into()),
            SetupError::Cancelled,
            SetupError::InstallerFailed(3),
            SetupError::TimedOut,
            SetupError::StillMissing,
        ];
        let texts: Vec<String> = all.iter().map(ToString::to_string).collect();
        assert!(texts
            .iter()
            .all(|text| text.contains("WebView2") || text.contains("Microsoft")));
        assert!(texts.iter().any(|text| text.contains("timed out")));
        assert!(texts.iter().any(|text| text.ends_with("than 7 bytes")));
        assert!(texts.iter().any(|text| text.contains("example.org")));
        assert!(texts.iter().any(|text| text.contains("(NotSigned)")));
        assert!(texts.iter().any(|text| text.ends_with("code 3")));
    }
}
