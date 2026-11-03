//! Chooses the audio track to play: the viewer's preferred language when the stream has it, else
//! whatever the stream defaults to.

use std::sync::{Mutex, PoisonError};

use player_api::{Language, MediaTrack, TrackKind, ENGLISH};

/// What to do about the audio track right after a stream has loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioChoice {
    /// Leave things as they are: the preferred language is already playing, or there is nothing
    /// to choose from.
    Keep,
    /// Switch to the track in the preferred language with this id.
    Switch { id: i64 },
    /// The stream has audio but none in the preferred language; the stream's own default plays.
    NotOffered,
}

/// Decides from the track list. The first track in `language` wins when the playing one is not.
pub(crate) fn preferred_audio_choice(tracks: &[MediaTrack], language: &Language) -> AudioChoice {
    let audio: Vec<&MediaTrack> = tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .collect();
    if audio.is_empty()
        || audio
            .iter()
            .any(|track| track.selected && track.is_language(language))
    {
        return AudioChoice::Keep;
    }
    audio
        .iter()
        .find(|track| track.is_language(language))
        .map_or(AudioChoice::NotOffered, |track| AudioChoice::Switch {
            id: track.id,
        })
}

/// The language the viewer wants to hear; shared between the player and its event thread so a
/// change in the settings applies to the next stream without restarting anything.
pub(crate) struct AudioPreference {
    language: Mutex<Option<Language>>,
}

impl Default for AudioPreference {
    /// English, as the player is configured at start.
    fn default() -> Self {
        Self {
            language: Mutex::new(Some(ENGLISH)),
        }
    }
}

impl AudioPreference {
    /// The preferred language, or `None` when the stream's own default should play.
    pub(crate) fn get(&self) -> Option<Language> {
        *self.language.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn set(&self, language: Option<Language>) {
        *self.language.lock().unwrap_or_else(PoisonError::into_inner) = language;
    }
}

/// The value of mpv's `alang` option for a preference: its codes in order, empty for none.
pub(crate) fn alang_value(language: Option<&Language>) -> String {
    language.map_or_else(String::new, |language| language.codes.join(","))
}

#[cfg(test)]
mod tests {
    use player_api::find_language;

    use super::*;

    fn audio(id: i64, language: &str, title: &str, selected: bool) -> MediaTrack {
        MediaTrack {
            id,
            kind: TrackKind::Audio,
            language: language.to_owned(),
            title: title.to_owned(),
            selected,
            external: false,
        }
    }

    fn hindi() -> Language {
        find_language("hin").expect("Hindi is known")
    }

    #[test]
    fn the_preferred_language_already_playing_is_left_alone() {
        let tracks = [audio(1, "eng", "", true), audio(2, "hin", "", false)];
        assert_eq!(preferred_audio_choice(&tracks, &ENGLISH), AudioChoice::Keep);
    }

    #[test]
    fn the_preferred_language_is_chosen_over_another_default() {
        let tracks = [audio(1, "hin", "", true), audio(2, "eng", "", false)];
        assert_eq!(
            preferred_audio_choice(&tracks, &ENGLISH),
            AudioChoice::Switch { id: 2 }
        );
        assert_eq!(preferred_audio_choice(&tracks, &hindi()), AudioChoice::Keep);
    }

    #[test]
    fn a_track_titled_with_the_language_but_without_a_tag_counts() {
        let tracks = [
            audio(1, "", "Hindi", true),
            audio(2, "und", "English 5.1", false),
        ];
        assert_eq!(
            preferred_audio_choice(&tracks, &ENGLISH),
            AudioChoice::Switch { id: 2 }
        );
    }

    #[test]
    fn without_the_language_the_default_plays() {
        let tracks = [audio(1, "hin", "", true), audio(2, "tam", "", false)];
        assert_eq!(
            preferred_audio_choice(&tracks, &ENGLISH),
            AudioChoice::NotOffered
        );
    }

    #[test]
    fn a_stream_without_audio_tracks_needs_nothing() {
        assert_eq!(preferred_audio_choice(&[], &ENGLISH), AudioChoice::Keep);
    }

    #[test]
    fn subtitle_tracks_are_never_mistaken_for_audio() {
        let mut subtitle = audio(1, "eng", "", true);
        subtitle.kind = TrackKind::Subtitle;
        assert_eq!(
            preferred_audio_choice(&[subtitle], &ENGLISH),
            AudioChoice::Keep
        );
    }

    #[test]
    fn the_preference_starts_as_english_and_can_be_changed_or_cleared() {
        let preference = AudioPreference::default();
        assert_eq!(preference.get(), Some(ENGLISH));
        preference.set(Some(hindi()));
        assert_eq!(preference.get().map(|l| l.name), Some("Hindi"));
        preference.set(None);
        assert_eq!(preference.get(), None);
    }

    #[test]
    fn mpv_gets_the_codes_of_the_language_or_nothing() {
        assert_eq!(alang_value(Some(&ENGLISH)), "eng,en");
        assert_eq!(alang_value(None), "");
    }
}
