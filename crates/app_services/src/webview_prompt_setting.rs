//! Whether the viewer closed the WebView2 prompt that appears after Home, per app version.
//!
//! A closed prompt does not come back at launch until the next app version; the dialog still opens
//! when a Cloudflare check needs WebView2, and from Settings.

use crate::{ServiceError, SettingsService};

const WEBVIEW_PROMPT_KEY: &str = "webview2_prompt_dismissed";

impl SettingsService {
    /// The app version in which the viewer last closed the launch prompt, if any.
    pub fn webview_prompt_dismissed(&self) -> Result<Option<String>, ServiceError> {
        Ok(self
            .stored(WEBVIEW_PROMPT_KEY)?
            .filter(|version| !version.is_empty()))
    }

    /// Remembers that the viewer closed the launch prompt in `app_version`.
    pub fn set_webview_prompt_dismissed(&self, app_version: &str) -> Result<(), ServiceError> {
        self.store_value(WEBVIEW_PROMPT_KEY, app_version)
    }

    /// Whether the launch prompt was closed in `app_version` (a read error counts as not closed,
    /// so the viewer is told about a missing runtime rather than never).
    pub fn webview_prompt_dismissed_for(&self, app_version: &str) -> bool {
        match self.webview_prompt_dismissed() {
            Ok(dismissed) => dismissed.as_deref() == Some(app_version),
            Err(error) => {
                tracing::warn!(%error, "cannot read whether the WebView2 prompt was closed");
                false
            }
        }
    }
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
    fn the_prompt_is_not_dismissed_until_closed() {
        let service = service();
        assert_eq!(service.webview_prompt_dismissed().expect("read"), None);
        assert!(!service.webview_prompt_dismissed_for("0.5.5"));
    }

    #[test]
    fn closing_counts_for_that_version_only() {
        let service = service();
        service.set_webview_prompt_dismissed("0.5.5").expect("set");
        assert_eq!(
            service.webview_prompt_dismissed().expect("read").as_deref(),
            Some("0.5.5")
        );
        assert!(service.webview_prompt_dismissed_for("0.5.5"));
        assert!(!service.webview_prompt_dismissed_for("0.5.6"));
    }

    #[test]
    fn an_empty_value_counts_as_not_closed() {
        let service = service();
        service.set_webview_prompt_dismissed("").expect("set");
        assert_eq!(service.webview_prompt_dismissed().expect("read"), None);
        assert!(!service.webview_prompt_dismissed_for(""));
    }
}
