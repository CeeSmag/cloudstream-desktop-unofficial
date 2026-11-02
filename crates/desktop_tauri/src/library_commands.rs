//! Tauri commands for managing library bookmarks and watch progress.

use crate::app_state::AppState;
use crate::dto::{BookmarkDto, WatchProgressDto};
use core_model::{Bookmark, WatchProgress};
use tauri::State;

/// Lists all bookmarks stored in the user library.
#[tauri::command]
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
            ..WatchProgress::default()
        };
        let dto = map_progress(progress);
        assert_eq!(dto.provider, "Archive");
        assert_eq!(dto.detail_url, "https://site.test/show");
        assert_eq!(dto.media_id, "ep-1");
        assert!((dto.percentage - 0.5).abs() < f32::EPSILON);
    }
}
