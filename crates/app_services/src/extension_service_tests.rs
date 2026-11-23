use std::fs;
use std::path::PathBuf;

use extension_repo::RepoError;
use storage::Database;

use super::*;
use crate::test_support::{FakeGateway, FixedClock};
use crate::Clock;

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
        Arc::new(FixedClock::at(5_000)) as Arc<dyn Clock>,
    );
    (service, gateway, dir)
}

const REPO: &str = "https://cdn.example/repo.json";

fn listing(name: &str) -> PluginListing {
    PluginListing {
        internal_name: name.into(),
        url: "https://cdn.example/x.cs3".into(),
        version: 3,
        ..PluginListing::default()
    }
}

#[test]
fn uninstall_rejects_unsafe_stored_file_names() {
    let (service, _, dir) = setup("jail");
    service.install(&listing("Good"), REPO).expect("installs");
    let mut record = service.installed().expect("list").remove(0);
    record.file_name = "../Good.cs3".into();
    assert!(service.uninstall(&record).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn install_downloads_records_and_loads_untrusted() {
    let (service, gateway, dir) = setup("install");
    let providers = service.install(&listing("Good"), REPO).expect("installs");
    assert_eq!(providers[0].name, "Good");
    let recorded = &service.installed().expect("list")[0];
    assert_eq!(recorded.version, 3);
    assert_eq!(recorded.installed_at_ms, 5_000);
    assert!(
        !gateway.loaded_paths.lock().expect("lock")[0].1,
        "new installs start untrusted"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_installed_reports_failures_without_blocking_other_extensions() {
    let (service, _, dir) = setup("failures");
    service.install(&listing("Good"), REPO).expect("installs");
    let _ = service.install(&listing("BrokenOne"), REPO);
    let report = service.load_installed().expect("loads");
    assert_eq!(report.providers.len(), 1);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].0.starts_with("BrokenOne"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn loading_reports_each_extension_and_a_final_done() {
    let (service, _, dir) = setup("progress");
    service.install(&listing("Good"), REPO).expect("installs");
    service.install(&listing("Other"), REPO).expect("installs");
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
fn trusting_an_extension_unloads_and_reloads_it_immediately() {
    let (service, gateway, dir) = setup("trust");
    service.install(&listing("Good"), REPO).expect("installs");
    let record = service.installed().expect("list").remove(0);
    gateway.loaded_paths.lock().expect("lock").clear();
    gateway.unloaded_paths.lock().expect("lock").clear();
    service.set_trusted(&record, true).expect("trust");
    assert_eq!(gateway.unloaded_paths.lock().expect("lock").len(), 1);
    let loads = gateway.loaded_paths.lock().expect("lock");
    assert_eq!(loads.len(), 1);
    assert!(loads[0].1, "reload must use trusted=true");
    assert!(service.installed().expect("list")[0].trusted);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn uninstall_stops_offering_the_providers_of_that_extension_only() {
    let (service, _, dir) = setup("hide");
    service.install(&listing("Good"), REPO).expect("installs");
    service.install(&listing("Other"), REPO).expect("installs");
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
    service.install(&listing("Good"), REPO).expect("installs");
    let record = service.installed().expect("list").remove(0);
    service.uninstall(&record).expect("uninstalls");
    assert!(service.installed().expect("list").is_empty());
    assert!(!dir.join("Good.cs3").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn uninstall_unloads_the_runtime_so_a_reinstall_can_load_again() {
    let (service, gateway, dir) = setup("unload");
    service.install(&listing("Good"), REPO).expect("installs");
    let record = service.installed().expect("list").remove(0);
    service.uninstall(&record).expect("uninstalls");
    let unloaded = gateway.unloaded_paths.lock().expect("lock");
    assert_eq!(unloaded.len(), 1);
    assert!(
        unloaded[0].ends_with("Good.cs3"),
        "unload path was {}",
        unloaded[0]
    );
    drop(unloaded);
    let providers = service.install(&listing("Good"), REPO).expect("reinstalls");
    assert_eq!(providers[0].name, "Good");
    assert!(service
        .providers()
        .iter()
        .any(|provider| provider.name == "Good"));
    let _ = fs::remove_dir_all(dir);
}

fn with_icon(name: &str, icon: &str) -> PluginListing {
    PluginListing {
        icon_url: Some(icon.into()),
        ..listing(name)
    }
}

fn icon_of(service: &ExtensionService, name: &str) -> Option<String> {
    service
        .installed()
        .expect("list")
        .into_iter()
        .find(|e| e.internal_name == name)
        .and_then(|e| e.icon_url)
}

#[test]
fn a_blank_icon_from_the_repository_is_not_recorded() {
    let (service, _, dir) = setup("blank_icon");
    service
        .install(&with_icon("Good", "   "), REPO)
        .expect("installs");
    assert_eq!(icon_of(&service, "Good"), None);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn remember_icons_fills_in_missing_icons_and_never_replaces_one() {
    let (service, _, dir) = setup("icons");
    service.install(&listing("Bare"), REPO).expect("installs");
    service
        .install(&with_icon("Dressed", "https://i/own.png"), REPO)
        .expect("installs");
    service
        .install(&listing("StillBare"), REPO)
        .expect("installs");

    service.remember_icons(&[
        with_icon("Bare", "https://i/bare.png"),
        with_icon("Dressed", "https://i/other.png"),
        with_icon("StillBare", " "),
        with_icon("NotInstalled", "https://i/x.png"),
    ]);

    assert_eq!(
        icon_of(&service, "Bare").as_deref(),
        Some("https://i/bare.png")
    );
    assert_eq!(
        icon_of(&service, "Dressed").as_deref(),
        Some("https://i/own.png")
    );
    assert_eq!(
        icon_of(&service, "StillBare"),
        None,
        "a blank icon is no icon"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn providers_of_names_what_each_file_registered() {
    let (service, _, dir) = setup("providers_of");
    service.install(&listing("Good"), REPO).expect("installs");
    let record = service.installed().expect("list").remove(0);
    assert_eq!(service.providers_of(&record.file_name), ["Good"]);
    assert!(service.providers_of("Unknown.cs3").is_empty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn repositories_the_plugins_asked_for_are_handed_out_once() {
    let (service, gateway, dir) = setup("added_repos");
    let found = DiscoveredRepository {
        name: "Mega".into(),
        url: "https://mega.example/repo.json".into(),
        icon_url: None,
    };
    gateway
        .added_repositories
        .lock()
        .expect("lock")
        .push(found.clone());
    assert_eq!(service.take_added_repositories().expect("taken"), [found]);
    assert!(service.take_added_repositories().expect("taken").is_empty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_controls_reach_the_runtime_and_a_refused_debug_choice_is_not_an_error() {
    let (service, gateway, dir) = setup("controls");
    service.set_debug_logging(true);
    gateway
        .refuses_debug_logging
        .store(true, std::sync::atomic::Ordering::SeqCst);
    service.set_debug_logging(false);
    service.extend_call_deadline(Duration::from_secs(30));
    service.shutdown();
    assert_eq!(
        *gateway.controls.lock().expect("lock"),
        ["debug true", "debug false", "extend 30s", "shutdown"]
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn reloading_one_extension_keeps_every_other_extensions_providers() {
    let (service, _, dir) = setup("reload_keeps");
    service.install(&listing("Good"), REPO).expect("installs");
    service.install(&listing("Other"), REPO).expect("installs");
    let good = service
        .installed()
        .expect("list")
        .into_iter()
        .find(|e| e.internal_name == "Good")
        .expect("recorded");
    service.set_trusted(&good, true).expect("trust");
    let mut names: Vec<String> = service.providers().into_iter().map(|p| p.name).collect();
    names.sort();
    assert_eq!(
        names,
        ["Good", "Other"],
        "one entry each, nothing dropped or doubled"
    );
    let _ = fs::remove_dir_all(dir);
}
