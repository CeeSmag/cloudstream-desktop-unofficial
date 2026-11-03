use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

static FOLDERS: AtomicU32 = AtomicU32::new(0);

/// A cache in a folder of its own that is removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "cs_subtitle_cache_{}_{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::SeqCst)
        ));
        Self(folder)
    }

    fn cache(&self, ttl: Duration) -> SubtitleCache {
        SubtitleCache::new(self.0.clone(), ttl)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);

fn ready(cache: &SubtitleCache, url: &str, language: &str) -> ReadySubtitle {
    let path = cache
        .store(url, "1\n00:00:01,000 --> 00:00:02,000\nhi\n", "srt")
        .expect("stored");
    ReadySubtitle {
        language: language.to_owned(),
        path,
        url: url.to_owned(),
    }
}

#[test]
fn a_stored_file_is_found_again_by_its_address() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    assert_eq!(cache.lookup("https://s.test/a.vtt"), None);
    let path = cache
        .store("https://s.test/a.vtt", "WEBVTT\n", "vtt")
        .expect("stored");
    assert!(path.to_string_lossy().ends_with(".vtt"));
    assert_eq!(cache.lookup("https://s.test/a.vtt"), Some(path));
    assert_eq!(cache.lookup("https://s.test/b.vtt"), None);
}

#[test]
fn forgetting_a_title_removes_its_list_and_files_and_nothing_of_other_titles() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    let mine = ready(&cache, "https://s.test/mine.srt", "English");
    let theirs = ready(&cache, "https://s.test/theirs.srt", "English");
    cache
        .remember_for_title("P\nmine", &mine)
        .expect("remembered");
    cache
        .remember_for_title("P\ntheirs", &theirs)
        .expect("remembered");

    assert_eq!(cache.forget_title("P\nmine"), 1);

    assert!(cache.cached_for_title("P\nmine").is_empty());
    let lists = fs::read_dir(scratch.0.join(TITLES_DIR))
        .expect("folder")
        .count();
    assert_eq!(lists, 1, "only the list of the other title is left on disk");
    assert_eq!(cache.lookup("https://s.test/mine.srt"), None);
    assert_eq!(cache.cached_for_title("P\ntheirs").len(), 1);
    assert_eq!(cache.forget_title("P\nmine"), 0, "nothing left to forget");
}

#[test]
fn a_file_is_never_left_half_written() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    cache
        .store("https://s.test/a.vtt", "WEBVTT\n", "vtt")
        .expect("stored");
    let leftovers = fs::read_dir(scratch.0.join(FILES_DIR))
        .expect("folder")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn an_empty_or_old_file_is_not_found() {
    let scratch = Scratch::new();
    let cache = scratch.cache(Duration::from_millis(80));
    cache
        .store("https://s.test/empty.srt", "", "srt")
        .expect("stored");
    assert_eq!(
        cache.lookup("https://s.test/empty.srt"),
        None,
        "an empty file is no subtitle"
    );
    cache
        .store("https://s.test/old.srt", "text", "srt")
        .expect("stored");
    assert!(cache.lookup("https://s.test/old.srt").is_some());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(cache.lookup("https://s.test/old.srt"), None, "expired");
}

#[test]
fn old_files_and_lists_are_pruned() {
    let scratch = Scratch::new();
    let cache = scratch.cache(Duration::from_millis(80));
    let subtitle = ready(&cache, "https://s.test/a.srt", "Arabic");
    cache
        .remember_for_title("P\nm", &subtitle)
        .expect("remembered");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(cache.prune(), 2, "the file and the list");
    assert!(cache.cached_for_title("P\nm").is_empty());
}

#[test]
fn a_title_gets_back_what_it_was_offered_once_each_while_the_files_exist() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    let arabic = ready(&cache, "https://s.test/ar.srt", "Arabic");
    let english = ready(&cache, "https://s.test/en.srt", "English");
    for subtitle in [&arabic, &english, &arabic] {
        cache
            .remember_for_title("CineStream\nspider-man", subtitle)
            .expect("remembered");
    }
    let found = cache.cached_for_title("CineStream\nspider-man");
    let languages: Vec<&str> = found.iter().map(|s| s.language.as_str()).collect();
    assert_eq!(languages, ["Arabic", "English"]);
    assert_eq!(found[0], arabic);
    assert!(cache
        .cached_for_title("CineStream\nsomething-else")
        .is_empty());
}

#[test]
fn a_listed_file_that_was_deleted_is_left_out() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    let arabic = ready(&cache, "https://s.test/ar.srt", "Arabic");
    let english = ready(&cache, "https://s.test/en.srt", "English");
    cache.remember_for_title("t", &arabic).expect("remembered");
    cache.remember_for_title("t", &english).expect("remembered");
    fs::remove_file(&arabic.path).expect("deleted");
    let found = cache.cached_for_title("t");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].language, "English");
}

#[test]
fn a_damaged_line_in_a_list_is_skipped() {
    let scratch = Scratch::new();
    let cache = scratch.cache(WEEK);
    let arabic = ready(&cache, "https://s.test/ar.srt", "Arabic");
    cache.remember_for_title("t", &arabic).expect("remembered");
    let list = cache.title_list("t");
    let mut text = fs::read_to_string(&list).expect("list");
    text.push_str("{this is not json\n");
    fs::write(&list, text).expect("damaged");
    assert_eq!(cache.cached_for_title("t").len(), 1);
}
