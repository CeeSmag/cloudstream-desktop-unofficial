//! Tauri commands for the Preferences page of the settings.

use std::sync::Arc;

use app_services::{PlaybackService, SettingsService};
use player_api::{all_languages, find_language};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

/// A language the viewer can pick.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LanguageDto {
    /// Code to send back when this language is chosen, for example `eng`.
    pub code: String,
    /// Readable name, for example `English`.
    pub name: String,
}

/// The viewer's preferences and the choices on offer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PreferencesDto {
    /// Code of the preferred audio language; absent when the stream's own default is wanted.
    pub preferred_audio: Option<String>,
    /// Every language that can be chosen, English first.
    pub languages: Vec<LanguageDto>,
}

/// Tells the player about the saved audio preference; run once at start.
pub fn apply_saved(settings: &SettingsService, playback: Option<&Arc<PlaybackService>>) {
    match settings.preferred_audio() {
        Ok(language) => {
            if let Some(playback) = playback {
                playback.set_preferred_audio(language);
            }
        }
        Err(error) => tracing::warn!(%error, "cannot read the preferred audio language"),
    }
}

/// The current preferences and the languages that can be chosen.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_preferences(state: State<'_, AppState>) -> Result<PreferencesDto, String> {
    describe(&state.read().settings)
}

/// Saves the preferred audio language (`None` for the stream's own default) and applies it to the
/// next stream that plays.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(language = ?language), err)]
pub async fn set_preferred_audio(
    language: Option<String>,
    state: State<'_, AppState>,
) -> Result<PreferencesDto, String> {
    let chosen = match language.as_deref() {
        None => None,
        Some(code) => Some(find_language(code).ok_or_else(|| format!("Unknown language: {code}"))?),
    };
    let services = state.read();
    services
        .settings
        .set_preferred_audio(chosen)
        .map_err(|error| error.to_string())?;
    if let Some(playback) = &services.playback {
        playback.set_preferred_audio(chosen);
    }
    describe(&services.settings)
}

fn describe(settings: &SettingsService) -> Result<PreferencesDto, String> {
    let preferred = settings
        .preferred_audio()
        .map_err(|error| error.to_string())?;
    Ok(PreferencesDto {
        preferred_audio: preferred.map(|language| language.code().to_owned()),
        languages: all_languages()
            .iter()
            .map(|language| LanguageDto {
                code: language.code().to_owned(),
                name: language.name.to_owned(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn settings() -> SettingsService {
        SettingsService::new(SettingsStore::new(Arc::new(
            Database::open_in_memory().expect("memory db"),
        )))
    }

    #[test]
    fn the_description_starts_with_english_and_lists_english_first() {
        let described = describe(&settings()).expect("describes");
        assert_eq!(described.preferred_audio.as_deref(), Some("eng"));
        assert_eq!(
            described.languages.first().map(|l| l.name.as_str()),
            Some("English")
        );
        assert!(described.languages.len() > 30);
    }

    #[test]
    fn no_preference_is_reported_as_none() {
        let settings = settings();
        settings.set_preferred_audio(None).expect("saves");
        assert_eq!(
            describe(&settings).expect("describes").preferred_audio,
            None
        );
    }
}
