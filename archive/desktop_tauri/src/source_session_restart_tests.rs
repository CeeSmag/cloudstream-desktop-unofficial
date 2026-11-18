//! The list of a refreshed video, and subtitles that are added once (T1, and a race test).

use std::sync::{Arc, Barrier};

use super::*;

fn link(url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        ..PlayableLink::default()
    }
}

fn track(url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: "English".to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

/// A video with three streams of which the second plays, and one subtitle the player has.
fn playing_second(session: &SourceSession) -> u64 {
    let mut sources = ActiveSources::new(
        PlaybackItem::default(),
        vec![link("a"), link("b"), link("c")],
        vec![track("/subs/en.vtt")],
    );
    sources.sizes = vec![None, Some(7), None];
    sources.subtitles_sent = 1;
    let generation = session.set(sources);
    session.mark_selected(1);
    session.remember_position(42_000);
    generation
}

#[test]
fn restarting_gives_a_new_generation_so_the_old_lists_work_stops() {
    let session = SourceSession::default();
    let old = playing_second(&session);
    let fresh = session.restart_playing(old).expect("restarted");
    assert_ne!(fresh, old);
    assert!(!session.is_current(old));
    assert!(session.is_current(fresh));
}

#[test]
fn restarting_keeps_only_the_stream_that_plays_with_what_was_known_about_it() {
    let session = SourceSession::default();
    let fresh = session
        .restart_playing(playing_second(&session))
        .expect("restarted");
    let active = session.snapshot().expect("open");
    let urls: Vec<&str> = active.links.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(urls, ["b"]);
    assert_eq!((active.selected, active.playing), (0, true));
    assert_eq!(active.health, [SourceHealth::Working]);
    assert_eq!(active.sizes, [Some(7)]);
    assert_eq!(active.position_ms, 42_000);
    assert_eq!(active.subtitles.len(), 1);
    assert!(
        session.unsent_subtitles(fresh).is_empty(),
        "the player already has the subtitle"
    );
}

#[test]
fn nothing_is_restarted_while_no_stream_plays_or_for_another_video() {
    let session = SourceSession::default();
    let old = playing_second(&session);
    session.start_snapshot(); // a switch is under way: nothing plays for a moment
    assert_eq!(session.restart_playing(old), None);
    session.mark_selected(1);
    assert_eq!(
        session.restart_playing(old + 100),
        None,
        "not the open video"
    );
    session.clear();
    assert_eq!(session.restart_playing(old), None, "closed");
}

#[test]
fn a_subtitle_file_already_in_the_list_is_not_added_again() {
    let session = SourceSession::default();
    let generation = playing_second(&session);
    assert_eq!(
        session.add_subtitle(generation, track("/subs/en.vtt")),
        Some(false)
    );
    assert_eq!(
        session.add_subtitle(generation, track("/subs/ar.vtt")),
        Some(true)
    );
    assert_eq!(session.add_subtitle(generation + 1, track("/x")), None);
    let given = session.unsent_subtitles(generation);
    assert_eq!(given.len(), 1, "only the new file goes to the player");
}

/// Two deliveries hand the same files over at the same time: each file is in the list once.
#[test]
fn race_the_same_subtitle_added_from_two_threads_is_listed_once() {
    const FILES: usize = 500;
    let session = Arc::new(SourceSession::default());
    let generation = playing_second(&session);
    let start = Arc::new(Barrier::new(2));
    let adders: Vec<_> = (0..2)
        .map(|_| {
            let (session, start) = (Arc::clone(&session), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                (0..FILES)
                    .filter(|n| {
                        session.add_subtitle(generation, track(&format!("/subs/{n}.vtt")))
                            == Some(true)
                    })
                    .count()
            })
        })
        .collect();
    let added: usize = adders
        .into_iter()
        .map(|adder| adder.join().expect("adder panicked"))
        .sum();
    assert_eq!(added, FILES, "each file added exactly once");
    let listed = session.snapshot().expect("open").subtitles.len();
    assert_eq!(listed, FILES + 1);
}
