//! T4: drive the real libmpv headlessly with a generated test stream.
//! Run with `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv -- --ignored`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::time::{Duration, Instant};

use player_api::{
    MediaTrack, PlaybackRequest, PlaybackState, SubtitleSource, TrackKind, VideoPlayer,
};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

/// A 2-second synthetic video produced by ffmpeg's lavfi inside mpv; needs no file or network.
const TEST_STREAM: &str = "av://lavfi:testsrc=duration=2:size=64x64:rate=10";

fn headless_player() -> MpvPlayer {
    let library = locate_libmpv().expect("libmpv located");
    MpvPlayer::new(
        &library,
        &PlayerOptions {
            headless: true,
            ..PlayerOptions::default()
        },
    )
    .expect("player starts")
}

fn wait_for(player: &MpvPlayer, accept: impl Fn(&PlaybackState) -> bool, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if accept(&player.status().state) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn plays_a_generated_stream_to_the_end() {
    let player = headless_player();
    let request = PlaybackRequest {
        url: TEST_STREAM.to_owned(),
        title: "test".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    assert!(
        wait_for(
            &player,
            |s| *s == PlaybackState::Playing,
            Duration::from_secs(10)
        ),
        "never started"
    );
    assert!(
        wait_for(
            &player,
            |s| *s == PlaybackState::Ended,
            Duration::from_secs(15)
        ),
        "never ended"
    );
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn unplayable_url_reports_failed_state() {
    let player = headless_player();
    let request = PlaybackRequest {
        url: "http://127.0.0.1:9/nothing.mp4".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    assert!(
        wait_for(
            &player,
            |s| matches!(s, PlaybackState::Failed(_)),
            Duration::from_secs(20)
        ),
        "state was {:?}",
        player.status().state
    );
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn stop_returns_the_player_to_idle() {
    let player = headless_player();
    let request = PlaybackRequest {
        url: TEST_STREAM.to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    assert!(wait_for(
        &player,
        |s| *s == PlaybackState::Playing,
        Duration::from_secs(10)
    ));
    player.stop().expect("stop accepted");
    assert!(wait_for(
        &player,
        |s| *s == PlaybackState::Idle,
        Duration::from_secs(5)
    ));
}

#[test]
fn missing_library_is_reported_as_unavailable_not_a_panic() {
    let error = MpvPlayer::new(
        std::path::Path::new("definitely/not/libmpv.dll"),
        &PlayerOptions::default(),
    )
    .err()
    .expect("must fail");
    assert!(matches!(error, player_api::PlayerError::Unavailable(_)));
}

#[test]
#[ignore = "needs libmpv and network; set CS_TEST_STREAM_URL"]
fn plays_the_stream_named_by_the_environment() {
    let url = std::env::var("CS_TEST_STREAM_URL").expect("CS_TEST_STREAM_URL must be set");
    let player = headless_player();
    let request = PlaybackRequest {
        url,
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    let started = wait_for(
        &player,
        |s| *s == PlaybackState::Playing,
        Duration::from_secs(60),
    );
    assert!(started, "state was {:?}", player.status().state);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn embedded_player_delivers_rgba_frames_of_the_requested_size() {
    let library = locate_libmpv().expect("libmpv located");
    let options = PlayerOptions {
        embedded: true,
        ..PlayerOptions::default()
    };
    let player = MpvPlayer::new(&library, &options).expect("player starts");
    assert!(player.is_embedded(), "render API must be available");
    player.set_surface_size(640, 360);
    let request = PlaybackRequest {
        url: "av://lavfi:testsrc=duration=3:size=320x180:rate=15".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut frame = None;
    while Instant::now() < deadline && frame.is_none() {
        frame = player.latest_frame(0);
        std::thread::sleep(Duration::from_millis(50));
    }
    let frame = frame.expect("a frame arrived");
    assert_eq!((frame.width, frame.height), (640, 360));
    assert_eq!(frame.rgba.len(), 640 * 360 * 4);
    assert!(
        frame
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255),
        "alpha must be opaque"
    );
    assert!(
        frame.rgba.iter().any(|byte| *byte > 40),
        "picture must not be black"
    );
}

fn wait_for_tracks(
    player: &MpvPlayer,
    kind: TrackKind,
    accept: impl Fn(&[MediaTrack]) -> bool,
) -> Vec<MediaTrack> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let tracks: Vec<MediaTrack> = player
            .tracks()
            .into_iter()
            .filter(|t| t.kind == kind)
            .collect();
        if accept(&tracks) || Instant::now() > deadline {
            return tracks;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Writes a one-line subtitle file and returns its path.
fn temp_subtitle_file() -> std::path::PathBuf {
    let srt = std::env::temp_dir().join(format!("cs_subtitle_test_{}.srt", std::process::id()));
    std::fs::write(
        &srt,
        "1
00:00:00,000 --> 00:00:06,000
Hello subtitles

",
    )
    .expect("srt written");
    srt
}

fn subtitle_request(srt: &std::path::Path) -> PlaybackRequest {
    PlaybackRequest {
        url: "av://lavfi:testsrc=duration=10:size=160x90:rate=10".to_owned(),
        subtitles: vec![SubtitleSource {
            url: srt.to_string_lossy().into_owned(),
            language: "English".to_owned(),
        }],
        ..PlaybackRequest::default()
    }
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn a_provider_subtitle_becomes_a_track_that_can_be_switched_on_and_off() {
    let srt = temp_subtitle_file();
    let player = headless_player();
    player
        .play(&subtitle_request(&srt))
        .expect("loadfile accepted");

    let tracks = wait_for_tracks(&player, TrackKind::Subtitle, |t| {
        t.iter().any(|x| x.external)
    });
    let subtitle = tracks
        .iter()
        .find(|t| t.external)
        .expect("subtitle track listed");
    // `sub-add ... auto` only selects a subtitle matching the preferred language, so it starts off.
    assert!(
        !subtitle.selected,
        "a provider subtitle starts off until the user turns it on"
    );

    player
        .select_track(TrackKind::Subtitle, Some(subtitle.id))
        .expect("subtitles on");
    assert!(wait_for_tracks(&player, TrackKind::Subtitle, |t| t
        .iter()
        .any(|x| x.selected))
    .iter()
    .any(|x| x.id == subtitle.id));

    player
        .select_track(TrackKind::Subtitle, None)
        .expect("subtitles off");
    assert!(wait_for_tracks(&player, TrackKind::Subtitle, |t| t
        .iter()
        .all(|x| !x.selected))
    .iter()
    .all(|x| !x.selected));
    let _ = std::fs::remove_file(srt);
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn audio_track_can_be_muted_and_restored() {
    let player = headless_player();
    let request = PlaybackRequest {
        url: "av://lavfi:sine=frequency=440:duration=10".to_owned(),
        ..PlaybackRequest::default()
    };
    player.play(&request).expect("loadfile accepted");
    let tracks = wait_for_tracks(&player, TrackKind::Audio, |t| !t.is_empty());
    let audio = tracks.first().expect("an audio track is listed");
    assert!(audio.selected);

    player
        .select_track(TrackKind::Audio, None)
        .expect("audio off");
    assert!(
        wait_for_tracks(&player, TrackKind::Audio, |t| t.iter().all(|x| !x.selected))
            .iter()
            .all(|x| !x.selected)
    );
    player
        .select_track(TrackKind::Audio, Some(audio.id))
        .expect("audio on");
    assert!(
        wait_for_tracks(&player, TrackKind::Audio, |t| t.iter().any(|x| x.selected))
            .iter()
            .any(|x| x.selected)
    );
}
