//! The sites the viewer allowed to open a Cloudflare verification window.

use memory_budget::{HelperIdle, HelperWindows};

use crate::{ServiceError, SettingsService};

const CLOUDFLARE_ALLOWED_KEY: &str = "cloudflare_allowed_hosts";
pub(crate) const HELPER_IDLE_KEY: &str = "cloudflare_helper_idle";
pub(crate) const HELPER_WINDOWS_KEY: &str = "cloudflare_helper_windows";
/// Hosts cannot contain spaces, so one separates them.
const HOST_SEPARATOR: char = ' ';

impl SettingsService {
    /// How long the verification helper may sit idle before it is closed: what the viewer chose,
    /// else the profile's value (2 minutes on a low-end PC, never on a high-end one).
    pub fn helper_idle(&self) -> Result<HelperIdle, ServiceError> {
        Ok(self.performance_budget()?.helper_idle)
    }

    /// Remembers the viewer's choice of idle time for the verification helper.
    pub fn set_helper_idle(&self, idle: HelperIdle) -> Result<(), ServiceError> {
        self.store_value(HELPER_IDLE_KEY, &idle.to_text())
    }

    /// The viewer's own choice, if one is stored and readable.
    pub(crate) fn stored_helper_idle(&self) -> Result<Option<HelperIdle>, ServiceError> {
        Ok(self
            .stored(HELPER_IDLE_KEY)?
            .as_deref()
            .and_then(HelperIdle::parse))
    }

    /// How many verified browser windows the helper keeps: what the viewer chose, else the
    /// profile's value (2 on a low-end PC). Each one is a browser process of its own.
    pub fn helper_windows(&self) -> Result<HelperWindows, ServiceError> {
        Ok(self.performance_budget()?.helper_windows)
    }

    /// Remembers the viewer's choice of how many verified browser windows the helper keeps.
    pub fn set_helper_windows(&self, windows: HelperWindows) -> Result<(), ServiceError> {
        self.store_value(HELPER_WINDOWS_KEY, &windows.to_text())
    }

    /// The viewer's own choice, if one is stored and offered.
    pub(crate) fn stored_helper_windows(&self) -> Result<Option<HelperWindows>, ServiceError> {
        Ok(self
            .stored(HELPER_WINDOWS_KEY)?
            .as_deref()
            .and_then(HelperWindows::parse))
    }

    /// Sites the viewer pressed Verify for; their checks open a window without asking again.
    /// Single-label names are dropped (they would match every host under that TLD).
    pub fn cloudflare_allowed_hosts(&self) -> Result<Vec<String>, ServiceError> {
        let raw = self.stored(CLOUDFLARE_ALLOWED_KEY)?.unwrap_or_default();
        Ok(raw
            .split(HOST_SEPARATOR)
            .filter(|host| is_multi_label_host(host))
            .map(str::to_owned)
            .collect())
    }

    /// Remembers that the viewer allowed `host`'s check (kept once, lower case).
    pub fn allow_cloudflare_host(&self, host: &str) -> Result<(), ServiceError> {
        let host = host.trim().to_ascii_lowercase();
        if !is_multi_label_host(&host) || host.contains(char::is_whitespace) {
            return Ok(());
        }
        let mut hosts = self.cloudflare_allowed_hosts()?;
        if hosts.contains(&host) {
            return Ok(());
        }
        hosts.push(host);
        let joined = hosts.join(&HOST_SEPARATOR.to_string());
        self.store_value(CLOUDFLARE_ALLOWED_KEY, &joined)
    }
}

/// Same rule as `cloudflare_bridge::allowed_hosts`: at least two non-empty labels.
fn is_multi_label_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.');
    !host.is_empty() && host.contains('.') && !host.split('.').any(str::is_empty)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    #[test]
    fn allow_cloudflare_host_is_kept_once_in_lower_case() {
        let service = service();
        assert!(service.cloudflare_allowed_hosts().expect("read").is_empty());
        service.allow_cloudflare_host("Site.Test").expect("allow");
        service.allow_cloudflare_host("site.test").expect("again");
        service.allow_cloudflare_host("other.test").expect("other");
        assert_eq!(
            service.cloudflare_allowed_hosts().expect("read"),
            ["site.test", "other.test"]
        );
    }

    #[test]
    fn allow_cloudflare_host_ignores_blank_and_spaced_names() {
        let service = service();
        service.allow_cloudflare_host("  ").expect("blank");
        service.allow_cloudflare_host("a b").expect("spaced");
        assert!(service.cloudflare_allowed_hosts().expect("read").is_empty());
    }

    #[test]
    fn allow_cloudflare_host_ignores_single_label_tld_like_names() {
        let service = service();
        service.allow_cloudflare_host("com").expect("tld");
        assert!(service.cloudflare_allowed_hosts().expect("read").is_empty());
        service.allow_cloudflare_host("site.test").expect("ok");
        assert_eq!(
            service.cloudflare_allowed_hosts().expect("read"),
            ["site.test"]
        );
    }
}
