use std::sync::{Arc, Mutex, PoisonError};

use core_model::{PlayableLink, SubtitleTrack, WatchProgress};
use player_api::{PlaybackRequest, PlaybackState, PlaybackStatus, SubtitleSource, VideoPlayer};
use storage::WatchProgressStore;

use crate::{Clock, ServiceError};

/// Progress is saved when the position moved at least this far since the last save.
const SAVE_EVERY_MS: i64 = 5_000;

/// A stream to play, with the subtitles to offer and where to begin.
#[derive(Debug, Clone, Copy)]
pub struct StreamSelection<'a> {
    /// The stream.
    pub link: &'a PlayableLink,
    /// External subtitles the provider offered.
    pub subtitles: &'a [SubtitleTrack],
    /// Where to begin, in milliseconds.
    pub start_position_ms: i64,
}

/// What is being watched; carried into saved progress.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackItem {
    /// Provider the item belongs to.
    pub provider: String,
    /// Payload identifying the item (movie data or episode data).
    pub media_key: String,
    /// Title shown in "continue watching".
    pub title: String,
    /// Provider URL of the parent title.
    pub detail_url: String,
    /// Poster image URL.
    pub poster_url: Option<String>,
}

struct ActivePlayback {
    item: PlaybackItem,
    last_saved_ms: i64,
}

/// Starts playback, resumes where the user left off and keeps their progress.
pub struct PlaybackService {
    player: Arc<dyn VideoPlayer>,
    progress: WatchProgressStore,
    clock: Arc<dyn Clock>,
    active: Mutex<Option<ActivePlayback>>,
}

impl PlaybackService {
    /// Creates the service.
    pub fn new(
        player: Arc<dyn VideoPlayer>,
        progress: WatchProgressStore,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            player,
            progress,
            clock,
            active: Mutex::new(None),
        }
    }

    /// Where playback of `item` should resume, in milliseconds (0 when there is nothing to resume).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn resume_position_ms(&self, item: &PlaybackItem) -> Result<i64, ServiceError> {
        let saved = self.progress.get(&item.provider, &item.media_key)?;
        Ok(saved
            .filter(|entry| entry.fraction_watched() < 0.95)
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

    /// Stops the current stream.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn stop(&self) -> Result<(), ServiceError> {
        *self.active.lock().unwrap_or_else(PoisonError::into_inner) = None;
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
        let position_ms = if finished {
            status.duration_ms
        } else {
            status.position_ms
        };
        self.progress.save(&WatchProgress {
            provider: current.item.provider.clone(),
            media_key: current.item.media_key.clone(),
            title: current.item.title.clone(),
            detail_url: current.item.detail_url.clone(),
            poster_url: current.item.poster_url.clone(),
            position_ms,
            duration_ms: status.duration_ms,
            updated_at_ms: self.clock.now_ms(),
        })?;
        current.last_saved_ms = position_ms;
        Ok(())
    }
}

fn build_request(
    title: &str,
    link: &PlayableLink,
    subtitles: &[SubtitleTrack],
    start_ms: i64,
) -> PlaybackRequest {
    let mut headers: Vec<(String, String)> = link
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    headers.sort();
    PlaybackRequest {
        url: link.url.clone(),
        title: title.to_owned(),
        headers,
        referer: link.referer.clone(),
        subtitles: subtitles
            .iter()
            .map(|track| SubtitleSource {
                url: track.url.clone(),
                language: track.lang.clone(),
            })
            .collect(),
        start_position_ms: start_ms,
    }
}

#[cfg(test)]
mod tests {
    use storage::Database;

    use super::*;
    use crate::test_support::{FakePlayer, FixedClock};

    fn setup() -> (
        PlaybackService,
        Arc<FakePlayer>,
        WatchProgressStore,
        Arc<FixedClock>,
    ) {
        let player = Arc::new(FakePlayer::default());
        let progress =
            WatchProgressStore::new(Arc::new(Database::open_in_memory().expect("memory db")));
        let clock = Arc::new(FixedClock::at(1_000));
        let service = PlaybackService::new(
            Arc::clone(&player) as Arc<dyn VideoPlayer>,
            progress.clone(),
            Arc::clone(&clock) as Arc<dyn Clock>,
        );
        (service, player, progress, clock)
    }

    fn item() -> PlaybackItem {
        PlaybackItem {
            provider: "P".into(),
            media_key: "ep1".into(),
            title: "Episode 1".into(),
            ..PlaybackItem::default()
        }
    }

    fn link() -> PlayableLink {
        PlayableLink {
            url: "https://cdn.test/v.mp4".into(),
            referer: "https://site.test".into(),
            ..PlayableLink::default()
        }
    }

    fn status(state: PlaybackState, position_ms: i64) -> PlaybackStatus {
        PlaybackStatus {
            state,
            position_ms,
            duration_ms: 100_000,
            has_video: true,
        }
    }

    #[test]
    fn start_sends_url_referer_and_subtitles_to_the_player() {
        let (service, player, _, _) = setup();
        let subtitle = SubtitleTrack {
            lang: "English".into(),
            url: "https://s.test/en.vtt".into(),
            headers: None,
        };
        service.start(item(), &link(), &[subtitle]).expect("starts");
        let request = player.requests.lock().expect("lock")[0].clone();
        assert_eq!(request.url, "https://cdn.test/v.mp4");
        assert_eq!(request.referer, "https://site.test");
        assert_eq!(request.subtitles[0].language, "English");
    }

    #[test]
    fn progress_is_saved_only_after_it_moved_enough() {
        let (service, player, progress, clock) = setup();
        service.start(item(), &link(), &[]).expect("starts");
        player.report(status(PlaybackState::Playing, 2_000));
        service.tick();
        assert!(
            progress.get("P", "ep1").expect("get").is_none(),
            "2s is below the save threshold"
        );
        clock.set(7_000);
        player.report(status(PlaybackState::Playing, 6_000));
        service.tick();
        assert_eq!(
            progress
                .get("P", "ep1")
                .expect("get")
                .expect("saved")
                .position_ms,
            6_000
        );
    }

    #[test]
    fn start_at_uses_the_given_position_instead_of_the_saved_one() {
        let (service, player, _, _) = setup();
        let stream = StreamSelection {
            link: &link(),
            subtitles: &[],
            start_position_ms: 77_000,
        };
        service.start_at(item(), &stream).expect("starts");
        assert_eq!(
            player.requests.lock().expect("lock")[0].start_position_ms,
            77_000
        );
    }

    #[test]
    fn restarting_resumes_from_the_saved_position() {
        let (service, player, _, _) = setup();
        service.start(item(), &link(), &[]).expect("starts");
        player.report(status(PlaybackState::Playing, 42_000));
        service.tick();
        service.start(item(), &link(), &[]).expect("starts again");
        assert_eq!(
            player.requests.lock().expect("lock")[1].start_position_ms,
            42_000
        );
    }

    #[test]
    fn finished_items_restart_from_the_beginning() {
        let (service, player, _, _) = setup();
        service.start(item(), &link(), &[]).expect("starts");
        player.report(status(PlaybackState::Ended, 100_000));
        service.tick();
        assert_eq!(service.resume_position_ms(&item()).expect("resume"), 0);
    }
}
