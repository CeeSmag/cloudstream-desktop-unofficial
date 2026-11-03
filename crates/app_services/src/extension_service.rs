use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

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
    pub fn installed(&self) -> Result<Vec<InstalledExtension>, ServiceError> {
        Ok(self.installed.list()?)
    }

    /// Downloads a plugin, records it and loads it into the runtime.
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
        };
        self.installed.upsert(&record)?;
        self.load_one(&path, record.trusted)
    }

    /// Deletes the file and the record and stops offering its providers. The runtime keeps the
    /// code in memory until the app restarts, but nothing can reach it any more.
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
    pub fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, ServiceError> {
        Ok(self.gateway.take_added_repositories()?)
    }

    /// Changes whether an extension is trusted; takes effect the next time it is loaded.
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
    pub fn load_installed(&self) -> Result<ExtensionLoadReport, ServiceError> {
        self.load_installed_reporting(&|_| {})
    }

    /// Like [`Self::load_installed`], telling `on_progress` before each extension starts and
    /// once more when all are done.
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
