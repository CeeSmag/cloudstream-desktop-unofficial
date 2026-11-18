//! Background task runner that marshals results onto the Slint event loop.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slint::Weak;

use crate::CloudStream;

/// Events produced by background work.
#[derive(Debug)]
pub(crate) enum UiEvent {
    StartupProgress {
        finished: usize,
        total: usize,
        current: String,
    },
    StartupDone {
        providers: Vec<String>,
        error: Option<String>,
    },
    HomeLoaded(Vec<core_model::SearchResult>),
    SearchLoaded(Vec<core_model::SearchResult>),
    BrowseLoaded(Vec<BrowseRow>),
    DetailsLoaded {
        title: String,
        plot: String,
        meta: String,
        resume: String,
        episodes: Vec<(String, String, String, bool)>,
        provider: String,
        url: String,
        data: String,
        media_key: String,
    },
    DetailsFailed(String),
    OpenProgress {
        found: usize,
        checked: usize,
        working: usize,
        search_done: bool,
    },
    OpenReady {
        index: usize,
    },
    OpenUpdate {
        links: Vec<(core_model::PlayableLink, Option<app_services::LinkHealth>)>,
        play_index: Option<usize>,
        subtitles: Vec<core_model::SubtitleTrack>,
    },
    OpenFailed(String),
    Notice(String),
    PlaybackTick {
        position_ms: f32,
        duration_ms: f32,
        paused: bool,
    },
    CloudflareNeeded {
        host: String,
        url: String,
        id: String,
    },
    UpdateLabel(String),
    StorageLabel(String),
}

#[derive(Debug, Clone)]
pub(crate) struct BrowseRow {
    pub title: String,
    pub subtitle: String,
    pub year: Option<i32>,
}

/// Channels results from worker threads into the UI.
pub(crate) struct EventPump {
    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
}

impl EventPump {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx }
    }

    pub(crate) fn sender(&self) -> Sender<UiEvent> {
        self.tx.clone()
    }

    pub(crate) fn drain(&self) -> Vec<UiEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            out.push(event);
        }
        out
    }
}

/// Spawn work and ignore join handle (fire-and-forget).
pub(crate) fn spawn_work(name: &'static str, work: impl FnOnce() + Send + 'static) {
    let _ = thread::Builder::new()
        .name(name.into())
        .spawn(work);
}

/// Ask Slint to wake so the timer can drain events.
pub(crate) fn wake(ui: &Weak<CloudStream>) {
    let _ = ui.upgrade_in_event_loop(|ui| {
        let _ = ui;
    });
}
