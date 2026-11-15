use storage::SettingsStore;

use crate::ServiceError;

const DISABLED_PROVIDERS_KEY: &str = "disabled_providers";
const DEBUG_LOGGING_KEY: &str = "debug_logging";
const AUTOPLAY_NEXT_KEY: &str = "autoplay_next";
const SOURCES_VALID_FROM_KEY: &str = "sources_valid_from";
const SKIP_LOOKUP_KEY: &str = "skip_lookup";
const ENABLED: &str = "yes";
const DISABLED: &str = "no";
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

    /// Whether the next episode starts by itself when one ends. On until the viewer switches it off.
    pub fn autoplay_next(&self) -> Result<bool, ServiceError> {
        self.flag(AUTOPLAY_NEXT_KEY)
    }

    /// Remembers whether the next episode starts by itself.
    pub fn set_autoplay_next(&self, enabled: bool) -> Result<(), ServiceError> {
        self.set_flag(AUTOPLAY_NEXT_KEY, enabled)
    }

    /// Whether skip times (intro, recap, credits) are looked up online by the title's id. On until
    /// the viewer switches it off, because the lookup tells a third-party site which title is played.
    pub fn skip_lookup(&self) -> Result<bool, ServiceError> {
        self.flag(SKIP_LOOKUP_KEY)
    }

    /// Remembers whether skip times are looked up online.
    pub fn set_skip_lookup(&self, enabled: bool) -> Result<(), ServiceError> {
        self.set_flag(SKIP_LOOKUP_KEY, enabled)
    }

    /// The time (seconds since 1970) of the last provider switch: saved stream links from before it
    /// are not used. 0 when the viewer never switched.
    pub fn sources_valid_from(&self) -> Result<u64, ServiceError> {
        Ok(self
            .store
            .get(SOURCES_VALID_FROM_KEY)?
            .and_then(|text| text.parse().ok())
            .unwrap_or(0))
    }

    /// Remembers the time of the last provider switch.
    pub fn set_sources_valid_from(&self, seconds: u64) -> Result<(), ServiceError> {
        self.store
            .set(SOURCES_VALID_FROM_KEY, &seconds.to_string())?;
        Ok(())
    }

    /// The stored text of `key`, for the setting files beside this one (`language_settings.rs`, ...).
    pub(crate) fn stored(&self, key: &str) -> Result<Option<String>, ServiceError> {
        Ok(self.store.get(key)?)
    }

    /// Stores `value` under `key`, for the setting files beside this one.
    pub(crate) fn store_value(&self, key: &str, value: &str) -> Result<(), ServiceError> {
        self.store.set(key, value)?;
        Ok(())
    }

    /// An on/off setting that is on until the viewer switches it off.
    pub(crate) fn flag(&self, key: &str) -> Result<bool, ServiceError> {
        Ok(self.store.get(key)?.as_deref() != Some(DISABLED))
    }

    /// Remembers an on/off setting read by [`SettingsService::flag`].
    pub(crate) fn set_flag(&self, key: &str, enabled: bool) -> Result<(), ServiceError> {
        self.store
            .set(key, if enabled { ENABLED } else { DISABLED })?;
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
    fn autoplay_and_skip_lookup_are_on_until_switched_off_and_independent() {
        let service = service();
        assert!(service.autoplay_next().expect("read"));
        assert!(service.skip_lookup().expect("read"));
        service.set_autoplay_next(false).expect("set");
        assert!(!service.autoplay_next().expect("read"));
        assert!(service.skip_lookup().expect("read"), "the other stays on");
        service.set_skip_lookup(false).expect("set");
        service.set_autoplay_next(true).expect("set");
        assert!(service.autoplay_next().expect("read"));
        assert!(!service.skip_lookup().expect("read"));
    }

    #[test]
    fn the_time_of_the_last_switch_round_trips_and_garbage_means_never() {
        let service = service();
        assert_eq!(service.sources_valid_from().expect("read"), 0);
        service.set_sources_valid_from(1_700_000_123).expect("set");
        assert_eq!(service.sources_valid_from().expect("read"), 1_700_000_123);
        service
            .store
            .set(SOURCES_VALID_FROM_KEY, "yesterday")
            .expect("write");
        assert_eq!(service.sources_valid_from().expect("read"), 0);
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
