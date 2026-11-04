//! What a lookup asks for and what it finds.

use serde::Serialize;

use crate::external_ids::ExternalIds;

/// What a stretch of the video is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipKind {
    /// The opening or intro.
    Intro,
    /// The "previously on" recap.
    Recap,
    /// The ending or the credits.
    Credits,
    /// The preview of the next episode.
    Preview,
}

/// A stretch of the video the viewer may want to jump over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SkipSegment {
    /// What it is.
    pub kind: SkipKind,
    /// Where it starts, in milliseconds.
    pub start_ms: u64,
    /// Where it ends, in milliseconds; skipping goes here.
    pub end_ms: u64,
}

/// What sort of title is asked about; it decides which databases are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TitleKind {
    /// A film.
    Movie,
    /// A series.
    Series,
    /// An anime series or OVA.
    Anime,
}

/// One title, episode and video length to find skip times for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipQuery {
    /// The ids of the title in the databases.
    pub ids: ExternalIds,
    /// Film, series or anime.
    pub kind: TitleKind,
    /// Season number; none for a film.
    pub season: Option<u32>,
    /// Episode number within the season; none for a film.
    pub episode: Option<u32>,
    /// Length of the video in milliseconds; 0 when not known yet.
    pub duration_ms: u64,
}
