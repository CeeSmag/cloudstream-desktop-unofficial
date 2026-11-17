//! Only the video opened last keeps saved sources, also when the checks of earlier videos finish
//! late (T1, and race tests with real threads and files).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use super::{KeptSave, KeptTitle};
use crate::cache_limits::CacheLimits;
use crate::test_folder::{many, Scratch, Time};
use crate::title_cache::{Lookup, TitleCache};

const MOVIE_A: &str = "provider\nmovie-a";
const MOVIE_B: &str = "provider\nmovie-b";

fn cache_in(scratch: &Scratch) -> TitleCache {
    scratch.cache(CacheLimits::default(), &Time::new())
}

fn is_saved(cache: &TitleCache, key: &str) -> bool {
    matches!(cache.load(key), Lookup::Hit(_))
}

fn saved(result: KeptSave) -> bool {
    matches!(result, KeptSave::Saved(_))
}

#[test]
fn keep_only_removes_the_other_titles_and_keeps_its_own() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch);
    cache.save(MOVIE_A, &many("a", 3)).expect("save a");
    cache.save(MOVIE_B, &many("b", 3)).expect("save b");
    let kept = cache.keep_only(MOVIE_B);
    assert_eq!(kept.key(), MOVIE_B);
    assert!(
        !is_saved(&cache, MOVIE_A),
        "the earlier video's sources are dropped"
    );
    assert!(is_saved(&cache, MOVIE_B), "the open video keeps its own");
    assert_eq!(scratch.files().len(), 1);
}

#[test]
fn keep_only_with_nothing_saved_yet_is_fine() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch);
    let kept = cache.keep_only(MOVIE_A);
    assert!(saved(cache.save_kept(&kept, &many("a", 2)).expect("save")));
    assert!(is_saved(&cache, MOVIE_A));
}

#[test]
fn a_late_save_of_the_previous_video_is_refused_and_leaves_no_file() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch);
    let video_a = cache.keep_only(MOVIE_A);
    let video_b = cache.keep_only(MOVIE_B);
    // Video A's checks finish after video B was opened.
    let late = cache.save_kept(&video_a, &many("a", 3)).expect("save a");
    assert_eq!(late, KeptSave::Replaced);
    assert!(saved(
        cache.save_kept(&video_b, &many("b", 3)).expect("save b")
    ));
    assert!(!is_saved(&cache, MOVIE_A));
    assert_eq!(scratch.files().len(), 1);
}

#[test]
fn opening_the_same_title_again_makes_the_older_opening_stale() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch);
    let first = cache.keep_only(MOVIE_A);
    let refreshed = cache.keep_only(MOVIE_A);
    cache.forget_kept(&refreshed).expect("forget");
    assert_eq!(
        cache.save_kept(&first, &many("old", 3)).expect("save"),
        KeptSave::Replaced,
        "the old search must not bring back what a refresh dropped"
    );
    assert!(!is_saved(&cache, MOVIE_A));
}

#[test]
fn a_late_removal_of_an_older_opening_keeps_what_the_newer_one_saved() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch);
    let first = cache.keep_only(MOVIE_A);
    let second = cache.keep_only(MOVIE_A);
    cache.save_kept(&second, &many("new", 3)).expect("save");
    assert!(!cache.forget_kept(&first).expect("forget"));
    assert!(is_saved(&cache, MOVIE_A));
    assert!(cache.forget_kept(&second).expect("forget"));
    assert!(!is_saved(&cache, MOVIE_A));
}

/// How many saved titles are in the folder (files being written do not count).
fn saved_titles(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count()
        })
        .unwrap_or(0)
}

/// Looks at the folder until `running` is cleared; returns the most titles it ever saw at once.
fn watch_folder(folder: PathBuf, running: Arc<AtomicBool>) -> thread::JoinHandle<usize> {
    thread::spawn(move || {
        let mut most = 0;
        while running.load(Ordering::SeqCst) {
            most = most.max(saved_titles(&folder));
        }
        most
    })
}

const VIDEOS: usize = 6;
const ROUNDS: usize = 60;

/// One video's thread: opens its title and saves it [`ROUNDS`] times, noting what it was handed.
fn open_and_save_again_and_again(
    video: usize,
    cache: Arc<TitleCache>,
    handed_out: Arc<Mutex<Vec<KeptTitle>>>,
    start: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        start.wait();
        let key = format!(
            "provider
video-{video}"
        );
        for _ in 0..ROUNDS {
            let kept = cache.keep_only(&key);
            handed_out.lock().expect("lock").push(kept.clone());
            cache.save_kept(&kept, &many(&key, 2)).expect("save");
        }
    })
}

/// One thread per video: each opens its title and saves it, again and again, while the others do
/// the same and a watcher looks at the folder. Whatever the order, there is never more than one
/// saved title, and the title opened last keeps its file.
#[test]
fn race_many_videos_opened_and_saved_at_once_leave_only_the_last_one() {
    let scratch = Scratch::new();
    let cache = Arc::new(cache_in(&scratch));
    let handed_out: Arc<Mutex<Vec<KeptTitle>>> = Arc::default();
    let start = Arc::new(Barrier::new(VIDEOS));
    let running = Arc::new(AtomicBool::new(true));
    let watcher = watch_folder(scratch.0.clone(), Arc::clone(&running));
    let threads: Vec<_> = (0..VIDEOS)
        .map(|video| {
            let shared = (Arc::clone(&handed_out), Arc::clone(&start));
            open_and_save_again_and_again(video, Arc::clone(&cache), shared.0, shared.1)
        })
        .collect();
    for thread in threads {
        thread.join().expect("a video thread panicked");
    }
    running.store(false, Ordering::SeqCst);
    let most_at_once = watcher.join().expect("the watcher panicked");
    assert!(
        most_at_once <= 1,
        "{most_at_once} titles were saved at once"
    );
    let handed_out = handed_out.lock().expect("lock");
    let last = handed_out.iter().max_by_key(|kept| kept.ticket);
    let last = last.expect("titles were kept");
    assert_eq!(
        scratch.files().len(),
        1,
        "files left: {:?}",
        scratch.files()
    );
    assert!(
        is_saved(&cache, last.key()),
        "the last video opened keeps its file"
    );
}

/// The checks of the previous video save while the next video is being opened, both many times.
#[test]
fn race_a_late_save_never_survives_the_next_opening() {
    const ROUNDS: usize = 300;
    let scratch = Scratch::new();
    let cache = Arc::new(cache_in(&scratch));
    for round in 0..ROUNDS {
        let previous = cache.keep_only(MOVIE_A);
        let start = Arc::new(Barrier::new(2));
        let late_save = {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                cache.save_kept(&previous, &many("a", 2)).expect("save a")
            })
        };
        start.wait();
        let next = cache.keep_only(MOVIE_B);
        late_save.join().expect("the late save panicked");
        assert!(
            !is_saved(&cache, MOVIE_A),
            "round {round}: the previous video's file survived the next opening"
        );
        cache.forget_kept(&next).expect("forget b");
    }
}
