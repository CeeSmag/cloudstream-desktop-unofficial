//! Release tags and version numbers: is a release newer than the running program? Tags are read as
//! semantic versions (`v0.4.2`, `0.5.0-rc.1`). A tag that is not one (`nightly`, `v0.5`, a typo) is
//! never offered: guessing could offer an older release as an update.

use semver::Version;

use crate::update_error::UpdateError;

/// The tag without spaces around it and without a leading `v`.
pub(crate) fn clean_version(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

fn parse_release(tag: &str) -> Result<Version, UpdateError> {
    Version::parse(clean_version(tag)).map_err(|source| UpdateError::UnreadableVersion {
        tag: tag.to_string(),
        source,
    })
}

/// Whether the release tagged `latest` is an update for the running version `current`: it must be
/// newer, and a pre-release (`-rc.1`) is offered only to someone already running a pre-release.
/// Build metadata (`+build.5`) does not count. Fails when either is not a version.
pub(crate) fn is_version_newer(latest: &str, current: &str) -> Result<bool, UpdateError> {
    let latest = parse_release(latest)?;
    let current = parse_release(current)?;
    if !latest.pre.is_empty() && current.pre.is_empty() {
        return Ok(false);
    }
    Ok(latest.cmp_precedence(&current).is_gt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the updater would offer `latest` while `current` runs (an unreadable tag is not).
    fn offered(latest: &str, current: &str) -> bool {
        is_version_newer(latest, current).unwrap_or(false)
    }

    #[test]
    fn is_version_newer_mixed_tags_compares_numbers() {
        assert!(offered("v0.3.1", "0.3.0"));
        assert!(offered("0.4.0", "0.3.0"));
        assert!(offered("v1.0.0", "0.3.0"));
        assert!(offered(" V0.3.1 ", "0.3.0"));
        assert!(!offered("v0.3.0", "0.3.0"));
        assert!(!offered("v0.2.9", "0.3.0"));
        assert!(!offered("0.3.0", "0.3.0"));
    }

    #[test]
    fn is_version_newer_two_digit_patch_beats_one_digit() {
        assert!(offered("0.4.10", "0.4.9"));
        assert!(!offered("0.4.9", "0.4.10"));
    }

    #[test]
    fn is_version_newer_unreadable_tag_returns_error_naming_it() {
        let result = is_version_newer("nightly", "0.4.2");
        assert!(matches!(
            result,
            Err(UpdateError::UnreadableVersion { ref tag, .. }) if tag == "nightly"
        ));
    }

    #[test]
    fn is_version_newer_unreadable_tag_is_not_offered() {
        assert!(!offered("nightly", "0.4.2"));
        assert!(!offered("latest", "0.4.2"));
        assert!(!offered("v0.5", "0.4.2"));
        assert!(!offered("", "0.4.2"));
    }

    #[test]
    fn is_version_newer_older_release_is_not_offered() {
        assert!(!offered("v0.4.1", "0.4.2"));
        assert!(!offered("v0.3.9", "0.4.2"));
    }

    #[test]
    fn is_version_newer_prerelease_while_on_stable_is_not_offered() {
        assert!(!offered("0.4.2-rc1", "0.4.2"));
        assert!(!offered("v0.5.0-rc.1", "0.4.2"));
    }

    #[test]
    fn is_version_newer_prerelease_while_on_prerelease_is_offered() {
        assert!(offered("v0.5.0-rc.2", "0.5.0-rc.1"));
        assert!(!offered("v0.5.0-rc.1", "0.5.0-rc.2"));
    }

    #[test]
    fn is_version_newer_stable_after_its_prerelease_is_offered() {
        assert!(offered("v0.5.0", "0.5.0-rc.1"));
    }

    #[test]
    fn is_version_newer_build_metadata_does_not_count() {
        assert!(!offered("v0.4.2+build.5", "0.4.2"));
    }
}
