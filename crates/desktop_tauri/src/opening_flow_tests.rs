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
use player_api::{PlaybackRequest, PlaybackStatus, PlayerError, SubtitleSource, VideoPlayer};
use storage::{Database, WatchProgressStore};

use crate::check_gate::Grace;
use crate::race_tests::{join_within, TIME_LIMIT};
use crate::source_manager::{SourceManager, Tell};
use crate::test_subtitles::TestSubtitles;
use crate::video_opening::{open_video, OpeningServices};

/// Records what the player was asked to play.
#[derive(Default)]
pub(crate) struct RecordingPlayer {
    pub(crate) played: Mutex<Vec<String>>,
    /// The subtitle files each stream was started with.
    pub(crate) started_with: Mutex<Vec<Vec<SubtitleSource>>>,
    /// Subtitle files given to a stream that was already playing.
    pub(crate) added_later: Mutex<Vec<SubtitleSource>>,
}

impl VideoPlayer for RecordingPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.played.lock().expect("lock").push(request.url.clone());
        self.started_with
            .lock()
            .expect("lock")
            .push(request.subtitles.clone());
        Ok(())
    }
    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        self.added_later
            .lock()
            .expect("lock")
            .extend_from_slice(subtitles);
    }
    fn set_paused(&self, _: bool) -> Result<(), PlayerError> {
        Ok(())
    }
    fn seek_to(&self, _: i64) -> Result<(), PlayerError> {
        Ok(())
    }
    fn stop(&self) -> Result<(), PlayerError> {
        Ok(())
    }
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::default()
    }
}

/// Every stream is a working video.
struct EveryStreamWorks;

impl HttpProbe for EveryStreamWorks {
    fn probe(&self, _: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
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
}

pub(crate) fn rig(script: SearchScript, grace: Grace) -> Rig {
    build(script, grace, Arc::new(TestSubtitles::new()))
}

/// A second setup that shares the subtitle folder (and so what was saved there) with `first`.
pub(crate) fn rig_sharing(first: &Rig, script: SearchScript, grace: Grace) -> Rig {
    build(script, grace, Arc::clone(&first.subtitles))
}

fn build(script: SearchScript, grace: Grace, subtitles: Arc<TestSubtitles>) -> Rig {
    let searches = Arc::new(FakeLinkSearches::new(move |_| script.clone()));
    let gateway = RpcProviderGateway::new(FakeSearchRuntime::new(Arc::clone(&searches)));
    let player = Arc::new(RecordingPlayer::default());
    let playback = PlaybackService::new(
        Arc::clone(&player) as Arc<dyn VideoPlayer>,
        WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db"))),
        Arc::new(SystemClock),
    );
    Rig {
        sources: SourceManager::new(
            StreamChecker::new(Arc::new(EveryStreamWorks)),
            Arc::clone(&subtitles.loader),
        )
        .with_grace(grace),
        services: OpeningServices {
            streams: Arc::new(StreamResolver::new(Arc::new(gateway))),
            playback: Arc::new(playback),
            subtitle_rank: Arc::new(|_| 0),
        },
        searches,
        player,
        subtitles,
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

pub(crate) fn grace(high_ms: u64, other_ms: u64, max_ms: u64) -> Grace {
    Grace {
        high_quality: Duration::from_millis(high_ms),
        other: Duration::from_millis(other_ms),
        max_wait: Duration::from_millis(max_ms),
    }
}

#[test]
fn playback_starts_within_the_limit_while_the_search_goes_on_in_the_background() {
    // Twelve streams, one every 150 ms from 100 ms on; the search ends 600 ms after the last.
    let script = SearchScript::steady(
        12,
        Duration::from_millis(150),
        480,
        Duration::from_millis(600),
    );
    let rig = rig(script, grace(100, 300, 1_000));

    let (outcome, background, waited) = open(&rig);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(200) && waited <= Duration::from_millis(1_500),
        "playback started after {waited:?}, the limit is 1 s"
    );
    assert_eq!(rig.player.played.lock().expect("lock").len(), 1);
    assert_eq!(
        rig.searches.stats().active,
        1,
        "the search must still be running when playback starts"
    );

    join_within(background, TIME_LIMIT);
    let found = rig
        .sources
        .session()
        .snapshot()
        .map_or(0, |active| active.links.len());
    assert_eq!(
        found, 12,
        "the search did not finish finding every stream in the background"
    );
    assert_eq!(
        rig.searches.stats().active,
        0,
        "the finished search was not forgotten"
    );
}

/// The viewer never waits longer than the limit, even when the grace period for a better stream is
/// much longer.
#[test]
fn the_limit_ends_the_wait_even_when_the_grace_for_a_better_stream_is_longer() {
    let script = SearchScript::steady(30, Duration::from_millis(100), 480, Duration::from_secs(3));
    let rig = rig(script, grace(5_000, 5_000, 800));

    let (outcome, background, waited) = open(&rig);

    assert_eq!(outcome, Ok(()));
    assert!(
        waited >= Duration::from_millis(700) && waited <= Duration::from_millis(1_500),
        "started after {waited:?}; the limit is 0.8 s and the grace 5 s"
    );
    assert_eq!(
        rig.searches.stats().active,
        1,
        "the search must go on after the limit"
    );
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

#[test]
fn with_nothing_found_in_time_it_keeps_searching_and_plays_the_first_stream() {
    // The first stream arrives after the 1 s limit.
    let script = SearchScript::steady(
        3,
        Duration::from_millis(1_400),
        720,
        Duration::from_millis(100),
    );
    let rig = rig(script, grace(50, 50, 1_000));

    let (outcome, background, waited) = open(&rig);

    assert_eq!(
        outcome,
        Ok(()),
        "a search that is still going must not fail the video"
    );
    assert!(
        waited >= Duration::from_millis(1_300),
        "started after {waited:?}, before anything was found"
    );
    assert_eq!(rig.player.played.lock().expect("lock").len(), 1);
    rig.sources.close();
    join_within(background, TIME_LIMIT);
}

#[test]
fn leaving_before_anything_is_found_ends_the_wait_and_stops_the_search() {
    let script = SearchScript::steady(1, Duration::from_secs(30), 720, Duration::from_secs(30));
    let rig = Arc::new(rig(script, grace(50, 50, 60_000)));
    let opening = {
        let rig = Arc::clone(&rig);
        std::thread::spawn(move || {
            let (outcome, background, _) = open(&rig);
            (outcome, background)
        })
    };
    std::thread::sleep(Duration::from_millis(250));

    rig.sources.close();

    let deadline = Instant::now() + Duration::from_secs(5);
    while !opening.is_finished() {
        assert!(
            Instant::now() < deadline,
            "closing did not end the wait for the first stream"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let (outcome, background) = opening.join().expect("the opening thread");
    assert!(
        outcome.is_err(),
        "a video closed before it started must not report success"
    );
    join_within(background, TIME_LIMIT);
    assert!(
        rig.player.played.lock().expect("lock").is_empty(),
        "nothing should have played"
    );
    assert_eq!(
        rig.searches.stats().active,
        0,
        "the search was left running"
    );
}

#[path = "opening_flow_closing_tests.rs"]
mod closing;
