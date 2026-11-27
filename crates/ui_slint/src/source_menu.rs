//! The player's Sources menu: which streams it lists, in what order, and what each row says.
//!
//! Broken streams are hidden except the one playing. Shown streams are always sorted by file size
//! (largest first, unknown sizes last, video above audio-only, ties broken by quality then order).
//! The viewer can filter to a single resolution group (`1080p` or `2160p`, or none) and to streams
//! with subtitles in chosen preferred languages.

use app_services::{
    full_label, link_rank, stream_kind, LinkHealth, PreferredResolution, HD_MAX_QUALITY,
};
use core_model::{PlayableLink, SubtitleTrack};
use player_api::Language;

/// A resolution group the viewer can filter by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier {
    /// 1080p and lower.
    Hd,
    /// Above 1080p: 1440p, 2160p and higher.
    Uhd,
}

/// The group of a stream's vertical resolution as the provider reports it; None when unknown.
pub(crate) fn tier_of(quality: i32) -> Option<Tier> {
    match quality {
        q if q <= 0 => None,
        q if q <= HD_MAX_QUALITY => Some(Tier::Hd),
        _ => Some(Tier::Uhd),
    }
}

/// Dropdown labels for Settings > Preferences > "Preferred resolution".
pub(crate) fn preferred_resolution_labels() -> Vec<String> {
    PreferredResolution::ALL
        .iter()
        .map(|choice| preferred_resolution_label(*choice).to_owned())
        .collect()
}

/// Human-readable label for a [`PreferredResolution`] choice.
pub(crate) fn preferred_resolution_label(resolution: PreferredResolution) -> &'static str {
    match resolution {
        PreferredResolution::Hd => "1080p",
        PreferredResolution::Uhd => "2160p",
        PreferredResolution::Any => "Any (largest file)",
    }
}

/// Maps a Settings dropdown label back to its [`PreferredResolution`].
pub(crate) fn parse_preferred_resolution_label(label: &str) -> Option<PreferredResolution> {
    PreferredResolution::ALL
        .into_iter()
        .find(|choice| preferred_resolution_label(*choice) == label)
}

/// The filters the viewer toggled in the menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceFilter {
    /// Selected resolution group (`None` shows every resolution; at most one may be active).
    pub(crate) tier: Option<Tier>,
    /// Names of the wanted subtitle languages (`English`).
    pub(crate) subtitles: Vec<String>,
}

impl SourceFilter {
    /// Initial filter for a newly opened video: starts with the Settings preferred resolution selected.
    pub(crate) fn for_preferred(preferred: PreferredResolution) -> Self {
        let tier = match preferred {
            PreferredResolution::Hd => Some(Tier::Hd),
            PreferredResolution::Uhd => Some(Tier::Uhd),
            PreferredResolution::Any => None,
        };
        Self {
            tier,
            subtitles: Vec::new(),
        }
    }

    /// Selects `tier` exclusively, or clears the resolution filter when toggled again.
    pub(crate) fn toggle_tier(&mut self, tier: Tier) {
        self.tier = if self.tier == Some(tier) {
            None
        } else {
            Some(tier)
        };
    }

    pub(crate) fn toggle_subtitle(&mut self, name: &str) {
        toggle(&mut self.subtitles, name.to_owned());
    }
}

fn toggle<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if let Some(at) = list.iter().position(|entry| *entry == item) {
        list.remove(at);
    } else {
        list.push(item);
    }
}

/// What the menu is built from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceList<'a> {
    pub(crate) links: &'a [(PlayableLink, Option<LinkHealth>)],
    /// The title's subtitle files from the provider: every stream offers them.
    pub(crate) subtitles: &'a [SubtitleTrack],
    pub(crate) selected: usize,
    /// The viewer's preferred subtitle languages, in their order.
    pub(crate) preferred: &'a [Language],
}

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRow {
    /// Place in the open list (what switching takes).
    pub(crate) index: usize,
    pub(crate) label: String,
    /// `HLS`, `DASH`, a file type such as `MKV`, or `Video`.
    pub(crate) kind: String,
    pub(crate) checking: bool,
    pub(crate) working: bool,
    pub(crate) selected: bool,
    /// `1.3 GB`; empty when unknown.
    pub(crate) size: String,
    /// The preferred subtitle languages it offers, as two capital letters (`EN`).
    pub(crate) flags: Vec<String>,
}

/// The rows to show and the counts next to the filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceMenu {
    pub(crate) rows: Vec<SourceRow>,
    /// Streams before any filter.
    pub(crate) total: usize,
    pub(crate) hd_count: usize,
    pub(crate) uhd_count: usize,
    /// Streams the resolution filter alone keeps.
    pub(crate) resolution_shown: usize,
    /// Streams the subtitle filter alone keeps.
    pub(crate) subtitle_shown: usize,
}

/// A stream offered by the menu, before the filters.
struct Candidate<'a> {
    index: usize,
    link: &'a PlayableLink,
    health: Option<&'a LinkHealth>,
    languages: Vec<&'a str>,
}

/// Builds the menu for `list` with `filter` applied.
pub(crate) fn source_menu(list: &SourceList<'_>, filter: &SourceFilter) -> SourceMenu {
    let candidates = offered(list);
    let wanted: Vec<&Language> = list
        .preferred
        .iter()
        .filter(|language| filter.subtitles.iter().any(|name| name == language.name))
        .collect();
    let keeps_subtitles = |c: &Candidate<'_>| {
        c.index == list.selected || wanted.iter().all(|l| offers(&c.languages, l))
    };
    let keeps_tier = |c: &Candidate<'_>| {
        filter
            .tier
            .is_none_or(|wanted| tier_of(c.link.quality) == Some(wanted))
    };
    let count_tier = |tier| {
        candidates
            .iter()
            .filter(|c| tier_of(c.link.quality) == Some(tier))
            .count()
    };
    let mut kept: Vec<&Candidate<'_>> = candidates
        .iter()
        .filter(|c| keeps_subtitles(c) && keeps_tier(c))
        .collect();
    // Always sort largest file first (unknown sizes last), keeping video above audio-only;
    // stable sort preserves offered()'s best-first (link_rank, index) tie-breaker.
    kept.sort_by_key(|c| (link_rank(c.link) < 0, std::cmp::Reverse(size_of(c.health))));
    SourceMenu {
        rows: kept.iter().map(|c| row(c, list)).collect(),
        total: candidates.len(),
        hd_count: count_tier(Tier::Hd),
        uhd_count: count_tier(Tier::Uhd),
        resolution_shown: candidates.iter().filter(|c| keeps_tier(c)).count(),
        subtitle_shown: candidates.iter().filter(|c| keeps_subtitles(c)).count(),
    }
}

/// The streams worth offering (not known to be broken, plus the playing one), best first.
fn offered<'a>(list: &SourceList<'a>) -> Vec<Candidate<'a>> {
    let mut shown: Vec<Candidate<'a>> = list
        .links
        .iter()
        .enumerate()
        .filter(|(index, (_, health))| {
            *index == list.selected || health.as_ref().is_none_or(LinkHealth::is_working)
        })
        .map(|(index, (link, health))| Candidate {
            index,
            link,
            health: health.as_ref(),
            languages: languages_of(health.as_ref(), list.subtitles),
        })
        .collect();
    shown.sort_by_key(|c| (std::cmp::Reverse(link_rank(c.link)), c.index));
    shown
}

/// The subtitle languages a stream offers: its own, then the title's files.
fn languages_of<'a>(health: Option<&'a LinkHealth>, shared: &'a [SubtitleTrack]) -> Vec<&'a str> {
    let own: &[String] = match health {
        Some(LinkHealth::Working { subtitles, .. }) => subtitles,
        _ => &[],
    };
    own.iter()
        .map(String::as_str)
        .chain(shared.iter().map(|track| track.lang.as_str()))
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn size_of(health: Option<&LinkHealth>) -> Option<u64> {
    match health {
        Some(LinkHealth::Working { size_bytes, .. }) => *size_bytes,
        _ => None,
    }
}

fn row(candidate: &Candidate<'_>, list: &SourceList<'_>) -> SourceRow {
    SourceRow {
        index: candidate.index,
        label: full_label(candidate.link),
        kind: stream_kind(candidate.link),
        checking: candidate.health.is_none(),
        working: candidate.health.is_some_and(LinkHealth::is_working),
        selected: candidate.index == list.selected,
        size: size_of(candidate.health)
            .map(file_size_text)
            .unwrap_or_default(),
        flags: list
            .preferred
            .iter()
            .filter(|language| offers(&candidate.languages, language))
            .map(|language| abbreviation(language.name))
            .collect(),
    }
}

fn offers(values: &[&str], language: &Language) -> bool {
    values.iter().any(|value| language_matches(value, language))
}

/// Whether `value`, a subtitle language as a stream names it (`eng`, `en-US`, `Arabic`,
/// `English SDH`), is `language`.
pub(crate) fn language_matches(value: &str, language: &Language) -> bool {
    let named = value.trim().to_lowercase();
    let name = language.name.to_lowercase();
    language.has_code(&named) || named == name || named.starts_with(&format!("{name} "))
}

/// The first two letters of a language's name in capitals: `EN`, `AR`.
pub(crate) fn abbreviation(name: &str) -> String {
    name.chars().take(2).collect::<String>().to_uppercase()
}

/// A file size for people: B, KB, MB or GB with one decimal.
pub(crate) fn file_size_text(bytes: u64) -> String {
    const UNIT: f64 = 1024.0;
    let value = bytes as f64;
    if value < UNIT {
        format!("{bytes} B")
    } else if value < UNIT * UNIT {
        format!("{:.1} KB", value / UNIT)
    } else if value < UNIT * UNIT * UNIT {
        format!("{:.1} MB", value / (UNIT * UNIT))
    } else {
        format!("{:.1} GB", value / (UNIT * UNIT * UNIT))
    }
}

#[cfg(test)]
#[path = "source_menu_tests.rs"]
mod tests;
