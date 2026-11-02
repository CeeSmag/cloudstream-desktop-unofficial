use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use core_model::{InstalledExtension, PluginListing, ProviderInfo};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher};
use storage::InstalledExtensionStore;

use crate::ServiceError;

/// What happened when the installed extensions were loaded into the runtime.
#[derive(Debug, Default)]
pub struct ExtensionLoadReport {
    /// Providers that are now available.
    pub providers: Vec<ProviderInfo>,
    /// Extensions that failed to load, with a readable reason; the rest still loaded.
    pub failures: Vec<(String, String)>,
}

/// Installs, removes and loads extension files.
#[derive(Clone)]
pub struct ExtensionService {
    gateway: Arc<dyn ProviderGateway>,
    installer: Arc<ExtensionInstaller<Arc<dyn HttpFetcher>>>,
    installed: InstalledExtensionStore,
    providers: Arc<Mutex<Vec<ProviderInfo>>>,
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

    /// Deletes the file and the record. A running runtime keeps the providers until restart.
    pub fn uninstall(&self, extension: &InstalledExtension) -> Result<(), ServiceError> {
        self.installer.uninstall(&extension.internal_name)?;
        self.installed.remove(&extension.file_name)?;
        Ok(())
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
        let mut report = ExtensionLoadReport::default();
        for extension in self.installed.list()? {
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
        Ok(report)
    }

    fn load_one(&self, path: &Path, trusted: bool) -> Result<Vec<ProviderInfo>, ServiceError> {
        let loaded = self.gateway.load_extension(path, trusted)?;
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
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use extension_repo::RepoError;
    use storage::Database;

    use super::*;
    use crate::test_support::FakeGateway;

    struct BytesFetcher;

    impl HttpFetcher for BytesFetcher {
        fn get_bytes(&self, _url: &str) -> Result<Vec<u8>, RepoError> {
            Ok(b"PK".to_vec())
        }
    }

    fn setup(label: &str) -> (ExtensionService, Arc<FakeGateway>, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("cs_ext_service_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let gateway = Arc::new(FakeGateway::default());
        let fetcher: Arc<dyn HttpFetcher> = Arc::new(BytesFetcher);
        let installed =
            InstalledExtensionStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
        let service = ExtensionService::new(
            Arc::clone(&gateway) as Arc<dyn ProviderGateway>,
            ExtensionInstaller::new(fetcher, dir.clone()),
            installed,
        );
        (service, gateway, dir)
    }

    fn listing(name: &str) -> PluginListing {
        PluginListing {
            internal_name: name.into(),
            url: "https://r/x.cs3".into(),
            version: 3,
            ..PluginListing::default()
        }
    }

    #[test]
    fn install_downloads_records_and_loads_untrusted() {
        let (service, gateway, dir) = setup("install");
        let providers = service
            .install(&listing("Good"), "https://r/repo.json")
            .expect("installs");
        assert_eq!(providers[0].name, "Good");
        assert_eq!(service.installed().expect("list")[0].version, 3);
        assert!(
            !gateway.loaded_paths.lock().expect("lock")[0].1,
            "new installs start untrusted"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_installed_reports_failures_without_blocking_other_extensions() {
        let (service, _, dir) = setup("failures");
        service.install(&listing("Good"), "r").expect("installs");
        let _ = service.install(&listing("BrokenOne"), "r");
        let report = service.load_installed().expect("loads");
        assert_eq!(report.providers.len(), 1);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].0.starts_with("BrokenOne"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn trusting_an_extension_is_used_on_the_next_load() {
        let (service, gateway, dir) = setup("trust");
        service.install(&listing("Good"), "r").expect("installs");
        let record = service.installed().expect("list").remove(0);
        service.set_trusted(&record, true).expect("trust");
        service.load_installed().expect("loads");
        assert!(
            gateway
                .loaded_paths
                .lock()
                .expect("lock")
                .last()
                .expect("a load")
                .1
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uninstall_removes_file_and_record() {
        let (service, _, dir) = setup("uninstall");
        service.install(&listing("Good"), "r").expect("installs");
        let record = service.installed().expect("list").remove(0);
        service.uninstall(&record).expect("uninstalls");
        assert!(service.installed().expect("list").is_empty());
        assert!(!dir.join("Good.cs3").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
