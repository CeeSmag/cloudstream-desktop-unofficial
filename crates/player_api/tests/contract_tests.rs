//! Contract tests covering default `VideoPlayer` trait methods and `player_api` types.

use player_api::{
    all_languages, describe_languages, find_language, CacheLimits, PlaybackRequest, PlaybackState,
    PlaybackStatus, PlayerError, StreamKind, SubtitleSource, TrackKind, VideoPlayer, ENGLISH,
};

struct MinimalPlayer;

impl VideoPlayer for MinimalPlayer {
    fn play(&self, _request: &PlaybackRequest) -> Result<(), PlayerError> {
        Ok(())
    }

    fn set_paused(&self, _paused: bool) -> Result<(), PlayerError> {
        Ok(())
    }

    fn seek_to(&self, _position_ms: i64) -> Result<(), PlayerError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlayerError> {
        Ok(())
    }

    fn status(&self) -> PlaybackStatus {
        PlaybackStatus::default()
    }
}

#[test]
fn default_video_player_methods_return_safe_noops() {
    let player = MinimalPlayer;

    assert!(player.set_volume(75).is_ok());
    assert!(player
        .set_cache_limits(CacheLimits {
            ahead_bytes: 192 * 1024 * 1024,
            behind_bytes: 64 * 1024 * 1024,
        })
        .is_ok());
    assert!(player.tracks().is_empty());
    assert!(player.select_track(TrackKind::Audio, Some(1)).is_ok());
    assert!(player.select_track(TrackKind::Subtitle, None).is_ok());
    player.add_subtitles(&[SubtitleSource {
        url: "/tmp/sub.vtt".into(),
        language: "en".into(),
    }]);
    assert_eq!(player.pending_subtitles(), 0);
    player.set_preferred_audio(Some(ENGLISH));
    player.set_preferred_audio(None);
    player.set_verbose_log(true);
    player.set_verbose_log(false);
    player.set_preferred_subtitles(&[ENGLISH]);
    assert!(player.chapters().is_empty());
    assert!(!player.is_embedded());
    player.set_surface_size(1280, 720);
    assert!(player.latest_frame(0).is_none());
}

#[test]
fn language_lookup_and_description_cover_codes_and_names() {
    assert!(!all_languages().is_empty());
    assert_eq!(find_language("en"), Some(ENGLISH));
    assert_eq!(find_language("eng"), Some(ENGLISH));
    assert_eq!(find_language("English"), Some(ENGLISH));
    assert_eq!(find_language("  EN  "), Some(ENGLISH));
    assert_eq!(find_language("not-a-language"), None);

    let described = describe_languages(&[SubtitleSource {
        url: "/tmp/en.vtt".into(),
        language: "en".into(),
    }]);
    assert!(!described.is_empty());
    assert_eq!(describe_languages(&[]), "");
}

#[test]
fn playback_status_cache_limits_and_errors_work() {
    let status = PlaybackStatus::default();
    assert_eq!(status.state, PlaybackState::Idle);
    assert_eq!(status.position_ms, 0);
    assert_eq!(status.duration_ms, 0);

    let req = PlaybackRequest {
        kind: StreamKind::Hls,
        url: "https://example.invalid/master.m3u8".into(),
        title: "Demo".into(),
        ..PlaybackRequest::default()
    };
    assert_eq!(req.kind, StreamKind::Hls);

    let limits = CacheLimits {
        ahead_bytes: 192 * 1024 * 1024,
        behind_bytes: 64 * 1024 * 1024,
    };
    assert!(limits.ahead_bytes > limits.behind_bytes);

    assert!(PlayerError::Unavailable("missing libmpv".into())
        .to_string()
        .contains("missing libmpv"));
    assert!(PlayerError::Command {
        command: "seek".into(),
        reason: "invalid".into(),
    }
    .to_string()
    .contains("seek"));
}
