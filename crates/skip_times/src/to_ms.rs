//! Milliseconds from the numbers databases send.

/// Longest time accepted: a day. Anything bigger is a wrong answer, not a position.
const MAX_MS: f64 = 24.0 * 3600.0 * 1000.0;

/// A number of milliseconds as a whole number; negative, not-a-number and absurd values become
/// 0 and the day limit.
pub(crate) fn to_ms(value: f64) -> u64 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    value.min(MAX_MS).round() as u64
}

/// A number of seconds as milliseconds.
pub(crate) fn seconds_to_ms(seconds: f64) -> u64 {
    to_ms(seconds * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_and_limits() {
        assert_eq!(to_ms(1499.6), 1500);
        assert_eq!(to_ms(-3.0), 0);
        assert_eq!(to_ms(f64::NAN), 0);
        assert_eq!(to_ms(f64::INFINITY), 86_400_000);
        assert_eq!(seconds_to_ms(54.711), 54_711);
    }
}
