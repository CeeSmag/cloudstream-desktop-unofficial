use storage::Database;

use super::*;
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
