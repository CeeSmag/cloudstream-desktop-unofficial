//! Tauri commands for listing and switching the streams of the item that is playing.

use app_services::{
    full_label, link_rank, stream_host, stream_kind, PlaybackService, StreamSelection,
};
use core_model::PlayableLink;
use player_api::PlaybackState;
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::source_manager::SourceManager;
use crate::source_session::{ActiveSources, SourceHealth};

/// One stream the player can switch to.
#[derive(Debug, Clone, Serialize)]
pub struct SourceDto {
    /// Position in the list; pass it to `switch_source`.
    pub index: usize,
    /// The complete description: quality and the full name of the stream.
    pub label: String,
    /// `HLS`, `DASH`, a file type such as `MKV`, or `Video`.
    pub kind: String,
    /// Vertical resolution the provider reports (for example 1080 or 2160), or 0 when unknown.
    pub quality: i32,
    /// Host the stream is served from.
    pub host: String,
    /// The stream address, shown in full.
    pub url: String,
    /// File size in bytes, when the check learned it.
    pub size_bytes: Option<u64>,
    /// Languages of the subtitles this stream offers: the ones inside the stream and the
    /// separate files the provider found for the title, as named (`eng`, `Arabic`), no repeats.
    pub subtitle_languages: Vec<String>,
    /// True for the stream that is playing now.
    pub selected: bool,
    /// True while the check of this stream has not finished.
    pub checking: bool,
}

/// Lists the streams worth offering for the item that is playing: the ones that answered the
/// check and the ones still being checked (empty when nothing is playing).
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_playback_sources(
    sources: State<'_, SourceManager>,
) -> Result<Vec<SourceDto>, String> {
    Ok(sources
        .session()
        .snapshot()
        .map(|active| describe_sources(&active))
        .unwrap_or_default())
}

/// Switches to another stream of the same item, continuing from the current position.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(index = ?index), err)]
pub async fn switch_source(
    index: usize,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_owned())?;
    switch_to(&sources, playback, index)
}

/// Which of the open video's streams to start, and from where.
struct SourceStart {
    index: usize,
    position_ms: i64,
}

/// Starts the stream at `index` where the video stopped. The viewer chose it, so the count of
/// automatic fallbacks starts over.
pub(crate) fn switch_to(
    sources: &SourceManager,
    playback: &PlaybackService,
    index: usize,
) -> Result<(), String> {
    let active = sources
        .session()
        .start_snapshot()
        .ok_or_else(|| "Nothing is playing.".to_owned())?;
    let live_position_ms = playback.tick().position_ms;
    let position_ms = if live_position_ms > 0 {
        live_position_ms
    } else {
        active.position_ms
    };
    start_source(
        sources,
        playback,
        &active,
        SourceStart { index, position_ms },
    )
}

/// A video that has played to its end is gone from mpv, so Play or a seek would do nothing. When
/// the viewer presses Play or seeks after the end, the stream that was playing starts again at
/// `position_ms`. Returns whether it did; false and nothing done when the video has not ended or
/// is no longer open.
pub(crate) fn restart_if_ended(
    sources: &SourceManager,
    playback: &PlaybackService,
    position_ms: i64,
) -> Result<bool, String> {
    if playback.tick().state != PlaybackState::Ended {
        return Ok(false);
    }
    let Some(active) = sources.session().start_snapshot() else {
        return Ok(false);
    };
    tracing::info!(position_ms, "the video had ended; playing it again");
    let index = active.selected;
    start_source(
        sources,
        playback,
        &active,
        SourceStart { index, position_ms },
    )?;
    Ok(true)
}

fn start_source(
    sources: &SourceManager,
    playback: &PlaybackService,
    active: &ActiveSources,
    start: SourceStart,
) -> Result<(), String> {
    let link = active
        .links
        .get(start.index)
        .ok_or_else(|| format!("There is no source number {}.", start.index + 1))?;
    let stream = StreamSelection {
        link,
        subtitles: &active.subtitles,
        start_position_ms: start.position_ms,
    };
    tracing::info!(index = start.index, stream = %title_cache::redact_url(&link.url), "starting a source");
    sources.fallback().reset();
    playback
        .start_at(active.item.clone(), &stream)
        .map_err(|error| format!("Cannot switch source: {error}"))?;
    sources.session().mark_selected(start.index);
    let late = sources.session().unsent_subtitles(active.generation);
    if !late.is_empty() {
        playback.add_subtitles(&late);
    }
    Ok(())
}

/// The streams to show: not known to be broken, plus the one that is playing.
fn describe_sources(active: &ActiveSources) -> Vec<SourceDto> {
    let mut shown: Vec<(usize, (&PlayableLink, &SourceHealth))> = active
        .links
        .iter()
        .zip(&active.health)
        .enumerate()
        .filter(|(index, (_, health))| {
            **health != SourceHealth::Broken || *index == active.selected
        })
        .collect();
    // Streams arrive in the order the provider finds them; the picker lists the best first.
    shown.sort_by_key(|(index, (link, _))| (std::cmp::Reverse(link_rank(link)), *index));
    shown
        .into_iter()
        .map(|(index, (link, health))| SourceDto {
            index,
            label: full_label(link),
            kind: stream_kind(link),
            quality: link.quality.max(0),
            host: stream_host(link),
            url: link.url.clone(),
            size_bytes: active.sizes.get(index).copied().flatten(),
            subtitle_languages: subtitle_languages_of(active, index),
            selected: index == active.selected,
            checking: *health == SourceHealth::Checking,
        })
        .collect()
}

/// The subtitle languages stream `index` offers: its own, then those of the title's subtitle files.
fn subtitle_languages_of(active: &ActiveSources, index: usize) -> Vec<String> {
    let own = active.stream_subtitles.get(index).into_iter().flatten();
    let shared = active.subtitles.iter().map(|track| &track.lang);
    let mut languages: Vec<String> = Vec::new();
    for language in own.chain(shared) {
        if !language.is_empty()
            && !languages
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(language))
        {
            languages.push(language.clone());
        }
    }
    languages
}

#[cfg(test)]
mod tests {
    use core_model::PlayableLink;

    use super::*;

    fn active(health: &[SourceHealth], selected: usize) -> ActiveSources {
        let links = health
            .iter()
            .enumerate()
            .map(|(index, _)| PlayableLink {
                name: format!("S{index}"),
                quality: 720,
                ..PlayableLink::default()
            })
            .collect();
        let mut sources = ActiveSources::new(Default::default(), links, Vec::new());
        sources.health = health.to_vec();
        sources.selected = selected;
        sources
    }

    #[test]
    fn broken_streams_are_hidden_and_checking_ones_are_marked() {
        use SourceHealth::{Broken, Checking, Working};
        let list = describe_sources(&active(&[Working, Broken, Checking, Working], 0));
        let indexes: Vec<usize> = list.iter().map(|source| source.index).collect();
        assert_eq!(indexes, [0, 2, 3], "index 1 is broken and hidden");
        assert!(list[0].selected && !list[1].selected);
        assert!(list[1].checking && !list[2].checking);
        assert_eq!(list[0].label, "720p - S0");
    }

    #[test]
    fn a_stream_offers_its_own_subtitle_languages_and_those_of_the_title_without_repeats() {
        use core_model::SubtitleTrack;
        let mut sources = active(&[SourceHealth::Working, SourceHealth::Working], 0);
        sources.stream_subtitles = vec![vec!["ara".to_owned(), "eng".to_owned()], Vec::new()];
        sources.subtitles = vec![
            SubtitleTrack {
                lang: "English".to_owned(),
                ..SubtitleTrack::default()
            },
            SubtitleTrack {
                lang: "ENG".to_owned(),
                ..SubtitleTrack::default()
            },
        ];
        let list = describe_sources(&sources);
        assert_eq!(list[0].subtitle_languages, ["ara", "eng", "English"]);
        assert_eq!(list[1].subtitle_languages, ["English", "ENG"]);
    }

    #[test]
    fn the_stream_that_is_playing_stays_listed_even_if_its_check_failed() {
        use SourceHealth::{Broken, Working};
        let list = describe_sources(&active(&[Working, Broken], 1));
        assert_eq!(list.len(), 2);
    }
}
