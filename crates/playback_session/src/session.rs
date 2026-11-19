//! Live open session: skip remaining checks and refresh sources while a video plays.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use app_services::StreamResolver;
use core_model::{PlayableLink, SubtitleTrack};

use crate::checked_open::{open_with, open_with_playing, OpenTarget};
use crate::open_types::{title_key, CheckedOpen, OpenCache, OpenError, OpenHooks, OpenProgress};
use title_cache::TitleCache;

/// Shared control flags for an in-flight open.
#[derive(Clone, Default)]
pub struct OpenControl {
    /// Skip remaining checks once a working stream exists.
    pub skip: Arc<AtomicBool>,
    /// Cancel the whole open.
    pub cancel: Arc<AtomicBool>,
}

impl OpenControl {
    /// Ask the open loop to stop checking and return with the best working stream.
    pub fn skip_checks(&self) {
        self.skip.store(true, Ordering::SeqCst);
    }

    /// Abandon the open.
    pub fn cancel_open(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Latest snapshot published by a live open / refresh.
#[derive(Clone, Default)]
pub struct LiveSources {
    inner: Arc<Mutex<CheckedOpen>>,
}

impl LiveSources {
    /// Empty list.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CheckedOpen {
                links: Vec::new(),
                play_index: None,
                subtitles: Vec::new(),
            })),
        }
    }

    /// Replace the snapshot.
    pub fn publish(&self, open: CheckedOpen) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = open;
        }
    }

    /// Copy of the current list.
    pub fn snapshot(&self) -> CheckedOpen {
        self.inner.lock().map(|g| g.clone()).unwrap_or(CheckedOpen {
            links: Vec::new(),
            play_index: None,
            subtitles: Vec::new(),
        })
    }

    /// Links only (for UI source menus).
    pub fn links(&self) -> Vec<PlayableLink> {
        self.snapshot()
            .links
            .into_iter()
            .map(|(link, _)| link)
            .collect()
    }

    /// Subtitles gathered so far.
    pub fn subtitles(&self) -> Vec<SubtitleTrack> {
        self.snapshot().subtitles
    }
}

/// Arguments for [`run_open`].
pub struct OpenRequest<'a> {
    /// Stream resolver.
    pub streams: &'a StreamResolver,
    /// Provider internal name.
    pub provider: &'a str,
    /// Episode / movie data payload for `load_links` / link search.
    pub data: &'a str,
    /// Media key for the title cache.
    pub media_key: &'a str,
    /// Optional disk cache of prior sources.
    pub cache: Option<&'a Arc<TitleCache>>,
    /// Progress / update / ready hooks.
    pub on_progress: &'a (dyn Fn(OpenProgress) + Sync),
    /// Full snapshot after each batch or check.
    pub on_update: &'a (dyn Fn(&CheckedOpen) + Sync),
    /// First working stream index.
    pub on_ready: &'a (dyn Fn(usize) + Sync),
    /// Skip / cancel flags.
    pub control: &'a OpenControl,
}

/// Runs a full open with live updates. Sources stay in memory until the UI saves on leave/close.
pub fn run_open(request: OpenRequest<'_>) -> Result<CheckedOpen, OpenError> {
    let open_cache = request.cache.map(|cache| OpenCache {
        cache: Arc::clone(cache),
        title_key: title_key(request.provider, request.media_key),
    });
    let hooks = OpenHooks {
        on_progress: request.on_progress,
        on_update: request.on_update,
        on_ready: request.on_ready,
        skip: &request.control.skip,
        cancel: &request.control.cancel,
    };
    let target = OpenTarget {
        streams: request.streams,
        provider: request.provider,
        data: request.data,
    };
    open_with(&target, open_cache.as_ref(), &hooks)
}

/// Arguments for [`refresh_sources`].
pub struct RefreshRequest<'a> {
    /// Stream resolver.
    pub streams: &'a StreamResolver,
    /// Provider internal name.
    pub provider: &'a str,
    /// Episode / movie data payload.
    pub data: &'a str,
    /// Media key for the title cache.
    pub media_key: &'a str,
    /// Stream currently playing (kept in the refreshed list as working so playback continues).
    pub playing_link: Option<&'a PlayableLink>,
    /// Optional disk cache.
    pub cache: Option<&'a Arc<TitleCache>>,
    /// Live source list to publish into.
    pub live: &'a LiveSources,
    /// Skip / cancel flags.
    pub control: &'a OpenControl,
    /// Progress callback.
    pub on_progress: &'a (dyn Fn(OpenProgress) + Sync),
    /// Incremental snapshot callback as refreshed sources and subtitles arrive.
    pub on_update: &'a (dyn Fn(&CheckedOpen) + Sync),
}

/// Refresh sources for the playing title: forgets saved links and runs a new search.
///
/// The currently playing stream stays in the list as working so playback is not interrupted, and
/// live incremental updates are forwarded as fresh sources and subtitles arrive.
pub fn refresh_sources(request: RefreshRequest<'_>) -> Result<CheckedOpen, OpenError> {
    request.control.skip.store(false, Ordering::SeqCst);
    request.control.cancel.store(false, Ordering::SeqCst);
    if let Some(cache) = request.cache {
        let key = title_key(request.provider, request.media_key);
        let _ = cache.forget(&key);
    }
    let playing_url = request.playing_link.map(|link| link.url.as_str());
    let live = request.live;
    let notify = request.on_update;
    let on_update = |open: &CheckedOpen| {
        let merged = pin_playing_index(open, playing_url);
        live.publish(merged.clone());
        notify(&merged);
    };
    let on_ready = |_index: usize| {};
    let hooks = OpenHooks {
        on_progress: request.on_progress,
        on_update: &on_update,
        on_ready: &on_ready,
        skip: &request.control.skip,
        cancel: &request.control.cancel,
    };
    let target = OpenTarget {
        streams: request.streams,
        provider: request.provider,
        data: request.data,
    };
    let opened = open_with_playing(&target, None, request.playing_link, &hooks)?;
    let pinned = pin_playing_index(&opened, playing_url);
    request.live.publish(pinned.clone());
    Ok(pinned)
}

fn pin_playing_index(open: &CheckedOpen, playing_url: Option<&str>) -> CheckedOpen {
    let mut merged = open.clone();
    if let Some(url) = playing_url {
        if let Some(index) = merged.links.iter().position(|(link, _)| link.url == url) {
            merged.play_index = Some(index);
        }
    }
    merged
}
