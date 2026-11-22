use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use app_services::{
    CatalogService, Clock, DeferredPlayer, ExtensionService, LibraryService, PlaybackService,
    PlayerFactory, RepositoryService, RoutedPlayer, SettingsService, StreamResolver, SystemClock,
    UiServices,
};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher, UreqFetcher};
use extension_runtime::{JvmProfile, PluginWorkerPool};
use memory_budget::SystemProbe;
use player_api::VideoPlayer;
use player_mpv::{MpvPlayer, PlayerOptions};
use poster_artwork::{ArtworkLoader, ArtworkLoaderConfig, DiskCache, UreqArtworkFetcher};
use skip_times::{SkipLookup, UreqSkipFetcher};
use storage::{
    BookmarkStore, Database, InstalledExtensionStore, RepositoryStore, ResumeStore, SettingsStore,
    WatchProgressStore,
};
use subtitle_store::{LoadOptions, SubtitleCache, SubtitleLoader, UreqSubtitleFetcher};
use title_cache::TitleCache;

use crate::app_paths::AppPaths;
use crate::sidecar_discovery;
use crate::stream_proxy_router::StreamProxyRouter;
use crate::system_probe::SysinfoProbe;
use crate::unavailable_gateway::UnavailableGateway;

/// Choices that differ between the app and tests.
#[derive(Debug, Clone, Default)]
pub struct ServiceOptions {
    /// Play without opening a window or producing sound (tests, CI).
    pub headless_player: bool,
    /// Window ID to attach hardware-accelerated mpv video to directly.
    pub player_wid: Option<i64>,
}

/// Builds every service from the real implementations.
pub fn build_services(paths: &AppPaths, options: &ServiceOptions) -> Result<UiServices> {
    let database =
        Arc::new(Database::open(&paths.database_file()).context("cannot open the database")?);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let fetcher: Arc<dyn HttpFetcher> = Arc::new(UreqFetcher::default());
    let gateway = build_gateway(&paths.data_dir().join("sidecar"));
    let installer = ExtensionInstaller::new(Arc::clone(&fetcher), paths.extensions_dir());
    let gl_video = Arc::new(AtomicBool::new(video_renderer_is_gpu(
        std::env::var("CS_VIDEO_RENDERER").ok().as_deref(),
    )));
    let (playback, player_problem) = build_playback(&database, &clock, options, &gl_video);
    let system = SysinfoProbe.facts();
    tracing::info!(
        total_mib = system.total_mib,
        cores = system.cpu_cores,
        "this PC"
    );
    let settings =
        SettingsService::new(SettingsStore::new(Arc::clone(&database))).with_system(system);
    apply_worker_limit(&settings, gateway.as_ref());
    let extensions = ExtensionService::new(
        Arc::clone(&gateway),
        installer,
        InstalledExtensionStore::new(Arc::clone(&database)),
        Arc::clone(&clock),
    );
    Ok(assemble_services(AssembleParts {
        paths,
        database,
        clock,
        fetcher,
        gateway,
        extensions,
        settings,
        playback,
        player_problem,
        gl_video,
    }))
}

struct AssembleParts<'a> {
    paths: &'a AppPaths,
    database: Arc<Database>,
    clock: Arc<dyn Clock>,
    fetcher: Arc<dyn HttpFetcher>,
    gateway: Arc<dyn ProviderGateway>,
    extensions: ExtensionService,
    settings: SettingsService,
    playback: Option<Arc<PlaybackService>>,
    player_problem: Option<String>,
    gl_video: Arc<AtomicBool>,
}

fn assemble_services(parts: AssembleParts<'_>) -> UiServices {
    let data = parts.paths.data_dir();
    let subtitles = Arc::new(SubtitleCache::new(
        data.join("subtitles"),
        std::time::Duration::from_secs(7 * 24 * 60 * 60),
    ));
    let subtitle_loader = Arc::new(SubtitleLoader::new(
        Arc::clone(&subtitles),
        Arc::new(UreqSubtitleFetcher::default()),
        LoadOptions::default(),
    ));
    let title_cache = TitleCache::new(data.join("sources"), title_cache::CacheLimits::default());
    if let Ok(valid_from) = parts.settings.sources_valid_from() {
        title_cache.set_valid_from(valid_from);
    }
    UiServices {
        repositories: RepositoryService::new(
            Arc::clone(&parts.fetcher),
            RepositoryStore::new(Arc::clone(&parts.database)),
            Arc::clone(&parts.clock),
        ),
        extensions: parts.extensions,
        catalog: CatalogService::new(Arc::clone(&parts.gateway)),
        streams: Arc::new(StreamResolver::new(parts.gateway)),
        playback: parts.playback,
        player_problem: parts.player_problem,
        library: LibraryService::new(
            BookmarkStore::new(Arc::clone(&parts.database)),
            WatchProgressStore::new(Arc::clone(&parts.database)),
            ResumeStore::new(Arc::clone(&parts.database)),
        ),
        settings: parts.settings,
        clock: parts.clock,
        title_cache: Arc::new(title_cache),
        subtitles,
        subtitle_loader,
        artwork: build_artwork(&data.join("artwork")),
        skips: Arc::new(SkipLookup::new(Arc::new(UreqSkipFetcher::default()))),
        gl_video: parts.gl_video,
    }
}

/// Decoded posters the loader itself keeps (hero and details art, repeated asks while one loads).
/// The tiles on screen are kept by the UI's picture store, once per address; a large cache here
/// would hold every tile a second time.
const ARTWORK_LOADER_MEMORY_BYTES: usize = 16 * 1024 * 1024;

/// The poster loader, with downloaded files kept in `dir` (trimmed by the post-Home cleanup worker).
fn build_artwork(dir: &Path) -> Arc<ArtworkLoader> {
    let config = ArtworkLoaderConfig {
        disk: Some(DiskCache::new(dir)),
        memory_budget_bytes: ARTWORK_LOADER_MEMORY_BYTES,
        ..ArtworkLoaderConfig::default()
    };
    Arc::new(ArtworkLoader::new(
        Arc::new(UreqArtworkFetcher::default()),
        config,
    ))
}

/// Applies the plugin process limit in force: the viewer's choice, else the profile's. The first
/// start with profiles drops the limits the old app wrote itself (see `migrate_performance_settings`).
fn apply_worker_limit(settings: &SettingsService, gateway: &dyn ProviderGateway) {
    if let Err(error) = settings.migrate_performance_settings() {
        tracing::warn!(%error, "performance settings were not migrated");
    }
    match settings.plugin_worker_limit() {
        Ok(limit) => gateway.set_resident_worker_limit(limit),
        Err(error) => tracing::warn!(%error, "plugin process limit not read; keeping the default"),
    }
}

/// The plugin JVM profile: lean unless `CS_JVM_PROFILE=legacy` asks for the JVM's own defaults
/// (the way back if a plugin does not run lean).
fn jvm_profile(setting: Option<&str>) -> JvmProfile {
    let Some(text) = setting else {
        return JvmProfile::default();
    };
    JvmProfile::parse(text).unwrap_or_else(|| {
        tracing::warn!(
            setting = text,
            "unknown CS_JVM_PROFILE; using the lean profile"
        );
        JvmProfile::default()
    })
}

/// Whether the viewer asked for zero-copy video: `CS_VIDEO_RENDERER=gpu` (anything else is the
/// software path, which stays the default until the GPU path is verified on real machines).
fn video_renderer_is_gpu(setting: Option<&str>) -> bool {
    setting.is_some_and(|text| text.trim().eq_ignore_ascii_case("gpu"))
}

fn build_gateway(sidecar_data_dir: &Path) -> Arc<dyn ProviderGateway> {
    match sidecar_discovery::discover() {
        Ok(config) => {
            let config = config
                .with_data_dir(sidecar_data_dir)
                .with_system_trust_store()
                .with_jvm_profile(jvm_profile(std::env::var("CS_JVM_PROFILE").ok().as_deref()));
            tracing::info!(
                program = %config.program.display(),
                "using JVM sidecar (one process per plugin)"
            );
            Arc::new(PluginWorkerPool::new(config))
        }
        Err(error) => {
            tracing::warn!(%error, "extension runtime not available");
            Arc::new(UnavailableGateway::new(error.to_string()))
        }
    }
}

/// Wraps the player so that HLS streams go through the local stream proxy while the viewer's
/// setting allows it; the setting is read again before every stream.
fn route_through_proxy(
    player: Arc<dyn VideoPlayer>,
    database: &Arc<Database>,
) -> Arc<dyn VideoPlayer> {
    let settings = SettingsService::new(SettingsStore::new(Arc::clone(database)));
    let router = StreamProxyRouter::new(move || settings.stream_proxy().unwrap_or(true));
    Arc::new(RoutedPlayer::new(player, Arc::new(router)))
}

/// A player that is built when the first video plays, drawing into the window's OpenGL context
/// when `gl_video` says so by then (the window clears it at start-up when it has no OpenGL).
fn deferred_player(
    player_options: PlayerOptions,
    gl_video: &Arc<AtomicBool>,
) -> Arc<dyn VideoPlayer> {
    let embedded = player_options.embedded;
    let gl_video = Arc::clone(gl_video);
    let factory: PlayerFactory = Box::new(move || {
        let options = PlayerOptions {
            gl_output: gl_video.load(Ordering::SeqCst),
            ..player_options.clone()
        };
        let player: Arc<dyn VideoPlayer> = Arc::new(MpvPlayer::auto_detect(&options)?);
        Ok(player)
    });
    Arc::new(DeferredPlayer::new(embedded, factory))
}

fn build_playback(
    database: &Arc<Database>,
    clock: &Arc<dyn Clock>,
    options: &ServiceOptions,
    gl_video: &Arc<AtomicBool>,
) -> (Option<Arc<PlaybackService>>, Option<String>) {
    let player_options = PlayerOptions {
        headless: options.headless_player,
        embedded: !options.headless_player && options.player_wid.is_none(),
        wid: options.player_wid,
        gl_output: false, // set per video from `gl_video`, below
    };
    // libmpv is loaded and initialized when the first video plays (about 40 MiB saved while the
    // viewer browses); the probe keeps a missing library a start-up message, as before.
    let player: Result<Arc<dyn VideoPlayer>, String> = player_mpv::probe_libmpv()
        .map(|()| deferred_player(player_options, gl_video))
        .map_err(|error| error.to_string());
    match player {
        Ok(player) => {
            let player = route_through_proxy(player, database);
            let progress = WatchProgressStore::new(Arc::clone(database));
            let resume = ResumeStore::new(Arc::clone(database));
            (
                Some(Arc::new(PlaybackService::new(
                    player,
                    progress,
                    resume,
                    Arc::clone(clock),
                ))),
                None,
            )
        }
        Err(problem) => {
            tracing::warn!(%problem, "playback unavailable");
            (None, Some(problem))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_video_renderer_is_software_unless_gpu_is_asked_for() {
        assert!(!video_renderer_is_gpu(None));
        assert!(!video_renderer_is_gpu(Some("cpu")));
        assert!(video_renderer_is_gpu(Some(" GPU ")));
    }

    #[test]
    fn the_jvm_profile_is_lean_unless_legacy_is_asked_for() {
        assert_eq!(jvm_profile(None), JvmProfile::Lean);
        assert_eq!(jvm_profile(Some("legacy")), JvmProfile::Legacy);
        assert_eq!(jvm_profile(Some("LEAN")), JvmProfile::Lean);
        assert_eq!(
            jvm_profile(Some("turbo")),
            JvmProfile::Lean,
            "unknown falls back"
        );
    }
}
