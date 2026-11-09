use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use core_model::{DiscoveredRepository, InstalledExtension, PluginListing, ProviderInfo};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher};
use storage::InstalledExtensionStore;

use crate::extension_load_report::{ExtensionLoadReport, LoadProgress};
use crate::ServiceError;

/// Installs, removes and loads extension files.
#[derive(Clone)]
pub struct ExtensionService {
    gateway: Arc<dyn ProviderGateway>,
    installer: Arc<ExtensionInstaller<Arc<dyn HttpFetcher>>>,
    installed: InstalledExtensionStore,
    providers: Arc<Mutex<Vec<ProviderInfo>>>,
    /// Provider names each extension file registered, so uninstalling can hide exactly those.
    provided_by: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl ExtensionService {
    /// Creates the service.
    pub fn new(
        gateway: Arc<dyn ProviderGateway>,
        installer: ExtensionInstaller<Arc<dyn HttpFetcher>>,
        installed: InstalledExtensionStore,
    ) -> Self {
        Self {
            gateway,
            installer: Arc::new(installer),
            installed,
            providers: Arc::default(),
            provided_by: Arc::default(),
        }
    }

    /// Providers loaded so far.
    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Extensions recorded as installed.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn installed(&self) -> Result<Vec<InstalledExtension>, ServiceError> {
        Ok(self.installed.list()?)
    }

    /// Downloads a plugin, records it and loads it into the runtime.
    #[tracing::instrument(level = "debug", skip_all, fields(repository_url = ?repository_url), err)]
    pub fn install(
        &self,
        listing: &PluginListing,
        repository_url: &str,
    ) -> Result<Vec<ProviderInfo>, ServiceError> {
        let path = self.installer.install(listing)?;
        let record = InstalledExtension {
            file_name: file_name_of(&path),
            internal_name: listing.internal_name.clone(),
            repository_url: Some(repository_url.to_owned()),
            version: listing.version,
            trusted: false,
            icon_url: listing
                .icon_url
                .clone()
                .filter(|icon| !icon.trim().is_empty()),
        };
        self.installed.upsert(&record)?;
        self.load_one(&path, record.trusted)
    }

    /// Deletes the file and the record and stops offering its providers. The runtime keeps the
    /// code in memory until the app restarts, but nothing can reach it any more.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn uninstall(&self, extension: &InstalledExtension) -> Result<(), ServiceError> {
        self.installer.uninstall(&extension.internal_name)?;
        self.installed.remove(&extension.file_name)?;
        let names = self
            .provided_by
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&extension.file_name)
            .unwrap_or_default();
        self.providers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|provider| !names.contains(&provider.name));
        Ok(())
    }

    /// Names of the providers an installed extension file registered.
    #[tracing::instrument(level = "debug", skip_all, fields(file_name = ?file_name))]
    pub fn providers_of(&self, file_name: &str) -> Vec<String> {
        self.provided_by
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(file_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Repositories that extensions asked the app to add since the last call (the "Mega" plugin
    /// does this on every start).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, ServiceError> {
        Ok(self.gateway.take_added_repositories()?)
    }

    /// Gives installed extensions that have no icon yet the icon their repository lists for them.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn remember_icons(&self, offered: &[PluginListing]) {
        let Ok(installed) = self.installed.list() else {
            return;
        };
        for extension in installed.iter().filter(|e| e.icon_url.is_none()) {
            let icon = offered
                .iter()
                .find(|plugin| plugin.internal_name == extension.internal_name)
                .and_then(|plugin| plugin.icon_url.as_deref())
                .filter(|icon| !icon.trim().is_empty());
            if let Some(icon) = icon {
                if let Err(error) = self
                    .installed
                    .set_icon_if_missing(&extension.internal_name, icon)
                {
                    tracing::warn!(%error, "could not save the extension icon");
                }
            }
        }
    }

    /// Keeps extension calls in flight from timing out for the next `extra` while the viewer passes a
    /// check in a browser window; call it again to keep them waiting.
    pub fn extend_call_deadline(&self, extra: Duration) {
        self.gateway.extend_call_deadline(extra);
    }

    /// The app is closing: ends the extension runtime and every process it started.
    pub fn shutdown(&self) {
        self.gateway.shutdown();
    }

    /// Tells the extension runtime whether debug logging is on. A runtime that cannot be
    /// reached is not an error here: it picks the choice up when it starts.
    pub fn set_debug_logging(&self, enabled: bool) {
        if let Err(error) = self.gateway.set_debug_logging(enabled) {
            tracing::warn!(%error, enabled, "the extension runtime did not take the debug logging choice");
        }
    }

    /// Changes whether an extension is trusted; takes effect the next time it is loaded.
    #[tracing::instrument(level = "debug", skip_all, fields(trusted = ?trusted), err)]
    pub fn set_trusted(
        &self,
        extension: &InstalledExtension,
        trusted: bool,
    ) -> Result<(), ServiceError> {
        Ok(self.installed.upsert(&InstalledExtension {
            trusted,
            ..extension.clone()
        })?)
    }

    /// Loads every installed extension; one failing extension never blocks the others.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn load_installed(&self) -> Result<ExtensionLoadReport, ServiceError> {
        self.load_installed_reporting(&|_| {})
    }

    /// Like [`Self::load_installed`], telling `on_progress` before each extension starts and
    /// once more when all are done.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn load_installed_reporting(
        &self,
        on_progress: &dyn Fn(&LoadProgress),
    ) -> Result<ExtensionLoadReport, ServiceError> {
        let mut report = ExtensionLoadReport::default();
        let installed = self.installed.list()?;
        let total = installed.len();
        for (finished, extension) in installed.into_iter().enumerate() {
            on_progress(&LoadProgress {
                finished,
                total,
                current: extension.internal_name.clone(),
            });
            let path = self.installer.extensions_dir().join(&extension.file_name);
            match self.load_one(&path, extension.trusted) {
                Ok(mut providers) => report.providers.append(&mut providers),
                Err(error) => {
                    tracing::warn!(extension = %extension.file_name, %error, "extension failed to load");
                    report
                        .failures
                        .push((extension.file_name, error.to_string()));
                }
            }
        }
        on_progress(&LoadProgress {
            finished: total,
            total,
            current: String::new(),
        });
        Ok(report)
    }

    fn load_one(&self, path: &Path, trusted: bool) -> Result<Vec<ProviderInfo>, ServiceError> {
        let extension = file_name_of(path);
        let started = Instant::now();
        tracing::info!(%extension, trusted, "loading extension");
        let loaded = self.gateway.load_extension(path, trusted)?;
        let names: Vec<&str> = loaded
            .iter()
            .map(|provider| provider.name.as_str())
            .collect();
        if names.is_empty() {
            tracing::warn!(%extension, "extension loaded but registered no providers");
        }
        tracing::info!(
            %extension,
            providers = ?names,
            elapsed_ms = started.elapsed().as_millis(),
            "extension loaded"
        );
        self.provided_by
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                file_name_of(path),
                loaded
                    .iter()
                    .map(|provider| provider.name.clone())
                    .collect(),
            );
        let mut known = self
            .providers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for provider in &loaded {
            known.retain(|existing| existing.name != provider.name);
            known.push(provider.clone());
        }
        Ok(loaded)
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
#[cfg(test)]
#[path = "extension_service_tests.rs"]
mod tests;
