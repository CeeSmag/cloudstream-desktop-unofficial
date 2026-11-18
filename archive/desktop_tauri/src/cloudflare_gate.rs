//! Which sites the viewer has said may open a Cloudflare verification window.
//!
//! A window that pops up by itself, for a provider the viewer has not touched (the first provider's
//! Home page loads when the app starts), would be a surprise. So a verification window opens only for
//! a site the viewer asked for: by picking its provider, or by pressing "Verify" on the Home page.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use core_model::ProviderInfo;
use tauri::Url;

/// The sites the viewer has allowed during this run of the app.
#[derive(Debug, Default)]
pub struct Gate {
    allowed: Mutex<HashSet<String>>,
}

impl Gate {
    /// Allows a verification window for `host` and its subdomains.
    pub fn allow(&self, host: &str) {
        let mut allowed = self.allowed.lock().unwrap_or_else(PoisonError::into_inner);
        allowed.insert(host.trim().to_ascii_lowercase());
    }

    /// Whether `host` (or a site it belongs to) was allowed.
    pub fn is_allowed(&self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        let allowed = self.allowed.lock().unwrap_or_else(PoisonError::into_inner);
        allowed
            .iter()
            .any(|site| host == *site || host.ends_with(&format!(".{site}")))
    }
}

/// The host of the site a provider scrapes.
pub fn host_of_provider(providers: &[ProviderInfo], name: &str) -> Option<String> {
    let provider = providers.iter().find(|provider| provider.name == name)?;
    Url::parse(&provider.main_url)
        .ok()?
        .host_str()
        .map(str::to_owned)
}

/// The provider that scrapes `host`, if any.
pub fn provider_of_host(providers: &[ProviderInfo], host: &str) -> Option<String> {
    providers
        .iter()
        .find(|provider| {
            let own = Url::parse(&provider.main_url).ok();
            let own = own.as_ref().and_then(|url| url.host_str());
            own.is_some_and(|own| host == own || host.ends_with(&format!(".{own}")))
        })
        .map(|provider| provider.name.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn provider(name: &str, url: &str) -> ProviderInfo {
        ProviderInfo {
            name: name.to_owned(),
            main_url: url.to_owned(),
            ..ProviderInfo::default()
        }
    }

    #[test]
    fn nothing_is_allowed_until_the_viewer_asks() {
        let gate = Gate::default();
        assert!(!gate.is_allowed("animepahe.pw"));
        gate.allow("animepahe.pw");
        assert!(gate.is_allowed("animepahe.pw"));
        assert!(
            gate.is_allowed("API.AnimePahe.pw"),
            "a subdomain, whatever its case"
        );
        assert!(
            !gate.is_allowed("notanimepahe.pw"),
            "a different site that ends the same way"
        );
        assert!(!gate.is_allowed("other.test"));
    }

    #[test]
    fn a_provider_and_its_host_are_found_from_each_other() {
        let providers = vec![
            provider("AnimePahe", "https://animepahe.pw"),
            provider("Other", "https://other.test/x"),
        ];
        assert_eq!(
            host_of_provider(&providers, "AnimePahe").as_deref(),
            Some("animepahe.pw")
        );
        assert_eq!(host_of_provider(&providers, "Nope"), None);
        assert_eq!(
            provider_of_host(&providers, "animepahe.pw").as_deref(),
            Some("AnimePahe")
        );
        assert_eq!(
            provider_of_host(&providers, "cdn.other.test").as_deref(),
            Some("Other")
        );
        assert_eq!(provider_of_host(&providers, "unknown.test"), None);
    }

    /// Many threads allow sites while others ask: once an `allow` has returned, that site is allowed
    /// for every later question, from any thread.
    #[test]
    fn an_allowed_site_is_allowed_for_every_thread_at_once() {
        let gate = Arc::new(Gate::default());
        let workers: Vec<_> = (0..8)
            .map(|worker| {
                let gate = Arc::clone(&gate);
                std::thread::spawn(move || {
                    for round in 0..400 {
                        let host = format!("site{worker}-{round}.test");
                        assert!(
                            !gate.is_allowed(&host),
                            "not allowed before it was asked for"
                        );
                        gate.allow(&host);
                        assert!(gate.is_allowed(&host), "allowed right after");
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("worker finished");
        }
    }
}
