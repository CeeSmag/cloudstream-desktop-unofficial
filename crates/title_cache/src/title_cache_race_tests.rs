//! Real threads against real files: the cache must never show a half-written title, lose a good
//! one to a cleanup, or leave temporary files behind.

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use crate::cache_limits::CacheLimits;
use crate::stored_file::Miss;
use crate::test_folder::{many, Scratch, Time};
use crate::title_cache::Lookup;
use crate::title_cache::TitleCache;

const KEY: &str = "provider\nmovie";

fn cache_in(scratch: &Scratch, limits: CacheLimits) -> Arc<TitleCache> {
    Arc::new(scratch.cache(limits, &Time::new()))
}

/// A title whose links all carry the same writer number and whose count is a function of it, so a
/// reader can tell whether what it got is one whole save.
fn save_of_writer(writer: usize) -> crate::cached_title::CachedTitle {
    many(&format!("writer{writer}"), 10 + writer)
}

fn is_one_whole_save(links: &[crate::cached_title::CachedLink]) -> bool {
    let Some(first) = links.first() else {
        return false;
    };
    let Some(writer) = first
        .link
        .url
        .split("writer")
        .nth(1)
        .and_then(|rest| rest.split('-').next())
        .and_then(|number| number.parse::<usize>().ok())
    else {
        return false;
    };
    links.len() == 10 + writer
        && links
            .iter()
            .all(|cached| cached.link.url.contains(&format!("writer{writer}-")))
}

/// Loads the title again and again until `writing` is cleared; every load must be one whole save.
fn read_until_done(cache: &TitleCache, writing: &AtomicBool, start: &Barrier) -> usize {
    start.wait();
    let mut seen = 0;
    while writing.load(Ordering::SeqCst) {
        match cache.load(KEY) {
            Lookup::Hit(hit) => assert!(
                is_one_whole_save(&hit.title.links),
                "a reader saw a mix of two saves"
            ),
            Lookup::Miss(miss) => panic!("the title vanished: {miss:?}"),
        }
        seen += 1;
    }
    seen
}

#[test]
fn readers_never_see_a_half_written_title_while_writers_replace_it() {
    const WRITERS: usize = 4;
    const SAVES: usize = 40;
    const READERS: usize = 4;
    let scratch = Scratch::new();
    let cache = cache_in(&scratch, CacheLimits::default());
    cache.save(KEY, &save_of_writer(0)).expect("first save");
    let writing = Arc::new(AtomicBool::new(true));
    let start = Arc::new(Barrier::new(WRITERS + READERS));

    let readers: Vec<_> = (0..READERS)
        .map(|_| {
            let (cache, writing, start) =
                (Arc::clone(&cache), Arc::clone(&writing), Arc::clone(&start));
            thread::spawn(move || read_until_done(&cache, &writing, &start))
        })
        .collect();
    let writers: Vec<_> = (0..WRITERS)
        .map(|writer| {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                for _ in 0..SAVES {
                    cache.save(KEY, &save_of_writer(writer)).expect("saved");
                }
            })
        })
        .collect();

    for writer in writers {
        writer.join().expect("writer");
    }
    writing.store(false, Ordering::SeqCst);
    let looked: usize = readers.into_iter().map(|r| r.join().expect("reader")).sum();
    assert!(looked > 0, "the readers never ran");
    assert_eq!(scratch.files().len(), 1, "{:?}", scratch.files());
}

/// A reader that finds a broken file removes it; if a save replaced it in between, the good file
/// must survive. Done many times with the two released together.
#[test]
fn a_cleanup_of_a_broken_file_never_removes_the_good_one_saved_meanwhile() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch, CacheLimits::default());
    cache
        .save("seed", &many("seed", 1))
        .expect("makes the folder");
    let path = cache.path_of(KEY);
    for round in 0..300 {
        fs::write(&path, b"broken").expect("broken file");
        let start = Arc::new(Barrier::new(2));
        let reader = {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                cache.load(KEY)
            })
        };
        let writer = {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                cache.save(KEY, &save_of_writer(1)).expect("saved");
            })
        };
        writer.join().expect("writer");
        let seen = reader.join().expect("reader");
        assert!(
            matches!(seen, Lookup::Hit(_) | Lookup::Miss(Miss::Corrupt)),
            "round {round}: {seen:?}"
        );
        assert!(
            matches!(cache.load(KEY), Lookup::Hit(_)),
            "round {round}: the good save was lost to the cleanup of the broken file"
        );
    }
}

#[test]
fn many_threads_saving_and_pruning_leave_no_temporary_files_and_respect_the_title_limit() {
    const THREADS: usize = 8;
    const TITLES_PER_THREAD: usize = 25;
    let scratch = Scratch::new();
    let limits = CacheLimits {
        max_titles: 10,
        ..CacheLimits::default()
    };
    let cache = cache_in(&scratch, limits);
    let start = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|thread_number| {
            let (cache, start) = (Arc::clone(&cache), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                for n in 0..TITLES_PER_THREAD {
                    let key = format!("provider\nthread{thread_number}-title{n}");
                    cache.save(&key, &many("a", 3)).expect("saved");
                    cache.prune();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread");
    }
    let files = scratch.files();
    assert!(files.len() <= 10, "{} files: {files:?}", files.len());
    assert!(
        files.iter().all(|name| name.ends_with(".json")),
        "{files:?}"
    );
}

#[test]
fn fifty_rounds_of_save_load_forget_leave_the_folder_empty() {
    let scratch = Scratch::new();
    let cache = cache_in(&scratch, CacheLimits::default());
    for round in 0..50 {
        let key = format!("provider\nround{round}");
        cache.save(&key, &many("a", 5)).expect("saved");
        assert!(matches!(cache.load(&key), Lookup::Hit(_)));
        assert!(cache.forget(&key).expect("forgotten"));
    }
    assert!(scratch.files().is_empty(), "{:?}", scratch.files());
}
