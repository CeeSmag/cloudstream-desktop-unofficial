//! Tauri commands for listing and switching the streams of the item that is playing.

use app_services::{source_label, StreamSelection};
use core_model::PlayableLink;
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::source_session::SourceSession;

/// One stream the player can switch to.
#[derive(Debug, Clone, Serialize)]
pub struct SourceDto {
    /// Position in the list; pass it to `switch_source`.
    pub index: usize,
    /// Readable name such as `1080p - Server (mp4)`.
    pub label: String,
    /// True for the stream that is playing now.
    pub selected: bool,
}

/// Lists the streams found for the item that is playing (empty when nothing is playing).
#[tauri::command]
pub async fn get_playback_sources(
    sources: State<'_, SourceSession>,
) -> Result<Vec<SourceDto>, String> {
    let list = sources
        .snapshot()
        .map(|active| describe_sources(&active.links, active.selected))
        .unwrap_or_default();
    Ok(list)
}

/// Switches to another stream of the same item, continuing from the current position.
#[tauri::command]
pub async fn switch_source(
    index: usize,
    state: State<'_, AppState>,
    sources: State<'_, SourceSession>,
) -> Result<(), String> {
    let active = sources
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
    let stream = StreamSelection {
        link,
        subtitles: &active.subtitles,
        start_position_ms: playback.tick().position_ms,
    };
    tracing::info!(index, url = %link.url, "switching source");
    playback
        .start_at(active.item.clone(), &stream)
        .map_err(|error| format!("Cannot switch source: {error}"))?;
    sources.mark_selected(index);
    Ok(())
}

fn describe_sources(links: &[PlayableLink], selected: usize) -> Vec<SourceDto> {
    links
        .iter()
        .enumerate()
        .map(|(index, link)| SourceDto {
            index,
            label: source_label(link),
            selected: index == selected,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_sources_marks_only_the_playing_one() {
        let links = vec![
            PlayableLink {
                name: "A".into(),
                quality: 720,
                ..PlayableLink::default()
            },
            PlayableLink {
                name: "B".into(),
                quality: 360,
                ..PlayableLink::default()
            },
        ];
        let list = describe_sources(&links, 1);
        assert_eq!(list.len(), 2);
        assert!(!list[0].selected && list[1].selected);
        assert_eq!(list[0].label, "720p - A");
    }
}
