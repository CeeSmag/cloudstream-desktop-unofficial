//! Settings > General > "Plugin processes (low-end)".

use app_services::PLUGIN_WORKER_LIMIT_CHOICES;
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

/// The resident plugin-process cap and its choices (`0` = unlimited).
#[derive(Debug, Clone, Serialize)]
pub struct PluginWorkerLimitDto {
    /// Current cap; `0` means unlimited.
    pub limit: u32,
    /// Values the viewer can pick (`0` = unlimited).
    pub choices: Vec<u32>,
}

fn describe(limit: Option<usize>) -> PluginWorkerLimitDto {
    PluginWorkerLimitDto {
        limit: limit.map_or(0, |n| n as u32),
        choices: PLUGIN_WORKER_LIMIT_CHOICES
            .iter()
            .map(|choice| choice.map_or(0, |n| n as u32))
            .collect(),
    }
}

/// Applies the stored (or migrated) resident-worker limit before extensions load.
pub(crate) fn apply_saved(services: &ui_app::UiServices) {
    let had_prior = services
        .extensions
        .installed()
        .map(|list| !list.is_empty())
        .unwrap_or(false)
        || services
            .repositories
            .list()
            .map(|list| !list.is_empty())
            .unwrap_or(false);
    match services.settings.plugin_worker_limit_migrating(had_prior) {
        Ok(limit) => {
            services.extensions.set_resident_worker_limit(limit);
            tracing::info!(?limit, "plugin resident worker limit applied");
        }
        Err(error) => {
            tracing::warn!(%error, "cannot read plugin worker limit; keeping the runtime default");
        }
    }
}

/// The resident plugin-process setting.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_plugin_worker_limit(
    state: State<'_, AppState>,
) -> Result<PluginWorkerLimitDto, String> {
    let services = state.read();
    let limit = services
        .settings
        .plugin_worker_limit()
        .map_err(|error| error.to_string())?;
    Ok(describe(limit))
}

/// Saves the cap and parks excess workers at once (`0` = unlimited).
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(limit), err)]
pub async fn set_plugin_worker_limit(
    limit: u32,
    state: State<'_, AppState>,
) -> Result<PluginWorkerLimitDto, String> {
    if limit > 8 {
        return Err("plugin worker limit must be 0 (unlimited) or 1–8".to_owned());
    }
    let chosen = if limit == 0 {
        None
    } else {
        Some(limit as usize)
    };
    let services = state.read();
    services
        .settings
        .set_plugin_worker_limit(chosen)
        .map_err(|error| error.to_string())?;
    services.extensions.set_resident_worker_limit(chosen);
    Ok(describe(chosen))
}
