//! The sites the viewer allowed to open a Cloudflare verification window.

use crate::{ServiceError, SettingsService};

const CLOUDFLARE_ALLOWED_KEY: &str = "cloudflare_allowed_hosts";
/// Hosts cannot contain spaces, so one separates them.
const HOST_SEPARATOR: char = ' ';

impl SettingsService {
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
