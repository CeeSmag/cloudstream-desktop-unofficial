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
    let checker = checker(&["https://x/mid.mp4", "https://x/hi.mp4", "https://x/low.mp4"]);
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
    assert_eq!(
        log.indices(),
        vec![0],
        "best among checked so far (only mid)"
    );
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
    assert!(matches!(state.ordered[0].1, Some(LinkHealth::Broken(_))));
}

#[test]
fn announce_ready_publishes_update_before_ready() {
    let order = Mutex::new(Vec::new());
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_update = |_: &crate::open_types::CheckedOpen| order.lock().expect("lock").push("update");
    let on_ready = |_: usize| order.lock().expect("lock").push("ready");
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &on_update,
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let mut state = OpenState::new();
    state.ordered = vec![(link(1080, "https://x/a.mp4"), None)];
    state.play_index = Some(0);
    state.announce_ready(&hooks);
    assert_eq!(*order.lock().expect("lock"), vec!["update", "ready"]);
}

#[test]
fn skipping_starts_playback_and_still_checks_remaining_sources() {
    let log = ReadyLog::new();
    let skip = AtomicBool::new(true);
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
    drive_checks(&mut state, &checker, &hooks);
    assert_eq!(
        log.indices(),
        vec![0],
        "started on first working after skip"
    );
    assert_eq!(state.checked, 2, "remaining sources still get checked");
    assert_eq!(state.working, 2);
}

#[test]
fn plant_playing_seeds_playing_stream_as_working_and_already_ready() {
    let updates = Mutex::new(Vec::new());
    let ready = ReadyLog::new();
    let skip = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let on_update =
        |open: &crate::open_types::CheckedOpen| updates.lock().expect("lock").push(open.clone());
    let on_ready = |index: usize| ready.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &on_update,
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let mut state = OpenState::new();
    let current = link(1080, "https://x/playing.mp4");
    crate::open_seeds::plant_playing(&mut state, &current, &hooks);
    assert!(state.ready_announced);
    assert_eq!(state.play_index, Some(0));
    assert_eq!(state.checked, 1);
    assert_eq!(state.working, 1);
    assert!(
        ready.indices().is_empty(),
        "already playing, no new on_ready"
    );
    let published = updates.lock().expect("lock");
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].links[0].0.url, "https://x/playing.mp4");
    assert!(published[0].links[0]
        .1
        .as_ref()
        .is_some_and(LinkHealth::is_working));
}

/// A link search that answers every poll with `links` and "done", counting the polls.
struct CountingGateway {
    links: Vec<PlayableLink>,
    polls: AtomicUsize,
}

impl extension_api::ProviderGateway for CountingGateway {
    fn load_extension(
        &self,
        _: &std::path::Path,
        _: bool,
    ) -> Result<Vec<core_model::ProviderInfo>, extension_api::GatewayError> {
        Ok(Vec::new())
    }
    fn search(
        &self,
        _: &str,
        _: &str,
        _: u32,
    ) -> Result<core_model::SearchPage, extension_api::GatewayError> {
        Ok(core_model::SearchPage::default())
    }
    fn main_page(
        &self,
        _: &str,
        _: u32,
        _: &core_model::MainPageEntry,
    ) -> Result<Option<core_model::HomePage>, extension_api::GatewayError> {
        Ok(None)
    }
    fn load(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<core_model::LoadDetails>, extension_api::GatewayError> {
        Ok(None)
    }
    fn load_links(
        &self,
        _: &str,
        _: &str,
    ) -> Result<core_model::PlayableLinks, extension_api::GatewayError> {
        Ok(core_model::PlayableLinks::default())
    }
    fn start_link_search(&self, _: &str, _: &str) -> Result<String, extension_api::GatewayError> {
        Ok("search".into())
    }
    fn poll_link_search(
        &self,
        _: &str,
        _: core_model::LinkSearchCursor,
        _: std::time::Duration,
    ) -> Result<core_model::LinkSearchUpdate, extension_api::GatewayError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(core_model::LinkSearchUpdate {
            links: self.links.clone(),
            next_link: self.links.len(),
            done: true,
            success: true,
            ..core_model::LinkSearchUpdate::default()
        })
    }
    fn extract(
        &self,
        _: &str,
        _: Option<&str>,
    ) -> Result<core_model::PlayableLinks, extension_api::GatewayError> {
        Ok(core_model::PlayableLinks::default())
    }
}

/// A search over `gateway` (started, not yet polled).
fn search_over(gateway: &Arc<CountingGateway>) -> app_services::LinkSearch {
    let shared: Arc<dyn extension_api::ProviderGateway> = gateway.clone();
    app_services::LinkSearch::start(shared, "Demo", "ep").expect("search starts")
}

fn counting(links: Vec<PlayableLink>) -> Arc<CountingGateway> {
    Arc::new(CountingGateway {
        links,
        polls: AtomicUsize::new(0),
    })
}

/// An open state that began `secs` seconds ago (the clock is not mocked; the start is moved).
fn started_ago(secs: u64) -> OpenState {
    let mut state = OpenState::new();
    state.started = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(secs))
        .expect("the monotonic clock runs longer than the test's offset");
    state
}

#[test]
fn past_the_total_cap_the_best_working_stream_starts_although_the_search_still_runs() {
    let log = ReadyLog::new();
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let checker = checker(&["https://x/a.mp4", "https://x/b.mp4"]);
    let mut state = started_ago(super::TOTAL_SECS + 1);
    state.ordered = vec![
        (link(720, "https://x/a.mp4"), None),
        (link(1080, "https://x/b.mp4"), None),
    ];
    check_pending(&mut state, &checker, &hooks);
    assert_eq!(
        log.indices(),
        vec![0],
        "the cap starts what works, search not done"
    );
    assert_eq!(
        state.checked, 1,
        "past the cap nothing more is checked once one works"
    );
}

#[test]
fn a_cancelled_open_stops_without_reading_the_search() {
    let gateway = counting(vec![link(1080, "https://x/a.mp4")]);
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(true));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let result = super::run_search(
        search_over(&gateway),
        OpenState::new(),
        &checker(&[]),
        &hooks,
    );
    assert_eq!(gateway.polls.load(Ordering::SeqCst), 0);
    assert!(matches!(result, Err(crate::OpenError::Empty)));
}

#[test]
fn past_the_total_cap_the_search_is_not_read_and_the_found_streams_are_still_checked() {
    let gateway = counting(Vec::new());
    let log = ReadyLog::new();
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let on_ready = |index: usize| log.push(index);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let mut state = started_ago(super::TOTAL_SECS + 1);
    state.ordered = vec![(link(1080, "https://x/a.mp4"), None)];
    let opened = super::run_search(
        search_over(&gateway),
        state,
        &checker(&["https://x/a.mp4"]),
        &hooks,
    )
    .expect("streams were found, so the open is not empty");
    assert_eq!(gateway.polls.load(Ordering::SeqCst), 0);
    assert_eq!(
        opened.play_index,
        Some(0),
        "the found stream is checked after the loop"
    );
    assert_eq!(log.indices(), vec![0]);
}

#[test]
fn nothing_found_within_the_first_wait_fails_the_open_without_reading_the_search() {
    let gateway = counting(vec![link(1080, "https://x/a.mp4")]);
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let state = started_ago(super::FIRST_WORKING_SECS);
    let result = super::run_search(search_over(&gateway), state, &checker(&[]), &hooks);
    assert_eq!(gateway.polls.load(Ordering::SeqCst), 0);
    assert!(matches!(result, Err(crate::OpenError::Empty)));
}

#[test]
fn distinct_subtitles_are_all_kept_and_repeats_dropped() {
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let track = |url: &str| core_model::SubtitleTrack {
        lang: "English".into(),
        url: url.into(),
        headers: None,
    };
    let mut state = OpenState::new();
    super::absorb_batch(
        &mut state,
        Vec::new(),
        vec![track("https://s/en.vtt"), track("https://s/ar.vtt")],
        &hooks,
    );
    super::absorb_batch(
        &mut state,
        Vec::new(),
        vec![track("https://s/en.vtt")],
        &hooks,
    );
    let urls: Vec<&str> = state.subtitles.iter().map(|t| t.url.as_str()).collect();
    assert_eq!(urls, vec!["https://s/en.vtt", "https://s/ar.vtt"]);
}
