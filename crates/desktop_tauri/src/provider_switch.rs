//! What happens when the viewer switches provider from the drop-down at the top right: everything
//! saved about stream links before the switch stops being used, so the next video of any title is
//! searched afresh. Watch progress and Continue Watching are not touched.

use app_services::SettingsService;
use tauri::State;
use title_cache::TitleCache;

use crate::app_state::AppState;
use crate::source_manager::SourceManager;

/// Drops the saved sources when `from` and `to` are two different providers. Returns whether it
/// did. Starting up and choosing the provider that is already chosen are not switches.
pub(crate) fn apply_switch(
    settings: &SettingsService,
    cache: &TitleCache,
    from: &str,
    to: &str,
) -> bool {
    if from.is_empty() || to.is_empty() || from == to {
        return false;
    }
    let at = cache.invalidate_now();
    if let Err(error) = settings.set_sources_valid_from(at) {
        // The switch still holds until the app closes, and the cleanup below removes the files.
        tracing::warn!(%error, "cannot remember the provider switch");
    }
    let report = cache.prune();
    tracing::info!(
        from,
        to,
        removed = report.invalidated,
        "provider switched; saved sources dropped"
    );
    true
}

/// Tells the app the viewer picked another provider, so saved sources of the old one (and of every
/// title opened before) are not used for the next video.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(from = ?from, to = ?to), err)]
pub async fn provider_switched(
    from: String,
    to: String,
    state: State<'_, AppState>,
    sources: State<'_, SourceManager>,
) -> Result<bool, String> {
    let services = state.read();
    Ok(apply_switch(
        &services.settings,
        &sources.titles,
        &from,
        &to,
    ))
}

#[cfg(test)]
#[path = "provider_switch_tests.rs"]
mod tests;
