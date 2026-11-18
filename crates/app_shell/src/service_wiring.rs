use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use app_services::{
    CatalogService, Clock, ExtensionService, LibraryService, PlaybackService, RepositoryService,
    RoutedPlayer, SettingsService, StreamResolver, SystemClock, UiServices,
};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher, UreqFetcher};
use extension_runtime::PluginWorkerPool;
use player_api::VideoPlayer;
use player_mpv::{MpvPlayer, PlayerOptions};
use poster_artwork::{ArtworkLoader, ArtworkLoaderConfig, DiskCache, UreqArtworkFetcher};
use storage::{
    BookmarkStore, Database, InstalledExtensionStore, RepositoryStore, ResumeStore, SettingsStore,
    WatchProgressStore,
};
use title_cache::TitleCache;

use crate::app_paths::AppPaths;
use crate::sidecar_discovery;
use crate::stream_proxy_router::StreamProxyRouter;
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
    let (playback, player_problem) = build_playback(&database, &clock, options);
    let settings = SettingsService::new(SettingsStore::new(Arc::clone(&database)));
    apply_worker_limit(&settings, &database, gateway.as_ref());
    let extensions = ExtensionService::new(
        Arc::clone(&gateway),
        installer,
        InstalledExtensionStore::new(Arc::clone(&database)),
        Arc::clone(&clock),
    );
    Ok(UiServices {
        repositories: RepositoryService::new(
            Arc::clone(&fetcher),
            RepositoryStore::new(Arc::clone(&database)),
            Arc::clone(&clock),
        ),
        extensions,
        catalog: CatalogService::new(Arc::clone(&gateway)),
        streams: Arc::new(StreamResolver::new(gateway)),
        playback,
        player_problem,
        library: LibraryService::new(
            BookmarkStore::new(Arc::clone(&database)),
            WatchProgressStore::new(Arc::clone(&database)),
            ResumeStore::new(Arc::clone(&database)),
        ),
        settings,
        clock,
        title_cache: Arc::new(TitleCache::new(
            paths.data_dir().join("sources"),
            title_cache::CacheLimits::default(),
        )),
        artwork: build_artwork(&paths.data_dir().join("artwork")),
    })
}

/// The poster loader, with downloaded files kept in `dir` (trimmed once at start, off this thread).
fn build_artwork(dir: &Path) -> Arc<ArtworkLoader> {
    let disk = DiskCache::new(dir);
    let pruned = disk.clone();
    let started = std::thread::Builder::new()
        .name("artwork-prune".into())
        .spawn(move || {
            let removed = pruned.prune();
            tracing::info!(removed, "old poster files removed");
        });
    if let Err(error) = started {
        tracing::warn!(%error, "poster files not trimmed this run");
    }
    let config = ArtworkLoaderConfig {
        disk: Some(disk),
        ..ArtworkLoaderConfig::default()
    };
    Arc::new(ArtworkLoader::new(
        Arc::new(UreqArtworkFetcher::default()),
        config,
    ))
}

/// Applies the saved plugin process limit; installs from before the setting keep "unlimited".
fn apply_worker_limit(
    settings: &SettingsService,
    database: &Arc<Database>,
    gateway: &dyn ProviderGateway,
) {
    let had_prior = InstalledExtensionStore::new(Arc::clone(database))
        .list()
        .map(|list| !list.is_empty())
        .unwrap_or(false);
    match settings.plugin_worker_limit_migrating(had_prior) {
        Ok(limit) => gateway.set_resident_worker_limit(limit),
        Err(error) => tracing::warn!(%error, "plugin process limit not read; keeping the default"),
    }
}

fn build_gateway(sidecar_data_dir: &Path) -> Arc<dyn ProviderGateway> {
    match sidecar_discovery::discover() {
        Ok(config) => {
            let config = config
                .with_data_dir(sidecar_data_dir)
                .with_system_trust_store();
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

fn build_playback(
    database: &Arc<Database>,
    clock: &Arc<dyn Clock>,
    options: &ServiceOptions,
) -> (Option<Arc<PlaybackService>>, Option<String>) {
    let player_options = PlayerOptions {
        headless: options.headless_player,
        embedded: !options.headless_player && options.player_wid.is_none(),
        wid: options.player_wid,
    };
    let player: Result<MpvPlayer, String> =
        MpvPlayer::auto_detect(&player_options).map_err(|error| error.to_string());
    match player {
        Ok(player) => {
            let player = route_through_proxy(Arc::new(player), database);
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
