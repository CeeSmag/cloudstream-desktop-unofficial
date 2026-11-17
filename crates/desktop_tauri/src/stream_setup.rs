//! Wiring of the stream sources at start: the manager that finds and checks a video's streams,
//! the saved-sources cache and the subtitle loader it uses.

use app_shell::AppPaths;
use tauri::Manager;

use crate::{source_manager, sources_cleanup};

/// How long downloaded subtitle files and the per-title lists are kept.
const SUBTITLE_KEEP: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

/// Wires the stream sources manager; returns the subtitle loader and the saved-sources cache it
/// uses, which the cleanup queue also tidies.
pub(crate) fn manage_streams(
    app: &mut tauri::App,
    paths: &AppPaths,
    valid_from: u64,
) -> (
    std::sync::Arc<subtitle_store::SubtitleLoader>,
    std::sync::Arc<title_cache::TitleCache>,
) {
    let subtitles = subtitle_loader(paths);
    let sources = sources_cleanup::open_title_cache(paths.data_dir(), valid_from);
    app.manage(source_manager::SourceManager::new(
        app_services::StreamChecker::new(std::sync::Arc::new(extension_repo::UreqProbe::default())),
        std::sync::Arc::clone(&subtitles),
        std::sync::Arc::clone(&sources),
    ));
    (subtitles, sources)
}

/// The loader that downloads, cleans and keeps subtitle files in the data folder; old ones are
/// removed by the cleanup queue.
fn subtitle_loader(paths: &AppPaths) -> std::sync::Arc<subtitle_store::SubtitleLoader> {
    let cache =
        subtitle_store::SubtitleCache::new(paths.data_dir().join("subtitles"), SUBTITLE_KEEP);
    std::sync::Arc::new(subtitle_store::SubtitleLoader::new(
        cache,
        std::sync::Arc::new(subtitle_store::UreqSubtitleFetcher::default()),
        subtitle_store::LoadOptions::default(),
    ))
}
