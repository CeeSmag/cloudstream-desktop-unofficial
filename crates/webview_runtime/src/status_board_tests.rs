//! T1 and race tests for [`StatusBoard`].

use std::sync::{Arc, Barrier};
use std::thread;

use super::*;

fn installed(version: &str) -> RuntimeStatus {
    RuntimeStatus::Installed {
        version: version.into(),
    }
}

#[test]
fn nothing_is_known_before_the_first_answer() {
    let board = StatusBoard::new();
    let _ticket = board.begin_probe();
    assert_eq!(board.current(), None);
}

#[test]
fn the_newest_probe_writes() {
    let board = StatusBoard::new();
    let ticket = board.begin_probe();
    assert!(board.finish_probe(ticket, RuntimeStatus::Missing));
    assert_eq!(board.current(), Some(RuntimeStatus::Missing));
}

#[test]
fn a_stale_probe_loses_to_a_newer_one_in_either_order() {
    let board = StatusBoard::new();
    let launch = board.begin_probe();
    let after_install = board.begin_probe();
    assert!(board.finish_probe(after_install, installed("1.0.0.1")));
    assert!(!board.finish_probe(launch, RuntimeStatus::Missing));
    assert_eq!(board.current(), Some(installed("1.0.0.1")));

    let board = StatusBoard::new();
    let launch = board.begin_probe();
    let after_install = board.begin_probe();
    assert!(!board.finish_probe(launch, RuntimeStatus::Missing));
    assert_eq!(board.current(), None);
    assert!(board.finish_probe(after_install, installed("1.0.0.1")));
}

/// The launch probe reads "missing", then the install's probe begins, answers, and only then the
/// launch probe hands its (now stale) answer in. Barriers fix that order on two real threads.
#[test]
fn race_launch_probe_answering_after_the_install_probe_is_discarded() {
    for _ in 0..200 {
        let board = Arc::new(StatusBoard::new());
        let launch_ticket = board.begin_probe();
        let read_done = Arc::new(Barrier::new(2));
        let install_done = Arc::new(Barrier::new(2));
        let launch = {
            let (board, read_done, install_done) = (
                Arc::clone(&board),
                Arc::clone(&read_done),
                Arc::clone(&install_done),
            );
            thread::spawn(move || {
                read_done.wait();
                install_done.wait();
                board.finish_probe(launch_ticket, RuntimeStatus::Missing)
            })
        };
        read_done.wait();
        let install_ticket = board.begin_probe();
        assert!(board.finish_probe(install_ticket, installed("2.0.0.1")));
        install_done.wait();
        assert!(!launch.join().unwrap());
        assert_eq!(board.current(), Some(installed("2.0.0.1")));
    }
}

/// Many probes begin and answer at once; the probe that began last must always have written, and
/// the board must end on its answer whatever order the others answered in.
#[test]
fn race_only_the_last_begun_probe_can_have_the_final_word() {
    const THREADS: usize = 16;
    for round in 0..100 {
        let board = Arc::new(StatusBoard::new());
        let start = Arc::new(Barrier::new(THREADS));
        let workers: Vec<_> = (0..THREADS)
            .map(|index| {
                let (board, start) = (Arc::clone(&board), Arc::clone(&start));
                thread::spawn(move || {
                    start.wait();
                    let ticket = board.begin_probe();
                    thread::yield_now();
                    let wrote = board.finish_probe(ticket, installed(&format!("1.0.{index}.1")));
                    (ticket, wrote)
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        let last = results.iter().map(|(ticket, _)| ticket.0).max().unwrap();
        let (_, last_wrote) = results.iter().find(|(t, _)| t.0 == last).unwrap();
        assert!(
            *last_wrote,
            "round {round}: the last-begun probe must write"
        );
        let winner = results.iter().position(|(t, _)| t.0 == last).unwrap();
        assert_eq!(
            board.current(),
            Some(installed(&format!("1.0.{winner}.1"))),
            "round {round}"
        );
    }
}
