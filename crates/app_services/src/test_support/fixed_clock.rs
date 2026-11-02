use std::sync::atomic::{AtomicI64, Ordering};

use crate::Clock;

/// A clock tests can set and advance.
#[derive(Default)]
pub(crate) struct FixedClock(AtomicI64);

impl FixedClock {
    pub(crate) fn at(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    pub(crate) fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::Relaxed);
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}
