//! Several threads against one failure, and closing while a source starts.

use super::*;

/// Eight polls see the same failure while the switch is still starting: one switches, the rest wait.
#[test]
fn many_callers_seeing_one_failure_switch_exactly_once() {
    const CALLERS: usize = 8;
    let rig = Arc::new(opened(6));
    *rig.player.start_takes.lock().expect("lock") = Duration::from_millis(300);
    let start = Arc::new(Barrier::new(CALLERS));
    let handles: Vec<JoinHandle<FallbackOutcome>> = (0..CALLERS)
        .map(|_| {
            let (rig, start) = (Arc::clone(&rig), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                fail(&rig, "broke")
            })
        })
        .collect();
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("caller"))
        .collect();
    let switched = outcomes
        .iter()
        .filter(|o| matches!(o, FallbackOutcome::Switched(_)))
        .count();
    let waiting = outcomes
        .iter()
        .filter(|o| **o == FallbackOutcome::Switching)
        .count();
    assert_eq!((switched, waiting), (1, CALLERS - 1), "{outcomes:?}");
    assert_eq!(played(&rig).len(), 2);
}

#[test]
fn closing_while_a_source_starts_leaves_nothing_open_and_does_not_hang() {
    let rig = Arc::new(opened(6));
    *rig.player.start_takes.lock().expect("lock") = Duration::from_millis(150);
    let closer = {
        let rig = Arc::clone(&rig);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            rig.sources.close();
        })
    };
    let switching = {
        let rig = Arc::clone(&rig);
        std::thread::spawn(move || fail(&rig, "broke"))
    };
    join_within(vec![closer], TIME_LIMIT);
    let _ = switching.join().expect("switch");
    assert!(rig.sources.session().snapshot().is_none());
    assert_eq!(fail(&rig, "late"), FallbackOutcome::Fine);
}
