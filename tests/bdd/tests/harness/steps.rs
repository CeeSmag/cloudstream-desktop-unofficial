//! Step definitions for all 5 CloudStream BDD feature files.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use app_services::{
    CatalogService, ExtensionService, LibraryService, PlaybackItem, PlaybackService,
    RepositoryService, StreamResolver,
};
use core_model::{
    Bookmark, EpisodeInfo, HomePage, HomePageSection, LoadDetails, MainPageEntry, PlayableLink,
    PlayableLinks, ProviderInfo, SearchPage, SearchResult, SubtitleTrack,
};
use cucumber::{given, then, when};
use extension_repo::ExtensionInstaller;
use flate2::write::GzEncoder;
use flate2::Compression;
use playback_session::{run_open, OpenControl, OpenRequest};
use player_api::{Chapter, PlaybackState, PlaybackStatus};
use skip_times::{ExternalIds, SkipKind, SkipLookup, SkipQuery, SkipSegment, TitleKind};
use storage::{
    BookmarkStore, InstalledExtensionStore, RepositoryStore, ResumeStore, WatchProgressStore,
};
use subtitle_store::{
    KeepGoing, LoadOptions, PrefetchConfig, ReadySubtitle, SubtitleCache, SubtitleLoader,
    SubtitlePrefetch, SKIP_DOWNLOAD_RANK,
};

use super::support::{
    AppWorld, FixedClock, LocalHttpServer, MapFetcher, MapSubFetcher, MergedPrompt,
    StaticSkipFetcher,
};

fn repo_service(world: &AppWorld) -> RepositoryService {
    let db = Arc::clone(world.db.as_ref().expect("db"));
    RepositoryService::new(
        Arc::new(MapFetcher(Arc::clone(&world.http_files))),
        RepositoryStore::new(db),
        Arc::new(FixedClock(10_000)),
    )
}

fn extension_service(world: &AppWorld) -> ExtensionService {
    let db = Arc::clone(world.db.as_ref().expect("db"));
    let ext_dir = world.data_dir.as_ref().expect("dir").join("extensions");
    ExtensionService::new(
        Arc::clone(&world.gateway) as Arc<dyn extension_api::ProviderGateway>,
        ExtensionInstaller::new(Arc::new(MapFetcher(Arc::clone(&world.http_files))), ext_dir),
        InstalledExtensionStore::new(db),
        Arc::new(FixedClock(10_000)),
    )
}

fn library_service(world: &AppWorld) -> LibraryService {
    let db = Arc::clone(world.db.as_ref().expect("db"));
    LibraryService::new(
        BookmarkStore::new(Arc::clone(&db)),
        WatchProgressStore::new(Arc::clone(&db)),
        ResumeStore::new(db),
    )
}

fn playback_service(world: &AppWorld) -> PlaybackService {
    let db = Arc::clone(world.db.as_ref().expect("db"));
    PlaybackService::new(
        Arc::clone(&world.player) as Arc<dyn player_api::VideoPlayer>,
        WatchProgressStore::new(Arc::clone(&db)),
        ResumeStore::new(db),
        Arc::new(FixedClock(20_000)),
    )
}

#[given("a clean app data directory with a local SQLite database")]
async fn given_clean_env(world: &mut AppWorld) {
    world.init_temp_env();
}

// --- 1. Extension lifecycle ---

/// Publishes `repo` with one plugin whose listing declares `tv_types` (a JSON array).
fn publish_repository(world: &mut AppWorld, repo: &str, plugin: &str, tv_types: &str) {
    let repo_url = format!("https://repo.example/{repo}/repo.json");
    let plugins_url = format!("https://repo.example/{repo}/plugins.json");
    let cs3_url = format!("https://repo.example/{repo}/{plugin}.cs3");
    let payload = b"PK\x03\x04bdd-cs3-bytes".to_vec();
    let repo_json = format!(r#"{{"name":"{repo}","pluginLists":["{plugins_url}"]}}"#);
    let plugins_json = format!(
        r#"[{{"name":"{plugin}","internalName":"{plugin}","version":2,"url":"{cs3_url}","fileSize":{},"tvTypes":{tv_types}}}]"#,
        payload.len()
    );
    {
        let mut files = world.http_files.lock().expect("http_files");
        files.insert(repo_url.clone(), repo_json.into_bytes());
        files.insert(plugins_url, plugins_json.into_bytes());
        files.insert(cs3_url, payload);
    }
    world.repo_urls.insert(repo.to_owned(), repo_url);
}

#[given(regex = r#"^a remote repository "([^"]+)" offering plugin "([^"]+)" providing "([^"]+)"$"#)]
async fn given_remote_repo(world: &mut AppWorld, repo: String, plugin: String, provider: String) {
    publish_repository(world, &repo, &plugin, r#"["Movie","TvSeries"]"#);
    *world.gateway.providers_on_load.lock().expect("providers") = vec![ProviderInfo {
        name: provider,
        main_url: "https://provider.example".into(),
        lang: "en".into(),
        types: vec!["Movie".into()],
        has_main_page: true,
        main_page: vec![MainPageEntry {
            name: "Popular".into(),
            data: "popular".into(),
            horizontal_images: false,
        }],
    }];
}

#[when(regex = r#"^the user adds the repository "([^"]+)"$"#)]
async fn when_add_repo(world: &mut AppWorld, repo: String) {
    let url = world.repo_urls.get(&repo).expect("repo url").clone();
    let entry = repo_service(world).add(&url).expect("add repo");
    assert_eq!(entry.name, repo);
}

#[when(regex = r#"^the user installs plugin "([^"]+)" from "([^"]+)"$"#)]
async fn when_install_plugin(world: &mut AppWorld, plugin: String, repo: String) {
    let url = world.repo_urls.get(&repo).expect("repo url").clone();
    let plugins = repo_service(world).plugins(&url).expect("plugins");
    let listing = plugins
        .into_iter()
        .find(|p| p.internal_name == plugin)
        .expect("listing found");
    let loaded = extension_service(world)
        .install(&listing, &url)
        .expect("install");
    assert!(!loaded.is_empty());
}

#[given(regex = r#"^a remote repository "([^"]+)" offering the game plugin "([^"]+)"$"#)]
async fn given_game_repo(world: &mut AppWorld, repo: String, plugin: String) {
    publish_repository(world, &repo, &plugin, r#"["Others"]"#);
}

#[then(regex = r#"^plugin "([^"]+)" from "([^"]+)" is not offered$"#)]
async fn then_not_offered(world: &mut AppWorld, plugin: String, repo: String) {
    let url = world.repo_urls.get(&repo).expect("repo url").clone();
    let service = repo_service(world);
    let offered = service.plugins(&url).expect("plugins");
    assert!(
        offered.iter().all(|p| p.internal_name != plugin),
        "{plugin} offered"
    );
    let catalog = service.all_plugins().expect("catalog");
    assert!(
        catalog.iter().all(|hit| hit.plugin.internal_name != plugin),
        "{plugin} in the catalog"
    );
}

#[then(regex = r#"^installing plugin "([^"]+)" from "([^"]+)" is refused as not a video plugin$"#)]
async fn then_install_refused(world: &mut AppWorld, plugin: String, repo: String) {
    let url = world.repo_urls.get(&repo).expect("repo url").clone();
    let hit = repo_service(world)
        .refused_plugins()
        .expect("refused")
        .into_iter()
        .find(|hit| hit.plugin.internal_name == plugin)
        .expect("kept apart as refused");
    let svc = extension_service(world);
    let error = svc.install(&hit.plugin, &url).expect_err("refused");
    assert!(error.to_string().contains("not a video plugin"), "{error}");
    assert!(
        svc.installed().expect("installed").is_empty(),
        "nothing installed"
    );
}

#[then(regex = r#"^provider "([^"]+)" is discovered and untrusted$"#)]
async fn then_provider_discovered_untrusted(world: &mut AppWorld, provider: String) {
    let svc = extension_service(world);
    let installed = svc.installed().expect("installed");
    assert_eq!(installed.len(), 1);
    assert!(!installed[0].trusted);
    assert_eq!(
        *world.gateway.trust_history.lock().expect("trust"),
        vec![false]
    );
    let loaded = svc.load_installed().expect("load_installed");
    assert!(loaded.providers.iter().any(|p| p.name == provider));
}

#[when(regex = r#"^the user marks plugin "([^"]+)" as (trusted|untrusted)$"#)]
async fn when_toggle_trust(world: &mut AppWorld, plugin: String, state: String) {
    let svc = extension_service(world);
    let ext = svc
        .installed()
        .expect("installed")
        .into_iter()
        .find(|e| e.internal_name == plugin)
        .expect("ext found");
    svc.set_trusted(&ext, state == "trusted")
        .expect("set_trusted");
}

#[then(
    regex = r#"^plugin "([^"]+)" is recorded as (trusted|untrusted) and reloaded in (trusted|sandboxed) mode$"#
)]
async fn then_trust_recorded(world: &mut AppWorld, plugin: String, state: String, mode: String) {
    let expected = state == "trusted";
    assert_eq!(expected, mode == "trusted");
    let ext = extension_service(world)
        .installed()
        .expect("installed")
        .into_iter()
        .find(|e| e.internal_name == plugin)
        .expect("ext found");
    assert_eq!(ext.trusted, expected);
    assert_eq!(
        world
            .gateway
            .trust_history
            .lock()
            .expect("trust")
            .last()
            .copied(),
        Some(expected)
    );
}

#[when(regex = r#"^the user uninstalls plugin "([^"]+)"$"#)]
async fn when_uninstall_plugin(world: &mut AppWorld, plugin: String) {
    let svc = extension_service(world);
    let ext = svc
        .installed()
        .expect("installed")
        .into_iter()
        .find(|e| e.internal_name == plugin)
        .expect("ext found");
    svc.uninstall(&ext).expect("uninstall");
}

#[then(regex = r#"^provider "([^"]+)" is no longer offered and no extensions remain installed$"#)]
async fn then_uninstalled_cleanly(world: &mut AppWorld, provider: String) {
    let svc = extension_service(world);
    assert!(svc.installed().expect("installed").is_empty());
    assert!(!svc.providers().iter().any(|p| p.name == provider));
    assert!(!world.gateway.unloaded.lock().expect("unloaded").is_empty());
}

// --- 2. Catalog, details, bookmark & resume ---

#[given(
    regex = r#"^provider "([^"]+)" has home and search title "([^"]+)" with episode "([^"]+)"$"#
)]
async fn given_catalog_title(
    world: &mut AppWorld,
    provider: String,
    title: String,
    ep_key: String,
) {
    let url = format!("https://provider.example/title/{title}");
    let item = SearchResult {
        name: title.clone(),
        url: url.clone(),
        poster_url: None,
        poster_headers: None,
        api_name: provider.clone(),
        media_type: Some("TvSeries".into()),
        year: Some(2025),
    };
    *world.gateway.home_reply.lock().expect("home") = Some(HomePage {
        items: vec![HomePageSection {
            name: "Trending".into(),
            list: vec![item.clone()],
            is_horizontal_images: false,
        }],
        has_next: false,
    });
    *world.gateway.search_reply.lock().expect("search") = SearchPage {
        items: vec![item],
        has_next: false,
    };
    *world.gateway.details_reply.lock().expect("details") = Some(LoadDetails {
        name: title,
        url,
        api_name: provider,
        episodes: vec![EpisodeInfo {
            data: ep_key,
            name: Some("Episode 1".into()),
            season: Some(1),
            episode: Some(1),
            poster_url: None,
            description: None,
        }],
        ..LoadDetails::default()
    });
}

#[when(regex = r#"^the user loads the home page and searches for "([^"]+)" on "([^"]+)"$"#)]
async fn when_home_and_search(world: &mut AppWorld, query: String, provider: String) {
    let catalog =
        CatalogService::new(Arc::clone(&world.gateway) as Arc<dyn extension_api::ProviderGateway>);
    let entry = MainPageEntry {
        name: "Trending".into(),
        data: "trending".into(),
        horizontal_images: false,
    };
    let home = catalog
        .home(&provider, &entry)
        .expect("home")
        .expect("page");
    assert_eq!(home.items[0].list[0].name, query);
    let results = catalog.search(
        &[ProviderInfo {
            name: provider,
            ..ProviderInfo::default()
        }],
        &query,
    );
    assert_eq!(results[0].result.as_ref().expect("ok").items[0].name, query);
}

#[when(regex = r#"^the user opens details for "([^"]+)"$"#)]
async fn when_open_details(world: &mut AppWorld, title: String) {
    let catalog =
        CatalogService::new(Arc::clone(&world.gateway) as Arc<dyn extension_api::ProviderGateway>);
    let url = format!("https://provider.example/title/{title}");
    let details = catalog.details("CineStream", &url).expect("details");
    assert_eq!(details.name, title);
    world.loaded_details = Some(details);
}

#[when(regex = r#"^the user bookmarks "([^"]+)"$"#)]
async fn when_bookmark_title(world: &mut AppWorld, title: String) {
    let details = world.loaded_details.as_ref().expect("details loaded");
    assert_eq!(details.name, title);
    library_service(world)
        .add_bookmark(&Bookmark {
            provider: details.api_name.clone(),
            url: details.url.clone(),
            name: details.name.clone(),
            poster_url: details.poster_url.clone(),
            media_type: details.media_type.clone(),
            added_at_ms: 15_000,
        })
        .expect("add bookmark");
}

#[when(
    regex = r#"^the user watches "([^"]+)" episode "([^"]+)" up to (\d+) ms of (\d+) ms and stops$"#
)]
async fn when_watch_and_stop(
    world: &mut AppWorld,
    title: String,
    ep_key: String,
    pos_ms: i64,
    dur_ms: i64,
) {
    let details = world.loaded_details.as_ref().expect("details").clone();
    let pb = playback_service(world);
    let link = PlayableLink {
        url: "https://cdn.example/video.mp4".into(),
        quality: 1080,
        link_type: "VIDEO".into(),
        ..PlayableLink::default()
    };
    pb.start(
        PlaybackItem {
            provider: details.api_name,
            media_key: ep_key,
            title,
            detail_url: details.url,
            poster_url: None,
            season: Some(1),
            episode: Some(1),
            next_episode: None,
        },
        &link,
        &[],
    )
    .expect("start playback");
    *world.player.status.lock().expect("status") = PlaybackStatus {
        state: PlaybackState::Playing,
        position_ms: pos_ms,
        duration_ms: dur_ms,
        has_video: true,
        ..PlaybackStatus::default()
    };
    pb.stop().expect("stop playback");
}

#[then(regex = r#"^"([^"]+)" is bookmarked in the library$"#)]
async fn then_bookmarked(world: &mut AppWorld, title: String) {
    let details = world.loaded_details.as_ref().expect("details");
    assert_eq!(details.name, title);
    assert!(library_service(world)
        .is_bookmarked(&details.api_name, &details.url)
        .expect("is_bookmarked"));
}

#[then(regex = r#"^continue watching lists "([^"]+)" resuming at (\d+) ms$"#)]
async fn then_continue_watching(world: &mut AppWorld, title: String, expected_pos: i64) {
    let items = library_service(world)
        .continue_watching()
        .expect("continue_watching");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, title);
    assert_eq!(items[0].position_ms, expected_pos);
}

// --- 3. Stream resolution, probing, fallback & watch progress ---

#[given("a local stream server with broken 1080p stream and working 720p stream")]
async fn given_stream_server(world: &mut AppWorld) {
    let server = LocalHttpServer::start(vec![
        ("/broken1080.mp4".into(), 404, b"missing".to_vec()),
        ("/working720.mp4".into(), 200, b"00000000ftypisom".to_vec()),
    ]);
    *world.gateway.links_reply.lock().expect("links") = PlayableLinks {
        success: true,
        links: vec![
            PlayableLink {
                source: "CineStream".into(),
                name: "1080p".into(),
                url: format!("{}/broken1080.mp4", server.base_url),
                quality: 1080,
                link_type: "VIDEO".into(),
                ..PlayableLink::default()
            },
            PlayableLink {
                source: "CineStream".into(),
                name: "720p".into(),
                url: format!("{}/working720.mp4", server.base_url),
                quality: 720,
                link_type: "VIDEO".into(),
                ..PlayableLink::default()
            },
        ],
        subtitles: Vec::new(),
    };
    world.stream_server = Some(server);
}

#[when(regex = r#"^the user opens streams for "([^"]+)" on "([^"]+)"$"#)]
async fn when_open_streams(world: &mut AppWorld, title: String, provider: String) {
    let resolver =
        StreamResolver::new(Arc::clone(&world.gateway) as Arc<dyn extension_api::ProviderGateway>);
    let control = OpenControl::default();
    let opened = run_open(OpenRequest {
        streams: &resolver,
        provider: &provider,
        data: &title,
        media_key: &title,
        cache: None,
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    })
    .expect("run_open succeeds");
    let idx = opened.play_index.expect("working stream chosen");
    world.chosen_link = Some(opened.links[idx].0.clone());
}

#[then("playback picks the working 720p stream")]
async fn then_picked_working_720p(world: &mut AppWorld) {
    let chosen = world.chosen_link.as_ref().expect("chosen link");
    assert_eq!(chosen.quality, 720);
    assert!(chosen.url.ends_with("/working720.mp4"));
}

#[when(regex = r#"^playback advances to (\d+) ms of (\d+) ms and stops$"#)]
async fn when_playback_advances_and_stops(world: &mut AppWorld, pos_ms: i64, dur_ms: i64) {
    let chosen = world.chosen_link.as_ref().expect("chosen link").clone();
    let pb = playback_service(world);
    pb.start(
        PlaybackItem {
            provider: "CineStream".into(),
            media_key: "Dune".into(),
            title: "Dune".into(),
            detail_url: "https://provider.example/dune".into(),
            poster_url: None,
            season: None,
            episode: None,
            next_episode: None,
        },
        &chosen,
        &[],
    )
    .expect("start");
    *world.player.status.lock().expect("status") = PlaybackStatus {
        state: PlaybackState::Playing,
        position_ms: pos_ms,
        duration_ms: dur_ms,
        has_video: true,
        ..PlaybackStatus::default()
    };
    pb.stop().expect("stop");
}

#[then(regex = r#"^watch progress for "([^"]+)" is saved at (\d+) ms$"#)]
async fn then_progress_saved(world: &mut AppWorld, title: String, expected_pos: i64) {
    let items = library_service(world)
        .continue_watching()
        .expect("continue_watching");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, title);
    assert_eq!(items[0].position_ms, expected_pos);
}

// --- 4. Subtitles pipeline ---

#[given(
    "external subtitle tracks for French, gzipped English, UTF-16 Arabic, and an HTML 403 page"
)]
async fn given_subtitle_tracks(world: &mut AppWorld) {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(b"WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello world\n")
        .expect("gz write");
    let en_gz = enc.finish().expect("gz finish");

    let mut ar_utf16 = vec![0xFF, 0xFE];
    for unit in "1\n00:00:01,000 --> 00:00:03,000\nMarhaba\n".encode_utf16() {
        ar_utf16.extend_from_slice(&unit.to_le_bytes());
    }

    let mut files = HashMap::new();
    files.insert(
        "https://subs.example/fr.srt".into(),
        b"1\n00:00:01,000 --> 00:00:02,000\nBonjour\n".to_vec(),
    );
    files.insert("https://subs.example/en.vtt.gz".into(), en_gz);
    files.insert("https://subs.example/ar.srt".into(), ar_utf16);
    files.insert(
        "https://subs.example/broken.srt".into(),
        b"<!DOCTYPE html><html><body>403 Forbidden</body></html>".to_vec(),
    );
    *world.sub_files.lock().expect("sub_files") = files;

    world.sub_tracks = vec![
        SubtitleTrack {
            lang: "French".into(),
            url: "https://subs.example/fr.srt".into(),
            headers: None,
        },
        SubtitleTrack {
            lang: "English".into(),
            url: "https://subs.example/en.vtt.gz".into(),
            headers: None,
        },
        SubtitleTrack {
            lang: "Arabic".into(),
            url: "https://subs.example/ar.srt".into(),
            headers: None,
        },
        SubtitleTrack {
            lang: "English".into(),
            url: "https://subs.example/broken.srt".into(),
            headers: None,
        },
    ];
}

#[when(regex = r#"^the user prefetches subtitles preferring "([^"]+)" then "([^"]+)"$"#)]
async fn when_prefetch_subtitles(world: &mut AppWorld, first: String, second: String) {
    let cache = Arc::new(SubtitleCache::new(
        world.data_dir.as_ref().expect("dir").join("subtitles"),
        Duration::from_secs(3600),
    ));
    let loader = Arc::new(SubtitleLoader::new(
        Arc::clone(&cache),
        Arc::new(MapSubFetcher(Arc::clone(&world.sub_files))),
        LoadOptions {
            attempts: 1,
            backoff: Duration::from_millis(1),
        },
    ));
    let delivered = Arc::new(Mutex::new(Vec::<ReadySubtitle>::new()));
    let delivered_cb = Arc::clone(&delivered);
    let rank_of: subtitle_store::LanguageRank = Arc::new(move |lang: &str| {
        if lang.eq_ignore_ascii_case(&first) {
            0
        } else if lang.eq_ignore_ascii_case(&second) {
            1
        } else {
            SKIP_DOWNLOAD_RANK
        }
    });
    let keep_going: KeepGoing = Arc::new(|| true);
    let (prefetch, handles) = SubtitlePrefetch::start(
        loader,
        &PrefetchConfig {
            workers: 1,
            rank_of,
        },
        Arc::new(move |ready| delivered_cb.lock().expect("delivered").push(ready)),
        keep_going,
    );
    prefetch.submit(world.sub_tracks.clone());
    prefetch.finish();
    for handle in handles {
        handle.join().expect("worker joined");
    }
    let summary = prefetch.summary();
    world.failed_sub_count = summary.failed;
    world.delivered_subs = delivered.lock().expect("delivered").clone();
}

#[then("the HTML error page subtitle is rejected")]
async fn then_html_sub_rejected(world: &mut AppWorld) {
    assert_eq!(world.failed_sub_count, 1);
}

#[then("the English and Arabic subtitles are normalized to UTF-8 local files on disk")]
async fn then_subs_normalized_on_disk(world: &mut AppWorld) {
    assert_eq!(world.delivered_subs.len(), 2);
    for sub in &world.delivered_subs {
        let text = std::fs::read_to_string(&sub.path).expect("valid utf-8 on disk");
        assert!(!text.trim().is_empty());
    }
}

#[then(regex = r#"^the highest priority delivered subtitle language is "([^"]+)"$"#)]
async fn then_first_delivered_lang(world: &mut AppWorld, expected: String) {
    assert_eq!(world.delivered_subs[0].language, expected);
}

// --- 5. Skip-segment merging & next-episode prompts ---

#[given(regex = r#"^an episode of length (\d+) ms with chapter "([^"]+)" from (\d+) to (\d+) ms$"#)]
async fn given_episode_chapter(
    world: &mut AppWorld,
    dur_ms: i64,
    title: String,
    start_ms: i64,
    end_ms: i64,
) {
    world.episode_duration_ms = dur_ms;
    world.chapters = vec![
        Chapter {
            title,
            time_ms: start_ms,
        },
        Chapter {
            title: "Main Act".into(),
            time_ms: end_ms,
        },
    ];
}

#[given(
    regex = r#"^online AniSkip returns intro (\d+) to (\d+) ms and credits (\d+) to (\d+) ms$"#
)]
async fn given_online_aniskip(
    world: &mut AppWorld,
    in_start: u64,
    in_end: u64,
    ed_start: u64,
    ed_end: u64,
) {
    let dur_s = world.episode_duration_ms as u64 / 1000;
    world.online_skip_json = format!(
        r#"{{"found":true,"results":[
            {{"interval":{{"startTime":{},"endTime":{}}},"skipType":"op","episodeLength":{dur_s}}},
            {{"interval":{{"startTime":{},"endTime":{}}},"skipType":"ed","episodeLength":{dur_s}}}
        ]}}"#,
        in_start / 1000,
        in_end / 1000,
        ed_start / 1000,
        ed_end / 1000,
    );
}

#[when("chapter segments and online skip segments are merged")]
async fn when_merge_skips(world: &mut AppWorld) {
    let lookup = SkipLookup::new(Arc::new(StaticSkipFetcher(world.online_skip_json.clone())));
    let online = lookup
        .find(&SkipQuery {
            ids: ExternalIds {
                mal: Some("42".into()),
                imdb: None,
                tmdb: None,
            },
            kind: TitleKind::Anime,
            season: Some(1),
            episode: Some(1),
            duration_ms: world.episode_duration_ms as u64,
        })
        .expect("online skips");

    let mut chapter_segs: Vec<SkipSegment> = Vec::new();
    for (idx, ch) in world.chapters.iter().enumerate() {
        let low = ch.title.to_ascii_lowercase();
        let kind = if low.contains("opening") || low.contains("intro") {
            Some(SkipKind::Intro)
        } else if low.contains("credits") || low.contains("ending") {
            Some(SkipKind::Credits)
        } else {
            None
        };
        if let Some(kind) = kind {
            let end_ms = world
                .chapters
                .get(idx + 1)
                .map_or(world.episode_duration_ms, |next| next.time_ms);
            chapter_segs.push(SkipSegment {
                kind,
                start_ms: ch.time_ms as u64,
                end_ms: end_ms as u64,
            });
        }
    }

    let mut merged = chapter_segs.clone();
    for seg in online {
        if !chapter_segs.iter().any(|c| c.kind == seg.kind) {
            merged.push(seg);
        }
    }
    merged.sort_by_key(|s| s.start_ms);
    world.merged_skips = merged;
}

#[then(regex = r#"^the merged intro uses the chapter timing (\d+) to (\d+) ms$"#)]
async fn then_merged_intro(world: &mut AppWorld, start: u64, end: u64) {
    let intro = world
        .merged_skips
        .iter()
        .find(|s| s.kind == SkipKind::Intro)
        .expect("intro present");
    assert_eq!((intro.start_ms, intro.end_ms), (start, end));
}

#[then(regex = r#"^the merged credits use the online timing (\d+) to (\d+) ms$"#)]
async fn then_merged_credits(world: &mut AppWorld, start: u64, end: u64) {
    let credits = world
        .merged_skips
        .iter()
        .find(|s| s.kind == SkipKind::Credits)
        .expect("credits present");
    assert_eq!((credits.start_ms, credits.end_ms), (start, end));
}

#[then(regex = r#"^at position (\d+) ms the prompt offers "([^"]+)" seeking to (\d+) ms$"#)]
async fn then_prompt_skips(world: &mut AppWorld, pos_ms: i64, label: String, to_ms: i64) {
    assert_eq!(
        world.prompt_at(pos_ms, true),
        Some(MergedPrompt::Skip {
            label: match label.as_str() {
                "Skip Intro" => "Skip Intro",
                "Skip Credits" => "Skip Credits",
                "Skip Recap" => "Skip Recap",
                _ => "Skip Preview",
            },
            to_ms,
        })
    );
}

#[then(regex = r#"^at position (\d+) ms with a next episode the prompt offers "Next episode"$"#)]
async fn then_prompt_next_episode(world: &mut AppWorld, pos_ms: i64) {
    assert_eq!(
        world.prompt_at(pos_ms, true),
        Some(MergedPrompt::NextEpisode)
    );
}
