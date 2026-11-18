//! Tauri commands for the video cache: the setting (Settings > General > "Video cache") and
//! leaving the player, which keeps the video waiting with a smaller cache (`video_away`).

use app_services::{
    PlaybackItem, PlaybackService, SettingsService, VideoCache, VIDEO_CACHE_AHEAD_PERCENTS,
    VIDEO_CACHE_SIZES_MB,
};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::source_manager::SourceManager;
use crate::video_away::holds_a_stream;

/// The video cache setting and its choices.
#[derive(Debug, Clone, Serialize)]
pub struct VideoCacheDto {
    /// The whole size, in MB.
    pub size_mb: u32,
    /// The share read ahead of the playing position, in percent.
    pub ahead_percent: u8,
    /// The sizes that can be chosen, in MB.
    pub sizes_mb: Vec<u32>,
    /// The ahead shares that can be chosen, in percent.
    pub ahead_percents: Vec<u8>,
}

fn describe(cache: VideoCache) -> VideoCacheDto {
    VideoCacheDto {
        size_mb: cache.size_mb,
        ahead_percent: cache.ahead_percent,
        sizes_mb: VIDEO_CACHE_SIZES_MB.to_vec(),
        ahead_percents: VIDEO_CACHE_AHEAD_PERCENTS.to_vec(),
    }
}

/// The viewer's video cache, or the default when the setting cannot be read.
pub(crate) fn video_cache(settings: &SettingsService) -> VideoCache {
    settings.video_cache().unwrap_or_else(|error| {
        tracing::warn!(%error, "cannot read the video cache setting; using the default");
        VideoCache::default()
    })
}

/// The video cache setting.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_video_cache(state: State<'_, AppState>) -> Result<VideoCacheDto, String> {
    Ok(describe(video_cache(&state.read().settings)))
}

/// Saves the video cache setting and applies it at once: the full size while a video plays, half
/// while it waits.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(size_mb, ahead_percent), err)]
pub async fn set_video_cache(
    size_mb: u32,
    ahead_percent: u8,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<VideoCacheDto, String> {
    let cache = VideoCache {
        size_mb,
        ahead_percent,
    };
    let services = state.read();
    services
        .settings
        .set_video_cache(cache)
        .map_err(|error| error.to_string())?;
    if let Some(playback) = &services.playback {
        let applied = sources.with_away(|away| {
            let limits = if away {
                cache.while_away()
            } else {
                cache.while_watching()
            };
            playback.set_cache_limits(limits)
        });
        if let Err(error) = applied {
            tracing::warn!(%error, "the new video cache size could not be given to the player");
        }
    }
    Ok(describe(cache))
}

/// The viewer left the player. A video that holds a stream waits: paused, loaded, its cache
/// halved, its progress saved. Anything else (still opening, ended, failed) is stopped as before.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn leave_playback(
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let playback = services
        .playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_owned())?;
    let keep_ready = services.settings.keep_video_ready().unwrap_or(true);
    leave_player(
        sources.inner(),
        playback,
        video_cache(&services.settings),
        keep_ready,
    )
}

/// What [`leave_playback`] does, without the window. When `keep_ready` is false the stream is
/// stopped instead of waiting paused (frees RAM on low-end PCs).
pub(crate) fn leave_player(
    sources: &SourceManager,
    playback: &PlaybackService,
    cache: VideoCache,
    keep_ready: bool,
) -> Result<(), String> {
    if keep_ready {
        let away = cache.while_away();
        let waits = holds_a_stream(&playback.tick().state)
            && sources.leave_with(|| playback.leave(away).map_err(|error| error.to_string()))?;
        if waits {
            return Ok(());
        }
    }
    sources.close();
    playback.stop().map_err(|error| error.to_string())?;
    Ok(())
}

/// Plays on the video that waits when the viewer opens it again, with the cache back at full size.
/// Returns false when `item` is not the waiting video (or its stream is gone), so it is opened.
pub(crate) fn come_back_to(
    sources: &SourceManager,
    playback: &PlaybackService,
    item: &PlaybackItem,
    cache: VideoCache,
) -> Result<bool, String> {
    if !holds_a_stream(&playback.tick().state) {
        return Ok(false);
    }
    sources.come_back_with(item, || {
        playback
            .come_back(cache.while_watching())
            .map_err(|error| error.to_string())
    })
}

#[cfg(test)]
#[path = "video_cache_flow_tests.rs"]
mod flow_tests;
