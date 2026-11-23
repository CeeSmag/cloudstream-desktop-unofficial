//! The content policy across install, start-up loading, trust, the provider list and the catalog
//! backfill, against the fake runtime.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use core_model::{InstalledExtension, PluginListing};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher, RepoError};
use storage::{Database, InstalledExtensionStore};

use crate::test_support::{FakeGateway, FixedClock};
use crate::{Clock, ExtensionService, PluginHit, ServiceError, Support};

const REPO: &str = "https://cdn.example/repo.json";
const MEGA: &str = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json";

struct BytesFetcher;

impl HttpFetcher for BytesFetcher {
    fn get_bytes(&self, _url: &str) -> Result<Vec<u8>, RepoError> {
        Ok(b"PK".to_vec())
    }
}

struct Setup {
    service: ExtensionService,
    gateway: Arc<FakeGateway>,
    store: InstalledExtensionStore,
    dir: PathBuf,
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn setup(label: &str) -> Setup {
    let dir = std::env::temp_dir().join(format!("cs_ext_policy_{label}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let gateway = Arc::new(FakeGateway::default());
    let fetcher: Arc<dyn HttpFetcher> = Arc::new(BytesFetcher);
    let store =
        InstalledExtensionStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
    let service = ExtensionService::new(
        Arc::clone(&gateway) as Arc<dyn ProviderGateway>,
        ExtensionInstaller::new(fetcher, dir.clone()),
        store.clone(),
        Arc::new(FixedClock::at(5_000)) as Arc<dyn Clock>,
    );
    Setup {
        service,
        gateway,
        store,
        dir,
    }
}

fn listing(name: &str, types: &[&str]) -> PluginListing {
    PluginListing {
        internal_name: name.into(),
        url: format!("https://cdn.example/{name}.cs3"),
        version: 1,
        tv_types: types.iter().map(|t| (*t).to_owned()).collect(),
        ..PluginListing::default()
    }
}

/// An extension recorded as installed, as an older app version left it (`declared_types: None`)
/// or with the given types.
fn recorded(setup: &Setup, name: &str, declared: Option<&[&str]>) {
    setup
        .store
        .upsert(&InstalledExtension {
            file_name: format!("{name}.cs3"),
            internal_name: name.into(),
            repository_url: Some(REPO.into()),
            version: 1,
            declared_types: declared.map(|types| types.iter().map(|t| (*t).to_owned()).collect()),
            ..InstalledExtension::default()
        })
        .expect("record");
}

fn script_providers(setup: &Setup, file: &str, providers: &[(&str, &[&str])]) {
    setup
        .gateway
        .providers_by_file
        .lock()
        .expect("lock")
        .insert(
            file.into(),
            providers
                .iter()
                .map(|(name, types)| {
                    (
                        (*name).to_owned(),
                        types.iter().map(|t| (*t).to_owned()).collect(),
                    )
                })
                .collect(),
        );
}

fn offered(service: &ExtensionService) -> Vec<String> {
    service.providers().into_iter().map(|p| p.name).collect()
}

fn loads(setup: &Setup) -> usize {
    setup.gateway.loaded_paths.lock().expect("lock").len()
}

#[test]
fn a_plugin_that_is_not_video_is_refused_before_any_download() {
    let s = setup("refuse");
    let error = s
        .service
        .install(&listing("Doom", &["Others"]), REPO)
        .expect_err("refused");
    assert!(matches!(&error, ServiceError::NotVideo { plugin, .. } if plugin == "Doom"));
    assert_eq!(
        error.to_string(),
        "Doom is not a video plugin (it declares Others)"
    );
    assert!(!s.dir.join("Doom.cs3").exists(), "nothing downloaded");
    assert_eq!(loads(&s), 0, "nothing loaded");
    assert!(
        s.service.installed().expect("list").is_empty(),
        "nothing recorded"
    );
}

#[test]
fn an_install_records_the_listing_types() {
    let s = setup("record");
    s.service
        .install(&listing("Hub", &["Movie", "TvSeries"]), REPO)
        .expect("installs");
    let record = &s.service.installed().expect("list")[0];
    assert_eq!(
        record.declared_types,
        Some(vec!["Movie".to_owned(), "TvSeries".to_owned()])
    );
}

#[test]
fn reviewed_plugins_install_whatever_they_declare() {
    let s = setup("reviewed");
    let mega = PluginListing {
        url: "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/MegaProvider.cs3"
            .into(),
        ..listing("MegaProvider", &[])
    };
    s.service
        .install(&mega, MEGA)
        .expect("MegaProvider is reviewed");
    assert_eq!(loads(&s), 1);
}

#[test]
fn an_installed_plugin_that_is_not_video_is_never_loaded_at_start() {
    let s = setup("startup");
    recorded(&s, "Doom", Some(&["Others"]));
    recorded(&s, "Hub", Some(&["Movie"]));
    let report = s.service.load_installed().expect("loads");
    assert_eq!(loads(&s), 1, "only the video plugin reached the runtime");
    assert_eq!(offered(&s.service), ["Hub"]);
    assert_eq!(
        report.not_video,
        [(
            "Doom.cs3".to_owned(),
            "not a video plugin (it declares Others)".to_owned()
        )]
    );
    assert!(report.failures.is_empty(), "a refusal is not a failure");
}

#[test]
fn an_old_record_is_judged_by_its_providers_and_a_refusal_is_remembered() {
    let s = setup("legacy");
    recorded(&s, "Doom", None);
    script_providers(&s, "Doom", &[("Doom", &["Others"])]);
    let report = s.service.load_installed().expect("loads");
    assert!(offered(&s.service).is_empty(), "never offered");
    assert_eq!(report.not_video.len(), 1);
    assert_eq!(
        s.gateway.unloaded_paths.lock().expect("lock").len(),
        1,
        "unloaded again"
    );
    assert_eq!(
        s.service.installed().expect("list")[0].declared_types,
        Some(vec!["Others".to_owned()]),
        "recorded from its providers"
    );
    // The next start knows: it does not even load it.
    s.gateway.loaded_paths.lock().expect("lock").clear();
    s.service.load_installed().expect("loads");
    assert_eq!(loads(&s), 0);
}

#[test]
fn an_old_record_of_a_video_plugin_loads_and_records_its_types() {
    let s = setup("legacy-video");
    recorded(&s, "Hub", None);
    script_providers(&s, "Hub", &[("Hub", &["Movie"])]);
    s.service.load_installed().expect("loads");
    assert_eq!(offered(&s.service), ["Hub"]);
    assert_eq!(
        s.service.installed().expect("list")[0].declared_types,
        Some(vec!["Movie".to_owned()])
    );
}

#[test]
fn a_provider_that_is_not_video_is_not_offered() {
    let s = setup("providers");
    script_providers(
        &s,
        "Youtube",
        &[("YoutubeVideos", &["Movie"]), ("YoutubeMusic", &["Music"])],
    );
    let installed = s
        .service
        .install(&listing("Youtube", &["Movie", "Music"]), REPO)
        .expect("installs");
    let names: Vec<String> = installed.into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["YoutubeVideos"]);
    assert_eq!(offered(&s.service), ["YoutubeVideos"]);
}

#[test]
fn trust_cannot_be_changed_for_a_plugin_that_is_not_video() {
    let s = setup("trust");
    recorded(&s, "Doom", Some(&["Others"]));
    let record = s.service.installed().expect("list").remove(0);
    assert!(matches!(
        s.service.set_trusted(&record, true),
        Err(ServiceError::NotVideo { .. })
    ));
    assert_eq!(loads(&s), 0);
    assert!(
        !s.service.installed().expect("list")[0].trusted,
        "nothing changed"
    );
}

#[test]
fn installed_plugins_carry_their_support() {
    let s = setup("status");
    recorded(&s, "Doom", Some(&["Others"]));
    recorded(&s, "Hub", Some(&["Anime"]));
    recorded(&s, "Old", None);
    let support: Vec<(String, Support)> = s
        .service
        .installed_with_status()
        .expect("list")
        .into_iter()
        .map(|status| (status.extension.internal_name, status.support))
        .collect();
    assert_eq!(
        support,
        [
            (
                "Doom".to_owned(),
                Support::NotVideo("not a video plugin (it declares Others)".into())
            ),
            ("Hub".to_owned(), Support::Supported),
            ("Old".to_owned(), Support::Unknown),
        ]
    );
}

fn hit(name: &str, repository_url: &str, types: &[&str]) -> PluginHit {
    PluginHit {
        repository_url: repository_url.into(),
        repository_name: "R".into(),
        repository_icon_url: None,
        plugin: listing(name, types),
    }
}

#[test]
fn providers_without_types_leave_an_old_record_unknown_and_refused_for_now() {
    let s = setup("typeless");
    recorded(&s, "Quiet", None);
    let report = s.service.load_installed().expect("loads");
    assert_eq!(report.not_video.len(), 1, "no type at all is not video");
    assert_eq!(
        s.service.installed().expect("list")[0].declared_types,
        None,
        "the listing may still tell"
    );
}

/// Old records whose plugins failed to load stay unknown; the catalog then decides.
fn backfilled() -> Setup {
    let s = setup("backfill");
    recorded(&s, "BrokenDoom", None);
    recorded(&s, "BrokenHub", None);
    recorded(&s, "Known", Some(&["Movie"]));
    s.service.load_installed().expect("loads");
    let catalog = [
        hit(
            "BrokenDoom",
            "https://cdn.example/repo.json?mirror=1",
            &["Others"],
        ),
        hit("BrokenHub", REPO, &["Movie"]),
        hit("Known", REPO, &["Others"]),
        hit("BrokenHub", "https://other.example/repo.json", &["Others"]),
    ];
    assert_eq!(
        s.service.backfill_declared_types(&catalog),
        2,
        "only unknown rows are filled"
    );
    s
}

#[test]
fn the_catalog_fills_unknown_types_by_repository_and_name_only() {
    let s = backfilled();
    let types: Vec<Option<Vec<String>>> = s
        .service
        .installed()
        .expect("list")
        .into_iter()
        .map(|e| e.declared_types)
        .collect();
    assert_eq!(
        types,
        [
            Some(vec!["Others".to_owned()]),
            Some(vec!["Movie".to_owned()]),
            Some(vec!["Movie".to_owned()])
        ],
        "matched by repository and name; known types kept"
    );
}

#[test]
fn the_catalog_unloads_what_turns_out_not_video() {
    let s = backfilled();
    let unloaded = s.gateway.unloaded_paths.lock().expect("lock").clone();
    assert_eq!(unloaded.len(), 1);
    assert!(
        unloaded[0].ends_with("BrokenDoom.cs3"),
        "the refused one is unloaded: {unloaded:?}"
    );
    assert_eq!(offered(&s.service), ["Known"]);
}

/// Deterministic interleaving: at the moment the runtime unloads a refused old plugin, its
/// providers must never have been offered (they are judged before registration, not after).
#[test]
fn a_refused_old_plugin_is_never_offered_even_for_a_moment() {
    let s = setup("interleave");
    recorded(&s, "Doom", None);
    script_providers(&s, "Doom", &[("Doom", &["Others"])]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (service, seen_in_hook) = (s.service.clone(), Arc::clone(&seen));
    *s.gateway.on_unload.lock().expect("lock") = Some(Box::new(move || {
        seen_in_hook.lock().expect("lock").push(offered(&service));
    }));
    s.service.load_installed().expect("loads");
    assert_eq!(*seen.lock().expect("lock"), [Vec::<String>::new()]);
}

/// Stress: readers of the provider list during start-up loading never see a refused plugin.
#[test]
fn concurrent_readers_never_see_a_refused_plugin() {
    for round in 0..50 {
        let s = setup(&format!("stress{round}"));
        for i in 0..6 {
            recorded(&s, &format!("Game{i}"), None);
            script_providers(&s, &format!("Game{i}"), &[("GameProvider", &["Others"])]);
            recorded(&s, &format!("Hub{i}"), None);
            script_providers(&s, &format!("Hub{i}"), &[(&*format!("Hub{i}"), &["Movie"])]);
        }
        let done = Arc::new(AtomicBool::new(false));
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (service, done) = (s.service.clone(), Arc::clone(&done));
                thread::spawn(move || {
                    let mut leaked = false;
                    while !done.load(Ordering::Acquire) {
                        leaked |= offered(&service).iter().any(|name| name == "GameProvider");
                    }
                    leaked
                })
            })
            .collect();
        s.service.load_installed().expect("loads");
        done.store(true, Ordering::Release);
        for reader in readers {
            assert!(
                !reader.join().expect("reader"),
                "round {round}: a refused provider was offered"
            );
        }
        assert_eq!(offered(&s.service).len(), 6);
    }
}
