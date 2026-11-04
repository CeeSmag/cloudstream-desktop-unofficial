//! Tauri commands for controlling video playback and player status.

use std::sync::Arc;

use crate::app_state::AppState;
use crate::dto::{ChapterDto, MediaTrackDto, PlayRequestDto, PlaybackStatusDto};
use crate::source_fallback::FallbackOutcome;
use crate::source_manager::{emit, CheckUpdate, SourceManager, Tell};
use crate::video_opening::{item_from, open_video, services_for_playing};
use player_api::{PlaybackState, PlaybackStatus, TrackKind};
use tauri::{AppHandle, Manager, State};

/// Opens a video: the provider's streams are checked as they are found and the best working one
/// plays when the search and all the checks are done, or when the viewer skips. A provider that
/// gives nothing in time, or never finishes, ends in an error. After a skip the search and the
/// checks go on in the background until they are done or the video closes.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn play_media(
    request: PlayRequestDto,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let item = item_from(request);
    let services = services_for_playing(&state)?;
    let events = app.clone();
    let tell: Tell = Arc::new(move |update| emit(&events, update));
    // The wait blocks, so it runs off the async worker threads.
    let (outcome, _background) = tauri::async_runtime::spawn_blocking(move || {
        let sources = app.state::<SourceManager>();
        open_video(&sources, &services, item, tell)
    })
    .await
    .map_err(|error| error.to_string())?;
    outcome
}

/// Lets the viewer start now: the best stream known to work plays and the search, the checks and
/// the subtitle downloads carry on in the background.
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
    app: AppHandle,
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
    let outcome = sources.fallback().on_status(&sources, playback, &status);
    if matches!(outcome, FallbackOutcome::Switched(_)) {
        emit(&app, CheckUpdate::SourcesChanged);
    }
    let streams = StreamParts {
        tracks: playback.tracks(),
        chapters: playback.chapters(),
    };
    Ok(apply_fallback(
        map_playback_status(status, streams),
        outcome,
    ))
}

/// A failure the fallback is dealing with is shown as loading; one it gave up on carries its
/// message, which names every source that failed.
fn apply_fallback(mut dto: PlaybackStatusDto, outcome: FallbackOutcome) -> PlaybackStatusDto {
    match outcome {
        FallbackOutcome::Switched(_) | FallbackOutcome::Switching => {
            dto.state = "Loading".to_owned();
            dto.error = None;
        }
        FallbackOutcome::GaveUp(message) => dto.error = Some(message),
        FallbackOutcome::Fine => {}
    }
    dto
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

/// What the player knows about the loaded stream besides its state.
#[derive(Default)]
struct StreamParts {
    tracks: Vec<player_api::MediaTrack>,
    chapters: Vec<player_api::Chapter>,
}

fn state_name(state: &PlaybackState) -> &'static str {
    match state {
        PlaybackState::Idle => "Idle",
        PlaybackState::Loading => "Loading",
        PlaybackState::Playing => "Playing",
        PlaybackState::Paused => "Paused",
        PlaybackState::Ended => "Ended",
        PlaybackState::Failed(_) => "Failed",
    }
}

fn track_dto(track: player_api::MediaTrack) -> MediaTrackDto {
    let title = track.label();
    MediaTrackDto {
        id: track.id,
        kind: match track.kind {
            TrackKind::Audio => "audio".to_owned(),
            TrackKind::Subtitle => "subtitle".to_owned(),
        },
        title,
        language: Some(track.language),
        selected: track.selected,
    }
}

fn map_playback_status(status: PlaybackStatus, streams: StreamParts) -> PlaybackStatusDto {
    let error = match &status.state {
        PlaybackState::Failed(reason) => Some(failure_message(reason)),
        _ => None,
    };
    PlaybackStatusDto {
        state: state_name(&status.state).to_owned(),
        position_ms: status.position_ms,
        duration_ms: status.duration_ms,
        has_video: status.has_video,
        volume: 100.0,
        tracks: streams.tracks.into_iter().map(track_dto).collect(),
        chapters: streams
            .chapters
            .into_iter()
            .map(|chapter| ChapterDto {
                title: chapter.title,
                time_ms: chapter.time_ms,
            })
            .collect(),
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
        let dto = map_playback_status(status, StreamParts::default());
        assert_eq!(dto.state, "Failed");
        assert!(dto.error.is_some_and(|text| text.contains("mpv error -13")));
    }

    #[test]
    fn healthy_states_have_no_error() {
        let status = PlaybackStatus {
            state: PlaybackState::Playing,
            ..PlaybackStatus::default()
        };
        assert!(map_playback_status(status, StreamParts::default())
            .error
            .is_none());
    }

    #[test]
    fn chapters_and_tracks_reach_the_page_in_order() {
        let streams = StreamParts {
            tracks: Vec::new(),
            chapters: vec![
                player_api::Chapter {
                    title: "Opening".to_owned(),
                    time_ms: 0,
                },
                player_api::Chapter {
                    title: "Part A".to_owned(),
                    time_ms: 93_000,
                },
            ],
        };
        let dto = map_playback_status(PlaybackStatus::default(), streams);
        let chapters: Vec<_> = dto
            .chapters
            .iter()
            .map(|c| (c.title.as_str(), c.time_ms))
            .collect();
        assert_eq!(chapters, [("Opening", 0), ("Part A", 93_000)]);
    }
}
