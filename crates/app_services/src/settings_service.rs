use storage::SettingsStore;

use crate::ServiceError;

const DISABLED_PROVIDERS_KEY: &str = "disabled_providers";
const DEBUG_LOGGING_KEY: &str = "debug_logging";
const LIST_SEPARATOR: char = '\n';

/// Typed access to the app's settings.
#[derive(Clone)]
pub struct SettingsService {
    store: SettingsStore,
}

impl SettingsService {
    /// Creates the service over a settings store.
    pub fn new(store: SettingsStore) -> Self {
        Self { store }
    }

    /// Names of providers the user switched off; they are skipped by search.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn disabled_providers(&self) -> Result<Vec<String>, ServiceError> {
        let raw = self.store.get(DISABLED_PROVIDERS_KEY)?.unwrap_or_default();
        Ok(raw
            .split(LIST_SEPARATOR)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// Whether the viewer switched debug logging on; off unless they did.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn debug_logging_enabled(&self) -> Result<bool, ServiceError> {
        Ok(self.store.get(DEBUG_LOGGING_KEY)?.as_deref() == Some("on"))
    }

    /// Remembers whether debug logging is on, so it stays that way after a restart.
    #[tracing::instrument(level = "debug", skip_all, fields(enabled = ?enabled), err)]
    pub fn set_debug_logging_enabled(&self, enabled: bool) -> Result<(), ServiceError> {
        self.store
            .set(DEBUG_LOGGING_KEY, if enabled { "on" } else { "off" })?;
        Ok(())
    }

    /// Switches a provider on or off.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, enabled = ?enabled), err)]
    pub fn set_provider_enabled(&self, provider: &str, enabled: bool) -> Result<(), ServiceError> {
        let mut disabled = self.disabled_providers()?;
        disabled.retain(|name| name != provider);
        if !enabled {
            disabled.push(provider.to_owned());
        }
        self.store.set(
            DISABLED_PROVIDERS_KEY,
            &disabled.join(&LIST_SEPARATOR.to_string()),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::Database;

    use super::*;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    #[test]
    fn debug_logging_is_off_until_switched_on_and_the_choice_is_kept() {
        let service = service();
        assert!(!service.debug_logging_enabled().expect("read"));
        service.set_debug_logging_enabled(true).expect("on");
        assert!(service.debug_logging_enabled().expect("read"));
        service.set_debug_logging_enabled(false).expect("off");
        assert!(!service.debug_logging_enabled().expect("read"));
    }

    #[test]
    fn providers_are_enabled_by_default() {
        assert!(service().disabled_providers().expect("read").is_empty());
    }

    #[test]
    fn disabling_and_re_enabling_a_provider_round_trips() {
        let service = service();
        service.set_provider_enabled("A", false).expect("disable");
        service.set_provider_enabled("B", false).expect("disable");
        assert_eq!(service.disabled_providers().expect("read"), ["A", "B"]);
        service.set_provider_enabled("A", true).expect("enable");
        assert_eq!(service.disabled_providers().expect("read"), ["B"]);
    }

    #[test]
    fn disabling_twice_does_not_duplicate() {
        let service = service();
        service.set_provider_enabled("A", false).expect("disable");
        service
            .set_provider_enabled("A", false)
            .expect("disable again");
        assert_eq!(service.disabled_providers().expect("read"), ["A"]);
    }
}
