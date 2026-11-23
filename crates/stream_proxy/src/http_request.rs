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

/// The request in `buffer` once its head is complete; `None` while it is not. A head longer than
/// [`MAX_HEAD_BYTES`] is refused even when it ended in the read that crossed the limit.
fn finished_head(buffer: &[u8]) -> Option<Result<HttpRequest, RequestError>> {
    let end = find_head_end(buffer)?;
    if end + HEAD_END.len() > MAX_HEAD_BYTES {
        return Some(Err(RequestError::TooLarge));
    }
    Some(parse_head(buffer.get(..end).unwrap_or_default()))
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
        if let Some(request) = finished_head(&buffer) {
            return request;
        }
        if buffer.len() >= MAX_HEAD_BYTES {
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

    /// Hands out `data` in pieces of `piece` bytes, after the scripted failures.
    struct Pieces {
        failures: Vec<ErrorKind>,
        data: Vec<u8>,
        piece: usize,
        endless: bool,
    }

    impl Read for Pieces {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if let Some(kind) = self.failures.pop() {
                return Err(kind.into());
            }
            if self.endless {
                let count = self.piece.min(out.len());
                out[..count].fill(b'a');
                return Ok(count);
            }
            let count = self.piece.min(out.len()).min(self.data.len());
            out[..count].copy_from_slice(&self.data[..count]);
            self.data.drain(..count);
            Ok(count)
        }
    }

    /// A request whose head, blank line included, is exactly `length` bytes.
    fn head_of(length: usize) -> Vec<u8> {
        let start = b"GET /a HTTP/1.1\r\nX-Pad: ";
        let end = b"\r\n\r\n";
        let mut head = start.to_vec();
        head.resize(length - end.len(), b'p');
        head.extend_from_slice(end);
        head
    }

    fn pieces(data: Vec<u8>) -> Pieces {
        Pieces {
            failures: Vec::new(),
            data,
            piece: 1000,
            endless: false,
        }
    }

    #[test]
    fn read_request_a_head_of_exactly_the_limit_is_read() {
        let mut reader = pieces(head_of(MAX_HEAD_BYTES));
        let request = read_request(&mut reader, later()).expect("a request");
        assert_eq!(request.target, "/a");
    }

    #[test]
    fn read_request_a_head_one_byte_over_the_limit_is_too_large() {
        let mut reader = pieces(head_of(MAX_HEAD_BYTES + 1));
        assert!(matches!(
            read_request(&mut reader, later()),
            Err(RequestError::TooLarge)
        ));
    }

    #[test]
    fn read_request_a_long_real_head_is_read() {
        // Cookies and a signed referer can make a head a few kilobytes long.
        let mut reader = pieces(head_of(8 * 1024));
        assert!(read_request(&mut reader, later()).is_ok());
    }

    #[test]
    fn read_request_a_head_that_never_ends_in_odd_pieces_is_too_large() {
        let mut reader = Pieces {
            endless: true,
            ..pieces(Vec::new())
        };
        assert!(matches!(
            read_request(&mut reader, later()),
            Err(RequestError::TooLarge)
        ));
    }

    #[test]
    fn read_request_waits_through_a_slow_or_interrupted_socket() {
        for kind in [
            ErrorKind::WouldBlock,
            ErrorKind::TimedOut,
            ErrorKind::Interrupted,
        ] {
            let mut reader = Pieces {
                failures: vec![kind, kind],
                ..pieces(head_of(64))
            };
            let request = read_request(&mut reader, later());
            assert!(request.is_ok(), "{kind:?}: {request:?}");
        }
    }

    #[test]
    fn read_request_a_broken_connection_is_reported_at_once() {
        let mut reader = Pieces {
            failures: vec![ErrorKind::ConnectionReset],
            ..pieces(head_of(64))
        };
        assert!(matches!(
            read_request(&mut reader, later()),
            Err(RequestError::Io(_))
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
