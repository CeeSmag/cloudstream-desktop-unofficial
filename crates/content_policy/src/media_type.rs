//! CloudStream's media types (`TvType` in the plugin library) and how repository labels map to them.

/// A CloudStream `TvType`, in the library's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaType {
    /// `Movie`.
    Movie,
    /// `AnimeMovie`.
    AnimeMovie,
    /// `TvSeries`.
    TvSeries,
    /// `Cartoon`.
    Cartoon,
    /// `Anime`.
    Anime,
    /// `OVA`.
    Ova,
    /// `Torrent`.
    Torrent,
    /// `Documentary`.
    Documentary,
    /// `AsianDrama`.
    AsianDrama,
    /// `Live` (TV channels, live sports).
    Live,
    /// `NSFW`.
    Nsfw,
    /// `Others`: the catch-all, used by games and tools as well as by some video sites.
    Others,
    /// `Music`.
    Music,
    /// `AudioBook`.
    AudioBook,
    /// `CustomMedia`.
    CustomMedia,
    /// `Audio`.
    Audio,
    /// `Podcast`.
    Podcast,
    /// `Video`.
    Video,
}

/// The library's own names, compared without case.
const CANONICAL: &[(&str, MediaType)] = &[
    ("movie", MediaType::Movie),
    ("animemovie", MediaType::AnimeMovie),
    ("tvseries", MediaType::TvSeries),
    ("cartoon", MediaType::Cartoon),
    ("anime", MediaType::Anime),
    ("ova", MediaType::Ova),
    ("torrent", MediaType::Torrent),
    ("documentary", MediaType::Documentary),
    ("asiandrama", MediaType::AsianDrama),
    ("live", MediaType::Live),
    ("nsfw", MediaType::Nsfw),
    ("others", MediaType::Others),
    ("music", MediaType::Music),
    ("audiobook", MediaType::AudioBook),
    ("custommedia", MediaType::CustomMedia),
    ("audio", MediaType::Audio),
    ("podcast", MediaType::Podcast),
    ("video", MediaType::Video),
];

/// Spellings found in real repositories (census of 767 listings, Nov 2026). Only unambiguous ones:
/// a label that could mean two things is left unrecognised rather than guessed.
const ALIASES: &[(&str, MediaType)] = &[
    ("movies", MediaType::Movie),
    ("series", MediaType::TvSeries),
    ("tv series", MediaType::TvSeries),
    ("drama", MediaType::TvSeries),
    ("tvchannel", MediaType::Live),
    ("cartoons", MediaType::Cartoon),
    ("animemovies", MediaType::AnimeMovie),
    ("ona", MediaType::Ova),
    ("special", MediaType::Ova),
    ("other", MediaType::Others),
];

impl MediaType {
    /// The type a repository label names, or `None` when the label is not one we can read
    /// (`All`, the literal `tvTypes`, a typo).
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        let wanted = label.trim().to_lowercase();
        CANONICAL
            .iter()
            .chain(ALIASES)
            .find(|(name, _)| *name == wanted)
            .map(|(_, media)| *media)
    }

    /// The library's own name (`TvSeries`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Movie => "Movie",
            Self::AnimeMovie => "AnimeMovie",
            Self::TvSeries => "TvSeries",
            Self::Cartoon => "Cartoon",
            Self::Anime => "Anime",
            Self::Ova => "OVA",
            Self::Torrent => "Torrent",
            Self::Documentary => "Documentary",
            Self::AsianDrama => "AsianDrama",
            Self::Live => "Live",
            Self::Nsfw => "NSFW",
            Self::Others => "Others",
            Self::Music => "Music",
            Self::AudioBook => "AudioBook",
            Self::CustomMedia => "CustomMedia",
            Self::Audio => "Audio",
            Self::Podcast => "Podcast",
            Self::Video => "Video",
        }
    }

    /// True for the types the app supports: movies, series, anime, cartoons, documentaries, live,
    /// torrents, NSFW and plain video. Music, audio, books, podcasts and the catch-alls are not.
    #[must_use]
    pub const fn is_video(self) -> bool {
        !matches!(
            self,
            Self::Others
                | Self::Music
                | Self::AudioBook
                | Self::CustomMedia
                | Self::Audio
                | Self::Podcast
        )
    }
}

/// What a plugin's `tvTypes` say once read: the types, plus the labels that could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLabels {
    /// Distinct types, in [`MediaType`] order.
    pub types: Vec<MediaType>,
    /// Labels that named no type, trimmed, as written.
    pub unrecognised: Vec<String>,
}

/// Reads a listing's `tvTypes`. A label holding several names separated by commas
/// (`"Movie,Anime,Cartoon"`, seen in the wild) counts as each of them; empty labels are ignored.
#[must_use]
pub fn parse_labels(labels: &[String]) -> ParsedLabels {
    let mut parsed = ParsedLabels::default();
    for part in labels.iter().flat_map(|label| label.split(',')) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match MediaType::from_label(part) {
            Some(media) => parsed.types.push(media),
            None => parsed.unrecognised.push(part.to_owned()),
        }
    }
    parsed.types.sort_unstable();
    parsed.types.dedup();
    parsed
}

#[cfg(test)]
#[path = "media_type_tests.rs"]
mod tests;
