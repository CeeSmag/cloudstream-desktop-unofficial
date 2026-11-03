//! The small decisions of the fallback: which source to try next, how to start it, what to tell
//! the viewer.

use app_services::{link_rank, PlaybackService, StreamSelection};

use crate::source_fallback::{FallbackOutcome, Services};
use crate::source_session::{ActiveSources, SourceHealth};

pub(crate) fn sources_word(count: usize) -> String {
    if count == 1 {
        "1 source".to_owned()
    } else {
        format!("{count} sources")
    }
}

/// The message when every source was tried or known broken.
pub(crate) fn none_left_message(reasons: &[String]) -> String {
    format!(
        "{} failed ({}) and no other source is left to try.",
        sources_word(reasons.len()),
        reasons.join("; ")
    )
}

/// The best source to try after `failed`: working ones first, then ones still being checked,
/// each by quality; never one known to be broken.
pub(crate) fn next_candidate(active: &ActiveSources, failed: usize) -> Option<usize> {
    active
        .links
        .iter()
        .zip(&active.health)
        .enumerate()
        .filter(|(index, (_, health))| *index != failed && **health != SourceHealth::Broken)
        .min_by_key(|(index, (link, health))| {
            let checking = **health == SourceHealth::Checking;
            (checking, std::cmp::Reverse(link_rank(link)), *index)
        })
        .map(|(index, _)| index)
}

/// Starts the source at `index` where the video stopped.
pub(crate) fn start_source(
    playback: &PlaybackService,
    active: &ActiveSources,
    index: usize,
) -> Result<(), String> {
    let link = active
        .links
        .get(index)
        .ok_or_else(|| format!("there is no source number {}", index + 1))?;
    let stream = StreamSelection {
        link,
        subtitles: &active.subtitles,
        start_position_ms: active.position_ms,
    };
    playback
        .start_at(active.item.clone(), &stream)
        .map_err(|error| error.to_string())
}

/// The source at `index` plays now: records it and hands over subtitles found meanwhile.
pub(crate) fn switched(
    services: Services<'_>,
    started: (&ActiveSources, usize),
) -> FallbackOutcome {
    let (active, index) = started;
    tracing::info!(
        index,
        position_ms = active.position_ms,
        "switched to the next source"
    );
    services.sources.session().mark_selected(index);
    let late = services
        .sources
        .session()
        .unsent_subtitles(active.generation);
    if !late.is_empty() {
        services.playback.add_subtitles(&late);
    }
    FallbackOutcome::Switched(index)
}
