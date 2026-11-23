//! Captures what a piece of code logs, so tests can prove that a secret never reaches a log line.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use tracing_subscriber::fmt::MakeWriter;

/// Everything logged inside [`captured`], every level, as text.
#[derive(Clone, Default)]
pub(crate) struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl LogBuffer {
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

impl io::Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogBuffer {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Runs `work` with every log line of this thread written to the returned buffer.
pub(crate) fn captured<T>(work: impl FnOnce() -> T) -> (T, String) {
    let buffer = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buffer.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let result = tracing::subscriber::with_default(subscriber, work);
    (result, buffer.text())
}
