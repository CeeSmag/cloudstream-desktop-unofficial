use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use super::*;

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// A cache folder of its own that is removed when the test ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cs_subtitle_loader_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Script = HashMap<String, VecDeque<Result<Vec<u8>, FetchError>>>;

/// Answers each address from a script, in turn; remembers how often it was asked and with what.
#[derive(Default)]
struct ScriptedFetcher {
    answers: Mutex<Script>,
    calls: AtomicUsize,
    headers_seen: Mutex<Vec<(String, String)>>,
}

impl ScriptedFetcher {
    fn answering(url: &str, answers: Vec<Result<Vec<u8>, FetchError>>) -> Self {
        let fetcher = Self::default();
        fetcher
            .answers
            .lock()
            .expect("lock")
            .insert(url.to_owned(), answers.into());
        fetcher
    }
}

impl SubtitleFetcher for ScriptedFetcher {
    fn fetch(&self, url: &str, headers: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.headers_seen
            .lock()
            .expect("lock")
            .extend_from_slice(headers);
        self.answers
            .lock()
            .expect("lock")
            .get_mut(url)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Err(FetchError::Status { status: 404 }))
    }
}

const URL: &str = "https://subs.test/ar.vtt";
const VTT: &[u8] = b"WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n";

fn track(url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: "Arabic".to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

fn loader(scratch: &Scratch, fetcher: Arc<ScriptedFetcher>) -> SubtitleLoader {
    let cache = SubtitleCache::new(scratch.0.clone(), Duration::from_secs(3600));
    let options = LoadOptions {
        attempts: 3,
        backoff: Duration::from_millis(1),
    };
    SubtitleLoader::new(cache, fetcher, options)
}

fn always() -> bool {
    true
}

#[test]
fn a_subtitle_is_downloaded_cleaned_saved_and_found_in_the_cache_next_time() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(ScriptedFetcher::answering(URL, vec![Ok(VTT.to_vec())]));
    let loader = loader(&scratch, Arc::clone(&fetcher));

    let first = loader.load(&track(URL), &always).expect("loads");
    assert!(!first.from_cache);
    assert_eq!(first.ready.language, "Arabic");
    assert!(first.ready.path.to_string_lossy().ends_with(".vtt"));
    assert_eq!(std::fs::read(&first.ready.path).expect("file"), VTT);

    let second = loader.load(&track(URL), &always).expect("loads");
    assert!(second.from_cache && second.ready.path == first.ready.path);
    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        1,
        "the second load must not download"
    );
}

#[test]
fn a_host_that_fails_once_is_tried_again() {
    let scratch = Scratch::new();
    let slow = FetchError::Network {
        reason: "timed out".to_owned(),
    };
    let fetcher = Arc::new(ScriptedFetcher::answering(
        URL,
        vec![Err(slow.clone()), Err(slow), Ok(VTT.to_vec())],
    ));
    let loaded = loader(&scratch, Arc::clone(&fetcher))
        .load(&track(URL), &always)
        .expect("third try works");
    assert!(!loaded.from_cache);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn a_missing_file_is_not_tried_again() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(ScriptedFetcher::answering(
        URL,
        vec![Err(FetchError::Status { status: 404 })],
    ));
    let error = loader(&scratch, Arc::clone(&fetcher))
        .load(&track(URL), &always)
        .expect_err("fails");
    assert!(
        matches!(error, LoadError::Download { attempts: 1, .. }),
        "{error}"
    );
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_host_that_never_answers_is_given_up_on_after_the_last_try() {
    let scratch = Scratch::new();
    let slow = FetchError::Network {
        reason: "timed out".to_owned(),
    };
    let fetcher = Arc::new(ScriptedFetcher::answering(
        URL,
        vec![Err(slow.clone()), Err(slow.clone()), Err(slow)],
    ));
    let error = loader(&scratch, Arc::clone(&fetcher))
        .load(&track(URL), &always)
        .expect_err("fails");
    assert!(
        matches!(error, LoadError::Download { attempts: 3, .. }),
        "{error}"
    );
}

#[test]
fn a_web_page_is_refused_and_nothing_is_saved() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(ScriptedFetcher::answering(
        URL,
        vec![Ok(b"<html>blocked</html>".to_vec())],
    ));
    let loader = loader(&scratch, fetcher);
    let error = loader.load(&track(URL), &always).expect_err("refused");
    assert!(
        matches!(error, LoadError::Unusable(NormalizeError::WebPage)),
        "{error}"
    );
    assert_eq!(loader.cache().lookup(URL), None);
}

#[test]
fn a_file_nobody_wants_any_more_is_not_downloaded() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(ScriptedFetcher::answering(URL, vec![Ok(VTT.to_vec())]));
    let error = loader(&scratch, Arc::clone(&fetcher))
        .load(&track(URL), &|| false)
        .expect_err("stopped");
    assert!(matches!(error, LoadError::Stopped));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn the_headers_the_provider_gave_are_sent() {
    let scratch = Scratch::new();
    let fetcher = Arc::new(ScriptedFetcher::answering(URL, vec![Ok(VTT.to_vec())]));
    let mut wanted = track(URL);
    wanted.headers = Some(HashMap::from([(
        "Referer".to_owned(),
        "https://site.test/".to_owned(),
    )]));
    loader(&scratch, Arc::clone(&fetcher))
        .load(&wanted, &always)
        .expect("loads");
    let seen = fetcher.headers_seen.lock().expect("lock");
    assert_eq!(
        *seen,
        [("Referer".to_owned(), "https://site.test/".to_owned())]
    );
}

#[test]
fn a_subtitle_playlist_is_saved_as_one_webvtt_file() {
    let scratch = Scratch::new();
    let playlist = "https://subs.test/list.m3u8";
    let fetcher = ScriptedFetcher::answering(
        playlist,
        vec![Ok(b"#EXTM3U\n#EXTINF:5,\npiece.vtt\n".to_vec())],
    );
    fetcher.answers.lock().expect("lock").insert(
        "https://subs.test/piece.vtt".to_owned(),
        VecDeque::from([Ok(VTT.to_vec())]),
    );
    let loaded = loader(&scratch, Arc::new(fetcher))
        .load(&track(playlist), &always)
        .expect("loads");
    assert!(loaded.ready.path.to_string_lossy().ends_with(".vtt"));
    assert!(std::fs::read_to_string(&loaded.ready.path)
        .expect("file")
        .contains("hi"));
}
