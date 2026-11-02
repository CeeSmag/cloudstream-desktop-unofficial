//! Tauri commands for controlling video playback and player status.

use crate::app_state::AppState;
use crate::dto::{MediaTrackDto, PlayRequestDto, PlaybackStatusDto};
use app_services::{playable_links_best_first, PlaybackItem};
use player_api::{PlaybackState, PlaybackStatus, TrackKind};
use tauri::State;

/// Starts playing the requested media stream.
#[tauri::command]
pub async fn play_media(request: PlayRequestDto, state: State<'_, AppState>) -> Result<(), String> {
    let item = PlaybackItem {
        provider: request.provider,
        media_key: request.media_key,
        title: request.title,
        detail_url: request.detail_url.unwrap_or_default(),
        poster_url: request.poster_url,
    };
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    let resolved = services
        .streams
        .resolve(&item.provider, &item.media_key)
        .map_err(|error| error.to_string())?;
    let playable = playable_links_best_first(resolved.links);
    let best_link = playable
        .first()
        .ok_or_else(|| "No playable stream links found".to_string())?;
    playback
        .start(item, best_link, &resolved.subtitles)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Pauses current playback.
#[tauri::command]
pub async fn pause_playback(state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback.set_paused(true).map_err(|error| error.to_string())
}

/// Resumes current playback.
#[tauri::command]
pub async fn resume_playback(state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback
        .set_paused(false)
        .map_err(|error| error.to_string())
}

/// Seeks to a specific millisecond position.
#[tauri::command]
pub async fn seek_to(position_ms: i64, state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback
        .seek_to(position_ms)
        .map_err(|error| error.to_string())
}

/// Stops playback and frees video resources.
#[tauri::command]
pub async fn stop_playback(state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback.stop().map_err(|error| error.to_string())
}

/// Retrieves the current snapshot of player state, time, buffer, and tracks.
#[tauri::command]
pub async fn get_playback_status(state: State<'_, AppState>) -> Result<PlaybackStatusDto, String> {
    let services = state.read();
    let playback = match &services.playback {
        Some(playback) => playback,
        None => return Ok(PlaybackStatusDto::default()),
    };
    let status = playback.tick();
    let tracks = playback.tracks();
    Ok(map_playback_status(status, tracks))
}

/// Selects or disables an audio or subtitle track.
#[tauri::command]
pub async fn select_media_track(
    kind: String,
    id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let track_kind = match kind.as_str() {
        "audio" => TrackKind::Audio,
        "subtitle" => TrackKind::Subtitle,
        _ => return Err("Invalid track kind; expected 'audio' or 'subtitle'".to_string()),
    };
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback
        .select_track(track_kind, id)
        .map_err(|error| error.to_string())
}

fn map_playback_status(
    status: PlaybackStatus,
    tracks: Vec<player_api::MediaTrack>,
) -> PlaybackStatusDto {
    let error = match &status.state {
        PlaybackState::Failed(reason) => Some(failure_message(reason)),
        _ => None,
    };
    let state_str = match status.state {
        PlaybackState::Idle => "Idle",
        PlaybackState::Loading => "Loading",
        PlaybackState::Playing => "Playing",
        PlaybackState::Paused => "Paused",
        PlaybackState::Ended => "Ended",
        PlaybackState::Failed(_) => "Failed",
    };

    let track_dtos = tracks
        .into_iter()
        .map(|track| MediaTrackDto {
            id: track.id,
            kind: match track.kind {
                TrackKind::Audio => "audio".to_owned(),
                TrackKind::Subtitle => "subtitle".to_owned(),
            },
            title: track.title,
            language: Some(track.language),
            selected: track.selected,
        })
        .collect();

    PlaybackStatusDto {
        state: state_str.to_owned(),
        position_ms: status.position_ms,
        duration_ms: status.duration_ms,
        has_video: status.has_video,
        volume: 100.0,
        tracks: track_dtos,
        error,
    }
}

/// Turns the player's terse failure reason into something a person can act on.
fn failure_message(reason: &str) -> String {
    format!("This stream could not be played ({reason}). The host may have refused the request or the file may be restricted; try another source.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_state_carries_a_readable_error() {
        let status = PlaybackStatus {
            state: PlaybackState::Failed("mpv error -13".into()),
            ..PlaybackStatus::default()
        };
        let dto = map_playback_status(status, Vec::new());
        assert_eq!(dto.state, "Failed");
        assert!(dto.error.is_some_and(|text| text.contains("mpv error -13")));
    }

    #[test]
    fn healthy_states_have_no_error() {
        let status = PlaybackStatus {
            state: PlaybackState::Playing,
            ..PlaybackStatus::default()
        };
        assert!(map_playback_status(status, Vec::new()).error.is_none());
    }
}
