//! Background watch of the sidecar Cloudflare exchange folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::exchange::{pending_requests, write_result, Outcome, VerifyRequest};

/// Something the UI should show about a Cloudflare challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudflareEvent {
    /// A site needs a human check; `url` is the page to open.
    Needed(VerifyRequest),
}

/// Stops the background watcher when dropped (or when [`WatchHandle::stop`] is called).
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    /// Events for the UI to drain each frame.
    pub events: Receiver<CloudflareEvent>,
}

impl WatchHandle {
    /// Asks the watcher thread to exit.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Starts watching `dir` for new verification requests.
///
/// When `auto_open` is true, each new request opens in the system browser and is answered as
/// [`Outcome::Waiting`] so the sidecar can retry after the viewer finishes (cookie return is done
/// by the UI via [`crate::write_result`]).
pub fn spawn_watcher(dir: PathBuf, auto_open: bool) -> WatchHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let stop_flag = Arc::clone(&stop);
    let join = thread::spawn(move || run_loop(dir, auto_open, stop_flag, tx));
    WatchHandle {
        stop,
        join: Some(join),
        events: rx,
    }
}

fn run_loop(dir: PathBuf, auto_open: bool, stop: Arc<AtomicBool>, tx: Sender<CloudflareEvent>) {
    let _ = std::fs::create_dir_all(&dir);
    let mut seen = HashSet::new();
    while !stop.load(Ordering::SeqCst) {
        for request in pending_requests(&dir, &mut seen) {
            let _ = tx.send(CloudflareEvent::Needed(request.clone()));
            if auto_open {
                open_and_wait(&dir, &request);
            }
        }
        thread::sleep(Duration::from_millis(400));
    }
}

fn open_and_wait(dir: &Path, request: &VerifyRequest) {
    tracing::info!(host = %request.host, "opening Cloudflare challenge in the system browser");
    if let Err(error) = open::that(&request.url) {
        tracing::warn!(%error, url = %request.url, "could not open the system browser");
    }
    let reason = "opened in the system browser; confirm in the app when the check is done";
    if let Err(error) = write_result(dir, &request.id, &Outcome::Waiting(reason.to_owned())) {
        tracing::warn!(%error, id = %request.id, "could not write Cloudflare result");
    }
}

#[cfg(test)]
mod tests {
    use crate::exchange::{clear_folder, parse_request};

    #[test]
    fn parse_request_accepts_https() {
        let request = parse_request("id=a1\nhost=h.test\nurl=https://h.test/\n");
        assert_eq!(request.map(|r| r.host), Some("h.test".to_owned()));
    }

    #[test]
    fn clear_folder_removes_files() {
        let dir = std::env::temp_dir().join(format!("cs_cf_watch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("request-x.txt"), "id=x\nhost=h\nurl=https://h/\n").expect("w");
        clear_folder(&dir);
        assert!(std::fs::read_dir(&dir).expect("rd").next().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
