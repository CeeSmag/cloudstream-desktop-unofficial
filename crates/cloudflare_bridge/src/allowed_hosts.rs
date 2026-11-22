//! The sites the viewer allowed to open a verification window.
//!
//! A viewer's consent permits checks for that host. Each request still owns its own
//! presentation decision and waits for the viewer's action when permission is missing.
//! Allowing a site covers its subdomains (`cdn.site.test` under `site.test`).
//! Single-label names (`com`) are refused: the suffix rule would otherwise match every `*.com`.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};

/// Shared between the UI (which adds sites) and the watcher thread (which reads them).
#[derive(Debug, Clone, Default)]
pub struct AllowedHosts {
    hosts: Arc<RwLock<HashSet<String>>>,
}

impl AllowedHosts {
    /// Starts with `hosts` (the ones saved in the settings); single-label names are dropped.
    pub fn new(hosts: impl IntoIterator<Item = String>) -> Self {
        let hosts = hosts
            .into_iter()
            .map(|host| normalize(&host))
            .filter(|host| is_multi_label_host(host))
            .collect();
        Self {
            hosts: Arc::new(RwLock::new(hosts)),
        }
    }

    /// Allows `host` and its subdomains; true when it was not allowed before.
    /// Single-label hosts are ignored (too broad for the subdomain rule).
    pub fn allow(&self, host: &str) -> bool {
        let host = normalize(host);
        if !is_multi_label_host(&host) {
            return false;
        }
        self.hosts
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(host)
    }

    /// True when `host`, or a site it belongs to, was allowed.
    pub fn is_allowed(&self, host: &str) -> bool {
        let host = normalize(host);
        let hosts = self.hosts.read().unwrap_or_else(PoisonError::into_inner);
        hosts.iter().any(|allowed| {
            is_multi_label_host(allowed)
                && (host == *allowed
                    || host
                        .strip_suffix(allowed.as_str())
                        .is_some_and(|prefix| prefix.ends_with('.')))
        })
    }
}

/// True when `host` has at least two non-empty labels (`site.test`, not `com` or `.test`).
pub(crate) fn is_multi_label_host(host: &str) -> bool {
    let host = normalize(host);
    !host.is_empty() && host.contains('.') && !host.split('.').any(str::is_empty)
}

fn normalize(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_allowed_covers_subdomains_not_lookalikes() {
        let allowed = AllowedHosts::new(["Site.Test".to_owned()]);
        assert!(allowed.is_allowed("site.test"));
        assert!(allowed.is_allowed("cdn.site.test"));
        assert!(!allowed.is_allowed("evilsite.test"));
        assert!(!allowed.is_allowed("other.test"));
    }

    #[test]
    fn allow_reports_new_sites_only() {
        let allowed = AllowedHosts::default();
        assert!(allowed.allow("a.test"));
        assert!(!allowed.allow("A.TEST."));
        assert!(allowed.is_allowed("a.test"));
    }

    #[test]
    fn allow_rejects_single_label_hosts_that_would_match_every_tld_suffix() {
        let allowed = AllowedHosts::default();
        assert!(!allowed.allow("com"));
        assert!(!allowed.is_allowed("evil.com"));
        assert!(!AllowedHosts::new(["com".to_owned()]).is_allowed("evil.com"));
        assert!(allowed.allow("site.test"));
        assert!(allowed.is_allowed("cdn.site.test"));
    }

    #[test]
    fn is_multi_label_host_needs_two_labels() {
        assert!(is_multi_label_host("site.test"));
        assert!(!is_multi_label_host("com"));
        assert!(!is_multi_label_host(""));
        assert!(!is_multi_label_host(".test"));
        assert!(!is_multi_label_host("site."));
    }

    #[test]
    fn allow_from_one_thread_is_seen_by_another() {
        let allowed = AllowedHosts::default();
        let writer = allowed.clone();
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let writer = writer.clone();
                std::thread::spawn(move || {
                    for round in 0..200 {
                        writer.allow(&format!("h{index}-{round}.test"));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("writer");
        }
        assert!((0..8).all(
            |index| (0..200).all(|round| allowed.is_allowed(&format!("h{index}-{round}.test")))
        ));
    }
}
