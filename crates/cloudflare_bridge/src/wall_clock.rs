//! The wall clock the saved-pass rules read. Passes expire at a wall-clock time (a cookie's `Expires`), so
//! this is `SystemTime`, not `Instant`; tests use a fixed clock to make every expiry exact.

use std::time::SystemTime;

/// What time it is.
pub trait WallClock: Send + Sync {
    /// Now.
    fn now(&self) -> SystemTime;
}

/// The system's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl WallClock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock that says what a test sets.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct FixedClock(std::sync::Mutex<SystemTime>);

#[cfg(test)]
impl FixedClock {
    pub(crate) fn at(time: SystemTime) -> Self {
        Self(std::sync::Mutex::new(time))
    }

    pub(crate) fn advance(&self, by: std::time::Duration) {
        let mut time = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *time += by;
    }
}

#[cfg(test)]
impl WallClock for FixedClock {
    fn now(&self) -> SystemTime {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn the_system_clock_moves_and_a_fixed_clock_moves_only_when_told() {
        let before = SystemTime::now();
        assert!(SystemClock.now() >= before);
        let fixed = FixedClock::at(UNIX_EPOCH);
        assert_eq!(fixed.now(), UNIX_EPOCH);
        fixed.advance(Duration::from_secs(3));
        assert_eq!(fixed.now(), UNIX_EPOCH + Duration::from_secs(3));
    }
}
