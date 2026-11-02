//! Tauri commands for browsing repositories and installing, trusting and removing extensions.

use app_services::ExtensionService;
use core_model::{InstalledExtension, PluginListing};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::app_state::AppState;
use crate::repository_discovery;
use crate::PROVIDERS_CHANGED_EVENT;

/// An extension that is installed on this computer.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledExtensionDto {
    /// Name used to install and remove it.
    pub internal_name: String,
    /// Version that is installed.
    pub version: i64,
    /// Repository it came from, when known.
    pub repository_url: Option<String>,
    /// Whether it may use files and the system (changes apply on the next start).
    pub trusted: bool,
    /// Providers it registered in this session.
    pub providers: Vec<String>,
}

/// A plugin a repository offers.
#[derive(Debug, Clone, Serialize)]
pub struct PluginDto {
    /// Name used to install it.
    pub internal_name: String,
    /// Display name.
    pub name: String,
    /// Latest version offered.
    pub version: i64,
    /// What the plugin does.
    pub description: Option<String>,
    /// Who wrote it.
    pub authors: Vec<String>,
    /// Language code, when the plugin has one.
    pub language: Option<String>,
    /// Icon URL.
    pub icon_url: Option<String>,
    /// Download size in bytes.
    pub file_size: Option<u64>,
    /// Version installed here, or `None` when it is not installed.
    pub installed_version: Option<i64>,
}

/// Lists the installed extensions.
#[tauri::command]
pub async fn get_installed_extensions(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledExtensionDto>, String> {
    let services = state.read();
    let installed = services
        .extensions
        .installed()
        .map_err(|error| error.to_string())?;
    Ok(installed
        .into_iter()
        .map(|extension| describe_installed(&services.extensions, extension))
        .collect())
}

/// Lists what a repository offers, marking what is already installed.
#[tauri::command]
pub async fn get_repository_plugins(
    url: String,
    state: State<'_, AppState>,
) -> Result<Vec<PluginDto>, String> {
    let services = state.read();
    let offered = services
        .repositories
        .plugins(&url)
        .map_err(|error| error.to_string())?;
    let installed = services
        .extensions
        .installed()
        .map_err(|error| error.to_string())?;
    Ok(offered
        .into_iter()
        .map(|plugin| describe_plugin(plugin, &installed))
        .collect())
}

/// Downloads a plugin from a repository, loads it, and starts collecting any repositories it adds.
#[tauri::command]
pub async fn install_plugin(
    repository_url: String,
    internal_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let services = state.read();
    let offered = services
        .repositories
        .plugins(&repository_url)
        .map_err(|error| error.to_string())?;
    let plugin = offered
        .into_iter()
        .find(|plugin| plugin.internal_name == internal_name)
        .ok_or_else(|| format!("{internal_name} is not offered by that repository any more."))?;
    tracing::info!(plugin = %internal_name, repository = %repository_url, "installing plugin");
    let providers = services
        .extensions
        .install(&plugin, &repository_url)
        .map_err(|error| error.to_string())?;
    notify_providers_changed(&app);
    repository_discovery::watch(
        app,
        services.extensions.clone(),
        services.repositories.clone(),
    );
    Ok(providers
        .into_iter()
        .map(|provider| provider.name)
        .collect())
}

/// Removes an installed extension and the providers it offered.
#[tauri::command]
pub async fn uninstall_extension(
    internal_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let services = state.read();
    let extension = find_installed(&services.extensions, &internal_name)?;
    services
        .extensions
        .uninstall(&extension)
        .map_err(|error| error.to_string())?;
    tracing::info!(extension = %internal_name, "extension uninstalled");
    notify_providers_changed(&app);
    Ok(())
}

/// Changes whether an extension is trusted; it applies the next time the app starts.
#[tauri::command]
pub async fn set_extension_trusted(
    internal_name: String,
    trusted: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let services = state.read();
    let extension = find_installed(&services.extensions, &internal_name)?;
    services
        .extensions
        .set_trusted(&extension, trusted)
        .map_err(|error| error.to_string())
}

fn find_installed(
    extensions: &ExtensionService,
    internal_name: &str,
) -> Result<InstalledExtension, String> {
    extensions
        .installed()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|extension| extension.internal_name == internal_name)
        .ok_or_else(|| format!("{internal_name} is not installed."))
}

fn notify_providers_changed(app: &AppHandle) {
    if let Err(error) = app.emit(PROVIDERS_CHANGED_EVENT, ()) {
        tracing::warn!(%error, "could not tell the UI that providers changed");
    }
}

fn describe_installed(
    extensions: &ExtensionService,
    extension: InstalledExtension,
) -> InstalledExtensionDto {
    InstalledExtensionDto {
        providers: extensions.providers_of(&extension.file_name),
        internal_name: extension.internal_name,
        version: extension.version,
        repository_url: extension.repository_url,
        trusted: extension.trusted,
    }
}

fn describe_plugin(plugin: PluginListing, installed: &[InstalledExtension]) -> PluginDto {
    let installed_version = installed
        .iter()
        .find(|extension| extension.internal_name == plugin.internal_name)
        .map(|extension| extension.version);
    PluginDto {
        name: if plugin.name.is_empty() {
            plugin.internal_name.clone()
        } else {
            plugin.name
        },
        internal_name: plugin.internal_name,
        version: plugin.version,
        description: plugin.description,
        authors: plugin.authors,
        language: plugin.language,
        icon_url: plugin.icon_url,
        file_size: plugin.file_size,
        installed_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(name: &str, version: i64) -> PluginListing {
        PluginListing {
            internal_name: name.into(),
            version,
            ..PluginListing::default()
        }
    }

    #[test]
    fn plugin_shows_the_installed_version_and_falls_back_to_the_internal_name() {
        let installed = vec![InstalledExtension {
            internal_name: "Mega".into(),
            version: 1,
            ..InstalledExtension::default()
        }];
        let dto = describe_plugin(listing("Mega", 2), &installed);
        assert_eq!(dto.installed_version, Some(1));
        assert_eq!(dto.name, "Mega");
        assert_eq!(
            describe_plugin(listing("Other", 1), &installed).installed_version,
            None
        );
    }
}
