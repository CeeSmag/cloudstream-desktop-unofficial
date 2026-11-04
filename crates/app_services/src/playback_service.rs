use std::sync::{Arc, Mutex, PoisonError};

use core_model::{PlayableLink, ResumePointer, SubtitleTrack, WatchProgress};
use player_api::{PlaybackState, PlaybackStatus, VideoPlayer};
use storage::{ResumeStore, WatchProgressStore};

use crate::playback_types::{build_request, subtitle_sources};
use crate::{Clock, ServiceError};

#[allow(unused_imports)]
pub use crate::playback_types::{NextEpisode, PlaybackItem, StreamSelection};

/// Progress is saved when the position moved at least this far since the last save.
const SAVE_EVERY_MS: i64 = 5_000;

/// Percentage of video watched to advance to the next episode (or mark finished).
pub const NEXT_WATCH_EPISODE_PERCENTAGE: f64 = 0.90;

/// Returns whether playback reached the completion threshold to advance to the next episode.
pub fn is_over_watch_threshold(position_ms: i64, duration_ms: i64) -> bool {
    if duration_ms <= 0 {
        return false;
    }
    let fraction = (position_ms as f64) / (duration_ms as f64);
    fraction >= NEXT_WATCH_EPISODE_PERCENTAGE
}

struct ActivePlayback {
    item: PlaybackItem,
    last_saved_ms: i64,
}

/// Starts playback, resumes where the user left off and keeps their progress.
pub struct PlaybackService {
    player: Arc<dyn VideoPlayer>,
    progress: WatchProgressStore,
    resume: ResumeStore,
    clock: Arc<dyn Clock>,
    active: Mutex<Option<ActivePlayback>>,
}

impl PlaybackService {
    /// Creates the service.
    pub fn new(
        player: Arc<dyn VideoPlayer>,
        progress: WatchProgressStore,
        resume: ResumeStore,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            player,
            progress,
            resume,
            clock,
            active: Mutex::new(None),
        }
    }

    /// Where playback of `item` should resume, in milliseconds (0 when there is nothing to resume).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn resume_position_ms(&self, item: &PlaybackItem) -> Result<i64, ServiceError> {
        let saved = self.progress.get(&item.provider, &item.media_key)?;
        Ok(saved
            .filter(|e| !is_over_watch_threshold(e.position_ms, e.duration_ms))
            .map_or(0, |entry| entry.position_ms))
    }

    /// Plays `link`, resuming from saved progress.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn start(
        &self,
        item: PlaybackItem,
        link: &PlayableLink,
        subtitles: &[SubtitleTrack],
    ) -> Result<(), ServiceError> {
        let start_position_ms = self.resume_position_ms(&item)?;
        self.start_at(
            item,
            &StreamSelection {
                link,
                subtitles,
                start_position_ms,
            },
        )
    }

    /// Plays `link` from an explicit position, for example after switching to another source.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn start_at(
        &self,
        item: PlaybackItem,
        stream: &StreamSelection<'_>,
    ) -> Result<(), ServiceError> {
        let start_position_ms = stream.start_position_ms;
        let request = build_request(
            &item.title,
            stream.link,
            stream.subtitles,
            start_position_ms,
        );
        self.player.play(&request)?;
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = Some(ActivePlayback {
            item,
            last_saved_ms: start_position_ms,
        });
        Ok(())
    }

    /// Offers subtitles the provider found after the stream started.
    #[tracing::instrument(level = "debug", skip_all, fields(count = subtitles.len()))]
    pub fn add_subtitles(&self, subtitles: &[SubtitleTrack]) {
        self.player.add_subtitles(&subtitle_sources(subtitles));
    }

    /// Pauses or resumes the current stream.
    #[tracing::instrument(level = "debug", skip_all, fields(paused = ?paused), err)]
    pub fn set_paused(&self, paused: bool) -> Result<(), ServiceError> {
        Ok(self.player.set_paused(paused)?)
    }

    /// Seeks to an absolute position in milliseconds.
    #[tracing::instrument(level = "debug", skip_all, fields(position_ms = ?position_ms), err)]
    pub fn seek_to(&self, position_ms: i64) -> Result<(), ServiceError> {
        Ok(self.player.seek_to(position_ms)?)
    }

    /// Sets the output volume as a percentage.
    #[tracing::instrument(level = "debug", skip_all, fields(percent = ?percent), err)]
    pub fn set_volume(&self, percent: u8) -> Result<(), ServiceError> {
        Ok(self.player.set_volume(percent)?)
    }

    /// Sets the language whose audio track plays when a stream offers it; `None` leaves the
    /// choice to the stream. Applies from the next stream that loads.
    #[tracing::instrument(level = "debug", skip_all, fields(language = ?language.map(|l| l.name)))]
    pub fn set_preferred_audio(&self, language: Option<player_api::Language>) {
        self.player.set_preferred_audio(language);
    }

    /// Sets the subtitle languages whose subtitles turn on by themselves; empty turns that off.
    #[tracing::instrument(level = "debug", skip_all, fields(count = languages.len()))]
    pub fn set_preferred_subtitles(&self, languages: &[player_api::Language]) {
        self.player.set_preferred_subtitles(languages);
    }

    /// Stops the current stream.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn stop(&self) -> Result<(), ServiceError> {
        let status = self.player.status();
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = active.as_mut() {
            if let Err(error) = self.save_progress(current, &status) {
                tracing::warn!(%error, "could not save watch progress on stop");
            }
        }
        *active = None;
        Ok(self.player.stop()?)
    }

    /// The audio and subtitle tracks of the current stream.
    pub fn tracks(&self) -> Vec<player_api::MediaTrack> {
        self.player.tracks()
    }

    /// Selects a track by id, or turns subtitles/audio off with `None`.
    #[tracing::instrument(level = "debug", skip_all, fields(id = ?id), err)]
    pub fn select_track(
        &self,
        kind: player_api::TrackKind,
        id: Option<i64>,
    ) -> Result<(), ServiceError> {
        Ok(self.player.select_track(kind, id)?)
    }

    /// Whether pictures arrive through [`PlaybackService::latest_frame`].
    pub fn is_embedded(&self) -> bool {
        self.player.is_embedded()
    }

    /// Tells an embedded player the size of the picture area in physical pixels.
    pub fn set_surface_size(&self, width: u32, height: u32) {
        self.player.set_surface_size(width, height);
    }

    /// The newest picture, unless its serial is `last_seen_serial`.
    pub fn latest_frame(
        &self,
        last_seen_serial: u64,
    ) -> Option<std::sync::Arc<player_api::VideoFrame>> {
        self.player.latest_frame(last_seen_serial)
    }

    /// Polls the player; call about once a second. Saves progress when due and returns the snapshot.
    pub fn tick(&self) -> PlaybackStatus {
        let status = self.player.status();
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = active.as_mut() {
            if let Err(error) = self.save_if_due(current, &status) {
                tracing::warn!(%error, "could not save watch progress");
            }
        }
        status
    }

    fn save_if_due(
        &self,
        current: &mut ActivePlayback,
        status: &PlaybackStatus,
    ) -> Result<(), ServiceError> {
        let watching = matches!(status.state, PlaybackState::Playing | PlaybackState::Paused);
        let finished = status.state == PlaybackState::Ended;
        let moved = (status.position_ms - current.last_saved_ms).abs() >= SAVE_EVERY_MS;
        if !(finished || (watching && moved)) {
            return Ok(());
        }
        self.save_progress(current, status)
    }

    fn save_progress(
        &self,
        current: &mut ActivePlayback,
        status: &PlaybackStatus,
    ) -> Result<(), ServiceError> {
        let position_ms = if status.state == PlaybackState::Ended {
            status.duration_ms
        } else {
            status.position_ms
        };
        let now_ms = self.clock.now_ms();
        self.progress.save(&WatchProgress {
            provider: current.item.provider.clone(),
            media_key: current.item.media_key.clone(),
            title: current.item.title.clone(),
            detail_url: current.item.detail_url.clone(),
            poster_url: current.item.poster_url.clone(),
            position_ms,
            duration_ms: status.duration_ms,
            updated_at_ms: now_ms,
            season: current.item.season,
            episode: current.item.episode,
        })?;
        current.last_saved_ms = position_ms;
        self.update_resume_pointer(&current.item, status)?;
        Ok(())
    }

    fn update_resume_pointer(
        &self,
        item: &PlaybackItem,
        status: &PlaybackStatus,
    ) -> Result<(), ServiceError> {
        if item.detail_url.is_empty() {
            return Ok(());
        }
        let finished = status.state == PlaybackState::Ended;
        let over_threshold = is_over_watch_threshold(status.position_ms, status.duration_ms);
        let now_ms = self.clock.now_ms();
        if finished || over_threshold {
            if let Some(next) = &item.next_episode {
                self.resume.save(&ResumePointer {
                    provider: item.provider.clone(),
                    detail_url: item.detail_url.clone(),
                    media_key: next.media_key.clone(),
                    title: item.title.clone(),
                    poster_url: item.poster_url.clone(),
                    season: next.season,
                    episode: next.episode,
                    updated_at_ms: now_ms,
                })?;
            } else {
                self.resume.remove(&item.provider, &item.detail_url)?;
            }
        } else {
            self.resume.save(&ResumePointer {
                provider: item.provider.clone(),
                detail_url: item.detail_url.clone(),
                media_key: item.media_key.clone(),
                title: item.title.clone(),
                poster_url: item.poster_url.clone(),
                season: item.season,
                episode: item.episode,
                updated_at_ms: now_ms,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "playback_service_tests.rs"]
mod tests;
