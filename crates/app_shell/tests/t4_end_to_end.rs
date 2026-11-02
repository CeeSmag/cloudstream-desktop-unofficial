//! T4: the whole stack without a window, against the real network.
//!
//! repository -> install official Internet Archive extension -> sidecar loads it -> search ->
//! details -> resolve streams -> headless mpv plays the stream -> progress is saved.
//!
//! Needs the sidecar, libmpv and network. Run with
//! `CS_SIDECAR_HOME=... CS_SIDECAR_JAVA_HOME=... CS_LIBMPV_PATH=...
//!  cargo test -p app_shell --test t4_end_to_end -- --ignored --nocapture --test-threads=1`
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::time::{Duration, Instant};

use app_services::PlaybackItem;
use app_shell::{build_services, AppPaths, ServiceOptions};
use core_model::PlayableLink;
use player_api::PlaybackState;
use ui_app::UiServices;

const OFFICIAL_REPOSITORY: &str =
    "https://raw.githubusercontent.com/recloudstream/extensions/master/repo.json";
const PLUGIN: &str = "InternetArchiveProvider";
const QUERY: &str = "apollo";

fn data_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cs_t4_{label}_{}", std::process::id()))
}

fn services_in(dir: std::path::PathBuf) -> UiServices {
    let paths = AppPaths::for_directory(dir);
    build_services(
        &paths,
        &ServiceOptions {
            headless_player: true,
            ..ServiceOptions::default()
        },
    )
    .expect("services build")
}

fn install_internet_archive(services: &UiServices) {
    let entry = services
        .repositories
        .add(OFFICIAL_REPOSITORY)
        .expect("official repository is reachable");
    let listing = services
        .repositories
        .plugins(&entry.url)
        .expect("plugin list")
        .into_iter()
        .find(|plugin| plugin.internal_name == PLUGIN)
        .expect("the official repository offers the Internet Archive plugin");
    let providers = services
        .extensions
        .install(&listing, &entry.url)
        .expect("plugin installs and loads");
    assert!(
        providers
            .iter()
            .any(|provider| provider.name == "Internet Archive"),
        "{providers:?}"
    );
}

/// Finds titles whose streams resolve; archive.org items can be restricted or slow.
fn playable_candidates(
    services: &UiServices,
    wanted: usize,
) -> Vec<(core_model::SearchResult, PlayableLink)> {
    let mut found = Vec::new();
    let providers = services.extensions.providers();
    let outcome = services.catalog.search(&providers, QUERY).remove(0);
    let page = outcome.result.expect("search succeeds");
    assert!(!page.items.is_empty(), "search returned nothing");
    // Audio items have no stream links in this extension (it has a TODO for them); use videos.
    let videos = page
        .items
        .into_iter()
        .filter(|item| item.media_type.as_deref() == Some("Movie"));
    for item in videos.take(6) {
        let details = services
            .catalog
            .details("Internet Archive", &item.url)
            .expect("details load");
        let data = details.data_url.clone().unwrap_or_else(|| item.url.clone());
        let links = services
            .streams
            .resolve("Internet Archive", &data)
            .expect("resolver does not error");
        if let Some(link) = links.links.into_iter().next() {
            found.push((item, link));
        }
        if found.len() >= wanted {
            break;
        }
    }
    assert!(
        !found.is_empty(),
        "no video with playable streams among the first results"
    );
    found
}

/// Polls playback until `accept` holds; returns the last state seen so failures are explainable.
fn wait_until(
    services: &UiServices,
    accept: impl Fn(&PlaybackState) -> bool,
    within: Duration,
) -> Result<(), PlaybackState> {
    let playback = services.playback.as_ref().expect("player available");
    let deadline = Instant::now() + within;
    let mut last = PlaybackState::Idle;
    while Instant::now() < deadline {
        last = playback.tick().state;
        if accept(&last) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(last)
}

#[test]
#[ignore = "needs sidecar, libmpv and network (see file header)"]
fn install_search_resolve_and_play_an_official_extension() {
    let dir = data_dir("full");
    let _ = std::fs::remove_dir_all(&dir);
    let services = services_in(dir);
    install_internet_archive(&services);

    let playback = services.playback.as_ref().expect("player available");
    let mut outcomes = Vec::new();
    for (item, link) in playable_candidates(&services, 3) {
        let playback_item = PlaybackItem {
            provider: "Internet Archive".into(),
            media_key: item.url.clone(),
            title: item.name.clone(),
            detail_url: item.url.clone(),
            poster_url: item.poster_url.clone(),
        };
        playback
            .start(playback_item, &link, &[])
            .expect("playback starts");
        let started = wait_until(
            &services,
            |state| *state == PlaybackState::Playing,
            Duration::from_secs(25),
        );
        playback.stop().expect("stops");
        if started.is_ok() {
            return;
        }
        outcomes.push(format!("{} -> {started:?}", link.url));
    }
    // Public streaming hosts are occasionally flaky; one playable candidate is enough, none is a failure.
    panic!("none of the candidate streams played: {outcomes:?}");
}

#[test]
#[ignore = "needs sidecar and network (see file header)"]
fn installed_extension_is_loaded_again_on_the_next_start() {
    let dir = data_dir("restart");
    let _ = std::fs::remove_dir_all(&dir);
    install_internet_archive(&services_in(dir.clone()));

    let restarted = services_in(dir);
    let report = restarted.extensions.load_installed().expect("load report");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.providers.len(), 1);
}
