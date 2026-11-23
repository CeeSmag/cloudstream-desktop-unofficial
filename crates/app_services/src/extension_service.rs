use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use content_policy::{ContentPolicy, Verdict};
use core_model::{DiscoveredRepository, InstalledExtension, PluginListing, ProviderInfo};
use extension_api::ProviderGateway;
use extension_repo::{plugin_path_in, ExtensionInstaller, HttpFetcher};
use storage::InstalledExtensionStore;

use crate::clock::Clock;
use crate::content_support::{judge_by_providers, judge_installed, judge_listing, listing_label};
use crate::extension_load_report::{ExtensionLoadReport, LoadProgress};
use crate::ServiceError;

/// Installs, removes and loads extension files. Only video plugins ([`ContentPolicy`]) are
/// installed, loaded and offered; see `content_support`.
#[derive(Clone)]
pub struct ExtensionService {
    pub(crate) gateway: Arc<dyn ProviderGateway>,
    pub(crate) installer: Arc<ExtensionInstaller<Arc<dyn HttpFetcher>>>,
    pub(crate) installed: InstalledExtensionStore,
    clock: Arc<dyn Clock>,
    pub(crate) policy: ContentPolicy,
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
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            gateway,
            installer: Arc::new(installer),
            installed,
            clock,
            policy: ContentPolicy::standard(),
            providers: Arc::default(),
            provided_by: Arc::default(),
        }
    }

    /// Uses `policy` instead of the app's standard content policy (tests).
    #[must_use]
    pub fn with_content_policy(mut self, policy: ContentPolicy) -> Self {
        self.policy = policy;
        self
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

    /// Downloads a plugin, records it and loads it into the runtime. A plugin that is not a video
    /// plugin is refused before anything is downloaded.
    #[tracing::instrument(level = "debug", skip_all, fields(repository_url = ?repository_url), err)]
    pub fn install(
        &self,
        listing: &PluginListing,
        repository_url: &str,
    ) -> Result<Vec<ProviderInfo>, ServiceError> {
        let verdict = judge_listing(&self.policy, repository_url, listing);
        if let Verdict::NotVideo(refusal) = &verdict {
            tracing::info!(plugin = %listing.internal_name, %refusal, "install refused");
            return Err(ServiceError::NotVideo {
                plugin: listing_label(listing).to_owned(),
                reason: refusal.to_string(),
            });
        }
        let path = self.installer.install(listing, repository_url)?;
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
            installed_at_ms: self.clock.now_ms(),
            declared_types: Some(listing.tv_types.clone()),
        };
        self.installed.upsert(&record)?;
        self.load_judged(&path, record.trusted, &verdict)
    }

    /// Unloads the extension in the runtime, deletes the file and the record, and stops offering
    /// its providers. Unloading first means a later install of the same plugin can register again
    /// without restarting the app.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn uninstall(&self, extension: &InstalledExtension) -> Result<(), ServiceError> {
        let path = plugin_path_in(self.installer.extensions_dir(), &extension.file_name)?;
        if let Err(error) = self.gateway.unload_extension(&path) {
            tracing::warn!(
                extension = %extension.file_name,
                %error,
                "the extension runtime did not unload the plugin; a reinstall may need an app restart"
            );
        }
        self.installer.uninstall_file(&extension.file_name)?;
        self.installed.remove(&extension.file_name)?;
        self.unregister(&extension.file_name);
        Ok(())
    }

    /// Stops offering the providers an extension file registered.
    pub(crate) fn unregister(&self, file_name: &str) {
        let names = self
            .provided_by
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(file_name)
            .unwrap_or_default();
        self.providers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|provider| !names.contains(&provider.name));
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

    /// Changes whether an extension is trusted and reloads it so the new sandbox mode applies now.
    /// Refused for an extension that is not a video plugin (it is never loaded).
    #[tracing::instrument(level = "debug", skip_all, fields(trusted = ?trusted), err)]
    pub fn set_trusted(
        &self,
        extension: &InstalledExtension,
        trusted: bool,
    ) -> Result<(), ServiceError> {
        let verdict = judge_installed(&self.policy, extension);
        if let Some(Verdict::NotVideo(refusal)) = &verdict {
            return Err(ServiceError::NotVideo {
                plugin: extension.internal_name.clone(),
                reason: refusal.to_string(),
            });
        }
        let updated = InstalledExtension {
            trusted,
            ..extension.clone()
        };
        self.installed.upsert(&updated)?;
        let path = plugin_path_in(self.installer.extensions_dir(), &updated.file_name)?;
        if let Err(error) = self.gateway.unload_extension(&path) {
            tracing::warn!(
                extension = %updated.file_name,
                %error,
                "the extension runtime did not unload before the trust reload"
            );
        }
        self.unregister(&updated.file_name);
        match verdict {
            Some(verdict) => self.load_judged(&path, trusted, &verdict).map(drop),
            None => match self.load_unjudged(&updated, &path) {
                LoadOutcome::Loaded(_) => Ok(()),
                LoadOutcome::Failed(error) => Err(error),
                LoadOutcome::NotVideo(reason) => Err(ServiceError::NotVideo {
                    plugin: updated.internal_name.clone(),
                    reason,
                }),
            },
        }
    }

    /// Loads every installed extension; one failing extension never blocks the others.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn load_installed(&self) -> Result<ExtensionLoadReport, ServiceError> {
        self.load_installed_reporting(&|_| {})
    }

    /// Like [`Self::load_installed`], telling `on_progress` before each extension starts and
    /// once more when all are done. Extensions that are not video plugins are not loaded
    /// ([`ExtensionLoadReport::not_video`]).
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
            self.load_recorded(extension, &mut report);
        }
        on_progress(&LoadProgress {
            finished: total,
            total,
            current: String::new(),
        });
        Ok(report)
    }

    /// Loads one installed extension into `report`: judged by its recorded types, or by its
    /// providers' types when those are unknown.
    fn load_recorded(&self, extension: InstalledExtension, report: &mut ExtensionLoadReport) {
        let path = match plugin_path_in(self.installer.extensions_dir(), &extension.file_name) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(extension = %extension.file_name, %error, "extension file name failed the path jail");
                report
                    .failures
                    .push((extension.file_name, error.to_string()));
                return;
            }
        };
        let outcome = match judge_installed(&self.policy, &extension) {
            Some(Verdict::NotVideo(refusal)) => LoadOutcome::NotVideo(refusal.to_string()),
            Some(verdict) => self.load_judged(&path, extension.trusted, &verdict).into(),
            None => self.load_unjudged(&extension, &path),
        };
        match outcome {
            LoadOutcome::Loaded(mut providers) => report.providers.append(&mut providers),
            LoadOutcome::Failed(error) => {
                tracing::warn!(extension = %extension.file_name, %error, "extension failed to load");
                report
                    .failures
                    .push((extension.file_name, error.to_string()));
            }
            LoadOutcome::NotVideo(reason) => {
                tracing::info!(extension = %extension.file_name, %reason, "extension not loaded");
                report.not_video.push((extension.file_name, reason));
            }
        }
    }

    /// Loads an extension whose verdict is known and offers the providers the policy shows.
    fn load_judged(
        &self,
        path: &Path,
        trusted: bool,
        verdict: &Verdict,
    ) -> Result<Vec<ProviderInfo>, ServiceError> {
        let loaded = self.load_raw(path, trusted)?;
        let (shown, hidden): (Vec<ProviderInfo>, Vec<ProviderInfo>) = loaded
            .into_iter()
            .partition(|provider| self.policy.shows_provider(verdict, &provider.types));
        if !hidden.is_empty() {
            let names: Vec<&str> = hidden.iter().map(|p| p.name.as_str()).collect();
            tracing::info!(extension = %file_name_of(path), hidden = ?names, "providers that are not video are not offered");
        }
        self.register(path, &shown);
        Ok(shown)
    }

    /// Loads an extension recorded before content types were (`declared_types` unknown). Its
    /// providers' types decide and are recorded; a refused extension is unloaded again before
    /// any of its providers is offered.
    fn load_unjudged(&self, extension: &InstalledExtension, path: &Path) -> LoadOutcome {
        let loaded = match self.load_raw(path, extension.trusted) {
            Ok(loaded) => loaded,
            Err(error) => return LoadOutcome::Failed(error),
        };
        let (verdict, types) = judge_by_providers(&self.policy, extension, &loaded);
        // Providers that report no types tell nothing: stay unknown, so the listing can still decide.
        if !types.is_empty() {
            if let Err(error) = self
                .installed
                .set_declared_types_if_missing(&extension.file_name, &types)
            {
                tracing::warn!(extension = %extension.file_name, %error, "could not record the content types");
            }
        }
        if let Verdict::NotVideo(refusal) = &verdict {
            if let Err(error) = self.gateway.unload_extension(path) {
                tracing::warn!(extension = %extension.file_name, %error, "could not unload a refused extension");
            }
            return LoadOutcome::NotVideo(refusal.to_string());
        }
        let shown: Vec<ProviderInfo> = loaded
            .into_iter()
            .filter(|provider| self.policy.shows_provider(&verdict, &provider.types))
            .collect();
        self.register(path, &shown);
        LoadOutcome::Loaded(shown)
    }

    /// Loads the file into the runtime; nothing is offered yet.
    fn load_raw(&self, path: &Path, trusted: bool) -> Result<Vec<ProviderInfo>, ServiceError> {
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
        Ok(loaded)
    }

    /// Offers `providers` as the ones the file at `path` registered.
    fn register(&self, path: &Path, providers: &[ProviderInfo]) {
        self.provided_by
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                file_name_of(path),
                providers
                    .iter()
                    .map(|provider| provider.name.clone())
                    .collect(),
            );
        let mut known = self
            .providers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for provider in providers {
            known.retain(|existing| existing.name != provider.name);
            known.push(provider.clone());
        }
    }
}

/// What loading one installed extension came to.
enum LoadOutcome {
    /// Loaded; these providers are offered.
    Loaded(Vec<ProviderInfo>),
    /// It could not be loaded.
    Failed(ServiceError),
    /// Not loaded: not a video plugin (why).
    NotVideo(String),
}

impl From<Result<Vec<ProviderInfo>, ServiceError>> for LoadOutcome {
    fn from(result: Result<Vec<ProviderInfo>, ServiceError>) -> Self {
        result.map_or_else(Self::Failed, Self::Loaded)
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
