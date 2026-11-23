//! Fetching one TMDB Discover page for the Browse screen.

use std::io::Read;
use std::time::Duration;

use crate::browse_genres::{TmdbPageResponse, TMDB_API_KEY};

/// The whole request may take this long.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// Connecting to TMDB may take this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// A Discover page is a few KB; anything far bigger is not one.
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const USER_AGENT: &str = "CloudStream-rs";

/// Fetches a single TMDB Discover page.
pub trait BrowseFetcher: Send + Sync {
    /// Sends a GET request to `endpoint` with `params` and parses the JSON page response.
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String>;
}

/// Connection-pooled `ureq` implementation of [`BrowseFetcher`].
pub struct UreqBrowseFetcher {
    agent: ureq::Agent,
}

impl Default for UreqBrowseFetcher {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }
}

/// `message` with the TMDB key taken out, so errors can be shown and logged.
fn redact_key(message: &str) -> String {
    message.replace(TMDB_API_KEY, "[REDACTED]")
}

impl BrowseFetcher for UreqBrowseFetcher {
    fn fetch_page(
        &self,
        endpoint: &str,
        params: &[(String, String)],
    ) -> Result<TmdbPageResponse, String> {
        let mut request = self.agent.get(endpoint).header("User-Agent", USER_AGENT);
        for (name, value) in params {
            request = request.query(name, value);
        }
        let response = request
            .call()
            .map_err(|error| format!("HTTP request failed: {}", redact_key(&error.to_string())))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("TMDB returned HTTP {status}"));
        }
        let mut body = String::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| format!("Reading response body failed: {error}"))?;
        serde_json::from_str(&body).map_err(|error| format!("JSON decode failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    use super::*;

    /// Answers one request on a local socket; hands back the request's head when joined.
    fn serve_once(status: &'static str, body: String) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://{}/discover/movie",
            listener.local_addr().expect("addr")
        );
        let worker = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a request");
            let mut reader = BufReader::new(stream);
            let (mut head, mut line) = (String::new(), String::new());
            while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                head.push_str(&line);
                line.clear();
            }
            let answer = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = reader.get_mut().write_all(answer.as_bytes());
            head
        });
        (url, worker)
    }

    fn params() -> Vec<(String, String)> {
        vec![
            ("page".to_owned(), "2".to_owned()),
            ("with_genres".to_owned(), "28,12".to_owned()),
        ]
    }

    #[test]
    fn a_page_is_read_and_the_query_and_app_name_are_sent() {
        let page =
            r#"{"results":[{"id":7,"title":"Heat"},{"id":8,"name":"Lost"}],"total_pages":4}"#;
        let (url, server) = serve_once("200 OK", page.to_owned());
        let response = UreqBrowseFetcher::default()
            .fetch_page(&url, &params())
            .expect("a page");
        assert_eq!(response.total_pages, Some(4));
        assert_eq!(response.results.map(|items| items.len()), Some(2));
        let head = server.join().expect("server");
        let request_line = head.lines().next().unwrap_or_default();
        assert!(request_line.contains("page=2"), "{request_line}");
        assert!(
            request_line.contains("with_genres=28%2C12"),
            "{request_line}"
        );
        assert!(
            head.to_ascii_lowercase()
                .contains("user-agent: cloudstream-rs"),
            "{head}"
        );
    }

    #[test]
    fn a_page_of_a_megabyte_and_a_half_is_read_whole() {
        // Far bigger than TMDB sends, well within the limit: nothing may be cut off it.
        let padding = "x".repeat(1_536 * 1024);
        let page = format!(r#"{{"results":[{{"id":1,"title":"{padding}"}}],"total_pages":1}}"#);
        let (url, server) = serve_once("200 OK", page);
        let response = UreqBrowseFetcher::default()
            .fetch_page(&url, &params())
            .expect("a page");
        assert_eq!(response.total_pages, Some(1));
        let _ = server.join();
    }

    #[test]
    fn an_error_status_is_reported_with_its_code() {
        let (url, server) = serve_once("429 Too Many Requests", "slow down".to_owned());
        let error = UreqBrowseFetcher::default()
            .fetch_page(&url, &params())
            .expect_err("refused");
        assert_eq!(error, "TMDB returned HTTP 429");
        let _ = server.join();
    }

    #[test]
    fn an_answer_that_is_not_a_page_is_a_decode_error() {
        let (url, server) = serve_once("200 OK", "<html>maintenance</html>".to_owned());
        let error = UreqBrowseFetcher::default()
            .fetch_page(&url, &params())
            .expect_err("not json");
        assert!(error.starts_with("JSON decode failed"), "{error}");
        let _ = server.join();
    }

    #[test]
    fn a_failed_connection_never_shows_the_api_key() {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("free port")
            .port();
        let keyed = vec![("api_key".to_owned(), TMDB_API_KEY.to_owned())];
        let error = UreqBrowseFetcher::default()
            .fetch_page(&format!("http://127.0.0.1:{port}/discover/movie"), &keyed)
            .expect_err("nothing listens");
        assert!(error.starts_with("HTTP request failed"), "{error}");
        assert!(!error.contains(TMDB_API_KEY), "{error}");
    }

    #[test]
    fn redact_key_removes_the_tmdb_key_from_an_error() {
        let redacted = redact_key(&format!("GET https://x/?api_key={TMDB_API_KEY} failed"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains(TMDB_API_KEY));
    }
}
