//! T2 integration and race tests for decoupled polling, parallel ranked checking, and CheckGate.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use app_services::{LinkSearch, StreamChecker, StreamResolver};
use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLink, PlayableLinks, ProviderInfo, SearchPage, SubtitleTrack,
};
use extension_api::{GatewayError, ProviderGateway};
use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::*;
use crate::open_run::OpenState;
use crate::open_types::OpenHooks;
use crate::subtitle_priority::CheckGate;

fn link(quality: i32, url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        link_type: "VIDEO".into(),
        ..PlayableLink::default()
    }
}

fn subtitle(lang: &str, url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: lang.to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

struct ScriptedBatch {
    delay: Duration,
    links: Vec<PlayableLink>,
    subtitles: Vec<SubtitleTrack>,
    done: bool,
    failure: Option<String>,
    poll_error: Option<String>,
}

impl ScriptedBatch {
    fn ok(
        delay: Duration,
        links: Vec<PlayableLink>,
        subtitles: Vec<SubtitleTrack>,
        done: bool,
    ) -> Self {
        Self {
            delay,
            links,
            subtitles,
            done,
            failure: None,
            poll_error: None,
        }
    }
}

struct ScriptedGateway {
    batches: Mutex<VecDeque<ScriptedBatch>>,
}

impl ScriptedGateway {
    fn search(batches: Vec<ScriptedBatch>) -> LinkSearch {
        let gateway = Arc::new(Self {
            batches: Mutex::new(VecDeque::from(batches)),
        });
        let resolver = StreamResolver::new(gateway);
        resolver.open_search("P", "d").expect("open search")
    }
}

impl ProviderGateway for ScriptedGateway {
    fn load_extension(&self, _: &Path, _: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
        Ok(vec![])
    }
    fn unload_extension(&self, _: &Path) -> Result<(), GatewayError> {
        Ok(())
    }
    fn search(&self, _: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
        Ok(SearchPage::default())
    }
    fn main_page(
        &self,
        _: &str,
        _: u32,
        _: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        Ok(None)
    }
    fn load(&self, _: &str, _: &str) -> Result<Option<LoadDetails>, GatewayError> {
        Ok(None)
    }
    fn load_links(&self, _: &str, _: &str) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }
    fn start_link_search(&self, _: &str, _: &str) -> Result<String, GatewayError> {
        Ok("s1".into())
    }
    fn poll_link_search(
        &self,
        _: &str,
        cursor: LinkSearchCursor,
        _: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let next = self.batches.lock().expect("lock").pop_front();
        let Some(batch) = next else {
            return Ok(LinkSearchUpdate {
                done: true,
                success: true,
                next_link: cursor.links,
                next_subtitle: cursor.subtitles,
                ..LinkSearchUpdate::default()
            });
        };
        if let Some(error) = batch.poll_error {
            return Err(GatewayError::Remote {
                kind: "PollFailed".into(),
                message: error,
            });
        }
        if !batch.delay.is_zero() {
            std::thread::sleep(batch.delay);
        }
        let next_link = cursor.links + batch.links.len();
        let next_subtitle = cursor.subtitles + batch.subtitles.len();
        let success = batch.failure.is_none();
        Ok(LinkSearchUpdate {
            links: batch.links,
            subtitles: batch.subtitles,
            done: batch.done,
            success,
            failure: batch.failure,
            next_link,
            next_subtitle,
        })
    }
    fn cancel_link_search(&self, _: &str) -> Result<(), GatewayError> {
        Ok(())
    }
    fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }
    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        Ok(vec![])
    }
    fn shutdown(&self) {}
}

struct BlockingProbe {
    blocked_url: String,
    released: Mutex<bool>,
    wake: Condvar,
}

impl BlockingProbe {
    fn new(blocked_url: &str) -> Self {
        Self {
            blocked_url: blocked_url.to_owned(),
            released: Mutex::new(false),
            wake: Condvar::new(),
        }
    }

    fn release(&self) {
        *self.released.lock().expect("lock") = true;
        self.wake.notify_all();
    }
}

impl HttpProbe for BlockingProbe {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        if url == self.blocked_url {
            let mut guard = self.released.lock().expect("lock");
            while !*guard {
                guard = self.wake.wait(guard).expect("wait");
            }
        }
        Ok(ok_video())
    }
}

struct CountingProbe {
    calls: AtomicUsize,
    delay: Duration,
}

impl HttpProbe for CountingProbe {
    fn probe(&self, _: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(ok_video())
    }
}

fn ok_video() -> ProbeResponse {
    ProbeResponse {
        status: 200,
        content_type: "video/mp4".into(),
        head: vec![0, 1, 2, 3, 4, 5, 6, 7],
        total_bytes: Some(1_000),
    }
}

fn run_with_hooks(
    search: LinkSearch,
    checker: &StreamChecker,
    gate: &CheckGate,
    hooks: &OpenHooks<'_>,
) -> CheckedOpen {
    let config = PoolConfig {
        checker,
        hooks,
        gate,
        workers: 4,
        grace: Duration::from_secs(2),
    };
    run_search_pool(search, OpenState::new(), &config).expect("opened")
}

fn slow_probe_batches() -> Vec<ScriptedBatch> {
    vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            vec![link(1080, "https://hang.test/video.mp4")],
            vec![subtitle("English", "https://sub.test/1.vtt")],
            false,
        ),
        ScriptedBatch::ok(
            Duration::from_millis(25),
            vec![link(720, "https://fast.test/video.mp4")],
            vec![
                subtitle("Arabic", "https://sub.test/2.vtt"),
                subtitle("French", "https://sub.test/3.vtt"),
            ],
            true,
        ),
    ]
}

#[test]
fn slow_stream_probe_does_not_block_subtitle_delivery_from_later_batches() {
    let search = ScriptedGateway::search(slow_probe_batches());
    let probe = Arc::new(BlockingProbe::new("https://hang.test/video.mp4"));
    let checker = StreamChecker::new(Arc::clone(&probe) as Arc<dyn HttpProbe>);
    let probe_release = Arc::clone(&probe);
    let subs_seen = Arc::new(AtomicUsize::new(0));
    let subs_cb = Arc::clone(&subs_seen);
    let on_update = move |open: &CheckedOpen| {
        if open.subtitles.len() >= 3 {
            subs_cb.store(open.subtitles.len(), Ordering::SeqCst);
            probe_release.release();
        }
    };
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &on_update,
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let started = Instant::now();
    let opened = run_with_hooks(search, &checker, &CheckGate::default(), &hooks);
    assert_eq!(subs_seen.load(Ordering::SeqCst), 3);
    assert_eq!(opened.subtitles.len(), 3);
    assert_eq!(opened.play_index, Some(0));
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn pinned_focus_batches() -> Vec<ScriptedBatch> {
    let links = (0..8)
        .map(|i| link(720, &format!("https://s.test/{i}.mp4")))
        .collect();
    vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            links,
            vec![subtitle("English", "https://sub.test/1.vtt")],
            false,
        ),
        ScriptedBatch::ok(
            Duration::from_millis(150),
            vec![],
            vec![subtitle("Arabic", "https://sub.test/2.vtt")],
            true,
        ),
    ]
}

#[test]
fn pinned_subtitle_focus_pauses_stream_checks_until_search_finishes() {
    let search = ScriptedGateway::search(pinned_focus_batches());
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(5),
    });
    let checker = StreamChecker::new(Arc::clone(&probe) as Arc<dyn HttpProbe>);
    let gate = CheckGate::default();
    gate.pin_subtitles_focus();
    let calls_at_batch2 = Arc::new(AtomicUsize::new(usize::MAX));
    let (calls_cb, probe_cb) = (Arc::clone(&calls_at_batch2), Arc::clone(&probe));
    let on_update = |open: &CheckedOpen| {
        if open.subtitles.len() == 2 {
            let _ = calls_cb.compare_exchange(
                usize::MAX,
                probe_cb.calls.load(Ordering::SeqCst),
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
    };
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &on_update,
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let opened = run_with_hooks(search, &checker, &gate, &hooks);
    assert_eq!(calls_at_batch2.load(Ordering::SeqCst), 0);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 8);
    assert_eq!(opened.links.len(), 8);
}

#[test]
fn cancel_while_parked_in_pinned_subtitle_focus_exits_promptly() {
    let search = ScriptedGateway::search(vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            vec![link(1080, "https://c.test/1.mp4")],
            vec![],
            false,
        ),
        ScriptedBatch::ok(Duration::from_millis(50), vec![], vec![], false),
    ]);
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(Arc::clone(&probe) as Arc<dyn HttpProbe>);
    let gate = Arc::new(CheckGate::default());
    gate.pin_subtitles_focus();
    let cancel = Arc::new(AtomicBool::new(false));
    let (gate_bg, cancel_bg) = (Arc::clone(&gate), Arc::clone(&cancel));
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        cancel_bg.store(true, Ordering::SeqCst);
        gate_bg.cancel();
    });
    let skip = AtomicBool::new(false);
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let started = Instant::now();
    let _ = run_with_hooks(search, &checker, &gate, &hooks);
    canceller.join().expect("join");
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn race_batches(iteration: usize) -> Vec<ScriptedBatch> {
    let batch1 = (0..12)
        .map(|i| link(480 + i * 60, &format!("https://r.test/{iteration}/{i}.mp4")))
        .collect();
    let batch2 = (12..24)
        .map(|i| {
            link(
                480 + (i % 6) * 120,
                &format!("https://r.test/{iteration}/{i}.mp4"),
            )
        })
        .collect();
    vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            batch1,
            vec![subtitle("English", "https://sub.test/en.vtt")],
            false,
        ),
        ScriptedBatch::ok(
            Duration::from_millis(5),
            batch2,
            vec![subtitle("Arabic", "https://sub.test/ar.vtt")],
            true,
        ),
    ]
}

fn race_iteration(iteration: usize) {
    let search = ScriptedGateway::search(race_batches(iteration));
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(1),
    });
    let checker = StreamChecker::new(Arc::clone(&probe) as Arc<dyn HttpProbe>);
    let gate = Arc::new(CheckGate::default());
    let skip = Arc::new(AtomicBool::new(false));
    let cancel = AtomicBool::new(false);
    let ready_count = AtomicUsize::new(0);
    let on_ready = |_idx: usize| {
        ready_count.fetch_add(1, Ordering::SeqCst);
        gate.prioritize_subtitles_auto();
    };
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let driver = spawn_priority_driver(Arc::clone(&gate), Arc::clone(&skip), iteration);
    let config = PoolConfig {
        checker: &checker,
        hooks: &hooks,
        gate: &gate,
        workers: 6,
        grace: Duration::from_millis(20),
    };
    let opened = run_search_pool(search, OpenState::new(), &config).expect("opened");
    driver.join().expect("driver");
    assert_eq!(opened.links.len(), 24);
    assert_eq!(opened.subtitles.len(), 2);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 24);
    assert_eq!(ready_count.load(Ordering::SeqCst), 1);
    assert!(opened.play_index.is_some());
}

fn spawn_priority_driver(
    gate: Arc<CheckGate>,
    skip: Arc<AtomicBool>,
    iteration: usize,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if iteration.is_multiple_of(2) {
            skip.store(true, Ordering::SeqCst);
        }
        gate.prioritize_subtitles_auto();
        std::thread::yield_now();
        gate.signal_subtitles_satisfied();
    })
}

#[test]
fn race_concurrent_batches_priority_toggles_and_skip_check_each_link_once() {
    for iteration in 0..20 {
        race_iteration(iteration);
    }
}

#[test]
fn batch_failure_errors_when_empty() {
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(probe as Arc<dyn HttpProbe>);
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let gate = CheckGate::default();
    let config = PoolConfig::standard(&checker, &hooks, &gate);
    let err_search = ScriptedGateway::search(vec![ScriptedBatch {
        delay: Duration::ZERO,
        links: vec![],
        subtitles: vec![],
        done: true,
        failure: Some("provider exploded".into()),
        poll_error: None,
    }]);
    let err = run_search_pool(err_search, OpenState::new(), &config).expect_err("must fail");
    assert!(matches!(err, OpenError::Search(ref m) if m.contains("provider exploded")));
}

#[test]
fn batch_failure_keeps_partial_links_and_deduplicates_across_batches() {
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(probe as Arc<dyn HttpProbe>);
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let partial = ScriptedGateway::search(vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            vec![link(1080, "https://p.test/ok.mp4")],
            vec![subtitle("English", "https://sub.test/en.vtt")],
            false,
        ),
        ScriptedBatch {
            delay: Duration::ZERO,
            links: vec![link(1080, "https://p.test/ok.mp4")],
            subtitles: vec![subtitle("English", "https://sub.test/en.vtt")],
            done: true,
            failure: Some("late mirror failed".into()),
            poll_error: None,
        },
    ]);
    let opened = run_with_hooks(partial, &checker, &CheckGate::default(), &hooks);
    assert_eq!(opened.links.len(), 1);
    assert_eq!(opened.subtitles.len(), 1);
}

#[test]
fn poll_transport_error_surfaces_as_open_error_search() {
    let search = ScriptedGateway::search(vec![ScriptedBatch {
        delay: Duration::ZERO,
        links: vec![],
        subtitles: vec![],
        done: false,
        failure: None,
        poll_error: Some("sidecar pipe broken".into()),
    }]);
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(probe as Arc<dyn HttpProbe>);
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let gate = CheckGate::default();
    let config = PoolConfig::standard(&checker, &hooks, &gate);
    let err = run_search_pool(search, OpenState::new(), &config).expect_err("poll fails");
    assert!(matches!(err, OpenError::Search(ref m) if m.contains("sidecar pipe broken")));
}

#[test]
fn pick_best_unclaimed_checks_highest_rank_first_with_earlier_tiebreak() {
    let ordered = vec![
        (link(480, "https://r/480.mp4"), None),
        (link(1080, "https://r/1080a.mp4"), None),
        (link(1080, "https://r/1080b.mp4"), None),
        (link(720, "https://r/720.mp4"), None),
    ];
    let claimed = vec![false; 4];
    assert_eq!(
        pick_best_unclaimed(&ordered, &claimed, PreferredResolution::Any),
        Some(1)
    );
    assert_eq!(
        pick_best_unclaimed(
            &ordered,
            &[false, true, false, false],
            PreferredResolution::Any
        ),
        Some(2)
    );
}

#[test]
fn pick_best_unclaimed_prioritizes_preferred_resolution_before_higher_rank() {
    let ordered = vec![
        (link(2160, "https://r/2160.mp4"), None),
        (link(1080, "https://r/1080.mp4"), None),
        (link(720, "https://r/720.mp4"), None),
    ];
    let claimed = vec![false; 3];
    assert_eq!(
        pick_best_unclaimed(&ordered, &claimed, PreferredResolution::Hd),
        Some(1),
        "1080p stream should be probed before 2160p when Hd is preferred"
    );
    assert_eq!(
        pick_best_unclaimed(&ordered, &claimed, PreferredResolution::Uhd),
        Some(0),
        "2160p stream should be probed first when Uhd is preferred"
    );
}

struct SizedProbe;

impl HttpProbe for SizedProbe {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        let size = url
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".mp4"))
            .and_then(|digits| digits.parse::<u64>().ok());
        if size.is_some_and(|bytes| bytes % 3 == 0) {
            std::thread::yield_now();
        }
        Ok(ProbeResponse {
            status: 200,
            content_type: "video/mp4".into(),
            head: vec![0; 8],
            total_bytes: size,
        })
    }
}

#[test]
fn concurrent_pool_deterministically_picks_largest_working_in_preferred_resolution() {
    let checker = StreamChecker::new(Arc::new(SizedProbe));
    for _ in 0..40 {
        let links = vec![
            link(2160, "https://size.test/9000.mp4"),
            link(1080, "https://size.test/1200.mp4"),
            link(720, "https://size.test/4500.mp4"),
            link(1080, "https://size.test/3000.mp4"),
            link(2160, "https://size.test/6000.mp4"),
        ];
        let search =
            ScriptedGateway::search(vec![ScriptedBatch::ok(Duration::ZERO, links, vec![], true)]);
        let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
        let hooks = OpenHooks {
            on_progress: &|_| {},
            on_update: &|_| {},
            on_ready: &|_| {},
            skip: &skip,
            cancel: &cancel,
        };
        let gate = CheckGate::default();
        let config = PoolConfig::standard(&checker, &hooks, &gate);
        let opened = run_search_pool(
            search,
            OpenState::with_preferred(PreferredResolution::Hd),
            &config,
        )
        .expect("pool succeeds");
        let chosen = opened.play_index.expect("chosen");
        assert_eq!(opened.links[chosen].0.url, "https://size.test/4500.mp4");
    }
}

fn zero_progress() -> OpenProgress {
    OpenProgress {
        found: 0,
        checked: 0,
        working: 0,
        search_done: false,
    }
}

#[test]
fn skip_mid_search_announces_ready_before_search_completes() {
    let search = ScriptedGateway::search(vec![
        ScriptedBatch::ok(
            Duration::ZERO,
            vec![link(1080, "https://m.test/1.mp4")],
            vec![],
            false,
        ),
        ScriptedBatch::ok(Duration::from_millis(120), vec![], vec![], true),
    ]);
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(probe as Arc<dyn HttpProbe>);
    let (skip, cancel) = (AtomicBool::new(true), AtomicBool::new(false));
    let (search_done, ready_early) = (AtomicBool::new(false), AtomicBool::new(false));
    let last_progress = Mutex::new(zero_progress());
    let on_progress = |p: OpenProgress| {
        search_done.fetch_or(p.search_done, Ordering::SeqCst);
        *last_progress.lock().expect("lock") = p;
    };
    let on_ready = |_| {
        ready_early.fetch_or(!search_done.load(Ordering::SeqCst), Ordering::SeqCst);
    };
    let hooks = OpenHooks {
        on_progress: &on_progress,
        on_update: &|_| {},
        on_ready: &on_ready,
        skip: &skip,
        cancel: &cancel,
    };
    let opened = run_with_hooks(search, &checker, &CheckGate::default(), &hooks);
    assert_eq!(opened.play_index, Some(0));
    assert!(ready_early.load(Ordering::SeqCst));
    let p = *last_progress.lock().expect("lock");
    assert_eq!((p.found, p.checked, p.working), (1, 1, 1));
}

#[test]
fn poll_error_stops_workers_without_probing_remaining_queued_links() {
    let links = (0..6)
        .map(|i| link(720, &format!("https://err.test/{i}.mp4")))
        .collect();
    let search = ScriptedGateway::search(vec![
        ScriptedBatch::ok(Duration::ZERO, links, vec![], false),
        ScriptedBatch {
            delay: Duration::from_millis(20),
            links: vec![],
            subtitles: vec![],
            done: false,
            failure: None,
            poll_error: Some("sidecar crashed".into()),
        },
    ]);
    let probe = Arc::new(CountingProbe {
        calls: AtomicUsize::new(0),
        delay: Duration::ZERO,
    });
    let checker = StreamChecker::new(Arc::clone(&probe) as Arc<dyn HttpProbe>);
    let gate = CheckGate::default();
    // Hold workers paused until the poll error triggers guard.stopped = true + mark_search_done.
    gate.pin_subtitles_focus();
    let (skip, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
    let hooks = OpenHooks {
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        skip: &skip,
        cancel: &cancel,
    };
    let config = PoolConfig::standard(&checker, &hooks, &gate);
    let err = run_search_pool(search, OpenState::new(), &config).expect_err("must fail");
    assert!(matches!(err, OpenError::Search(_)));
    assert_eq!(
        probe.calls.load(Ordering::SeqCst),
        0,
        "workers must stop immediately on poll error without probing queued links"
    );
}

#[test]
fn timed_out_pool_stops_claiming_when_working_exists_and_fails_empty_search() {
    let mut state = OpenState::new();
    state.started = Instant::now() - Duration::from_secs(TOTAL_SECS + 5);
    state.ordered = vec![
        (
            link(1080, "https://t/ok.mp4"),
            Some(LinkHealth::Working {
                size_bytes: None,
                subtitles: vec![],
            }),
        ),
        (link(720, "https://t/unchecked.mp4"), None),
    ];
    state.play_index = Some(0);
    let mut pool = SharedPool::new(state);
    assert!(matches!(pool.claim_next(false), Claim::Done));

    let mut empty = OpenState::new();
    empty.started = Instant::now() - Duration::from_secs(FIRST_WORKING_SECS + 5);
    let shared = Mutex::new(SharedPool::new(empty));
    assert!(matches!(
        check_empty_timeout(&shared),
        Err(OpenError::Empty)
    ));
}
