use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use app_services::{
    CatalogService, Clock, ExtensionService, LibraryService, PlaybackService, RepositoryService,
    SettingsService, StreamResolver, SystemClock,
};
use extension_api::ProviderGateway;
use extension_repo::{ExtensionInstaller, HttpFetcher, UreqFetcher};
use extension_runtime::{RpcProviderGateway, SidecarProcessTransport};
use player_api::VideoPlayer;
use player_mpv::{MpvPlayer, PlayerOptions};
use storage::{
    BookmarkStore, Database, InstalledExtensionStore, RepositoryStore, ResumeStore, SettingsStore,
    WatchProgressStore,
};
use ui_app::UiServices;

use crate::app_paths::AppPaths;
use crate::sidecar_discovery;
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
    Ok(UiServices {
        repositories: RepositoryService::new(
            Arc::clone(&fetcher),
            RepositoryStore::new(Arc::clone(&database)),
            Arc::clone(&clock),
        ),
        extensions: ExtensionService::new(
            Arc::clone(&gateway),
            installer,
            InstalledExtensionStore::new(Arc::clone(&database)),
        ),
        catalog: CatalogService::new(Arc::clone(&gateway)),
        streams: Arc::new(StreamResolver::new(gateway)),
        playback,
        player_problem,
        library: LibraryService::new(
            BookmarkStore::new(Arc::clone(&database)),
            WatchProgressStore::new(Arc::clone(&database)),
            ResumeStore::new(Arc::clone(&database)),
        ),
        settings: SettingsService::new(SettingsStore::new(database)),
        clock,
    })
}

fn build_gateway(sidecar_data_dir: &Path) -> Arc<dyn ProviderGateway> {
    match sidecar_discovery::discover() {
        Ok(config) => {
            let config = config
                .with_data_dir(sidecar_data_dir)
                .with_system_trust_store();
            tracing::info!(program = %config.program.display(), "using JVM sidecar");
            Arc::new(RpcProviderGateway::new(SidecarProcessTransport::new(
                config,
            )))
        }
        Err(error) => {
            tracing::warn!(%error, "extension runtime not available");
            Arc::new(UnavailableGateway::new(error.to_string()))
        }
    }
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
            let player: Arc<dyn VideoPlayer> = Arc::new(player);
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
