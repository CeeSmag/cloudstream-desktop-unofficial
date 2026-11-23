use storage::Database;

use super::*;
use crate::playback_types::NextEpisode;
use crate::test_support::{FakePlayer, FixedClock};

fn setup() -> (
    PlaybackService,
    Arc<FakePlayer>,
    WatchProgressStore,
    ResumeStore,
    Arc<FixedClock>,
) {
    let player = Arc::new(FakePlayer::default());
    let database = Arc::new(Database::open_in_memory().expect("memory db"));
    let progress = WatchProgressStore::new(Arc::clone(&database));
    let resume = ResumeStore::new(Arc::clone(&database));
    let clock = Arc::new(FixedClock::at(1_000));
    let service = PlaybackService::new(
        Arc::clone(&player) as Arc<dyn VideoPlayer>,
        progress.clone(),
        resume.clone(),
        Arc::clone(&clock) as Arc<dyn Clock>,
    );
    (service, player, progress, resume, clock)
}

fn item() -> PlaybackItem {
    PlaybackItem {
        provider: "P".into(),
        media_key: "ep1".into(),
        title: "Episode 1".into(),
        detail_url: "https://show/1".into(),
        poster_url: Some("p.png".into()),
        season: Some(1),
        episode: Some(1),
        next_episode: Some(NextEpisode {
            media_key: "ep2".into(),
            season: Some(1),
            episode: Some(2),
        }),
    }
}

fn link() -> PlayableLink {
    PlayableLink {
        url: "https://cdn.test/v.mp4".into(),
        referer: "https://site.test".into(),
        ..PlayableLink::default()
    }
}

fn status(state: PlaybackState, position_ms: i64) -> PlaybackStatus {
    PlaybackStatus {
        state,
        position_ms,
        duration_ms: 100_000,
        has_video: true,
        ..PlaybackStatus::default()
    }
}

#[test]
fn boundary_test_89_90_91_percent() {
    assert!(!is_over_watch_threshold(89_000, 100_000));
    assert!(is_over_watch_threshold(90_000, 100_000));
    assert!(is_over_watch_threshold(91_000, 100_000));
}

#[test]
fn start_sends_url_referer_and_subtitles_to_the_player() {
    let (service, player, _, _, _) = setup();
    let subtitle = SubtitleTrack {
        lang: "English".into(),
        url: "https://s.test/en.vtt".into(),
        headers: None,
    };
    service.start(item(), &link(), &[subtitle]).expect("starts");
    let request = player.requests.lock().expect("lock")[0].clone();
    assert_eq!(request.url, "https://cdn.test/v.mp4");
    assert_eq!(request.referer, "https://site.test");
    assert_eq!(request.subtitles[0].language, "English");
}

#[test]
fn progress_is_saved_only_after_it_moved_enough() {
    let (service, player, progress, _, clock) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    player.report(status(PlaybackState::Playing, 2_000));
    service.tick();
    assert!(
        progress.get("P", "ep1").expect("get").is_none(),
        "2s is below the save threshold"
    );
    clock.set(7_000);
    player.report(status(PlaybackState::Playing, 6_000));
    service.tick();
    assert_eq!(
        progress
            .get("P", "ep1")
            .expect("get")
            .expect("saved")
            .position_ms,
        6_000
    );
}

#[test]
fn under_90_percent_keeps_current_episode_as_resume_pointer() {
    let (service, player, _, resume, clock) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    clock.set(10_000);
    player.report(status(PlaybackState::Playing, 89_000));
    service.tick();

    let pointer = resume
        .get("P", "https://show/1")
        .expect("get")
        .expect("present");
    assert_eq!(pointer.media_key, "ep1");
    assert_eq!(pointer.season, Some(1));
    assert_eq!(pointer.episode, Some(1));
}

#[test]
fn at_or_over_90_percent_advances_to_next_episode() {
    let (service, player, _, resume, clock) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    clock.set(10_000);
    player.report(status(PlaybackState::Playing, 90_000));
    service.tick();

    let pointer = resume
        .get("P", "https://show/1")
        .expect("get")
        .expect("present");
    assert_eq!(pointer.media_key, "ep2");
    assert_eq!(pointer.season, Some(1));
    assert_eq!(pointer.episode, Some(2));
}

#[test]
fn movie_or_last_episode_at_90_percent_removes_resume_pointer() {
    let (service, player, _, resume, clock) = setup();
    let mut last_ep = item();
    last_ep.next_episode = None;
    service.start(last_ep, &link(), &[]).expect("starts");

    clock.set(5_000);
    player.report(status(PlaybackState::Playing, 50_000));
    service.tick();
    assert!(resume.get("P", "https://show/1").expect("get").is_some());

    clock.set(10_000);
    player.report(status(PlaybackState::Playing, 91_000));
    service.tick();
    assert!(
        resume.get("P", "https://show/1").expect("get").is_none(),
        "finished show without next episode leaves continue watching"
    );
}

#[test]
fn start_at_uses_the_given_position_instead_of_the_saved_one() {
    let (service, player, _, _, _) = setup();
    let stream = StreamSelection {
        link: &link(),
        subtitles: &[],
        start_position_ms: 77_000,
    };
    service.start_at(item(), &stream).expect("starts");
    assert_eq!(
        player.requests.lock().expect("lock")[0].start_position_ms,
        77_000
    );
}

#[test]
fn restarting_resumes_from_the_saved_position() {
    let (service, player, _, _, _) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    player.report(status(PlaybackState::Playing, 42_000));
    service.tick();
    service.start(item(), &link(), &[]).expect("starts again");
    assert_eq!(
        player.requests.lock().expect("lock")[1].start_position_ms,
        42_000
    );
}

#[test]
fn finished_items_restart_from_the_beginning() {
    let (service, player, _, _, _) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    player.report(status(PlaybackState::Ended, 100_000));
    service.tick();
    assert_eq!(service.resume_position_ms(&item()).expect("resume"), 0);
}

#[test]
fn the_players_detailed_log_follows_the_debug_switch_both_ways() {
    let (service, player, ..) = setup();
    assert!(!player.verbose_log.load(std::sync::atomic::Ordering::SeqCst));
    service.set_verbose_player_log(true);
    assert!(player.verbose_log.load(std::sync::atomic::Ordering::SeqCst));
    service.set_verbose_player_log(false);
    assert!(!player.verbose_log.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn set_cache_limits_forwards_to_the_player() {
    let (service, player, ..) = setup();
    service
        .set_cache_limits(player_api::CacheLimits {
            ahead_bytes: 60,
            behind_bytes: 20,
        })
        .expect("limits");
    assert_eq!(*player.calls.lock().expect("lock"), ["cache 60 20"]);
}

#[test]
fn every_control_reaches_the_player_in_order() {
    let (service, player, ..) = setup();
    assert!(
        !service.is_embedded(),
        "a player that draws into a window is not embedded"
    );
    service.set_paused(true).expect("pause");
    service.seek_to(42_000).expect("seek");
    service.set_volume(80).expect("volume");
    service
        .select_track(player_api::TrackKind::Subtitle, Some(3))
        .expect("track");
    service.set_surface_size(1280, 720);
    service.set_preferred_audio(Some(player_api::ENGLISH));
    service.set_preferred_subtitles(&[player_api::ENGLISH]);
    service.add_subtitles(&[SubtitleTrack {
        lang: "Arabic".into(),
        url: "https://s.test/ar.vtt".into(),
        headers: None,
    }]);
    assert_eq!(
        *player.calls.lock().expect("lock"),
        [
            "paused true",
            "seek 42000",
            "volume 80",
            "track Subtitle Some(3)",
            "surface 1280x720",
            "audio Some(\"English\")",
            "subtitles [\"English\"]",
            "subtitle Arabic https://s.test/ar.vtt",
        ]
    );
}

#[test]
fn a_control_the_player_refuses_is_reported_to_the_caller() {
    let (service, player, ..) = setup();
    *player.failing.lock().expect("lock") = Some("not loaded".into());
    let answers = [
        ("seek", service.seek_to(1)),
        ("pause", service.set_paused(false)),
        ("volume", service.set_volume(1)),
        (
            "track",
            service.select_track(player_api::TrackKind::Audio, None),
        ),
        ("stop", service.stop()),
    ];
    for (control, answer) in answers {
        assert!(
            matches!(answer, Err(ServiceError::Player(_))),
            "{control}: {answer:?}"
        );
    }
}

#[test]
fn stopping_saves_where_the_viewer_stopped_and_forgets_the_stream() {
    let (service, player, progress, _, clock) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    // Below the save interval: only the stop itself can have saved it.
    player.report(status(PlaybackState::Playing, 3_000));
    service.stop().expect("stops");
    let saved = |progress: &WatchProgressStore| {
        progress
            .get("P", "ep1")
            .expect("get")
            .expect("saved")
            .position_ms
    };
    assert_eq!(saved(&progress), 3_000);
    assert_eq!(
        player
            .calls
            .lock()
            .expect("lock")
            .last()
            .map(String::as_str),
        Some("stop")
    );

    clock.set(60_000);
    player.report(status(PlaybackState::Playing, 50_000));
    service.tick();
    assert_eq!(
        saved(&progress),
        3_000,
        "a stopped stream is not saved again"
    );
}

/// A GL target that has nothing to draw.
struct NoPicture;

impl player_api::GlVideoTarget for NoPicture {
    fn attach(&self, _loader: player_api::GlLoader<'_>) -> Result<(), player_api::PlayerError> {
        Ok(())
    }
    fn frame_pending(&self) -> bool {
        false
    }
    fn render(
        &self,
        _width: u32,
        _height: u32,
    ) -> Result<Option<player_api::GlPicture>, player_api::PlayerError> {
        Ok(None)
    }
    fn detach(&self) {}
}

#[test]
fn the_players_answers_are_passed_through() {
    let (service, player, ..) = setup();
    let frame = Arc::new(player_api::VideoFrame {
        width: 2,
        height: 1,
        rgba: vec![0; 8],
        serial: 4,
    });
    *player.answers.lock().expect("lock") = crate::test_support::PlayerAnswers {
        tracks: vec![player_api::MediaTrack {
            id: 2,
            kind: player_api::TrackKind::Audio,
            language: "eng".into(),
            title: "Main".into(),
            selected: true,
            external: false,
        }],
        chapters: vec![player_api::Chapter {
            title: "Opening".into(),
            time_ms: 0,
        }],
        pending_subtitles: 3,
        embedded: true,
        frame: Some(Arc::clone(&frame)),
        gl_target: Some(Arc::new(NoPicture)),
    };
    player.report(status(PlaybackState::Paused, 9_000));
    assert_eq!(service.status().position_ms, 9_000);
    assert_eq!(service.pending_subtitles(), 3);
    assert_eq!(service.tracks().first().map(|t| t.id), Some(2));
    assert_eq!(
        service.chapters().first().map(|c| c.title.as_str()),
        Some("Opening")
    );
    assert!(service.is_embedded());
    assert_eq!(service.latest_frame(0).map(|f| f.serial), Some(4));
    assert!(
        service.latest_frame(4).is_none(),
        "the frame already drawn is not sent again"
    );
    assert!(service.gl_target().is_some());
}

#[test]
fn resuming_partway_saves_again_only_once_it_moved_enough_from_there() {
    let (service, player, progress, _, clock) = setup();
    let stream = StreamSelection {
        link: &link(),
        subtitles: &[],
        start_position_ms: 30_000,
    };
    service.start_at(item(), &stream).expect("starts");
    clock.set(2_000);
    player.report(status(PlaybackState::Playing, 31_000));
    service.tick();
    assert!(
        progress.get("P", "ep1").expect("get").is_none(),
        "one second after the resume point is not worth a save"
    );
    player.report(status(PlaybackState::Playing, 36_000));
    service.tick();
    assert_eq!(
        progress
            .get("P", "ep1")
            .expect("get")
            .map(|p| p.position_ms),
        Some(36_000)
    );
}
