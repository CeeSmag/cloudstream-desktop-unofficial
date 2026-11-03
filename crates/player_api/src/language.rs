//! Languages the app knows by name, with the ISO 639 codes that stand for them.

/// A language and its two- and three-letter ISO 639 codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// Readable name, for example `English`.
    pub name: &'static str,
    /// Codes found in media files; the first is the three-letter one.
    pub codes: &'static [&'static str],
}

/// English, the language played when nothing else is chosen.
pub const ENGLISH: Language = Language {
    name: "English",
    codes: &["eng", "en"],
};

const LANGUAGES: &[Language] = &[
    ENGLISH,
    Language {
        name: "Spanish",
        codes: &["spa", "es"],
    },
    Language {
        name: "French",
        codes: &["fre", "fra", "fr"],
    },
    Language {
        name: "German",
        codes: &["ger", "deu", "de"],
    },
    Language {
        name: "Italian",
        codes: &["ita", "it"],
    },
    Language {
        name: "Portuguese",
        codes: &["por", "pt"],
    },
    Language {
        name: "Russian",
        codes: &["rus", "ru"],
    },
    Language {
        name: "Japanese",
        codes: &["jpn", "ja"],
    },
    Language {
        name: "Korean",
        codes: &["kor", "ko"],
    },
    Language {
        name: "Chinese",
        codes: &["chi", "zho", "zh"],
    },
    Language {
        name: "Arabic",
        codes: &["ara", "ar"],
    },
    Language {
        name: "Turkish",
        codes: &["tur", "tr"],
    },
    Language {
        name: "Hindi",
        codes: &["hin", "hi"],
    },
    Language {
        name: "Tamil",
        codes: &["tam", "ta"],
    },
    Language {
        name: "Telugu",
        codes: &["tel", "te"],
    },
    Language {
        name: "Malayalam",
        codes: &["mal", "ml"],
    },
    Language {
        name: "Kannada",
        codes: &["kan", "kn"],
    },
    Language {
        name: "Bengali",
        codes: &["ben", "bn"],
    },
    Language {
        name: "Marathi",
        codes: &["mar", "mr"],
    },
    Language {
        name: "Punjabi",
        codes: &["pan", "pa"],
    },
    Language {
        name: "Urdu",
        codes: &["urd", "ur"],
    },
    Language {
        name: "Thai",
        codes: &["tha", "th"],
    },
    Language {
        name: "Vietnamese",
        codes: &["vie", "vi"],
    },
    Language {
        name: "Indonesian",
        codes: &["ind", "id"],
    },
    Language {
        name: "Malay",
        codes: &["may", "msa", "ms"],
    },
    Language {
        name: "Filipino",
        codes: &["fil", "tl"],
    },
    Language {
        name: "Polish",
        codes: &["pol", "pl"],
    },
    Language {
        name: "Dutch",
        codes: &["dut", "nld", "nl"],
    },
    Language {
        name: "Swedish",
        codes: &["swe", "sv"],
    },
    Language {
        name: "Danish",
        codes: &["dan", "da"],
    },
    Language {
        name: "Norwegian",
        codes: &["nor", "no"],
    },
    Language {
        name: "Finnish",
        codes: &["fin", "fi"],
    },
    Language {
        name: "Ukrainian",
        codes: &["ukr", "uk"],
    },
    Language {
        name: "Hebrew",
        codes: &["heb", "he"],
    },
    Language {
        name: "Greek",
        codes: &["gre", "ell", "el"],
    },
    Language {
        name: "Czech",
        codes: &["cze", "ces", "cs"],
    },
    Language {
        name: "Hungarian",
        codes: &["hun", "hu"],
    },
    Language {
        name: "Romanian",
        codes: &["rum", "ron", "ro"],
    },
    Language {
        name: "Bulgarian",
        codes: &["bul", "bg"],
    },
    Language {
        name: "Croatian",
        codes: &["hrv", "hr"],
    },
    Language {
        name: "Serbian",
        codes: &["srp", "sr"],
    },
    Language {
        name: "Persian",
        codes: &["per", "fas", "fa"],
    },
];

impl Language {
    /// The code to store and to hand to a player, for example `eng`.
    pub fn code(&self) -> &'static str {
        self.codes.first().copied().unwrap_or_default()
    }

    /// Whether `code` (any case, with or without a region such as `en-US`) is this language.
    pub fn has_code(&self, code: &str) -> bool {
        let lowered = code.trim().to_ascii_lowercase();
        let primary = lowered.split(['-', '_']).next().unwrap_or_default();
        self.codes.contains(&primary)
    }
}

/// Every language the app knows, English first.
pub fn all_languages() -> &'static [Language] {
    LANGUAGES
}

/// Finds a language by any of its codes or by its name.
pub fn find_language(code_or_name: &str) -> Option<Language> {
    let wanted = code_or_name.trim();
    LANGUAGES
        .iter()
        .find(|language| language.has_code(wanted) || language.name.eq_ignore_ascii_case(wanted))
        .copied()
}

/// Expands common ISO 639 codes; unknown codes are shown as given.
pub(crate) fn language_name(code: &str) -> String {
    LANGUAGES
        .iter()
        .find(|language| language.has_code(code))
        .map_or_else(|| code.to_owned(), |language| language.name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_in_any_case_and_with_a_region() {
        assert!(ENGLISH.has_code("eng"));
        assert!(ENGLISH.has_code("EN"));
        assert!(ENGLISH.has_code("en-US"));
        assert!(!ENGLISH.has_code("enx"));
        assert!(!ENGLISH.has_code(""));
    }

    #[test]
    fn languages_are_found_by_code_or_name() {
        assert_eq!(find_language("hin").map(|l| l.name), Some("Hindi"));
        assert_eq!(find_language("hindi").map(|l| l.name), Some("Hindi"));
        assert_eq!(find_language("xx"), None);
    }

    #[test]
    fn english_comes_first_and_every_language_has_a_code() {
        assert_eq!(all_languages().first(), Some(&ENGLISH));
        assert!(all_languages()
            .iter()
            .all(|language| !language.code().is_empty()));
        assert_eq!(ENGLISH.code(), "eng");
    }

    #[test]
    fn unknown_codes_are_shown_as_given() {
        assert_eq!(language_name("jpn"), "Japanese");
        assert_eq!(language_name("zzz"), "zzz");
    }
}
