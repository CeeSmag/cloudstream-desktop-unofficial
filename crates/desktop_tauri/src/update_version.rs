//! Release tags and version numbers: is a release newer than the running program?

/// The tag without spaces around it and without a leading `v`.
pub(crate) fn clean_version(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

/// `major.minor.patch` from a tag; a missing patch counts as 0, a `-suffix` after it is ignored.
fn parse_version_numbers(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = clean_version(version).split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split('-')
        .next()?
        .parse::<u32>()
        .ok()?;
    Some((major, minor, patch))
}

/// True when the release tagged `latest` is newer than the running version `current`.
pub(crate) fn is_version_newer(latest: &str, current: &str) -> bool {
    match (
        parse_version_numbers(latest),
        parse_version_numbers(current),
    ) {
        (Some(latest), Some(current)) => latest > current,
        _ => clean_version(latest) != clean_version(current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_version_newer_mixed_tags_compares_numbers() {
        assert!(is_version_newer("v0.3.1", "0.3.0"));
        assert!(is_version_newer("0.4.0", "0.3.0"));
        assert!(is_version_newer("v1.0.0", "0.3.0"));
        assert!(!is_version_newer("v0.3.0", "0.3.0"));
        assert!(!is_version_newer("v0.2.9", "0.3.0"));
        assert!(!is_version_newer("0.3.0", "0.3.0"));
    }

    #[test]
    fn is_version_newer_two_digit_patch_beats_one_digit() {
        assert!(is_version_newer("0.4.10", "0.4.9"));
        assert!(!is_version_newer("0.4.9", "0.4.10"));
    }

    #[test]
    fn is_version_newer_missing_patch_counts_as_zero() {
        assert!(!is_version_newer("v0.4", "0.4.0"));
        assert!(is_version_newer("v0.5", "0.4.2"));
    }
}
