//! T2/T4 without a window or libmpv: the whole opening of a video, from the search through the
//! gateway and a scripted provider, the checks, the wait for the first good stream and the start of
//! playback, to closing the video. Measures that playback starts within the limit while the search
//! goes on, and that closing (or opening and closing fifty videos in a row) leaves nothing running.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use app_services::{PlaybackItem, PlaybackService, StreamChecker, StreamResolver, SystemClock};
use extension_repo::{HttpProbe, ProbeResponse, RepoError};
use extension_runtime::fake_link_search::{FakeLinkSearches, SearchScript};
use extension_runtime::fake_search_runtime::FakeSearchRuntime;
use extension_runtime::RpcProviderGateway;
use player_api::VideoPlayer;
use storage::{Database, WatchProgressStore};

use crate::check_gate::WaitLimits;
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::recording_player::RecordingPlayer;
use crate::source_manager::{SourceManager, Tell};
use crate::test_subtitles::TestSubtitles;
use crate::test_titles::TestTitles;
use crate::video_opening::{open_video, OpeningServices};

/// Every stream is a working video, except the ones whose address holds a word of `dead`.
struct ProbeByAddress(Arc<Mutex<Vec<String>>>);

impl HttpProbe for ProbeByAddress {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        if self
            .0
            .lock()
            .expect("lock")
            .iter()
            .any(|word| url.contains(word))
        {
            return Err(RepoError::Download {
                url: url.to_owned(),
                reason: "dead in this test".to_owned(),
            });
        }
        Ok(ProbeResponse {
            status: 206,
            content_type: "video/mp4".to_owned(),
            head: b"\0\0\0\x18ftypmp42".to_vec(),
            total_bytes: Some(1_000_000),
        })
    }
}

/// Everything one test needs: the manager, the services, and the fakes to look at afterwards.
pub(crate) struct Rig {
    pub(crate) sources: SourceManager,
    pub(crate) services: OpeningServices,
    pub(crate) searches: Arc<FakeLinkSearches>,
    pub(crate) player: Arc<RecordingPlayer>,
    pub(crate) subtitles: Arc<TestSubtitles>,
    /// The saved sources of the titles opened with this rig (and the ones sharing it).
    pub(crate) titles: Arc<TestTitles>,
    /// Streams whose address holds one of these words fail their check.
    pub(crate) dead: Arc<Mutex<Vec<String>>>,
}

pub(crate) fn rig(script: SearchScript, limits: WaitLimits) -> Rig {
    let (subtitles, titles) = (Arc::new(TestSubtitles::new()), Arc::new(TestTitles::new()));
    build(script, limits, (subtitles, titles))
}

/// A second setup that shares the subtitle and saved-sources folders (and so what was saved
/// there) with `first`, as the app does when a title is opened again.
pub(crate) fn rig_sharing(first: &Rig, script: SearchScript, limits: WaitLimits) -> Rig {
    let shared = (Arc::clone(&first.subtitles), Arc::clone(&first.titles));
    build(script, limits, shared)
}

fn build(
    script: SearchScript,
    limits: WaitLimits,
    (subtitles, titles): (Arc<TestSubtitles>, Arc<TestTitles>),
) -> Rig {
    let searches = Arc::new(FakeLinkSearches::new(move |_| script.clone()));
    let gateway = RpcProviderGateway::new(FakeSearchRuntime::new(Arc::clone(&searches)));
    let player = Arc::new(RecordingPlayer::default());
    let dead = Arc::new(Mutex::new(Vec::new()));
    let playback = PlaybackService::new(
        Arc::clone(&player) as Arc<dyn VideoPlayer>,
        WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db"))),
        Arc::new(SystemClock),
    );
    Rig {
        sources: SourceManager::new(
            StreamChecker::new(Arc::new(ProbeByAddress(Arc::clone(&dead)))),
            Arc::clone(&subtitles.loader),
            Arc::clone(&titles.cache),
        )
        .with_limits(limits),
        services: OpeningServices {
            streams: Arc::new(StreamResolver::new(Arc::new(gateway))),
            playback: Arc::new(playback),
            subtitle_rank: Arc::new(|_| 0),
        },
        searches,
        player,
        subtitles,
        titles,
        dead,
    }
}

pub(crate) fn open(rig: &Rig) -> (Result<(), String>, Vec<JoinHandle<()>>, Duration) {
    let tell: Tell = Arc::new(|_| {});
    let item = PlaybackItem {
        provider: "Fake".to_owned(),
        media_key: "title".to_owned(),
        ..PlaybackItem::default()
    };
    let started = Instant::now();
    let (outcome, background) = open_video(&rig.sources, &rig.services, item, tell);
    (outcome, background, started.elapsed())
}

/// Limits for a test: the provider's first stream is due after `first_ms`, everything after `max_ms`.
pub(crate) fn limits(first_ms: u64, max_ms: u64) -> WaitLimits {
    WaitLimits {
        first_stream: Duration::from_millis(first_ms),
        max_wait: Duration::from_millis(max_ms),
    }
}

/// Like [`open`], with the viewer pressing skip `pause` after the video was opened.
pub(crate) fn open_skipping_after(
    rig: &Rig,
    pause: Duration,
) -> (Result<(), String>, Vec<JoinHandle<()>>, Duration) {
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(pause);
            rig.sources.skip_with(&|_| {});
        });
        open(rig)
    })
}

#[test]
fn playback_starts_only_when_the_search_and_all_checks_are_done_on_the_best_stream() {
    // Three streams of different quality found over 300 ms; the search ends at 700 ms.
    let mut script = SearchScript::steady(3, Duration::from_millis(100), 480, Duration::ZERO);
    script.links = vec![
        (Duration::from_millis(100), 480),
        (Duration::from_millis(200), 1080),
        (Duration::from_millis(300), 720),
    ];
    script.ends_after = Duration::from_millis(700);
    let rig = rig(script, limits(5_000, 10_000));

    let (outcome, background, waited) = open(&rig);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(650),
        "playback started after {waited:?}, before the search was over"
    );
    assert_eq!(
        rig.searches.stats().active,
        0,
        "the search was still running"
    );
    let played = rig.player.played.lock().expect("lock").clone();
    assert_eq!(played.len(), 1);
    let found = rig.sources.session().snapshot().expect("sources");
    assert_eq!(found.links.len(), 3);
    assert_eq!(played[0], found.links[1].url, "the 1080 stream is the best");
    join_within(background, TIME_LIMIT);
}

/// Skipping plays at once on what works and leaves the search and the checks running.
#[test]
fn skipping_plays_at_once_and_the_search_goes_on_in_the_background() {
    let script = SearchScript::steady(30, Duration::from_millis(100), 480, Duration::from_secs(1));
    let rig = rig(script, limits(5_000, 30_000));

    let (outcome, background, waited) = open_skipping_after(&rig, Duration::from_millis(500));

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(450) && waited <= Duration::from_millis(2_000),
        "skipped after 0.5 s but playback started after {waited:?}"
    );
    assert_eq!(rig.player.played.lock().expect("lock").len(), 1);
    assert_eq!(
        rig.searches.stats().active,
        1,
        "skipping must not stop the search"
    );

    join_within(background, TIME_LIMIT);
    let found = rig
        .sources
        .session()
        .snapshot()
        .map_or(0, |active| active.links.len());
    assert_eq!(found, 30, "the search did not finish in the background");
    assert_eq!(rig.searches.stats().active, 0);
}

/// A search that never ends does not hold the viewer forever: at the cap the best stream plays.
#[test]
fn a_search_that_never_ends_is_cut_at_the_cap_and_the_best_stream_plays() {
    let script = SearchScript::steady(3, Duration::from_millis(100), 720, Duration::from_secs(600));
    let rig = rig(script, limits(5_000, 800));

    let (outcome, background, waited) = open(&rig);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(750) && waited <= Duration::from_millis(2_000),
        "started after {waited:?}; the cap is 0.8 s"
    );
    assert_eq!(rig.player.played.lock().expect("lock").len(), 1);
    assert_eq!(
        rig.searches.stats().active,
        1,
        "the search goes on after the cap"
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

/// A provider that gives nothing is an error for the viewer, and nothing is left running.
#[test]
fn a_provider_that_gives_no_stream_in_time_is_an_error_and_its_search_is_stopped() {
    let script = SearchScript::steady(1, Duration::from_secs(600), 720, Duration::from_secs(600));
    let rig = rig(script, limits(400, 5_000));

    let (outcome, background, waited) = open(&rig);

    let reason = outcome.expect_err("the provider never answered");
    assert!(reason.contains("did not answer"), "{reason}");
    assert!(
        waited >= Duration::from_millis(350) && waited <= Duration::from_millis(2_000),
        "failed after {waited:?}; the limit is 0.4 s"
    );
    assert!(rig.player.played.lock().expect("lock").is_empty());
    join_within(background, TIME_LIMIT);
    assert_eq!(
        rig.searches.stats().active,
        0,
        "the search was left running"
    );
}

#[path = "opening_flow_closing_tests.rs"]
mod closing;

#[path = "opening_flow_saved_tests.rs"]
mod saved;
