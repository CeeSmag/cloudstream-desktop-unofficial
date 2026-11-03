//! Tauri commands for controlling video playback and player status.

use std::sync::Arc;

use crate::app_state::AppState;
use crate::dto::{MediaTrackDto, PlayRequestDto, PlaybackStatusDto};
use crate::source_manager::{SearchRequest, SourceManager};
use crate::video_opening::{
    item_from, services_for_playing, start_search, start_stream, wait_for_start, StreamStart,
};
use player_api::{PlaybackState, PlaybackStatus, TrackKind};
use tauri::{AppHandle, State};

/// Opens a video: the provider's streams are checked as they are found and the best working one
/// plays a moment after the first is known, but never later than five seconds after the viewer
/// opened it. The search and the checks go on in the background until they are done or the video
/// closes.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn play_media(
    request: PlayRequestDto,
    app: AppHandle,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let item = item_from(request);
    let gate = sources.begin();
    let opened_at = std::time::Instant::now();
    let (streams, playback) = services_for_playing(&state)?;
    let (generation, search) = start_search(&sources, &gate, &streams, &item)?;
    let late_subtitles = Arc::clone(&playback);
    sources.discover(
        app,
        SearchRequest {
            gate: Arc::clone(&gate),
            generation,
            search,
            give_subtitles: Arc::new(move |subtitles| late_subtitles.add_subtitles(subtitles)),
        },
    );
    let start = wait_for_start(&gate).await?;
    let elapsed_ms = opened_at.elapsed().as_millis();
    tracing::info!(start, elapsed_ms, "starting playback");
    start_stream(&sources, &playback, StreamStart { item, start, gate })
}

/// Lets the viewer start now: the best stream known to work plays and the rest of the checks
/// carry on in the background.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn skip_source_check(
    app: AppHandle,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    sources.skip(&app);
    Ok(())
}

/// Pauses current playback.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
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
#[tracing::instrument(level = "debug", skip_all, fields(position_ms = ?position_ms), err)]
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
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn stop_playback(
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    sources.close();
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    playback.stop().map_err(|error| error.to_string())
}

/// Sets the player volume, 0 to 100 percent (130 amplifies).
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(percent = ?percent), err)]
pub async fn set_volume(percent: u8, state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_string())?;
    tracing::debug!(percent, "setting volume");
    playback
        .set_volume(percent)
        .map_err(|error| error.to_string())
}

/// Retrieves the current snapshot of player state, time, buffer, and tracks.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_playback_status(
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<PlaybackStatusDto, String> {
    let services = state.read();
    let playback = match &services.playback {
        Some(playback) => playback,
        None => return Ok(PlaybackStatusDto::default()),
    };
    let status = playback.tick();
    if matches!(status.state, PlaybackState::Playing | PlaybackState::Paused) {
        sources.session().remember_position(status.position_ms);
    }
    let tracks = playback.tracks();
    Ok(map_playback_status(status, tracks))
}

/// Selects or disables an audio or subtitle track.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(kind = ?kind, id = ?id), err)]
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
        .map(|track| {
            let label = track.label();
            MediaTrackDto {
                id: track.id,
                kind: match track.kind {
                    TrackKind::Audio => "audio".to_owned(),
                    TrackKind::Subtitle => "subtitle".to_owned(),
                },
                title: label,
                language: Some(track.language),
                selected: track.selected,
            }
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
