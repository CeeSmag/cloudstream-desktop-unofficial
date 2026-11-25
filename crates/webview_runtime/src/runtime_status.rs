//! What the PC tells us about WebView2, and what that means for the Cloudflare check.

/// The oldest Windows (major version) current WebView2 runtimes support.
pub const MIN_WINDOWS_MAJOR: u32 = 10;

/// What a [`crate::RuntimeProbe`] read from the PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeFacts {
    /// Not Windows: the Cloudflare window uses the system's own browser engine.
    NotWindows,
    /// Windows, with what its registry says.
    Windows {
        /// `CurrentMajorVersionNumber` (only Windows 10 and newer write it).
        major: Option<u32>,
        /// Every `pv` value found for the runtime (machine-wide and per-user), unchecked.
        versions: Vec<String>,
    },
}

/// Whether the Cloudflare check can open its window on this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeStatus {
    /// Not Windows: nothing to install.
    NotNeeded,
    /// No runtime, and this Windows is too old for a current one.
    Unsupported,
    /// No runtime; it can be installed.
    Missing,
    /// A runtime is installed.
    Installed {
        /// Its version, for the log and Settings.
        version: String,
    },
}

impl RuntimeStatus {
    /// Whether the viewer can be offered the install.
    pub fn can_install(&self) -> bool {
        matches!(self, Self::Missing)
    }

    /// Whether the Cloudflare window cannot open because of the runtime.
    pub fn blocks_verification(&self) -> bool {
        matches!(self, Self::Missing | Self::Unsupported)
    }
}

/// A runtime version as the registry writes it (`a.b.c.d`), or `None` for an empty, malformed or
/// all-zero value (an uninstall leaves `0.0.0.0` behind).
pub fn parse_version(raw: &str) -> Option<String> {
    let text = raw.trim();
    let parts: Vec<&str> = text.split('.').collect();
    let numeric = parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    let all_zero = parts
        .iter()
        .all(|part| part.bytes().all(|byte| byte == b'0'));
    (parts.len() == 4 && numeric && !all_zero).then(|| text.to_owned())
}

/// The status the facts give. A runtime that is there counts as installed whatever the Windows
/// version; only a missing one on an old Windows is `Unsupported`.
pub fn judge(facts: &RuntimeFacts) -> RuntimeStatus {
    let RuntimeFacts::Windows { major, versions } = facts else {
        return RuntimeStatus::NotNeeded;
    };
    if let Some(version) = versions.iter().find_map(|raw| parse_version(raw)) {
        return RuntimeStatus::Installed { version };
    }
    if major.is_some_and(|major| major >= MIN_WINDOWS_MAJOR) {
        RuntimeStatus::Missing
    } else {
        RuntimeStatus::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(major: Option<u32>, versions: &[&str]) -> RuntimeFacts {
        RuntimeFacts::Windows {
            major,
            versions: versions.iter().map(|text| (*text).to_owned()).collect(),
        }
    }

    #[test]
    fn real_versions_are_read() {
        assert_eq!(parse_version("131.0.2903.70"), Some("131.0.2903.70".into()));
        assert_eq!(parse_version(" 1.0.0.1\n"), Some("1.0.0.1".into()));
    }

    #[test]
    fn empty_zero_and_malformed_versions_are_absent() {
        for raw in [
            "",
            "0.0.0.0",
            "00.0.000.0",
            "1.2.3",
            "1.2.3.4.5",
            "1..3.4",
            "1.2.3.x",
            "-1.2.3.4",
        ] {
            assert_eq!(parse_version(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn not_windows_needs_nothing() {
        assert_eq!(judge(&RuntimeFacts::NotWindows), RuntimeStatus::NotNeeded);
    }

    #[test]
    fn the_first_readable_version_wins() {
        assert_eq!(
            judge(&windows(Some(10), &["0.0.0.0", "120.0.1.2", "121.0.0.1"])),
            RuntimeStatus::Installed {
                version: "120.0.1.2".into()
            }
        );
    }

    #[test]
    fn windows_10_without_a_runtime_can_install_it() {
        let status = judge(&windows(Some(10), &["0.0.0.0"]));
        assert_eq!(status, RuntimeStatus::Missing);
        assert!(status.can_install() && status.blocks_verification());
    }

    #[test]
    fn old_or_unknown_windows_without_a_runtime_is_unsupported() {
        for major in [None, Some(6), Some(9)] {
            let status = judge(&windows(major, &[]));
            assert_eq!(status, RuntimeStatus::Unsupported, "{major:?}");
            assert!(!status.can_install() && status.blocks_verification());
        }
    }

    #[test]
    fn a_runtime_on_old_windows_is_still_used() {
        assert_eq!(
            judge(&windows(Some(6), &["109.0.1518.140"])),
            RuntimeStatus::Installed {
                version: "109.0.1518.140".into()
            }
        );
    }

    #[test]
    fn installed_and_not_needed_block_nothing() {
        for status in [
            RuntimeStatus::NotNeeded,
            RuntimeStatus::Installed {
                version: "1.0.0.1".into(),
            },
        ] {
            assert!(!status.can_install() && !status.blocks_verification());
        }
    }
}
