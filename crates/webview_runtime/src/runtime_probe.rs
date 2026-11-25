//! Reading the PC: the seam between the rules and the registry.

use std::sync::Arc;

use crate::RuntimeFacts;

/// Reads what the PC says about WebView2. Implemented by the registry on Windows; tests and the
/// `CS_WEBVIEW2_PROBE` override use [`FixedProbe`].
pub trait RuntimeProbe: Send + Sync {
    /// The facts as they are now (read again on every call).
    fn facts(&self) -> RuntimeFacts;
}

/// A probe that always answers the same facts.
#[derive(Debug, Clone)]
pub struct FixedProbe(pub RuntimeFacts);

impl RuntimeProbe for FixedProbe {
    fn facts(&self) -> RuntimeFacts {
        self.0.clone()
    }
}

/// The facts a `CS_WEBVIEW2_PROBE` value pretends (`missing`, `unsupported`, `installed`), so the
/// dialog can be tried on a PC that has the runtime. Other values pretend nothing.
pub fn pretended_facts(value: &str) -> Option<RuntimeFacts> {
    let windows = |major: u32, versions: &[&str]| RuntimeFacts::Windows {
        major: Some(major),
        versions: versions.iter().map(|text| (*text).to_owned()).collect(),
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "missing" => Some(windows(10, &[])),
        "unsupported" => Some(windows(6, &[])),
        "installed" => Some(windows(10, &["1.0.0.1"])),
        _ => None,
    }
}

/// The probe for this PC: the registry on Windows, "not Windows" elsewhere.
pub fn platform_probe() -> Arc<dyn RuntimeProbe> {
    #[cfg(windows)]
    {
        Arc::new(crate::windows::registry_probe::RegistryProbe)
    }
    #[cfg(not(windows))]
    {
        Arc::new(FixedProbe(RuntimeFacts::NotWindows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{judge, RuntimeStatus};

    #[test]
    fn pretended_values_give_each_status() {
        let status = |value: &str| pretended_facts(value).map(|facts| judge(&facts));
        assert_eq!(status(" Missing "), Some(RuntimeStatus::Missing));
        assert_eq!(status("unsupported"), Some(RuntimeStatus::Unsupported));
        assert_eq!(
            status("INSTALLED"),
            Some(RuntimeStatus::Installed {
                version: "1.0.0.1".into()
            })
        );
        assert_eq!(status(""), None);
        assert_eq!(status("yes"), None);
    }

    #[test]
    fn a_fixed_probe_repeats_its_facts() {
        let probe = FixedProbe(RuntimeFacts::NotWindows);
        assert_eq!(probe.facts(), RuntimeFacts::NotWindows);
        assert_eq!(probe.facts(), RuntimeFacts::NotWindows);
    }

    #[test]
    fn the_platform_probe_matches_the_platform() {
        let facts = platform_probe().facts();
        assert_eq!(
            matches!(facts, RuntimeFacts::NotWindows),
            cfg!(not(windows))
        );
    }
}
