use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use core_model::PlayableLink;

use crate::stream_resolver::link_rank;

#[derive(Default)]
struct QueueState {
    waiting: Vec<(usize, PlayableLink)>,
    closed: bool,
}

/// Streams waiting to be checked. The search adds them as it finds them, the checkers take the
/// best one first (highest quality, earliest found), so a good stream is judged before a poor one.
#[derive(Default)]
pub struct CheckQueue {
    state: Mutex<QueueState>,
    wake: Condvar,
}

impl CheckQueue {
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds streams, each paired with the index the caller knows it by.
    pub fn push(&self, items: impl IntoIterator<Item = (usize, PlayableLink)>) {
        self.lock().waiting.extend(items);
        self.wake.notify_all();
    }

    /// No more streams will be added; the checkers finish what is waiting and stop.
    pub fn close(&self) {
        self.lock().closed = true;
        self.wake.notify_all();
    }

    /// Nobody wants the answers any more: forgets what is waiting and stops the checkers.
    pub fn stop(&self) {
        let mut state = self.lock();
        state.waiting.clear();
        state.closed = true;
        drop(state);
        self.wake.notify_all();
    }

    /// The best waiting stream, waiting for one to be added if there is none yet.
    /// `None` once the queue is closed and empty.
    pub(crate) fn next(&self) -> Option<(usize, PlayableLink)> {
        let mut state = self.lock();
        loop {
            if let Some(position) = best_position(&state.waiting) {
                return Some(state.waiting.remove(position));
            }
            if state.closed {
                return None;
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Highest rank wins; among equals the one added first.
fn best_position(waiting: &[(usize, PlayableLink)]) -> Option<usize> {
    waiting
        .iter()
        .enumerate()
        .max_by_key(|(position, (_, link))| (link_rank(link), std::cmp::Reverse(*position)))
        .map(|(position, _)| position)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    fn link(quality: i32) -> PlayableLink {
        PlayableLink {
            quality,
            url: format!("https://h/{quality}.mp4"),
            ..PlayableLink::default()
        }
    }

    #[test]
    fn the_best_quality_comes_out_first_and_ties_keep_their_order() {
        let queue = CheckQueue::default();
        queue.push([
            (0, link(480)),
            (1, link(1080)),
            (2, link(1080)),
            (3, link(720)),
        ]);
        queue.close();
        let order: Vec<usize> =
            std::iter::from_fn(|| queue.next().map(|(index, _)| index)).collect();
        assert_eq!(order, [1, 2, 3, 0]);
    }

    #[test]
    fn a_closed_empty_queue_ends_the_checkers() {
        let queue = CheckQueue::default();
        queue.close();
        assert!(queue.next().is_none());
    }

    #[test]
    fn stopping_forgets_what_is_waiting() {
        let queue = CheckQueue::default();
        queue.push([(0, link(720))]);
        queue.stop();
        assert!(queue.next().is_none());
    }

    #[test]
    fn a_checker_waits_for_streams_that_are_added_later() {
        let queue = Arc::new(CheckQueue::default());
        let later = Arc::clone(&queue);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            later.push([(7, link(360))]);
            later.close();
        });
        assert_eq!(queue.next().map(|(index, _)| index), Some(7));
        assert!(queue.next().is_none());
    }

    /// Many threads add streams while many others take them: each stream must be taken exactly
    /// once, whichever thread gets it.
    #[test]
    fn under_concurrent_pushing_and_taking_every_stream_is_taken_exactly_once() {
        const PRODUCERS: usize = 4;
        const PER_PRODUCER: usize = 250;
        let queue = Arc::new(CheckQueue::default());
        let taken = Arc::new(std::sync::Mutex::new(Vec::new()));
        let consumers: Vec<_> = (0..8)
            .map(|_| {
                let (queue, taken) = (Arc::clone(&queue), Arc::clone(&taken));
                std::thread::spawn(move || {
                    while let Some((index, _)) = queue.next() {
                        taken.lock().expect("lock").push(index);
                    }
                })
            })
            .collect();
        let producers: Vec<_> = (0..PRODUCERS)
            .map(|producer| {
                let queue = Arc::clone(&queue);
                std::thread::spawn(move || {
                    for offset in 0..PER_PRODUCER {
                        let index = producer * PER_PRODUCER + offset;
                        queue.push([(index, link(i32::try_from(index % 5).unwrap_or(0) * 360))]);
                    }
                })
            })
            .collect();
        for producer in producers {
            producer.join().expect("producer");
        }
        queue.close();
        join_all_within(consumers, Duration::from_secs(20));

        let mut taken = taken.lock().expect("lock").clone();
        taken.sort_unstable();
        let expected: Vec<usize> = (0..PRODUCERS * PER_PRODUCER).collect();
        assert_eq!(taken, expected, "a stream was lost or taken twice");
    }

    /// Stopping must wake every consumer that is waiting for streams, however many there are.
    #[test]
    fn stopping_wakes_every_waiting_consumer() {
        for _ in 0..100 {
            let queue = Arc::new(CheckQueue::default());
            let consumers: Vec<_> = (0..8)
                .map(|_| {
                    let queue = Arc::clone(&queue);
                    std::thread::spawn(move || assert!(queue.next().is_none()))
                })
                .collect();
            queue.stop();
            join_all_within(consumers, Duration::from_secs(20));
        }
    }

    fn join_all_within(handles: Vec<std::thread::JoinHandle<()>>, limit: Duration) {
        let deadline = std::time::Instant::now() + limit;
        for handle in handles {
            while !handle.is_finished() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "a thread did not finish: deadlock"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            handle.join().expect("a test thread panicked");
        }
    }
}
