use super::*;

fn sources() -> ActiveSources {
    ActiveSources::new(
        PlaybackItem::default(),
        vec![PlayableLink::default(), PlayableLink::default()],
        Vec::new(),
    )
}

#[test]
fn empty_until_set_and_empty_again_after_clear() {
    let session = SourceSession::default();
    assert!(session.snapshot().is_none());
    session.set(sources());
    assert!(session.snapshot().is_some());
    session.clear();
    assert!(session.snapshot().is_none());
}

#[test]
fn new_sources_start_unchecked_with_the_first_one_playing() {
    let fresh = sources();
    assert_eq!(fresh.health, [SourceHealth::Checking; 2]);
    assert_eq!(fresh.selected, 0);
}

#[test]
fn mark_selected_updates_the_playing_index() {
    let session = SourceSession::default();
    session.set(sources());
    session.mark_selected(1);
    assert_eq!(session.snapshot().map(|active| active.selected), Some(1));
}

#[test]
fn remembered_position_ignores_zero_from_a_failed_stream() {
    let session = SourceSession::default();
    session.set(sources());
    session.remember_position(95_000);
    session.remember_position(0);
    assert_eq!(
        session.snapshot().map(|active| active.position_ms),
        Some(95_000)
    );
}

#[test]
fn sizes_are_kept_for_the_current_generation_only() {
    let session = SourceSession::default();
    let first = session.set(sources());
    assert!(session.set_size(first, 1, 1_400_000_000));
    assert_eq!(
        session.snapshot().map(|active| active.sizes),
        Some(vec![None, Some(1_400_000_000)])
    );
    let second = session.set(sources());
    assert!(!session.set_size(first, 0, 5), "stale result");
    assert!(session.set_size(second, 0, 5));
}

#[test]
fn mark_selected_without_playback_does_nothing() {
    let session = SourceSession::default();
    session.mark_selected(3);
    assert!(session.snapshot().is_none());
}

#[test]
fn health_is_recorded_only_for_the_current_generation() {
    let session = SourceSession::default();
    let first = session.set(sources());
    assert!(session.set_health(first, 0, SourceHealth::Broken));
    let second = session.set(sources());
    assert!(
        !session.set_health(first, 1, SourceHealth::Working),
        "stale result"
    );
    assert!(session.set_health(second, 1, SourceHealth::Working));
    let health = session.snapshot().map(|active| active.health);
    assert_eq!(
        health,
        Some(vec![SourceHealth::Checking, SourceHealth::Working])
    );
    assert!(session.is_current(second) && !session.is_current(first));
    session.clear();
    assert!(!session.is_current(second), "closed");
    assert!(
        !session.set_health(second, 0, SourceHealth::Working),
        "nothing playing"
    );
}

#[test]
fn streams_found_later_are_added_after_the_ones_before_them() {
    let session = SourceSession::default();
    let generation = session.set(ActiveSources::new(
        Default::default(),
        Vec::new(),
        Vec::new(),
    ));
    let link = |name: &str| PlayableLink {
        name: name.to_owned(),
        ..PlayableLink::default()
    };
    assert_eq!(
        session.append(generation, vec![link("a")], Vec::new()),
        Some(0)
    );
    assert_eq!(
        session.append(generation, vec![link("b"), link("c")], Vec::new()),
        Some(1)
    );
    let active = session.snapshot().expect("open");
    assert_eq!(active.links.len(), 3);
    assert_eq!(active.health, [SourceHealth::Checking; 3]);
    assert_eq!(active.sizes, [None; 3]);
    assert_eq!(
        session.append(generation + 1, vec![link("x")], Vec::new()),
        None
    );
}

#[test]
fn late_subtitles_go_to_the_player_once_and_only_after_playback_started() {
    let session = SourceSession::default();
    let generation = session.set(sources());
    let subtitle = |lang: &str| SubtitleTrack {
        lang: lang.to_owned(),
        ..SubtitleTrack::default()
    };
    session.append(generation, Vec::new(), vec![subtitle("en")]);
    assert!(
        session.unsent_subtitles(generation).is_empty(),
        "not playing yet"
    );
    assert_eq!(session.start_snapshot().map(|a| a.subtitles.len()), Some(1));
    session.mark_selected(0);
    session.append(generation, Vec::new(), vec![subtitle("fr"), subtitle("de")]);
    let fresh = session.unsent_subtitles(generation);
    assert_eq!(fresh.len(), 2, "the English one was given at the start");
    assert!(session.unsent_subtitles(generation).is_empty(), "only once");
}
