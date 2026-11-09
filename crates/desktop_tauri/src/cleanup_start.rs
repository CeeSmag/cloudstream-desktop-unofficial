//! Putting the app's cleanups into the queue (`cleanup_queue`) and starting it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use player_api::PlaybackState;
use tauri::{AppHandle, Manager};
use title_cache::TitleCache;

use crate::app_state::AppState;
use crate::cleanup_queue::{spawn, Queue, Task, World};
use crate::cleanup_tasks::{
    browser_cache, old_files, old_program_folders, trim_main_log, BROWSER_CACHE_MAX_BYTES,
    EXCHANGE_MAX_AGE, OLD_LOG_MAX_AGE,
};
use crate::startup::StartupTracker;
use crate::storage_cap::{check_once, running_folder};

const HOURLY: Duration = Duration::from_secs(60 * 60);
const EVERY_SIX_HOURS: Duration = Duration::from_secs(6 * 60 * 60);

/// What the app has to clean, and where.
pub struct Cleanup {
    pub data_dir: PathBuf,
    pub log_file: PathBuf,
    pub sources: Arc<TitleCache>,
    pub subtitles: Arc<subtitle_store::SubtitleLoader>,
}

/// The app as the queue sees it.
struct AppWorld(AppHandle);

impl World for AppWorld {
    fn ready(&self) -> bool {
        self.0
            .try_state::<StartupTracker>()
            .is_some_and(|tracker| tracker.snapshot().ready)
    }

    fn busy(&self) -> bool {
        let Some(state) = self.0.try_state::<AppState>() else {
            return false;
        };
        let services = state.read();
        services.playback.as_ref().is_some_and(|playback| {
            matches!(
                playback.tick().state,
                PlaybackState::Playing | PlaybackState::Loading
            )
        })
    }
}

/// Fills the queue and starts it; nothing runs until the app has fully started.
pub fn start(app: &AppHandle, cleanup: Cleanup) {
    let mut queue = Queue::default();
    let running = running_folder(&cleanup.data_dir);
    let data = cleanup.data_dir.clone();
    queue.add(Task::once("older program folders", move || {
        old_program_folders(&data, running.as_deref());
    }));
    let exchange = cleanup.data_dir.join("sidecar").join("cloudflare");
    queue.add(Task::every("old exchange files", HOURLY, move || {
        old_files(&exchange, EXCHANGE_MAX_AGE, |_| true);
    }));
    let sources = cleanup.sources;
    queue.add(Task::every("saved stream sources", HOURLY, move || {
        let report = sources.prune();
        tracing::info!(?report, "saved stream sources pruned");
    }));
    let subtitles = cleanup.subtitles;
    queue.add(Task::every("subtitles", EVERY_SIX_HOURS, move || {
        let removed = subtitles.cache().prune();
        tracing::info!(removed, "old subtitle files removed");
    }));
    let log = cleanup.log_file;
    queue.add(Task::every("main log", HOURLY, move || trim_main_log(&log)));
    let debug_logs = cleanup.data_dir.join("logs").join("debug");
    queue.add(Task::every(
        "rotated debug logs",
        EVERY_SIX_HOURS,
        move || {
            old_files(&debug_logs, OLD_LOG_MAX_AGE, |name| name.ends_with(".old"));
        },
    ));
    add_heavy(&mut queue, app, &cleanup.data_dir);
    spawn(queue, AppWorld(app.clone()));
}

/// The tasks that look through the whole data folder.
fn add_heavy(queue: &mut Queue, app: &AppHandle, data_dir: &Path) {
    let data = data_dir.to_path_buf();
    queue.add(Task::every(
        "check browser cache",
        EVERY_SIX_HOURS,
        move || {
            browser_cache(&data, BROWSER_CACHE_MAX_BYTES);
        },
    ));
    let (app, data) = (app.clone(), data_dir.to_path_buf());
    queue.add(Task::every("storage limit", HOURLY, move || {
        check_once(&app, &data);
    }));
}
