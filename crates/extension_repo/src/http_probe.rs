use std::io::Read;
use std::time::Duration;

use crate::RepoError;

/// How long one probe may take; a host that is slower than this is not a good source anyway.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// How much of the start of the file is read: enough to tell a video or playlist from a web page
/// and to read the track list at the start of a Matroska file. Only this much is read; the rest of the
/// body is never downloaded.
const PROBE_BYTES: u64 = 32 * 1024;
/// The range libmpv asks for on every request. The probe asks for exactly this, not for a small bounded
/// range: some hosts serve a bounded range and refuse an open-ended one (403), so a bounded probe would
/// call a source "working" that the player then cannot open.
const PLAYER_RANGE: &str = "bytes=0-";
/// Statuses with which hosts refuse a `Range` request (same set as `stream_proxy::origin_fetch`).
const RANGE_REFUSED: &[u16] = &[400, 403, 405, 416];

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
        let first = self.probe_once(url, headers, true)?;
        if RANGE_REFUSED.contains(&first.status) {
            tracing::debug!(
                status = first.status,
                "the host refused the range; probing without Range"
            );
            return self.probe_once(url, headers, false);
        }
        Ok(first)
    }
}

impl UreqProbe {
    fn probe_once(
        &self,
        url: &str,
        headers: &[(String, String)],
        with_range: bool,
    ) -> Result<ProbeResponse, RepoError> {
        let fail = |reason: String| RepoError::Download {
            url: url.to_owned(),
            reason,
        };
        let mut request = self.agent.get(url);
        if with_range {
            request = request.header("Range", PLAYER_RANGE);
        }
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
    use std::io::{Read, Write as _};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

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

    /// Serves responses in order for Range-refuse then retry.
    fn serve_scripted(replies: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let queue = Arc::new(Mutex::new(replies));
        std::thread::spawn(move || loop {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer);
            let next = {
                let mut q = queue.lock().expect("lock");
                if q.is_empty() {
                    break;
                }
                q.remove(0)
            };
            let _ = stream.write_all(next.as_bytes());
        });
        format!("http://127.0.0.1:{port}/video.mp4")
    }

    /// Serves `reply` to every request and records the request head of the last one.
    fn serve_recording(reply: &'static str) -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let seen = Arc::new(Mutex::new(String::new()));
        let record = Arc::clone(&seen);
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 2048];
                let read = stream.read(&mut buffer).unwrap_or(0);
                *record.lock().expect("lock") =
                    String::from_utf8_lossy(&buffer[..read]).into_owned();
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}/video.mp4"), seen)
    }

    /// A host like the gdflix workers: a bounded range is served, anything else is refused with 403.
    fn serve_bounded_ranges_only() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 2048];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let head = String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
                let bounded = head
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .is_some_and(|range| {
                        range
                            .trim()
                            .split('-')
                            .nth(1)
                            .is_some_and(|end| !end.is_empty())
                    });
                let reply = if bounded {
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-3/100\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd"
                } else {
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}/video.mp4")
    }

    #[test]
    fn the_probe_asks_for_the_start_exactly_as_the_player_does() {
        let (url, seen) = serve_recording(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-3/100\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd",
        );
        UreqProbe::default().probe(&url, &[]).expect("answers");
        let head = seen.lock().expect("lock").to_ascii_lowercase();
        assert!(
            head.contains("range: bytes=0-\r\n"),
            "libmpv's open-ended range: {head}"
        );
    }

    #[test]
    fn a_host_that_serves_only_bounded_ranges_is_not_reported_as_working() {
        let url = serve_bounded_ranges_only();
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(
            response.status, 403,
            "the player's request is refused, so is the probe's"
        );
    }

    #[test]
    fn only_the_start_of_a_long_body_is_read() {
        // The host would send far more than the probe wants; the answer says so and the probe stops early.
        let body: &'static str = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                PROBE_BYTES * 4,
                "x".repeat(usize::try_from(PROBE_BYTES * 4).expect("fits"))
            )
            .into_boxed_str(),
        );
        let url = serve_once(body);
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.head.len() as u64, PROBE_BYTES);
        assert_eq!(response.total_bytes, Some(PROBE_BYTES * 4));
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
        // 404 is not a Range-refuse code, so there is no second request.
        let url =
            serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.status, 404);
    }

    #[test]
    fn range_refuse_retries_without_range_and_keeps_the_second_answer() {
        let url = serve_scripted(vec![
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd",
        ]);
        let response = UreqProbe::default().probe(&url, &[]).expect("answers");
        assert_eq!(response.status, 200);
        assert_eq!(response.head, b"abcd");
    }

    #[test]
    fn an_unreachable_host_is_an_error() {
        let result = UreqProbe::default().probe("http://127.0.0.1:1/none", &[]);
        assert!(result.is_err());
    }
}
