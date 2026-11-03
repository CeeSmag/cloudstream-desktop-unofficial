//! Race tests for the list of streams of the open video: threads appending streams, answering their
//! checks, reading the list and taking the subtitles for the player all at once.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use app_services::PlaybackItem;
use core_model::{PlayableLink, SubtitleTrack};

use crate::race_tests::{join_within, TIME_LIMIT};
use crate::source_session::{ActiveSources, SourceHealth, SourceSession};

/// A repeatable pseudo-random number, so a failure can be reproduced.
fn pseudo_random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

const APPENDERS: usize = 3;
const BATCHES: usize = 150;

/// Threads that keep adding streams (three per batch) and one subtitle per batch.
fn spawn_appenders(
    session: &Arc<SourceSession>,
    generation: u64,
) -> Vec<std::thread::JoinHandle<()>> {
    (0..APPENDERS)
        .map(|appender| {
            let session = Arc::clone(session);
            std::thread::spawn(move || {
                for batch in 0..BATCHES {
                    let subtitle = SubtitleTrack {
                        url: format!("subtitle-{appender}-{batch}"),
                        ..SubtitleTrack::default()
                    };
                    session.append(generation, vec![PlayableLink::default(); 3], vec![subtitle]);
                }
            })
        })
        .collect()
}

/// Threads that answer checks, read the list and take subtitles for the player, until `done`.
fn spawn_background(
    session: &Arc<SourceSession>,
    generation: u64,
    done: &Arc<AtomicBool>,
    delivered: &Arc<Mutex<Vec<String>>>,
) -> Vec<std::thread::JoinHandle<()>> {
    (0..6u64)
        .map(|worker| {
            let (session, done, delivered) =
                (Arc::clone(session), Arc::clone(done), Arc::clone(delivered));
            std::thread::spawn(move || {
                let mut seed = 0x9E37_79B9_7F4A_7C15 ^ worker;
                while !done.load(Ordering::SeqCst) {
                    match worker % 3 {
                        0 => answer_some_stream(&session, generation, &mut seed),
                        1 => assert_lists_equal(&session),
                        _ => {
                            let fresh = session.unsent_subtitles(generation);
                            delivered
                                .lock()
                                .expect("lock")
                                .extend(fresh.into_iter().map(|t| t.url));
                        }
                    }
                }
            })
        })
        .collect()
}

fn every_subtitle_appended() -> Vec<String> {
    let mut urls: Vec<String> = (0..APPENDERS)
        .flat_map(|a| (0..BATCHES).map(move |b| format!("subtitle-{a}-{b}")))
        .collect();
    urls.sort();
    urls
}

#[test]
fn appending_answering_and_reading_streams_keeps_the_lists_equal_and_hands_each_subtitle_over_once()
{
    let session = Arc::new(SourceSession::default());
    let generation = session.set(ActiveSources::new(
        PlaybackItem::default(),
        Vec::new(),
        Vec::new(),
    ));
    session.mark_selected(0);
    let done = Arc::new(AtomicBool::new(false));
    let delivered = Arc::new(Mutex::new(Vec::<String>::new()));

    let appenders = spawn_appenders(&session, generation);
    let background = spawn_background(&session, generation, &done, &delivered);
    join_within(appenders, TIME_LIMIT);
    done.store(true, Ordering::SeqCst);
    join_within(background, TIME_LIMIT);

    let mut delivered = delivered.lock().expect("lock");
    delivered.extend(
        session
            .unsent_subtitles(generation)
            .into_iter()
            .map(|t| t.url),
    );
    delivered.sort();
    assert_eq!(
        *delivered,
        every_subtitle_appended(),
        "a subtitle was lost or handed over twice"
    );
    assert_lists_equal(&session);
}

fn answer_some_stream(session: &SourceSession, generation: u64, seed: &mut u64) {
    let Some(active) = session.snapshot() else {
        return;
    };
    if active.links.is_empty() {
        return;
    }
    let index =
        usize::try_from(pseudo_random(seed) % u64::try_from(active.links.len()).unwrap_or(1))
            .unwrap_or(0);
    session.set_health(generation, index, SourceHealth::Working);
    session.set_size(generation, index, 1_000);
    session.set_stream_subtitles(generation, index, vec!["eng".to_owned()]);
}

fn assert_lists_equal(session: &SourceSession) {
    if let Some(active) = session.snapshot() {
        let lengths = (
            active.links.len(),
            active.health.len(),
            active.sizes.len(),
            active.stream_subtitles.len(),
        );
        assert!(
            lengths.0 == lengths.1 && lengths.0 == lengths.2 && lengths.0 == lengths.3,
            "the lists of one video got out of step: {lengths:?}"
        );
    }
}
