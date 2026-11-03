use player_api::{find_language, Language, ENGLISH};
use storage::SettingsStore;

use crate::ServiceError;

const DISABLED_PROVIDERS_KEY: &str = "disabled_providers";
const DEBUG_LOGGING_KEY: &str = "debug_logging";
const PREFERRED_AUDIO_KEY: &str = "preferred_audio";
/// Stored when the viewer wants the stream's own default audio track.
const NO_AUDIO_PREFERENCE: &str = "none";
const PREFERRED_SUBTITLES_KEY: &str = "preferred_subtitles";
/// Most subtitle languages the viewer can prefer; the player shows one flag for each.
pub const MAX_PREFERRED_SUBTITLES: usize = 3;
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

    /// The language whose audio track plays when a stream offers it. English until the viewer
    /// chooses otherwise; `None` means the stream's own default track.
    pub fn preferred_audio(&self) -> Result<Option<Language>, ServiceError> {
        Ok(match self.store.get(PREFERRED_AUDIO_KEY)?.as_deref() {
            None => Some(ENGLISH),
            Some(NO_AUDIO_PREFERENCE) => None,
            Some(code) => Some(find_language(code).unwrap_or(ENGLISH)),
        })
    }

    /// Remembers the preferred audio language (`None` for the stream's default).
    pub fn set_preferred_audio(&self, language: Option<Language>) -> Result<(), ServiceError> {
        let value = language.map_or(NO_AUDIO_PREFERENCE, |language| language.code());
        self.store.set(PREFERRED_AUDIO_KEY, value)?;
        Ok(())
    }

    /// The languages whose subtitles get a flag in the player (at most
    /// [`MAX_PREFERRED_SUBTITLES`]), in the order the viewer chose them. English until changed.
    pub fn preferred_subtitles(&self) -> Result<Vec<Language>, ServiceError> {
        Ok(match self.store.get(PREFERRED_SUBTITLES_KEY)? {
            None => vec![ENGLISH],
            Some(raw) => raw
                .split(',')
                .filter_map(find_language)
                .take(MAX_PREFERRED_SUBTITLES)
                .collect(),
        })
    }

    /// Remembers the preferred subtitle languages; only the first [`MAX_PREFERRED_SUBTITLES`] count.
    pub fn set_preferred_subtitles(&self, languages: &[Language]) -> Result<(), ServiceError> {
        let codes: Vec<&str> = languages
            .iter()
            .take(MAX_PREFERRED_SUBTITLES)
            .map(Language::code)
            .collect();
        self.store.set(PREFERRED_SUBTITLES_KEY, &codes.join(","))?;
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
    fn the_preferred_audio_is_english_until_changed_and_the_choice_is_kept() {
        let service = service();
        assert_eq!(service.preferred_audio().expect("read"), Some(ENGLISH));
        let hindi = find_language("hin");
        service.set_preferred_audio(hindi).expect("set");
        assert_eq!(service.preferred_audio().expect("read"), hindi);
        service.set_preferred_audio(None).expect("clear");
        assert_eq!(service.preferred_audio().expect("read"), None);
        service.set_preferred_audio(Some(ENGLISH)).expect("set");
        assert_eq!(service.preferred_audio().expect("read"), Some(ENGLISH));
    }

    #[test]
    fn the_preferred_subtitles_are_english_until_changed_and_keep_their_order_and_limit() {
        let service = service();
        assert_eq!(service.preferred_subtitles().expect("read"), vec![ENGLISH]);
        let chosen: Vec<Language> = ["spa", "fre", "ger", "ita"]
            .iter()
            .filter_map(|code| find_language(code))
            .collect();
        service.set_preferred_subtitles(&chosen).expect("set");
        let saved = service.preferred_subtitles().expect("read");
        let names: Vec<&str> = saved.iter().map(|language| language.name).collect();
        assert_eq!(names, ["Spanish", "French", "German"], "three at most");
        service.set_preferred_subtitles(&[]).expect("clear");
        assert!(service.preferred_subtitles().expect("read").is_empty());
    }

    #[test]
    fn an_unknown_stored_language_falls_back_to_english() {
        let service = service();
        service
            .store
            .set(PREFERRED_AUDIO_KEY, "zzz")
            .expect("write");
        assert_eq!(service.preferred_audio().expect("read"), Some(ENGLISH));
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
