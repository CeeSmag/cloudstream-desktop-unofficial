//! Only the video opened last keeps its subtitles, also when downloads of earlier videos finish
//! late (T1, and race tests with real threads and files).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use crate::subtitle_cache::{ReadySubtitle, SubtitleCache};

const VIDEO_A: &str = "provider\nepisode-1";
const VIDEO_B: &str = "provider\nepisode-2";
const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// A folder of its own that is removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cs_subtitle_keep_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        )))
    }

    fn cache(&self) -> SubtitleCache {
        SubtitleCache::new(self.0.clone(), WEEK)
    }

    /// How many subtitle files are on disk.
    fn files(&self) -> usize {
        fs::read_dir(self.0.join("files")).map_or(0, |entries| entries.count())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Stores a file for `url`, as a finished download does.
fn download(cache: &SubtitleCache, url: &str) -> ReadySubtitle {
    let path = cache
        .store(url, "WEBVTT\n\n00:01.000 --> 00:02.000\nhi\n", "vtt")
        .expect("stored");
    ReadySubtitle {
        language: "English".to_owned(),
        path,
        url: url.to_owned(),
    }
}

fn download_and_remember(cache: &SubtitleCache, title: &str, url: &str) -> ReadySubtitle {
    let ready = download(cache, url);
    cache.remember_for_title(title, &ready).expect("remembered");
    ready
}

#[test]
fn keeping_a_title_removes_the_lists_and_files_of_the_others() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    download_and_remember(&cache, VIDEO_A, "https://s.test/a.vtt");
    let own = download_and_remember(&cache, VIDEO_B, "https://s.test/b.vtt");
    assert_eq!(cache.keep_only_title(VIDEO_B), 2, "A's list and A's file");
    assert!(cache.cached_for_title(VIDEO_A).is_empty());
    assert_eq!(cache.cached_for_title(VIDEO_B), vec![own]);
    assert_eq!(
        cache.lookup("https://s.test/a.vtt"),
        None,
        "A downloads again"
    );
    assert_eq!(scratch.files(), 1);
}

#[test]
fn a_file_both_titles_name_stays_with_the_kept_one() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    download_and_remember(&cache, VIDEO_A, "https://s.test/shared.vtt");
    download_and_remember(&cache, VIDEO_B, "https://s.test/shared.vtt");
    cache.keep_only_title(VIDEO_B);
    assert_eq!(cache.cached_for_title(VIDEO_B).len(), 1);
    assert_eq!(scratch.files(), 1);
}

#[test]
fn files_not_named_by_any_list_go_too() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    download(&cache, "https://s.test/never-remembered.vtt");
    assert_eq!(cache.keep_only_title(VIDEO_B), 1);
    assert_eq!(scratch.files(), 0);
}

#[test]
fn a_late_download_of_the_replaced_video_is_not_noted_and_its_file_goes() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    cache.keep_only_title(VIDEO_A);
    // B is opened while A's download is still running; it finishes and is noted afterwards.
    cache.keep_only_title(VIDEO_B);
    let late = download(&cache, "https://s.test/a.vtt");
    cache.remember_for_title(VIDEO_A, &late).expect("no error");
    assert!(cache.cached_for_title(VIDEO_A).is_empty());
    assert!(!late.path.exists(), "the late file is removed");
}

#[test]
fn a_late_download_keeps_a_file_the_open_video_also_names() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    cache.keep_only_title(VIDEO_B);
    let shared = download_and_remember(&cache, VIDEO_B, "https://s.test/shared.vtt");
    cache
        .remember_for_title(VIDEO_A, &shared)
        .expect("no error");
    assert!(shared.path.exists());
    assert!(cache.cached_for_title(VIDEO_A).is_empty());
}

#[test]
fn before_any_video_is_opened_every_title_is_noted() {
    let scratch = Scratch::new();
    let cache = scratch.cache();
    download_and_remember(&cache, VIDEO_A, "https://s.test/a.vtt");
    download_and_remember(&cache, VIDEO_B, "https://s.test/b.vtt");
    assert_eq!(cache.cached_for_title(VIDEO_A).len(), 1);
    assert_eq!(cache.cached_for_title(VIDEO_B).len(), 1);
}

/// A download of video A finishes and is noted while video B is being opened, many times over.
/// Whatever the order, A has nothing kept afterwards and no file of A is left on disk.
#[test]
fn race_a_late_download_never_survives_the_next_opening() {
    const ROUNDS: usize = 200;
    let scratch = Scratch::new();
    let cache = Arc::new(scratch.cache());
    for round in 0..ROUNDS {
        cache.keep_only_title(VIDEO_A);
        let start = Arc::new(Barrier::new(2));
        let late = {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                download_and_remember(&cache, VIDEO_A, &format!("https://s.test/a-{round}.vtt"))
            })
        };
        start.wait();
        cache.keep_only_title(VIDEO_B);
        let late = late.join().expect("the late download panicked");
        assert!(
            cache.cached_for_title(VIDEO_A).is_empty(),
            "round {round}: A's list survived the next opening"
        );
        assert!(!late.path.exists(), "round {round}: A's file survived");
    }
}
