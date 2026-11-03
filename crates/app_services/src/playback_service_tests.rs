use storage::Database;

use super::*;
use crate::test_support::{FakePlayer, FixedClock};

fn setup() -> (
    PlaybackService,
    Arc<FakePlayer>,
    WatchProgressStore,
    Arc<FixedClock>,
) {
    let player = Arc::new(FakePlayer::default());
    let progress =
        WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
    let clock = Arc::new(FixedClock::at(1_000));
    let service = PlaybackService::new(
        Arc::clone(&player) as Arc<dyn VideoPlayer>,
        progress.clone(),
        Arc::clone(&clock) as Arc<dyn Clock>,
    );
    (service, player, progress, clock)
}

fn item() -> PlaybackItem {
    PlaybackItem {
        provider: "P".into(),
        media_key: "ep1".into(),
        title: "Episode 1".into(),
        ..PlaybackItem::default()
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
    }
}

#[test]
fn start_sends_url_referer_and_subtitles_to_the_player() {
    let (service, player, _, _) = setup();
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
    let (service, player, progress, clock) = setup();
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
fn start_at_uses_the_given_position_instead_of_the_saved_one() {
    let (service, player, _, _) = setup();
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
    let (service, player, _, _) = setup();
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
    let (service, player, _, _) = setup();
    service.start(item(), &link(), &[]).expect("starts");
    player.report(status(PlaybackState::Ended, 100_000));
    service.tick();
    assert_eq!(service.resume_position_ms(&item()).expect("resume"), 0);
}
