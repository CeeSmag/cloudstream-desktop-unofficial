//! Closing a video that plays, and opening and closing many in a row.

use super::*;

#[test]
fn closing_a_playing_video_stops_its_search_and_checks() {
    let script = SearchScript::steady(80, Duration::from_millis(100), 480, Duration::from_secs(1));
    let rig = rig(script, grace(50, 100, 1_000));
    let (outcome, background, _) = open(&rig);
    assert_eq!(outcome, Ok(()));
    assert_eq!(rig.searches.stats().active, 1);

    rig.sources.close();

    join_within(background, TIME_LIMIT);
    let stats = rig.searches.stats();
    assert_eq!(
        (stats.cancelled, stats.active),
        (1, 0),
        "the search was not stopped"
    );
    assert!(rig.sources.session().snapshot().is_none());
}

/// A viewer who opens and leaves fifty videos in a row: every search ends, every thread ends, and
/// nothing is left in the provider.
#[test]
fn fifty_videos_opened_and_closed_in_a_row_leave_nothing_running() {
    const VIDEOS: u64 = 50;
    let script = SearchScript::steady(
        20,
        Duration::from_millis(10),
        720,
        Duration::from_millis(200),
    );
    let rig = rig(script, grace(10, 20, 500));
    let mut every_thread = Vec::new();
    for _ in 0..VIDEOS {
        let (outcome, background, _) = open(&rig);
        assert_eq!(outcome, Ok(()));
        rig.sources.close();
        every_thread.extend(background);
    }

    join_within(every_thread, TIME_LIMIT);

    let stats = rig.searches.stats();
    assert_eq!(stats.started, VIDEOS);
    assert_eq!(
        stats.active, 0,
        "{} searches were left running",
        stats.active
    );
    assert_eq!(rig.player.played.lock().expect("lock").len(), 50);
    assert!(rig.sources.session().snapshot().is_none());
}
