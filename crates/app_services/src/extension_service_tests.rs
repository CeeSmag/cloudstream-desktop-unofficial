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
    let dir = std::env::temp_dir().join(format!("cs_ext_service_{label}_{}", std::process::id()));
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
fn loading_reports_each_extension_and_a_final_done() {
    let (service, _, dir) = setup("progress");
    service.install(&listing("Good"), "r").expect("installs");
    service.install(&listing("Other"), "r").expect("installs");
    let seen = Mutex::new(Vec::new());
    service
        .load_installed_reporting(&|p| seen.lock().expect("lock").push(p.clone()))
        .expect("loads");
    let seen = seen.into_inner().expect("lock");
    let steps: Vec<(usize, usize, &str)> = seen
        .iter()
        .map(|p| (p.finished, p.total, p.current.as_str()))
        .collect();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[2], (2, 2, ""));
    assert!(steps[0].2 == "Good" || steps[0].2 == "Other");
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
fn uninstall_stops_offering_the_providers_of_that_extension_only() {
    let (service, _, dir) = setup("hide");
    service.install(&listing("Good"), "r").expect("installs");
    service.install(&listing("Other"), "r").expect("installs");
    let record = service
        .installed()
        .expect("list")
        .into_iter()
        .find(|e| e.internal_name == "Good")
        .expect("recorded");
    service.uninstall(&record).expect("uninstalls");
    let names: Vec<String> = service.providers().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Other"]);
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
