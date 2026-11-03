use std::io::Read;
use std::time::Duration;

use crate::RepoError;

/// How long one probe may take; a host that is slower than this is not a good source anyway.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// How much of the start of the file is read (enough to tell a video or playlist from a web page).
const PROBE_BYTES: u64 = 1024;

/// What a host answered when asked for the first bytes of a stream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProbeResponse {
    /// HTTP status code.
    pub status: u16,
    /// `Content-Type` header, empty when absent.
    pub content_type: String,
    /// The first bytes of the body.
    pub head: Vec<u8>,
    /// Size of the whole file in bytes, when the host said.
    pub total_bytes: Option<u64>,
}

/// Asks a server for the start of a file without downloading it.
pub trait HttpProbe: Send + Sync {
    /// Requests the first bytes of `url` with the given extra headers.
    fn probe(&self, url: &str, headers: &[(String, String)]) -> Result<ProbeResponse, RepoError>;
}

/// `HttpProbe` backed by `ureq`; error statuses are returned, not raised.
pub struct UreqProbe {
    agent: ureq::Agent,
}

impl Default for UreqProbe {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(PROBE_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl HttpProbe for UreqProbe {
    #[tracing::instrument(level = "debug", skip_all, fields(url = ?url))]
    fn probe(&self, url: &str, headers: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        let fail = |reason: String| RepoError::Download {
            url: url.to_owned(),
            reason,
        };
        let mut request = self.agent.get(url).header("Range", "bytes=0-1023");
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.call().map_err(|error| fail(error.to_string()))?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = header("content-type").unwrap_or_default();
        let total_bytes = total_size(
            status,
            header("content-range").as_deref(),
            header("content-length").as_deref(),
        );
        let mut head = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(PROBE_BYTES)
            .read_to_end(&mut head)
            .map_err(|error| fail(error.to_string()))?;
        Ok(ProbeResponse {
            status,
            content_type,
            head,
            total_bytes,
        })
    }
}

/// The size of the whole file: from `Content-Range: bytes 0-1023/1234` for a partial answer, from
/// `Content-Length` when the host sent the file from the start.
fn total_size(
    status: u16,
    content_range: Option<&str>,
    content_length: Option<&str>,
) -> Option<u64> {
    match status {
        206 => content_range?.rsplit('/').next()?.trim().parse().ok(),
        200 => content_length?.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::TcpListener;

    use super::*;

    /// Serves one canned HTTP response on a local port and returns the URL.
    fn serve_once(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}/video.mp4")
    }

    #[test]
    fn reads_status_type_and_the_start_of_the_body() {
        let url = serve_once(
            "HTTP/1.1 206 Partial Content\r\nContent-Type: video/mp4\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd",
        );
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.status, 206);
        assert_eq!(response.content_type, "video/mp4");
        assert_eq!(response.head, b"abcd");
    }

    #[test]
    fn the_size_of_the_whole_file_comes_from_the_content_range() {
        let url = serve_once(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-3/1500000000\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd",
        );
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.total_bytes, Some(1_500_000_000));
    }

    #[test]
    fn total_size_reads_ranges_and_lengths_and_ignores_the_rest() {
        assert_eq!(total_size(206, Some("bytes 0-1023/5000"), None), Some(5000));
        assert_eq!(total_size(206, Some("bytes 0-1023/*"), None), None);
        assert_eq!(total_size(200, None, Some("42")), Some(42));
        assert_eq!(total_size(404, None, Some("42")), None);
        assert_eq!(total_size(206, None, Some("4")), None);
    }

    #[test]
    fn error_statuses_are_returned_not_raised() {
        let url =
            serve_once("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.status, 403);
    }

    #[test]
    fn an_unreachable_host_is_an_error() {
        let result = UreqProbe::default().probe("http://127.0.0.1:1/none", &[]);
        assert!(result.is_err());
    }
}
