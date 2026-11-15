//! Reads the one request the player sends on a connection.

use std::io::{ErrorKind, Read};
use std::time::Instant;

/// The head of a request may not be longer than this; the player's are a few hundred bytes.
pub(crate) const MAX_HEAD_BYTES: usize = 16 * 1024;

/// The end of a request's head.
const HEAD_END: &[u8] = b"\r\n\r\n";

/// A request: what was asked, of what, and the headers. A body is never read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) headers: Vec<(String, String)>,
}

impl HttpRequest {
    /// The value of a request header, whatever its case.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Why no request could be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RequestError {
    /// The head is longer than [`MAX_HEAD_BYTES`].
    #[error("the request head is too large")]
    TooLarge,
    /// The head is not an HTTP/1 request.
    #[error("the request is malformed")]
    Malformed,
    /// The client closed the connection or was too slow.
    #[error("the request did not arrive in time")]
    Incomplete,
    /// The connection failed.
    #[error("the connection failed")]
    Io(#[source] std::io::Error),
}

fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

/// Parses a request head (everything before the blank line).
pub(crate) fn parse_head(head: &[u8]) -> Result<HttpRequest, RequestError> {
    let text = std::str::from_utf8(head).map_err(|_| RequestError::Malformed)?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or(RequestError::Malformed)?.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (first.next(), first.next(), first.next(), first.next())
    else {
        return Err(RequestError::Malformed);
    };
    if !is_token(method) || !target.starts_with('/') || !version.starts_with("HTTP/1.") {
        return Err(RequestError::Malformed);
    }
    let headers = lines
        .map(|line| {
            let (name, value) = line.split_once(':').ok_or(RequestError::Malformed)?;
            is_token(name)
                .then(|| (name.to_owned(), value.trim().to_owned()))
                .ok_or(RequestError::Malformed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HttpRequest {
        method: method.to_owned(),
        target: target.to_owned(),
        headers,
    })
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(HEAD_END.len())
        .position(|window| window == HEAD_END)
}

/// Reads until the head is complete. The reader should time out on its own after a short while
/// (the socket's read timeout); `deadline` bounds the whole read, so that a client sending a byte
/// at a time cannot hold a connection for ever.
pub(crate) fn read_request(
    reader: &mut impl Read,
    deadline: Instant,
) -> Result<HttpRequest, RequestError> {
    let mut buffer: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = find_head_end(&buffer) {
            return parse_head(buffer.get(..end).unwrap_or_default());
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(RequestError::TooLarge);
        }
        if Instant::now() >= deadline {
            return Err(RequestError::Incomplete);
        }
        match reader.read(&mut chunk) {
            Ok(0) => return Err(RequestError::Incomplete),
            Ok(count) => buffer.extend_from_slice(chunk.get(..count).unwrap_or_default()),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(RequestError::Io(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::Duration;

    use super::*;

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn parse_head_a_player_request_returns_method_target_and_headers() {
        let head = b"GET /s/ab/3.ts HTTP/1.1\r\nHost: 127.0.0.1:5000\r\nRange: bytes=0-99\r\nUser-Agent: Lavf/61";
        let request = parse_head(head).expect("a request");
        assert_eq!(
            (request.method.as_str(), request.target.as_str()),
            ("GET", "/s/ab/3.ts")
        );
        assert_eq!(request.header("range"), Some("bytes=0-99"));
        assert_eq!(request.header("HOST"), Some("127.0.0.1:5000"));
        assert_eq!(request.header("missing"), None);
    }

    #[test]
    fn parse_head_rejects_what_is_not_an_http_1_request() {
        for head in [
            "",
            "GET",
            "GET /x",
            "GET /x HTTP/2",
            "GET x HTTP/1.1",
            "GET http://other/x HTTP/1.1",
            "GET /x HTTP/1.1 extra",
            "G ET /x HTTP/1.1",
            "GET /x HTTP/1.1\r\nno colon here",
            "GET /x HTTP/1.1\r\nBad Name: x",
        ] {
            assert!(parse_head(head.as_bytes()).is_err(), "{head:?}");
        }
    }

    #[test]
    fn read_request_reads_a_head_that_arrives_in_pieces() {
        struct Trickle(Vec<&'static [u8]>);
        impl Read for Trickle {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let Some(piece) = self.0.pop() else {
                    return Ok(0);
                };
                out[..piece.len()].copy_from_slice(piece);
                Ok(piece.len())
            }
        }
        // Popped from the end: the request line first, the blank line last.
        let mut reader = Trickle(vec![
            &b"\r\n\r\n"[..],
            &b"Host: x"[..],
            &b"GET /a HTTP/1.1\r\n"[..],
        ]);
        let request = read_request(&mut reader, later()).expect("a request");
        assert_eq!(request.target, "/a");
        assert_eq!(request.header("host"), Some("x"));
    }

    #[test]
    fn read_request_ignores_what_follows_the_head() {
        let mut reader = Cursor::new(b"GET /a HTTP/1.1\r\nHost: x\r\n\r\nBODY".to_vec());
        assert_eq!(
            read_request(&mut reader, later())
                .expect("a request")
                .headers
                .len(),
            1
        );
    }

    #[test]
    fn read_request_a_client_that_closes_early_is_incomplete() {
        let mut reader = Cursor::new(b"GET /a HTTP/1.1\r\nHost".to_vec());
        assert!(matches!(
            read_request(&mut reader, later()),
            Err(RequestError::Incomplete)
        ));
    }

    #[test]
    fn read_request_a_head_that_never_ends_is_too_large() {
        let mut reader = Cursor::new(vec![b'a'; MAX_HEAD_BYTES * 2]);
        assert!(matches!(
            read_request(&mut reader, later()),
            Err(RequestError::TooLarge)
        ));
    }

    #[test]
    fn read_request_past_the_deadline_gives_up_even_when_bytes_keep_coming() {
        struct Endless;
        impl Read for Endless {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                out[0] = b'a';
                Ok(1)
            }
        }
        let expired = Instant::now();
        assert!(matches!(
            read_request(&mut Endless, expired),
            Err(RequestError::Incomplete)
        ));
    }
}
