use crate::language::{language_name, Language};

/// Which kind of stream inside a media file a track is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    /// A sound track (language, commentary, ...).
    Audio,
    /// Subtitles, embedded or loaded from a separate file.
    Subtitle,
}

/// One selectable audio or subtitle track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTrack {
    /// Player-assigned id, used to select the track.
    pub id: i64,
    /// Audio or subtitle.
    pub kind: TrackKind,
    /// Language code reported by the file, such as `eng`; empty when unknown.
    pub language: String,
    /// Title reported by the file or given when the subtitle was added; empty when none.
    pub title: String,
    /// Whether this track is playing/showing right now.
    pub selected: bool,
    /// Whether the track comes from a separate file (for example a subtitle URL from a provider).
    pub external: bool,
}

impl MediaTrack {
    /// Whether the track is in `language`: by its language code (`eng`, `en`, `en-US`), or, when
    /// the file gave no usable language, by the language's name in its title.
    pub fn is_language(&self, language: &Language) -> bool {
        if language.has_code(&self.language) {
            return true;
        }
        let code = self.language.trim().to_ascii_lowercase();
        let unknown = code.is_empty() || matches!(code.as_str(), "und" | "unk");
        unknown
            && self
                .title
                .to_ascii_lowercase()
                .contains(&language.name.to_ascii_lowercase())
    }

    /// A readable name for menus, for example `English`, `eng - Commentary` or `Track 2`.
    pub fn label(&self) -> String {
        match (self.language.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} - {}", language_name(&self.language), self.title),
            (false, true) => language_name(&self.language),
            (true, false) => self.title.clone(),
            (true, true) => format!("Track {}", self.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::ENGLISH;

    fn track(language: &str, title: &str) -> MediaTrack {
        MediaTrack {
            id: 2,
            kind: TrackKind::Audio,
            language: language.to_owned(),
            title: title.to_owned(),
            selected: false,
            external: false,
        }
    }

    #[test]
    fn english_is_recognised_by_language_code_in_any_spelling() {
        for code in ["eng", "en", "EN", "en-US", "en_GB", " eng "] {
            assert!(track(code, "").is_language(&ENGLISH), "{code}");
        }
        assert!(!track("hin", "").is_language(&ENGLISH));
        assert!(!track("enx", "").is_language(&ENGLISH));
    }

    #[test]
    fn english_is_recognised_by_title_only_when_the_language_is_unknown() {
        assert!(track("", "English 5.1").is_language(&ENGLISH));
        assert!(track("und", "english").is_language(&ENGLISH));
        assert!(
            !track("hin", "English").is_language(&ENGLISH),
            "the language tag wins"
        );
        assert!(!track("", "Commentary").is_language(&ENGLISH));
    }

    #[test]
    fn label_prefers_language_name_and_appends_title() {
        assert_eq!(track("eng", "").label(), "English");
        assert_eq!(track("jpn", "Commentary").label(), "Japanese - Commentary");
    }

    #[test]
    fn label_falls_back_to_title_then_track_number() {
        assert_eq!(track("", "Director").label(), "Director");
        assert_eq!(track("", "").label(), "Track 2");
    }

    #[test]
    fn label_names_languages_found_on_streaming_files() {
        assert_eq!(track("ta", "").label(), "Tamil");
        assert_eq!(track("TEL", "").label(), "Telugu");
        assert_eq!(track("hin", "HDHub4u.Ag").label(), "Hindi - HDHub4u.Ag");
    }

    #[test]
    fn label_keeps_unknown_language_codes() {
        assert_eq!(track("xyz", "").label(), "xyz");
    }
}
