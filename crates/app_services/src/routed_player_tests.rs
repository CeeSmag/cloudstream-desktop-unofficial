use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use player_api::{
    Chapter, MediaTrack, PlaybackState, SubtitleSource, TrackKind, VideoFrame, ENGLISH,
};

use super::*;

/// A GL target that does nothing; only its identity matters to the forwarding tests.
struct NoGl;

impl player_api::GlVideoTarget for NoGl {
    fn attach(&self, _: player_api::GlLoader<'_>) -> Result<(), PlayerError> {
        Ok(())
    }
    fn frame_pending(&self) -> bool {
        false
    }
    fn render(&self, _: u32, _: u32) -> Result<Option<player_api::GlPicture>, PlayerError> {
        Ok(None)
    }
    fn detach(&self) {}
}

/// What happened, in order, across the player and the router.
type Log = Arc<Mutex<Vec<String>>>;

fn note(log: &Log, entry: &str) {
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry.to_owned());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// A player that answers every question with something other than the trait's default, so that a
/// method the wrapper forgets to forward shows.
struct ScriptedPlayer {
    log: Log,
    fail_to_play: AtomicBool,
}

impl VideoPlayer for ScriptedPlayer {
    fn play(&self, request: &PlaybackRequest) -> Result<(), PlayerError> {
        note(&self.log, &format!("play {}", request.url));
        if self.fail_to_play.load(Ordering::SeqCst) {
            return Err(PlayerError::Unavailable("cannot play".to_owned()));
        }
        Ok(())
    }
    fn set_paused(&self, paused: bool) -> Result<(), PlayerError> {
        note(&self.log, &format!("set_paused {paused}"));
        Ok(())
    }
    fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        note(&self.log, &format!("seek_to {position_ms}"));
        Ok(())
    }
    fn stop(&self) -> Result<(), PlayerError> {
        note(&self.log, "player stop");
        Ok(())
    }
    fn set_volume(&self, percent: u8) -> Result<(), PlayerError> {
        note(&self.log, &format!("set_volume {percent}"));
        Ok(())
    }
    fn status(&self) -> PlaybackStatus {
        PlaybackStatus {
            state: PlaybackState::Playing,
            position_ms: 1234,
            ..PlaybackStatus::default()
        }
    }
    fn tracks(&self) -> Vec<MediaTrack> {
        vec![MediaTrack {
            id: 7,
            kind: TrackKind::Audio,
            language: "eng".to_owned(),
            title: String::new(),
            selected: true,
            external: false,
        }]
    }
    fn select_track(&self, kind: TrackKind, id: Option<i64>) -> Result<(), PlayerError> {
        note(&self.log, &format!("select_track {kind:?} {id:?}"));
        Ok(())
    }
    fn add_subtitles(&self, subtitles: &[SubtitleSource]) {
        note(&self.log, &format!("add_subtitles {}", subtitles.len()));
    }
    fn pending_subtitles(&self) -> usize {
        3
    }
    fn set_preferred_audio(&self, language: Option<Language>) {
        note(
            &self.log,
            &format!("set_preferred_audio {}", language.is_some()),
        );
    }
    fn set_verbose_log(&self, enabled: bool) {
        note(&self.log, &format!("set_verbose_log {enabled}"));
    }
    fn set_preferred_subtitles(&self, languages: &[Language]) {
        note(
            &self.log,
            &format!("set_preferred_subtitles {}", languages.len()),
        );
    }
    fn set_cache_limits(&self, limits: CacheLimits) -> Result<(), PlayerError> {
        note(
            &self.log,
            &format!(
                "set_cache_limits {} {}",
                limits.ahead_bytes, limits.behind_bytes
            ),
        );
        Ok(())
    }
    fn chapters(&self) -> Vec<Chapter> {
        vec![Chapter {
            title: "Opening".to_owned(),
            time_ms: 0,
        }]
    }
    fn is_embedded(&self) -> bool {
        true
    }
    fn set_surface_size(&self, width: u32, height: u32) {
        note(&self.log, &format!("set_surface_size {width}x{height}"));
    }
    fn gl_target(&self) -> Option<Arc<dyn player_api::GlVideoTarget>> {
        Some(Arc::new(NoGl))
    }
    fn latest_frame(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        Some(Arc::new(VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
            serial: last_seen_serial + 1,
        }))
    }
}

/// A router that sends every stream to `proxied:<url>` and notes what it is asked.
struct RecordingRouter {
    log: Log,
}

impl RequestRouter for RecordingRouter {
    fn route(&self, request: &PlaybackRequest) -> PlaybackRequest {
        note(&self.log, &format!("route {}", request.url));
        PlaybackRequest {
            url: format!("proxied:{}", request.url),
            headers: Vec::new(),
            ..request.clone()
        }
    }
    fn release(&self) {
        note(&self.log, "router release");
    }
}

fn routed(fail_to_play: bool) -> (RoutedPlayer, Log) {
    let log = Log::default();
    let player = ScriptedPlayer {
        log: Arc::clone(&log),
        fail_to_play: AtomicBool::new(fail_to_play),
    };
    let router = RecordingRouter {
        log: Arc::clone(&log),
    };
    (RoutedPlayer::new(Arc::new(player), Arc::new(router)), log)
}

fn request() -> PlaybackRequest {
    PlaybackRequest {
        url: "https://h.test/a.m3u8".to_owned(),
        title: "Episode".to_owned(),
        headers: vec![("X-Token".to_owned(), "t".to_owned())],
        referer: "https://site.test/".to_owned(),
        start_position_ms: 4_000,
        clear_key: Some("00".repeat(16)),
        ..PlaybackRequest::default()
    }
}

#[test]
fn play_gives_the_player_the_routed_request_and_nothing_else_changes() {
    let (player, log) = routed(false);
    player.play(&request()).expect("plays");
    assert_eq!(
        entries(&log),
        [
            "route https://h.test/a.m3u8",
            "play proxied:https://h.test/a.m3u8"
        ]
    );
}

#[test]
fn play_that_fails_lets_the_router_go_and_returns_the_players_error() {
    let (player, log) = routed(true);
    let result = player.play(&request());
    assert!(matches!(result, Err(PlayerError::Unavailable(_))));
    assert_eq!(
        entries(&log).last().map(String::as_str),
        Some("router release")
    );
}

#[test]
fn stop_stops_the_player_first_and_only_then_lets_the_router_go() {
    let (player, log) = routed(false);
    player.stop().expect("stops");
    assert_eq!(entries(&log), ["player stop", "router release"]);
}

#[test]
fn the_gl_target_is_forwarded_not_left_at_its_default() {
    let (player, _log) = routed(false);
    assert!(player.gl_target().is_some());
}

#[test]
fn every_other_method_reaches_the_player_and_its_answer_comes_back() {
    let (player, log) = routed(false);
    player.set_paused(true).expect("paused");
    player.seek_to(5_000).expect("seeks");
    player.set_volume(80).expect("volume");
    player
        .select_track(TrackKind::Subtitle, Some(3))
        .expect("selects");
    player.add_subtitles(&[SubtitleSource::default()]);
    player.set_preferred_audio(Some(ENGLISH));
    player.set_verbose_log(true);
    player.set_preferred_subtitles(&[ENGLISH, ENGLISH]);
    player.set_surface_size(640, 360);
    player
        .set_cache_limits(CacheLimits {
            ahead_bytes: 3,
            behind_bytes: 1,
        })
        .expect("limits");
    assert_eq!(
        entries(&log),
        [
            "set_paused true",
            "seek_to 5000",
            "set_volume 80",
            "select_track Subtitle Some(3)",
            "add_subtitles 1",
            "set_preferred_audio true",
            "set_verbose_log true",
            "set_preferred_subtitles 2",
            "set_surface_size 640x360",
            "set_cache_limits 3 1",
        ]
    );
    assert_eq!(player.status().position_ms, 1234);
    assert_eq!(player.tracks().len(), 1);
    assert_eq!(player.chapters().len(), 1, "chapters are not lost");
    assert_eq!(player.pending_subtitles(), 3);
    assert!(player.is_embedded());
    assert_eq!(player.latest_frame(9).map(|frame| frame.serial), Some(10));
}
