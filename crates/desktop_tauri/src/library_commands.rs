//! Tauri commands for managing library bookmarks and watch progress.

use crate::app_state::AppState;
use crate::dto::{BookmarkDto, ResumePointerDto, WatchProgressDto};
use crate::source_manager::SourceManager;
use crate::video_opening::title_key_of;
use core_model::{Bookmark, WatchProgress};
use tauri::State;

/// Lists all bookmarks stored in the user library.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_bookmarks(state: State<'_, AppState>) -> Result<Vec<BookmarkDto>, String> {
    let services = state.read();
    let bookmarks = services
        .library
        .bookmarks()
        .map_err(|error| error.to_string())?;
    Ok(bookmarks.into_iter().map(map_bookmark).collect())
}

/// Adds or updates a title in the user library.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn add_bookmark(payload: BookmarkDto, state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    let now_ms = services.clock.now_ms();
    let bookmark = Bookmark {
        provider: payload.provider,
        url: payload.url,
        name: payload.title,
        poster_url: payload.poster_url,
        media_type: Some(payload.status),
        added_at_ms: now_ms,
    };
    services
        .library
        .add_bookmark(&bookmark)
        .map_err(|error| error.to_string())
}

/// Removes a bookmark from the library.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, url = ?url), err)]
pub async fn remove_bookmark(
    provider: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let services = state.read();
    services
        .library
        .remove_bookmark(&provider, &url)
        .map_err(|error| error.to_string())
}

/// Checks whether a media item is currently bookmarked.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, url = ?url), err)]
pub async fn is_bookmarked(
    provider: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let services = state.read();
    services
        .library
        .is_bookmarked(&provider, &url)
        .map_err(|error| error.to_string())
}

/// Lists recent titles to offer in Continue Watching.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_continue_watching(
    state: State<'_, AppState>,
) -> Result<Vec<WatchProgressDto>, String> {
    let services = state.read();
    let list = services
        .library
        .continue_watching()
        .map_err(|error| error.to_string())?;
    Ok(list.into_iter().map(map_progress).collect())
}

/// Gets the resume pointer for a given show or movie.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
pub async fn get_resume_pointer(
    provider: String,
    detail_url: String,
    state: State<'_, AppState>,
) -> Result<Option<ResumePointerDto>, String> {
    let services = state.read();
    let pointer = services
        .library
        .resume_pointer(&provider, &detail_url)
        .map_err(|error| error.to_string())?;
    Ok(pointer.map(|p| ResumePointerDto {
        provider: p.provider,
        detail_url: p.detail_url,
        media_key: p.media_key,
        title: p.title,
        poster_url: p.poster_url,
        season: p.season,
        episode: p.episode,
        position_ms: p.position_ms,
        duration_ms: p.duration_ms,
    }))
}

/// The saved position of every episode of a show, so its title page can mark them.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
pub async fn get_show_progress(
    provider: String,
    detail_url: String,
    state: State<'_, AppState>,
) -> Result<Vec<WatchProgressDto>, String> {
    let services = state.read();
    let list = services
        .library
        .show_progress(&provider, &detail_url)
        .map_err(|error| error.to_string())?;
    Ok(list.into_iter().map(map_progress).collect())
}

/// Takes one title out of Continue Watching and forgets all its saved sources and subtitle
/// files on disk.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, detail_url = ?detail_url), err)]
pub async fn remove_continue_watching(
    provider: String,
    detail_url: String,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let removed_keys = services
        .library
        .remove_continue_watching(&provider, &detail_url)
        .map_err(|error| error.to_string())?;
    for key in &removed_keys {
        sources.forget_title(&title_key_of(&provider, key));
    }
    Ok(())
}

/// Empties Continue Watching and forgets what was kept on disk for each of its titles.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn clear_continue_watching(
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<(), String> {
    let services = state.read();
    let removed = services
        .library
        .clear_continue_watching()
        .map_err(|error| error.to_string())?;
    for (provider, media_key) in &removed {
        sources.forget_title(&title_key_of(provider, media_key));
    }
    tracing::info!(count = removed.len(), "continue watching cleared");
    Ok(())
}

fn map_bookmark(bookmark: Bookmark) -> BookmarkDto {
    BookmarkDto {
        id: bookmark.url.clone(),
        title: bookmark.name,
        poster_url: bookmark.poster_url,
        status: bookmark.media_type.unwrap_or_else(|| "Watching".to_owned()),
        provider: bookmark.provider,
        url: bookmark.url,
    }
}

fn map_progress(progress: WatchProgress) -> WatchProgressDto {
    let percentage = progress.fraction_watched();
    WatchProgressDto {
        media_id: progress.media_key,
        provider: progress.provider,
        detail_url: progress.detail_url,
        poster_url: progress.poster_url,
        title: progress.title,
        position_ms: progress.position_ms,
        duration_ms: progress.duration_ms,
        percentage,
        updated_at: progress.updated_at_ms / 1000,
        season: progress.season,
        episode: progress.episode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_keeps_provider_and_title_page() {
        let progress = WatchProgress {
            provider: "Archive".into(),
            media_key: "ep-1".into(),
            detail_url: "https://site.test/show".into(),
            position_ms: 30,
            duration_ms: 60,
            season: Some(1),
            episode: Some(2),
            ..WatchProgress::default()
        };
        let dto = map_progress(progress);
        assert_eq!(dto.provider, "Archive");
        assert_eq!(dto.detail_url, "https://site.test/show");
        assert_eq!(dto.media_id, "ep-1");
        assert_eq!(dto.season, Some(1));
        assert_eq!(dto.episode, Some(2));
        assert!((dto.percentage - 0.5).abs() < f32::EPSILON);
    }
}
