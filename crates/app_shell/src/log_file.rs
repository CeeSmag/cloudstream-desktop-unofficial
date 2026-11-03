//! One text log file that is opened when debug logging is on and closed when it is off.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// A file that has grown past this many bytes is moved to `<name>.old` and started afresh.
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// A log file shared between the layer that writes it and the controls that open and close it.
pub type SharedLogFile = Arc<Mutex<LogFile>>;

/// A text file in the debug log folder.
pub struct LogFile {
    path: PathBuf,
    open: Option<File>,
    written: u64,
}

impl LogFile {
    /// A closed file; nothing is created until [`LogFile::open`].
    pub fn shared(path: PathBuf) -> SharedLogFile {
        Arc::new(Mutex::new(Self {
            path,
            open: None,
            written: 0,
        }))
    }

    /// Opens the file for appending and marks where this logging session begins.
    pub fn open(&mut self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.written = file.metadata()?.len();
        let marker = "---- debug logging started ----\n";
        file.write_all(marker.as_bytes())?;
        self.written += marker.len() as u64;
        self.open = Some(file);
        Ok(())
    }

    /// Stops writing; the file stays on disk.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Appends text; does nothing while the file is closed.
    pub fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.written >= MAX_FILE_BYTES {
            self.start_afresh()?;
        }
        if let Some(file) = self.open.as_mut() {
            file.write_all(bytes)?;
            self.written += bytes.len() as u64;
        }
        Ok(())
    }

    /// Size on disk, or `None` when the file does not exist.
    pub fn size(&self) -> Option<u64> {
        fs::metadata(&self.path).ok().map(|meta| meta.len())
    }

    fn start_afresh(&mut self) -> io::Result<()> {
        if self.open.is_none() {
            return Ok(());
        }
        let mut old = self.path.clone().into_os_string();
        old.push(".old");
        self.open = None;
        fs::rename(&self.path, PathBuf::from(old))?;
        self.open = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?,
        );
        self.written = 0;
        Ok(())
    }
}

/// Locks a shared file, ignoring a poisoned lock: logging must never take the app down.
pub fn lock(file: &SharedLogFile) -> MutexGuard<'_, LogFile> {
    file.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the logging layers write into: forwards every write to a shared file.
pub struct RoutedWriter(pub SharedLogFile);

impl Write for RoutedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        lock(&self.0).append(buffer)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("cs_log_file_{label}_{}", std::process::id()))
            .join("test-logs.txt")
    }

    #[test]
    fn a_closed_file_ignores_writes_and_creates_nothing() {
        let path = temp_path("closed");
        let file = LogFile::shared(path.clone());
        lock(&file).append(b"hello\n").expect("ignored");
        assert!(!path.exists());
    }

    #[test]
    fn an_open_file_appends_after_a_session_marker() {
        let path = temp_path("open");
        let file = LogFile::shared(path.clone());
        lock(&file).open().expect("opens");
        lock(&file).append(b"first line\n").expect("writes");
        lock(&file).close();
        lock(&file).append(b"dropped\n").expect("ignored again");
        let text = fs::read_to_string(&path).expect("readable");
        assert!(text.contains("debug logging started"));
        assert!(text.contains("first line"));
        assert!(!text.contains("dropped"));
        let _ = fs::remove_dir_all(path.parent().expect("has parent"));
    }
}
