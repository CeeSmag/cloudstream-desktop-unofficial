//! Captures what a piece of code logs, so tests can prove that a secret never reaches a log line.
//!
//! One global subscriber (installed once for the test binary) is always interested in every event and writes
//! each line into a buffer of the thread that logged it, while that thread captures. A scoped subscriber per
//! test is not used: `tracing` caches per call site whether anyone listens, and a test running in parallel
//! can leave a call site cached as "nobody listens" while another test's scoped subscriber is being set up,
//! so a capture would miss lines.

use std::cell::RefCell;
use std::io;
use std::sync::Once;

use tracing_subscriber::fmt::MakeWriter;

thread_local! {
    static CAPTURE: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// Writes into the logging thread's capture buffer, if it captures.
struct ThreadBuffer;

impl io::Write for ThreadBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        CAPTURE.with(|capture| {
            if let Some(buffer) = capture.borrow_mut().as_mut() {
                buffer.extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ThreadWriters;

impl<'a> MakeWriter<'a> for ThreadWriters {
    type Writer = ThreadBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        ThreadBuffer
    }
}

fn install_once() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(ThreadWriters)
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish();
        // Another test binary component may have set one already; then lines of this crate still reach it
        // and `captured` returns what this thread wrote.
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

/// Runs `work` and returns, with its result, every line this thread logged meanwhile (every level).
pub(crate) fn captured<T>(work: impl FnOnce() -> T) -> (T, String) {
    install_once();
    CAPTURE.with(|capture| *capture.borrow_mut() = Some(Vec::new()));
    let result = work();
    let bytes = CAPTURE.with(|capture| capture.borrow_mut().take().unwrap_or_default());
    (result, String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_holds_this_thread_s_lines_only() {
        let ((), mine) = captured(|| {
            tracing::info!("from the capturing thread");
            std::thread::spawn(|| tracing::info!("from another thread"))
                .join()
                .expect("thread");
        });
        assert!(mine.contains("from the capturing thread"), "{mine}");
        assert!(!mine.contains("from another thread"), "{mine}");
        let ((), next) = captured(|| ());
        assert!(next.is_empty(), "each capture starts empty: {next}");
    }
}
