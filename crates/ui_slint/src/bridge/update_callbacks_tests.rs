use std::sync::atomic::AtomicUsize;

use super::*;

fn check(has_update: bool, asset: bool) -> UpdateCheck {
    UpdateCheck {
        current_version: "0.4.8".into(),
        latest_version: "0.5.0".into(),
        has_update,
        asset_name: asset.then(|| "CloudStream-0.5.0-x86_64.AppImage".to_owned()),
        asset_size: asset.then_some(142 * 1024 * 1024),
        release_name: Some("v0.5.0".into()),
        release_notes: Some("  - Faster playback\n  - Bug fixes  ".into()),
    }
}

#[test]
fn update_label_says_what_to_do() {
    assert!(update_label(&check(true, true)).starts_with("Version 0.5.0 is ready to install"));
    assert!(update_label(&check(true, false)).contains("not for this system"));
    assert_eq!(update_label(&check(false, true)), "Up to date (0.4.8).");
}

#[test]
fn update_badge_only_for_an_installable_newer_release() {
    assert_eq!(update_badge(&check(true, true)), "Update v0.5.0");
    assert_eq!(update_badge(&check(true, false)), "");
    assert_eq!(update_badge(&check(false, true)), "");
}

#[test]
fn build_update_view_state_carries_version_and_trimmed_notes() {
    let state = build_update_view_state(&check(true, true));
    assert_eq!(state.version, "v0.5.0 · 142.0 MB");
    assert_eq!(state.notes, "- Faster playback\n  - Bug fixes");
    assert!(state.can_apply);
    assert!(!state.busy);

    let up_to_date = build_update_view_state(&check(false, true));
    assert_eq!(up_to_date.version, "");
    assert_eq!(up_to_date.notes, "");
    assert!(!up_to_date.can_apply);
}

#[test]
fn update_gate_allows_at_most_one_active_worker_across_threads() {
    let gate = UpdateGate::default();
    let active = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));

    std::thread::scope(|scope| {
        for _ in 0..8 {
            let g = gate.clone();
            let a = Arc::clone(&active);
            let m = Arc::clone(&max_seen);
            scope.spawn(move || {
                for _ in 0..2000 {
                    if let Some(_guard) = g.try_enter() {
                        let now = a.fetch_add(1, Ordering::SeqCst) + 1;
                        m.fetch_max(now, Ordering::SeqCst);
                        a.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            });
        }
    });

    assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    assert!(gate.try_enter().is_some());
}
