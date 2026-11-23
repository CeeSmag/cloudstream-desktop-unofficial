//! Passes an answer from the stream's host on to the player, piece by piece.

use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant};

use crate::entry_kind::EntryKind;
use crate::http_response::write_head;
use crate::origin_fetch::OriginResponse;
use crate::stream_session::StreamSession;

/// How much is copied at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// The longest one answer may take to pass on. A piece of a stream is a few seconds of video; one
/// that takes longer than this has stalled, and the player asks again.
const RELAY_DEADLINE: Duration = Duration::from_secs(180);

/// What is being relayed and for whom.
pub(crate) struct RelayJob<'a> {
    pub(crate) session: &'a StreamSession,
    pub(crate) kind: EntryKind,
    /// Answer a `HEAD` request: the head only.
    pub(crate) head_only: bool,
}

/// The headers worth passing on. The content type is the proxy's own for pieces: hosts disguise
/// them as pages or pictures on purpose.
fn relay_headers(response: &OriginResponse, kind: EntryKind) -> Vec<(&'static str, String)> {
    let content_type = match (kind, response.header("content-type")) {
        (EntryKind::Playlist, Some(own)) => own,
        _ => kind.content_type(),
    };
    let mut headers = vec![("Content-Type", content_type.to_owned())];
    for (name, wanted) in [
        ("Content-Length", "content-length"),
        ("Content-Range", "content-range"),
        ("Accept-Ranges", "accept-ranges"),
    ] {
        if let Some(value) = response.header(wanted) {
            headers.push((name, value.to_owned()));
        }
    }
    headers
}

/// Copies `body` to `out` until it ends, the stream is revoked or the deadline passes. A body that
/// breaks off is logged and ends the answer; the player sees a short answer and asks again.
fn copy_body(
    out: &mut impl Write,
    body: &mut dyn Read,
    session: &StreamSession,
) -> std::io::Result<()> {
    let mut chunk = vec![0u8; CHUNK_BYTES];
    let started = Instant::now();
    while !session.is_revoked() && started.elapsed() < RELAY_DEADLINE {
        let count = match body.read(&mut chunk) {
            Ok(0) => return Ok(()),
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                tracing::warn!(%error, "the stream's host stopped sending in the middle of a piece");
                return Ok(());
            }
        };
        out.write_all(chunk.get(..count).unwrap_or_default())?;
    }
    tracing::debug!("a piece was cut short: the stream was closed or the piece took too long");
    Ok(())
}

/// Writes the head of `response` and then its body, preceded by `prefix` (bytes already taken
/// from the body to look at them).
pub(crate) fn relay(
    out: &mut impl Write,
    job: &RelayJob<'_>,
    mut response: OriginResponse,
    prefix: &[u8],
) -> std::io::Result<()> {
    write_head(out, response.status, &relay_headers(&response, job.kind))?;
    if job.head_only {
        return Ok(());
    }
    out.write_all(prefix)?;
    copy_body(out, &mut *response.body, job.session)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Error, ErrorKind};
    use std::sync::Arc;

    use super::*;
    use crate::stream_spec::StreamSpec;

    fn session() -> Arc<StreamSession> {
        let spec = StreamSpec {
            url: "https://h.test/p.m3u8".to_owned(),
            ..StreamSpec::default()
        };
        Arc::new(StreamSession::new("ab".to_owned(), &spec))
    }

    /// A body that never ends and revokes the stream when it is read for the third time.
    struct RevokedOnThirdRead {
        session: Arc<StreamSession>,
        reads: usize,
    }

    impl Read for RevokedOnThirdRead {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            if self.reads == 3 {
                self.session.revoke();
            }
            out[0] = b'x';
            Ok(1)
        }
    }

    #[test]
    fn copy_body_stops_at_the_next_chunk_once_the_stream_is_revoked() {
        let session = session();
        let mut body = RevokedOnThirdRead {
            session: Arc::clone(&session),
            reads: 0,
        };
        let mut sink = Vec::new();
        copy_body(&mut sink, &mut body, &session).expect("a cut answer is not an error");
        assert_eq!(body.reads, 3, "no read after the revoke");
        assert_eq!(sink, b"xxx");
    }

    #[test]
    fn copy_body_a_revoked_stream_sends_nothing() {
        let session = session();
        session.revoke();
        let mut sink = Vec::new();
        copy_body(&mut sink, &mut Cursor::new(b"data".to_vec()), &session).expect("no error");
        assert!(sink.is_empty());
    }

    #[test]
    fn copy_body_a_host_that_breaks_off_ends_the_answer_with_what_came() {
        struct BreaksOff(bool);
        impl Read for BreaksOff {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if std::mem::replace(&mut self.0, true) {
                    return Err(Error::new(ErrorKind::ConnectionReset, "reset"));
                }
                out[..3].copy_from_slice(b"abc");
                Ok(3)
            }
        }
        let mut sink = Vec::new();
        copy_body(&mut sink, &mut BreaksOff(false), &session()).expect("no error");
        assert_eq!(sink, b"abc");
    }

    #[test]
    fn copy_body_a_player_that_went_away_is_an_error_for_the_caller_to_log() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(Error::new(ErrorKind::BrokenPipe, "closed"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let result = copy_body(&mut Closed, &mut Cursor::new(b"data".to_vec()), &session());
        assert_eq!(result.expect_err("fails").kind(), ErrorKind::BrokenPipe);
    }

    #[test]
    fn copy_body_a_read_interrupted_by_a_signal_is_tried_again() {
        /// Sends "first", is interrupted once, then sends "second" and ends.
        struct Interrupted(u8);
        impl Read for Interrupted {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                self.0 += 1;
                let piece: &[u8] = match self.0 {
                    1 => b"first",
                    2 => return Err(Error::from(ErrorKind::Interrupted)),
                    3 => b"second",
                    _ => b"",
                };
                out[..piece.len()].copy_from_slice(piece);
                Ok(piece.len())
            }
        }
        let mut sink = Vec::new();
        copy_body(&mut sink, &mut Interrupted(0), &session()).expect("no error");
        assert_eq!(
            sink, b"firstsecond",
            "an interrupted read is not the end of the piece"
        );
    }
}
