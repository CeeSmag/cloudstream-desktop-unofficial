//! How many plugin JVMs may stay resident (0 = unlimited).

/// Parses a stored limit: empty / "0" / "unlimited" → unlimited; "1"/"2"/… → Some(n).
pub fn parse_limit(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unlimited") || trimmed == "0" {
        return None;
    }
    trimmed.parse::<usize>().ok().filter(|n| *n > 0)
}

/// Serializes a limit for settings storage.
pub fn format_limit(limit: Option<usize>) -> String {
    match limit {
        None => "0".to_owned(),
        Some(n) => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_limit_treats_zero_and_unlimited_as_none() {
        assert_eq!(parse_limit(""), None);
        assert_eq!(parse_limit("0"), None);
        assert_eq!(parse_limit("unlimited"), None);
        assert_eq!(parse_limit("1"), Some(1));
        assert_eq!(parse_limit("2"), Some(2));
        assert_eq!(parse_limit("nope"), None);
    }

    #[test]
    fn format_limit_round_trips_with_parse() {
        assert_eq!(parse_limit(&format_limit(None)), None);
        assert_eq!(parse_limit(&format_limit(Some(1))), Some(1));
    }
}
