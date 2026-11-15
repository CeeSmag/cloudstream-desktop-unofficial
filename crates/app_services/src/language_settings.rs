//! The viewer's audio and subtitle language choices.

use player_api::{find_language, Language, ENGLISH};

use crate::{ServiceError, SettingsService};

const PREFERRED_AUDIO_KEY: &str = "preferred_audio";
/// Stored when the viewer wants the stream's own default audio track.
const NO_AUDIO_PREFERENCE: &str = "none";
const PREFERRED_SUBTITLES_KEY: &str = "preferred_subtitles";
const AUTO_SUBTITLES_KEY: &str = "auto_subtitles";
/// Most subtitle languages the viewer can prefer; the player shows one flag for each.
pub const MAX_PREFERRED_SUBTITLES: usize = 3;

impl SettingsService {
    /// The language whose audio track plays when a stream offers it. English until the viewer
    /// chooses otherwise; `None` means the stream's own default track.
    pub fn preferred_audio(&self) -> Result<Option<Language>, ServiceError> {
        Ok(match self.stored(PREFERRED_AUDIO_KEY)?.as_deref() {
            None => Some(ENGLISH),
            Some(NO_AUDIO_PREFERENCE) => None,
            Some(code) => Some(find_language(code).unwrap_or(ENGLISH)),
        })
    }

    /// Remembers the preferred audio language (`None` for the stream's default).
    pub fn set_preferred_audio(&self, language: Option<Language>) -> Result<(), ServiceError> {
        let value = language.map_or(NO_AUDIO_PREFERENCE, |language| language.code());
        self.store_value(PREFERRED_AUDIO_KEY, value)
    }

    /// The languages whose subtitles get a flag in the player (at most
    /// [`MAX_PREFERRED_SUBTITLES`]), in the order the viewer chose them. English until changed.
    pub fn preferred_subtitles(&self) -> Result<Vec<Language>, ServiceError> {
        Ok(match self.stored(PREFERRED_SUBTITLES_KEY)? {
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
        self.store_value(PREFERRED_SUBTITLES_KEY, &codes.join(","))
    }

    /// Whether the subtitle in a preferred language turns on by itself when a video has one.
    /// On until the viewer switches it off.
    pub fn auto_select_subtitles(&self) -> Result<bool, ServiceError> {
        self.flag(AUTO_SUBTITLES_KEY)
    }

    /// Remembers whether a preferred-language subtitle turns on by itself.
    pub fn set_auto_select_subtitles(&self, enabled: bool) -> Result<(), ServiceError> {
        self.set_flag(AUTO_SUBTITLES_KEY, enabled)
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
    fn subtitles_turn_on_by_themselves_until_the_viewer_switches_that_off() {
        let service = service();
        assert!(service.auto_select_subtitles().expect("read"));
        service.set_auto_select_subtitles(false).expect("set");
        assert!(!service.auto_select_subtitles().expect("read"));
        service.set_auto_select_subtitles(true).expect("set");
        assert!(service.auto_select_subtitles().expect("read"));
    }

    #[test]
    fn an_unknown_stored_language_falls_back_to_english() {
        let service = service();
        service
            .store_value(PREFERRED_AUDIO_KEY, "zzz")
            .expect("write");
        assert_eq!(service.preferred_audio().expect("read"), Some(ENGLISH));
    }
}
