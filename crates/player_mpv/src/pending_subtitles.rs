//! Subtitle files waiting for the stream they belong to to finish loading.
//!
//! Each batch is tagged with the stream generation it was queued for, so files of a stream that
//! was replaced meanwhile are never handed to the stream that replaced it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use player_api::SubtitleSource;

/// The waiting files and the stream generation they belong to.
#[derive(Debug, Default)]
pub(crate) struct PendingSubtitles {
    generation: u64,
    files: Vec<SubtitleSource>,
}

impl PendingSubtitles {
    /// A new stream started: forgets what was waiting and queues `files` for `generation`.
    pub(crate) fn replace(&mut self, generation: u64, files: Vec<SubtitleSource>) {
        self.generation = generation;
        self.files = files;
    }

    /// Queues more files for the stream that is current.
    pub(crate) fn extend(&mut self, files: &[SubtitleSource]) {
        self.files.extend_from_slice(files);
    }

    /// How many files wait.
    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    /// Hands over the waiting files when they belong to stream `generation`; files queued for
    /// another stream stay where they are (and are never handed to this one).
    pub(crate) fn take_for(&mut self, generation: u64) -> Vec<SubtitleSource> {
        if self.generation == generation {
            std::mem::take(&mut self.files)
        } else {
            Vec::new()
        }
    }
}

/// A new stream starts: moves the stream generation on and queues `files` for it, both under the
/// queue's lock, so no reader can pair the new files with the old generation (or the old files
/// with the new one). Returns the new generation.
pub(crate) fn queue_for_new_stream(
    pending: &Mutex<PendingSubtitles>,
    generation: &AtomicU64,
    files: Vec<SubtitleSource>,
) -> u64 {
    let mut pending = pending.lock().unwrap_or_else(PoisonError::into_inner);
    let next = generation.fetch_add(1, Ordering::SeqCst) + 1;
    pending.replace(next, files);
    next
}

/// The files to attach now: those waiting for stream `loaded`, provided it is still the current
/// stream. `None` when nothing waits, or when the stream was replaced meanwhile.
pub(crate) fn take_when_loaded(
    pending: &Mutex<PendingSubtitles>,
    generation: &AtomicU64,
    loaded: Option<u64>,
) -> Option<(u64, Vec<SubtitleSource>)> {
    let stream = loaded.filter(|stream| *stream == generation.load(Ordering::SeqCst))?;
    let files = pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take_for(stream);
    (!files.is_empty()).then_some((stream, files))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn file(language: &str) -> SubtitleSource {
        SubtitleSource {
            url: format!("https://s/{language}.vtt"),
            language: language.to_owned(),
        }
    }

    #[test]
    fn files_are_handed_over_once_to_the_stream_they_were_queued_for() {
        let mut pending = PendingSubtitles::default();
        pending.replace(3, vec![file("Arabic")]);
        pending.extend(&[file("English")]);
        assert_eq!(pending.len(), 2);
        assert_eq!(pending.take_for(3).len(), 2);
        assert!(pending.take_for(3).is_empty(), "only once");
    }

    #[test]
    fn files_of_a_replaced_stream_are_never_handed_to_the_next_one() {
        let mut pending = PendingSubtitles::default();
        pending.replace(3, vec![file("Arabic")]);
        assert!(
            pending.take_for(4).is_empty(),
            "stream 4 must not get stream 3's files"
        );
        pending.replace(4, vec![file("English")]);
        let taken = pending.take_for(4);
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].language, "English");
    }

    const SWITCHES: usize = 4_000;

    type Shared = (
        Arc<Mutex<PendingSubtitles>>,
        Arc<AtomicU64>,
        Arc<Mutex<std::collections::HashMap<u64, usize>>>,
    );

    /// A viewer who switches source over and over; remembers which switch started which stream.
    fn spawn_switcher(shared: &Shared) -> std::thread::JoinHandle<()> {
        let (pending, generation, queued) = (
            Arc::clone(&shared.0),
            Arc::clone(&shared.1),
            Arc::clone(&shared.2),
        );
        std::thread::spawn(move || {
            for switch in 0..SWITCHES {
                let files = vec![file(&format!("switch-{switch}"))];
                let stream = queue_for_new_stream(&pending, &generation, files);
                queued.lock().expect("lock").insert(stream, switch);
            }
        })
    }

    /// The event thread: keeps attaching whatever waits for the stream that is current.
    fn spawn_attacher(shared: &Shared) -> std::thread::JoinHandle<Vec<(u64, Vec<SubtitleSource>)>> {
        let (pending, generation) = (Arc::clone(&shared.0), Arc::clone(&shared.1));
        std::thread::spawn(move || {
            let mut taken = Vec::new();
            for _ in 0..SWITCHES * 4 {
                let loaded = Some(generation.load(Ordering::SeqCst));
                if let Some(batch) = take_when_loaded(&pending, &generation, loaded) {
                    taken.push(batch);
                }
                std::thread::yield_now();
            }
            taken
        })
    }

    /// A viewer who keeps switching source while the event thread attaches subtitles must never
    /// see a file queued for one stream handed to another.
    #[test]
    fn under_concurrent_switching_a_stream_only_ever_gets_its_own_files() {
        let shared: Shared = (
            Arc::new(Mutex::new(PendingSubtitles::default())),
            Arc::new(AtomicU64::new(0)),
            Arc::default(),
        );
        let switcher = spawn_switcher(&shared);
        let attachers: Vec<_> = (0..3).map(|_| spawn_attacher(&shared)).collect();
        switcher.join().expect("switcher");

        let queued = shared.2.lock().expect("lock");
        let mut handed_over = 0;
        for attacher in attachers {
            for (stream, files) in attacher.join().expect("attacher") {
                handed_over += 1;
                let switch = queued.get(&stream).expect("a stream that was started");
                assert!(
                    files
                        .iter()
                        .all(|f| f.language == format!("switch-{switch}")),
                    "stream {stream} was given files queued for another stream"
                );
            }
        }
        assert!(handed_over > 0, "the attachers never got to run");
    }

    #[test]
    fn a_stream_that_was_replaced_before_it_loaded_gets_nothing() {
        let pending = Mutex::new(PendingSubtitles::default());
        let generation = AtomicU64::new(0);
        let first = queue_for_new_stream(&pending, &generation, vec![file("Arabic")]);
        let second = queue_for_new_stream(&pending, &generation, vec![file("English")]);
        assert!(take_when_loaded(&pending, &generation, Some(first)).is_none());
        let (stream, files) = take_when_loaded(&pending, &generation, Some(second)).expect("files");
        assert_eq!((stream, files.len()), (second, 1));
        assert!(take_when_loaded(&pending, &generation, None).is_none());
    }
}
