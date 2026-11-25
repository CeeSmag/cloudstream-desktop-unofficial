//! The registry values Microsoft documents for finding the WebView2 runtime, and the Windows
//! version. Thin glue over winsafe's safe registry API; the rules are in `runtime_status`.

use winsafe::prelude::*;
use winsafe::{RegistryValue, HKEY};

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
            text_value(&HKEY::LOCAL_MACHINE, &machine_wow, "pv"),
            text_value(&HKEY::LOCAL_MACHINE, &native, "pv"),
            text_value(&HKEY::CURRENT_USER, &native, "pv"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let facts = RuntimeFacts::Windows {
            major: number_value(
                &HKEY::LOCAL_MACHINE,
                CURRENT_VERSION_KEY,
                "CurrentMajorVersionNumber",
            ),
            versions,
        };
        tracing::debug!(?facts, "read the WebView2 runtime registry values");
        facts
    }
}

fn text_value(root: &HKEY, key: &str, name: &str) -> Option<String> {
    match root.RegGetValue(Some(key), Some(name)) {
        Ok(RegistryValue::Sz(text) | RegistryValue::ExpandSz(text)) => Some(text),
        _ => None,
    }
}

fn number_value(root: &HKEY, key: &str, name: &str) -> Option<u32> {
    match root.RegGetValue(Some(key), Some(name)) {
        Ok(RegistryValue::Dword(number)) => Some(number),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T4: this PC (Windows 10 or newer) reports its real major version.
    #[test]
    #[ignore = "reads this PC's registry"]
    fn t4_this_pc_reports_windows_10_or_newer() {
        let RuntimeFacts::Windows { major, versions } = RegistryProbe.facts() else {
            panic!("a Windows build must read Windows facts");
        };
        assert!(major.is_some_and(|major| major >= 10), "{major:?}");
        tracing::info!(?versions, "runtime versions on this PC");
    }
}
