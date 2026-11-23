//! A safety net under the `rejected=` protocol: the same saved pass is not handed to the same origin again
//! and again in a short time.
//!
//! The sidecar names a refused pass (`rejected=`), and that is what retires it. But a sidecar that failed to
//! say so (an older build, a plugin that drops the clearance itself, a bug) must still not loop: once a pass
//! was handed to an origin [`REASK_LIMIT`] times within [`REASK_WINDOW`], the next request of that origin
//! goes to the check window instead. Two plugins asking about one site at once still both get the pass.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::pass_ids::PassId;

/// How long hand-outs are counted.
pub(crate) const REASK_WINDOW: Duration = Duration::from_secs(10);

/// How often one pass may be handed to one origin within [`REASK_WINDOW`]: each plugin process asks once
/// per origin and then holds the pass, so a few plugins on one site all get it, and a loop costs at most this
/// many quick answers before the window takes over.
pub(crate) const REASK_LIMIT: usize = 4;

/// Recent hand-outs per origin.
#[derive(Debug, Default)]
pub(crate) struct ReaskGuard {
    served: HashMap<String, Vec<(PassId, SystemTime)>>,
}

impl ReaskGuard {
    /// True when `pass` may not be handed to `origin` at `now`.
    pub(crate) fn blocks(&self, origin: &str, pass: &PassId, now: SystemTime) -> bool {
        self.served.get(origin).is_some_and(|list| {
            list.iter()
                .filter(|(id, at)| id == pass && is_recent(*at, now))
                .count()
                >= REASK_LIMIT
        })
    }

    /// Notes that `pass` was handed to `origin` at `now`; forgets hand-outs that no longer count.
    pub(crate) fn note_served(&mut self, origin: &str, pass: &PassId, now: SystemTime) {
        self.served.retain(|_, list| {
            list.retain(|(_, at)| is_recent(*at, now));
            !list.is_empty()
        });
        self.served
            .entry(origin.to_owned())
            .or_default()
            .push((pass.clone(), now));
    }
}

/// True while a hand-out at `at` still counts at `now`. A clock that went back counts it (when in doubt,
/// do not hand the pass out again).
fn is_recent(at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(at)
        .map_or(true, |elapsed| elapsed < REASK_WINDOW)
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    fn id(text: &str) -> PassId {
        PassId::parse(text).expect("id")
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_000 + secs)
    }

    const ORIGIN: &str = "https://www.site.test";

    #[test]
    fn a_pass_is_handed_out_up_to_the_limit_within_the_window() {
        let mut guard = ReaskGuard::default();
        for second in 0..REASK_LIMIT {
            let second = u64::try_from(second).expect("small");
            assert!(
                !guard.blocks(ORIGIN, &id("p1"), at(second)),
                "hand-out {second}"
            );
            guard.note_served(ORIGIN, &id("p1"), at(second));
        }
        let last = u64::try_from(REASK_LIMIT - 1).expect("small");
        assert!(guard.blocks(ORIGIN, &id("p1"), at(last)));
        assert!(guard.blocks(ORIGIN, &id("p1"), at(9)));
        assert!(
            !guard.blocks(ORIGIN, &id("p1"), at(10)),
            "the first no longer counts"
        );
        assert!(!guard.blocks(ORIGIN, &id("p1"), at(last + 10)));
    }

    #[test]
    fn another_pass_or_another_origin_is_not_blocked() {
        let mut guard = ReaskGuard::default();
        for _ in 0..REASK_LIMIT {
            guard.note_served(ORIGIN, &id("p1"), at(0));
        }
        assert!(guard.blocks(ORIGIN, &id("p1"), at(0)));
        assert!(!guard.blocks(ORIGIN, &id("p2"), at(0)));
        assert!(!guard.blocks("https://api.site.test", &id("p1"), at(0)));
    }

    #[test]
    fn a_clock_that_went_back_keeps_the_guard_up() {
        let mut guard = ReaskGuard::default();
        for _ in 0..REASK_LIMIT {
            guard.note_served(ORIGIN, &id("p1"), at(5));
        }
        assert!(guard.blocks(ORIGIN, &id("p1"), at(0)));
    }

    #[test]
    fn old_hand_outs_are_forgotten_so_the_guard_stays_small() {
        let mut guard = ReaskGuard::default();
        for second in 0..100 {
            let origin = format!("https://h{second}.site.test");
            guard.note_served(&origin, &id("p1"), at(second));
        }
        assert!(guard.served.len() <= 10, "{}", guard.served.len());
        assert!(guard.served.values().all(|list| list.len() == 1));
    }
}
