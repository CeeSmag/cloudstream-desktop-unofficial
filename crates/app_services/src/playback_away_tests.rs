//! Leaving the player and coming back: progress saved, paused with a smaller cache, then the
//! full cache and playing again (T1 against the fake player).

use std::sync::Arc;

use core_model::PlayableLink;
use player_api::{CacheLimits, PlaybackState, PlaybackStatus, VideoPlayer};
use storage::{Database, ResumeStore, WatchProgressStore};

use crate::test_support::{FakePlayer, FixedClock};
use crate::{Clock, PlaybackItem, PlaybackService};

fn setup() -> (PlaybackService, Arc<FakePlayer>, WatchProgressStore) {
    let player = Arc::new(FakePlayer::default());
    let database = Arc::new(Database::open_in_memory().expect("memory db"));
    let progress = WatchProgressStore::new(Arc::clone(&database));
    let service = PlaybackService::new(
        Arc::clone(&player) as Arc<dyn VideoPlayer>,
        progress.clone(),
        ResumeStore::new(database),
        Arc::new(FixedClock::at(1_000)) as Arc<dyn Clock>,
    );
    (service, player, progress)
}

fn item() -> PlaybackItem {
    PlaybackItem {
        provider: "P".into(),
        media_key: "ep1".into(),
        title: "Episode 1".into(),
        detail_url: "https://show/1".into(),
        ..PlaybackItem::default()
    }
}

const AWAY: CacheLimits = CacheLimits {
    ahead_bytes: 30,
    behind_bytes: 10,
};
const WATCHING: CacheLimits = CacheLimits {
    ahead_bytes: 60,
    behind_bytes: 20,
};

fn calls(player: &FakePlayer) -> Vec<String> {
    player.calls.lock().expect("lock").clone()
}

#[test]
fn leaving_saves_the_progress_pauses_and_shrinks_the_cache_and_keeps_the_stream() {
    let (service, player, progress) = setup();
    service
        .start(item(), &PlayableLink::default(), &[])
        .expect("starts");
    player.report(PlaybackStatus {
        state: PlaybackState::Playing,
        position_ms: 42_000,
        duration_ms: 600_000,
        ..PlaybackStatus::default()
    });

    service.leave(AWAY).expect("left");

    let saved = progress.get("P", "ep1").expect("read").expect("saved");
    assert_eq!(saved.position_ms, 42_000);
    assert_eq!(calls(&player), ["paused true", "cache 30 10"]);
    assert_eq!(
        player.requests.lock().expect("lock").len(),
        1,
        "not opened again"
    );
}

#[test]
fn coming_back_grows_the_cache_first_and_then_plays() {
    let (service, player, _) = setup();
    service.come_back(WATCHING).expect("back");
    assert_eq!(calls(&player), ["cache 60 20", "paused false"]);
}
