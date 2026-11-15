use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::entry_kind::EntryKind;

/// A deadlock must fail a test, not hang the run.
const TEST_LIMIT: Duration = Duration::from_secs(20);

const THREADS: usize = 8;
const ROUNDS: usize = 1_500;

fn spec(n: usize) -> StreamSpec {
    StreamSpec {
        url: format!("https://host.test/{n}/master.m3u8"),
        ..StreamSpec::default()
    }
}

/// Runs `body` on its own thread and fails when it has not finished within [`TEST_LIMIT`].
fn finishes_within_limit(body: impl FnOnce() + Send + 'static) {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        body();
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(TEST_LIMIT).is_ok(),
        "the test body did not finish: a deadlock, or a panic in a worker"
    );
}

#[test]
fn open_the_first_stream_numbers_its_playlist_zero_and_resolves_by_token() {
    let registry = SessionRegistry::default();
    let session = registry.open(&spec(1)).expect("opens");
    let found = registry.lookup(session.token()).expect("is live");
    assert_eq!(found.token(), session.token());
    let first = session.entry(0).expect("number 0");
    assert_eq!(first.url, spec(1).url);
    assert_eq!(first.kind, EntryKind::Playlist);
}

#[test]
fn open_an_address_that_is_not_http_is_refused() {
    let registry = SessionRegistry::default();
    for url in ["file:///C:/secret.m3u8", "ftp://h/x.m3u8", "x.m3u8", ""] {
        let result = registry.open(&StreamSpec {
            url: url.to_owned(),
            ..StreamSpec::default()
        });
        assert!(
            matches!(result, Err(ProxyError::UnsupportedAddress)),
            "{url:?}"
        );
    }
}

#[test]
fn lookup_an_unknown_token_finds_nothing() {
    let registry = SessionRegistry::default();
    registry.open(&spec(1)).expect("opens");
    assert!(registry
        .lookup("00000000000000000000000000000000")
        .is_none());
    assert!(registry.lookup("").is_none());
}

#[test]
fn open_a_second_stream_keeps_the_first_until_a_third_opens() {
    let registry = SessionRegistry::default();
    let first = registry.open(&spec(1)).expect("opens");
    let second = registry.open(&spec(2)).expect("opens");
    assert!(
        registry.lookup(first.token()).is_some(),
        "the player still reads it"
    );
    assert!(!first.is_revoked());
    let third = registry.open(&spec(3)).expect("opens");
    assert!(registry.lookup(first.token()).is_none());
    assert!(
        first.is_revoked(),
        "a read in progress stops at its next chunk"
    );
    assert!(registry.lookup(second.token()).is_some());
    assert!(registry.lookup(third.token()).is_some());
}

#[test]
fn a_read_that_arrives_after_the_switch_is_still_served_by_the_old_stream() {
    // Video A is open and its playlist request is held up; the viewer switches to B. The held
    // request must still find A, otherwise the player sees a failed read and starts a fallback.
    let registry = SessionRegistry::default();
    let a = registry.open(&spec(1)).expect("opens");
    let held = registry.lookup(a.token()).expect("found before the switch");
    registry.open(&spec(2)).expect("opens");
    assert!(!held.is_revoked());
    assert!(registry.lookup(a.token()).is_some());
}

#[test]
fn close_revokes_every_stream_including_the_ones_in_use() {
    let registry = SessionRegistry::default();
    let first = registry.open(&spec(1)).expect("opens");
    let second = registry.open(&spec(2)).expect("opens");
    registry.close();
    assert!(first.is_revoked() && second.is_revoked());
    assert!(registry.lookup(first.token()).is_none());
    assert!(registry.lookup(second.token()).is_none());
}

#[test]
fn close_between_opening_and_the_first_request_leaves_nothing_to_serve() {
    let registry = SessionRegistry::default();
    let session = registry.open(&spec(1)).expect("opens");
    registry.close();
    assert!(registry.lookup(session.token()).is_none());
}

#[test]
fn open_after_close_starts_a_fresh_stream() {
    let registry = SessionRegistry::default();
    let before = registry.open(&spec(1)).expect("opens");
    registry.close();
    let after = registry.open(&spec(2)).expect("opens");
    assert_ne!(before.token(), after.token());
    assert!(registry.lookup(after.token()).is_some());
    assert!(registry.lookup(before.token()).is_none());
}

#[test]
fn many_threads_opening_never_hand_a_token_to_the_wrong_stream_and_at_most_two_stay_live() {
    finishes_within_limit(|| {
        let registry = Arc::new(SessionRegistry::default());
        let workers: Vec<_> = (0..THREADS)
            .map(|worker| {
                let registry = Arc::clone(&registry);
                std::thread::spawn(move || {
                    let mut opened = Vec::new();
                    for round in 0..ROUNDS {
                        let session = registry
                            .open(&spec(worker * ROUNDS + round))
                            .expect("opens");
                        if let Some(found) = registry.lookup(session.token()) {
                            assert_eq!(found.token(), session.token(), "wrong owner");
                        }
                        opened.push(session);
                    }
                    opened
                })
            })
            .collect();
        let all: Vec<_> = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker finished"))
            .collect();
        let tokens: HashSet<&str> = all.iter().map(|session| session.token()).collect();
        assert_eq!(tokens.len(), THREADS * ROUNDS, "every token is unique");
        let live = all.iter().filter(|session| !session.is_revoked()).count();
        assert_eq!(live, 2, "exactly the newest and the replaced one stay live");
        for session in &all {
            let served = registry.lookup(session.token()).is_some();
            assert_eq!(
                served,
                !session.is_revoked(),
                "the flag and the lookup agree"
            );
        }
    });
}

#[test]
fn opening_closing_and_looking_up_at_once_leave_nothing_live_after_the_last_close() {
    finishes_within_limit(|| {
        let registry = Arc::new(SessionRegistry::default());
        let openers: Vec<_> = (0..4)
            .map(|worker| {
                let registry = Arc::clone(&registry);
                std::thread::spawn(move || {
                    (0..ROUNDS)
                        .map(|round| {
                            registry
                                .open(&spec(worker * ROUNDS + round))
                                .expect("opens")
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let closer = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || (0..ROUNDS).for_each(|_| registry.close()))
        };
        let looker = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                (0..ROUNDS).for_each(|n| {
                    registry.lookup(&format!("{n:032x}"));
                });
            })
        };
        let all: Vec<_> = openers
            .into_iter()
            .flat_map(|worker| worker.join().expect("opener finished"))
            .collect();
        closer.join().expect("closer finished");
        looker.join().expect("looker finished");
        registry.close();
        for session in &all {
            assert!(session.is_revoked(), "a stream survived the last close");
            assert!(registry.lookup(session.token()).is_none());
        }
    });
}
