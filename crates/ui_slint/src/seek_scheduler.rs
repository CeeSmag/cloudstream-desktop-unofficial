//! Spaces out the jumps the arrow keys send to the player.
//!
//! Ported from the old app (`seekScheduler.ts`). Each jump can make the player fetch the video
//! again from the new place, and a host that limits requests answers a flood of them with "429
//! Too Many Requests", after which the player skips minutes of video. The first press goes at
//! once; the next waits until [`FAST_GAP`] has passed (carrying the newest target), and a key held
//! (or tapped) for longer than [`SUSTAINED`] sends one only every [`SLOW_GAP`].

use std::time::{Duration, Instant};

/// The shortest time between two jumps while keys are tapped.
pub(crate) const FAST_GAP: Duration = Duration::from_millis(200);
/// The shortest time between two jumps once a run of presses lasted [`SUSTAINED`].
pub(crate) const SLOW_GAP: Duration = Duration::from_millis(500);
/// How long a run of presses lasts before it counts as a held key.
pub(crate) const SUSTAINED: Duration = Duration::from_millis(1000);
/// A pause between presses this long ends a run.
pub(crate) const RUN_ENDS: Duration = Duration::from_millis(400);
/// How long a sent jump holds its target on the bar while the player catches up.
pub(crate) const SETTLE_WINDOW: Duration = Duration::from_millis(1500);
/// A reported position within this distance of the target means the player arrived.
pub(crate) const ARRIVED_MS: i64 = 2000;

/// The jumps asked for and sent.
#[derive(Debug, Default)]
pub(crate) struct SeekScheduler {
    /// The target waiting to be sent, and when it may go.
    waiting: Option<(i64, Instant)>,
    /// The most recent jump target and when it was asked for (holds the bar without snap-back).
    settled: Option<(i64, Instant)>,
    last_sent: Option<Instant>,
    last_press: Option<Instant>,
    run_started: Option<Instant>,
}

impl SeekScheduler {
    /// A press asks for `target_ms`; returns it when it should be sent now, else it waits.
    pub(crate) fn press(&mut self, target_ms: i64, now: Instant) -> Option<i64> {
        let run_over = self
            .last_press
            .is_none_or(|last| now.duration_since(last) >= RUN_ENDS);
        if run_over {
            self.run_started = Some(now);
        }
        self.last_press = Some(now);
        self.settled = Some((target_ms, now));
        let held = self
            .run_started
            .is_some_and(|start| now.duration_since(start) >= SUSTAINED);
        let gap = if held { SLOW_GAP } else { FAST_GAP };
        match self.last_sent.map(|sent| sent + gap) {
            Some(allowed) if allowed > now => {
                self.waiting = Some((target_ms, allowed));
                None
            }
            _ => Some(self.send(target_ms, now)),
        }
    }

    /// Sends an explicit bar click/drag target immediately, dropping any queued key jump.
    pub(crate) fn send_now(&mut self, target_ms: i64, now: Instant) {
        self.settled = Some((target_ms, now));
        let _ = self.send(target_ms, now);
    }

    /// The waiting target, once its time has come (called often while the player is open).
    pub(crate) fn due(&mut self, now: Instant) -> Option<i64> {
        match self.waiting {
            Some((target, allowed)) if allowed <= now => Some(self.send(target, now)),
            _ => None,
        }
    }

    /// The target the next jump goes to, while one waits.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> Option<i64> {
        self.waiting.map(|(target, _)| target)
    }

    /// Starting point for the next relative jump: queued target, fresh sent target, else `playing_ms`.
    pub(crate) fn seek_base(&self, playing_ms: i64, now: Instant) -> i64 {
        if let Some((target, _)) = self.waiting {
            return target;
        }
        self.fresh_settled(now).unwrap_or(playing_ms)
    }

    /// Position to show on the seek bar so clicks and taps never rubber-band before mpv arrives.
    pub(crate) fn shown_position(&mut self, playing_ms: i64, now: Instant) -> i64 {
        if let Some((target, _)) = self.waiting {
            return target;
        }
        let Some(target) = self.fresh_settled(now) else {
            self.settled = None;
            return playing_ms;
        };
        if (playing_ms - target).abs() <= ARRIVED_MS {
            self.settled = None;
            return playing_ms;
        }
        target
    }

    /// Forgets any waiting or settling jump (player closed or new stream opened).
    pub(crate) fn cancel(&mut self) {
        self.waiting = None;
        self.settled = None;
    }

    fn fresh_settled(&self, now: Instant) -> Option<i64> {
        let (target, at) = self.settled?;
        (now.checked_duration_since(at)
            .is_some_and(|age| age < SETTLE_WINDOW))
        .then_some(target)
    }

    fn send(&mut self, target_ms: i64, now: Instant) -> i64 {
        self.waiting = None;
        self.last_sent = Some(now);
        target_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn press_first_goes_at_once_next_waits_with_the_newest_target() {
        let start = Instant::now();
        let mut seeks = SeekScheduler::default();
        assert_eq!(seeks.press(10_000, start), Some(10_000));
        assert_eq!(seeks.press(20_000, at(start, 50)), None);
        assert_eq!(seeks.press(30_000, at(start, 100)), None);
        assert_eq!(seeks.pending(), Some(30_000));
        assert_eq!(seeks.due(at(start, 150)), None, "too early");
        assert_eq!(
            seeks.due(at(start, 200)),
            Some(30_000),
            "only the newest goes"
        );
        assert_eq!(seeks.due(at(start, 300)), None, "nothing left");
    }

    #[test]
    fn press_held_key_slows_to_one_jump_per_half_second() {
        let start = Instant::now();
        let mut seeks = SeekScheduler::default();
        let mut sent = Vec::new();
        // Key repeat every 50 ms for two seconds.
        for step in 0..=40 {
            let now = at(start, step * 50);
            if let Some(target) = seeks.due(now) {
                sent.push((step * 50, target));
            }
            if let Some(target) = seeks.press(i64::try_from(step).unwrap_or(0), now) {
                sent.push((step * 50, target));
            }
        }
        let late: Vec<u64> = sent
            .iter()
            .map(|(ms, _)| *ms)
            .filter(|ms| *ms > 1_200)
            .collect();
        let gaps: Vec<u64> = late
            .windows(2)
            .filter_map(|pair| match pair {
                [first, second] => Some(second - first),
                _ => None,
            })
            .collect();
        assert!(
            !gaps.is_empty() && gaps.iter().all(|gap| *gap >= 500),
            "{sent:?}"
        );
        assert!(sent.len() < 12, "far fewer jumps than 41 presses: {sent:?}");
    }

    #[test]
    fn press_after_a_pause_starts_a_new_fast_run() {
        let start = Instant::now();
        let mut seeks = SeekScheduler::default();
        for step in 0..30 {
            seeks.press(0, at(start, step * 50));
            seeks.due(at(start, step * 50));
        }
        assert_eq!(
            seeks.press(99, at(start, 3_000)),
            Some(99),
            "a fresh tap goes at once"
        );
    }

    #[test]
    fn cancel_drops_the_waiting_jump() {
        let start = Instant::now();
        let mut seeks = SeekScheduler::default();
        seeks.press(1, start);
        seeks.press(2, at(start, 10));
        seeks.cancel();
        assert_eq!(seeks.pending(), None);
        assert_eq!(seeks.due(at(start, 500)), None);
        assert_eq!(seeks.shown_position(999, at(start, 20)), 999);
    }

    #[test]
    fn send_now_holds_position_until_arrived_or_settled() {
        let start = Instant::now();
        let mut seeks = SeekScheduler::default();
        seeks.send_now(60_000, start);
        assert_eq!(seeks.shown_position(10_000, at(start, 200)), 60_000);
        assert_eq!(seeks.seek_base(10_000, at(start, 200)), 60_000);
        assert_eq!(
            seeks.shown_position(59_500, at(start, 400)),
            59_500,
            "arriving within ARRIVED_MS hands control back to mpv"
        );
        assert_eq!(seeks.shown_position(10_000, at(start, 500)), 10_000);

        seeks.send_now(120_000, at(start, 600));
        assert_eq!(
            seeks.shown_position(10_000, at(start, 2_200)),
            10_000,
            "expired settle window falls back to reported position"
        );
    }
}
