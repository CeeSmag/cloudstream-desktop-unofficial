//! The sites the viewer allowed to open a verification window.
//!
//! A window opens only for a site the viewer said yes to (pressing Verify); every other request is
//! answered at once with "ask again" so the plugin fails fast and the UI can show the question.
//! Allowing a site covers its subdomains (`cdn.site.test` under `site.test`).

use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};

/// Shared between the UI (which adds sites) and the watcher thread (which reads them).
#[derive(Debug, Clone, Default)]
pub struct AllowedHosts {
    hosts: Arc<RwLock<HashSet<String>>>,
}

impl AllowedHosts {
    /// Starts with `hosts` (the ones saved in the settings).
    pub fn new(hosts: impl IntoIterator<Item = String>) -> Self {
        let hosts = hosts.into_iter().map(|host| normalize(&host)).collect();
        Self {
            hosts: Arc::new(RwLock::new(hosts)),
        }
    }

    /// Allows `host` and its subdomains; true when it was not allowed before.
    pub fn allow(&self, host: &str) -> bool {
        self.hosts
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(normalize(host))
    }

    /// True when `host`, or a site it belongs to, was allowed.
    pub fn is_allowed(&self, host: &str) -> bool {
        let host = normalize(host);
        let hosts = self.hosts.read().unwrap_or_else(PoisonError::into_inner);
        hosts.iter().any(|allowed| {
            host == *allowed
                || host
                    .strip_suffix(allowed.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }
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
