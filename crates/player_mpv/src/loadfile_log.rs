//! The log line written when a stream is handed to mpv.

use core_model::redact_url;
use player_api::{describe_languages, PlaybackRequest};

/// Logs the stream mpv is about to open, as `host#fingerprint`: stream addresses carry tokens and
/// signatures (`X-Amz-Signature=...`) that must never reach a log file.
pub(crate) fn log_loadfile(request: &PlaybackRequest) {
    tracing::info!(
        stream = %redact_url(&request.url),
        subtitle_files = request.subtitles.len(),
        subtitle_languages = %describe_languages(&request.subtitles),
        "mpv loadfile"
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Collects what a test's log lines would have written.
    #[derive(Clone, Default)]
    struct CapturedLog(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturedLog {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("log buffer").extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn logged_by(run: impl FnOnce()) -> String {
        let log = CapturedLog::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, run);
        let bytes = log.0.lock().expect("log buffer").clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn log_loadfile_signed_stream_address_logs_only_host_and_fingerprint() {
        let url = "https://bucket.r2.cloudflarestorage.com/hub/d138?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Signature=deadbeef42";
        let request = PlaybackRequest {
            url: url.to_owned(),
            ..PlaybackRequest::default()
        };
        let logged = logged_by(|| log_loadfile(&request));
        assert!(logged.contains("mpv loadfile"), "{logged}");
        assert!(logged.contains(&redact_url(url)), "{logged}");
        for secret in ["X-Amz-Signature", "deadbeef42", "/hub/d138"] {
            assert!(!logged.contains(secret), "the log leaks {secret}: {logged}");
        }
    }
}
