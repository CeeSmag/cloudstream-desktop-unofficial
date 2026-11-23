//! Saved-source seeds: a fresh list starts at once, a stale one is probed briefly and only until
//! one works. Every test runs before any live search, so a start can only come from the seeds.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_services::StreamChecker;
use core_model::PlayableLink;
use extension_repo::{HttpProbe, ProbeResponse, RepoError};
use title_cache::{CacheLimits, CachedLink, CachedTitle, LinkHealth, TitleCache};

use super::{plant_saved, STALE_TEST_SECS};
use crate::open_run::OpenState;
use crate::open_types::{OpenCache, OpenHooks};

/// Saved at this time; read at the same time it is fresh, two hours later it is stale.
const SAVED_AT_SECS: u64 = 10_000;
const TWO_HOURS: u64 = 7_200;

/// Answers as working video for the listed addresses, 404 for every other.
struct MapProbe {
    ok: Vec<String>,
}

impl HttpProbe for MapProbe {
    fn probe(&self, url: &str, _headers: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        let works = self.ok.iter().any(|known| known == url);
        Ok(ProbeResponse {
            status: if works { 200 } else { 404 },
            content_type: if works {
                "video/mp4".into()
            } else {
                String::new()
            },
            head: if works { vec![0; 8] } else { Vec::new() },
            total_bytes: works.then_some(1_000),
        })
    }
}

fn checker(ok: &[&str]) -> StreamChecker {
    StreamChecker::new(Arc::new(MapProbe {
        ok: ok.iter().map(|url| (*url).to_owned()).collect(),
    }))
}

fn link(quality: i32, url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        link_type: "VIDEO".into(),
        ..PlayableLink::default()
    }
}

fn folder(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|age| age.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("cs_seeds_{tag}_{}_{nanos}", std::process::id()))
}

/// A saved list on disk; its folder is removed when the test ends.
struct Seeded {
    open_cache: OpenCache,
    folder: PathBuf,
}

impl Drop for Seeded {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// A cache holding `links` (all saved as working, so playing order is by quality) and read
/// `read_after` seconds after they were saved.
fn cache_with(tag: &str, links: &[PlayableLink], read_after: u64) -> Seeded {
    let now = Arc::new(Mutex::new(SAVED_AT_SECS));
    let clock_now = Arc::clone(&now);
    let folder = folder(tag);
    let cache = Arc::new(TitleCache::with_clock(
        folder.clone(),
        CacheLimits::default(),
        Arc::new(move || UNIX_EPOCH + Duration::from_secs(*clock_now.lock().expect("clock"))),
    ));
    let title_key = OpenCache::key("Demo", tag);
    let title = CachedTitle {
        complete: true,
        links: links
            .iter()
            .map(|link| CachedLink {
                link: link.clone(),
                health: LinkHealth::Working,
            })
            .collect(),
        last_good: None,
    };
    cache.save(&title_key, &title).expect("save seeds");
    *now.lock().expect("clock") += read_after;
    Seeded {
        open_cache: OpenCache { cache, title_key },
        folder,
    }
}

/// Plants the seeds of `open_cache` and returns the indices `on_ready` was called with.
fn plant(state: &mut OpenState, open_cache: &OpenCache, checker: &StreamChecker) -> Vec<usize> {
    let ready = Mutex::new(Vec::new());
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let on_ready = |index: usize| ready.lock().expect("ready").push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    plant_saved(state, open_cache, checker, &hooks);
    let indices = ready.lock().expect("ready").clone();
    indices
}

#[test]
fn a_fresh_saved_stream_that_works_starts_playback_before_any_search() {
    let seeds = [
        link(1080, "https://x/hi.mp4"),
        link(720, "https://x/low.mp4"),
    ];
    let seeded = cache_with("fresh_ok", &seeds, 0);
    let mut state = OpenState::new();
    let ready = plant(
        &mut state,
        &seeded.open_cache,
        &checker(&["https://x/hi.mp4", "https://x/low.mp4"]),
    );
    assert_eq!(
        ready,
        vec![0],
        "the first saved stream starts at once, search not done"
    );
    assert_eq!(state.play_index, Some(0));
    assert!(!state.search_done);
}

#[test]
fn a_fresh_saved_stream_that_fails_waits_for_the_gate_instead_of_starting() {
    let seeds = [
        link(1080, "https://x/hi.mp4"),
        link(720, "https://x/low.mp4"),
    ];
    let seeded = cache_with("fresh_broken", &seeds, 0);
    let mut state = OpenState::new();
    let ready = plant(
        &mut state,
        &seeded.open_cache,
        &checker(&["https://x/low.mp4"]),
    );
    assert!(
        ready.is_empty(),
        "only the first saved stream is trusted to start at once"
    );
    assert_eq!(state.play_index, Some(1), "the rest is still checked");
    assert_eq!(state.checked, 2);
}

#[test]
fn a_stale_list_starts_on_the_first_saved_stream_that_works_and_stops_probing() {
    let seeds = [
        link(1080, "https://x/a.mp4"),
        link(720, "https://x/b.mp4"),
        link(480, "https://x/c.mp4"),
    ];
    let seeded = cache_with("stale_ok", &seeds, TWO_HOURS);
    let mut state = OpenState::new();
    let ready = plant(
        &mut state,
        &seeded.open_cache,
        &checker(&["https://x/b.mp4", "https://x/c.mp4"]),
    );
    assert_eq!(ready, vec![1], "a is broken, b is the first that works");
    assert_eq!(state.next_to_check, 2, "c is left to the live checks");
    assert_eq!(state.checked, 2);
}

#[test]
fn a_stale_list_where_nothing_works_hands_over_at_once_not_after_the_probe_window() {
    let seeds = [link(1080, "https://x/a.mp4"), link(720, "https://x/b.mp4")];
    let seeded = cache_with("stale_broken", &seeds, TWO_HOURS);
    let mut state = OpenState::new();
    let started = Instant::now();
    let ready = plant(&mut state, &seeded.open_cache, &checker(&[]));
    assert!(
        started.elapsed() < Duration::from_secs(STALE_TEST_SECS),
        "every seed was checked, so the open must not sit out the {STALE_TEST_SECS} s window"
    );
    assert!(ready.is_empty());
    assert_eq!((state.checked, state.next_to_check), (2, 2));
    assert_eq!(state.play_index, None);
}
