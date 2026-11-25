//! The registry values Microsoft documents for finding the WebView2 runtime, and the Windows
//! version. Thin glue over `windows-registry`; the rules are in `runtime_status`.

use windows_registry::{Key, CURRENT_USER, LOCAL_MACHINE};

use crate::{RuntimeFacts, RuntimeProbe};

/// The runtime's EdgeUpdate client key, below `SOFTWARE\` or `SOFTWARE\WOW6432Node\`.
const CLIENT_KEY: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const CURRENT_VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

/// Reads the runtime's `pv` values (machine-wide 32- and 64-bit views, and per-user) and
/// `CurrentMajorVersionNumber`.
pub(crate) struct RegistryProbe;

impl RuntimeProbe for RegistryProbe {
    fn facts(&self) -> RuntimeFacts {
        let machine_wow = format!(r"SOFTWARE\WOW6432Node\{CLIENT_KEY}");
        let native = format!(r"SOFTWARE\{CLIENT_KEY}");
        let versions = [
            text_value(LOCAL_MACHINE, &machine_wow, "pv"),
            text_value(LOCAL_MACHINE, &native, "pv"),
            text_value(CURRENT_USER, &native, "pv"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let major = LOCAL_MACHINE
            .open(CURRENT_VERSION_KEY)
            .and_then(|key| key.get_u32("CurrentMajorVersionNumber"))
            .ok();
        let facts = RuntimeFacts::Windows { major, versions };
        tracing::debug!(?facts, "read the WebView2 runtime registry values");
        facts
    }
}

fn text_value(root: &Key, key: &str, name: &str) -> Option<String> {
    root.open(key).and_then(|key| key.get_string(name)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{judge, RuntimeStatus};

    /// T4: this PC (Windows 10 or newer, with the runtime that Windows 11 carries and the
    /// Cloudflare window uses) reports its real version and an installed runtime.
    #[test]
    #[ignore = "reads this PC's registry"]
    fn t4_this_pc_reports_windows_10_and_its_runtime() {
        let facts = RegistryProbe.facts();
        let RuntimeFacts::Windows { major, versions } = &facts else {
            panic!("a Windows build must read Windows facts");
        };
        assert!(major.is_some_and(|major| major >= 10), "{major:?}");
        assert!(!versions.is_empty(), "no `pv` value was read: {facts:?}");
        assert!(
            matches!(judge(&facts), RuntimeStatus::Installed { .. }),
            "{facts:?}"
        );
    }
}
