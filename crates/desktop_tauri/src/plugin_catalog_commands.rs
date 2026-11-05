//! Tauri commands for the list of every extension of every repository.

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::extension_commands::{describe_plugin, PluginDto};

/// An extension of one repository.
#[derive(Debug, Clone, Serialize)]
pub struct PluginHitDto {
    /// Address of the repository that offers it.
    pub repository_url: String,
    /// Display name of that repository.
    pub repository_name: String,
    /// Icon of that repository, when it has one.
    pub repository_icon_url: Option<String>,
    /// The extension, marked with what is installed here.
    pub plugin: PluginDto,
}

/// Downloads the extensions of every repository so they can be listed; returns how many there
/// are. The first call takes a while, later ones return at once.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn prepare_plugin_catalog(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .run_blocking(|services| {
            services
                .repositories
                .prepare_plugin_catalog()
                .map_err(|error| error.to_string())
        })
        .await
}

/// Every extension of every repository, each marked with what is installed here and the
/// repository that offers it.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn list_all_plugins(state: State<'_, AppState>) -> Result<Vec<PluginHitDto>, String> {
    state
        .run_blocking(|services| {
            let hits = services
                .repositories
                .all_plugins()
                .map_err(|error| error.to_string())?;
            let installed = services
                .extensions
                .installed()
                .map_err(|error| error.to_string())?;
            Ok(hits
                .into_iter()
                .map(|hit| PluginHitDto {
                    repository_url: hit.repository_url,
                    repository_name: hit.repository_name,
                    repository_icon_url: hit.repository_icon_url,
                    plugin: describe_plugin(hit.plugin, &installed),
                })
                .collect())
        })
        .await
}
