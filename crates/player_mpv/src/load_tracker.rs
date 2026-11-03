//! Tells which stream's file has finished loading, from the order of mpv's events.
//!
//! Switching source while a file is still loading leaves events of the old file in the queue. They
//! arrive before the next file's `StartFile`, so a `FileLoaded` is credited to the stream that was
//! current when its own `StartFile` came, never to the stream that is current when it is read.

use crate::mpv_event::MpvEvent;

/// What the event thread knows about loading.
#[derive(Debug, Default)]
pub(crate) struct LoadTracker {
    started: Option<u64>,
    loaded: Option<u64>,
}

impl LoadTracker {
    /// Takes one event; `generation` is the stream generation current when it was read.
    pub(crate) fn apply(&mut self, event: MpvEvent, generation: u64) {
        match event {
            MpvEvent::StartFile => {
                self.started = Some(generation);
                self.loaded = None;
            }
            MpvEvent::FileLoaded => self.loaded = self.started.or(Some(generation)),
            MpvEvent::EndFile(_) => self.loaded = None,
            _ => {}
        }
    }

    /// The generation of the stream whose file is loaded, if one is.
    pub(crate) fn loaded(&self) -> Option<u64> {
        self.loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv_event::EndReason;

    const STOPPED: MpvEvent = MpvEvent::EndFile(EndReason::Stopped);

    #[test]
    fn a_stream_is_loaded_once_its_file_has_loaded() {
        let mut tracker = LoadTracker::default();
        tracker.apply(MpvEvent::StartFile, 1);
        assert_eq!(tracker.loaded(), None, "still loading");
        tracker.apply(MpvEvent::FileLoaded, 1);
        assert_eq!(tracker.loaded(), Some(1));
    }

    #[test]
    fn a_late_loaded_event_of_the_old_file_does_not_mark_the_new_stream_as_loaded() {
        let mut tracker = LoadTracker::default();
        tracker.apply(MpvEvent::StartFile, 1);
        // The viewer switches source (generation 2) before the thread read the old file's event.
        tracker.apply(MpvEvent::FileLoaded, 2);
        assert_eq!(
            tracker.loaded(),
            Some(1),
            "credited to the stream it belongs to"
        );
        tracker.apply(STOPPED, 2);
        tracker.apply(MpvEvent::StartFile, 2);
        assert_eq!(tracker.loaded(), None, "the new file is still loading");
        tracker.apply(MpvEvent::FileLoaded, 2);
        assert_eq!(tracker.loaded(), Some(2));
    }

    #[test]
    fn a_file_that_ends_is_no_longer_loaded() {
        let mut tracker = LoadTracker::default();
        tracker.apply(MpvEvent::StartFile, 1);
        tracker.apply(MpvEvent::FileLoaded, 1);
        tracker.apply(STOPPED, 1);
        assert_eq!(tracker.loaded(), None);
    }

    #[test]
    fn a_loaded_event_without_a_start_event_uses_the_current_generation() {
        let mut tracker = LoadTracker::default();
        tracker.apply(MpvEvent::FileLoaded, 5);
        assert_eq!(tracker.loaded(), Some(5));
    }
}
