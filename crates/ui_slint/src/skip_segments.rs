//! The parts of a video worth skipping (intro, recap, credits, preview) and the button to offer.
//!
//! Ported from the old app (`chapterSkips.ts`, `skipPrompt.ts`). The parts come from the file's
//! own chapters ("Opening", "Ending", "OP", "Previously"): no network and no ids needed. Inside a
//! part the player offers "Skip Intro" (and so on); inside credits that run to the end it offers
//! "Next episode"; without any part it offers "Next episode" from 90 % on.

use player_api::Chapter;

/// A chapter shorter than this is a label, not a part worth a Skip button.
pub(crate) const MIN_SKIPPABLE_MS: i64 = 5_000;
/// Credits that end this close to the end of the video mean "the episode is over".
pub(crate) const NEXT_EPISODE_WINDOW_MS: i64 = 20_000;
/// Without any part, the Next episode button shows from this share of the video on.
pub(crate) const NEXT_BUTTON_FRACTION: f64 = 0.9;

/// What a part of the video is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipKind {
    Intro,
    Recap,
    Credits,
    Preview,
}

impl SkipKind {
    /// Its name on the seek bar.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Intro => "Intro",
            Self::Recap => "Recap",
            Self::Credits => "Credits",
            Self::Preview => "Preview",
        }
    }

    /// Its colour slot on the seek bar (0 intro, 1 recap, 2 credits, 3 preview).
    pub(crate) fn slot(self) -> i32 {
        match self {
            Self::Intro => 0,
            Self::Recap => 1,
            Self::Credits => 2,
            Self::Preview => 3,
        }
    }
}

/// A part of the video, start included, end not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) kind: SkipKind,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
}

/// What the skip button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptAction {
    /// Jump to this place.
    Seek(i64),
    NextEpisode,
}

/// The button over the video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkipPrompt {
    pub(crate) label: &'static str,
    pub(crate) action: PromptAction,
}

/// What a chapter title says the part is; None for an ordinary chapter ("Part A", "Chapter 3").
pub(crate) fn kind_of_title(title: &str) -> Option<SkipKind> {
    let text: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = text.split_whitespace().collect();
    if let Some(kind) = whole_title(&words) {
        return Some(kind);
    }
    // The first naming word decides: "Opening Credits" is an intro.
    words.iter().enumerate().find_map(|(at, word)| {
        let next = words.get(at + 1).copied().unwrap_or_default();
        naming_word(word, next)
    })
}

/// Short forms such as "OP" or "ED 2" count only as the whole title.
fn whole_title(words: &[&str]) -> Option<SkipKind> {
    let (first, rest) = words.split_first()?;
    let numbered = |stem: &str| {
        let tail = first.strip_prefix(stem);
        match (tail, rest) {
            (Some(tail), []) => tail.chars().all(|c| c.is_ascii_digit()),
            (Some(""), [number]) => number.chars().all(|c| c.is_ascii_digit()),
            _ => false,
        }
    };
    if numbered("op") {
        Some(SkipKind::Intro)
    } else if numbered("ed") {
        Some(SkipKind::Credits)
    } else {
        None
    }
}

fn naming_word(word: &str, next: &str) -> Option<SkipKind> {
    let stem_with_number = |stems: &[&str]| {
        stems.iter().any(|stem| {
            word.strip_prefix(stem)
                .is_some_and(|tail| tail.chars().all(|c| c.is_ascii_digit()))
        })
    };
    if stem_with_number(&["opening", "intro"]) {
        Some(SkipKind::Intro)
    } else if stem_with_number(&["ending", "outro", "credits"]) {
        Some(SkipKind::Credits)
    } else if matches!(word, "recap" | "previously") {
        Some(SkipKind::Recap)
    } else if word == "preview" || (word == "next" && next == "time") {
        Some(SkipKind::Preview)
    } else {
        None
    }
}

/// The parts the chapters name: a chapter runs until the next starts (the last until the end,
/// once the length is known); touching parts of one kind become one.
pub(crate) fn segments_from_chapters(chapters: &[Chapter], duration_ms: i64) -> Vec<Segment> {
    let mut ordered: Vec<&Chapter> = chapters.iter().collect();
    ordered.sort_by_key(|chapter| chapter.time_ms);
    let mut found: Vec<Segment> = Vec::new();
    for (at, chapter) in ordered.iter().enumerate() {
        let Some(kind) = kind_of_title(&chapter.title) else {
            continue;
        };
        let next = ordered.get(at + 1).map(|next| next.time_ms);
        let Some(end) = next.or((duration_ms > 0).then_some(duration_ms)) else {
            continue;
        };
        let end = if duration_ms > 0 {
            end.min(duration_ms)
        } else {
            end
        };
        if end - chapter.time_ms < MIN_SKIPPABLE_MS {
            continue;
        }
        match found.last_mut() {
            Some(last) if last.kind == kind && last.end_ms == chapter.time_ms => last.end_ms = end,
            _ => found.push(Segment {
                kind,
                start_ms: chapter.time_ms,
                end_ms: end,
            }),
        }
    }
    found
}

/// The part playing at `position_ms`.
pub(crate) fn segment_at(segments: &[Segment], position_ms: i64) -> Option<&Segment> {
    segments
        .iter()
        .find(|segment| position_ms >= segment.start_ms && position_ms < segment.end_ms)
}

/// What to offer at `position_ms` (see the module text).
pub(crate) fn skip_prompt(
    segments: &[Segment],
    position_ms: i64,
    duration_ms: i64,
    has_next: bool,
) -> Option<SkipPrompt> {
    const NEXT: SkipPrompt = SkipPrompt {
        label: "Next episode",
        action: PromptAction::NextEpisode,
    };
    if let Some(active) = segment_at(segments, position_ms) {
        let ends_episode = matches!(active.kind, SkipKind::Credits | SkipKind::Preview)
            && duration_ms > 0
            && duration_ms - active.end_ms < NEXT_EPISODE_WINDOW_MS;
        if has_next && ends_episode {
            return Some(NEXT);
        }
        let label = match active.kind {
            SkipKind::Intro => "Skip Intro",
            SkipKind::Recap => "Skip Recap",
            SkipKind::Credits => "Skip Credits",
            SkipKind::Preview => "Skip Preview",
        };
        return Some(SkipPrompt {
            label,
            action: PromptAction::Seek(active.end_ms),
        });
    }
    let late = duration_ms > 0 && position_ms as f64 >= duration_ms as f64 * NEXT_BUTTON_FRACTION;
    (has_next && late).then_some(NEXT)
}

/// The parts an online database knows, in this module's terms.
pub(crate) fn from_online(found: &[skip_times::SkipSegment]) -> Vec<Segment> {
    found
        .iter()
        .map(|segment| Segment {
            kind: match segment.kind {
                skip_times::SkipKind::Intro => SkipKind::Intro,
                skip_times::SkipKind::Recap => SkipKind::Recap,
                skip_times::SkipKind::Credits => SkipKind::Credits,
                skip_times::SkipKind::Preview => SkipKind::Preview,
            },
            start_ms: i64::try_from(segment.start_ms).unwrap_or(i64::MAX),
            end_ms: i64::try_from(segment.end_ms).unwrap_or(i64::MAX),
        })
        .filter(|segment| segment.end_ms > segment.start_ms)
        .collect()
}

/// The parts to offer: the chapters decide for every kind they name (they describe this very
/// file), the online times fill in the kinds the chapters do not cover; in time order.
pub(crate) fn merge_segments(from_chapters: &[Segment], online: &[Segment]) -> Vec<Segment> {
    let mut merged: Vec<Segment> = from_chapters.to_vec();
    merged.extend(
        online
            .iter()
            .filter(|segment| !from_chapters.iter().any(|c| c.kind == segment.kind)),
    );
    merged.sort_by_key(|segment| segment.start_ms);
    merged
}

/// Which databases fit the title: films, anime (by the provider's kind) or other series.
pub(crate) fn title_kind(media_type: &str, is_movie: bool) -> skip_times::TitleKind {
    let kind = media_type.to_ascii_lowercase();
    if is_movie {
        skip_times::TitleKind::Movie
    } else if kind.contains("anime") || kind.contains("ova") {
        skip_times::TitleKind::Anime
    } else {
        skip_times::TitleKind::Series
    }
}

#[cfg(test)]
#[path = "skip_segments_tests.rs"]
mod tests;
