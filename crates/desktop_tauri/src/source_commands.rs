//! Tauri commands for listing and switching the streams of the item that is playing.

use app_services::{full_label, stream_host, stream_kind, StreamSelection};
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
    /// Host the stream is served from.
    pub host: String,
    /// The stream address, shown in full.
    pub url: String,
    /// File size in bytes, when the check learned it.
    pub size_bytes: Option<u64>,
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
    let active = sources
        .session()
        .snapshot()
        .ok_or_else(|| "Nothing is playing.".to_owned())?;
    let link = active
        .links
        .get(index)
        .ok_or_else(|| format!("There is no source number {}.", index + 1))?;
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_owned())?;
    let live_position_ms = playback.tick().position_ms;
    let stream = StreamSelection {
        link,
        subtitles: &active.subtitles,
        start_position_ms: if live_position_ms > 0 {
            live_position_ms
        } else {
            active.position_ms
        },
    };
    tracing::info!(index, url = %link.url, "switching source");
    playback
        .start_at(active.item.clone(), &stream)
        .map_err(|error| format!("Cannot switch source: {error}"))?;
    sources.session().mark_selected(index);
    Ok(())
}

/// The streams to show: not known to be broken, plus the one that is playing.
fn describe_sources(active: &ActiveSources) -> Vec<SourceDto> {
    active
        .links
        .iter()
        .zip(&active.health)
        .enumerate()
        .filter(|(index, (_, health))| {
            **health != SourceHealth::Broken || *index == active.selected
        })
        .map(|(index, (link, health))| SourceDto {
            index,
            label: full_label(link),
            kind: stream_kind(link),
            host: stream_host(link),
            url: link.url.clone(),
            size_bytes: active.sizes.get(index).copied().flatten(),
            selected: index == active.selected,
            checking: *health == SourceHealth::Checking,
        })
        .collect()
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
    fn the_stream_that_is_playing_stays_listed_even_if_its_check_failed() {
        use SourceHealth::{Broken, Working};
        let list = describe_sources(&active(&[Working, Broken], 1));
        assert_eq!(list.len(), 2);
    }
}
