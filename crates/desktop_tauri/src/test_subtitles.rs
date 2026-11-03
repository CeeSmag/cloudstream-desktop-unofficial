//! A fake subtitle host for tests: serves WebVTT for any address, except `dead` ones (404) and
//! `slow` ones (a pause first), and remembers what it was asked for.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use subtitle_store::{FetchError, LoadOptions, SubtitleCache, SubtitleFetcher, SubtitleLoader};

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// How long a `slow` address takes to answer.
pub(crate) const SLOW: Duration = Duration::from_millis(600);

/// The fake host.
#[derive(Default)]
pub(crate) struct FakeSubtitleHost {
    pub(crate) calls: AtomicUsize,
    pub(crate) asked: Mutex<Vec<String>>,
}

impl SubtitleFetcher for FakeSubtitleHost {
    fn fetch(&self, url: &str, _: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked.lock().expect("lock").push(url.to_owned());
        if url.contains("slow") {
            std::thread::sleep(SLOW);
        }
        if url.contains("dead") {
            return Err(FetchError::Status { status: 404 });
        }
        Ok(format!("WEBVTT\n\n00:01.000 --> 00:02.000\n{url}\n").into_bytes())
    }
}

/// A loader over the fake host, with a cache folder of its own that is removed afterwards.
pub(crate) struct TestSubtitles {
    pub(crate) loader: Arc<SubtitleLoader>,
    pub(crate) host: Arc<FakeSubtitleHost>,
    folder: PathBuf,
}

impl TestSubtitles {
    pub(crate) fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "cs_desktop_subtitles_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        ));
        let host = Arc::new(FakeSubtitleHost::default());
        let cache = SubtitleCache::new(folder.clone(), Duration::from_secs(3600));
        let options = LoadOptions {
            attempts: 2,
            backoff: Duration::from_millis(1),
        };
        let loader = Arc::new(SubtitleLoader::new(
            cache,
            Arc::clone(&host) as Arc<dyn SubtitleFetcher>,
            options,
        ));
        Self {
            loader,
            host,
            folder,
        }
    }
}

impl Drop for TestSubtitles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// A loader for tests that never download anything; its folder is not created.
pub(crate) fn unused_loader() -> Arc<SubtitleLoader> {
    TestSubtitles::new().loader.clone()
}
