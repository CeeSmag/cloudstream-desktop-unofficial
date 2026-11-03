//! Turns on the subtitle in the viewer's preferred language once the stream has one, one time per
//! stream, and never against the viewer's own choice.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use player_api::{Language, MediaTrack, TrackKind};

/// What to do about the subtitles at this moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubtitleChoice {
    /// The subtitle in the preferred language already shows.
    Keep,
    /// Show the subtitle track with this id.
    Switch { id: i64 },
    /// No track in a preferred language is there (yet); more may still arrive.
    NotYet,
}

/// Decides from the track list: the first language of the list that has a subtitle track wins.
pub(crate) fn preferred_subtitle_choice(
    tracks: &[MediaTrack],
    languages: &[Language],
) -> SubtitleChoice {
    let subtitles: Vec<&MediaTrack> = tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Subtitle)
        .collect();
    languages
        .iter()
        .find_map(|language| subtitles.iter().find(|track| track.is_language(language)))
        .map_or(SubtitleChoice::NotYet, |track| {
            if track.selected {
                SubtitleChoice::Keep
            } else {
                SubtitleChoice::Switch { id: track.id }
            }
        })
}

/// The viewer's preferred subtitle languages, and what has already been decided for the stream
/// that plays. Shared between the player and its event thread.
#[derive(Default)]
pub(crate) struct SubtitlePreference {
    languages: Mutex<Vec<Language>>,
    /// One more than the generation of the stream the choice was made for (0: none yet).
    chosen_for: AtomicU64,
    /// One more than the generation of the stream whose subtitles the viewer changed by hand.
    changed_by_viewer_for: AtomicU64,
}

impl SubtitlePreference {
    pub(crate) fn languages(&self) -> Vec<Language> {
        self.languages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Sets the languages in order of preference; an empty list turns the automatic choice off.
    pub(crate) fn set_languages(&self, languages: Vec<Language>) {
        *self
            .languages
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = languages;
    }

    /// Whether the subtitle of stream `generation` may still be chosen automatically.
    pub(crate) fn may_choose(&self, generation: u64) -> bool {
        let this_stream = generation + 1;
        !self.languages().is_empty()
            && self.chosen_for.load(Ordering::SeqCst) != this_stream
            && self.changed_by_viewer_for.load(Ordering::SeqCst) != this_stream
    }

    /// The automatic choice is done for stream `generation`.
    pub(crate) fn mark_chosen(&self, generation: u64) {
        self.chosen_for.store(generation + 1, Ordering::SeqCst);
    }

    /// The viewer picked (or turned off) a subtitle of stream `generation` themselves.
    pub(crate) fn mark_changed_by_viewer(&self, generation: u64) {
        self.changed_by_viewer_for
            .store(generation + 1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use player_api::{find_language, ENGLISH};

    use super::*;

    fn subtitle(id: i64, language: &str, selected: bool) -> MediaTrack {
        MediaTrack {
            id,
            kind: TrackKind::Subtitle,
            language: language.to_owned(),
            title: String::new(),
            selected,
            external: true,
        }
    }

    fn arabic() -> Language {
        find_language("ara").expect("Arabic is known")
    }

    #[test]
    fn the_first_preferred_language_that_has_a_track_is_shown() {
        let tracks = [subtitle(1, "eng", false), subtitle(2, "ara", false)];
        let choice = preferred_subtitle_choice(&tracks, &[arabic(), ENGLISH]);
        assert_eq!(choice, SubtitleChoice::Switch { id: 2 });
        let choice = preferred_subtitle_choice(&tracks, &[ENGLISH, arabic()]);
        assert_eq!(choice, SubtitleChoice::Switch { id: 1 });
    }

    #[test]
    fn a_subtitle_that_already_shows_is_left_alone() {
        let tracks = [subtitle(1, "eng", true), subtitle(2, "ara", false)];
        assert_eq!(
            preferred_subtitle_choice(&tracks, &[ENGLISH]),
            SubtitleChoice::Keep
        );
    }

    #[test]
    fn nothing_is_chosen_while_no_track_in_a_preferred_language_is_there() {
        let tracks = [subtitle(1, "fre", false)];
        assert_eq!(
            preferred_subtitle_choice(&tracks, &[arabic()]),
            SubtitleChoice::NotYet
        );
        assert_eq!(
            preferred_subtitle_choice(&[], &[arabic()]),
            SubtitleChoice::NotYet
        );
        assert_eq!(
            preferred_subtitle_choice(&tracks, &[]),
            SubtitleChoice::NotYet
        );
    }

    #[test]
    fn audio_tracks_are_not_subtitles() {
        let mut audio = subtitle(1, "ara", false);
        audio.kind = TrackKind::Audio;
        assert_eq!(
            preferred_subtitle_choice(&[audio], &[arabic()]),
            SubtitleChoice::NotYet
        );
    }

    #[test]
    fn the_choice_is_made_once_per_stream_and_again_for_the_next_stream() {
        let preference = SubtitlePreference::default();
        assert!(!preference.may_choose(0), "no languages, nothing to choose");
        preference.set_languages(vec![ENGLISH]);
        assert!(preference.may_choose(0));
        preference.mark_chosen(0);
        assert!(!preference.may_choose(0));
        assert!(preference.may_choose(1), "a new stream gets its own choice");
    }

    #[test]
    fn a_subtitle_the_viewer_changed_by_hand_is_never_chosen_for_again() {
        let preference = SubtitlePreference::default();
        preference.set_languages(vec![ENGLISH]);
        preference.mark_changed_by_viewer(4);
        assert!(!preference.may_choose(4));
        assert!(preference.may_choose(5));
    }

    #[test]
    fn an_empty_list_turns_the_automatic_choice_off() {
        let preference = SubtitlePreference::default();
        preference.set_languages(vec![ENGLISH]);
        preference.set_languages(Vec::new());
        assert!(!preference.may_choose(0));
    }
}
