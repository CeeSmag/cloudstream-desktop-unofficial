//! Tauri commands for managing application settings and extension repositories.

use crate::app_state::AppState;
use core_model::RepositoryEntry;
use tauri::State;

/// Lists all registered extension repositories.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_repositories(state: State<'_, AppState>) -> Result<Vec<RepositoryEntry>, String> {
    let services = state.read();
    services
        .repositories
        .list()
        .map_err(|error| error.to_string())
}

/// Adds an extension repository from a remote URL.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
pub async fn add_repository(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    services
        .repositories
        .add(&url)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Forgets an extension repository.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(url = ?url), err)]
pub async fn remove_repository(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let services = state.read();
    services
        .repositories
        .remove(&url)
        .map_err(|error| error.to_string())
}

/// Lists disabled provider IDs.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_disabled_providers(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let services = state.read();
    services
        .settings
        .disabled_providers()
        .map_err(|error| error.to_string())
}

/// Enables or disables a provider.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, enabled = ?enabled), err)]
pub async fn set_provider_enabled(
    provider: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let services = state.read();
    services
        .settings
        .set_provider_enabled(&provider, enabled)
        .map_err(|error| error.to_string())
}
