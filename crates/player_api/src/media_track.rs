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

/// Language names with the ISO 639 codes (two- and three-letter) that stand for them.
const LANGUAGES: &[(&str, &[&str])] = &[
    ("English", &["eng", "en"]),
    ("Spanish", &["spa", "es"]),
    ("French", &["fre", "fra", "fr"]),
    ("German", &["ger", "deu", "de"]),
    ("Italian", &["ita", "it"]),
    ("Portuguese", &["por", "pt"]),
    ("Russian", &["rus", "ru"]),
    ("Japanese", &["jpn", "ja"]),
    ("Korean", &["kor", "ko"]),
    ("Chinese", &["chi", "zho", "zh"]),
    ("Arabic", &["ara", "ar"]),
    ("Turkish", &["tur", "tr"]),
    ("Hindi", &["hin", "hi"]),
    ("Tamil", &["tam", "ta"]),
    ("Telugu", &["tel", "te"]),
    ("Malayalam", &["mal", "ml"]),
    ("Kannada", &["kan", "kn"]),
    ("Bengali", &["ben", "bn"]),
    ("Marathi", &["mar", "mr"]),
    ("Punjabi", &["pan", "pa"]),
    ("Urdu", &["urd", "ur"]),
    ("Thai", &["tha", "th"]),
    ("Vietnamese", &["vie", "vi"]),
    ("Indonesian", &["ind", "id"]),
    ("Malay", &["may", "msa", "ms"]),
    ("Filipino", &["fil", "tl"]),
    ("Polish", &["pol", "pl"]),
    ("Dutch", &["dut", "nld", "nl"]),
    ("Swedish", &["swe", "sv"]),
    ("Danish", &["dan", "da"]),
    ("Norwegian", &["nor", "no"]),
    ("Finnish", &["fin", "fi"]),
    ("Ukrainian", &["ukr", "uk"]),
    ("Hebrew", &["heb", "he"]),
    ("Greek", &["gre", "ell", "el"]),
    ("Czech", &["cze", "ces", "cs"]),
    ("Hungarian", &["hun", "hu"]),
    ("Romanian", &["rum", "ron", "ro"]),
    ("Bulgarian", &["bul", "bg"]),
    ("Croatian", &["hrv", "hr"]),
    ("Serbian", &["srp", "sr"]),
    ("Persian", &["per", "fas", "fa"]),
];

/// Expands common ISO 639 codes; unknown codes are shown as given.
fn language_name(code: &str) -> String {
    let lowered = code.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(_, codes)| codes.contains(&lowered.as_str()))
        .map_or_else(|| code.to_owned(), |(name, _)| (*name).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

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
