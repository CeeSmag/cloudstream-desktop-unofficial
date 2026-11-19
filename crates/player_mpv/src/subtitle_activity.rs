//! Tracks in-flight background `sub-add` work and notifies the event thread when tracks change.
//!
//! Counts are tagged with the stream generation they belong to, so a worker finishing for a
//! replaced stream never decrements or marks dirty for the newer stream.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

#[derive(Debug, Default)]
struct AttachingState {
    generation: u64,
    count: usize,
}

/// Shared between `MpvPlayer`, `EventWorker`, and `subtitle_attacher`.
#[derive(Debug, Default)]
pub(crate) struct SubtitleActivity {
    attaching: Mutex<AttachingState>,
    tracks_dirty: AtomicBool,
}

impl SubtitleActivity {
    /// A new stream started (or playback stopped): forgets the old stream's attaching count.
    pub(crate) fn reset(&self, generation: u64) {
        let mut state = self
            .attaching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.generation = generation;
        state.count = 0;
        self.tracks_dirty.store(false, Ordering::SeqCst);
    }

    /// Reserves `files` attaching slots for stream `generation`.
    pub(crate) fn begin(&self, generation: u64, files: usize) {
        if files == 0 {
            return;
        }
        let mut state = self
            .attaching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.generation == generation {
            state.count = state.count.saturating_add(files);
        } else if generation > state.generation {
            state.generation = generation;
            state.count = files;
        }
    }

    /// Marks one file of `generation` finished; when `attached`, requests an immediate track refresh.
    pub(crate) fn finish_one(&self, generation: u64, attached: bool) {
        let mut state = self
            .attaching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.generation != generation {
            return;
        }
        state.count = state.count.saturating_sub(1);
        if attached {
            self.tracks_dirty.store(true, Ordering::SeqCst);
        }
    }

    /// Releases `remaining` un-tried slots when a batch stops early.
    pub(crate) fn finish_remaining(&self, generation: u64, remaining: usize) {
        if remaining == 0 {
            return;
        }
        let mut state = self
            .attaching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.generation == generation {
            state.count = state.count.saturating_sub(remaining);
        }
    }

    /// Number of external subtitle files currently attaching for stream `generation`.
    pub(crate) fn count_for(&self, generation: u64) -> usize {
        let state = self
            .attaching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.generation == generation {
            state.count
        } else {
            0
        }
    }

    /// Returns `true` once if a subtitle finished attaching since the last check.
    pub(crate) fn take_tracks_dirty(&self) -> bool {
        self.tracks_dirty.swap(false, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn attaching_count_tracks_current_generation_and_marks_dirty_on_attach() {
        let activity = SubtitleActivity::default();
        activity.reset(1);
        activity.begin(1, 3);
        assert_eq!(activity.count_for(1), 3);
        assert!(!activity.take_tracks_dirty());

        activity.finish_one(1, false);
        assert_eq!(activity.count_for(1), 2);
        assert!(!activity.take_tracks_dirty());

        activity.finish_one(1, true);
        assert_eq!(activity.count_for(1), 1);
        assert!(activity.take_tracks_dirty());
        assert!(!activity.take_tracks_dirty(), "consumed once");

        activity.finish_remaining(1, 1);
        assert_eq!(activity.count_for(1), 0);
    }

    #[test]
    fn stale_generation_completion_never_touches_newer_stream() {
        let activity = SubtitleActivity::default();
        activity.reset(1);
        activity.begin(1, 4);

        // Stream switches to generation 2 before generation 1's workers finish.
        activity.reset(2);
        activity.begin(2, 2);

        activity.finish_one(1, true);
        activity.finish_remaining(1, 3);
        assert_eq!(activity.count_for(2), 2);
        assert!(
            !activity.take_tracks_dirty(),
            "stale worker must not dirty newer stream"
        );
        assert_eq!(activity.count_for(1), 0);
    }

    #[test]
    fn concurrent_switches_and_completions_never_leak_or_corrupt_latest_stream() {
        let activity = Arc::new(SubtitleActivity::default());
        let generation = Arc::new(AtomicU64::new(0));
        let mut workers = Vec::new();
        for switch in 1u64..=400 {
            generation.store(switch, Ordering::SeqCst);
            activity.reset(switch);
            activity.begin(switch, 3);
            let act = Arc::clone(&activity);
            workers.push(std::thread::spawn(move || {
                act.finish_one(switch, true);
                act.finish_one(switch, false);
                act.finish_remaining(switch, 1);
            }));
        }
        for handle in workers {
            handle.join().expect("worker joined");
        }
        let latest = generation.load(Ordering::SeqCst);
        assert_eq!(activity.count_for(latest), 0);
    }
}
