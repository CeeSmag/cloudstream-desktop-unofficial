//! A cookie's value: a credential, so it can be handed on but never printed.
//!
//! `Debug` shows `<redacted>` and there is no `Display`, so a value cannot reach a log line, a panic
//! message or a `{:?}` of a larger structure by accident. The only way to read it is [`CookieValue::expose`],
//! which is easy to find in review.

use std::fmt;

/// The value of a browser cookie (`cf_clearance` and friends).
#[derive(Clone, PartialEq, Eq)]
pub struct CookieValue(String);

impl CookieValue {
    /// Wraps `value`.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value itself, for the one place that must send it on (the sidecar's result file).
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// True when the value is empty (a cookie the site cleared).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for CookieValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CookieValue(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let value = CookieValue::new("s3cr3t-clearance");
        let printed = format!("{value:?} {:?}", vec![value.clone()]);
        assert!(!printed.contains("s3cr3t"), "{printed}");
        assert!(printed.contains("<redacted>"));
    }

    #[test]
    fn expose_and_is_empty_read_the_wrapped_value() {
        assert_eq!(CookieValue::new("abc").expose(), "abc");
        assert!(CookieValue::new("").is_empty());
        assert!(!CookieValue::new("x").is_empty());
        assert_eq!(CookieValue::new("a"), CookieValue::new(String::from("a")));
        assert_ne!(CookieValue::new("a"), CookieValue::new("b"));
    }
}
