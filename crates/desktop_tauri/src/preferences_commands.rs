//! Tauri commands for the Preferences page of the settings.

use std::sync::Arc;

use app_services::{PlaybackService, SettingsService, MAX_PREFERRED_SUBTITLES};
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
    /// Every code that stands for the language (`eng`, `en`), to recognise it on a track.
    pub codes: Vec<String>,
}

/// The viewer's preferences and the choices on offer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PreferencesDto {
    /// Code of the preferred audio language; absent when the stream's own default is wanted.
    pub preferred_audio: Option<String>,
    /// Codes of the languages whose subtitles get a flag in the player, in the chosen order.
    pub preferred_subtitles: Vec<String>,
    /// Whether the subtitle in the first of those languages the video has turns on by itself.
    pub auto_subtitles: bool,
    /// Whether the next episode starts by itself when one ends.
    pub autoplay_next: bool,
    /// Whether skip times are looked up online by the title's id.
    pub skip_lookup: bool,
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
    if let Some(playback) = playback {
        playback.set_preferred_subtitles(&subtitle_languages_to_turn_on(settings));
    }
}

/// The languages whose subtitles turn on by themselves: the preferred ones, or none when the
/// viewer switched that off.
fn subtitle_languages_to_turn_on(settings: &SettingsService) -> Vec<player_api::Language> {
    if !settings.auto_select_subtitles().unwrap_or(true) {
        return Vec::new();
    }
    settings.preferred_subtitles().unwrap_or_else(|error| {
        tracing::warn!(%error, "cannot read the preferred subtitle languages");
        Vec::new()
    })
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

/// Saves the languages whose subtitles get a flag in the player: at most three, each known.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(languages = ?languages), err)]
pub async fn set_preferred_subtitles(
    languages: Vec<String>,
    state: State<'_, AppState>,
) -> Result<PreferencesDto, String> {
    let mut chosen = Vec::new();
    for code in &languages {
        let language = find_language(code).ok_or_else(|| format!("Unknown language: {code}"))?;
        if !chosen.contains(&language) {
            chosen.push(language);
        }
    }
    if chosen.len() > MAX_PREFERRED_SUBTITLES {
        return Err(format!(
            "Choose at most {MAX_PREFERRED_SUBTITLES} subtitle languages."
        ));
    }
    let services = state.read();
    services
        .settings
        .set_preferred_subtitles(&chosen)
        .map_err(|error| error.to_string())?;
    apply_subtitle_choice(&services);
    describe(&services.settings)
}

/// Switches the automatic subtitle choice on or off.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(enabled = ?enabled), err)]
pub async fn set_auto_subtitles(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<PreferencesDto, String> {
    let services = state.read();
    services
        .settings
        .set_auto_select_subtitles(enabled)
        .map_err(|error| error.to_string())?;
    apply_subtitle_choice(&services);
    describe(&services.settings)
}

/// Switches one playback option on or off: `autoplay_next` or `skip_lookup`.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(option = ?option, enabled = ?enabled), err)]
pub async fn set_playback_option(
    option: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<PreferencesDto, String> {
    let services = state.read();
    apply_option(&services.settings, &option, enabled)?;
    describe(&services.settings)
}

fn apply_option(settings: &SettingsService, option: &str, enabled: bool) -> Result<(), String> {
    match option {
        "autoplay_next" => settings.set_autoplay_next(enabled),
        "skip_lookup" => settings.set_skip_lookup(enabled),
        other => return Err(format!("Unknown playback option: {other}")),
    }
    .map_err(|error| error.to_string())
}

fn apply_subtitle_choice(services: &ui_app::UiServices) {
    if let Some(playback) = &services.playback {
        playback.set_preferred_subtitles(&subtitle_languages_to_turn_on(&services.settings));
    }
}

fn describe(settings: &SettingsService) -> Result<PreferencesDto, String> {
    let preferred = settings
        .preferred_audio()
        .map_err(|error| error.to_string())?;
    let subtitles = settings
        .preferred_subtitles()
        .map_err(|error| error.to_string())?;
    Ok(PreferencesDto {
        auto_subtitles: settings.auto_select_subtitles().unwrap_or(true),
        autoplay_next: settings.autoplay_next().unwrap_or(true),
        skip_lookup: settings.skip_lookup().unwrap_or(true),
        preferred_audio: preferred.map(|language| language.code().to_owned()),
        preferred_subtitles: subtitles
            .iter()
            .map(|language| language.code().to_owned())
            .collect(),
        languages: all_languages()
            .iter()
            .map(|language| LanguageDto {
                code: language.code().to_owned(),
                name: language.name.to_owned(),
                codes: language
                    .codes
                    .iter()
                    .map(|code| (*code).to_owned())
                    .collect(),
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
    fn the_description_lists_the_preferred_subtitles_and_every_code_of_a_language() {
        let settings = settings();
        let described = describe(&settings).expect("describes");
        assert_eq!(described.preferred_subtitles, ["eng"]);
        let french = described.languages.iter().find(|l| l.name == "French");
        assert_eq!(french.map(|l| l.codes.len()), Some(3));
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

    #[test]
    fn playback_options_are_saved_by_name_and_an_unknown_name_is_refused() {
        let settings = settings();
        apply_option(&settings, "autoplay_next", false).expect("saves");
        apply_option(&settings, "skip_lookup", false).expect("saves");
        let described = describe(&settings).expect("describes");
        assert!(!described.autoplay_next && !described.skip_lookup);
        let refused = apply_option(&settings, "volume", false).expect_err("unknown");
        assert!(refused.contains("volume"), "{refused}");
    }
}
