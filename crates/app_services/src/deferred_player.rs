//! A player that is only built when the first video plays (plan step 2f, `docs/MEMORY.md`).
//!
//! libmpv and the codec libraries it pulls in cost about 40 MiB as soon as they are loaded and
//! initialized, and most sessions spend a while on Home before anything plays. The app builds the
//! real player on the first `play` instead of at start-up. Until then questions get idle answers,
//! and what the app tells the player (volume, cache limits, preferred languages, picture size, the
//! verbose log) is remembered and applied, once, just before the first video starts.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use player_api::{
    CacheLimits, Chapter, Language, MediaTrack, PlaybackRequest, PlaybackStatus, PlayerError,
    SubtitleSource, TrackKind, VideoFrame, VideoPlayer,
};

/// Builds the real player. Called on the first play, again after a failure.
pub type PlayerFactory = Box<dyn Fn() -> Result<Arc<dyn VideoPlayer>, PlayerError> + Send + Sync>;

/// What was told to the player before it existed; the latest value of each setting.
#[derive(Default)]
struct Remembered {
    surface: Option<(u32, u32)>,
    volume: Option<u8>,
    cache_limits: Option<CacheLimits>,
    preferred_audio: Option<Option<Language>>,
    preferred_subtitles: Option<Vec<Language>>,
    verbose_log: Option<bool>,
}

enum State {
    Waiting(Remembered),
    Ready(Arc<dyn VideoPlayer>),
}

/// A [`VideoPlayer`] that builds the player behind it on the first [`VideoPlayer::play`].
pub struct DeferredPlayer {
    embedded: bool,
    factory: PlayerFactory,
    state: Mutex<State>,
}

impl DeferredPlayer {
    /// `embedded` is what [`VideoPlayer::is_embedded`] answers before there is a player.
    pub fn new(embedded: bool, factory: PlayerFactory) -> Self {
        Self {
            embedded,
            factory,
            state: Mutex::new(State::Waiting(Remembered::default())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The real player, once there is one.
    fn ready(&self) -> Option<Arc<dyn VideoPlayer>> {
        match &*self.lock() {
            State::Ready(player) => Some(Arc::clone(player)),
            State::Waiting(_) => None,
        }
    }

    /// Records a setting while waiting, or hands back the player to apply it to. One lock covers
    /// both: a setting made while the player starts is either replayed or applied, never lost.
    fn remember_or_player(
        &self,
        remember: impl FnOnce(&mut Remembered),
    ) -> Option<Arc<dyn VideoPlayer>> {
        match &mut *self.lock() {
            State::Waiting(remembered) => {
                remember(remembered);
                None
            }
            State::Ready(player) => Some(Arc::clone(player)),
        }
    }

    /// The real player, built now if this is the first call. Everyone else waits on the lock, so
    /// the factory runs once however many plays arrive together.
    fn ready_or_start(&self) -> Result<Arc<dyn VideoPlayer>, PlayerError> {
        let mut state = self.lock();
        let remembered = match &mut *state {
            State::Ready(player) => return Ok(Arc::clone(player)),
            State::Waiting(remembered) => std::mem::take(remembered),
        };
        tracing::info!("starting the video player for the first video");
        match (self.factory)() {
            Ok(player) => {
                apply(&*player, &remembered);
                *state = State::Ready(Arc::clone(&player));
                Ok(player)
            }
            Err(error) => {
                *state = State::Waiting(remembered); // kept for the next try
                Err(error)
            }
        }
    }
}

/// Tells a new player everything it was told before it existed.
fn apply(player: &dyn VideoPlayer, remembered: &Remembered) {
    if let Some((width, height)) = remembered.surface {
        player.set_surface_size(width, height);
    }
    if let Some(percent) = remembered.volume {
        if let Err(error) = player.set_volume(percent) {
            tracing::warn!(%error, "the volume was not applied to the new player");
        }
    }
    if let Some(limits) = remembered.cache_limits {
        if let Err(error) = player.set_cache_limits(limits) {
            tracing::warn!(%error, "the cache limits were not applied to the new player");
        }
    }
    if let Some(language) = &remembered.preferred_audio {
        player.set_preferred_audio(*language);
    }
    if let Some(languages) = &remembered.preferred_subtitles {
        player.set_preferred_subtitles(languages);
    }
    if let Some(enabled) = remembered.verbose_log {
        player.set_verbose_log(enabled);
    }
}

impl VideoPlayer for DeferredPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        self.ready_or_start()?.play(request)
    }

    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        self.ready()
            .map_or(Ok(()), |player| player.set_paused(paused))
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.ready()
            .map_or(Ok(()), |player| player.seek_to(position_ms))
    }

    fn stop(&self) -> Result<(), PlayerError> {
        self.ready().map_or(Ok(()), |player| player.stop())
    }

    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        match self.remember_or_player(|remembered| remembered.volume = Some(percent)) {
            Some(player) => player.set_volume(percent),
            None => Ok(()),
        }
    }

    fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), PlayerError> {
        match self.remember_or_player(|remembered| remembered.cache_limits = Some(limits)) {
            Some(player) => player.set_cache_limits(limits),
            None => Ok(()),
        }
    }

    fn status(&self) -> PlaybackStatus {
        self.ready()
            .map_or_else(PlaybackStatus::default, |player| player.status())
    }

    fn tracks(&self) -> Vec<MediaTrack> {
        self.ready().map_or_else(Vec::new, |player| player.tracks())
    }

    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        self.ready()
            .map_or(Ok(()), |player| player.select_track(kind, id))
    }

    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        if let Some(player) = self.ready() {
            player.add_subtitles(subtitles);
        }
    }

    fn pending_subtitles(&self) -> usize {
        self.ready().map_or(0, |player| player.pending_subtitles())
    }

    fn set_preferred_audio(&self, language: Option<Language>) {
        let remembered = language;
        if let Some(player) =
            self.remember_or_player(|state| state.preferred_audio = Some(remembered))
        {
            player.set_preferred_audio(language);
        }
    }

    fn set_verbose_log(&self, enabled: bool) {
        if let Some(player) =
            self.remember_or_player(|remembered| remembered.verbose_log = Some(enabled))
        {
            player.set_verbose_log(enabled);
        }
    }

    fn set_preferred_subtitles(&self, languages: &[Language]) {
        let remembered = languages.to_vec();
        if let Some(player) =
            self.remember_or_player(|state| state.preferred_subtitles = Some(remembered))
        {
            player.set_preferred_subtitles(languages);
        }
    }

    fn chapters(&self) -> Vec<Chapter> {
        self.ready()
            .map_or_else(Vec::new, |player| player.chapters())
    }

    fn is_embedded(&self) -> bool {
        self.embedded
    }

    fn set_surface_size(&self, width: u32, height: u32) {
        if let Some(player) =
            self.remember_or_player(|remembered| remembered.surface = Some((width, height)))
        {
            player.set_surface_size(width, height);
        }
    }

    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        self.ready()
            .and_then(|player| player.latest_frame(last_seen_serial))
    }
}

#[cfg(test)]
#[path = "deferred_player_tests.rs"]
mod tests;
