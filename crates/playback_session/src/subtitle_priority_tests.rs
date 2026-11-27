//! Unit and concurrency tests for [`CheckGate`] and [`evaluate_gate`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn evaluate_gate_runs_in_normal_mode_or_once_search_is_done() {
    let now = Instant::now();
    let grace = Duration::from_secs(8);
    assert_eq!(
        evaluate_gate(CheckPriority::Normal, false, now, grace),
        GateDecision::Run
    );
    assert_eq!(
        evaluate_gate(CheckPriority::SubtitlesPinned, true, now, grace),
        GateDecision::Run
    );
    assert_eq!(
        evaluate_gate(
            CheckPriority::SubtitlesAuto { started_at: now },
            true,
            now,
            grace
        ),
        GateDecision::Run
    );
}

#[test]
fn evaluate_gate_pauses_auto_until_grace_expires_and_holds_pinned_indefinitely() {
    let start = Instant::now();
    let grace = Duration::from_secs(8);
    assert_eq!(
        evaluate_gate(
            CheckPriority::SubtitlesAuto { started_at: start },
            false,
            start + Duration::from_secs(3),
            grace,
        ),
        GateDecision::Pause(Some(Duration::from_secs(5)))
    );
    assert_eq!(
        evaluate_gate(
            CheckPriority::SubtitlesAuto { started_at: start },
            false,
            start + Duration::from_secs(8),
            grace,
        ),
        GateDecision::Run
    );
    assert_eq!(
        evaluate_gate(
            CheckPriority::SubtitlesPinned,
            false,
            start + Duration::from_secs(60),
            grace,
        ),
        GateDecision::Pause(None)
    );
}

#[test]
fn signal_subtitles_satisfied_resumes_auto_but_respects_manual_pin() {
    let gate = CheckGate::default();
    gate.prioritize_subtitles_auto();
    assert!(matches!(gate.mode(), CheckPriority::SubtitlesAuto { .. }));
    gate.signal_subtitles_satisfied();
    assert_eq!(gate.mode(), CheckPriority::Normal);

    gate.pin_subtitles_focus();
    gate.prioritize_subtitles_auto();
    assert_eq!(gate.mode(), CheckPriority::SubtitlesPinned);
    gate.signal_subtitles_satisfied();
    assert_eq!(gate.mode(), CheckPriority::SubtitlesPinned);
    gate.resume_checks_now();
    assert_eq!(gate.mode(), CheckPriority::Normal);
}

#[test]
fn wait_until_runnable_wakes_on_grace_expiry_satisfaction_and_cancel() {
    let gate = Arc::new(CheckGate::default());
    gate.prioritize_subtitles_auto();
    let start = Instant::now();
    assert!(gate.wait_until_runnable(Duration::from_millis(40)));
    assert!(start.elapsed() >= Duration::from_millis(30));

    let gate_cancel = Arc::new(CheckGate::default());
    gate_cancel.pin_subtitles_focus();
    let woke = Arc::new(AtomicBool::new(false));
    let (g_t, w_t) = (Arc::clone(&gate_cancel), Arc::clone(&woke));
    let worker = std::thread::spawn(move || {
        let allowed = g_t.wait_until_runnable(Duration::from_secs(30));
        w_t.store(true, Ordering::SeqCst);
        allowed
    });
    std::thread::sleep(Duration::from_millis(20));
    assert!(!woke.load(Ordering::SeqCst));
    gate_cancel.cancel();
    assert!(!worker.join().expect("worker joined"));
}
