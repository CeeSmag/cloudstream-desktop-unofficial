//! Gate behaviour of the open loop: defer ready, pick best by rank, skip starts best so far.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use app_services::{LinkHealth, StreamChecker};
use core_model::PlayableLink;
use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::{check_one, check_pending, OpenState};
use crate::open_types::OpenHooks;

struct MapProbe {
    /// URLs that should look like working video.
    ok: Vec<String>,
}

impl HttpProbe for MapProbe {
    fn probe(&self, url: &str, _headers: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        if self.ok.iter().any(|known| known == url) {
            Ok(ProbeResponse {
                status: 200,
                content_type: "video/mp4".into(),
                head: vec![0, 1, 2, 3, 4, 5, 6, 7],
                total_bytes: Some(1_000),
            })
        } else {
            Ok(ProbeResponse {
                status: 404,
                ..ProbeResponse::default()
            })
        }
    }
}

fn link(quality: i32, url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        link_type: "VIDEO".into(),
        ..PlayableLink::default()
    }
}

fn checker(ok: &[&str]) -> StreamChecker {
    StreamChecker::new(Arc::new(MapProbe {
        ok: ok.iter().map(|url| (*url).to_owned()).collect(),
    }))
}

struct ReadyLog {
    ready: Mutex<Vec<usize>>,
    count: AtomicUsize,
}

impl ReadyLog {
    fn new() -> Self {
        Self {
            ready: Mutex::new(Vec::new()),
            count: AtomicUsize::new(0),
        }
    }

    fn push(&self, index: usize) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.ready.lock().expect("ready").push(index);
    }

    fn indices(&self) -> Vec<usize> {
        self.ready.lock().expect("ready").clone()
    }
}

fn drive_checks(state: &mut OpenState, checker: &StreamChecker, hooks: &OpenHooks<'_>) {
    check_pending(state, checker, hooks);
}

#[test]
fn a_working_stream_does_not_start_playback_while_checks_go_on() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let checker = checker(&["https://x/low.mp4", "https://x/hi.mp4"]);
    let mut state = OpenState::new();
    state.ordered = vec![
        (link(720, "https://x/low.mp4"), None),
        (link(1080, "https://x/hi.mp4"), None),
    ];
    // Search still running: first working must not announce.
    check_one(&mut state, &checker, &hooks);
    assert!(state.play_index.is_some());
    assert!(log.indices().is_empty(), "must not OpenReady mid-check");
    assert_eq!(log.count.load(Ordering::SeqCst), 0);
}

#[test]
fn starts_on_the_best_working_stream_once_everything_is_checked() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let checker = checker(&["https://x/low.mp4", "https://x/hi.mp4"]);
    let mut state = OpenState::new();
    state.search_done = true;
    state.ordered = vec![
        (link(720, "https://x/low.mp4"), None),
        (link(1080, "https://x/hi.mp4"), None),
    ];
    drive_checks(&mut state, &checker, &hooks);
    assert_eq!(log.indices(), vec![1], "best by quality, not first working");
    assert_eq!(state.play_index, Some(1));
}

#[test]
fn skipping_starts_at_once_on_the_best_working_so_far() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let checker = checker(&[
        "https://x/mid.mp4",
        "https://x/hi.mp4",
        "https://x/low.mp4",
    ]);
    let mut state = OpenState::new();
    state.ordered = vec![
        (link(720, "https://x/mid.mp4"), None),
        (link(1080, "https://x/hi.mp4"), None),
        (link(480, "https://x/low.mp4"), None),
    ];
    check_one(&mut state, &checker, &hooks);
    assert!(log.indices().is_empty());
    skip.store(true, Ordering::SeqCst);
    state.maybe_announce_ready(&hooks);
    assert_eq!(log.indices(), vec![0], "best among checked so far (only mid)");
    // A later better one is not required once skip already started.
    assert_eq!(state.play_index, Some(0));
}

#[test]
fn a_better_stream_found_while_waiting_wins_before_announce() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let checker = checker(&["https://x/a.mp4", "https://x/b.mp4"]);
    let mut state = OpenState::new();
    state.search_done = true;
    state.ordered = vec![
        (link(480, "https://x/a.mp4"), None),
        (link(2160, "https://x/b.mp4"), None),
    ];
    drive_checks(&mut state, &checker, &hooks);
    assert_eq!(state.play_index, Some(1));
    assert_eq!(log.indices(), vec![1]);
}

#[test]
fn broken_streams_are_skipped_when_picking_best() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    // High quality is broken; mid works.
    let checker = checker(&["https://x/mid.mp4"]);
    let mut state = OpenState::new();
    state.search_done = true;
    state.ordered = vec![
        (link(2160, "https://x/hi.mp4"), None),
        (link(720, "https://x/mid.mp4"), None),
    ];
    drive_checks(&mut state, &checker, &hooks);
    assert_eq!(state.play_index, Some(1));
    assert_eq!(log.indices(), vec![1]);
    assert!(matches!(
        state.ordered[0].1,
        Some(LinkHealth::Broken(_))
    ));
}
